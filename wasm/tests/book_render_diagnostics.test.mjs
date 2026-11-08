import assert from "node:assert/strict";
import test from "node:test";
import { BOOK_RENDER_DIAGNOSTICS_MAX_BYTES, parseBookRenderDiagnostics,
  bookRenderDiagnosticsReceipt, bookRenderDiagnosticsByteLength } from "../book_render_diagnostics.mjs";
import { prepareBookInput } from "../book_session.mjs";
import { createRetainedBook, createCachedBookRenderer } from "../book_retained.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createBookPublicationClient } from "../book_publication.mjs";
import { createBookPdfProof } from "../book_pdf_proof.mjs";
import { createBookEditionSession } from "../demo/book_edition.mjs";
import { chapters, included, geometry } from "./book_pdfa_fixture.mjs";
import { pdfResult, proofHost } from "./book_pdf_proof_helpers.mjs";
import { renderDiagnosticsFixture } from "./book_render_diagnostics_fixture.mjs";

// Production admission, session, worker protocol, cache, publication and view
// ownership. Native exports and EventTarget endpoints are explicit boundaries;
// structuredClone really transfers buffers. These are not native PDF proofs.
const invalid = { code: "INVALID_BOOK_RENDER_DIAGNOSTICS" };
const warning = (message = "Image unavailable — α 😀") => ({ code: "unresolved_image", severity: "warning",
  start: 0, end: 0, scope: "document", message });
const immutable = value => {
  assert(Object.isFrozen(value));
  for (const item of value) assert(Object.isFrozen(item));
};

test("diagnostics admission owns immutable records and distinguishes unavailable from clean", () => {
  const source = [warning(), { code: "layout_error", severity: "error", start: 1, end: 3, message: "A bounded source finding" }];
  const parsed = parseBookRenderDiagnostics(JSON.stringify(source), 4);
  assert.deepEqual(parsed, source); immutable(parsed);
  assert.equal(bookRenderDiagnosticsByteLength(parsed), new TextEncoder().encode(JSON.stringify(source)).length);
  const receipt = bookRenderDiagnosticsReceipt({ diagnostics: source, sourceLength: 4 }, "pdf");
  source[0].message = "Changed after receipt"; source.pop();
  assert.equal(receipt.diagnostics[0].message, warning().message); assert.equal(receipt.diagnostics.length, 2);
  immutable(receipt.diagnostics);
  assert.throws(() => { receipt.diagnostics[0].message = "Changed"; }, TypeError);
  assert.deepEqual(bookRenderDiagnosticsReceipt({ sourceLength: 0 }, "pdf"), {});
  assert.deepEqual(bookRenderDiagnosticsReceipt({ diagnostics: undefined, sourceLength: 0 }, "pdf"), {});
  const clean = bookRenderDiagnosticsReceipt({ diagnostics: [], sourceLength: 0 }, "pdf");
  assert.deepEqual(clean.diagnostics, []); immutable(clean.diagnostics);
  assert.equal(bookRenderDiagnosticsByteLength(clean.diagnostics), 2);
  assert.equal(bookRenderDiagnosticsByteLength(undefined), 0);
  assert.throws(() => bookRenderDiagnosticsByteLength([]), invalid, "only admitted arrays carry a trusted byte count");
});

test("malformed or oversized findings are refused without invoking record or array accessors", () => {
  const cases = [null, {}, [null], [warning("")], [{ ...warning(), code: "bad code" }],
    [{ ...warning(), code: "a".repeat(129) }], [{ ...warning(), severity: "info" }],
    [{ ...warning(), start: -1 }], [{ ...warning(), start: 2, end: 1 }],
    [{ ...warning(), start: 0.5 }], [{ ...warning(), end: 5 }],
    [{ ...warning(), scope: "chapter" }], [{ ...warning(), start: 1, end: 1 }],
    [{ ...warning(), path: "not-an-admitted-location.md" }], [warning("\ud800")],
    [warning("😀".repeat(513))], Array(4097).fill(warning()), new Array(1)];
  for (const diagnostics of cases)
    assert.throws(() => bookRenderDiagnosticsReceipt({ diagnostics, sourceLength: 4 }, "pdf"), invalid);
  for (const sourceLength of [-1, 0.5, NaN, "4", 64 * 1024 * 1024 + 1])
    assert.throws(() => bookRenderDiagnosticsReceipt({ diagnostics: [], sourceLength }, "pdf"), invalid);
  assert.throws(() => bookRenderDiagnosticsReceipt({ diagnostics: [], sourceLength: 4 }, "epub"), invalid);
  for (const json of ["{", "null", "{}", "[", " ".repeat(BOOK_RENDER_DIAGNOSTICS_MAX_BYTES + 1)])
    assert.throws(() => parseBookRenderDiagnostics(json, 4), invalid);
  // Every message is individually admissible; their escaped JSON exceeds the
  // aggregate budget. Transfer admission must enforce it without a giant copy.
  const escaped = Array(4096).fill(warning("\n".repeat(600)));
  assert.throws(() => bookRenderDiagnosticsReceipt({ diagnostics: escaped, sourceLength: 0 }, "pdf"), invalid);
  let reads = 0;
  const item = { ...warning() };
  Object.defineProperty(item, "message", { get() { reads++; return "accessor"; } });
  const array = [];
  Object.defineProperty(array, "0", { get() { reads++; return warning(); } });
  const envelope = { sourceLength: 4 };
  Object.defineProperty(envelope, "diagnostics", { get() { reads++; return []; } });
  for (const value of [{ diagnostics: [item], sourceLength: 4 }, { diagnostics: array, sourceLength: 4 }, envelope])
    assert.throws(() => bookRenderDiagnosticsReceipt(value, "pdf"), invalid);
  assert.equal(reads, 0);
});

test("ordinary, paper override and archival exports choose their reported methods and free both handles", async () => {
  const f = renderDiagnosticsFixture();
  for (const [options, method] of [
    [{}, "renderPdfReported"], [{ page: geometry() }, "renderPdfWithPageReported"],
    [{ pdfA: "2b", pdfAStrict: true, page: geometry() }, "renderPdfArchivalReported"],
  ]) {
    const output = await f.api.renderBookPdf(chapters(), { ...options, includeSources: included() });
    assert.equal(output.format, "book-pdf"); immutable(output.diagnostics);
    assert.equal(f.state.reportedCalls.at(-1).method, method);
    assert.deepEqual(output.diagnostics, f.state.results.at(-1).notes);
    if (options.page) {
      const args = f.state.reportedCalls.at(-1).args;
      assert.deepEqual([...args.at(-1)], [720, 540, 18, 24, 30, 36]);
      if (options.pdfA) { assert.equal(args[0], "2b"); assert.equal(args[1], true); }
    }
    assert.equal(output.pdfA, options.pdfA); assert.equal(output.pdfAStrict, options.pdfAStrict);
  }
  assert.equal(f.state.reportFreed, 3); assert.equal(f.state.freed, 3);
  assert.deepEqual(f.state.legacyCalls, [], "reported output is never followed by a second legacy render");
});

test("retained per-export paper and archival overrides preserve report ownership", async t => {
  const f = renderDiagnosticsFixture(), session = await f.api.createBook(chapters());
  t.after(() => session.dispose());
  const ordinary = session.renderPdf(), archived = session.renderPdf({ pdfA: "2b", pdfAStrict: true }),
    paper = session.renderPdf({ page: geometry() });
  assert.deepEqual(f.state.reportedCalls.map(value => value.method),
    ["renderPdfReported", "renderPdfArchivalReported", "renderPdfWithPageReported"]);
  assert.equal(f.state.reportedCalls[1].args[2].length, 0);
  assert.equal(ordinary.diagnostics[0].message, f.state.results[0].notes[0].message);
  f.state.results[0].notes[0].message = "Later native state";
  assert.match(ordinary.diagnostics[0].message, /^Render 1:/);
  assert.match(archived.diagnostics[0].message, /^Render 2:/);
  assert.match(paper.diagnostics[0].message, /^Render 3:/);
  assert.equal(f.state.reportFreed, 3); assert.equal(f.state.freed, 0);
});

for (const [name, patch, expected = invalid] of [
  ["bad JSON", result => { result.diagnosticsJson = () => "{"; }],
  ["mismatched source", result => { result.sourceLength++; }],
  ["wrong format", result => { result.format = "html"; }],
  ["wrong MIME", result => { result.mimeType = "text/html"; }],
  ["wrong extension", result => { result.extension = "txt"; }],
  ["missing diagnostics method", result => { result.diagnosticsJson = undefined; }],
  ["empty bytes", result => { result.bytes = new Uint8Array(); }],
  ["shared bytes", result => { result.bytes = new Uint8Array(new SharedArrayBuffer(4)); }],
  ["diagnostics throw", result => { result.diagnosticsJson = () => { throw "diagnostic read failed"; }; }, /diagnostic read failed/],
  ["byte read throws", result => { Object.defineProperty(result, "bytes", { get() { throw Error("byte read failed"); } }); }, /byte read failed/],
]) test(`reported ${name} frees the native result and refuses byte fallback`, async () => {
  const f = renderDiagnosticsFixture(); f.state.changeResult = patch;
  await assert.rejects(f.api.renderBookPdf(chapters()), expected);
  assert.equal(f.state.reportedCalls.length, 1); assert.equal(f.state.reportFreed, 1);
  assert.equal(f.state.freed, 1); assert.deepEqual(f.state.legacyCalls, []);
});

test("a reported native throw disposes the book without a second render", async () => {
  const f = renderDiagnosticsFixture();
  f.state.beforeReport = () => { throw Object.assign(Error("native export refused"), { code: "NATIVE_REFUSAL" }); };
  await assert.rejects(f.api.renderBookPdf(chapters()), { code: "NATIVE_REFUSAL" });
  assert.equal(f.state.reportedCalls.length, 1); assert.equal(f.state.reportFreed, 0); assert.equal(f.state.freed, 1);
  assert.deepEqual(f.state.legacyCalls, []);
});

test("legacy byte exports have absent diagnostics, while a reported clean result owns an empty array", async () => {
  const old = renderDiagnosticsFixture({ legacy: true });
  for (const options of [{}, { page: geometry() }, { pdfA: "2b", pdfAStrict: true, page: geometry() }]) {
    const output = await old.api.renderBookPdf(chapters(), options);
    assert.equal(Object.hasOwn(output, "diagnostics"), false);
  }
  assert.deepEqual(old.state.legacyCalls, ["renderPdf", "renderPdfWithPage", "renderPdfArchival"]);
  assert.equal(old.state.freed, 3); assert.equal(old.state.reportFreed, 0);
  const modern = renderDiagnosticsFixture(), storage = Uint8Array.of(99, 10, 20, 88);
  modern.state.changeResult = result => { result.diagnosticsJson = () => "[]"; result.bytes = storage.subarray(1, 3); };
  const clean = await modern.api.renderBookPdf(chapters()); storage.fill(0);
  assert.deepEqual(clean.diagnostics, []); immutable(clean.diagnostics);
  assert.deepEqual([...clean.bytes], [10, 20]); assert.equal(clean.bytes.buffer.byteLength, 2);
  assert.equal(modern.state.reportFreed, 1);
});

function wireHarness(t, { legacy = false, changeNative, mutateWire } = {}) {
  const endpoints = [], sent = [];
  function factory() {
    const native = renderDiagnosticsFixture({ legacy }); changeNative?.(native);
    const preview = (engine, files, options) => engine.renderBookSite(files, options);
    const retained = createRetainedBook(native.api, preview), cache = createCachedBookRenderer(retained, preview);
    const endpoint = new EventTarget(), scope = new EventTarget();
    Object.assign(endpoint, { native, cache, terminated: false });
    endpoint.terminate = () => { endpoint.terminated = true; cache.clear(); };
    endpoint.postMessage = (data, transfer = []) => {
      const copy = structuredClone(data, { transfer }); sent.push(copy);
      queueMicrotask(() => { if (!endpoint.terminated) scope.dispatchEvent(new MessageEvent("message", { data: copy })); });
    };
    scope.postMessage = (data, transfer = []) => {
      const copy = structuredClone(data, { transfer });
      for (const buffer of transfer) assert.equal(buffer.byteLength, 0, "worker output buffers really transfer");
      mutateWire?.(copy);
      queueMicrotask(() => { if (!endpoint.terminated) endpoint.dispatchEvent(new MessageEvent("message", { data: copy })); });
    };
    installBookWorker(scope, { ...native.api, renderRetainedBook: cache.render, clearRetainedBook: cache.clear });
    endpoints.push(endpoint); return endpoint;
  }
  return { endpoints, sent, createClient() {
    const client = createBookWorkerClient({ workerFactory: factory, retainBook: true, timeoutMs: 5000 });
    t.after(() => client.dispose()); return client;
  } };
}

test("real worker protocol carries immutable findings through transfer, cache hits and source undo", async t => {
  const wire = wireHarness(t), client = wire.createClient(), files = chapters(), options = { includeSources: included() };
  const first = await client.render(files, "pdf", options), expected = first.bytes.slice();
  immutable(first.diagnostics);
  structuredClone(first.bytes, { transfer: [first.bytes.buffer] });
  assert.equal(first.bytes.byteLength, 0);
  const hit = await client.render(files, "pdf", options), endpoint = wire.endpoints[0];
  assert.deepEqual(hit.bytes, expected); assert.deepEqual(hit.diagnostics, first.diagnostics); immutable(hit.diagnostics);
  assert.equal(endpoint.native.state.reportedCalls.length, 1); assert.equal(endpoint.cache.cacheStats.hits, 1);
  const changed = await client.renderSourceUpdate([{ ...files[0], source: "Changed source α" }], "pdf",
    { expectedRevision: client.retainedInputRevision });
  assert.match(changed.diagnostics[0].message, /^Render 2:/);
  const restored = await client.renderSourceUpdate([files[0]], "pdf", { expectedRevision: client.retainedInputRevision });
  assert.deepEqual(restored.bytes, expected); assert.deepEqual(restored.diagnostics, first.diagnostics);
  assert.equal(endpoint.native.state.reportedCalls.length, 2); assert.equal(endpoint.native.state.reportFreed, 2);
  assert.equal(wire.endpoints.length, 1); assert.equal(wire.sent.length, 4);
  client.dispose(); assert.equal(endpoint.native.state.freed, 1);
  assert.deepEqual(first.diagnostics, restored.diagnostics, "retiring the native/cache owner cannot change an admitted report");
});

test("malformed findings crossing a worker boundary fail closed and a later request can recover", async t => {
  let corrupt = true;
  const wire = wireHarness(t, { mutateWire(value) {
    if (corrupt && value.diagnostics) { corrupt = false; value.diagnostics[0].end = value.sourceLength + 1; }
  } });
  const client = wire.createClient();
  await assert.rejects(client.render(chapters(), "pdf"), invalid);
  assert.equal(wire.endpoints[0].terminated, true); assert.equal(client.busy, false);
  const next = await client.render(chapters(), "pdf"); immutable(next.diagnostics);
  assert.equal(wire.endpoints.length, 2); assert.equal(wire.endpoints[0].native.state.reportFreed, 1);
});

test("cache budgets include diagnostics payloads and copied records cannot be poisoned", async () => {
  const input = prepareBookInput([{ path: "one.md", source: "text" }]);
  let calls = 0;
  const notes = [warning("A".repeat(200))];
  const renderer = { clear() {}, async render() { calls++; return { bytes: Uint8Array.of(1), sourceLength: 4, diagnostics: notes }; } };
  const preview = () => { throw Error("unexpected preview"); };
  const tight = createCachedBookRenderer(renderer, preview, { maxBytes: 128, maxEntryBytes: 128 });
  await tight.render(input.files, input.options, "pdf"); await tight.render(input.files, input.options, "pdf");
  assert.equal(calls, 2); assert.equal(tight.cacheStats.entries, 0, "tiny PDF bytes do not hide a larger diagnostics allocation");
  const cache = createCachedBookRenderer(renderer, preview), first = await cache.render(input.files, input.options, "pdf");
  assert.equal(cache.cacheStats.bytes, first.bytes.byteLength + bookRenderDiagnosticsByteLength(first.diagnostics));
  const message = first.diagnostics[0].message; notes[0].message = "Changed producer state"; first.bytes[0] = 9;
  const hit = await cache.render(input.files, input.options, "pdf");
  assert.equal(calls, 3); assert.equal(hit.bytes[0], 1); assert.equal(hit.diagnostics[0].message, message); immutable(hit.diagnostics);
  tight.clear(); cache.clear(); assert.equal(first.diagnostics[0].message, message);
});

for (const legacy of [false, true]) test(`multi-format edition keeps PDF findings through later exports and retires stale state: legacy=${legacy}`, async t => {
  const wire = wireHarness(t, { legacy }), input = prepareBookInput(chapters(), { includeSources: included() });
  const publisher = createBookPublicationClient({ createClient: wire.createClient, captureInput: prepareBookInput, maxWorkerRetries: 0 });
  let revision = 0, captured;
  const live = new Map(), revoked = [], urls = {
    createObjectURL(blob) { const href = `blob:edition/${live.size}`; live.set(href, blob); return href; },
    revokeObjectURL(href) { revoked.push(href); live.delete(href); },
  };
  const session = createBookEditionSession({
    publisher: { ...publisher, async render(...args) { captured = await publisher.render(...args); return captured; } },
    capture: () => structuredClone(input), checkpoint: () => revision, urls,
  });
  t.after(() => session.dispose());
  const ready = await session.prepare(), pdf = captured.outputs[0];
  assert.deepEqual(captured.outputs.map(output => output.format), ["book-pdf", "book-epub", "book-site"]);
  assert.equal(live.size, 3); assert.equal(wire.endpoints[0].terminated, true);
  for (const output of captured.outputs.slice(1)) assert.equal(Object.hasOwn(output, "diagnostics"), false);
  if (legacy) {
    assert.equal(Object.hasOwn(pdf, "diagnostics"), false); assert.equal(ready.pdfDiagnostics, undefined);
  } else {
    immutable(pdf.diagnostics); immutable(ready.pdfDiagnostics);
    assert.deepEqual(ready.pdfDiagnostics, pdf.diagnostics);
    wire.endpoints[0].native.state.results[0].notes[0].message = "Later native output";
    assert.match(pdf.diagnostics[0].message, /^Render 1:/);
    assert.equal(wire.endpoints[0].native.state.reportFreed, 1);
  }
  revision++;
  assert.equal(session.state.pdfDiagnostics, undefined, "silent raw edits cannot keep an old findings verdict visible");
  session.changed(); assert.equal(live.size, 0); assert.equal(revoked.length, 3);
  assert.equal(session.state.downloads.length, 0);
  session.setFormats(["epub"]);
  const epub = await session.prepare(); assert.equal(epub.pdfDiagnostics, undefined); assert.equal(epub.downloads.length, 1);
});

test("an invalid later PDF report prevents a partial edition or any download URL", async t => {
  const wire = wireHarness(t, { changeNative(f) { f.state.changeResult = result => { result.diagnosticsJson = () => "{}"; }; } });
  const publisher = createBookPublicationClient({ createClient: wire.createClient, captureInput: prepareBookInput, maxWorkerRetries: 0 });
  let urls = 0;
  const session = createBookEditionSession({ publisher, capture: () => prepareBookInput(chapters()), checkpoint: () => 0,
    urls: { createObjectURL() { urls++; return "blob:unexpected"; }, revokeObjectURL() {} } });
  t.after(() => session.dispose()); session.setFormats(["epub", "pdf", "site"]);
  await assert.rejects(session.prepare(), invalid);
  assert.equal(urls, 0); assert.equal(session.state.downloads.length, 0); assert.equal(session.state.pdfDiagnostics, undefined);
  assert.deepEqual(wire.endpoints[0].native.state.calls.map(value => value.kind), ["epub", "pdf"]);
  assert.equal(wire.endpoints[0].native.state.reportFreed, 1); assert.equal(wire.endpoints[0].native.state.freed, 1);
});

test("PDF proofs keep immutable findings with their Blob and reject late stale reports", async t => {
  const host = proofHost(), proof = createBookPdfProof(host); t.after(() => proof.dispose());
  const firstPending = proof.render(), notes = [warning("First proof")], output = { ...pdfResult(), diagnostics: notes };
  host.jobs[0].resolve(output); const first = await firstPending;
  immutable(first.diagnostics); notes[0].message = "Changed worker record";
  assert.equal(first.diagnostics[0].message, "First proof");
  const bytes = await first.blob.text(); output.bytes.fill(0); assert.equal(await first.blob.text(), bytes);
  host.edit("New source"); assert.equal(proof.current, null);
  const pending = proof.render(), old = host.jobs.at(-1), rejected = assert.rejects(pending, { code: "STALE_SOURCE" });
  host.edit("Newest source"); await rejected;
  const nextPending = proof.render(); host.jobs.at(-1).resolve({ ...pdfResult(), diagnostics: [warning("Current proof")] });
  const current = await nextPending;
  old.resolve({ ...pdfResult(), diagnostics: [warning("Stale proof")] }); await Promise.resolve();
  assert.equal(proof.current, current); assert.equal(current.diagnostics[0].message, "Current proof");
  proof.invalidate(); assert.equal(proof.current, null);
});

test("proofs preserve unavailable versus clean reports and refuse malformed findings", async t => {
  for (const diagnostics of [undefined, [], [warning("")]]) {
    const host = proofHost(), proof = createBookPdfProof(host); t.after(() => proof.dispose());
    const pending = proof.render(); host.jobs[0].resolve({ ...pdfResult(), ...(diagnostics === undefined ? {} : { diagnostics }) });
    if (diagnostics?.length) {
      await assert.rejects(pending, invalid); assert.equal(proof.current, null);
    } else {
      const result = await pending;
      assert.equal(Object.hasOwn(result, "diagnostics"), diagnostics !== undefined);
      if (diagnostics) { assert.deepEqual(result.diagnostics, []); immutable(result.diagnostics); }
    }
  }
});
