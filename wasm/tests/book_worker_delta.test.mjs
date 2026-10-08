import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { serialize } from "node:v8";
import { prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker, applyBookSourceUpdate } from "../book_worker.mjs";
import { nativeFixture } from "./book_worker_delta_fixture.mjs";

const MiB = 1024 * 1024;
const files = [{ path: "guide/one.md", source: "# One" }, { path: "two.md", source: "# Two" }];
const options = () => ({ title: "Manual", author: "Author", lang: "fr", customCss: "p{line-height:1.5}",
  font: "serif", fontScale: 1.1, darkMode: "disabled", toc: true, pageNumbers: true,
  typography: "optimal-pagination", baseFontSize: 12, headingScale: 1.3, tableFontSize: 10,
  tocDepth: 3, codeLineNumbers: true, metadataEpochSeconds: 0, htmlFontFormat: "ttf",
  page: { size: "a4", margins: 24 }, running: { footer: { center: "{page}" }, skipFirstPage: true },
  includeSources: [{ path: "parts/shared.txt", source: "Shared text" }],
  images: [{ destination: "guide/picture.svg", bytes: new Uint8Array([1, 2, 3]) }],
  fontAssets: [{ slot: "body-regular", weight: 550, bytes: new Uint8Array([4, 5, 6]) }] });
const decode = result => JSON.parse(new TextDecoder().decode(result.bytes));
const code = wanted => error => error?.code === wanted;
const tick = () => new Promise(resolve => setImmediate(resolve));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(fn) {
  const deadline = Date.now() + 4000;
  while (!fn()) { assert(Date.now() < deadline, "worker did not reach the expected state"); await sleep(5); }
}
function threaded(t, config = {}, native = {}, alter = value => value) {
  const counters = new Int32Array(new SharedArrayBuffer(24)), endpoints = [];
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 5000, ...config, workerFactory() {
    const thread = new Worker(new URL("./book_worker_delta_fixture.mjs", import.meta.url), {
      workerData: { counters: counters.buffer, ...native } });
    const worker = new EventTarget(); worker.messages = []; worker.terminated = 0;
    thread.on("message", value => worker.dispatchEvent(new MessageEvent("message", { data: alter(value) })));
    for (const kind of ["error", "messageerror"]) thread.on(kind, () => worker.dispatchEvent(new Event(kind)));
    worker.postMessage = (value, transfer) => {
      worker.messages.push({ value: structuredClone(value), bytes: serialize(value).length,
        transferredBytes: transfer.reduce((n, buffer) => n + buffer.byteLength, 0) });
      thread.postMessage(value, transfer);
    };
    worker.terminate = () => { worker.terminated++; return thread.terminate(); };
    endpoints.push(worker); return worker;
  } });
  t.after(() => client.dispose());
  return { client, endpoints, counters,
    update: (changes, format = "pdf", request) => client.renderSourceUpdate(changes, format,
      { expectedRevision: client.retainedInputRevision }, request) };
}
function direct(engine = nativeFixture().engine) {
  let handle; const replies = [];
  installBookWorker({ addEventListener(_, fn) { handle = fn; }, postMessage(value) { replies.push(value); } }, engine);
  const send = data => handle({ data: { schemaVersion: 1, maxOutputBytes: 128 * MiB, ...data } });
  return { replies, send };
}

test("chapter and include deltas preserve all rendering options/resources across formats", async t => {
  const f = threaded(t), opts = options();
  await f.client.render(files, "pdf", opts);
  const changes = [{ path: files[0].path, source: "\ufeff# Révisé 😀\r\n" },
    { path: "parts/shared.txt", source: "Updated include\r" }];
  const result = await f.update(changes, "epub");
  const value = decode(result);
  assert.deepEqual(value.files, [changes[0], files[1]]);
  assert.deepEqual(value.includes, [changes[1]]);
  assert.equal(result.retainedInputRevision, 2);
  assert.equal(f.client.retainedInputRevision, 2);
  assert.equal(result.sourceLength, Buffer.byteLength(changes[0].source + files[1].source + changes[1].source));
  const fresh = threaded(t);
  const expected = await fresh.client.render(value.files, "epub", { ...opts, includeSources: value.includes });
  assert.deepEqual(result.bytes, expected.bytes, "same production facade and native ABI inputs as a fresh export");
  assert.equal(Atomics.load(f.counters, 0), 1); assert.equal(Atomics.load(f.counters, 1), 1);
  assert.equal(f.endpoints.length, 1);
  const packet = f.endpoints[0].messages[1];
  assert.deepEqual(packet.value.sourceUpdate, { expectedRevision: 1, files: changes });
  assert(!Object.hasOwn(packet.value, "files") && !Object.hasOwn(packet.value, "options"));
  assert.equal(packet.transferredBytes, 0);
});

test("unchanged source exports another format without resending text or updating native source", async t => {
  const f = threaded(t); await f.client.render(files, "site", options());
  await f.update([], "preview"); await f.update([], "pdf");
  assert.equal(Atomics.load(f.counters, 0), 1); assert.equal(Atomics.load(f.counters, 1), 0);
  assert.equal(Atomics.load(f.counters, 2), 3);
  assert.equal(f.client.retainedInputRevision, 3);
  assert.deepEqual(f.endpoints[0].messages[2].value.sourceUpdate.files, []);
});

test("large authorized image/font payloads cross the thread boundary once, not on source edits", async t => {
  const f = threaded(t), opts = options();
  opts.images[0].bytes = new Uint8Array(8 * MiB).fill(51);
  opts.fontAssets[0].bytes = new Uint8Array(4 * MiB).fill(73);
  const initial = decode(await f.client.render(files, "pdf", opts));
  const changed = decode(await f.update([{ path: files[0].path, source: "Only this source changed" }]));
  const [full, delta] = f.endpoints[0].messages;
  assert.equal(full.transferredBytes, 12 * MiB);
  assert.equal(delta.transferredBytes, 0); assert(delta.bytes < 512);
  assert.deepEqual(changed.images, initial.images); assert.deepEqual(changed.fonts, initial.fonts);
  assert.equal(opts.images[0].bytes.byteLength, 8 * MiB); assert.equal(opts.images[0].bytes[0], 51);
  assert.equal(opts.fontAssets[0].bytes.byteLength, 4 * MiB);
  t.diagnostic(`Fixture transfer bytes: full=${full.transferredBytes}, delta=${delta.transferredBytes}; Node serialized delta=${delta.bytes} bytes.`);
});

test("source update captures caller strings before asynchronous worker execution", async t => {
  const f = threaded(t); await f.client.render(files, "site", options());
  const changes = [{ path: files[1].path, source: "Submitted" }];
  const pending = f.update(changes);
  changes[0].source = "Too late"; changes[0].path = "other.md"; changes.push({ path: "bad.md", source: "x" });
  assert.equal(decode(await pending).files[1].source, "Submitted");
});

test("stale capture revisions reject locally without destroying the current idle capture", async t => {
  const f = threaded(t); await f.client.render(files, "site", options());
  const previous = f.client.retainedInputRevision;
  await f.update([{ path: files[1].path, source: "New" }]);
  await assert.rejects(f.client.renderSourceUpdate([], "pdf", { expectedRevision: previous }), code("STALE_BOOK_CAPTURE"));
  assert.equal(f.endpoints[0].messages.length, 2); assert.equal(f.endpoints[0].terminated, 0);
  assert.equal(f.client.retainedInputRevision, 2);
  assert.equal(decode(await f.update([])).files[1].source, "New");
});

test("malformed deltas cannot smuggle settings/resources, duplicate paths, getters or iterators", async t => {
  const f = threaded(t); await f.client.render(files, "site", options());
  let read = false;
  for (const [changes, config] of [
    [[{ path: files[0].path, source: "x", images: [] }], { expectedRevision: 1 }],
    [[{ path: files[0].path, get source() { read = true; return "x"; } }], { expectedRevision: 1 }],
    [new Array(1), { expectedRevision: 1 }],
    [[], { get expectedRevision() { read = true; return 1; } }],
    [[], { expectedRevision: 1, title: "injected" }],
    [[], { expectedRevision: -1 }], [[], { expectedRevision: 1.5 }], [[], {}],
    [[], { expectedRevision: Number.MAX_SAFE_INTEGER + 1 }],
    [[{ path: files[0].path, source: 1 }], { expectedRevision: 1 }],
    [[{ path: files[0].path, source: "a" }, { path: files[0].path, source: "b" }], { expectedRevision: 1 }],
    [[{ path: files[0].path, source: "\ud800" }], { expectedRevision: 1 }],
    [new Array(4097), { expectedRevision: 1 }],
  ]) await assert.rejects(f.client.renderSourceUpdate(changes, "pdf", config));
  assert.equal(read, false); assert.equal(f.endpoints[0].messages.length, 1);
  const changes = [{ path: files[0].path, source: "Exact indexed input" }];
  changes[Symbol.iterator] = () => { throw Error("must not use caller iterator"); };
  assert.equal(decode(await f.update(changes)).files[0].source, "Exact indexed input");
});

test("unknown and renamed paths reject before native mutation; recovery requires a complete capture", async t => {
  const f = threaded(t); await f.client.render(files, "site", options());
  await assert.rejects(f.update([{ path: "renamed.md", source: "new" }]), code("UNKNOWN_BOOK_SOURCE"));
  assert.equal(Atomics.load(f.counters, 1), 0); assert.equal(Atomics.load(f.counters, 3), 1);
  assert.equal(f.client.retainedInputRevision, null); assert.equal(f.endpoints[0].terminated, 1);
  const latest = [{ path: "renamed.md", source: "Full source replacement" }];
  assert.deepEqual(decode(await f.client.render(latest, "pdf", options())).files, latest);
});

test("native rejection, invalid receipts and post-update rendering failure never fall back silently", async t => {
  for (const [source, reason] of [["REJECT", "FIXTURE_REJECT"], ["BAD_RECEIPT", "INVALID_BOOK_UPDATE_REPORT"],
    ["RENDER_FAIL", "FIXTURE_RENDER"]]) {
    const f = threaded(t); await f.client.render(files, "site", options());
    await assert.rejects(f.update([{ path: files[0].path, source }]), code(reason));
    assert.equal(Atomics.load(f.counters, 0), 1); assert.equal(Atomics.load(f.counters, 3), 1);
    assert.equal(f.client.retainedInputRevision, null); assert.equal(f.endpoints[0].terminated, 1);
  }
});

test("reconstruction validates total resulting UTF-8 source bytes and allows shrink-funded growth", () => {
  const opts = options(); opts.includeSources = [{ path: "i", source: "x".repeat(32 * MiB - 16) }];
  const original = prepareBookInput([{ path: "a", source: "a".repeat(32 * MiB - 16) }], opts);
  const before = original.files[0].source;
  assert.throws(() => applyBookSourceUpdate(original, { expectedRevision: 1,
    files: [{ path: "a", source: "b".repeat(32 * MiB + 100) }] }), /64 MiB|budget/);
  assert.equal(original.files[0].source, before);
  const next = applyBookSourceUpdate(original, { expectedRevision: 1,
    files: [{ path: "a", source: "b".repeat(32 * MiB + 100) }, { path: "i", source: "" }] });
  assert.equal(next.options.includeSources[0].source, "");
  assert.equal(next.options.images, original.options.images);
  assert.equal(next.options.fontAssets, original.options.fontAssets);
  assert.equal(original.options.includeSources[0].source.length, 32 * MiB - 16);
});

test("malformed raw worker envelopes cannot combine deltas with full configuration or change retention mode", async () => {
  for (const modify of [
    d => d.files = files, d => d.options = { images: [] },
    d => { delete d.retainBook; d.retainPreview = true; d.format = "preview"; },
    d => d.sourceUpdate.extra = [], d => d.sourceUpdate.expectedRevision = 0,
    d => { delete d.retainBook; }, d => d.id = 1,
  ]) {
    const native = nativeFixture(), f = direct(native.engine);
    await f.send({ id: 1, format: "site", files, options: options(), retainBook: true });
    const update = { id: 2, format: "pdf", retainBook: true, sourceUpdate: { expectedRevision: 1, files: [] } };
    modify(update); await f.send(update);
    assert(f.replies.at(-1).error); assert.equal(native.stats.exports, 1); assert.equal(native.stats.updates, 0);
    assert.equal(native.stats.frees, 1);
  }
});

test("new client keeps legacy worker full-snapshot rendering but refuses unsupported deltas", async t => {
  const f = threaded(t, {}, {}, value => { delete value.retainedInputRevision; return value; });
  await f.client.render(files, "site", options());
  assert.equal(f.client.retainedInputRevision, null); assert(f.client.hasRetainedBook);
  await assert.rejects(f.client.renderSourceUpdate([], "pdf", { expectedRevision: 1 }), code("UNSUPPORTED_BOOK_SOURCE_DELTA"));
  await f.client.render(files, "pdf", options());
  assert.equal(f.endpoints.length, 1); assert.equal(f.endpoints[0].messages.length, 2);
});

test("old native source-edit APIs reconstruct from current worker capture without asset retransmission", async t => {
  const f = threaded(t, {}, { old: true }); await f.client.render(files, "site", options());
  const result = await f.update([{ path: files[0].path, source: "Fresh even on older native ABI" }]);
  assert.equal(decode(result).files[0].source, "Fresh even on older native ABI");
  assert.equal(Atomics.load(f.counters, 0), 2); assert.equal(Atomics.load(f.counters, 3), 1);
  assert.equal(f.endpoints[0].messages[1].transferredBytes, 0);
});

test("missing or incorrect post-delta acknowledgement retires the endpoint", async t => {
  for (const revision of [undefined, 1, "2", -1]) {
    const f = threaded(t, {}, {}, value => value.id === 2 ? { ...value, retainedInputRevision: revision } : value);
    await f.client.render(files, "site", options());
    await assert.rejects(f.update([]), code("WORKER_PROTOCOL_ERROR"));
    assert.equal(f.client.retainedInputRevision, null); assert.equal(f.endpoints[0].terminated, 1);
  }
});

test("capture expiry and cancellation cannot resurrect an old worker or silently reuse a new one", async t => {
  for (const expiry of [false, true]) {
    const f = threaded(t, expiry ? { idleTimeoutMs: 40 } : {});
    await f.client.render(files, "site", options());
    const revision = f.client.retainedInputRevision;
    if (expiry) await waitFor(() => !f.client.hasRetainedBook); else f.client.cancel();
    await assert.rejects(f.client.renderSourceUpdate([], "pdf", { expectedRevision: revision }), code("BOOK_CAPTURE_EXPIRED"));
    assert.equal(f.endpoints.length, 1);
    await f.client.render(files, "pdf", options());
    await assert.rejects(f.client.renderSourceUpdate([], "pdf", { expectedRevision: revision }), code("STALE_BOOK_CAPTURE"));
    assert.equal(f.endpoints.length, 2);
  }
});

test("abort and timeout physically terminate synchronous work reached through a delta", async t => {
  for (const timeout of [false, true]) {
    const f = threaded(t, timeout ? { timeoutMs: 1000 } : {});
    await f.client.render(files, "site", options());
    const abort = new AbortController();
    const pending = f.update([{ path: files[0].path, source: "BLOCK" }], "pdf", { signal: abort.signal });
    const rejected = assert.rejects(pending, code(timeout ? "EXPORT_TIMEOUT" : "EXPORT_CANCELLED"));
    assert.equal(f.client.retainedInputRevision, null, "in-flight revisions are not offered as idle captures");
    await waitFor(() => Atomics.load(f.counters, 4) === 1);
    await assert.rejects(f.client.render(files, "epub"), code("BOOK_BUSY"));
    if (!timeout) abort.abort();
    await rejected; assert.equal(f.endpoints[0].terminated, 1);
    assert.equal(f.client.retainedInputRevision, null);
    await f.client.render(files, "site", options()); assert.equal(f.endpoints.length, 2);
  }
});

test("pre-aborted updates preserve the idle capture and do not post messages", async t => {
  const f = threaded(t); await f.client.render(files, "pdf", options());
  const abort = new AbortController(); abort.abort();
  await assert.rejects(f.update([], "pdf", { signal: abort.signal }), code("EXPORT_CANCELLED"));
  assert.equal(f.client.retainedInputRevision, 1); assert.equal(f.endpoints[0].messages.length, 1);
});

test("Proxy reentry during delta admission cannot start a competing job or use a cancelled endpoint", async t => {
  const f = threaded(t); await f.client.render(files, "pdf", options());
  let competing;
  const update = new Proxy({ expectedRevision: 1 }, { ownKeys(target) {
    competing = f.client.render(files, "site"); f.client.cancel(); return Reflect.ownKeys(target);
  } });
  await assert.rejects(f.client.renderSourceUpdate([], "pdf", update), code("EXPORT_CANCELLED"));
  await assert.rejects(competing, code("BOOK_BUSY"));
  assert.equal(f.endpoints[0].messages.length, 1); assert.equal(f.endpoints[0].terminated, 1);
  assert.equal(f.client.retainedInputRevision, null);
});

test("failed listener detachment cannot send a delta to a newly constructed empty worker", async t => {
  const f = threaded(t); await f.client.render(files, "pdf", options());
  f.endpoints[0].removeEventListener = () => { throw Error("adapter failed"); };
  await assert.rejects(f.update([]), code("BOOK_CAPTURE_EXPIRED"));
  assert.equal(f.endpoints.length, 1); assert.equal(f.endpoints[0].terminated, 1);
});

test("full resource replacement/revocation advances capture and subsequent deltas cannot restore old grants", async t => {
  const f = threaded(t); await f.client.render(files, "pdf", options());
  const revision = f.client.retainedInputRevision;
  const revoked = { ...options(), images: [], fontAssets: [], title: "Revoked" };
  await f.client.render(files, "site", revoked);
  await assert.rejects(f.client.renderSourceUpdate([], "pdf", { expectedRevision: revision }), code("STALE_BOOK_CAPTURE"));
  const result = decode(await f.update([{ path: files[0].path, source: "After revocation" }]));
  assert.deepEqual(result.images, []); assert.deepEqual(result.fonts, []);
  assert.equal(result.settings.metadata[0], "Revoked"); assert.equal(Atomics.load(f.counters, 0), 2);
});

test("preview-only retention supports deltas but one-shot and source-only checks do not", async t => {
  const preview = threaded(t, { retainBook: false, retainPreview: true });
  await preview.client.render(files, "preview", options());
  assert.equal(decode(await preview.update([{ path: files[1].path, source: "Preview edit" }], "preview")).files[1].source, "Preview edit");
  await assert.rejects(preview.update([], "pdf"), code("UNSUPPORTED_BOOK_SOURCE_DELTA"));
  assert(preview.client.hasRetainedPreview);
  const one = threaded(t, { retainBook: false });
  await assert.rejects(one.client.renderSourceUpdate([], "pdf", { expectedRevision: 1 }), code("UNSUPPORTED_BOOK_SOURCE_DELTA"));
  assert.equal(one.endpoints.length, 0);
  for (const format of ["inspection", "links"]) await assert.rejects(
    preview.client.renderSourceUpdate([], format, { expectedRevision: 1 }), code("UNSUPPORTED_BOOK_SOURCE_DELTA"));
});

test("output limit after source mutation clears both native state and transport revision", async t => {
  const f = threaded(t, { maxOutputBytes: 2048 }); await f.client.render(files, "pdf", options());
  await assert.rejects(f.update([{ path: files[0].path, source: "x".repeat(4096) }]), code("OUTPUT_LIMIT"));
  assert.equal(Atomics.load(f.counters, 1), 1); assert.equal(Atomics.load(f.counters, 3), 1);
  assert.equal(f.client.retainedInputRevision, null);
});

test("overlapping raw requests cannot retire another request's capture or release its busy slot", async () => {
  const native = nativeFixture(); let unblock, held = false;
  const original = native.engine.renderRetainedBook;
  native.engine.renderRetainedBook = async (...args) => {
    const result = await original(...args);
    if (!held && args[0][0].source === "WAIT") { held = true; await new Promise(resolve => { unblock = resolve; }); }
    return result;
  };
  const f = direct(native.engine);
  await f.send({ id: 1, files, format: "pdf", options: options(), retainBook: true });
  const pending = f.send({ id: 2, format: "pdf", retainBook: true,
    sourceUpdate: { expectedRevision: 1, files: [{ path: files[0].path, source: "WAIT" }] } });
  await tick();
  await f.send({ id: 3, format: "pdf", retainBook: true, sourceUpdate: { expectedRevision: 1, files: [] } });
  assert.equal(f.replies.at(-1).error.code, "BOOK_BUSY"); assert.equal(native.stats.frees, 0);
  unblock(); await pending;
  assert.equal(f.replies.at(-1).retainedInputRevision, 2);
  await f.send({ id: 4, format: "epub", retainBook: true, sourceUpdate: { expectedRevision: 2, files: [] } });
  assert.equal(f.replies.at(-1).retainedInputRevision, 4); assert.equal(native.stats.creates, 1);
  native.retained.clear();
});
