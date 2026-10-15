// Production admission, transport, retained native adapter and output cache.
// Real Node threads transfer buffers; only the native renderer is a double.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createBookWorkerClient, installBookWorker, applyBookSourceUpdate, applyBookSourceSet } from "../book_worker.mjs";
import { prepareBookInput } from "../book_session.mjs";
import { chapters, included, geometry, decode, code } from "./book_pdfa_fixture.mjs";

function threaded(t, { old = false, retainBook = true, maxOutputBytes, timeoutMs = 10000 } = {}) {
  const state = { created: 0, terminated: 0, messages: [], stops: [] };
  const program = `
    import { parentPort } from 'node:worker_threads';
    import { installBookWorker } from ${JSON.stringify(new URL('../book_worker.mjs', import.meta.url).href)};
    import { createRetainedBook, createCachedBookRenderer } from ${JSON.stringify(new URL('../book_retained.mjs', import.meta.url).href)};
    import { nativeBookFixture } from ${JSON.stringify(new URL('./book_pdfa_fixture.mjs', import.meta.url).href)};
    const {api} = nativeBookFixture({ old: ${old} });
    const preview = async (engine, files, options) => engine.renderBookSite(files, options);
    const retained = createRetainedBook(api, preview);
    const cache = createCachedBookRenderer(retained, preview);
    installBookWorker({
      addEventListener(kind, handler) { parentPort.on(kind, data => handler({data})); },
      postMessage(data, transfer) { parentPort.postMessage(data, transfer); }
    }, {...api, renderRetainedBook: cache.render, clearRetainedBook: cache.clear});
  `;
  const client = createBookWorkerClient({ retainBook, maxOutputBytes, timeoutMs,
    workerFactory() {
      state.created++;
      const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(program)}`)), listeners = new Map();
      return {
        postMessage(data, transfer) { state.messages.push(structuredClone(data)); worker.postMessage(data, transfer); },
        addEventListener(kind, handler) {
          const wrapped = kind === "message" ? data => handler({data}) : error => handler({error});
          listeners.set(handler, wrapped); worker.on(kind, wrapped);
        },
        removeEventListener(kind, handler) { const wrapped = listeners.get(handler); if (wrapped) worker.off(kind, wrapped); listeners.delete(handler); },
        terminate() { state.terminated++; const stopped = worker.terminate(); state.stops.push(stopped); return stopped; },
      };
    },
  });
  t.after(async () => { client.dispose(); await Promise.all(state.stops); });
  return { client, state };
}

for (const retainBook of [false, true]) test(`real worker captures policy/assets before transfer: retained=${retainBook}`, async t => {
  const { client, state } = threaded(t, { retainBook });
  const options = { pdfA: "2b", pdfAStrict: true, page: geometry(), includeSources: included(),
    images: [{ destination: "guide/img.png", bytes: new Uint8Array([1, 2]) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]) }] };
  const pending = client.render(chapters(), "pdf", options);
  options.pdfA = "off"; options.pdfAStrict = false; options.page.size.widthPt = 900;
  options.images[0].bytes.fill(0);
  const output = await pending, got = decode(output);
  assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, true);
  assert.equal(got.pdfA, "2b"); assert.equal(got.strict, true);
  assert.deepEqual(got.page, [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(got.images, [["guide/img.png", [1, 2]]]);
  assert.equal(options.images[0].bytes.byteLength, 2);
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
  assert.equal(state.created, 1); assert.equal(state.terminated, retainBook ? 0 : 1);
});

test("EPUB-first retained edition keeps PDF policy through deltas, history hits, source sets and link checks", async t => {
  const { client, state } = threaded(t);
  const first = await client.render(chapters(), "epub", { pdfA: "2b", pdfAStrict: true, includeSources: included(), page: geometry() });
  assert.equal(first.pdfA, undefined); assert.equal(decode(first).kind, "epub");
  const delta = (changes, format) => client.renderSourceUpdate(changes, format, { expectedRevision: client.retainedInputRevision });
  const firstPdf = await delta([], "pdf"); assert.equal(firstPdf.pdfAStrict, true);
  const revision = client.retainedInputRevision;
  await assert.rejects(client.renderSourceUpdate([], "pdf", { expectedRevision: revision - 1 }), code("STALE_BOOK_CAPTURE"));
  assert.equal(client.retainedInputRevision, revision);
  await delta([{ path: "shared.txt", source: "changed" }], "pdf");
  const original = await delta(included(), "pdf"); assert.deepEqual(original.bytes, firstPdf.bytes);
  assert.equal(original.pdfAStrict, true);
  const next = chapters().reverse(); next.push({ path: "extra.md", source: "# Extra" });
  const reordered = await client.renderSourceSet(next, "pdf", { expectedRevision: client.retainedInputRevision });
  assert.equal(reordered.pdfAStrict, true); assert.deepEqual(decode(reordered).files, next);
  assert.deepEqual(decode(reordered).includes, []);
  const checked = await delta([], "links"); assert.equal(checked.pdfA, undefined); assert.equal(checked.format, "book-links");
  const last = await delta([], "pdf"); assert.equal(last.pdfAStrict, true);
  assert.equal(state.created, 1);
  for (const message of state.messages.slice(1)) {
    assert.equal(message.options, undefined); assert.equal(message.files, undefined);
    assert.ok(message.sourceUpdate || message.sourceSet);
  }
});

test("same worker moves among ordinary, archival and strict settings without stale cache hits", async t => {
  const { client, state } = threaded(t);
  for (const policy of [{}, {pdfA: "2b"}, {pdfA: "2b", pdfAStrict: true}, {}]) {
    const output = await client.render(chapters(), "pdf", policy);
    assert.equal(decode(output).pdfA, policy.pdfA ?? "off");
    assert.equal(decode(output).strict, policy.pdfAStrict ?? false);
    assert.equal(output.pdfA, policy.pdfA);
    const second = await client.renderSourceUpdate([], "pdf", { expectedRevision: client.retainedInputRevision });
    assert.deepEqual(second.bytes, output.bytes); assert.equal(second.pdfA, output.pdfA);
  }
  assert.equal(state.created, 1);
});

test("old native worker rejects archival output and the client can render ordinary PDF afterwards", async t => {
  const { client, state } = threaded(t, { old: true });
  await assert.rejects(client.render(chapters(), "pdf", {pdfA: "2b"}), code("UNSUPPORTED_BOOK_PDFA"));
  assert.equal(client.hasRetainedBook, false); assert.equal(state.terminated, 1);
  assert.equal(decode(await client.render(chapters(), "pdf")).pdfA, "off");
  assert.equal(state.created, 2);
});

test("archival admission and output limits retain the original worker budgets", async t => {
  const { client, state } = threaded(t, { maxOutputBytes: 1 });
  await assert.rejects(client.render(chapters(), "pdf", {pdfAStrict: true}), code("INVALID_OPTIONS"));
  assert.equal(state.created, 0);
  await assert.rejects(client.render(chapters(), "pdf", {pdfA: "2b"}), code("OUTPUT_LIMIT"));
  assert.equal(client.hasRetainedBook, false); assert.equal(state.terminated, 1);
});

test("cancellation retires the archival capture, and a later ordinary job has no inherited policy", async t => {
  const { client, state } = threaded(t);
  const pending = client.render(chapters(), "pdf", {pdfA: "2b", pdfAStrict: true});
  client.cancel(); await assert.rejects(pending, code("EXPORT_CANCELLED"));
  assert.equal(state.terminated, 1); assert.equal(client.busy, false);
  assert.equal(decode(await client.render(chapters(), "pdf")).pdfA, "off");
});

function endpoint(t, transform, settings = {}) {
  let listener; const state = { terminated: 0 };
  const client = createBookWorkerClient({ ...settings, workerFactory: () => ({
    addEventListener(kind, fn) { if (kind === "message") listener = fn; },
    removeEventListener(kind, fn) { if (kind === "message" && listener === fn) listener = null; },
    postMessage(data) { const reply = transform(data); queueMicrotask(() => listener?.({data: reply})); },
    terminate() { state.terminated++; },
  }) });
  t.after(() => client.dispose()); return {client, state};
}
const success = request => ({ schemaVersion: 1, id: request.id, format: request.format,
  bytes: Uint8Array.of(1), sourceLength: 1,
  ...(request.retainBook ? {retainedBook: true, retainedInputRevision: request.id} : {}),
  ...(request.sourceSet ? {sourceSetApplied: true} : {}),
});
for (const format of ["pdf", "epub", "site"]) test(`old worker scripts cannot silently discard policy even for ${format}-first requests`, async t => {
  const {client, state} = endpoint(t, success, {retainBook: true});
  await assert.rejects(client.render(chapters(), format, {pdfA: "2b", pdfAStrict: true}), code("UNSUPPORTED_BOOK_PDFA"));
  assert.equal(state.terminated, 1); assert.equal(client.hasRetainedBook, false);
});
for (const fields of [{bookPdfA: "off", bookPdfAStrict: true}, {bookPdfA: "2b"},
  {bookPdfA: "2b", bookPdfAStrict: false}, {bookPdfA: "2b", bookPdfAStrict: "true"}]) {
  test(`mismatched policy acknowledgement rejects: ${JSON.stringify(fields)}`, async t => {
    const {client} = endpoint(t, d => ({...success(d), ...fields}));
    await assert.rejects(client.render(chapters(), "pdf", {pdfA: "2b", pdfAStrict: true}), code("UNSUPPORTED_BOOK_PDFA"));
  });
}

test("an unrequested archival acknowledgement cannot label an ordinary PDF", async t => {
  const {client} = endpoint(t, d => ({...success(d), bookPdfA: "2b", bookPdfAStrict: false}));
  await assert.rejects(client.render(chapters(), "pdf"), code("WORKER_PROTOCOL_ERROR"));
});

test("retained deltas require the original policy, not a replacement worker assertion", async t => {
  let calls = 0;
  const {client, state} = endpoint(t, d => ({...success(d), bookPdfA: "2b", bookPdfAStrict: ++calls === 1}), {retainBook: true});
  await client.render(chapters(), "epub", {pdfA: "2b", pdfAStrict: true});
  await assert.rejects(client.renderSourceUpdate([], "pdf", {expectedRevision: client.retainedInputRevision}), code("UNSUPPORTED_BOOK_PDFA"));
  assert.equal(state.terminated, 1); assert.equal(client.retainedInputRevision, null);
});

test("worker-side admission requires an actual archival exporter receipt, not echoed options", async () => {
  let receive, calls = 0; const replies = [];
  installBookWorker({ addEventListener(_, fn) { receive = fn; }, postMessage(value) { replies.push(value); } }, {
    async renderBookPdf() { calls++; return { bytes: Uint8Array.of(1), sourceLength: 1 }; },
  });
  const base = {schemaVersion: 1, id: 1, format: "pdf", maxOutputBytes: 10000, files: chapters()};
  await receive({data: {...base, options: {pdfA: "2b", pdfAStrict: true}}});
  assert.equal(replies[0].error.code, "UNSUPPORTED_BOOK_PDFA"); assert.equal(replies[0].bytes, undefined);
  await receive({data: {...base, id: 2, options: {pdfAStrict: true}}});
  assert.equal(replies[1].error.code, "INVALID_OPTIONS"); assert.equal(calls, 1);
  await receive({data: {...base, id: 3, options: {}}}); assert.equal(replies[2].error, undefined);
});

test("source reconstruction retains policy and original asset ownership without renewal", () => {
  const input = prepareBookInput(chapters(), {pdfA: "2b", pdfAStrict: true, includeSources: included(),
    images: [{destination: "figure.png", bytes: Uint8Array.of(7)}]});
  const delta = applyBookSourceUpdate(input, {expectedRevision: 1, files: [{path: "shared.txt", source: "new"}]});
  const set = applyBookSourceSet(delta, {expectedRevision: 2, files: chapters().reverse(), includeSources: []});
  for (const next of [delta, set]) {
    assert.equal(next.options.pdfA, "2b"); assert.equal(next.options.pdfAStrict, true);
    assert.equal(next.options.images, input.options.images); assert.equal(next.options.fontAssets, input.options.fontAssets);
  }
  assert.deepEqual(input.options.includeSources, included());
});
