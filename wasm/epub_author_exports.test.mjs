import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createWorkerRenderer } from "./document_worker.mjs";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { renderDocumentBatch } from "./document-batch.js";
import { normalizeFlowExport, withFlowExports } from "./flow_export.mjs";

// Real Node threads execute the production client, transport, dispatcher and
// export coordinator. The native renderers/snapshots are explicit doubles;
// these tests do not establish generated-WASM output or EPUB-reader conformance.
const author = "  É中 & <Research>\r\nCo-author 😀  ";
const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));
const code = expected => error => error.code === expected;
const token = () => ({ revision: "9007199254740993", layoutRevision: "9" });

function workerFactory(t, fixture, workerData = {}) {
  const workers = [];
  const create = () => {
    const worker = new Worker(new URL(fixture, import.meta.url), { workerData });
    workers.push(worker);
    const listeners = new Map();
    return {
      postMessage(message, transfer) { worker.postMessage(message, transfer); },
      terminate() { return worker.terminate(); },
      addEventListener(kind, callback) {
        const listener = kind === "message" ? data => {
          if (!data.testEvent) callback({ data });
        } : callback;
        listeners.set(callback, listener);
        worker.on(kind, listener);
      },
      removeEventListener(kind, callback) {
        worker.off(kind, listeners.get(callback));
        listeners.delete(callback);
      },
    };
  };
  t.after(async () => { await Promise.all(workers.map(worker => worker.terminate())); });
  return { create, workers };
}
function documentWorker(t, data = {}, runtime = {}) {
  const factory = workerFactory(t, "./tests/document_publication_worker_fixture.mjs", data);
  const renderer = createWorkerRenderer({ timeoutMs: 5000, ...runtime, workerFactory: factory.create });
  t.after(() => renderer.dispose());
  return { renderer, workers: factory.workers };
}
async function flowWorker(t, data = {}, runtime = {}) {
  const factory = workerFactory(t, "./tests/flow_export_worker_fixture.mjs", data);
  const session = await createWorkerFlowSessionWith(factory.create, "# Source 😀", { font: "serif" },
    { timeoutMs: 5000, ...runtime });
  t.after(() => session.dispose());
  return session;
}
function directFlow() {
  const calls = [];
  const session = {
    disposed: false, source: "# Source 😀", token: token(),
    snapshot({ offset }) {
      return { schemaVersion: 1, ...token(), offset, total: 0, nextOffset: null, items: [] };
    },
  };
  const formats = { html: "text/html; charset=utf-8", pdf: "application/pdf", epub: "application/epub+zip" };
  const output = (format, source, options) => ({ format, mimeType: formats[format], diagnostics: [],
    bytes: new TextEncoder().encode(JSON.stringify({ source, options })) });
  const renderers = Object.fromEntries(Object.keys(formats).map(format => [format, async (source, options) => {
    calls.push({ format, source, options });
    return output(format, source, options);
  }]));
  return { calls, renderers, output, wrap: () => withFlowExports(session, renderers, "serif") };
}

test("document EPUB authors survive queueing and per-format publication capture", async t => {
  const { renderer } = documentWorker(t);
  const first = renderer.renderHtml("first");
  const settings = { author, title: "Before", profile: "commonmark-gfm" };
  const epub = renderer.renderEpub("# Owned source", settings);
  const targets = [
    { format: "pdf", options: { author: "PDF creator" } },
    { format: "epub", options: settings },
    { format: "html" },
  ];
  const publication = renderer.publish("# One edition", targets);
  // Keep failures observed even when an earlier assertion fails.
  for (const pending of [first, epub, publication]) pending.catch(() => {});
  settings.author = "Later";
  settings.title = "After";
  targets[0].options.author = "Later PDF creator";
  await first;
  const single = decode(await epub);
  assert.equal(single.source, "# Owned source");
  assert.equal(single.options.author, author);
  assert.equal(single.options.title, "Before");
  assert.equal(single.options.profile, "commonmark-gfm");
  const outputs = (await publication).outputs.map(decode);
  assert.deepEqual(outputs.map(value => value.options.author), ["PDF creator", author, undefined]);
  assert.ok(outputs.every(value => value.source === "# One edition"));
  assert.equal(decode(await renderer.renderEpub("---\nauthor: Inherited\n---\n# Book", { author: "" })).options.author, "");
  assert.equal(Object.hasOwn(decode(await renderer.renderEpub("# Book")).options, "author"), false);
  assert.equal(renderer.pendingOperations, 0);
  assert.equal(renderer.pendingBytes, 0);
});

test("document EPUB author admission bounds UTF-8 before starting workers", async t => {
  const { renderer, workers } = documentWorker(t);
  let reads = 0;
  const accessor = Object.defineProperty({}, "author", { get() { reads++; return "Never"; } });
  for (const value of [null, 1, {}, "é".repeat(32769), "x".repeat(65537), "broken\ud800"]) {
    await assert.rejects(renderer.renderEpub("# Book", { author: value }));
    await assert.rejects(renderer.publish("# Book", [
      { format: "html" }, { format: "epub", options: { author: value } },
    ]));
  }
  await assert.rejects(renderer.renderEpub("# Book", accessor), code("INVALID_OPTIONS"));
  assert.equal(reads, 0);
  assert.equal(workers.length, 0);
  const limit = "é".repeat(32768);
  assert.equal(decode(await renderer.renderEpub("# Book", { author: limit })).options.author, limit);
  for (const format of ["svg", "interactive-html"])
    await assert.rejects(renderer.render(format, "# Book", { author: "Unsupported here" }), code("INVALID_OPTIONS"));
  assert.equal(renderer.pendingBytes, 0);
});

test("document publications propagate unsupported EPUB metadata without partial artifacts", async t => {
  const { renderer } = documentWorker(t, { epubAuthorSupport: false });
  await assert.rejects(renderer.publish("# Book", [
    { format: "html" }, { format: "epub", options: { author } },
  ]), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(renderer.pendingOperations, 0);
  assert.equal(renderer.pendingBytes, 0);
  assert.equal(renderer.disposed, true);
  const fresh = documentWorker(t, { epubAuthorSupport: false }).renderer;
  assert.equal((await fresh.renderEpub("# Book")).format, "epub");
});

test("EPUB author strings participate in document worker ingress limits", async t => {
  const { renderer, workers } = documentWorker(t, {}, { maxPendingBytes: 2048 });
  await assert.rejects(renderer.renderEpub("# Book", { author: "x".repeat(800) }), code("WORKER_QUEUE_FULL"));
  await assert.rejects(renderer.publish("# Book", [
    { format: "epub", options: { author: "x".repeat(800) } },
  ]), code("WORKER_QUEUE_FULL"));
  assert.equal(workers.length, 0);
  assert.equal(renderer.pendingBytes, 0);
  assert.equal(decode(await renderer.renderEpub("# Book", { author })).options.author, author);
});

test("mixed browser batches keep each source and target's EPUB author", async t => {
  const factory = workerFactory(t, "./tests/document_publication_worker_fixture.mjs");
  const single = { format: "epub", source: "# First", options: { author } };
  const edition = { source: "# Second", targets: [
    { format: "html" }, { format: "epub", options: { author: "Other creator" } },
    { format: "pdf", options: { author: "PDF creator" } },
  ] };
  const stream = renderDocumentBatch([single, edition], { concurrency: 2,
    worker: { workerFactory: factory.create, timeoutMs: 5000 } });
  t.after(() => stream.return());
  const pending = stream.next();
  single.options.author = "Changed";
  edition.targets[1].options.author = "Changed too";
  const first = (await pending).value;
  assert.equal(first.status, "fulfilled");
  assert.equal(decode(first.value).options.author, author);
  const second = (await stream.next()).value;
  assert.equal(second.status, "fulfilled");
  assert.deepEqual(second.value.outputs.map(value => decode(value).options.author),
    [undefined, "Other creator", "PDF creator"]);
  assert.ok(second.value.outputs.every(value => decode(value).source === "# Second"));
  assert.equal((await stream.next()).done, true);
});

test("direct Flow EPUB authors preserve exact text, empty inputs and publication ownership", async () => {
  const f = directFlow();
  let release;
  f.renderers.html = () => new Promise(resolve => { release = resolve; });
  const api = f.wrap(), settings = { author, title: "Guide" };
  const pending = api.exportDocuments([
    { format: "html" }, { format: "epub", options: settings },
    { format: "pdf", options: { author: "PDF creator" } },
  ], api.token);
  pending.catch(() => {});
  settings.author = "Changed";
  assert.equal(typeof release, "function");
  release(f.output("html", api.source, {}));
  const outputs = await pending;
  assert.deepEqual(outputs.map(value => decode(value).options.author), [undefined, author, "PDF creator"]);
  assert.equal(decode(await api.exportDocument("epub", { author: "" }, api.token)).options.author, "");
  assert.equal(Object.hasOwn(decode(await api.exportDocument("epub", {}, api.token)).options, "author"), false);
});

test("Flow EPUB author admission counts UTF-8 and preserves other format boundaries", async () => {
  const f = directFlow(), api = f.wrap();
  let reads = 0;
  const accessor = Object.defineProperty({}, "author", { get() { reads++; return "Never"; } });
  for (const value of [null, 1, {}, "é".repeat(32769), "x".repeat(65537), "broken\ud800"])
    await assert.rejects(api.exportDocument("epub", { author: value }, api.token));
  await assert.rejects(api.exportDocument("epub", accessor, api.token), code("INVALID_OPTIONS"));
  assert.equal(reads, 0);
  assert.equal(f.calls.length, 0);
  const limit = "é".repeat(32768);
  assert.equal(decode(await api.exportDocument("epub", { author: limit }, api.token)).options.author, limit);
  for (const format of ["svg"])
    assert.throws(() => normalizeFlowExport(format, { author }, token()), code("INVALID_OPTIONS"));
});

test("Flow worker EPUB authors are captured and charged before queueing", async t => {
  const session = await flowWorker(t, { delayMs: 20, image: true });
  const first = session.exportDocument("pdf", {}, session.token);
  first.catch(() => {});
  const before = session.pendingBytes, repeated = author.repeat(128), settings = { author: repeated };
  const pending = session.exportDocument("epub", settings, session.token);
  pending.catch(() => {});
  assert.ok(session.pendingBytes >= before + 2 * repeated.length);
  settings.author = "Changed";
  await first;
  const output = decode(await pending);
  assert.equal(output.author, repeated);
  assert.deepEqual(output.images, [["local.png", [1, 2, 3]]]);
  const targets = [
    { format: "html" }, { format: "epub", options: { author: "EPUB creator" } },
    { format: "pdf", options: { author: "PDF creator" } },
  ];
  const publication = session.exportDocuments(targets, session.token);
  targets[1].options.author = "Later";
  assert.deepEqual((await publication).map(value => decode(value).author), [undefined, "EPUB creator", "PDF creator"]);
  assert.equal(decode(await session.exportDocument("epub", { author: "" }, session.token)).author, "");
  assert.equal(Object.hasOwn(decode(await session.exportDocument("epub", {}, session.token)), "author"), false);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
});

test("Flow workers report stale native metadata support and remain usable after refusal", async t => {
  const session = await flowWorker(t, { epubAuthorSupport: false });
  const expected = error => error.code === "UNSUPPORTED_WASM_PACKAGE" && /EPUB author/.test(error.message);
  await assert.rejects(session.exportDocument("epub", { author }, session.token), expected);
  await assert.rejects(session.exportDocuments([
    { format: "html" }, { format: "epub", options: { author } },
  ], session.token), expected);
  await assert.rejects(session.exportDocument("epub", { author: "é".repeat(32769) }, session.token), code("INVALID_OPTIONS"));
  assert.equal((await session.exportDocument("epub", {}, session.token)).format, "epub");
  assert.equal(session.disposed, false);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
});

test("document HTML authors survive queueing and atomic publication capture", async t => {
  const { renderer } = documentWorker(t);
  const first = renderer.renderPdf("first");
  const source = "---\nauthor: Source creator\n---\n# Owned source";
  const settings = { author, title: "Before", profile: "gfm-plus" };
  const html = renderer.renderHtml(source, settings);
  const targets = [
    { format: "pdf", options: { author: "PDF creator" } },
    { format: "html", options: settings },
    { format: "epub", options: { author: "EPUB creator" } },
  ];
  const publication = renderer.publish(source, targets);
  for (const pending of [first, html, publication]) pending.catch(() => {});
  settings.author = "Later"; settings.title = "After"; targets[0].options.author = "Later PDF";
  await first;
  const single = decode(await html);
  assert.equal(single.source, source); assert.equal(single.options.author, author);
  assert.equal(single.options.title, "Before"); assert.equal(single.options.profile, "gfm-plus");
  const outputs = (await publication).outputs.map(decode);
  assert.deepEqual(outputs.map(value => value.options.author), ["PDF creator", author, "EPUB creator"]);
  assert.ok(outputs.every(value => value.source === source));
  for (const explicit of ["", "  "])
    assert.equal(decode(await renderer.renderHtml(source, { author: explicit })).options.author, explicit);
  assert.equal(Object.hasOwn(decode(await renderer.renderHtml(source)).options, "author"), false);
  assert.equal(renderer.pendingOperations, 0); assert.equal(renderer.pendingBytes, 0);
});

test("document HTML author admission validates UTF-8 and charges ingress before workers start", async t => {
  const { renderer, workers } = documentWorker(t);
  let reads = 0;
  const accessor = Object.defineProperty({}, "author", { get() { reads++; return "Never"; } });
  for (const value of [null, 42, {}, "é".repeat(32769), "x".repeat(65537), "broken\ud800"]) {
    const expected = code(typeof value === "string" && value.startsWith("broken") ? "INVALID_UNICODE" : "BUDGET_EXCEEDED");
    await assert.rejects(renderer.renderHtml("# Article", { author: value }), expected);
    await assert.rejects(renderer.publish("# Article", [{ format: "pdf" },
      { format: "html", options: { author: value } }]), expected);
  }
  await assert.rejects(renderer.renderHtml("# Article", accessor), code("INVALID_OPTIONS"));
  assert.equal(reads, 0); assert.equal(workers.length, 0);
  const limit = "é".repeat(32768);
  assert.equal(decode(await renderer.renderHtml("# Article", { author: limit })).options.author, limit);
  const limited = documentWorker(t, {}, { maxPendingBytes: 2048 });
  await assert.rejects(limited.renderer.renderHtml("# Article", { author: "x".repeat(800) }), code("WORKER_QUEUE_FULL"));
  await assert.rejects(limited.renderer.publish("# Article", [
    { format: "html", options: { author: "x".repeat(800) } },
  ]), code("WORKER_QUEUE_FULL"));
  assert.equal(limited.workers.length, 0); assert.equal(limited.renderer.pendingBytes, 0);
});

test("document publications expose unsupported HTML metadata without returning partial artifacts", async t => {
  const { renderer } = documentWorker(t, { htmlAuthorSupport: false });
  await assert.rejects(renderer.publish("# Article", [
    { format: "pdf" }, { format: "html", options: { author } },
  ]), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(renderer.pendingOperations, 0); assert.equal(renderer.pendingBytes, 0);
  assert.equal(renderer.disposed, true);
  const fresh = documentWorker(t, { htmlAuthorSupport: false }).renderer;
  assert.equal((await fresh.renderHtml("# Article")).format, "html");
  assert.equal(decode(await fresh.renderHtml("---\nauthor: Hidden\n---\n# Article", { author: "" })).options.author, "");
});

test("mixed document batches preserve each HTML author beside EPUB and PDF metadata", async t => {
  const factory = workerFactory(t, "./tests/document_publication_worker_fixture.mjs");
  const single = { format: "html", source: "# First", options: { author } };
  const edition = { source: "# Second", targets: [
    { format: "html", options: { author: "HTML creator" } },
    { format: "epub", options: { author: "EPUB creator" } },
    { format: "pdf", options: { author: "PDF creator" } },
  ] };
  const stream = renderDocumentBatch([single, edition], { concurrency: 2,
    worker: { workerFactory: factory.create, timeoutMs: 5000 } });
  t.after(() => stream.return());
  const pending = stream.next(); single.options.author = "Later"; edition.targets[0].options.author = "Later too";
  const first = (await pending).value, second = (await stream.next()).value;
  assert.equal(first.status, "fulfilled"); assert.equal(second.status, "fulfilled");
  assert.equal(decode(first.value).options.author, author);
  assert.deepEqual(second.value.outputs.map(value => decode(value).options.author),
    ["HTML creator", "EPUB creator", "PDF creator"]);
  assert.ok(second.value.outputs.every(value => decode(value).source === "# Second"));
  assert.equal((await stream.next()).done, true);
});

test("direct Flow HTML exports preserve explicit emptiness, whitespace and captured batch authors", async () => {
  const f = directFlow(); let release;
  f.renderers.pdf = () => new Promise(resolve => { release = resolve; });
  const api = f.wrap(), settings = { author, title: "Guide" };
  const pending = api.exportDocuments([
    { format: "pdf" }, { format: "html", options: settings },
    { format: "epub", options: { author: "EPUB creator" } },
  ], api.token);
  pending.catch(() => {}); settings.author = "Changed";
  release(f.output("pdf", api.source, {}));
  assert.deepEqual((await pending).map(value => decode(value).options.author), [undefined, author, "EPUB creator"]);
  for (const explicit of ["", "  "])
    assert.equal(decode(await api.exportDocument("html", { author: explicit }, api.token)).options.author, explicit);
  assert.equal(Object.hasOwn(decode(await api.exportDocument("html", {}, api.token)).options, "author"), false);
});

test("Flow HTML author admission preserves the UTF-8 boundary and rejects accessors", async () => {
  const f = directFlow(), api = f.wrap(); let reads = 0;
  const accessor = Object.defineProperty({}, "author", { get() { reads++; return "Never"; } });
  for (const value of [null, 42, {}, "é".repeat(32769), "x".repeat(65537), "broken\ud800"])
    await assert.rejects(api.exportDocument("html", { author: value }, api.token),
      code(typeof value === "string" && value.startsWith("broken") ? "INVALID_UNICODE" : "INVALID_OPTIONS"));
  await assert.rejects(api.exportDocument("html", accessor, api.token), code("INVALID_OPTIONS"));
  assert.equal(reads, 0); assert.equal(f.calls.length, 0);
  const limit = "é".repeat(32768);
  assert.equal(decode(await api.exportDocument("html", { author: limit }, api.token)).options.author, limit);
});

test("Flow worker HTML authors are captured, charged and retained with publication images", async t => {
  const session = await flowWorker(t, { delayMs: 20, image: true });
  const first = session.exportDocument("pdf", {}, session.token); first.catch(() => {});
  const before = session.pendingBytes, repeated = author.repeat(128), settings = { author: repeated };
  const pending = session.exportDocument("html", settings, session.token); pending.catch(() => {});
  assert.ok(session.pendingBytes >= before + 2 * repeated.length); settings.author = "Later";
  await first;
  const output = decode(await pending);
  assert.equal(output.author, repeated); assert.equal(output.markdown, "# Source 😀");
  assert.deepEqual(output.images, [["local.png", [1, 2, 3]]]);
  const targets = [{ format: "html", options: { author: "HTML creator" } },
    { format: "epub", options: { author: "EPUB creator" } }, { format: "pdf", options: { author: "PDF creator" } }];
  const publication = session.exportDocuments(targets, session.token); targets[0].options.author = "Later";
  assert.deepEqual((await publication).map(value => decode(value).author), ["HTML creator", "EPUB creator", "PDF creator"]);
  for (const explicit of ["", "  "])
    assert.equal(decode(await session.exportDocument("html", { author: explicit }, session.token)).author, explicit);
  assert.equal(Object.hasOwn(decode(await session.exportDocument("html", {}, session.token)), "author"), false);
  assert.equal(session.pendingOperations, 0); assert.equal(session.pendingBytes, 0);
});

test("Flow workers retain their session after unsupported HTML author metadata", async t => {
  const session = await flowWorker(t, { htmlAuthorSupport: false });
  const expected = error => error.code === "UNSUPPORTED_WASM_PACKAGE" && /HTML author/.test(error.message);
  await assert.rejects(session.exportDocument("html", { author }, session.token), expected);
  await assert.rejects(session.exportDocuments([
    { format: "epub" }, { format: "html", options: { author } },
  ], session.token), expected);
  await assert.rejects(session.exportDocument("html", { author: "é".repeat(32769) }, session.token), code("INVALID_OPTIONS"));
  assert.equal(decode(await session.exportDocument("html", { author: "" }, session.token)).author, "");
  assert.equal((await session.exportDocument("html", {}, session.token)).format, "html");
  assert.equal(session.disposed, false); assert.equal(session.pendingOperations, 0); assert.equal(session.pendingBytes, 0);
});
