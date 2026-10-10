// Real adapters and Node structured-clone/worker transport, with an explicit
// recording engine. The returned bytes describe calls, NOT rendered PDFs.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { createBookBindings, prepareBookInput, bookLinkOptions } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker, createRetainedBookPreview } from "../book_worker.mjs";

const FILES = [{ path: "guide.md", source: "# Guide\n\ntext" }];
const state = { created: 0, freed: 0, instances: [] };
class RecordingBook {
  constructor(paths, sources) {
    state.created++;
    state.instances.push(this);
    this.chapterCount = paths.length;
    this.sourceLength = sources.reduce((n, source) => n + new TextEncoder().encode(source).length, 0);
    this.settings = {};
  }
  setMetadata(...args) { this.settings.metadata = args; }
  setCustomCss(css) { this.settings.css = css; }
  setTheme(...args) { this.settings.theme = args; }
  setNavigation(...args) { this.settings.navigation = args; }
  setFontScale(scale) { this.settings.fontScale = scale; }
  setImage(destination, bytes) { (this.settings.images ??= []).push([destination, [...bytes]]); }
  setFont(slot, bytes) { (this.settings.fonts ??= []).push([slot, [...bytes]]); }
  setFontWeight(slot, weight) { this.settings.weight = [slot, weight]; }
  setPdfOptions(...args) { this.settings.pdf = args; }
  setPdfOptionsWithRunningImages(...args) {
    this.settings.pdf = args.slice(0, 12);
    this.settings.runningImages = args.slice(12).map(value => [...value]);
  }
  setHtmlFontFormat(format) { this.settings.htmlFontFormat = format; }
  renderPdf() { return new TextEncoder().encode(JSON.stringify(this.settings)); }
  renderPdfWithPage(page) { this.settings.page = [...page]; return this.renderPdf(); }
  renderEpub() { return this.renderPdf(); }
  renderSite() { return this.renderPdf(); }
  free() { assert.ok(!this.dead, "no double free"); this.dead = true; state.freed++; }
}
const api = createBookBindings(async () => RecordingBook);
const decoded = result => JSON.parse(new TextDecoder().decode(result.bytes));
const full = () => ({
  title: "Manual", author: "Writer", lang: "de", toc: true, pageNumbers: true,
  typography: "homogeneous, antiriver,pareto", optimalPagination: true, microtype: "expansion",
  baseFontSize: 12, headingScale: 1.3, tableFontSize: 9, tocDepth: 3, fitToPages: 4,
  codeLineNumbers: true, metadataEpochSeconds: 0, htmlFontFormat: "woff2",
  running: { header: { left: "{title}", right: "{author}", rule: true },
    footer: { center: "{page}/{pages}", right: "{date}", rule: true }, skipFirstPage: true },
  page: { size: "letter" },
});
const PDF_ARGS = ["homogeneous,antiriver,pareto,optimal-pagination,expansion",
  12, 1.3, 9, 3, 4, true, 0, ["{title}", "", "{author}", "", "{page}/{pages}", "{date}"], true, true, true];

if (!isMainThread) {
  if (workerData?.old) delete RecordingBook.prototype.setPdfOptions;
  if (workerData?.oldImages) delete RecordingBook.prototype.setPdfOptionsWithRunningImages;
  installBookWorker({
    addEventListener(type, listener) { parentPort.on(type, data => listener({ data })); },
    postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
  }, api);
} else {
  test("book logos use the additive settings transaction and retain exact host assets", async () => {
    let ready;
    const delayed = createBookBindings(() => new Promise(resolve => { ready = resolve; }));
    const bytes = new Uint8Array([99, 1, 2, 3, 88]);
    const options = { ...full(), images: [{ destination: "logo.svg", bytes: bytes.subarray(1, 4) }] };
    options.running.header.image = { dest: "logo.svg", position: "right", heightPt: 24 };
    options.running.footer.image = { dest: "logo.svg" };
    const admitted = prepareBookInput(FILES, options);
    assert(Object.isFrozen(admitted.options.running.header.image));
    assert.deepEqual(prepareBookInput(admitted.files, structuredClone(admitted.options)), admitted);
    const pending = delayed.createBook(FILES, options);
    options.running.header.image.dest = "changed.svg";
    options.running.header.image.heightPt = 30;
    options.images[0].destination = "changed.svg";
    bytes.fill(9);
    ready(RecordingBook);
    const session = await pending;
    const result = decoded(session.renderPdf());
    assert.deepEqual(result.pdf, PDF_ARGS);
    assert.deepEqual(result.runningImages, [["logo.svg", "logo.svg"], [1, 0], [24, 0]]);
    assert.deepEqual(result.images, [["logo.svg", [1, 2, 3]]]);
    session.dispose();
    assert.equal(bytes.byteLength, 5);
  });

  test("book image options reject malformed data and unsupported packages before construction", async () => {
    let getterCalls = 0;
    const accessor = Object.defineProperty({}, "dest", { get() { getterCalls++; return "logo"; } });
    for (const image of [null, [], {}, accessor, { dest: " " }, { dest: "\ud800" },
      { dest: "logo", position: "center" }, { dest: "logo", heightPt: 0 },
      { dest: "logo", heightPt: 1.5 }, { dest: "logo", heightPt: 65536 },
      { dest: "logo", bytes: new Uint8Array([1]) }]) {
      assert.throws(() => prepareBookInput(FILES, { running: { header: { image } } }));
    }
    assert.equal(getterCalls, 0);
    class OldBook extends RecordingBook {}
    Object.defineProperty(OldBook.prototype, "setPdfOptionsWithRunningImages", { value: undefined });
    const old = createBookBindings(async () => OldBook);
    const created = state.created;
    await assert.rejects(old.createBook(FILES, { running: { header: { image: { dest: "logo" } } } }),
      error => error.code === "UNSUPPORTED_BOOK_OPTIONS");
    assert.equal(state.created, created);
    const text = await old.createBook(FILES, { running: { footer: { center: "{page}" } } });
    text.dispose();
  });

  test("book PDF settings and running bands reach the raw method in exact ABI order", async () => {
    const session = await api.createBook(FILES, full());
    const result = decoded(session.renderPdf());
    assert.deepEqual(result.pdf, PDF_ARGS);
    assert.deepEqual(result.metadata, ["Manual", "Writer", "de"]);
    assert.deepEqual(result.navigation, [true, true]);
    assert.deepEqual(result.page, [612, 792, 72, 72, 72, 72]);
    assert.equal(result.htmlFontFormat, "woff2");
    const created = state.created;
    assert.equal(decoded(session.renderSite()).htmlFontFormat, "woff2");
    session.renderEpub();
    assert.equal(state.created, created, "all exports reuse the captured book");
    session.dispose();
  });

  test("canonical render settings survive repeated admission and structured cloning", () => {
    const once = prepareBookInput(FILES, full());
    const clone = structuredClone(once);
    const twice = prepareBookInput(clone.files, clone.options);
    const three = prepareBookInput(twice.files, twice.options);
    assert.deepEqual(twice, once);
    assert.deepEqual(three, once);
    assert.equal(Object.isFrozen(once.options.running), true);
    assert.equal(Object.isFrozen(once.options.running.header), true);
  });

  test("source and settings are captured before asynchronous engine initialization", async () => {
    let ready;
    const delayed = createBookBindings(() => new Promise(resolve => { ready = resolve; }));
    const options = full(), files = structuredClone(FILES);
    const bytes = Uint8Array.of(1, 2, 3);
    options.images = [{ destination: "image.png", bytes }];
    const pending = delayed.createBook(files, options);
    options.typography = "unknown";
    options.baseFontSize = 99;
    options.running.header.left = "changed";
    options.page.size = "a4";
    options.htmlFontFormat = "ttf";
    files[0].source = "changed";
    bytes.fill(9);
    ready(RecordingBook);
    const session = await pending;
    const result = decoded(session.renderPdf());
    assert.deepEqual(result.pdf, PDF_ARGS);
    assert.equal(result.htmlFontFormat, "woff2");
    assert.equal(session.sourceLength, new TextEncoder().encode(FILES[0].source).length);
    assert.deepEqual(result.images, [["image.png", [1, 2, 3]]]);
    assert.deepEqual(result.page, [612, 792, 72, 72, 72, 72]);
    session.dispose();
  });

  test("each caller-controlled option is read once", () => {
    const supplied = full(), reads = new Map(), options = {};
    for (const [name, value] of Object.entries(supplied)) Object.defineProperty(options, name, {
      get() { reads.set(name, (reads.get(name) ?? 0) + 1); return reads.get(name) === 1 ? value : "invalid second value"; },
    });
    const captured = prepareBookInput(FILES, options);
    assert.equal(captured.options.typography, PDF_ARGS[0]);
    assert.ok([...reads.values()].every(count => count === 1));
  });

  test("invalid quality inputs fail before loading the engine or inspecting image bytes", async () => {
    let loads = 0;
    const guarded = createBookBindings(async () => { loads++; return RecordingBook; });
    for (const option of [
      { typography: ["pareto"] }, { typography: "pareto,misspelled" }, { typography: " ".repeat(257) },
      { optimalPagination: "false" }, { microtype: "unknown" }, { microtypeProtrusion: 1 },
      { baseFontSize: 5.999999999 }, { headingScale: 2.000000001 }, { tableFontSize: NaN },
      { tocDepth: 1.5 }, { fitToPages: 0 }, { fitToPages: 0x100000000 },
      { metadataEpochSeconds: -1 }, { metadataEpochSeconds: Number.MAX_SAFE_INTEGER + 1 },
      { codeLineNumbers: null }, { htmlFontFormat: "unknown" },
    ]) {
      Object.defineProperty(option, "images", { get() { assert.fail("invalid settings must precede asset admission"); } });
      await assert.rejects(guarded.createBook(FILES, option));
    }
    assert.equal(loads, 0);
  });

  test("running templates enforce UTF-8 limits, strict booleans and supported data keys", () => {
    for (const running of [
      [], { header: "wrong" }, { footer: { unknown: "x" } }, { skipFirstPage: 1 },
      { header: { left: 42 } }, { header: { left: "\ud800" } },
      { header: { left: "é".repeat(2049) } }, { footer: { rule: "false" } },
    ]) assert.throws(() => prepareBookInput(FILES, { running }));
    let touched = false;
    assert.throws(() => prepareBookInput(FILES, { running: { header: {
      get left() { touched = true; return "not a plain data value"; },
    } } }), /accessor/);
    assert.equal(touched, false);
    assert.equal(prepareBookInput(FILES, { running: { header: { left: "é".repeat(2048) } } })
      .options.running.header.left.length, 2048);
  });

  test("skip-first alone is retained for the page-number footer", async () => {
    const result = decoded(await api.renderBookPdf(FILES, { pageNumbers: true, running: { skipFirstPage: true } }));
    assert.equal(result.pdf[11], true);
    assert.deepEqual(result.pdf[8], ["", "", "", "", "", ""]);
    assert.deepEqual(result.navigation, [false, true]);
  });

  test("microtype modes override only microtype tokens and preserve other typography flags", async () => {
    for (const [mode, expected] of [["off", "pareto"], ["expansion", "pareto,expansion"], ["all", "pareto,protrusion"]]) {
      const result = decoded(await api.renderBookPdf(FILES, { typography: "pareto,protrusion", microtype: mode }));
      assert.equal(result.pdf[0], expected);
    }
    assert.equal(decoded(await api.renderBookPdf(FILES, { microtypeProtrusion: true })).pdf[0], "protrusion");
  });

  test("old packages retain default calls but cannot silently ignore requested options", async () => {
    class Old extends RecordingBook {}
    Object.defineProperty(Old.prototype, "setPdfOptions", { value: undefined });
    Object.defineProperty(Old.prototype, "setHtmlFontFormat", { value: undefined });
    const old = createBookBindings(async () => Old);
    const defaultOutput = decoded(await old.renderBookPdf(FILES, {
      typography: "", optimalPagination: false, microtype: "off", codeLineNumbers: false, running: {},
    }));
    assert.equal(defaultOutput.pdf, undefined);
    const before = state.created;
    for (const options of [{ typography: "pareto" }, { tocDepth: 2 }, { htmlFontFormat: "ttf" },
      { running: { header: { left: "Title" } } }, { metadataEpochSeconds: 0 }]) {
      await assert.rejects(old.createBook(FILES, options), error => error.code === "UNSUPPORTED_BOOK_OPTIONS");
    }
    assert.equal(state.created, before, "missing capability fails before native allocation");
  });

  test("failed raw option configuration frees the book and keeps a bounded error reason", async () => {
    class Refusing extends RecordingBook { setPdfOptions() { throw "profile rejected: " + "x".repeat(3000); } }
    const refusing = createBookBindings(async () => Refusing);
    const before = state.freed;
    await assert.rejects(refusing.createBook(FILES, { typography: "pareto" }), error => {
      assert.ok(error instanceof Error);
      assert.ok(error.message.startsWith("profile rejected"));
      assert.equal(error.message.length, 2048);
      return true;
    });
    assert.equal(state.freed, before + 1);
    await api.renderBookPdf(FILES, { typography: "pareto" });
  });

  test("render-only page override cannot erase retained typography or running bands", async () => {
    const session = await api.createBook(FILES, full());
    const before = decoded(session.renderPdf());
    const changed = decoded(session.renderPdf({ page: { size: "a4", orientation: "landscape", margins: 36 } }));
    assert.deepEqual(changed.pdf, before.pdf);
    assert.notDeepEqual(changed.page, before.page);
    assert.deepEqual(decoded(session.renderPdf()).page, before.page);
    assert.throws(() => session.renderPdf({ typography: "pareto" }), /only page/);
    session.dispose();
  });

  test("links-only admission never inspects rendering configuration getters", () => {
    const options = {};
    for (const name of ["typography", "running", "baseFontSize", "metadataEpochSeconds", "htmlFontFormat"])
      Object.defineProperty(options, name, { get() { assert.fail(`link check read ${name}`); } });
    assert.doesNotThrow(() => prepareBookInput(FILES, bookLinkOptions(options)));
  });

  test("all numeric boundaries survive admission without coercion", () => {
    const low = prepareBookInput(FILES, { baseFontSize: 6, headingScale: 1.05, tableFontSize: 5, tocDepth: 1,
      fitToPages: 1, metadataEpochSeconds: 0 }).options;
    assert.equal(low.baseFontSize, 6); assert.equal(low.metadataEpochSeconds, 0);
    const high = prepareBookInput(FILES, { baseFontSize: 24, headingScale: 2, tableFontSize: 24, tocDepth: 6,
      fitToPages: 0xffffffff, metadataEpochSeconds: Number.MAX_SAFE_INTEGER }).options;
    assert.equal(high.fitToPages, 0xffffffff); assert.equal(high.metadataEpochSeconds, Number.MAX_SAFE_INTEGER);
    for (const value of [Infinity, -Infinity, NaN, "12", null]) assert.throws(() => prepareBookInput(FILES, { baseFontSize: value }));
  });

  function workerEndpoint(old = false, oldImages = false) {
    const worker = new Worker(new URL(import.meta.url), { workerData: { old, oldImages } });
    const callbacks = new Map();
    return {
      postMessage: (message, transfer) => worker.postMessage(message, transfer),
      terminate: () => worker.terminate(),
      addEventListener(type, listener) {
        const wrapped = type === "message" ? data => listener({ data }) : error => listener({ error });
        callbacks.set(listener, wrapped); worker.on(type, wrapped);
      },
      removeEventListener(type, listener) { const callback = callbacks.get(listener); if (callback) worker.off(type, callback); callbacks.delete(listener); },
    };
  }
  test("real worker transport preserves every setting through client, handler and session", async () => {
    const client = createBookWorkerClient({ workerFactory: () => workerEndpoint(), timeoutMs: 10000 });
    try {
      const options = full(), bytes = Uint8Array.of(4, 5, 6);
      options.images = [{ destination: "figure.png", bytes }];
      const pending = client.render(FILES, "pdf", options);
      options.running.header.left = "too late";
      options.metadataEpochSeconds = 100;
      const result = decoded(await pending);
      assert.deepEqual(result.pdf, PDF_ARGS);
      assert.equal(result.htmlFontFormat, "woff2");
      assert.deepEqual(result.images, [["figure.png", [4, 5, 6]]]);
      assert.deepEqual([...bytes], [4, 5, 6], "host bytes are not transferred");
      assert.equal(client.busy, false);
    } finally { client.dispose(); }
  });

  test("missing book option capability propagates across a real worker and releases its slot", async () => {
    const client = createBookWorkerClient({ workerFactory: () => workerEndpoint(true), timeoutMs: 10000 });
    try {
      await assert.rejects(client.render(FILES, "pdf", { typography: "pareto" }),
        error => error.code === "UNSUPPORTED_BOOK_OPTIONS" && /setPdfOptions/.test(error.message));
      assert.equal(client.busy, false);
      assert.equal(decoded(await client.render(FILES, "pdf")).pdf, undefined);
    } finally { client.dispose(); }
  });

  test("retained book logo configuration survives a real worker and rejects an old image binding", async () => {
    const client = createBookWorkerClient({ workerFactory: () => workerEndpoint(), timeoutMs: 10000 });
    try {
      const bytes = new Uint8Array([3, 4]);
      const options = { images: [{ destination: "logo.svg", bytes }],
        running: { header: { image: { dest: "logo.svg", position: "right", heightPt: 24 } } } };
      const pending = client.render(FILES, "pdf", options);
      options.running.header.image.dest = "late.svg";
      bytes.fill(9);
      const result = decoded(await pending);
      assert.deepEqual(result.runningImages, [["logo.svg", ""], [1, 0], [24, 0]]);
      assert.deepEqual(result.images, [["logo.svg", [3, 4]]]);
    } finally { client.dispose(); }
    const old = createBookWorkerClient({ workerFactory: () => workerEndpoint(false, true), timeoutMs: 10000 });
    try {
      await assert.rejects(old.render(FILES, "pdf", { running: { header: { image: { dest: "logo" } } } }),
        error => error.code === "UNSUPPORTED_BOOK_OPTIONS");
      assert.equal(old.busy, false);
      assert.equal(decoded(await old.render(FILES, "pdf")).pdf, undefined);
    } finally { old.dispose(); }
  });

  test("retained preview invalidation includes every new publishing option", async () => {
    const preview = createRetainedBookPreview(api, (engine) => engine.renderBookSite());
    let prior = state.created;
    const variants = [{}, { htmlFontFormat: "woff2" }, { htmlFontFormat: "ttf" },
      { tocDepth: 2 }, { tocDepth: 3 }, { typography: "pareto" }, { typography: "antiriver" },
      { baseFontSize: 12 }, { headingScale: 1.3 }, { tableFontSize: 9 }, { fitToPages: 3 },
      { codeLineNumbers: true }, { metadataEpochSeconds: 0 },
      { running: { header: { left: "First" } } }, { running: { header: { left: "Second" } } },
      { running: { header: { image: { dest: "logo" } } } },
      { running: { header: { image: { dest: "logo", position: "right", heightPt: 24 } } } }];
    try {
      for (const options of variants) {
        const input = prepareBookInput(FILES, options);
        await preview.render(input.files, input.options);
        assert.equal(state.created, ++prior, JSON.stringify(options));
        const identical = prepareBookInput(FILES, options);
        await preview.render(identical.files, identical.options);
        assert.equal(state.created, prior, "identical owned configuration reuses the session");
      }
    } finally { preview.clear(); }
  });
}
