// Production book facade, retained session and worker transport. NativeBook is
// an explicit ABI double: its deterministic JSON is NOT rendered PDF/EPUB/HTML.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { isMainThread, parentPort, workerData } from "node:worker_threads";
import { createBookBindings } from "../book_session.mjs";
import { createRetainedBook, installBookWorker } from "../book_worker.mjs";

export function nativeFixture({ counters, old = false } = {}) {
  const stats = { creates: 0, updates: 0, exports: 0, frees: 0 };
  const bump = (key, index) => { stats[key]++; if (counters) Atomics.add(counters, index, 1); };
  const asset = bytes => ({ size: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") });
  class NativeBook {
    constructor(paths, sources) {
      this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = []; this.settings = {}; this.images = []; this.fonts = [];
      this.sourceRevision = 0; this.dead = false; bump("creates", 0);
    }
    static fromSources(paths, sources, includedPaths, includedSources) {
      const book = new NativeBook(paths, sources);
      book.includes = includedPaths.map((path, i) => ({ path, source: includedSources[i] }));
      return book;
    }
    get chapterCount() { return this.files.length; }
    get sourceLength() { return [...this.files, ...this.includes].reduce((n, f) => n + Buffer.byteLength(f.source), 0); }
    setMetadata(...value) { this.settings.metadata = value; }
    setTheme(...value) { this.settings.theme = value; }
    setCustomCss(value) { this.settings.css = value; }
    setNavigation(...value) { this.settings.navigation = value; }
    setFontScale(value) { this.settings.fontScale = value; }
    setPdfOptions(...value) { this.settings.pdf = value; }
    setHtmlFontFormat(value) { this.settings.fontFormat = value; }
    setImage(destination, bytes) { this.images.push({ destination, ...asset(bytes) }); }
    setFont(slot, bytes) { this.fonts.push({ slot, ...asset(bytes) }); }
    setFontWeight(slot, weight) { this.fonts.find(font => font.slot === slot).weight = weight; }
    updateSources(paths, sources, expected) {
      assert(!this.dead); assert.equal(expected, this.sourceRevision);
      if (sources.includes("REJECT")) throw Object.assign(Error("Native fixture rejection"), { code: "FIXTURE_REJECT" });
      const all = [...this.files, ...this.includes];
      let changed = 0;
      for (let i = 0; i < paths.length; i++) {
        const file = all.find(file => file.path === paths[i]);
        assert(file, "selective native update must not create a path");
        if (file.source !== sources[i]) { changed++; file.source = sources[i]; }
      }
      if (changed) this.sourceRevision++;
      bump("updates", 1);
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        changed_sources: changed, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: sources.includes("BAD_RECEIPT") ? [this.chapterCount] : [] });
    }
    export(format, page) {
      assert(!this.dead); bump("exports", 2);
      if (counters && this.files.some(file => file.source === "BLOCK")) {
        Atomics.store(counters, 4, 1); Atomics.wait(counters, 5, 0, 10000);
      }
      if (this.files.some(file => file.source === "RENDER_FAIL"))
        throw Object.assign(Error("Native fixture rendering failure"), { code: "FIXTURE_RENDER" });
      return new TextEncoder().encode(JSON.stringify({ format, page, files: this.files, includes: this.includes,
        settings: this.settings, images: this.images, fonts: this.fonts }));
    }
    renderPdf() { return this.export("pdf"); }
    renderPdfWithPage(page) { return this.export("pdf", [...page]); }
    renderEpub() { return this.export("epub"); }
    renderSite() { return this.export("site"); }
    free() { assert(!this.dead, "double free"); this.dead = true; bump("frees", 3); }
  }
  if (old) NativeBook.prototype.updateSources = undefined;
  const engine = createBookBindings(async () => NativeBook);
  const preview = async api => api.renderBookSite(); // Routing double, not the HTML preview codec.
  const retained = createRetainedBook(engine, preview);
  return { stats, engine: { ...engine,
    renderBookPreview: (files, options) => preview({ renderBookSite: () => engine.renderBookSite(files, options) }),
    renderRetainedBook: retained.render, clearRetainedBook: retained.clear,
    renderRetainedBookPreview: (files, options) => retained.render(files, options, "preview"),
    clearRetainedBookPreview: retained.clear }, retained };
}

if (!isMainThread) {
  const { engine } = nativeFixture({ ...workerData, counters: new Int32Array(workerData.counters) });
  installBookWorker({
    addEventListener(kind, listener) { if (kind === "message") parentPort.on("message", data => listener({ data })); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  }, engine);
}
