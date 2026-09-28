import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, createRetainedBookPreview, installBookWorker } from "../book_worker.mjs";
import { parseBookPreview, renderBookPreview } from "../book_site_preview.mjs";
import { nativeFixture } from "./book_retained_fixture.mjs";

const files = () => [{ path: "start.md", source: "\ufeff# Start\r\n" }, { path: "end.md", source: "End 日本語\n" }];
const options = () => ({ title: "Manual", page: { size: "a4" }, fontScale: 1.2,
  includeSources: [{ path: "shared.rs", source: "fn main() {}\r\n" }],
  images: [{ destination: "plot.png", bytes: new Uint8Array([1, 2, 3]) }],
  fontAssets: [{ slot: "body-regular", weight: 425, bytes: new Uint8Array([8, 9]) }] });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate) {
  for (let i = 0; i < 200 && !predicate(); i++) await sleep(5);
  assert(predicate(), "expected asynchronous state was not reached");
}
function retained(fixture = nativeFixture(), render = renderBookPreview) {
  const session = createRetainedBookPreview(fixture.engine, render);
  return { ...fixture, session, async render(f = files(), o = options()) {
    const input = prepareBookInput(f, o);
    return session.render(input.files, input.options);
  } };
}
function transport(t, config = {}) {
  const counters = new Int32Array(new SharedArrayBuffer(24)), endpoints = [];
  const client = createBookWorkerClient({ timeoutMs: 5000, retainPreview: true, ...config,
    workerFactory() {
      const worker = new Worker(new URL("./book_retained_fixture.mjs", import.meta.url),
        { workerData: { counters: counters.buffer, old: config.old } });
      const listeners = new Map();
      const endpoint = { worker, terminated: 0, requests: [],
        postMessage(data, transfer) { endpoint.requests.push(structuredClone(data)); worker.postMessage(data, transfer); },
        addEventListener(kind, listener) {
          const bridge = value => listener(kind === "message" ? { data: value } : value);
          listeners.set(listener, bridge); worker.on(kind, bridge);
        },
        removeEventListener(kind, listener) { const bridge = listeners.get(listener); if (bridge) { worker.off(kind, bridge); listeners.delete(listener); } },
        terminate() { endpoint.terminated++; return worker.terminate(); },
      };
      endpoints.push(endpoint); return endpoint;
    },
  });
  t.after(() => client.dispose());
  return { client, endpoints, counters };
}

test("retained native facade updates only changed chapter/include sources and preserves output parity", async () => {
  const f = retained(), input = files(), opts = options();
  await f.render(input, opts);
  input[0].source = "# Revised\r\n"; opts.includeSources[0].source = "shared É😀\n";
  const second = await f.render(input, opts);
  const fresh = await renderBookPreview(f.engine, input, opts);
  assert.deepEqual(second, fresh);
  assert.deepEqual(f.stats.updates, [{ changes: [input[0], opts.includeSources[0]], expected: 0 }]);
  assert.equal(f.stats.creates, 2); // One retained book plus the independent fresh oracle.
  assert.equal(f.stats.frees, 1);
  await f.render(input, opts);
  assert.equal(f.stats.updates.length, 1, "unchanged capture must not update native source");
  assert.equal(f.stats.creates, 2);
  f.session.clear(); assert.equal(f.stats.frees, 2);
});

test("every metadata, geometry, resource and structural change rebuilds rather than reuses stale state", async () => {
  const changes = [
    (f, o) => o.title = "Changed", (f, o) => o.author = "A", (f, o) => o.lang = "de",
    (f, o) => o.customCss = "p{}", (f, o) => o.toc = true, (f, o) => o.pageNumbers = true,
    (f, o) => o.font = "serif", (f, o) => o.darkMode = "disabled", (f, o) => o.fontScale = 1.3,
    (f, o) => o.page = { size: "letter" }, (f, o) => o.images[0].bytes[2] = 4,
    (f, o) => o.images[0].destination = "other.png", (f, o) => o.images = [],
    (f, o) => o.fontAssets[0].bytes[1] = 10, (f, o) => o.fontAssets[0].weight = 426,
    (f, o) => o.fontAssets[0].slot = "body-bold", (f, o) => o.fontAssets = [],
    f => f.reverse(), f => f[0].path = "renamed.md", f => f.pop(),
    (f, o) => o.includeSources[0].path = "renamed.rs", (f, o) => o.includeSources = [],
    (f, o) => { o.includeSources = []; o.expandIncludes = false; },
  ];
  for (const change of changes) {
    const f = retained(), input = files(), opts = options(); await f.render(input, opts);
    change(input, opts); const output = await f.render(input, opts);
    assert.equal(f.stats.creates, 2); assert.equal(f.stats.frees, 1); assert.equal(f.stats.updates.length, 0);
    assert.deepEqual(output, await renderBookPreview(f.engine, input, opts));
    f.session.clear();
  }
});

test("same-length changed resource bytes cannot masquerade as unchanged assets", async () => {
  const f = retained(), input = files(), opts = options();
  const original = await f.render(input, opts);
  opts.images[0].bytes.reverse(); opts.fontAssets[0].bytes.reverse();
  const result = await f.render(input, opts);
  assert.notDeepEqual(result.bytes, original.bytes); assert.equal(f.stats.creates, 2);
  f.session.clear();
});

test("native update rejection and malformed native report retire the cached book", async () => {
  for (const kind of ["failUpdate", "badReport"]) {
    const f = retained(), input = files(); await f.render(input);
    f.stats[kind] = true; input[0].source = "modified";
    await assert.rejects(f.render(input)); assert.equal(f.stats.frees, 1);
    f.stats[kind] = false; await f.render(input); assert.equal(f.stats.creates, 2);
    f.session.clear();
  }
});

test("the real ZIP validator rejects corrupt output and prevents cache reuse", async () => {
  const f = retained(); await f.render(); f.stats.badZip = true;
  await assert.rejects(f.render(), { code: "INVALID_BOOK_PREVIEW" });
  assert.equal(f.stats.frees, 1); f.stats.badZip = false;
  await f.render(); assert.equal(f.stats.creates, 2); f.session.clear();
});

test("older native engines reconstruct on an edit instead of silently using old text", async () => {
  const f = retained(nativeFixture({ old: true })); const input = files();
  await f.render(input); await f.render(input); assert.equal(f.stats.creates, 1);
  input[0].source = "new source";
  const result = await f.render(input); assert.equal(f.stats.creates, 2); assert.equal(f.stats.updates.length, 0);
  assert.match(parseBookPreview(result.bytes).pages[0].html, /new source/); f.session.clear();
});

test("concurrent native admissions reject and release during initialization disposes late handles", async () => {
  const fixture = nativeFixture(); let finish;
  const session = createRetainedBookPreview({ createBook: (...args) => new Promise(resolve => {
    finish = async () => resolve(await fixture.engine.createBook(...args));
  }) }, renderBookPreview);
  const input = prepareBookInput(files(), options()), pending = session.render(input.files, input.options);
  await assert.rejects(session.render(input.files, input.options), { code: "BOOK_BUSY" });
  session.clear(); await finish();
  await assert.rejects(pending, { code: "EXPORT_CANCELLED" }); assert.equal(fixture.stats.frees, 1);
});

test("real workers reuse both transport and native session across source edits", async t => {
  const f = transport(t), input = files(), opts = options();
  const before = opts.images[0].bytes.slice();
  await f.client.render(input, "preview", opts);
  f.client.cancelPending(); // Idle source edits do not retire the native book.
  input[0].source = "new chapter\n"; opts.includeSources[0].source = "new snippet";
  const result = await f.client.render(input, "preview", opts);
  assert.equal(f.endpoints.length, 1); assert.equal(f.counters[0], 1); assert.equal(f.counters[1], 1);
  assert.equal(f.counters[2], 2); assert.equal(f.endpoints[0].terminated, 0);
  assert.match(parseBookPreview(result.bytes).pages[0].html, /new chapter/);
  assert.deepEqual(opts.images[0].bytes, before, "caller bytes were not detached");
  assert.equal(f.endpoints[0].requests[1].retainPreview, true);
});

test("default one-shot transport remains the incumbent and produces the same preview bytes", async t => {
  const reused = transport(t), original = transport(t, { retainPreview: false });
  const input = files(), opts = options();
  for (let i = 0; i < 5; i++) {
    input[0].source += `${i} `;
    const a = await reused.client.render(input, "preview", opts);
    const b = await original.client.render(input, "preview", opts);
    assert.deepEqual(a.bytes, b.bytes);
  }
  assert.equal(reused.endpoints.length, 1); assert.equal(reused.counters[0], 1); assert.equal(reused.counters[1], 4);
  assert.equal(original.endpoints.length, 5); assert.equal(original.counters[0], 5); assert.equal(original.counters[1], 0);
  assert(original.endpoints.every(e => e.terminated === 1));
});

test("cancel, dispose and non-preview operations release idle preview workers", async t => {
  const f = transport(t); await f.client.render(files(), "preview", options());
  f.client.cancel(); assert.equal(f.endpoints[0].terminated, 1);
  await f.client.render(files(), "preview", options());
  await f.client.render(files(), "pdf", options());
  assert.equal(f.endpoints.length, 3); assert.equal(f.endpoints[1].terminated, 1); assert.equal(f.endpoints[2].terminated, 1);
  await f.client.render(files(), "preview", options()); f.client.dispose();
  assert.equal(f.endpoints[3].terminated, 1);
  await assert.rejects(f.client.render(files(), "preview"), { code: "SESSION_DISPOSED" });
});

test("idle expiry releases only the idle endpoint and a subsequent build recreates it", async t => {
  const f = transport(t, { idleTimeoutMs: 20 }); await f.client.render(files(), "preview", options());
  await until(() => f.endpoints[0].terminated === 1);
  await f.client.render(files(), "preview", options()); assert.equal(f.endpoints.length, 2);
});

test("abort terminates synchronous work and obsolete signals cannot cancel a later request", async t => {
  const f = transport(t), old = new AbortController(), input = files();
  await f.client.render(input, "preview", options(), { signal: old.signal });
  input[0].source = "BLOCK_WORKER"; const now = new AbortController();
  const pending = f.client.render(input, "preview", options(), { signal: now.signal });
  await until(() => Atomics.load(f.counters, 4) === 1);
  old.abort(); assert.equal(f.client.busy, true);
  now.abort(); await assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  assert.equal(f.endpoints[0].terminated, 1);
  await f.client.render(files(), "preview", options()); assert.equal(f.endpoints.length, 2);
});

test("invalid source or failed output cannot leave the previously retained worker alive", async t => {
  const f = transport(t); await f.client.render(files(), "preview", options());
  await assert.rejects(f.client.render([{ path: "a", source: "\ud800" }], "preview"));
  assert.equal(f.endpoints[0].terminated, 1);
  const input = files(); input[0].source = "CORRUPT_ZIP";
  await assert.rejects(f.client.render(input, "preview", options()), { code: "INVALID_BOOK_PREVIEW" });
  assert.equal(f.endpoints[1].terminated, 1);
  await f.client.render(files(), "preview", options()); assert.equal(f.endpoints.length, 3);
});

class Endpoint extends EventTarget {
  terminated = 0;
  removeFails = false;
  postMessage(message) { this.message = message; }
  terminate() { this.terminated++; }
  removeEventListener(...args) { if (this.removeFails) throw Error("detach failed"); super.removeEventListener(...args); }
  reply(overrides = {}) { this.dispatchEvent(new MessageEvent("message", { data: {
    schemaVersion: 1, id: this.message.id, format: this.message.format,
    bytes: new Uint8Array([1]), sourceLength: 0, retainedPreview: true, ...overrides,
  } })); }
}

test("retention requires an explicit matching worker acknowledgment", async () => {
  const worker = new Endpoint(), client = createBookWorkerClient({ workerFactory: () => worker, retainPreview: true });
  const pending = client.render(files(), "preview"); worker.reply({ retainedPreview: undefined });
  await assert.rejects(pending, { code: "WORKER_PROTOCOL_ERROR" }); assert.equal(worker.terminated, 1);
  client.dispose();
});

test("idle crashes, unsolicited responses and failed listener detach discard the endpoint", async () => {
  for (const event of ["error", "messageerror", "message", "detach"]) {
    const worker = new Endpoint(), client = createBookWorkerClient({ workerFactory: () => worker, retainPreview: true });
    const pending = client.render(files(), "preview");
    if (event === "detach") worker.removeFails = true;
    worker.reply(); await pending;
    if (event !== "detach") worker.dispatchEvent(new Event(event));
    assert.equal(worker.terminated, 1); client.dispose();
  }
});

test("worker request admission never enables retention for unrelated formats", async () => {
  let listener, result;
  installBookWorker({ addEventListener(_kind, fn) { listener = fn; }, postMessage(value) { result = value; } }, {});
  await listener({ data: { schemaVersion: 1, id: 1, format: "pdf", retainPreview: true, maxOutputBytes: 100 } });
  assert.equal(result.error.code, "UNSUPPORTED_BOOK_PREVIEW");
  await listener({ data: { schemaVersion: 1, id: 2, format: "preview", retainPreview: "true", maxOutputBytes: 100 } });
  assert.equal(result.error.code, "WORKER_PROTOCOL_ERROR");
});

test("retention option and idle deadline admission reject invalid configurations", () => {
  for (const retainPreview of [1, "true", null]) assert.throws(() => createBookWorkerClient({ workerFactory() {}, retainPreview }), { code: "INVALID_OPTIONS" });
  for (const idleTimeoutMs of [0, -1, 0.5, NaN, Infinity, 600001])
    assert.throws(() => createBookWorkerClient({ workerFactory() {}, retainPreview: true, idleTimeoutMs }), { code: "INVALID_OPTIONS" });
});

test("timeout terminates an active retained endpoint and never falls back to UI-thread work", async () => {
  const worker = new Endpoint(), client = createBookWorkerClient({ workerFactory: () => worker,
    retainPreview: true, timeoutMs: 5 });
  await assert.rejects(client.render(files(), "preview"), { code: "EXPORT_TIMEOUT" });
  assert.equal(worker.terminated, 1); assert.equal(client.busy, false); client.dispose();
});

test("reentrant cancellation during input capture releases the claimed idle worker", async () => {
  const worker = new Endpoint(), client = createBookWorkerClient({ workerFactory: () => worker, retainPreview: true });
  const first = client.render(files(), "preview"); worker.reply(); await first;
  const pending = client.render([{ path: "a.md", get source() { client.cancelPending(); return "new"; } }], "preview");
  await assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  assert.equal(worker.terminated, 1); assert.equal(client.busy, false); client.dispose();
});

test("old result cleanup cannot terminate a reentrantly created newer idle worker", async () => {
  const workers = [new Endpoint(), new Endpoint()]; let calls = 0, next;
  const client = createBookWorkerClient({ workerFactory: () => workers[calls++], retainPreview: true });
  const signal = { aborted: false, addEventListener() {}, removeEventListener() {
    next = client.render(files(), "preview"); workers[1].reply();
  } };
  const old = client.render(files(), "preview", {}, { signal }); workers[0].reply();
  await old; await next;
  assert.equal(workers[0].terminated, 1); assert.equal(workers[1].terminated, 0);
  client.dispose(); assert.equal(workers[1].terminated, 1);
});
