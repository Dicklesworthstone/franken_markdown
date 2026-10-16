// Real Node threads and production model/profile/proof/session/worker/cache
// modules. The existing native boundary fixture emits JSON observations; a PDF
// header here is an envelope admission probe, NOT a rendered or conforming PDF.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createBookWorkerClient } from "../book_worker.mjs";
import { createBookPdfProof } from "../book_pdf_proof.mjs";
import { setup } from "./book_publication_archival_helpers.mjs";

function threaded(t, old = false) {
  const state = { created: 0, terminated: 0, sent: [], stops: [] };
  const program = `
    import { parentPort, workerData } from 'node:worker_threads';
    import { installBookWorker } from ${JSON.stringify(new URL('../book_worker.mjs', import.meta.url).href)};
    import { createRetainedBook, createCachedBookRenderer } from ${JSON.stringify(new URL('../book_retained.mjs', import.meta.url).href)};
    import { nativeBookFixture } from ${JSON.stringify(new URL('./book_pdfa_fixture.mjs', import.meta.url).href)};
    const {api, NativeBook} = nativeBookFixture({old: workerData.old});
    const emit = NativeBook.prototype.emit;
    NativeBook.prototype.emit = function (...args) {
      const observed = emit.call(this, ...args);
      return args[0] !== 'pdf' ? observed : new TextEncoder().encode('%PDF-1.7\\n' + new TextDecoder().decode(observed));
    };
    const preview = async (engine, files, options) => engine.renderBookSite(files, options);
    const retained = createRetainedBook(api, preview), cache = createCachedBookRenderer(retained, preview);
    installBookWorker({
      addEventListener(kind, handler) { parentPort.on(kind, data => handler({data})); },
      postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
    }, {...api, renderRetainedBook: cache.render, clearRetainedBook: cache.clear});
  `;
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 10000,
    workerFactory() {
      state.created++; const listeners = new Map();
      const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(program)}`), {workerData: {old}});
      return {
        addEventListener(kind, handler) {
          const wrapped = kind === 'message' ? data => handler({data}) : error => handler({error});
          if (!listeners.has(kind)) listeners.set(kind, new Map());
          listeners.get(kind).set(handler, wrapped); worker.on(kind, wrapped);
        },
        removeEventListener(kind, handler) {
          const list = listeners.get(kind), wrapped = list?.get(handler);
          if (wrapped) worker.off(kind, wrapped); list?.delete(handler);
        },
        postMessage(data, transfer) { state.sent.push(structuredClone(data)); worker.postMessage(data, transfer); },
        terminate() { state.terminated++; const stopped = worker.terminate(); state.stops.push(stopped); return stopped; },
      };
    },
  });
  t.after(async () => { client.dispose(); await Promise.all(state.stops); });
  return {client, state};
}
const observations = async proof => JSON.parse((await proof.blob.text()).slice(9));

test("applied archival policy reaches native ABI through proofs, source deltas, source sets and restored projects", async t => {
  const h = setup(t), {client, state} = threaded(t);
  const session = createBookPdfProof({collection: h.book, controls: h.controls, worker: client});
  t.after(() => session.dispose());
  h.input("pdfPolicy", "2b-strict"); h.ui.apply();
  const original = await session.render(), first = await observations(original);
  assert.equal(original.pdfA, "2b"); assert.equal(original.pdfAStrict, true);
  assert.equal(first.pdfA, "2b"); assert.equal(first.strict, true);
  assert.deepEqual(first.images, [["assets/logo.png", [1, 2, 3]]]);
  assert.deepEqual(first.fonts, [["body-regular", [4, 5]]]);
  assert.equal(first.includes[0].source, "Exact\r\n本文");
  assert.deepEqual(first.page.slice(2), [36, 36, 36, 36]);
  assert.equal(await session.render(), original); assert.equal(state.sent.length, 1);
  h.book.edit(2, "shared.txt", "Changed include"); assert.equal(session.current, null);
  const edited = await session.render(); assert.equal(edited.pdfAStrict, true);
  assert.equal((await observations(edited)).includes[0].source, "Changed include");
  assert.deepEqual(state.sent[1].sourceUpdate.files, [{path: "shared.txt", source: "Changed include"}]);
  assert.equal(state.sent[1].options, undefined);
  h.book.move(0, 1); const reordered = await session.render();
  assert.equal(reordered.pdfAStrict, true); assert.equal((await observations(reordered)).files[0].path, "two.md");
  assert.ok(state.sent[2].sourceSet); assert.equal(state.sent[2].options, undefined); assert.equal(state.created, 1);
  // Restoring a source project must preserve the policy but revoke resources,
  // forcing a new complete input and releasing the old authorized worker.
  const project = h.book.project(); h.book.replaceProject(project);
  const restored = await session.render(), restoredData = await observations(restored);
  assert.equal(restored.pdfAStrict, true); assert.deepEqual(restoredData.images, []); assert.deepEqual(restoredData.fonts, []);
  assert.equal(state.created, 2); assert.equal(state.terminated, 1);
  h.input("pdfPolicy", "2b"); h.ui.apply();
  const relaxed = await session.render(); assert.equal(relaxed.pdfAStrict, false);
  assert.equal((await observations(relaxed)).strict, false); assert.equal(state.created, 3);
  h.input("pdfPolicy", "off"); h.ui.apply();
  const ordinary = await session.render(); assert.equal(ordinary.pdfA, undefined);
  assert.equal((await observations(ordinary)).pdfA, "off"); assert.equal(state.created, 4);
});

test("old native worker cannot satisfy an archival proof; only an explicitly applied ordinary policy permits retry", async t => {
  const h = setup(t, {pdfA: "2b", pdfAStrict: true}), {client, state} = threaded(t, true);
  const session = createBookPdfProof({collection: h.book, controls: h.controls, worker: client});
  t.after(() => session.dispose());
  await assert.rejects(session.render(), {code: "UNSUPPORTED_BOOK_PDFA"});
  assert.equal(session.current, null); assert.equal(state.created, 1); assert.equal(state.terminated, 1);
  assert.equal(h.book.options.pdfAStrict, true);
  h.input("pdfPolicy", "off"); h.ui.apply();
  const proof = await session.render(); assert.equal(proof.pdfA, undefined);
  assert.equal((await observations(proof)).pdfA, "off"); assert.equal(state.created, 2);
});
