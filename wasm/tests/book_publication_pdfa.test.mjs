// Actual public factory, coordinator, both link-check adapters, transport,
// session and cache run unchanged through normal imports. Native rendering and
// navigation generation are explicit doubles; Node worker threads are real.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createBookPublicationWorker } from "../book-worker.js";
import { createBookPublicationClient } from "../book_publication.mjs";
import { prepareBookInput } from "../book_session.mjs";
import { chapters, included, geometry, decode, code } from "./book_pdfa_fixture.mjs";

function publicPublisher(t, { crash, nativeError, old = false, ...settings } = {}) {
  const state = { created: 0, terminated: 0, sent: [], stops: [] };
  const program = `
    import { parentPort, workerData } from 'node:worker_threads';
    import { installBookWorker } from ${JSON.stringify(new URL('../book_worker.mjs', import.meta.url).href)};
    import { createRetainedBook, createCachedBookRenderer } from ${JSON.stringify(new URL('../book_retained.mjs', import.meta.url).href)};
    import { nativeBookFixture } from ${JSON.stringify(new URL('./book_pdfa_fixture.mjs', import.meta.url).href)};
    const {api, state, NativeBook} = nativeBookFixture({ old: workerData.old });
    if (workerData.nativeError) state.failure = Object.assign(Error('native archival refusal'), {code: workerData.nativeError});
    const emit = NativeBook.prototype.emit;
    NativeBook.prototype.emit = function (...args) {
      const value = JSON.parse(new TextDecoder().decode(emit.call(this, ...args)));
      return new TextEncoder().encode(JSON.stringify({...value, worker: workerData.ordinal}));
    };
    const preview = async (engine, files, options) => engine.renderBookSite(files, options);
    const retained = createRetainedBook(api, preview), cache = createCachedBookRenderer(retained, preview);
    installBookWorker({
      addEventListener(kind, receive) { parentPort.on(kind, data => {
        if (workerData.crash === data.format) throw Error('injected worker process loss');
        void receive({data});
      }); },
      postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
    }, {...api, renderRetainedBook: cache.render, clearRetainedBook: cache.clear});
  `;
  const publisher = createBookPublicationWorker({ timeoutMs: 10000, ...settings,
    workerFactory() {
      const ordinal = ++state.created, listeners = new Map();
      const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(program)}`), {
        workerData: { ordinal, old, nativeError, crash: crash?.worker === ordinal ? crash.format : null },
      });
      return {
        addEventListener(kind, handler) {
          const wrapped = kind === 'message' ? data => handler({data}) : error => handler({error});
          listeners.set(handler, wrapped); worker.on(kind, wrapped);
        },
        removeEventListener(kind, handler) {
          const wrapped = listeners.get(handler); if (wrapped) worker.off(kind, wrapped); listeners.delete(handler);
        },
        postMessage(data, transfer) { state.sent.push({ordinal, data: structuredClone(data)}); worker.postMessage(data, transfer); },
        terminate() { state.terminated++; const promise = worker.terminate(); state.stops.push(promise); return promise; },
      };
    },
  });
  t.after(async () => { publisher.dispose(); await Promise.all(state.stops); });
  return { publisher, state };
}
const archive = () => ({ pdfA: "2b", pdfAStrict: true, page: geometry(), includeSources: included(),
  title: "Captured manual", images: [{destination: "guide/logo.png", bytes: Uint8Array.of(1, 2)}],
  fontAssets: [{slot: "body-regular", bytes: Uint8Array.of(3, 4)}] });

for (const formats of [["pdf"], ["pdf", "epub", "site"], ["epub", "site", "pdf"], ["site", "pdf", "epub"]]) {
  for (const strictLinks of [false, true]) test(`public archived edition keeps exact policy and native receipts: ${formats}, links=${strictLinks}`, async t => {
    const {publisher, state} = publicPublisher(t);
    const options = archive(), files = chapters(), phases = [];
    const pending = publisher.render(files, formats, options, {strictLinks, onProgress: p => phases.push(p)});
    files[0].source = "Changed editor"; options.pdfA = "off"; options.pdfAStrict = false;
    options.images[0].bytes.fill(0); options.page.size.widthPt = 900;
    const result = await pending;
    assert.equal(result.format, "book-publication"); assert.equal(publisher.busy, false);
    assert.equal(result.outputs.length, formats.length);
    assert.equal(result.byteLength, result.outputs.reduce((n, o) => n + o.bytes.length, 0));
    const worker = strictLinks ? 2 : 1;
    for (const [index, output] of result.outputs.entries()) {
      const format = formats[index], observed = decode(output);
      assert.equal(observed.kind, format); assert.equal(observed.worker, worker);
      assert.deepEqual(observed.files, chapters()); assert.deepEqual(observed.includes, included());
      assert.deepEqual(observed.images, [["guide/logo.png", [1, 2]]]);
      assert.deepEqual(observed.fonts, [["body-regular", [3, 4]]]);
      assert.equal(output.sourceLength, result.sourceLength);
      assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
      if (format === "pdf") {
        assert.equal(observed.pdfA, "2b"); assert.equal(observed.strict, true);
        assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, true);
        assert.deepEqual(observed.page, [720, 540, 18, 24, 30, 36]);
      } else {
        assert.equal(output.pdfA, undefined); assert.equal(output.pdfAStrict, undefined);
      }
    }
    assert.equal(state.created, worker); assert.equal(state.terminated, worker);
    assert.equal(Boolean(result.linkReport), strictLinks);
    const sends = state.sent.filter(s => s.ordinal === worker).map(s => s.data);
    assert.deepEqual(sends.map(d => d.format), [...formats, ...(strictLinks ? ["links"] : [])]);
    assert.equal(sends[0].options.pdfA, "2b");
    for (const data of sends.slice(1)) { assert.deepEqual(data.sourceUpdate.files, []); assert.equal(data.options, undefined); }
    if (strictLinks) {
      const check = state.sent[0].data;
      assert.equal(check.options.pdfA, undefined); assert.deepEqual(check.options.images, []); assert.deepEqual(check.options.fontAssets, []);
      assert.equal(phases.at(-1).phase, "verifying-publication");
      assert.equal(result.linkReport.summary.findings, 0);
    }
  });
}

for (const failure of [
  {worker: 1, format: "links", finalWorker: 3},
  {worker: 2, format: "epub", finalWorker: 4},
  {worker: 2, format: "links", finalWorker: 4},
]) test(`real worker loss during ${JSON.stringify(failure)} restarts a complete archived edition with both checks`, async t => {
  const {publisher, state} = publicPublisher(t, {crash: failure});
  const options = archive(), files = chapters(), phases = [];
  const result = await publisher.render(files, ["pdf", "epub", "site"], options, {strictLinks: true,
    onProgress(p) { phases.push(p); if (p.phase === "retrying") {
      options.pdfA = "off"; options.pdfAStrict = false; options.images[0].bytes.fill(0); files[0].source = "after loss";
    } },
  });
  assert.equal(state.created, failure.finalWorker); assert.equal(state.terminated, state.created);
  assert.equal(phases.filter(p => p.phase === "checking-links").length, 2);
  assert.equal(phases.filter(p => p.phase === "retrying").length, 1);
  for (const output of result.outputs) {
    const value = decode(output); assert.equal(value.worker, failure.finalWorker);
    assert.deepEqual(value.files, chapters()); assert.deepEqual(value.images, [["guide/logo.png", [1, 2]]]);
  }
  assert.equal(result.outputs[0].pdfAStrict, true); assert.equal(result.linkReport.summary.findings, 0);
});

for (const settings of [{old: true}, {nativeError: "STRICT_ARCHIVAL_REFUSAL"}]) {
  test(`unsupported or rejected native archival emission never retries or publishes preceding EPUB: ${JSON.stringify(settings)}`, async t => {
    const {publisher, state} = publicPublisher(t, settings); const phases = [];
    await assert.rejects(publisher.render(chapters(), ["epub", "pdf", "site"], archive(), {
      strictLinks: true, onProgress: p => phases.push(p),
    }), code(settings.old ? "UNSUPPORTED_BOOK_PDFA" : "STRICT_ARCHIVAL_REFUSAL"));
    assert.equal(state.created, 2); assert.equal(state.terminated, 2);
    assert.equal(phases.some(p => p.phase === "retrying"), false);
    assert.equal(phases.some(p => p.phase === "verifying-publication"), false);
    assert.equal(publisher.busy, false);
  });
}

// Deliberately bypass worker validation to exercise the coordinator's own
// independent output admission. This is an injected client, not native code.
function endpoint(t, transform, {before, ...settings} = {}) {
  const state = {creates: 0, disposes: 0, calls: [], outputs: [], input: null};
  const publisher = createBookPublicationClient({ captureInput: prepareBookInput, ...settings,
    createClient() {
      state.creates++; let revision = 0;
      const emit = format => {
        state.calls.push(format); before?.(state, format);
        let value = {format: `book-${format}`, sourceLength: 10, retainedInputRevision: ++revision,
          bytes: Uint8Array.of(7, revision), ...(format === "pdf" && state.input.pdfA === "2b"
            ? {pdfA: "2b", pdfAStrict: state.input.pdfAStrict} : {})};
        value = transform?.(value, state) ?? value; state.outputs.push(value); return value;
      };
      return { render(_files, format, options) {state.input = options; return emit(format);},
        renderSourceUpdate(_changes, format) {return emit(format);}, dispose() {state.disposes++;} };
    },
  });
  t.after(() => publisher.dispose()); return {publisher, state};
}

for (const receipt of [{}, {pdfA: "off", pdfAStrict: true}, {pdfA: "2b"},
  {pdfA: "2b", pdfAStrict: false}, {pdfA: "2b", pdfAStrict: "true"}]) {
  test(`whole-edition admission independently rejects a missing/mismatched archival receipt: ${JSON.stringify(receipt)}`, async t => {
    const {publisher, state} = endpoint(t, value => {
      if (value.format !== "book-pdf") return value;
      const {pdfA, pdfAStrict, ...rest} = value; return {...rest, ...receipt};
    });
    await assert.rejects(publisher.render(chapters(), ["epub", "pdf", "site"], archive()), code("UNSUPPORTED_BOOK_PDFA"));
    assert.deepEqual(state.calls, ["epub", "pdf"]); assert.equal(state.creates, 1); assert.equal(state.disposes, 1);
    assert.equal(publisher.busy, false);
  });
}

for (const format of ["pdf", "epub", "site"]) test(`unrequested archival metadata is not attached to ${format}`, async t => {
  const {publisher, state} = endpoint(t, v => ({...v, pdfA: "2b", pdfAStrict: true}));
  await assert.rejects(publisher.render(chapters(), [format]), code("WORKER_PROTOCOL_ERROR"));
  assert.equal(state.disposes, 1);
});

test("policy is captured before a client can mutate option containers, rather than read back from returned metadata", async t => {
  const {publisher, state} = endpoint(t, undefined, { before(state, format) {
    if (format === "pdf") {state.input.pdfA = "off"; state.input.pdfAStrict = false;}
  } });
  await assert.rejects(publisher.render(chapters(), ["epub", "pdf"], archive()), code("UNSUPPORTED_BOOK_PDFA"));
  assert.equal(state.disposes, 1);
});

test("PDF receipt and its exact view remain independently owned after later outputs and disposal", async t => {
  const {publisher, state} = endpoint(t, value => ({...value, bytes: Uint8Array.of(1, 2, 3, 4).subarray(1, 3)}));
  const result = await publisher.render(chapters(), ["pdf", "epub"], archive());
  for (const value of state.outputs) value.bytes.fill(0);
  assert.deepEqual([...result.outputs[0].bytes], [2, 3]); assert.equal(result.outputs[0].pdfAStrict, true);
  assert.equal(result.byteLength, 4); assert.equal(state.disposes, 1);
});

test("ordinary editions remain compatible with older native packages and carry no archival receipt", async t => {
  const {publisher, state} = publicPublisher(t, {old: true});
  const result = await publisher.render(chapters(), ["pdf", "epub", "site"], {pdfA: "off", pdfAStrict: false});
  for (const output of result.outputs) { assert.equal(output.pdfA, undefined); assert.equal(output.pdfAStrict, undefined); }
  assert.equal(state.created, 1); assert.equal(state.terminated, 1);
});

test("non-PDF selections use existing exporters even when a saved policy requests strict archival PDF", async t => {
  const {publisher} = publicPublisher(t, {old: true});
  const result = await publisher.render(chapters(), ["epub", "site"], archive());
  assert.equal(result.outputs.length, 2);
  for (const output of result.outputs) assert.equal(output.pdfA, undefined);
});

test("strict navigation and archival policy remain separate, with cancellation before final verification releasing all output", async t => {
  const {publisher, state} = publicPublisher(t);
  await assert.rejects(publisher.render(chapters(), ["pdf", "epub", "site"], archive(), { strictLinks: true,
    onProgress(p) {if (p.phase === "verifying-publication") publisher.cancel();},
  }), code("EXPORT_CANCELLED"));
  assert.equal(state.created, 2); assert.equal(state.terminated, 2);
  assert.deepEqual(state.sent.filter(v => v.ordinal === 2).map(v => v.data.format), ["pdf", "epub", "site"]);
  assert.equal(publisher.busy, false);
});

test("aggregate output limits remain terminal for archival editions", async t => {
  const {publisher, state} = endpoint(t, undefined, {maxOutputBytes: 2});
  await assert.rejects(publisher.render(chapters(), ["pdf", "epub"], archive()), code("OUTPUT_LIMIT"));
  assert.deepEqual(state.calls, ["pdf"]); assert.equal(state.creates, 1); assert.equal(state.disposes, 1);
});
