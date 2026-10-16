// Production profile model/controls and PDF proof session/controller. The
// deferred worker and DOM are explicit boundaries. Header-prefixed payloads
// exercise admission only; they are NOT typeset or independently valid PDFs.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookPdfProof } from "../book_pdf_proof.mjs";
import { createBookPdfControls } from "../demo/book_pdf_controls.mjs";
import { setup } from "./book_publication_archival_helpers.mjs";
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
const policy = strict => ({ pdfA: "2b", pdfAStrict: strict });
function deferredWorker() {
  const state = { jobs: [], cancels: 0, disposed: 0, idle: false };
  const worker = {
    get hasRetainedBook() { return state.idle; },
    render(files, format, options, { signal }) {
      state.idle = false;
      return new Promise((resolve, reject) => state.jobs.push({ files, format, options, signal,
        resolve(value) { state.idle = true; resolve(value); }, reject }));
    },
    cancel() { state.cancels++; state.idle = false; },
    dispose() { state.disposed++; state.idle = false; },
  };
  return { worker, state };
}
function receipt(job, emission = {}) {
  const sourceLength = [...job.files, ...job.options.includeSources]
    .reduce((n, f) => n + new TextEncoder().encode(f.source).length, 0);
  return { format: "book-pdf", mimeType: "application/pdf", extension: "pdf", sourceLength,
    bytes: new TextEncoder().encode("%PDF-1.7\nexplicit admission fixture\n"), ...emission };
}
function fixture(t, emission = policy(true)) {
  const host = setup(t, emission), endpoint = deferredWorker();
  const proof = createBookPdfProof({ collection: host.book, controls: host.controls, worker: endpoint.worker });
  t.after(() => proof.dispose());
  const render = async (emission = {}) => {
    const pending = proof.render(), job = endpoint.state.jobs.at(-1);
    job.resolve(receipt(job, emission)); return pending;
  };
  return { ...host, ...endpoint, proof, render };
}

for (const strict of [false, true]) test(`admitted archival proof retains exact policy and bytes: strict=${strict}`, async t => {
  const f = fixture(t, policy(strict)); const pending = f.proof.render(), job = f.state.jobs.at(-1);
  assert.equal(job.options.pdfA, "2b"); assert.equal(job.options.pdfAStrict, strict);
  const output = receipt(job, policy(strict)), wanted = output.bytes.slice();
  const larger = new Uint8Array(wanted.length + 12); larger.set(wanted, 5);
  output.bytes = larger.subarray(5, 5 + wanted.length);
  output.blob = () => { throw Error("do not trust host closures"); };
  job.resolve(output); const proof = await pending; larger.fill(0);
  assert.equal(proof.pdfA, "2b"); assert.equal(proof.pdfAStrict, strict); assert(Object.isFrozen(proof));
  assert.deepEqual(new Uint8Array(await proof.blob.arrayBuffer()), wanted);
  assert.equal(proof.size, wanted.length); assert.equal(f.proof.current, proof);
  assert.equal(await f.proof.render(), proof); assert.equal(f.state.jobs.length, 1);
  f.book.setPublication(policy(strict), f.book.revision); assert.equal(f.proof.current, proof);
});

for (const emission of [{}, { pdfA: "off", pdfAStrict: true }, { pdfA: "2b" },
  { pdfA: "2b", pdfAStrict: false }, { pdfA: "2b", pdfAStrict: "true" }, { pdfA: "3b", pdfAStrict: true }]) {
  test(`mismatched archival proof never becomes downloadable or keeps its idle worker: ${JSON.stringify(emission)}`, async t => {
    const f = fixture(t), before = f.book.project();
    await assert.rejects(f.render(emission), { code: "UNSUPPORTED_BOOK_PDFA" });
    assert.equal(f.proof.current, null); assert.equal(f.proof.pending, false); assert.equal(f.state.idle, false);
    assert.equal(f.state.cancels, 1); assert.equal(f.state.jobs.length, 1); assert.deepEqual(f.book.project(), before);
    assert.equal((await f.render(policy(true))).pdfAStrict, true);
  });
}

for (const emission of [policy(false), { pdfAStrict: false }, { pdfA: "off" }]) {
  test(`ordinary proof refuses unrequested policy fields: ${JSON.stringify(emission)}`, async t => {
    const f = fixture(t, {});
    await assert.rejects(f.render(emission), { code: "INVALID_BOOK_PDF" });
    assert.equal(f.proof.current, null); assert.equal(f.state.idle, false);
    const ordinary = await f.render();
    assert.equal(Object.hasOwn(ordinary, "pdfA"), false); assert.equal(Object.hasOwn(ordinary, "pdfAStrict"), false);
  });
}

test("worker-side option mutation cannot downgrade a previously captured strict policy", async t => {
  const f = fixture(t), pending = f.proof.render(), job = f.state.jobs.at(-1);
  job.options.pdfA = "off"; job.options.pdfAStrict = false;
  job.resolve(receipt(job));
  await assert.rejects(pending, { code: "UNSUPPORTED_BOOK_PDFA" });
  assert.equal(f.book.options.pdfAStrict, true); assert.equal(f.proof.current, null);
});

test("policy changes cancel pending proofs; late archival output cannot retire a newer ordinary proof", async t => {
  const f = fixture(t), pending = f.proof.render(), old = f.state.jobs.at(-1);
  f.book.setPublication({}, f.book.revision);
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(old.signal.aborted, true);
  const ordinary = await f.render(); old.resolve(receipt(old, policy(true))); await tick();
  assert.equal(f.proof.current, ordinary); assert.equal(ordinary.pdfA, undefined);
  assert.equal(f.state.jobs.length, 2);
});

test("receipt inspection cannot retain a proof after reentrant source invalidation", async t => {
  const f = fixture(t), pending = f.proof.render(), job = f.state.jobs.at(-1);
  const output = receipt(job, policy(true));
  Object.defineProperty(output, "pdfAStrict", { get() {
    f.book.edit(0, "guide/one.md", "Changed while reading receipt"); return true;
  } });
  job.resolve(output); await assert.rejects(pending, { code: "STALE_SOURCE" });
  assert.equal(f.proof.current, null);
});

test("native refusal does not publish or change the saved policy and never retries automatically", async t => {
  const f = fixture(t), pending = f.proof.render(), job = f.state.jobs.at(-1);
  job.reject(Object.assign(Error("native strict writer refused"), { code: "STRICT_ARCHIVAL_REFUSAL" }));
  await assert.rejects(pending, { code: "STRICT_ARCHIVAL_REFUSAL" }); await tick();
  assert.equal(f.proof.current, null); assert.equal(f.state.jobs.length, 1); assert.equal(f.book.options.pdfAStrict, true);
  assert.equal((await f.render(policy(true))).pdfAStrict, true);
});

function view(t, emission = policy(true)) {
  const host = setup(t, emission), endpoint = deferredWorker(), issued = [], revoked = [];
  const ui = createBookPdfControls({ root: host.root, controls: host.controls, collection: host.book,
    worker: endpoint.worker, navigator: { pdfViewerEnabled: false },
    urls: { createObjectURL(blob) { const url = `blob:proof-${issued.length}`; issued.push({ url, blob }); return url; },
      revokeObjectURL(url) { revoked.push(url); } } });
  t.after(() => ui.dispose());
  const el = id => host.root.querySelector(`#book-pdf-${id}`);
  const build = async emission => { const pending = ui.build(), job = endpoint.state.jobs.at(-1);
    job.resolve(receipt(job, emission)); return pending; };
  return { ...host, ...endpoint, proofUi: ui, proofEl: el, issued, revoked, build };
}

for (const strict of [false, true]) test(`viewer and exact-byte download describe only admitted policy: strict=${strict}`, async t => {
  const h = view(t, policy(strict)); const proof = await h.build(policy(strict));
  assert.equal(h.proofEl("download").hidden, false); assert.equal(h.issued.length, 1);
  assert.equal(h.issued[0].blob, proof.blob);
  assert.match(h.proofEl("status").textContent, /Writer receipt: PDF\/A-2b/);
  assert.equal(h.proofEl("status").textContent.includes("with strict policy"), strict);
  assert.match(h.proofEl("status").textContent, /Not independent conformance validation/);
  h.input("pdfPolicy", "off"); // Unapplied draft is not authority to relabel or discard valid output.
  assert.equal(h.proofEl("download").hidden, false); assert.equal(h.revoked.length, 0);
  assert.equal(await h.proofUi.build(), proof); assert.equal(h.state.jobs.length, 1);
  assert.match(h.proofEl("status").textContent, /reused without rendering/);
  assert.match(h.proofEl("status").textContent, /Writer receipt: PDF\/A-2b/);
  h.ui.apply(); assert.equal(h.proofEl("download").hidden, true); assert.deepEqual(h.revoked, [h.issued[0].url]);
  const ordinary = await h.build(); assert.equal(ordinary.pdfA, undefined);
  assert(!h.proofEl("status").textContent.includes("Writer receipt")); assert.equal(h.proofEl("download").hidden, false);
});

test("mismatched proof receipts create no viewer/download URL and preserve authoring state", async t => {
  const h = view(t), before = h.book.project();
  await assert.rejects(h.build(), { code: "UNSUPPORTED_BOOK_PDFA" });
  assert.equal(h.issued.length, 0); assert.equal(h.proofEl("download").hidden, true);
  assert.match(h.proofEl("status").textContent, /UNSUPPORTED_BOOK_PDFA/);
  assert.deepEqual(h.book.project(), before); assert.equal(h.state.jobs.length, 1);
});

test("restored archival project is proofed with restored settings but no inherited resource grant", async t => {
  const h = view(t, {}), project = h.book.project();
  project.schemaVersion = 5; project.options = { ...project.options, ...policy(true) };
  h.book.replaceProject(project); const proof = await h.build(policy(true)), job = h.state.jobs.at(-1);
  assert.equal(proof.pdfAStrict, true); assert.equal(job.options.pdfAStrict, true);
  assert.deepEqual(job.options.images, []); assert.deepEqual(job.options.fontAssets, []);
  assert.deepEqual(h.book.images, []); assert.deepEqual(h.book.fonts, []);
});

test("a silent editor checkpoint change still blocks activation of an archival download", async t => {
  const h = view(t); await h.build(policy(true)); h.editCheckpoint();
  assert.equal(h.proofEl("download").emit("click"), false);
  assert.equal(h.proofEl("download").hidden, true); assert.equal(h.revoked.length, 1);
  assert.equal(h.state.jobs.length, 1);
});

test("suspension revokes the archival URL and blocks proofing until resumed", async t => {
  const h = view(t); await h.build(policy(true)); h.proofUi.suspend();
  assert.equal(h.proofEl("download").hidden, true); assert.equal(h.revoked.length, 1);
  await assert.rejects(h.proofUi.build(), { code: "BOOK_BUSY" });
  h.proofUi.resume(); await h.build(policy(true)); assert.equal(h.issued.length, 2);
  assert.match(h.proofEl("status").textContent, /with strict policy/);
});
