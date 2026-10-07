// Explicit native-ABI double for JS dispatch/ownership tests, NOT a renderer.
// The worker_threads harness runs the production session/transport/ZIP codec.
import { parentPort, workerData } from "node:worker_threads";
import { deflateRawSync } from "node:zlib";
import { createBookBindings } from "../book_session.mjs";
import { createRetainedBook, installBookWorker } from "../book_worker.mjs";
import { renderBookPreview, renderBookSelectedPreview, renderBookSessionChapterPreview } from "../book_site_preview.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
export function siteArchive(files) {
  const chapters = files.map((file, i) => ({ page: `ch${i}.html`, source: file.path, title: `Chapter ${i}` }));
  const entries = [["index.html", "<!doctype html><p>Landing</p>"],
    ["search-index.json", JSON.stringify({ schema: "fmd-book-search-index-v1", chapters })],
    ...files.map((file, i) => [`ch${i}.html`, `<p>Legacy ${i}: ${file.source}</p>`])];
  const locals = [], directory = [];
  let offset = 0;
  for (const [path, text] of entries) {
    const name = Buffer.from(path), plain = Buffer.from(text), body = deflateRawSync(plain);
    const crc = crc32(plain), local = Buffer.alloc(30), central = Buffer.alloc(46);
    local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x800, 6); local.writeUInt16LE(8, 8);
    local.writeUInt32LE(crc, 14); local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(plain.length, 22); local.writeUInt16LE(name.length, 26);
    central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x800, 8); central.writeUInt16LE(8, 10);
    central.writeUInt32LE(crc, 16); central.writeUInt32LE(body.length, 20);
    central.writeUInt32LE(plain.length, 24); central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(offset, 42);
    locals.push(local, name, body); directory.push(central, name);
    offset += local.length + name.length + body.length;
  }
  const table = Buffer.concat(directory), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(table.length, 12); end.writeUInt32LE(offset, 16);
  return new Uint8Array(Buffer.concat([...locals, table, end]));
}

export function fixtureEngine({ legacy = false, started } = {}) {
  const calls = { created: 0, freed: 0, previews: 0, sites: 0, updates: 0, replacements: 0, images: 0 };
  const count = text => new TextEncoder().encode(text).length;
  class Raw {
    constructor(paths, sources) {
      calls.created++;
      this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = []; this.sourceRevision = 0; this.dead = false;
    }
    static fromSources(paths, sources, resourcePaths, resources) {
      const raw = new Raw(paths, sources);
      raw.includes = resourcePaths.map((path, i) => ({ path, source: resources[i] }));
      return raw;
    }
    get chapterCount() { return this.files.length; }
    get sourceLength() { return [...this.files, ...this.includes].reduce((n, file) => n + count(file.source), 0); }
    setMetadata() {} setCustomCss() {} setTheme() {} setNavigation() {} setFontScale() {}
    setImage() { calls.images++; }
    setFont() {} setFontWeight() {}
    updateSources(paths, sources, expected) {
      if (expected !== this.sourceRevision) throw new Error("bad fixture revision");
      if (sources.includes("FAIL_UPDATE")) throw Object.assign(new Error("fixture transaction rejected"), { code: "INVALID_INCLUDE" });
      let changed = 0;
      for (let i = 0; i < paths.length; i++) {
        const file = [...this.files, ...this.includes].find(file => file.path === paths[i]);
        if (!file) throw new Error("unknown fixture source");
        if (file.source !== sources[i]) { file.source = sources[i]; changed++; }
      }
      calls.updates++;
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        changed_sources: changed, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: changed ? this.files.map((_, i) => i) : [] });
    }
    replaceSources(paths, sources, resourcePaths, resources, expected) {
      if (expected !== this.sourceRevision) throw new Error("bad fixture revision");
      calls.replacements++;
      this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = resourcePaths.map((path, i) => ({ path, source: resources[i] }));
      this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-set-v1", revision: this.sourceRevision,
        changed: true, chapter_count: this.chapterCount, source_length: this.sourceLength,
        resource_count: this.includes.length, reparsed_chapter_count: this.chapterCount });
    }
    renderChapterPreview(selected) {
      calls.previews++;
      if (this.dead) throw new Error("freed fixture handle used");
      const source = this.files[selected].source;
      if (source === "FAIL_NATIVE") throw Object.assign(new Error("fixture native render failed"), { code: "PREVIEW_LIMIT" });
      if (source === "BAD_REPLY") return encode({ schema: "wrong" });
      if (source === "BLOCK" && started) {
        Atomics.store(started, 0, 1);
        const until = Date.now() + 5000;
        while (Date.now() < until) { /* Termination must stop synchronous work. */ }
        Atomics.store(started, 0, 2);
      }
      return encode({ schema: "fmd-book-chapter-preview-v1", selected,
        pages: this.files.map((file, i) => ({ path: `ch${i}.html`, source: file.path, title: `Chapter ${i}` })),
        html: JSON.stringify({ selected, source, includes: this.includes, revision: this.sourceRevision, ...calls }) });
    }
    renderSite() { calls.sites++; return siteArchive(this.files); }
    renderPdf() { return encode(calls); }
    renderEpub() { return encode(calls); }
    free() { if (this.dead) throw new Error("fixture double free"); this.dead = true; calls.freed++; }
  }
  if (legacy) delete Raw.prototype.renderChapterPreview;
  const book = createBookBindings(async () => Raw);
  const retained = createRetainedBook(book, renderBookPreview, renderBookSessionChapterPreview);
  const engine = { ...book,
    renderBookPreview: (files, options) => renderBookPreview(book, files, options),
    renderBookSelectedPreview: (files, options, selected) => renderBookSelectedPreview(book, files, options, selected),
    renderRetainedBook: retained.render, clearRetainedBook: retained.clear,
    renderRetainedBookPreview: (files, options, selected) => retained.render(files, options,
      selected === undefined ? "preview" : "chapter-preview", selected),
    clearRetainedBookPreview: retained.clear };
  return { engine, book, calls, retained };
}

if (parentPort) {
  const { engine } = fixtureEngine(workerData ?? {});
  installBookWorker({
    addEventListener(type, callback) {
      if (type !== "message") throw new Error("unsupported fixture event");
      parentPort.on("message", data => callback({ data }));
    },
    postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
  }, engine);
}
