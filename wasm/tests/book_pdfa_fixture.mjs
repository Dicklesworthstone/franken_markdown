// Explicit native boundary double. Payloads are JSON observations, NOT PDFs,
// EPUBs, conformance evidence, or another Markdown/include implementation.
import assert from "node:assert/strict";
import { createBookBindings } from "../book_session.mjs";
export const chapters = () => [
  { path: "guide/one.md", source: "# One\r\n\n[Two](../two.md#two)\n{{#include ../shared.txt}}" },
  { path: "two.md", source: "# Two\n\nα 🙂" },
];
export const included = () => [{ path: "shared.txt", source: "note[^n]\n\n[^n]: Shared note" }];
export const geometry = () => ({ size: { widthPt: 720, heightPt: 540 },
  margins: { topPt: 18, rightPt: 24, bottomPt: 30, leftPt: 36 } });
export const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));
export const code = expected => error => error?.code === expected;
export function nativeBookFixture({ old = false, load = async () => {} } = {}) {
  const state = { loads: 0, created: 0, freed: 0, calls: [], updates: 0, sets: 0, instances: [], failure: null };
  class NativeBook {
    constructor(paths, sources) {
      state.created++; state.instances.push(this);
      this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = []; this.settings = {}; this.images = []; this.fonts = [];
      this.sourceRevision = 0; this.dead = false;
    }
    static fromSources(paths, sources, paths2, sources2) {
      const book = new this(paths, sources);
      book.includes = paths2.map((path, i) => ({ path, source: sources2[i] }));
      return book;
    }
    get chapterCount() { return this.files.length; }
    get sourceLength() { return [...this.files, ...this.includes].reduce((n, f) => n + new TextEncoder().encode(f.source).length, 0); }
    setMetadata(...value) { this.settings.metadata = value; }
    setCustomCss(value) { this.settings.css = value; }
    setTheme(...value) { this.settings.theme = value; }
    setNavigation(...value) { this.settings.navigation = value; }
    setFontScale(value) { this.settings.scale = value; }
    setPdfOptions(...value) { this.settings.pdf = value; }
    setPdfOptionsWithRunningImages(...value) { this.setPdfOptions(...value); }
    setHtmlFontFormat(value) { this.settings.htmlFontFormat = value; }
    setImage(key, bytes) { this.images.push([key, [...bytes]]); }
    setFont(key, bytes) { this.fonts.push([key, [...bytes]]); }
    setFontWeight(...value) { this.settings.weight = value; }
    updateSources(paths, sources, revision) {
      assert.equal(this.dead, false); assert.equal(revision, this.sourceRevision); state.updates++;
      let count = 0;
      for (let i = 0; i < paths.length; i++) {
        const file = [...this.files, ...this.includes].find(f => f.path === paths[i]);
        assert.ok(file); if (file.source !== sources[i]) { file.source = sources[i]; count++; }
      }
      if (count) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        changed_sources: count, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: count ? this.files.map((_, i) => i) : [] });
    }
    replaceSources(paths, sources, paths2, sources2, revision) {
      assert.equal(this.dead, false); assert.equal(revision, this.sourceRevision); state.sets++;
      const files = paths.map((path, i) => ({ path, source: sources[i] }));
      const includes = paths2.map((path, i) => ({ path, source: sources2[i] }));
      const changed = JSON.stringify([files, includes]) !== JSON.stringify([this.files, this.includes]);
      this.files = files; this.includes = includes;
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-set-v1", changed, revision: this.sourceRevision,
        chapter_count: this.chapterCount, resource_count: includes.length, source_length: this.sourceLength,
        reparsed_chapter_count: changed ? this.chapterCount : 0 });
    }
    emit(kind, page = [], pdfA = "off", strict = false) {
      assert.equal(this.dead, false);
      const value = { kind, page: [...page], pdfA, strict, files: this.files, includes: this.includes,
        settings: this.settings, images: this.images, fonts: this.fonts };
      state.calls.push(structuredClone(value));
      if (pdfA === "2b" && state.failure !== null) throw state.failure;
      return new TextEncoder().encode(JSON.stringify(value));
    }
    renderPdf(...args) { assert.equal(args.length, 0); return this.emit("pdf"); }
    renderPdfWithPage(page) { assert.ok(page instanceof Float64Array); return this.emit("pdf", page); }
    renderPdfArchival(profile, strict, page) {
      assert.ok(page instanceof Float64Array); return this.emit("pdf", page, profile, strict);
    }
    renderEpub() { return this.emit("epub"); }
    renderSite() { return this.emit("site"); }
    validateLinks() {
      return JSON.stringify({ schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
        chapters: this.files.map(f => ({ path: f.path, checked: 0, external: 0, unchecked: 0, findings: [] })) });
    }
    free() { assert.equal(this.dead, false); this.dead = true; state.freed++; }
  }
  if (old) NativeBook.prototype.renderPdfArchival = undefined;
  const api = createBookBindings(async () => { state.loads++; await load(); return NativeBook; });
  return { api, state, NativeBook };
}
