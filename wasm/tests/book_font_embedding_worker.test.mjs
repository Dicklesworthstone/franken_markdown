// Production session/worker/cache/preview/coordinator modules run unchanged.
// Native rendering is explicitly doubled; these are protocol and ownership
// tests, not generated-WASM execution or browser font-shaping evidence.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { bookFontEmbeddingReceipt, prepareBookInput } from "../book_session.mjs";
import { createRetainedBook, createCachedBookRenderer } from "../book_retained.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createBookPublicationClient } from "../book_publication.mjs";
import { renderBookPreview, renderBookSessionChapterPreview, renderBookSelectedPreview,
  parseBookPreview } from "../book_site_preview.mjs";
import { nativeBookFixture, chapters, included, decode, code } from "./book_pdfa_fixture.mjs";

const unsupported = code("UNSUPPORTED_BOOK_FONT_EMBEDDING");
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const previewDouble = (engine, files, options) => engine.renderBookSite(files, options);
function native() {
  const fixture = nativeBookFixture();
  fixture.NativeBook.prototype.setFontEmbedding = function (value) { this.settings.fontEmbedding = value; };
  return fixture;
}
function cacheFixture() {
  const fixture = native();
  const retained = createRetainedBook(fixture.api, previewDouble);
  const cached = createCachedBookRenderer(retained, previewDouble);
  return { ...fixture, cached, render(files, options, format) {
    const input = prepareBookInput(files, options);
    return cached.render(input.files, input.options, format);
  } };
}
function workerFixture(t, { engine, response } = {}) {
  const fixture = cacheFixture(), replies = [], sent = [], listeners = new Map();
  let receive, terminated = 0;
  installBookWorker({
    addEventListener(kind, handler) { assert.equal(kind, "message"); receive = handler; },
    postMessage(value, transfer = []) {
      const data = structuredClone(value, { transfer }); replies.push(structuredClone(data));
      response?.(data);
      queueMicrotask(() => {
        for (const handler of [...(listeners.get("message") ?? [])]) handler({ data });
      });
    },
  }, engine ?? { ...fixture.api, renderRetainedBook: fixture.cached.render, clearRetainedBook: fixture.cached.clear });
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 2000, workerFactory: () => ({
    addEventListener(kind, handler) {
      if (!listeners.has(kind)) listeners.set(kind, new Set());
      listeners.get(kind).add(handler);
    },
    removeEventListener(kind, handler) { listeners.get(kind)?.delete(handler); },
    postMessage(value, transfer) {
      sent.push(structuredClone(value));
      const data = structuredClone(value, { transfer }); queueMicrotask(() => receive({ data }));
    },
    terminate() { terminated++; },
  }) });
  t.after(() => { client.dispose(); fixture.cached.clear(); });
  return { ...fixture, client, replies, sent, get terminated() { return terminated; } };
}

for (const format of ["epub", "site"]) test(`${format}: Full → Subset → Full invalidates native and output caches`, async t => {
  const f = cacheFixture(); t.after(() => f.cached.clear());
  let previous;
  for (const embedding of ["full", "subset", "full"]) {
    const output = await f.render(chapters(), { fontEmbedding: embedding }, format);
    assert.equal(output.fontEmbedding, embedding === "full" ? "full" : undefined);
    assert.equal(decode(output).settings.fontEmbedding, output.fontEmbedding);
    const hot = await f.render(chapters(), { fontEmbedding: embedding }, format);
    assert.deepEqual(hot, output); assert.notEqual(hot.bytes, output.bytes);
    if (previous) assert.notDeepEqual(output.bytes, previous);
    previous = output.bytes;
  }
  assert.equal(f.state.created, 3); assert.equal(f.state.calls.length, 3);
});

test("Full output receipts survive owned/transferred bytes and source-history revisits", async t => {
  const f = cacheFixture(); t.after(() => f.cached.clear());
  const files = chapters(), options = { fontEmbedding: "full", includeSources: included() };
  const first = await f.render(files, options, "epub"), expected = first.bytes.slice();
  const sent = structuredClone(first.bytes, { transfer: [first.bytes.buffer] });
  assert.deepEqual(sent, expected); assert.equal(first.bytes.byteLength, 0);
  const hot = await f.render(files, options, "epub"); hot.bytes.fill(0);
  const edited = files.map((file, i) => ({ ...file, source: file.source + (i === 0 ? "\nEdit" : "") }));
  const changed = await f.render(edited, options, "epub"); assert.equal(changed.fontEmbedding, "full");
  const restored = await f.render(files, options, "epub");
  assert.equal(restored.fontEmbedding, "full"); assert.deepEqual(restored.bytes, expected);
  assert.equal(f.state.created, 1); assert.equal(f.state.calls.length, 2);
});

for (const format of ["pdf", "epub", "site", "preview"]) {
  test(`older worker cannot silently discard Full on first ${format} request`, async t => {
    // Simulate an old successful wire reply independently of the new worker.
    const f = workerFixture(t, { response(data) { delete data.bookFontEmbedding; delete data.fontEmbedding; } });
    await assert.rejects(f.client.render(chapters(), format, { fontEmbedding: "full" }), unsupported);
    assert.equal(f.client.hasRetainedBook, false); assert.equal(f.terminated, 1);
  });
}

for (const [name, mutate] of [
  ["missing private capture", data => { delete data.bookFontEmbedding; }],
  ["malformed private capture", data => { data.bookFontEmbedding = { policy: "full" }; }],
  ["missing exporter receipt", data => { delete data.fontEmbedding; }],
  ["malformed exporter receipt", data => { data.fontEmbedding = "subset"; }],
]) test(`Full worker response rejects ${name}`, async t => {
  const f = workerFixture(t, { response: mutate });
  await assert.rejects(f.client.render(chapters(), "epub", { fontEmbedding: "full" }), unsupported);
  assert.equal(f.client.retainedInputRevision, null);
});

for (const field of ["fontEmbedding", "bookFontEmbedding"])
  test(`default requests reject unexpected ${field}: Full`, async t => {
    const f = workerFixture(t, { response(data) { data[field] = "full"; } });
    await assert.rejects(f.client.render(chapters(), "site"), unsupported);
  });

test("PDF-first source deltas preserve the captured Full policy through links and EPUB", async t => {
  const f = workerFixture(t);
  let result = await f.client.render(chapters(), "pdf", { fontEmbedding: "full" });
  assert.equal(result.fontEmbedding, undefined);
  result = await f.client.renderSourceUpdate([], "links", { expectedRevision: result.retainedInputRevision });
  assert.equal(result.fontEmbedding, undefined);
  const changed = [{ path: chapters()[0].path, source: "# Edited" }];
  result = await f.client.renderSourceUpdate(changed, "epub", { expectedRevision: result.retainedInputRevision });
  assert.equal(result.fontEmbedding, "full"); assert.equal(decode(result).files[0].source, "# Edited");
  assert.equal(decode(result).settings.fontEmbedding, "full");
  assert.ok(f.replies.every(reply => reply.bookFontEmbedding === "full"));
  assert.ok(f.sent.slice(1).every(request => request.options === undefined));
  assert.equal(f.state.created, 1);
});

for (const format of ["epub", "site", "preview"]) {
  test(`${format} requires the actual exporter receipt, not echoed worker options`, async t => {
    const f = workerFixture(t, { engine: {
      renderRetainedBook: async () => ({ bytes: Uint8Array.of(1, 2), sourceLength: 3 }),
      clearRetainedBook() {},
    } });
    await assert.rejects(f.client.render(chapters(), format, { fontEmbedding: "full" }), unsupported);
    assert.equal(f.replies[0].error.code, "UNSUPPORTED_BOOK_FONT_EMBEDDING");
    assert.equal(f.replies[0].retainedInputRevision, undefined);
  });
}

test("real Node worker captures scalar Full and the exact font view, retains deltas, and resets to Subset", async t => {
  const program = `
    import { parentPort } from 'node:worker_threads';
    import { installBookWorker } from ${JSON.stringify(new URL("../book_worker.mjs", import.meta.url).href)};
    import { createRetainedBook, createCachedBookRenderer } from ${JSON.stringify(new URL("../book_retained.mjs", import.meta.url).href)};
    import { nativeBookFixture } from ${JSON.stringify(new URL("./book_pdfa_fixture.mjs", import.meta.url).href)};
    const {api, NativeBook} = nativeBookFixture();
    NativeBook.prototype.setFontEmbedding = function (value) { this.settings.fontEmbedding = value; };
    const preview = (engine, files, options) => engine.renderBookSite(files, options);
    const retained = createRetainedBook(api, preview), cache = createCachedBookRenderer(retained, preview);
    installBookWorker({
      addEventListener(kind, receive) { parentPort.on(kind, data => receive({data})); },
      postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
    }, {...api, renderRetainedBook: cache.render, clearRetainedBook: cache.clear});
  `;
  let creates = 0;
  const stops = [], sent = [];
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 5000, workerFactory() {
    creates++; const listeners = new Map();
    const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(program)}`));
    return {
      addEventListener(kind, handler) {
        const wrapped = kind === "message" ? data => handler({ data }) : error => handler({ error });
        if (!listeners.has(kind)) listeners.set(kind, new Map());
        listeners.get(kind).set(handler, wrapped); worker.on(kind, wrapped);
      },
      removeEventListener(kind, handler) {
        const map = listeners.get(kind), wrapped = map?.get(handler);
        if (wrapped) worker.off(kind, wrapped); map?.delete(handler);
      },
      postMessage(data, transfer) { sent.push(structuredClone(data)); worker.postMessage(data, transfer); },
      terminate() { const stop = worker.terminate(); stops.push(stop); return stop; },
    };
  } });
  t.after(async () => { client.dispose(); await Promise.all(stops); });
  const backing = Uint8Array.of(91, 3, 4, 92), view = backing.subarray(1, 3);
  const options = { fontEmbedding: "full", fontAssets: [{ slot: "body-regular", bytes: view }] };
  const pending = client.render(chapters(), "pdf", options);
  options.fontEmbedding = "subset"; backing.fill(0);
  let result = await pending;
  assert.equal(result.fontEmbedding, undefined); assert.equal(backing.byteLength, 4);
  assert.deepEqual(decode(result).fonts, [["body-regular", [3, 4]]]);
  result = await client.renderSourceUpdate([], "links", { expectedRevision: result.retainedInputRevision });
  assert.equal(result.fontEmbedding, undefined);
  result = await client.renderSourceUpdate([{ path: chapters()[0].path, source: "# Changed" }], "epub",
    { expectedRevision: result.retainedInputRevision });
  assert.equal(result.fontEmbedding, "full"); assert.equal(decode(result).settings.fontEmbedding, "full");
  assert.deepEqual(decode(result).fonts, [["body-regular", [3, 4]]]);
  const full = result.bytes.slice(); structuredClone(result.bytes, { transfer: [result.bytes.buffer] });
  result = await client.renderSourceUpdate([], "epub", { expectedRevision: result.retainedInputRevision });
  assert.equal(result.fontEmbedding, "full"); assert.deepEqual(result.bytes, full);
  result = await client.render(chapters(), "site", { fontEmbedding: "subset" });
  assert.equal(result.fontEmbedding, undefined); assert.equal(decode(result).settings.fontEmbedding, undefined);
  assert.equal(creates, 1);
  assert.ok(sent.slice(1, 4).every(request => request.options === undefined));
  assert.deepEqual([...sent[0].options.fontAssets[0].bytes], [3, 4]);
});

// Independent, stored ZIP construction exercises the real preview decoder.
function archive(entries) {
  const local = [], central = []; let offset = 0;
  for (const [name, text] of entries) {
    const path = Buffer.from(name), body = Buffer.from(text);
    let crc = 0xffffffff;
    for (const byte of body) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
    crc = (crc ^ 0xffffffff) >>> 0;
    const a = Buffer.alloc(30), b = Buffer.alloc(46);
    a.writeUInt32LE(0x04034b50); a.writeUInt16LE(20, 4); a.writeUInt16LE(0x800, 6);
    a.writeUInt32LE(crc, 14); a.writeUInt32LE(body.length, 18); a.writeUInt32LE(body.length, 22);
    a.writeUInt16LE(path.length, 26);
    b.writeUInt32LE(0x02014b50); b.writeUInt16LE(20, 4); b.writeUInt16LE(20, 6); b.writeUInt16LE(0x800, 8);
    b.writeUInt32LE(crc, 16); b.writeUInt32LE(body.length, 20); b.writeUInt32LE(body.length, 24);
    b.writeUInt16LE(path.length, 28); b.writeUInt32LE(offset, 42);
    local.push(a, path, body); central.push(b, path); offset += a.length + path.length + body.length;
  }
  const directory = Buffer.concat(central), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(directory.length, 12); end.writeUInt32LE(offset, 16);
  return new Uint8Array(Buffer.concat([...local, directory, end]));
}
test("actual preview codecs preserve Full through whole-book, native chapter, and site fallback", async () => {
  const page = { path: "one.html", source: "one.md", title: "One", html: "<p>office</p>" };
  const bytes = archive([[page.path, page.html], ["index.html", "<p>Book</p>"],
    ["search-index.json", JSON.stringify({ schema: "fmd-book-search-index-v1",
      chapters: [{ page: page.path, source: page.source, title: page.title }] })]]);
  const site = { bytes, sourceLength: 8, fontEmbedding: "full" };
  const full = await renderBookPreview({ renderBookSite: () => site }, [], { fontEmbedding: "full" });
  assert.equal(full.fontEmbedding, "full"); assert.deepEqual(parseBookPreview(full.bytes).pages, [page]);
  await assert.rejects(renderBookPreview({ renderBookSite: () => ({ ...site, fontEmbedding: undefined }) }, [],
    { fontEmbedding: "full" }), unsupported);
  const chapter = encode({ schema: "fmd-book-chapter-preview-v1", selected: 0,
    pages: [{ path: page.path, source: page.source, title: page.title }], html: page.html });
  const session = { chapterCount: 1, supportsChapterPreview: true,
    renderChapterPreview: () => ({ bytes: chapter, sourceLength: 8, fontEmbedding: "full" }),
    renderSite: () => site, dispose() {} };
  const native = await renderBookSessionChapterPreview(session);
  assert.equal(native.fontEmbedding, "full"); assert.equal(native.previewMode, "native-chapter");
  session.supportsChapterPreview = false;
  const fallback = await renderBookSessionChapterPreview(session);
  assert.equal(fallback.fontEmbedding, "full"); assert.equal(fallback.previewMode, "site-fallback");
  const selected = await renderBookSelectedPreview({ createBook: () => session }, [], { fontEmbedding: "full" });
  assert.deepEqual(selected.bytes, fallback.bytes); assert.equal(selected.fontEmbedding, "full");
});

test("atomic publication rejects an older client without Full receipts and publishes no partial edition", async t => {
  const calls = []; let revision = 0, disposed = 0;
  const publisher = createBookPublicationClient({ captureInput: prepareBookInput, createClient: () => ({
    render(_files, format) { return output(format); },
    renderSourceUpdate(_changes, format) { return output(format); },
    dispose() { disposed++; },
  }) });
  function output(format) {
    calls.push(format); return { format: `book-${format}`, bytes: Uint8Array.of(1),
      sourceLength: 4, retainedInputRevision: ++revision };
  }
  t.after(() => publisher.dispose());
  await assert.rejects(publisher.render(chapters(), ["pdf", "epub", "site"], { fontEmbedding: "full" }), unsupported);
  assert.deepEqual(calls, ["pdf", "epub"]); assert.equal(disposed, 1); assert.equal(publisher.busy, false);
});

test("invalid font policies reject before asset getters and native loading; old defaults remain valid", async () => {
  const fixture = nativeBookFixture(); let assetsRead = 0;
  for (const policy of [null, false, 3, "FULL", "unknown", {}, []]) {
    await assert.rejects(fixture.api.createBook(chapters(), { fontEmbedding: policy,
      get fontAssets() { assetsRead++; throw new Error("assets read"); } }), code("INVALID_OPTIONS"));
  }
  assert.equal(assetsRead, 0); assert.equal(fixture.state.loads, 0);
  await assert.rejects(fixture.api.createBook(chapters(), { fontEmbedding: "full" }), unsupported);
  assert.equal(fixture.state.created, 0);
  const a = await fixture.api.renderBookEpub(chapters());
  const b = await fixture.api.renderBookEpub(chapters(), { fontEmbedding: "subset" });
  assert.deepEqual(a.bytes, b.bytes); assert.equal(a.fontEmbedding, undefined); assert.equal(b.fontEmbedding, undefined);
});

test("receipt helper distinguishes optional shape from an explicit Subset expectation", () => {
  assert.deepEqual(bookFontEmbeddingReceipt({ fontEmbedding: "full" }), { fontEmbedding: "full" });
  assert.deepEqual(bookFontEmbeddingReceipt({}), {});
  assert.throws(() => bookFontEmbeddingReceipt({ fontEmbedding: "full" }, "subset"), unsupported);
  assert.throws(() => bookFontEmbeddingReceipt({}, "full"), unsupported);
  for (const value of ["subset", null, false, {}, []])
    assert.throws(() => bookFontEmbeddingReceipt({ fontEmbedding: value }), unsupported);
});
