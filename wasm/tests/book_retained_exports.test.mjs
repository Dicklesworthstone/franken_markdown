import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { fixture } from "./book_retained_exports_fixture.mjs";

const file = (path, source = `# ${path}`) => ({ path, source });
const roots = [file("one.md", "# One"), file("two.md", "# Two")];
const decode = result => JSON.parse(new TextDecoder().decode(result.bytes));
const code = expected => error => error?.code === expected;
const profile = () => ({ title: "Manual", author: "Writer", lang: "fr", toc: true, pageNumbers: true,
  baseFontSize: 12, typography: "optimal-pagination", metadataEpochSeconds: 0,
  running: { footer: { center: "{page}" } }, page: { size: "a4", margins: 24 },
  images: [{ destination: "figure.svg", bytes: new Uint8Array([1, 2, 3]) }],
  fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([4, 5]), weight: 650 }],
  includeSources: [file("part.txt", "Shared")] });
async function render(f, files = roots, options = profile(), format = "site") {
  const input = prepareBookInput(files, options);
  return f.retained.render(input.files, input.options, format);
}

function threaded(t, config = {}, native = {}, mutate = value => value) {
  const counters = new Int32Array(new SharedArrayBuffer(24)), endpoints = [];
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 5000, ...config,
    workerFactory() {
      const nativeWorker = new Worker(new URL("./book_retained_exports_fixture.mjs", import.meta.url), {
        workerData: { counters: counters.buffer, ...native },
      });
      const listeners = new Map();
      const endpoint = { requests: [], terminated: 0,
        postMessage(value, transfer) { this.requests.push(value); nativeWorker.postMessage(value, transfer); },
        addEventListener(kind, fn) {
          const wrapper = value => fn(kind === "message" ? { data: mutate(value) } : value);
          if (!listeners.has(kind)) listeners.set(kind, new Map());
          listeners.get(kind).set(fn, wrapper); nativeWorker.on(kind, wrapper);
        },
        removeEventListener(kind, fn) {
          const wrapper = listeners.get(kind)?.get(fn);
          if (wrapper) { nativeWorker.off(kind, wrapper); listeners.get(kind).delete(fn); }
        },
        terminate() { this.terminated++; return nativeWorker.terminate(); },
      };
      endpoints.push(endpoint); return endpoint;
    },
  });
  t.after(() => client.dispose());
  return { client, counters, endpoints };
}
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(predicate) {
  const deadline = Date.now() + 4000;
  while (!predicate()) { assert(Date.now() < deadline, "worker did not reach the expected state"); await delay(5); }
}

test("unchanged input exports all formats through one native book with fresh-output parity", async () => {
  const f = fixture(), options = profile();
  for (const format of ["site", "pdf", "epub", "preview", "pdf"]) {
    const result = await render(f, roots, options, format);
    const fresh = fixture();
    const expected = await render(fresh, roots, options, format);
    assert.deepEqual(result.bytes, expected.bytes);
    fresh.retained.clear();
  }
  assert.equal(f.stats.creates, 1);
  assert.equal(f.stats.updates, 0); assert.equal(f.stats.replacements, 0);
  assert.equal(f.stats.frees, 0);
  const pdf = decode(await render(f, roots, options, "pdf"));
  assert.equal(pdf.page[2], 24); assert.equal(pdf.settings.metadata[0], "Manual");
  assert.deepEqual(pdf.images[0].bytes, [1, 2, 3]); assert.equal(pdf.fonts[0].weight, 650);
  f.retained.clear(); assert.equal(f.stats.frees, 1);
});

test("selective edits, collection replacement, and subsequent edits share native revisions", async () => {
  const f = fixture(), options = profile();
  await render(f, roots, options, "pdf");
  const edited = [file("one.md", "First edit"), roots[1]];
  await render(f, edited, { ...options, includeSources: [file("part.txt", "Changed part")] }, "epub");
  assert.deepEqual(f.stats.calls[0], { method: "update", paths: ["one.md", "part.txt"], expected: 0 });
  // Reorder, remove, rename, add and promote/demote in one complete capture.
  const next = [file("part.txt", "Promoted"), roots[1], file("renamed.md", "新章😀")];
  const nextOptions = { ...options, includeSources: [edited[0]] };
  const result = decode(await render(f, next, nextOptions, "site"));
  assert.deepEqual(result.files, next); assert.deepEqual(result.includes, [edited[0]]);
  assert.deepEqual(f.stats.calls[1], { method: "replace", paths: next.map(f => f.path),
    includedPaths: ["one.md"], expected: 1 });
  await render(f, [next[0], next[1], file("renamed.md", "Newest")], nextOptions, "pdf");
  assert.equal(f.stats.calls[2].expected, 2);
  assert.equal(f.stats.creates, 1); assert.equal(f.stats.replacements, 1); assert.equal(f.stats.updates, 2);
  f.retained.clear();
});

test("omitted resources remove the old include capture rather than resurrecting it", async () => {
  const f = fixture();
  await render(f);
  const options = profile(); delete options.includeSources;
  const result = decode(await render(f, roots, options));
  assert.deepEqual(result.includes, []);
  assert.equal(f.stats.creates, 1); assert.equal(f.stats.replacements, 1);
  f.retained.clear();
});

test("old raw binaries reconstruct collection edits even through a newer facade", async () => {
  for (const old of [false, true]) {
    const f = fixture({ noSets: true, old });
    await render(f);
    const next = [file("added.md", "Current")];
    assert.deepEqual(decode(await render(f, next)).files, next);
    assert.equal(f.stats.creates, 2); assert.equal(f.stats.frees, 1);
    await render(f, next);
    assert.equal(f.stats.creates, 2, "no-op needs no old-binary revision access");
    await render(f, [file("added.md", "Later")]);
    assert.equal(f.stats.creates, old ? 3 : 2);
    f.retained.clear();
  }
});

test("settings, paper, images and fonts rebuild only when their exact values change", async () => {
  for (const change of [
    o => { o.title = "Other"; }, o => { o.page.margins = 36; },
    o => { o.running.footer.center = "Page {page}"; }, o => { o.expandIncludes = false; o.includeSources = []; },
    o => { o.images[0].bytes[1] = 8; }, o => { o.fontAssets[0].bytes[0] = 9; },
    o => { o.images = []; }, o => { o.fontAssets = []; },
  ]) {
    const f = fixture(); await render(f); await render(f);
    assert.equal(f.stats.creates, 1);
    const next = profile(); change(next);
    const actual = await render(f, roots, next, "pdf");
    const fresh = fixture(); const expected = await render(fresh, roots, next, "pdf");
    assert.deepEqual(actual.bytes, expected.bytes);
    assert.equal(f.stats.creates, 2); assert.equal(f.stats.frees, 1);
    f.retained.clear(); fresh.retained.clear();
  }
});

test("native rejection and bad success receipts are not hidden by fallback reconstruction", async () => {
  for (const [source, expected] of [["REJECT", "FIXTURE_REJECT"], ["BAD_RECEIPT", "INVALID_BOOK_SOURCE_SET_REPORT"]]) {
    const f = fixture(); await render(f);
    await assert.rejects(render(f, [file("new.md", source)]), code(expected));
    assert.equal(f.stats.creates, 1); assert.equal(f.stats.frees, 1);
    await render(f, [file("recovered.md")]);
    assert.equal(f.stats.creates, 2); f.retained.clear();
  }
});

test("a rendering failure after a committed collection edit retires the capture", async () => {
  const f = fixture(); await render(f);
  await assert.rejects(render(f, [file("new.md", "RENDER_FAIL")], profile(), "pdf"), code("FIXTURE_RENDER"));
  assert.equal(f.stats.replacements, 1); assert.equal(f.stats.frees, 1);
  const result = decode(await render(f, roots));
  assert.deepEqual(result.files, roots); assert.equal(f.stats.creates, 2);
  f.retained.clear();
});

test("clearing during initialization disposes the eventual handle and publishes no output", async () => {
  const f = fixture(); let release;
  const normal = f.engine.createBook;
  const { createRetainedBook } = await import("../book_retained.mjs");
  const retained = createRetainedBook({ createBook: (...args) => new Promise(resolve => {
    release = async () => resolve(await normal(...args));
  }) }, f.preview);
  const input = prepareBookInput(roots, profile());
  const pending = retained.render(input.files, input.options, "pdf");
  await assert.rejects(retained.render(input.files, input.options, "epub"), code("BOOK_BUSY"));
  retained.clear(); await release();
  await assert.rejects(pending, code("EXPORT_CANCELLED"));
  assert.equal(f.stats.creates, 1); assert.equal(f.stats.frees, 1);
});

test("real worker transports reuse one session across preview and all publication formats", async t => {
  const f = threaded(t), options = profile(), originalImage = options.images[0].bytes.slice();
  let saved;
  for (const format of ["preview", "pdf", "epub", "site"]) {
    const output = await f.client.render(roots, format, options);
    assert.equal(output.format, `book-${format}`);
    const payload = format === "preview" ? decode(output).preview : decode(output);
    assert.deepEqual(payload.files, roots);
    assert.equal(payload.settings.metadata[0], "Manual");
    assert.deepEqual(payload.images[0].bytes, [...originalImage]);
    saved = output;
    assert.equal(f.client.hasRetainedBook, true);
    assert.equal(f.client.hasRetainedPreview, format === "preview");
  }
  assert.equal(f.endpoints.length, 1); assert.equal(Atomics.load(f.counters, 0), 1);
  assert(f.endpoints[0].requests.every(r => r.retainBook === true && r.retainPreview === undefined));
  assert.deepEqual(options.images[0].bytes, originalImage, "private transfers never detach caller assets");
  f.client.cancel();
  assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.client.hasRetainedBook, false);
  assert.deepEqual(decode(saved).files, roots, "output remains owned after session retirement");
});

test("real worker collection edits preserve the current capture across format changes", async t => {
  const f = threaded(t); await f.client.render(roots, "pdf", profile());
  const next = [roots[1], file("added.md", "新しい章😀")];
  const options = { ...profile(), includeSources: [file("replacement.txt", "Snippet")] };
  const result = await f.client.render(next, "epub", options);
  assert.deepEqual(decode(result).files, next); assert.deepEqual(decode(result).includes, options.includeSources);
  assert.equal(result.sourceLength, next.concat(options.includeSources).reduce((n, f) => n + Buffer.byteLength(f.source), 0));
  await f.client.render([next[0], file("added.md", "Later")], "site", options);
  assert.equal(f.endpoints.length, 1); assert.equal(Atomics.load(f.counters, 0), 1);
  assert.equal(Atomics.load(f.counters, 2), 1); assert.equal(Atomics.load(f.counters, 1), 1);
});

test("real worker fallback for old WASM still publishes newly selected sources", async t => {
  const f = threaded(t, {}, { old: true }); await f.client.render(roots, "pdf");
  const next = [file("new.md", "Latest")];
  assert.deepEqual(decode(await f.client.render(next, "epub")).files, next);
  assert.equal(f.endpoints.length, 1); assert.equal(Atomics.load(f.counters, 0), 2);
  assert.equal(Atomics.load(f.counters, 3), 1);
});

test("native transaction failure terminates the worker and recovery starts from current input", async t => {
  const f = threaded(t); await f.client.render(roots, "site");
  await assert.rejects(f.client.render([file("new.md", "BAD_RECEIPT")], "pdf"), code("INVALID_BOOK_SOURCE_SET_REPORT"));
  assert.equal(f.client.hasRetainedBook, false); assert.equal(f.endpoints[0].terminated, 1);
  const next = [file("recovered.md")];
  assert.deepEqual(decode(await f.client.render(next, "epub")).files, next);
  assert.equal(f.endpoints.length, 2);
});

test("cancellation kills synchronous work on a warm worker, not just its Promise", async t => {
  const f = threaded(t); await f.client.render(roots, "pdf");
  const pending = f.client.render([file("new.md", "BLOCK")], "epub");
  const rejected = assert.rejects(pending, code("EXPORT_CANCELLED"));
  await waitFor(() => Atomics.load(f.counters, 4) === 1);
  await assert.rejects(f.client.render(roots, "site"), code("BOOK_BUSY"));
  f.client.cancelPending(); await rejected;
  assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.client.hasRetainedBook, false);
  await f.client.render(roots, "pdf"); assert.equal(f.endpoints.length, 2);
});

test("timeout retires a warm session and releases the active slot", async t => {
  const f = threaded(t, { timeoutMs: 1000 }); await f.client.render(roots, "pdf");
  await assert.rejects(f.client.render([file("blocking.md", "BLOCK")], "pdf"), code("EXPORT_TIMEOUT"));
  assert.equal(Atomics.load(f.counters, 4), 1); assert.equal(f.client.busy, false);
  assert.equal(f.endpoints[0].terminated, 1);
});

test("idle expiry and explicit cancellation bound retained lifetime", async t => {
  const f = threaded(t, { idleTimeoutMs: 40 }); await f.client.render(roots, "epub");
  f.client.cancelPending(); assert.equal(f.client.hasRetainedBook, true);
  await waitFor(() => !f.client.hasRetainedBook);
  assert.equal(f.endpoints[0].terminated, 1);
  await f.client.render(roots, "pdf"); f.client.cancel();
  assert.equal(f.endpoints[1].terminated, 1);
});

test("source-only checks remain one-shot and never inspect asset getters", async t => {
  const f = threaded(t); await f.client.render(roots, "site");
  const options = { get images() { throw Error("must not read assets"); }, get fontAssets() { throw Error("must not read fonts"); } };
  await f.client.render(roots, "inspection", options);
  assert.equal(f.client.hasRetainedBook, false);
  assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.endpoints[1].terminated, 1);
  assert.equal(f.endpoints[1].requests[0].retainBook, undefined);
  await f.client.render(roots, "links", options); assert.equal(f.endpoints[2].terminated, 1);
});

test("old preview-only retention and default one-shot behavior are unchanged", async t => {
  const f = threaded(t, { retainBook: false, retainPreview: true });
  await f.client.render(roots, "preview"); await f.client.render(roots, "preview");
  assert.equal(f.endpoints.length, 1); assert.equal(f.client.hasRetainedPreview, true);
  await f.client.render(roots, "pdf"); assert.equal(f.endpoints.length, 2);
  assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.endpoints[1].terminated, 1);
  const once = threaded(t, { retainBook: false });
  await once.client.render(roots, "pdf"); await once.client.render(roots, "epub");
  assert.equal(once.endpoints.length, 2); assert(once.endpoints.every(w => w.terminated === 1));
});

test("missing retention acknowledgement fails instead of caching an incompatible endpoint", async t => {
  const f = threaded(t, {}, {}, data => ({ ...data, retainedBook: undefined }));
  await assert.rejects(f.client.render(roots, "pdf"), code("WORKER_PROTOCOL_ERROR"));
  assert.equal(f.client.hasRetainedBook, false); assert.equal(f.endpoints[0].terminated, 1);
});

test("output-limit failure after native source replacement releases all retained state", async t => {
  const f = threaded(t, { maxOutputBytes: 1000 }); await f.client.render(roots, "pdf");
  await assert.rejects(f.client.render([file("new.md", "x".repeat(2000))], "epub"), code("OUTPUT_LIMIT"));
  assert.equal(Atomics.load(f.counters, 2), 1); assert.equal(Atomics.load(f.counters, 3), 1);
  assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.client.hasRetainedBook, false);
});

test("handler rejects unsupported, contradictory and non-boolean retention requests before loading", async () => {
  let handler; const replies = []; let calls = 0;
  installBookWorker({ addEventListener(_, fn) { handler = fn; }, postMessage(value) { replies.push(value); } }, {
    createBook() { calls++; },
  });
  for (const flags of [{ retainBook: "yes" }, { retainBook: true, retainPreview: true }, { retainBook: true }]) {
    await handler({ data: { schemaVersion: 1, id: 1, format: "pdf", files: roots, options: {}, maxOutputBytes: 4096, ...flags } });
    assert(replies.at(-1).error);
  }
  assert.equal(calls, 0);
  for (const retainBook of [1, null, "true"]) assert.throws(() => createBookWorkerClient({ workerFactory() {}, retainBook }), code("INVALID_OPTIONS"));
});
