// Production admission/facade/worker code with explicit native ABI adapters.
// Rust source refactoring is verified separately by the native core tests.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createBookSourceRename, prepareBookSourceRename, parseBookSourceRename } from "../book_source_rename.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";

const files = [{ path: "one.md", source: "\ufeff# One\r\n\r\n[Two](two.md#end)\r\n{{#include parts/note.md}}\r\n" },
  { path: "two.md", source: "# Two\n\n[One](one.md?view=1#one)\n" }];
const includeSources = [{ path: "parts/note.md", source: "[First](../one.md)\n" }];
const report = {
  schema: "fmd-book-source-rename-v1", oldPath: "one.md", newPath: "guide/one.md",
  files: [{ path: "guide/one.md", source: "\ufeff# One\r\n\r\n[Two](../two.md#end)\r\n{{#include ../parts/note.md}}\r\n" },
    { path: "two.md", source: "# Two\n\n[One](guide/one.md?view=1#one)\n" }],
  includeSources: [{ path: "parts/note.md", source: "[First](../guide/one.md)\n" }],
  changedSourceCount: 3, editCount: 4,
};
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const clone = value => structuredClone(value);
const failure = code => error => error.code === code;
const deferred = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const capture = () => prepareBookSourceRename(files, "one.md", "guide/one.md", { includeSources });

function native(calls, result = report) {
  return function rename(paths, sources, includes, texts, oldPath, newPath, profile, imageDestinations) {
    calls.push({ paths, sources, includes, texts, oldPath, newPath, profile, imageDestinations });
    return JSON.stringify(result);
  };
}

test("native refactoring captures exact source, identities and dialect before initialization, without resource reads", async () => {
  const gate = deferred(), calls = [], local = clone(files), snippets = clone(includeSources);
  const imageDestinations = [" diagram.svg ", "guide/diagram.svg", "/root.svg", "data:image/svg+xml;test"];
  const options = { profile: "commonmark-gfm", includeSources: snippets, imageDestinations };
  for (const key of ["images", "fontAssets", "customCss", "page", "running"]) {
    Object.defineProperty(options, key, { get() { assert.fail("source rename read " + key); } });
  }
  const rename = createBookSourceRename(async () => { await gate.promise; return native(calls); });
  const pending = rename(local, "one.md", "guide/one.md", options);
  local[0].source = "later"; snippets[0].source = "later include"; options.profile = "gfm-plus";
  imageDestinations[0] = "later.svg"; options.imageDestinations = ["replacement.svg"];
  gate.resolve(); const output = await pending;
  assert.deepEqual(calls, [{ paths: ["one.md", "two.md"], sources: files.map(file => file.source),
    includes: ["parts/note.md"], texts: includeSources.map(file => file.source),
    oldPath: "one.md", newPath: "guide/one.md", profile: "commonmark-gfm",
    imageDestinations: ["diagram.svg", "guide/diagram.svg", "/root.svg", "data:image/svg+xml;test"] }]);
  assert(Object.isFrozen(calls[0].imageDestinations));
  assert.deepEqual(output.report, report); assert.deepEqual(output.bytes, encode(report));
  assert.equal(output.sourceLength, [...files, ...includeSources].reduce((n, file) => n + Buffer.byteLength(file.source), 0));
  assert(Object.isFrozen(output.report.files[0])); assert(Object.isFrozen(output.report.includeSources));
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
});

test("unsupported engines and disabled expansion fail explicitly without a compatibility rewrite", async () => {
  for (const raw of [undefined, () => "{}", (a, b, c, d, e, f) => "{}", (a, b, c, d, e, f, g) => "{}"]) {
    await assert.rejects(createBookSourceRename(async () => raw)(files, "one.md", "new.md", { includeSources }),
      failure("UNSUPPORTED_BOOK_SOURCE_RENAME"));
  }
  let loads = 0;
  await assert.rejects(createBookSourceRename(async () => { loads++; })(files, "one.md", "new.md", { expandIncludes: false }),
    failure("UNSUPPORTED_BOOK_RENAME_POLICY"));
  assert.equal(loads, 0);
});

test("image inventory is bounded, immutable and independent of source path spelling", () => {
  const imageDestinations = [" ../literal.svg ", "/root.svg", "a%20b.svg?view#crop", "https://assets.example.test/x.svg"];
  const input = prepareBookSourceRename(files, "one.md", "new.md", { imageDestinations });
  assert.deepEqual(input.options.imageDestinations, imageDestinations.map(key => key.trim()));
  assert(Object.isFrozen(input.options.imageDestinations));
  assert.throws(() => { input.options.imageDestinations[0] = "changed.svg"; }, TypeError);
  assert.throws(() => { input.options.imageDestinations = []; }, TypeError);
  assert.deepEqual(prepareBookSourceRename(files, "one.md", "new.md").options.imageDestinations, []);
  for (const imageDestinations of [null, {}, "diagram.svg", [""], [" \t\n"], [1], ["bad\ud800"],
    ["same.svg", " same.svg "], ["a".repeat(4097)], ["é".repeat(2049)], Array.from({ length: 4097 }, (_, i) => `image${i}`)]) {
    assert.throws(() => prepareBookSourceRename(files, "one.md", "new.md", { imageDestinations }));
  }
  const untrusted = ["first.svg", "second.svg"];
  untrusted[Symbol.iterator] = () => { assert.fail("image-key capture must use bounded array indices"); };
  assert.deepEqual(prepareBookSourceRename(files, "one.md", "new.md", { imageDestinations: untrusted })
    .options.imageDestinations, ["first.svg", "second.svg"]);
});

test("rename admission rejects missing/colliding/noncanonical paths and preserves invalid source", () => {
  for (const value of ["", "/new.md", "./new.md", "a/../new.md", "a//b.md", "a\\b.md", "http:new.md", "a\u0000.md", "a\ud800.md"]) {
    assert.throws(() => prepareBookSourceRename(files, "one.md", value));
  }
  assert.throws(() => prepareBookSourceRename(files, "absent.md", "new.md"), failure("UNKNOWN_BOOK_SOURCE"));
  assert.throws(() => prepareBookSourceRename(files, "one.md", "two.md"), failure("DUPLICATE_BOOK_SOURCE"));
  assert.throws(() => prepareBookSourceRename(files, "one.md", "new.md", { includeSources: [{ path: "one.md", source: "shadow" }] }),
    failure("DUPLICATE_BOOK_SOURCE"));
  assert.throws(() => prepareBookSourceRename([{ path: "one.md", source: "bad\ud800" }], "one.md", "new.md"));
});

test("receipt validation binds ordered sources, roles, endpoints, exact counts and byte budgets", () => {
  assert.deepEqual(parseBookSourceRename(encode(report), capture()), report);
  const bad = [
    r => { r.schema = "other"; }, r => { r.oldPath = "two.md"; }, r => { r.newPath = "other.md"; },
    r => { r.files.reverse(); }, r => { r.files.pop(); }, r => { r.files.push({ path: "extra.md", source: "" }); },
    r => { r.includeSources = []; }, r => { r.files[0].path = "one.md"; },
    r => { r.images = []; }, r => { r.files[0].role = "include"; }, r => { r.files[0].source = "bad\ud800"; },
    r => { r.changedSourceCount = 1; }, r => { r.editCount = 0; }, r => { r.editCount = -1; },
    r => { r.editCount = 1; }, r => { r.editCount = 16385; },
  ];
  for (const change of bad) { const value = clone(report); change(value);
    assert.throws(() => parseBookSourceRename(encode(value), capture()), failure("INVALID_BOOK_SOURCE_RENAME")); }
  for (const bytes of [new Uint8Array(), Uint8Array.of(0xff), new TextEncoder().encode("{")]) {
    assert.throws(() => parseBookSourceRename(bytes, capture()), failure("INVALID_BOOK_SOURCE_RENAME"));
  }
});

test("no-op receipts are exact and path-only renames count one source without inventing text edits", () => {
  const input = prepareBookSourceRename([{ path: "a.md", source: "# A\r\n" }], "a.md", "a.md");
  const value = { schema: report.schema, oldPath: "a.md", newPath: "a.md", files: input.files,
    includeSources: [], changedSourceCount: 0, editCount: 0 };
  assert.deepEqual(parseBookSourceRename(encode(value), input), value);
  assert.throws(() => parseBookSourceRename(encode({ ...value, files: [{ path: "a.md", source: "rewritten" }],
    changedSourceCount: 1, editCount: 1 }), input), failure("INVALID_BOOK_SOURCE_RENAME"));
  input.newPath = "b.md";
  assert.equal(parseBookSourceRename(encode({ ...value, newPath: "b.md", files: [{ path: "b.md", source: "# A\r\n" }],
    changedSourceCount: 1 }), input).editCount, 0);
});

function transport(engine) {
  const workers = [], requests = [];
  const factory = () => {
    const worker = new EventTarget(), scope = new EventTarget();
    worker.terminated = false; worker.terminate = () => { worker.terminated = true; };
    worker.reply = value => worker.dispatchEvent(new MessageEvent("message", { data: value }));
    worker.postMessage = (value, transfer = []) => { const data = structuredClone(value, { transfer }); requests.push(data);
      if (engine) queueMicrotask(() => scope.dispatchEvent(new MessageEvent("message", { data }))); };
    scope.postMessage = (value, transfer = []) => { const data = structuredClone(value, { transfer });
      if (!worker.terminated) queueMicrotask(() => worker.reply(data)); };
    if (engine) installBookWorker(scope, engine); workers.push(worker); return worker;
  };
  return { factory, workers, requests };
}

test("worker rename is one-shot source-only and returns the native transaction without publication", async () => {
  const calls = [], endpoint = transport({ renameBookSource: createBookSourceRename(async () => native(calls)) });
  const worker = createBookWorkerClient({ workerFactory: endpoint.factory, retainBook: true });
  const imageDestinations = ["diagram.svg", "guide/diagram.svg"];
  const options = { includeSources, profile: "gfm-plus", imageDestinations };
  for (const key of ["images", "fontAssets", "customCss", "page", "running"]) {
    Object.defineProperty(options, key, { get() { assert.fail(key + " was read"); } });
  }
  const pending = worker.renameSource(files, "one.md", "guide/one.md", options);
  imageDestinations[0] = "changed-after-capture.svg";
  const result = await pending;
  assert.deepEqual(result.report, report); assert.equal(result.format, "book-source-rename");
  assert.equal(calls.length, 1); assert.equal(calls[0].profile, "gfm-plus");
  assert.deepEqual(calls[0].imageDestinations, ["diagram.svg", "guide/diagram.svg"]);
  assert(Object.isFrozen(calls[0].imageDestinations));
  assert.deepEqual(endpoint.requests[0].options.imageDestinations, ["diagram.svg", "guide/diagram.svg"]);
  assert.deepEqual(endpoint.requests[0].options.images, []); assert.deepEqual(endpoint.requests[0].options.fontAssets, []);
  assert.equal(endpoint.requests[0].retainBook, undefined); assert.equal(worker.hasRetainedBook, false);
  assert(endpoint.workers[0].terminated); worker.dispose();
});

test("worker cancellation retires a plan physically and late replies cannot replace a fresh rename", async () => {
  const endpoint = transport(), worker = createBookWorkerClient({ workerFactory: endpoint.factory });
  const first = worker.renameSource(files, "one.md", "guide/one.md", { includeSources });
  await assert.rejects(worker.renameSource(files, "one.md", "guide/one.md", { includeSources }), failure("BOOK_BUSY"));
  worker.cancel(); await assert.rejects(first, failure("EXPORT_CANCELLED"));
  const next = worker.renameSource(files, "one.md", "guide/one.md", { includeSources });
  const envelope = { schemaVersion: 1, id: 1, format: "source-rename", bytes: encode(report),
    sourceLength: [...files, ...includeSources].reduce((n, file) => n + Buffer.byteLength(file.source), 0) };
  endpoint.workers[0].reply(envelope); assert(worker.busy);
  endpoint.workers[1].reply({ ...envelope, id: 2 });
  assert.deepEqual((await next).report, report); assert(endpoint.workers.every(value => value.terminated)); worker.dispose();
});

test("worker rejects wrong input sizes, forged paths and unknown native capability", async () => {
  for (const change of [r => { r.sourceLength = 0; }, r => { r.bytes = encode({ ...report, newPath: "forged.md" }); }]) {
    const endpoint = transport(), worker = createBookWorkerClient({ workerFactory: endpoint.factory });
    const pending = worker.renameSource(files, "one.md", "guide/one.md", { includeSources });
    const value = { schemaVersion: 1, id: 1, format: "source-rename", bytes: encode(report),
      sourceLength: [...files, ...includeSources].reduce((n, file) => n + Buffer.byteLength(file.source), 0) };
    change(value); endpoint.workers[0].reply(value); await assert.rejects(pending); assert(!worker.busy); worker.dispose();
  }
  const endpoint = transport({}), worker = createBookWorkerClient({ workerFactory: endpoint.factory });
  await assert.rejects(worker.renameSource(files, "one.md", "guide/one.md", { includeSources }), failure("UNSUPPORTED_BOOK_SOURCE_RENAME"));
  await assert.rejects(worker.render(files, "pdf", {}, { sourceRename: { oldPath: "one.md", newPath: "new.md" } }), failure("INVALID_OPTIONS"));
  worker.dispose();
});

test("rename timeout releases its private worker and never acknowledges partial source", async () => {
  const endpoint = transport(), worker = createBookWorkerClient({ workerFactory: endpoint.factory, timeoutMs: 5 });
  await assert.rejects(worker.renameSource(files, "one.md", "guide/one.md", { includeSources }), failure("EXPORT_TIMEOUT"));
  assert(endpoint.workers[0].terminated); assert.equal(worker.busy, false); assert.equal(worker.hasRetainedBook, false);
  worker.dispose();
});

test("source refactoring crosses a real Node worker and retains exact source bytes", async () => {
  const workerUrl = new URL("../book_worker.mjs", import.meta.url).href;
  const facadeUrl = new URL("../book_source_rename.mjs", import.meta.url).href;
  const code = `import {parentPort} from 'node:worker_threads';
    import {installBookWorker} from ${JSON.stringify(workerUrl)};
    import {createBookSourceRename} from ${JSON.stringify(facadeUrl)};
    const report = ${JSON.stringify(report)};
    const scope = {addEventListener(type, fn) {parentPort.on(type, data => fn({data}));},
      postMessage(value, transfer) {parentPort.postMessage(value, transfer);}};
    installBookWorker(scope, {renameBookSource: createBookSourceRename(async () =>
      function(a,b,c,d,e,f,g,h) {if(g !== 'commonmark-gfm') throw Error('Missing dialect');
        if(JSON.stringify(h) !== '["diagram.svg","guide/diagram.svg"]') throw Error('Missing image inventory');
        return JSON.stringify(report);})});`;
  let thread;
  const worker = createBookWorkerClient({ workerFactory() {
    thread = new Worker(new URL("data:text/javascript," + encodeURIComponent(code)), { type: "module" });
    const endpoint = new EventTarget();
    thread.on("message", data => endpoint.dispatchEvent(new MessageEvent("message", { data })));
    thread.on("error", error => endpoint.dispatchEvent(new MessageEvent("error", { data: error.message })));
    endpoint.postMessage = (value, transfer) => thread.postMessage(value, transfer);
    endpoint.terminate = () => thread.terminate(); return endpoint;
  } });
  try {
    const output = await worker.renameSource(files, "one.md", "guide/one.md", { includeSources, profile: "commonmark-gfm",
      imageDestinations: ["diagram.svg", "guide/diagram.svg"] });
    assert.deepEqual(output.report, report); assert.deepEqual(output.bytes, encode(report));
    assert.equal(worker.busy, false);
  } finally { worker.dispose(); await thread?.terminate(); }
});
