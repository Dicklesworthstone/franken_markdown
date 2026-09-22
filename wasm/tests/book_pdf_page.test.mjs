// Production book admission/session/worker code with an explicit engine double.
// Worker tests use real Node threads and structured-clone transfers. They do
// NOT execute compiled WASM or assert that the double's bytes are real PDFs.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { bookLinkOptions, createBookBindings, prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { normalizePdfPage, pdfPageGeometry } from "../pdf_page.mjs";

const files = () => [{ path: "guide/one.md", source: "# One" }, { path: "two.md", source: "# Two" }];
const custom = () => ({
  size: { widthPt: 720, heightPt: 540 },
  margins: { topPt: 18, rightPt: 24, bottomPt: 30, leftPt: 36 },
});
const decode = (output) => JSON.parse(new TextDecoder().decode(output.bytes));
const encoded = (value) => new TextEncoder().encode(JSON.stringify(value));

function fixture({ legacy = false, load = async () => {}, fail = false } = {}) {
  const state = { loads: 0, created: 0, freed: 0, calls: [], images: [], fonts: [] };
  class EngineBook {
    constructor(paths, sources) {
      state.created++;
      state.paths = paths;
      state.sources = sources;
      this.chapterCount = paths.length;
      this.sourceLength = sources.reduce((sum, s) => sum + new TextEncoder().encode(s).length, 0);
    }
    setMetadata(...args) { state.metadata = args; }
    setCustomCss(css) { state.css = css; }
    setTheme(...args) { state.theme = args; }
    setNavigation(...args) { state.navigation = args; }
    setFontScale(scale) { state.scale = scale; }
    setImage(destination, bytes) { state.images.push([destination, Array.from(bytes)]); }
    setFont(slot, bytes) { state.fonts.push([slot, Array.from(bytes)]); }
    setFontWeight(slot, weight) { state.weight = [slot, weight]; }
    renderPdf(...args) {
      assert.equal(args.length, 0, "legacy ABI must remain argument-free");
      state.calls.push("legacy");
      return encoded("legacy");
    }
    renderPdfWithPage(geometry) {
      assert.ok(geometry instanceof Float64Array);
      state.calls.push(Array.from(geometry));
      if (fail) throw "native book render failed";
      return encoded(Array.from(geometry));
    }
    renderEpub() { state.calls.push("epub"); return encoded("epub"); }
    renderSite() { state.calls.push("site"); return encoded("site"); }
    free() { state.freed++; }
  }
  if (legacy) EngineBook.prototype.renderPdfWithPage = undefined;
  const api = createBookBindings(async () => { state.loads++; await load(); return EngineBook; });
  return { api, state };
}

test("legacy one-shot and reusable calls keep their old ABI and disposal", async () => {
  const { api, state } = fixture({ legacy: true });
  assert.equal(decode(await api.renderBookPdf(files())), "legacy");
  assert.equal(state.freed, 1);
  const session = await api.createBook(files());
  assert.equal(decode(session.renderPdf()), "legacy");
  assert.equal(decode(session.renderPdf({ page: undefined })), "legacy");
  session.dispose();
  session.dispose();
  assert.equal(state.freed, 2);
});

test("one-shot custom geometry preserves shared rendering settings and output ownership", async () => {
  const { api, state } = fixture();
  const output = await api.renderBookPdf(files(), {
    page: custom(), title: "Manual", author: "Author", lang: "de", font: "serif",
    toc: true, pageNumbers: true, fontScale: 1.125,
    images: [{ destination: "guide/figure.svg", bytes: new Uint8Array([1, 2]) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]), weight: 550 }],
  });
  assert.deepEqual(decode(output), [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(state.metadata, ["Manual", "Author", "de"]);
  assert.deepEqual(state.theme, ["serif", "auto"]);
  assert.deepEqual(state.navigation, [true, true]);
  assert.equal(state.scale, 1.125);
  assert.deepEqual(state.images, [["guide/figure.svg", [1, 2]]]);
  assert.deepEqual(state.fonts, [["body-regular", [3, 4]]]);
  assert.deepEqual(state.weight, ["body-regular", 550]);
  assert.equal(output.format, "book-pdf");
  assert.equal(output.mimeType, "application/pdf");
  assert.equal(output.filename("manual"), "manual.pdf");
  assert.equal(state.freed, 1);
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
});

test("creation snapshots source, geometry, margins and assets before initialization", async () => {
  let release;
  const { api, state } = fixture({ load: () => new Promise((resolve) => { release = resolve; }) });
  const page = custom(), chapters = files(), bytes = new Uint8Array([9, 8]);
  let reads = 0;
  const pending = api.createBook(chapters, {
    get page() { reads++; return page; },
    images: [{ destination: "a.svg", bytes }],
  });
  page.size.widthPt = 900;
  page.margins.leftPt = 200;
  chapters[0].source = "changed";
  bytes.fill(0);
  release();
  const session = await pending;
  assert.equal(reads, 1);
  assert.deepEqual(decode(session.renderPdf()), [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(state.sources, ["# One", "# Two"]);
  assert.deepEqual(state.images, [["a.svg", [9, 8]]]);
  session.dispose();
});

test("per-export overrides neither reparse nor replace a session's default page", async () => {
  const { api, state } = fixture();
  const session = await api.createBook(files(), { page: custom() });
  assert.deepEqual(decode(session.renderPdf({ page: { size: "letter", orientation: "landscape", margins: 0 } })),
    [792, 612, 0, 0, 0, 0]);
  assert.deepEqual(decode(session.renderPdf()), [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(decode(session.renderPdf({ page: {} })), [612, 792, 72, 72, 72, 72]);
  assert.deepEqual(decode(session.renderPdf()), [720, 540, 18, 24, 30, 36]);
  assert.equal(decode(session.renderEpub()), "epub");
  assert.equal(decode(session.renderSite()), "site");
  assert.equal(state.created, 1);
  assert.equal(session.chapterCount, 2);
  session.dispose();
});

test("an unconfigured session returns to the legacy ABI after a one-off override", async () => {
  const { api, state } = fixture();
  const session = await api.createBook(files());
  session.renderPdf({ page: custom() });
  assert.equal(decode(session.renderPdf()), "legacy");
  assert.equal(state.created, 1);
  session.dispose();
});

test("named A4 paper, orientation and independent margin defaults reach the ABI", async () => {
  const { api } = fixture();
  const output = await api.renderBookPdf(files(), {
    page: { size: "a4", orientation: "landscape", margins: { leftPt: 24, bottomPt: 0 } },
  });
  assert.deepEqual(decode(output), [297 * 72 / 25.4, 210 * 72 / 25.4, 72, 72, 0, 24]);
});

test("page admission is stable across repeated preparation and structured clones", () => {
  const first = prepareBookInput(files(), { page: custom() });
  const wire = structuredClone(first);
  const second = prepareBookInput(wire.files, wire.options);
  assert.deepEqual(first, second);
  for (const value of [second.options.page, second.options.page.size, second.options.page.margins])
    assert.equal(Object.isFrozen(value), true);
  assert.notEqual(first.options.page, second.options.page);
});

test("invalid paper is rejected before engine loading, including f32 boundary collapse", async () => {
  const { api, state } = fixture();
  for (const page of [
    null, [], "a4", { size: "A4" }, { unknown: 1 }, { size: { widthPt: 612 } },
    { margins: "36" }, { margins: -1 }, { size: { widthPt: Infinity, heightPt: 792 } },
    { size: { widthPt: 143.999999, heightPt: 792 } },
    { margins: { leftPt: 270, rightPt: 270.000001 } },
  ]) {
    await assert.rejects(api.createBook(files(), { page }), { code: "INVALID_OPTIONS" });
  }
  assert.equal(state.loads, 0);
  assert.equal(state.created, 0);
});

test("export options reject misspellings and accessors without invoking them", async () => {
  const { api, state } = fixture();
  const session = await api.createBook(files());
  let accessed = false;
  for (const options of [null, [], { pages: custom() }, { font: "serif" },
    { get page() { accessed = true; return custom(); } }, { [Symbol("page")]: custom() }]) {
    assert.throws(() => session.renderPdf(options), { code: "INVALID_OPTIONS" });
  }
  assert.equal(accessed, false);
  assert.deepEqual(state.calls, []);
  session.dispose();
});

test("reentrant disposal during option inspection never uses a freed raw handle", async () => {
  const { api, state } = fixture();
  const session = await api.createBook(files());
  const options = new Proxy({ page: custom() }, {
    ownKeys(target) { session.dispose(); return Reflect.ownKeys(target); },
  });
  assert.throws(() => session.renderPdf(options), /disposed/);
  assert.deepEqual(state.calls, []);
  assert.equal(state.freed, 1);
  assert.throws(() => session.renderPdf(), /disposed/);
});

test("old WASM never silently drops requested geometry and one-shot failures free it", async () => {
  const { api, state } = fixture({ legacy: true });
  await assert.rejects(api.renderBookPdf(files(), { page: custom() }), {
    code: "UNSUPPORTED_PDF_PAGE",
  });
  assert.equal(state.freed, 1);
  assert.deepEqual(state.calls, []);
  assert.equal(decode(await api.renderBookPdf(files())), "legacy");
});

test("native failure stays bounded, frees one-shot state, and cannot replace a default", async () => {
  const { api, state } = fixture({ fail: true });
  await assert.rejects(api.renderBookPdf(files(), { page: custom() }), /native book render failed/);
  assert.equal(state.freed, 1);
  const session = await api.createBook(files());
  assert.throws(() => session.renderPdf({ page: custom() }), /native book render failed/);
  assert.equal(decode(session.renderPdf()), "legacy");
  session.dispose();
});

test("link-only input preparation does not inspect presentation or page getters", () => {
  const settings = { get page() { throw new Error("page should not be read"); } };
  assert.equal(prepareBookInput(files(), bookLinkOptions(settings)).options.page, undefined);
});

test("1000 seeded paper configurations retain exact values through book admission", () => {
  let seed = 1729;
  const random = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 2 ** 32);
  for (let i = 0; i < 1000; i++) {
    const widthPt = 200 + 2000 * random(), heightPt = 200 + 2000 * random();
    const topPt = (heightPt - 72) * random() / 3, bottomPt = (heightPt - 72) * random() / 3;
    const leftPt = (widthPt - 72) * random() / 3, rightPt = (widthPt - 72) * random() / 3;
    const page = { size: { widthPt, heightPt }, margins: { topPt, rightPt, bottomPt, leftPt } };
    const admitted = prepareBookInput(files(), { page }).options.page;
    assert.deepEqual(admitted, normalizePdfPage(page));
    assert.deepEqual(Array.from(pdfPageGeometry(admitted)), [widthPt, heightPt, topPt, rightPt, bottomPt, leftPt]);
  }
});

function threadedClient(legacy = false) {
  const state = { workers: 0, terminated: 0 };
  const code = `
    import { parentPort } from 'node:worker_threads';
    import { installBookWorker } from ${JSON.stringify(new URL("../book_worker.mjs", import.meta.url).href)};
    import { createBookBindings } from ${JSON.stringify(new URL("../book_session.mjs", import.meta.url).href)};
    class EngineBook {
      constructor(paths, sources) {
        this.chapterCount = paths.length;
        this.sourceLength = sources.reduce((sum, s) => sum + new TextEncoder().encode(s).length, 0);
      }
      setMetadata() {} setCustomCss() {} setTheme() {} setNavigation() {}
      setFontScale() {} setImage() {} setFont() {} setFontWeight() {} free() {}
      renderPdf() { return new TextEncoder().encode(JSON.stringify('legacy')); }
      renderPdfWithPage(page) { return new TextEncoder().encode(JSON.stringify(Array.from(page))); }
    }
    if (${legacy}) EngineBook.prototype.renderPdfWithPage = undefined;
    installBookWorker({
      addEventListener(kind, listener) { parentPort.on(kind, data => listener({ data })); },
      postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
    }, createBookBindings(async () => EngineBook));
  `;
  const client = createBookWorkerClient({
    workerFactory() {
      state.workers++;
      const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(code)}`));
      const listeners = new Map();
      return {
        postMessage(data, transfer) { worker.postMessage(data, transfer); },
        addEventListener(kind, listener) {
          const wrapped = kind === "message" ? data => listener({ data }) : error => listener({ error });
          listeners.set(listener, wrapped);
          worker.on(kind, wrapped);
        },
        removeEventListener(kind, listener) {
          const wrapped = listeners.get(listener);
          if (wrapped) worker.off(kind, wrapped);
          listeners.delete(listener);
        },
        terminate() { state.terminated++; return worker.terminate(); },
      };
    },
    timeoutMs: 10000,
  });
  return { client, state };
}

test("real worker transport delivers captured geometry to the book ABI", async (t) => {
  const { client, state } = threadedClient();
  t.after(() => client.dispose());
  const page = custom();
  const pending = client.render(files(), "pdf", { page });
  page.size.widthPt = 1000;
  page.margins.leftPt = 300;
  assert.deepEqual(decode(await pending), [720, 540, 18, 24, 30, 36]);
  assert.equal(client.busy, false);
  assert.equal(state.terminated, 1);
});

test("worker rejects bad geometry before allocation and remains reusable", async (t) => {
  const { client, state } = threadedClient();
  t.after(() => client.dispose());
  await assert.rejects(client.render(files(), "pdf", { page: { margins: 400 } }), { code: "INVALID_OPTIONS" });
  assert.equal(state.workers, 0);
  assert.equal(client.busy, false);
  assert.equal(decode(await client.render(files(), "pdf")), "legacy");
});

test("worker preserves missing-ABI errors, cleans up, then accepts legacy work", async (t) => {
  const { client, state } = threadedClient(true);
  t.after(() => client.dispose());
  await assert.rejects(client.render(files(), "pdf", { page: custom() }), { code: "UNSUPPORTED_PDF_PAGE" });
  assert.equal(state.terminated, 1);
  assert.equal(decode(await client.render(files(), "pdf")), "legacy");
  assert.equal(state.terminated, 2);
});

test("cancellation terminates a page-configured worker without retaining a busy slot", async (t) => {
  const { client, state } = threadedClient();
  t.after(() => client.dispose());
  const pending = client.render(files(), "pdf", { page: custom() });
  client.cancel();
  await assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  assert.equal(state.terminated, 1);
  assert.equal(client.busy, false);
  assert.deepEqual(decode(await client.render(files(), "pdf", { page: custom() })), [720, 540, 18, 24, 30, 36]);
});

test("worker-side admission rejects forged invalid geometry before invoking the engine", async () => {
  let receive, calls = 0;
  const replies = [];
  installBookWorker({
    addEventListener(_kind, listener) { receive = listener; },
    postMessage(reply) { replies.push(reply); },
  }, {
    async renderBookPdf(_files, options) { calls++; return { bytes: encoded(options.page), sourceLength: 10 }; },
  });
  const data = { schemaVersion: 1, id: 1, format: "pdf", files: files(), maxOutputBytes: 4096,
    options: { page: { size: { widthPt: 612, heightPt: 792 }, margins: { leftPt: 270, rightPt: 270.000001 } } } };
  await receive({ data });
  assert.equal(calls, 0);
  assert.equal(replies[0].error.code, "INVALID_OPTIONS");
  await receive({ data: { ...data, id: 2, options: { page: custom() } } });
  assert.equal(calls, 1);
  assert.deepEqual(decode(replies[1]), normalizePdfPage(custom()));
});
