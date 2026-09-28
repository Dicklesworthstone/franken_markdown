// Independent classic-ZIP fixtures and an explicitly fake native ABI. The real
// book facade, worker protocol, preview codec and transfer boundaries still run.
import assert from "node:assert/strict";
import { isMainThread, parentPort, workerData } from "node:worker_threads";
import { createBookBindings, bookTextBytes } from "../book_session.mjs";
import { createRetainedBookPreview, installBookWorker } from "../book_worker.mjs";
import { renderBookPreview } from "../book_site_preview.mjs";

export function zip(entries) {
  const body = [], directory = [];
  let offset = 0;
  for (const [name, text] of entries) {
    const filename = Buffer.from(name), data = Buffer.from(text);
    let crc = 0xffffffff;
    for (const byte of data) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
    }
    crc = (crc ^ 0xffffffff) >>> 0;
    const local = Buffer.alloc(30), central = Buffer.alloc(46);
    local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4); local.writeUInt16LE(0x800, 6);
    local.writeUInt32LE(crc, 14); local.writeUInt32LE(data.length, 18); local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(filename.length, 26);
    central.writeUInt32LE(0x02014b50); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x800, 8); central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(data.length, 20); central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(filename.length, 28); central.writeUInt32LE(offset, 42);
    body.push(local, filename, data); directory.push(central, filename);
    offset += local.length + filename.length + data.length;
  }
  const central = Buffer.concat(directory), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(central.length, 12); end.writeUInt32LE(offset, 16);
  return new Uint8Array(Buffer.concat([...body, central, end]));
}

export function nativeFixture({ counters, old = false } = {}) {
  const stats = { creates: 0, updates: [], sites: 0, frees: 0, failUpdate: false, badReport: false, badZip: false };
  const bump = (key, index) => { stats[key]++; if (counters) Atomics.add(counters, index, 1); };
  const escape = value => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  class NativeBook {
    constructor(paths, sources) {
      bump("creates", 0); this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = []; this.sourceRevision = 0; this.images = []; this.fonts = []; this.settings = {};
      this.freed = false;
    }
    static fromSources(paths, sources, includedPaths, includedSources) {
      const book = new NativeBook(paths, sources);
      book.includes = includedPaths.map((path, i) => ({ path, source: includedSources[i] }));
      return book;
    }
    get chapterCount() { return this.files.length; }
    get sourceLength() { return [...this.files, ...this.includes].reduce((n, f) => n + bookTextBytes(f.source), 0); }
    setMetadata(...values) { this.settings.metadata = values; }
    setCustomCss(value) { this.settings.css = value; }
    setTheme(...values) { this.settings.theme = values; }
    setNavigation(...values) { this.settings.navigation = values; }
    setFontScale(value) { this.settings.scale = value; }
    setImage(destination, bytes) { this.images.push({ destination, bytes: [...bytes] }); }
    setFont(slot, bytes) { this.fonts.push({ slot, bytes: [...bytes] }); }
    setFontWeight(slot, weight) { this.fonts.find(font => font.slot === slot).weight = weight; }
    updateSources(paths, sources, expected) {
      assert(!this.freed); assert.equal(expected, this.sourceRevision);
      if (stats.failUpdate || sources.includes("REJECT_UPDATE")) throw Object.assign(Error("fixture update rejection"), { code: "FIXTURE_UPDATE" });
      const all = [...this.files, ...this.includes];
      const changes = paths.map((path, i) => ({ path, source: sources[i] }));
      assert(changes.every(f => all.some(old => old.path === f.path)));
      stats.updates.push({ changes: structuredClone(changes), expected });
      if (counters) Atomics.add(counters, 1, 1);
      let changed = 0;
      for (const next of changes) { const old = all.find(f => f.path === next.path); if (old.source !== next.source) { changed++; old.source = next.source; } }
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        changed_sources: changed, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: stats.badReport ? [this.chapterCount] : [] });
    }
    renderSite() {
      assert(!this.freed); bump("sites", 2);
      if (this.files.some(f => f.source === "BLOCK_WORKER") && counters) {
        Atomics.store(counters, 4, 1); Atomics.wait(counters, 5, 0, 10000);
      }
      if (stats.badZip || this.files.some(f => f.source === "CORRUPT_ZIP")) return new Uint8Array([1, 2, 3]);
      const chapters = this.files.map((f, i) => ({ source: f.path, title: `Fixture ${i}`, page: `page-${i}.html` }));
      const metadata = JSON.stringify({ includes: this.includes, settings: this.settings, images: this.images, fonts: this.fonts });
      return zip([["index.html", "fixture landing"],
        ["search-index.json", JSON.stringify({ schema: "fmd-book-search-index-v1", chapters })],
        ...chapters.map((c, i) => [c.page, `<pre>${escape(this.files[i].source)}\n${escape(metadata)}</pre>`])]);
    }
    renderPdf() { return new TextEncoder().encode("%PDF-1.7 fixture, not a rendering"); }
    renderPdfWithPage() { return this.renderPdf(); }
    renderEpub() { return new Uint8Array([1]); }
    free() { assert(!this.freed, "native double free"); this.freed = true; bump("frees", 3); }
  }
  if (old) Object.defineProperty(NativeBook.prototype, "updateSources", { value: undefined });
  return { stats, engine: createBookBindings(async () => NativeBook) };
}

if (!isMainThread) {
  const { engine } = nativeFixture({ counters: new Int32Array(workerData.counters), old: workerData.old });
  const retained = createRetainedBookPreview(engine, renderBookPreview);
  installBookWorker({
    addEventListener(kind, listener) { if (kind === "message") parentPort.on("message", data => listener({ data })); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  }, { ...engine, renderBookPreview: (files, options) => renderBookPreview(engine, files, options),
    renderRetainedBookPreview: retained.render, clearRetainedBookPreview: retained.clear });
}
