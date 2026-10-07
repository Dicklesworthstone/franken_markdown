import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { createBookPdfProof } from "../book_pdf_proof.mjs";
import { pdfResult, proofHost } from "./book_pdf_proof_helpers.mjs";
import { workbench } from "./book_publishing_retention_helpers.mjs";
const code = expected => error => error?.code === expected;
const tick = () => new Promise(resolve => setImmediate(resolve));
const decodeProof = async proof => JSON.parse((await proof.blob.text()).replace(/^%PDF-1.7\n/, ""));

test("publisher reuses the native book across PDF/EPUB/site while replacing stale download URLs", async t => {
  const f = await workbench(t);
  let previous;
  for (const format of ["pdf", "epub", "site"]) {
    await f.controls.prepare(format);
    assert.equal((await f.published()).format, format);
    assert.equal(f.publishing.client.hasRetainedBook, true);
    assert.equal(f.el("download").hidden, false);
    assert.equal(f.el("cancel-export").disabled, false, "idle retained resources remain explicitly releasable");
    if (previous) assert(f.revoked.includes(previous));
    previous = f.el("download").href;
  }
  assert.equal(f.publishing.endpoints.length, 1);
  assert.equal(f.publishing.endpoints[0].native.stats.creates, 1);
  assert.equal(f.publishing.endpoints[0].native.stats.updates, 0);
});

test("source edits clear visible outputs immediately but reparse through the retained source transaction", async t => {
  const f = await workbench(t);
  await f.controls.prepare("pdf"); await f.pdf.build();
  const oldDownload = f.el("download").href, oldProof = f.el("book-pdf-download").href;
  f.edit("# Revised 😀");
  assert(f.revoked.includes(oldDownload)); assert(f.revoked.includes(oldProof));
  assert.equal(f.el("download").hidden, true); assert.equal(f.el("book-pdf-download").hidden, true);
  assert.equal(f.publishing.client.hasRetainedBook, true);
  assert.equal(f.proofing.client.hasRetainedBook, true);
  await f.controls.prepare("epub");
  const proof = await f.pdf.build();
  assert.equal((await f.published()).files[0].source, "# Revised 😀");
  assert.equal((await decodeProof(proof)).files[0].source, "# Revised 😀");
  for (const group of [f.publishing, f.proofing]) {
    assert.equal(group.endpoints.length, 1); assert.equal(group.endpoints[0].native.stats.creates, 1);
    assert.equal(group.endpoints[0].native.stats.updates, 1);
  }
});

test("include-only editing and batched text replacement reach the retained exporter", async t => {
  const f = await workbench(t); await f.controls.prepare("site");
  f.el("chapters").value = "2"; f.el("chapters").fire("change");
  f.edit("Current shared content");
  await f.controls.prepare("pdf");
  assert.equal((await f.published()).includes[0].source, "Current shared content");
  const next = f.collection.files; next[0].source = "Batch chapter"; next[2].source = "Batch include";
  f.controls.applySources(next, f.controls.checkpoint());
  await f.controls.prepare("epub");
  const result = await f.published();
  assert.equal(result.files[0].source, "Batch chapter"); assert.equal(result.includes[0].source, "Batch include");
  assert.equal(f.publishing.endpoints.length, 1);
  assert.equal(f.publishing.endpoints[0].native.stats.updates, 2);
});

test("explicit publisher cancellation releases only its own idle session", async t => {
  const f = await workbench(t); await f.controls.prepare("pdf"); const proof = await f.pdf.build();
  f.el("cancel-export").fire("click");
  assert.equal(f.publishing.client.hasRetainedBook, false);
  assert.equal(f.publishing.endpoints[0].terminated, 1);
  assert.equal(f.proofing.client.hasRetainedBook, true);
  assert.equal(f.el("book-pdf-download").hidden, false);
  assert.equal(await f.pdf.build(), proof, "clearing publication must not rerender a current proof");
  assert.equal(f.el("cancel-export").disabled, true);
});

test("clearing an invalidated PDF view still releases the warm proof worker and not publication", async t => {
  const f = await workbench(t); await f.controls.prepare("pdf"); await f.pdf.build();
  f.edit("Changed");
  assert.equal(f.el("book-pdf-release").disabled, false, "a hidden proof may still own native resources");
  f.el("book-pdf-release").fire("click");
  assert.equal(f.proofing.client.hasRetainedBook, false);
  assert.equal(f.proofing.endpoints[0].terminated, 1);
  assert.equal(f.publishing.client.hasRetainedBook, true);
  await f.pdf.build(); assert.equal(f.proofing.endpoints.length, 2);
});

test("image and font revocation immediately release both retained native sessions", async t => {
  const f = await workbench(t);
  for (const revoke of [() => f.collection.revokeImages(), () => f.collection.revokeFonts()]) {
    await f.controls.prepare("site"); await f.pdf.build();
    revoke();
    assert.equal(f.publishing.client.hasRetainedBook, false);
    assert.equal(f.proofing.client.hasRetainedBook, false);
    assert.equal(f.el("download").hidden, true); assert.equal(f.el("book-pdf-download").hidden, true);
  }
  await f.controls.prepare("pdf");
  assert.deepEqual((await f.published()).images, []); assert.deepEqual((await f.published()).fonts, []);
});

test("settings and structural UI changes conservatively release idle captures", async t => {
  const f = await workbench(t); await f.controls.prepare("site"); await f.pdf.build();
  f.el("title").value = "New book title"; f.el("title").fire("input");
  assert.equal(f.publishing.client.hasRetainedBook, false); assert.equal(f.proofing.client.hasRetainedBook, false);
  await f.controls.prepare("pdf"); assert.equal((await f.published()).settings.metadata[0], "New book title");
  f.el("move-down").fire("click");
  assert.equal(f.publishing.client.hasRetainedBook, false);
  await f.controls.prepare("epub"); assert.equal((await f.published()).files[0].path, "two.md");
  assert.equal(f.publishing.endpoints.length, 3);
});

test("edits cancel an in-flight warm publication and stale results cannot overwrite recovery", async t => {
  const f = await workbench(t, { hold: true }); await f.controls.prepare("pdf");
  f.edit("HOLD"); const old = f.controls.prepare("epub");
  const rejected = assert.rejects(old, code("EXPORT_CANCELLED"));
  for (let i = 0; !f.publishing.endpoints[0].held && i < 20; i++) await tick();
  assert.equal(f.publishing.endpoints[0].held, true);
  f.edit("Current after cancellation"); await rejected;
  assert.equal(f.el("download").hidden, true);
  await f.controls.prepare("site"); const link = f.el("download").href;
  f.publishing.endpoints[0].release(); await tick();
  assert.equal(f.el("download").href, link);
  assert.equal((await f.published()).files[0].source, "Current after cancellation");
});

test("suspension releases idle workers even when resource revocation emits no notification", async t => {
  const f = await workbench(t); await f.controls.prepare("pdf"); await f.pdf.build();
  f.collection.revokeImages = f.collection.revokeFonts = () => {};
  f.controls.suspend(); f.pdf.suspend();
  assert.equal(f.publishing.client.hasRetainedBook, false); assert.equal(f.proofing.client.hasRetainedBook, false);
  assert.equal(f.el("download").hidden, true); assert.equal(f.el("book-pdf-download").hidden, true);
  f.pdf.resume(); assert.equal(f.proofing.endpoints.length, 1, "returning to the page does not render automatically");
});

test("old collection and worker adapters remain conservative rather than requiring new methods", async t => {
  const f = await workbench(t, { legacy: true }); await f.controls.prepare("pdf");
  f.edit("Legacy edit");
  assert.equal(f.publishing.client.hasRetainedBook, false);
  await f.controls.prepare("site");
  assert.equal(f.publishing.endpoints.length, 2);
  assert.equal((await f.published()).files[0].source, "Legacy edit");
});

test("PDF envelope failure after worker success releases the newly idle native session", async () => {
  const host = proofHost(); let idle = false, cancelled = 0;
  host.worker.render = async () => { idle = true; return pdfResult(new Uint8Array([1, 2, 3])); };
  Object.defineProperty(host.worker, "hasRetainedBook", { get: () => idle });
  host.worker.cancel = () => { cancelled++; idle = false; };
  const session = createBookPdfProof(host);
  await assert.rejects(session.render(), code("INVALID_BOOK_PDF"));
  assert.equal(cancelled, 1); assert.equal(idle, false); assert.equal(session.current, null);
  session.dispose();
});

test("retiring a proof releases idle ownership before callbacks can start a newer render", async () => {
  const host = proofHost(); let idle = false, cancelled = 0, reentry;
  Object.defineProperty(host.worker, "hasRetainedBook", { get: () => idle });
  host.worker.cancel = () => { cancelled++; idle = false; };
  const session = createBookPdfProof(host);
  const signal = { aborted: false, addEventListener() {}, removeEventListener() { reentry = session.render(); } };
  const old = session.render({ signal }); idle = true; // Transport has completed; proof continuation has not.
  session.cancel(); await assert.rejects(old, code("EXPORT_CANCELLED"));
  assert.equal(cancelled, 1); assert.equal(host.jobs.length, 2);
  host.jobs[0].reject(Error("late")); await tick();
  assert.equal(host.jobs[1].signal.aborted, false);
  host.jobs[1].resolve(pdfResult()); await reentry;
  session.dispose();
});

test("publisher bootstrap opts in exactly the independent publication and proof workers", async () => {
  const original = await readFile(new URL("../demo/book.js", import.meta.url), "utf8");
  const source = original.replace(/^import[\s\S]*?from "[^\"]+";\n/gm, "");
  assert(!source.includes("import "));
  const workers = [], instances = new Map(), events = new Map();
  const sandbox = {
    document: { querySelector: () => ({}) },
    window: { confirm: () => true, addEventListener(name, fn) { events.set(name, fn); } },
    createBookCollection: () => ({}),
    createBookWorker(options = {}) { const worker = { options, dispose() { this.disposed = true; } }; workers.push(worker); return worker; },
  };
  for (const name of ["Controls", "ImageControls", "FontControls", "InspectionControls", "LinkControls", "LibraryControls", "PdfControls", "PreviewControls", "SearchControls"]) {
    sandbox[`createBook${name}`] = options => {
      const instance = { options, suspended: 0, resumed: 0, disposed: 0,
        suspend() { this.suspended++; }, resume() { this.resumed++; }, dispose() { this.disposed++; } };
      instances.set(name, instance); return instance;
    };
  }
  for (const name of ["Image", "Font", "Inspection", "Link", "Pdf"]) sandbox[`createBook${name}Panel`] = () => {};
  vm.runInNewContext(source, sandbox);
  assert.equal(workers.length, 6);
  assert.equal(workers.filter(w => w.options.retainBook === true).length, 2);
  assert.equal(instances.get("Controls").options.worker.options.retainBook, true);
  assert.equal(instances.get("PdfControls").options.worker.options.retainBook, true);
  assert.equal(instances.get("PreviewControls").options.worker.options.retainPreview, true);
  assert.notEqual(instances.get("Controls").options.worker, instances.get("PdfControls").options.worker);
  assert.equal(instances.get("FontControls").options.worker.options.retainBook, undefined);
  events.get("pagehide")({ persisted: true });
  assert([...instances.values()].every(i => i.suspended === 1));
  events.get("pageshow")({ persisted: true }); assert.equal(instances.get("PdfControls").resumed, 1);
  events.get("pagehide")({ persisted: false }); assert([...instances.values()].every(i => i.disposed === 1));
});

test("retained runtime dependency ships in both assemblers and the isolated browser helper", async () => {
  const name = "book_retained.mjs";
  const manifest = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
  assert(manifest.files.includes(name));
  assert((await readFile(new URL(`../${name}`, import.meta.url), "utf8")).length > 0);
  for (const script of ["dsr-wasm-package.sh", "check-wasm-package.sh"]) {
    const source = await readFile(new URL(`../../scripts/${script}`, import.meta.url), "utf8");
    const lists = [...source.matchAll(/for file in ([\s\S]*?); do/g)].map(m => m[1].replaceAll("\\", "").split(/\s+/));
    assert(lists.some(files => files.includes("book_worker.mjs") && files.includes(name)), `${script} must copy the dependency with its importing worker`);
  }
  const browser = await readFile(new URL("../book_images_browser.py", import.meta.url), "utf8");
  const modules = browser.slice(browser.indexOf("modules ="), browser.indexOf("]}"));
  assert(modules.includes(`"${name}"`));
  assert(modules.indexOf(`"${name}"`) < modules.indexOf('"book_worker.mjs"'), "blob-module dependencies must be linked before their importer");
});
