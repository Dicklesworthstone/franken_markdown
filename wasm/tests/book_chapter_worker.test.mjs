// Production worker protocol, native-session facade and preview codecs. Native
// output is an explicit ABI double; no WASM/rendering performance is asserted.
import test from "node:test";
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { createBookWorkerClient, installBookWorker, createRetainedBookPreview } from "../book_worker.mjs";
import { prepareBookInput, parseBookChapterPreview } from "../book_session.mjs";
import { renderBookPreview, renderBookSelectedPreview, renderBookSessionChapterPreview, parseBookPreview } from "../book_site_preview.mjs";
import { fixtureEngine } from "./book_chapter_worker_fixture.mjs";

const files = [{ path: "intro.md", source: "Intro" }, { path: "next.md", source: "Next" }];
const parsed = output => parseBookChapterPreview(output.bytes, output.selectedChapter);
const metrics = output => JSON.parse(parsed(output).html);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function threadClient(options = {}, workerData = {}) {
  const endpoints = [];
  const client = createBookWorkerClient({ timeoutMs: 10000, ...options,
    workerFactory() {
      const raw = new Worker(new URL("./book_chapter_worker_fixture.mjs", import.meta.url), { workerData });
      const listeners = new Map(), sent = [];
      const endpoint = { raw, sent, stopped: null,
        postMessage(data, transfer) { sent.push(structuredClone(data)); raw.postMessage(data, transfer); },
        terminate() { endpoint.stopped = raw.terminate(); return endpoint.stopped; },
        addEventListener(type, callback) {
          const wrapped = type === "message" ? data => callback({ data }) : error => callback({ error });
          if (!listeners.has(type)) listeners.set(type, new Map());
          listeners.get(type).set(callback, wrapped); raw.on(type, wrapped);
        },
        removeEventListener(type, callback) {
          const wrapped = listeners.get(type)?.get(callback);
          if (wrapped) raw.off(type, wrapped);
          listeners.get(type)?.delete(callback);
        } };
      endpoints.push(endpoint); return endpoint;
    } });
  return { client, endpoints, async close() {
    client.dispose(); await Promise.all(endpoints.map(endpoint => endpoint.stopped));
  } };
}

// Loopback replies remain explicitly test-owned, so malformed protocol cases
// need not compromise or monkey-patch the production worker implementation.
function replyingClient(reply) {
  let terminated = 0, created = 0;
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 1000,
    workerFactory() {
      created++;
      const events = new Map();
      return { addEventListener(type, fn) { if (!events.has(type)) events.set(type, new Set()); events.get(type).add(fn); },
        removeEventListener(type, fn) { events.get(type)?.delete(fn); },
        terminate() { terminated++; },
        postMessage(request) { queueMicrotask(() => {
          const data = { schemaVersion: 1, id: request.id, format: request.format,
            retainedBook: true, retainedInputRevision: request.id, selectedChapter: request.selectedChapter,
            previewMode: "native-chapter", sourceLength: 9,
            bytes: new TextEncoder().encode(JSON.stringify({ schema: "fmd-book-chapter-preview-v1", selected: 1,
              pages: files.map((file, i) => ({ path: `ch${i}.html`, source: file.path, title: "Test" })), html: "<p>selected</p>" })) };
          reply(data);
          for (const fn of [...events.get("message") ?? []]) fn({ data });
        }); } };
    } });
  return { client, get terminated() { return terminated; }, get created() { return created; } };
}

test("real worker renders one requested chapter and survives session disposal", async () => {
  const f = threadClient();
  try {
    assert.equal(f.client.supportsChapterPreview, true);
    const result = await f.client.render(files, "chapter-preview", {}, { selectedChapter: 1 });
    assert.equal(result.format, "book-chapter-preview");
    assert.equal(result.selectedChapter, 1);
    assert.equal(result.previewMode, "native-chapter");
    assert.equal(result.sourceLength, 9);
    assert.equal(result.mimeType, "application/json");
    assert.equal(metrics(result).source, "Next");
    assert.equal(metrics(result).sites, 0);
    assert.equal(metrics(result).created, 1);
    assert.equal(metrics(result).previews, 1);
    assert.equal(f.client.hasRetainedBook, false);
    assert.deepEqual(new Uint8Array(await result.blob().arrayBuffer()), result.bytes);
  } finally { await f.close(); }
});

test("retained navigation sends no source/asset bytes and does no whole-site rendering", async () => {
  const f = threadClient({ retainPreview: true });
  try {
    const asset = new Uint8Array([1, 2, 3]);
    const first = await f.client.render(files, "chapter-preview", {
      images: [{ destination: "figure.png", bytes: asset }],
    });
    assert.deepEqual([...asset], [1, 2, 3]);
    assert.equal(f.client.hasRetainedPreview, true);
    const second = await f.client.renderSourceUpdate([], "chapter-preview", {
      expectedRevision: first.retainedInputRevision,
    }, { selectedChapter: 1 });
    const work = metrics(second), sent = f.endpoints[0].sent[1];
    assert.equal(f.endpoints.length, 1);
    assert.equal(work.created, 1); assert.equal(work.updates, 0);
    assert.equal(work.previews, 2); assert.equal(work.sites, 0); assert.equal(work.images, 1);
    assert.equal(Object.hasOwn(sent, "files"), false);
    assert.equal(Object.hasOwn(sent, "options"), false);
    assert.deepEqual(sent.sourceUpdate.files, []);
    assert.equal(second.retainedInputRevision, f.client.retainedInputRevision);
  } finally { await f.close(); }
});

test("source-only edits reuse the native session and refresh the selected chapter", async () => {
  const f = threadClient({ retainPreview: true });
  try {
    const first = await f.client.render(files, "chapter-preview", {
      includeSources: [{ path: "note.md", source: "Before" }],
    });
    const next = [{ ...files[0], source: "New intro" }, files[1]];
    const result = await f.client.renderSources(next, "chapter-preview", {
      expectedRevision: first.retainedInputRevision,
      includeSources: [{ path: "note.md", source: "After" }],
    }, { selectedChapter: 0 });
    const work = metrics(result);
    assert.equal(work.source, "New intro"); assert.equal(work.includes[0].source, "After");
    assert.equal(work.created, 1); assert.equal(work.updates, 1); assert.equal(work.revision, 1);
    assert.equal(work.sites, 0);
    assert.deepEqual(f.endpoints[0].sent[1].sourceUpdate.files.map(file => file.path), ["intro.md", "note.md"]);
  } finally { await f.close(); }
});

test("one retained book shares chapter previews with PDF and EPUB publications", async () => {
  const f = threadClient({ retainBook: true });
  try {
    let result = await f.client.render(files, "chapter-preview");
    result = await f.client.renderSourceUpdate([], "pdf", { expectedRevision: result.retainedInputRevision });
    assert.equal(JSON.parse(new TextDecoder().decode(result.bytes)).created, 1);
    assert.equal(f.client.hasRetainedPreview, false);
    result = await f.client.renderSourceUpdate([], "epub", { expectedRevision: result.retainedInputRevision });
    result = await f.client.renderSourceUpdate([], "chapter-preview", { expectedRevision: result.retainedInputRevision }, { selectedChapter: 1 });
    assert.equal(metrics(result).created, 1); assert.equal(metrics(result).sites, 0);
    assert.equal(f.client.hasRetainedPreview, true); assert.equal(f.endpoints.length, 1);
  } finally { await f.close(); }
});

test("configuration changes rebuild; chapter membership changes use native replacement", async () => {
  const f = threadClient({ retainBook: true });
  try {
    await f.client.render(files, "chapter-preview");
    const reordered = await f.client.render([files[1], files[0]], "chapter-preview", {}, { selectedChapter: 1 });
    assert.equal(metrics(reordered).created, 1); assert.equal(metrics(reordered).replacements, 1);
    assert.equal(metrics(reordered).source, "Intro");
    const themed = await f.client.render([files[1], files[0]], "chapter-preview", { darkMode: "disabled" });
    assert.equal(metrics(themed).created, 2); assert.equal(metrics(themed).freed, 1);
  } finally { await f.close(); }
});

test("129-chapter books no longer hit the old whole-site preview chapter ceiling", async () => {
  const f = threadClient();
  try {
    const chapters = Array.from({ length: 129 }, (_, i) => ({ path: `source${i}.md`, source: `Chapter ${i}` }));
    const result = await f.client.render(chapters, "chapter-preview", {}, { selectedChapter: 128 });
    assert.equal(parsed(result).pages.length, 129);
    assert.equal(metrics(result).source, "Chapter 128"); assert.equal(metrics(result).sites, 0);
  } finally { await f.close(); }
});

test("old native packages use the actual bounded compressed ZIP fallback", async () => {
  const f = threadClient({ retainPreview: true }, { legacy: true });
  try {
    const result = await f.client.render(files, "chapter-preview", {}, { selectedChapter: 1 });
    assert.equal(result.previewMode, "site-fallback");
    assert.equal(parsed(result).html, "<p>Legacy 1: Next</p>");
    assert.equal(parsed(result).pages[0].source, "intro.md");
    assert.equal(Object.hasOwn(parsed(result).pages[0], "html"), false);
    const legacy = await f.client.renderSourceUpdate([], "preview", { expectedRevision: result.retainedInputRevision });
    assert.equal(legacy.format, "book-preview");
    assert.equal(parseBookPreview(legacy.bytes).pages[0].html, "<p>Legacy 0: Intro</p>");
  } finally { await f.close(); }
});

test("native failures never retry the larger site path and always dispose one-shot handles", async () => {
  const f = fixtureEngine();
  for (const [source, code] of [["FAIL_NATIVE", "PREVIEW_LIMIT"], ["BAD_REPLY", "INVALID_BOOK_PREVIEW"]]) {
    await assert.rejects(renderBookSelectedPreview(f.book, [{ path: "x.md", source }], {}, 0), { code });
  }
  assert.equal(f.calls.sites, 0); assert.equal(f.calls.created, 2); assert.equal(f.calls.freed, 2);
});

test("invalid selection is rejected before initialization, and stale deltas preserve newer idle state", async () => {
  const f = threadClient({ retainBook: true });
  try {
    for (const selectedChapter of [-1, 4096, 0.5, NaN, Infinity, "1", null, {}, 1n])
      await assert.rejects(f.client.render(files, "chapter-preview", {}, { selectedChapter }), { code: "INVALID_CHAPTER_INDEX" });
    await assert.rejects(f.client.render(files, "pdf", {}, { selectedChapter: 0 }), { code: "INVALID_OPTIONS" });
    assert.equal(f.endpoints.length, 0);
    const first = await f.client.render(files, "chapter-preview");
    await assert.rejects(f.client.renderSourceUpdate([], "chapter-preview", {
      expectedRevision: first.retainedInputRevision,
    }, { selectedChapter: 2 }), { code: "INVALID_CHAPTER_INDEX" });
    assert.equal(f.client.retainedInputRevision, first.retainedInputRevision);
    const next = await f.client.renderSourceUpdate([], "chapter-preview", {
      expectedRevision: first.retainedInputRevision,
    }, { selectedChapter: 1 });
    await assert.rejects(f.client.renderSourceUpdate([], "chapter-preview", {
      expectedRevision: first.retainedInputRevision,
    }), { code: "STALE_BOOK_CAPTURE" });
    assert.equal(f.client.retainedInputRevision, next.retainedInputRevision);
    assert.equal(f.endpoints.length, 1);
  } finally { await f.close(); }
});

test("a failed source transaction retires the worker and permits a fresh capture", async () => {
  const f = threadClient({ retainPreview: true });
  try {
    const first = await f.client.render(files, "chapter-preview");
    await assert.rejects(f.client.renderSourceUpdate([{ path: "intro.md", source: "FAIL_UPDATE" }], "chapter-preview", {
      expectedRevision: first.retainedInputRevision,
    }), { code: "INVALID_INCLUDE" });
    assert.equal(f.client.hasRetainedBook, false);
    const next = await f.client.render(files, "chapter-preview");
    assert.equal(metrics(next).source, "Intro"); assert.equal(f.endpoints.length, 2);
  } finally { await f.close(); }
});

test("cancel and AbortSignal terminate genuinely synchronous work in a real worker thread", async () => {
  const started = new Int32Array(new SharedArrayBuffer(4));
  const f = threadClient({ retainPreview: true }, { started });
  try {
    const controller = new AbortController();
    const promise = f.client.render([{ path: "block.md", source: "BLOCK" }], "chapter-preview", {}, { signal: controller.signal });
    const rejected = assert.rejects(promise, { code: "EXPORT_CANCELLED" });
    const deadline = Date.now() + 8000;
    while (!Atomics.load(started, 0) && Date.now() < deadline) await sleep(5);
    assert.equal(Atomics.load(started, 0), 1, "fixture must enter synchronous work before abort");
    controller.abort(); await rejected; await f.endpoints[0].stopped;
    assert.equal(Atomics.load(started, 0), 1, "terminated computation must not run to completion");
    assert.equal(f.client.hasRetainedBook, false);
    const result = await f.client.render(files, "chapter-preview");
    assert.equal(metrics(result).source, "Intro");
    f.client.cancelPending(); assert.equal(f.client.hasRetainedPreview, true);
    f.client.cancel(); assert.equal(f.client.hasRetainedBook, false);
  } finally { await f.close(); }
});

test("idle expiry invalidates navigation captures instead of silently rebuilding stale source", async () => {
  const f = threadClient({ retainPreview: true, idleTimeoutMs: 20 });
  try {
    const result = await f.client.render(files, "chapter-preview");
    await sleep(60);
    assert.equal(f.client.hasRetainedPreview, false);
    await assert.rejects(f.client.renderSourceUpdate([], "chapter-preview", {
      expectedRevision: result.retainedInputRevision,
    }, { selectedChapter: 1 }), { code: "BOOK_CAPTURE_EXPIRED" });
  } finally { await f.close(); }
});

test("reply index, mode, count, schema and HTML must be verified before retention", async () => {
  const cases = [
    data => { data.selectedChapter = 0; },
    data => { delete data.previewMode; },
    data => { data.previewMode = "optimistic"; },
    data => { data.bytes = new TextEncoder().encode('{"schema":"wrong"}'); },
    data => { const json = JSON.parse(new TextDecoder().decode(data.bytes)); json.selected = 0; data.bytes = new TextEncoder().encode(JSON.stringify(json)); },
    data => { const json = JSON.parse(new TextDecoder().decode(data.bytes)); json.pages.push({ path: "other.html", source: "other.md", title: "Other" }); data.bytes = new TextEncoder().encode(JSON.stringify(json)); },
  ];
  for (const change of cases) {
    const f = replyingClient(change);
    try {
      await assert.rejects(f.client.render(files, "chapter-preview", {}, { selectedChapter: 1 }), error =>
        ["WORKER_PROTOCOL_ERROR", "INVALID_BOOK_PREVIEW"].includes(error.code));
      assert.equal(f.terminated, 1); assert.equal(f.client.hasRetainedBook, false);
    } finally { f.client.dispose(); }
  }
});

test("server rejects invalid indexes before native work and validates new-format replies", async () => {
  const f = fixtureEngine(); let handler; const replies = [];
  installBookWorker({ addEventListener: (_, fn) => { handler = fn; }, postMessage: data => replies.push(data) }, f.engine);
  for (const selectedChapter of [-1, 2, "1"]) {
    await handler({ data: { schemaVersion: 1, id: 1, format: "chapter-preview", files, options: {}, selectedChapter, maxOutputBytes: 1048576 } });
    assert.equal(replies.at(-1).error.code, "INVALID_CHAPTER_INDEX");
  }
  assert.equal(f.calls.created, 0);
  await handler({ data: { schemaVersion: 1, id: 2, format: "chapter-preview", files, options: {}, selectedChapter: 1, maxOutputBytes: 10 } });
  assert.equal(replies.at(-1).error.code, "OUTPUT_LIMIT");
  assert.equal(f.calls.freed, 1);
});

test("preview-only compatibility adapter opts into selected rendering without altering legacy calls", async () => {
  const f = fixtureEngine(), input = prepareBookInput(files);
  const retained = createRetainedBookPreview(f.book, renderBookPreview, renderBookSessionChapterPreview);
  try {
    const first = await retained.render(input.files, input.options, 1);
    assert.equal(parseBookChapterPreview(first.bytes).selected, 1); assert.equal(f.calls.sites, 0);
    const old = await retained.render(input.files, input.options);
    assert.equal(parseBookPreview(old.bytes).pages.length, 2); assert.equal(f.calls.sites, 1);
    assert.equal(f.calls.created, 1);
  } finally { retained.clear(); }
});
