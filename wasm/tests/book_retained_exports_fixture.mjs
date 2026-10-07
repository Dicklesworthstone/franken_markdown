// The real facade, worker protocol, source receipts and owned-buffer transfers
// run around this explicit native-ABI double. Payloads are JSON, NOT PDF/EPUB.
import assert from "node:assert/strict";
import { isMainThread, parentPort, workerData } from "node:worker_threads";
import { createBookBindings } from "../book_session.mjs";
import { createRetainedBook, installBookWorker } from "../book_worker.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const failure = code => Object.assign(new Error(code), { code });
export function fixture({ counters, old = false, noSets = false } = {}) {
  const stats = { creates: 0, updates: 0, replacements: 0, frees: 0, instances: [], calls: [] };
  const bump = (key, index) => { stats[key]++; if (counters) Atomics.add(counters, index, 1); };
  const files = (paths, sources) => paths.map((path, i) => ({ path, source: sources[i] }));
  class NativeBook {
    constructor(paths, sources) {
      bump("creates", 0); stats.instances.push(this);
      this.files = files(paths, sources); this.includes = []; this.sourceRevision = 0;
      this.settings = {}; this.images = []; this.fonts = []; this.dead = false;
    }
    static fromSources(paths, sources, includedPaths, includedSources) {
      const book = new NativeBook(paths, sources);
      book.includes = files(includedPaths, includedSources);
      return book;
    }
    get chapterCount() { return this.files.length; }
    get sourceLength() { return [...this.files, ...this.includes].reduce((n, f) => n + Buffer.byteLength(f.source), 0); }
    setMetadata(...value) { this.settings.metadata = value; }
    setCustomCss(value) { this.settings.css = value; }
    setTheme(...value) { this.settings.theme = value; }
    setNavigation(...value) { this.settings.navigation = value; }
    setFontScale(value) { this.settings.scale = value; }
    setPdfOptions(...value) { this.settings.pdf = value; }
    setHtmlFontFormat(value) { this.settings.fontFormat = value; }
    setImage(destination, bytes) { this.images.push({ destination, bytes: [...bytes] }); }
    setFont(slot, bytes) { this.fonts.push({ slot, bytes: [...bytes] }); }
    setFontWeight(slot, weight) { this.fonts.find(f => f.slot === slot).weight = weight; }
    updateSources(paths, sources, expected) {
      assert(!this.dead); assert.equal(expected, this.sourceRevision);
      if (sources.includes("REJECT")) throw failure("FIXTURE_REJECT");
      bump("updates", 1); stats.calls.push({ method: "update", paths, expected });
      const all = [...this.files, ...this.includes];
      let changed = 0;
      for (const next of files(paths, sources)) {
        const old = all.find(f => f.path === next.path);
        assert(old, "selective edit cannot add a source");
        if (old.source !== next.source) { old.source = next.source; changed++; }
      }
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        changed_sources: changed, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: sources.includes("BAD_RECEIPT") ? [this.chapterCount] : [] });
    }
    replaceSources(paths, sources, includedPaths, includedSources, expected) {
      assert(!this.dead); assert.equal(expected, this.sourceRevision);
      if (sources.includes("REJECT") || includedSources.includes("REJECT")) throw failure("FIXTURE_REJECT");
      bump("replacements", 2); stats.calls.push({ method: "replace", paths, includedPaths, expected });
      const next = files(paths, sources), includes = files(includedPaths, includedSources);
      const changed = JSON.stringify([this.files, this.includes]) !== JSON.stringify([next, includes]);
      this.files = next; this.includes = includes;
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-set-v1", revision: this.sourceRevision,
        changed, chapter_count: this.chapterCount, resource_count: this.includes.length,
        source_length: this.sourceLength,
        reparsed_chapter_count: sources.includes("BAD_RECEIPT") ? 9999 : changed ? this.chapterCount : 0 });
    }
    payload(format, page) {
      assert(!this.dead);
      if (this.files.some(f => f.source === "RENDER_FAIL")) throw failure("FIXTURE_RENDER");
      if (counters && this.files.some(f => f.source === "BLOCK")) {
        Atomics.store(counters, 4, 1);
        Atomics.wait(counters, 5, 0, 10000);
      }
      return encode({ format, page, files: this.files, includes: this.includes,
        settings: this.settings, images: this.images, fonts: this.fonts });
    }
    renderPdf() { return this.payload("pdf"); }
    renderPdfWithPage(page) { return this.payload("pdf", [...page]); }
    renderEpub() { return this.payload("epub"); }
    renderSite() { return this.payload("site"); }
    validateLinks() { return JSON.stringify({ fixture: "links" }); }
    free() { assert(!this.dead, "double free"); this.dead = true; bump("frees", 3); }
  }
  if (old) NativeBook.prototype.updateSources = undefined;
  if (noSets || old) NativeBook.prototype.replaceSources = undefined;
  const engine = createBookBindings(async () => NativeBook);
  // Routing double only: the unchanged ZIP/HTML preview codec has its own suite.
  const preview = async api => {
    const site = await api.renderBookSite();
    return { bytes: encode({ preview: JSON.parse(new TextDecoder().decode(site.bytes)) }), sourceLength: site.sourceLength };
  };
  return { stats, engine, preview, retained: createRetainedBook(engine, preview) };
}

if (!isMainThread) {
  const { engine, retained, preview } = fixture({ ...workerData,
    counters: new Int32Array(workerData.counters) });
  installBookWorker({
    addEventListener(kind, listener) { if (kind === "message") parentPort.on("message", data => listener({ data })); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  }, { ...engine, renderBookPreview: (files, options) => preview({ renderBookSite: () => engine.renderBookSite(files, options) }),
    renderRetainedBook: retained.render, clearRetainedBook: retained.clear,
    renderRetainedBookPreview: (files, options) => retained.render(files, options, "preview"),
    clearRetainedBookPreview: retained.clear,
    inspectBook: async () => ({ bytes: encode({ fixture: "inspection" }), sourceLength: 0 }) });
}
