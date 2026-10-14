// Explicit native-ABI/editor fixture, NOT Markdown rendering or compiled WASM.
// Everything between the facade, real Worker, collection, reader and iframe is
// production JavaScript. Loaded only by book_chapter_browser.py.
import { createBookBindings, bookTextBytes } from "../book_session.mjs";
import { createBookWorkerClient, createRetainedBook, installBookWorker } from "../book_worker.mjs";
import { renderBookPreview, renderBookSessionChapterPreview } from "../book_site_preview.mjs";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookPreviewControls } from "../demo/book_preview_controls.mjs";

const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aKXcAAAAASUVORK5CYII=";
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const escape = value => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

if (typeof document === "undefined") {
  const calls = { creates: 0, updates: 0, renders: 0, sites: 0 };
  class NativeFixture {
    constructor(paths, sources) {
      calls.creates++;
      this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = []; this.sourceRevision = 0;
    }
    static fromSources(paths, sources, paths2, sources2) {
      const book = new NativeFixture(paths, sources);
      book.includes = paths2.map((path, i) => ({ path, source: sources2[i] }));
      return book;
    }
    get chapterCount() { return this.files.length; }
    get sourceLength() { return [...this.files, ...this.includes].reduce((n, f) => n + bookTextBytes(f.source), 0); }
    setMetadata() {} setCustomCss() {} setTheme() {} setNavigation() {} setFontScale() {}
    setImage() {} setFont() {} setFontWeight() {}
    updateSources(paths, sources, expected) {
      if (this.sourceRevision !== expected) throw new Error("fixture revision mismatch");
      let changed = 0;
      for (let i = 0; i < paths.length; i++) {
        const file = [...this.files, ...this.includes].find(f => f.path === paths[i]);
        if (!file) throw new Error("fixture unknown source");
        if (file.source !== sources[i]) { file.source = sources[i]; changed++; }
      }
      calls.updates++;
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        changed_sources: changed, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: changed ? this.files.map((_, i) => i) : [] });
    }
    renderSite() { calls.sites++; throw new Error("Native chapter mode must not export a ZIP"); }
    renderChapterPreview(selected) {
      calls.renders++;
      const source = this.files[selected].source;
      if (source === "BLOCK") {
        console.log("FMD_CHAPTER_FIXTURE_BLOCK_STARTED");
        const end = Date.now() + 10000;
        while (Date.now() < end) { /* Cancellation must terminate this worker. */ }
        console.log("FMD_CHAPTER_FIXTURE_BLOCK_FINISHED");
      }
      if (source === "FAIL") throw Object.assign(new Error("Explicit fixture rendering failure"), { code: "PREVIEW_LIMIT" });
      const detail = JSON.stringify({ selected, source, includes: this.includes, ...calls });
      const html = `<!doctype html><html lang="ja"><head><style>@import url('https://blocked.invalid/style');</style></head><body>
        <h1 id="chapter">Chapter ${selected + 1}</h1><p id="content">${escape(source)}</p>
        <pre id="metrics">${escape(detail)}</pre><a id="same" href="#target">Same chapter</a>
        <a id="cross" href="chapter-${1 - selected}.html#target">Other chapter</a>
        <a id="external" href="https://blocked.invalid/nav">External</a>
        <p style="margin-top:1000px" id="target">Fragment destination</p>
        <img id="embedded" src="data:image/png;base64,${png}">
        <img id="remote" src="https://blocked.invalid/pixel" onerror="window.__fixturePwned=true">
        <iframe src="https://blocked.invalid/frame"></iframe>
        <script>window.__fixturePwned=true;fetch('https://blocked.invalid/script')</script>
        <form action="https://blocked.invalid/form"><button>Submit</button></form>
        </body></html>`;
      return encode({ schema: "fmd-book-chapter-preview-v1", selected,
        pages: this.files.map((file, i) => ({ path: `chapter-${i}.html`, source: file.path, title: `Chapter ${i + 1}` })), html });
    }
    free() {}
  }
  const book = createBookBindings(async () => NativeFixture);
  const retained = createRetainedBook(book, renderBookPreview, renderBookSessionChapterPreview);
  installBookWorker(self, { ...book,
    renderRetainedBookPreview: (files, options, selected) => retained.render(files, options,
      selected === undefined ? "preview" : "chapter-preview", selected),
    clearRetainedBookPreview: retained.clear,
  });
} else {
  const collection = createBookCollection();
  collection.append({ chapters: [{ path: "one.md", source: "First chapter" }, { path: "two.md", source: "Second chapter" }],
    includeSources: [{ path: "note.md", source: "Shared text" }],
    images: [{ destination: "dot.png", bytes: Uint8Array.from(atob(png), c => c.charCodeAt(0)) }] });
  const counters = { snapshots: 0, projects: 0, requests: [], workers: 0, terminations: 0, errors: [] };
  const model = new Proxy({}, { get(_target, key) {
    if (key === "snapshot" || key === "project") return () => {
      counters[key === "snapshot" ? "snapshots" : "projects"]++;
      return collection[key]();
    };
    return collection[key];
  } });
  const worker = createBookWorkerClient({ retainPreview: true, workerFactory() {
    // Managed Chromium permits offline classic workers but not direct module
    // worker startup from about:blank. This test-only bootstrap imports the same
    // ESM graph and buffers startup messages; it is NOT the package entrypoint.
    const bootstrap = `const waiting=[]; const hold=e=>waiting.push(e.data);
      addEventListener('message',hold); import(${JSON.stringify(import.meta.url)}).then(()=>{
        removeEventListener('message',hold);
        for(const data of waiting) dispatchEvent(new MessageEvent('message',{data}));
      }).catch(error=>{setTimeout(()=>{throw error})});`;
    const url = URL.createObjectURL(new Blob([bootstrap], { type: "text/javascript" }));
    const real = new Worker(url); counters.workers++;
    real.addEventListener("error", event => counters.errors.push({ message: event.message, filename: event.filename?.slice(0, 160), line: event.lineno }));
    return {
      addEventListener: real.addEventListener.bind(real), removeEventListener: real.removeEventListener.bind(real),
      terminate() { counters.terminations++; real.terminate(); URL.revokeObjectURL(url); },
      postMessage(data, transfer) {
        counters.requests.push({ format: data.format, selected: data.selectedChapter,
          assetBytes: transfer.reduce((n, buffer) => n + buffer.byteLength, 0),
          sources: data.sourceUpdate?.files.map(file => file.path) ?? null });
        real.postMessage(data, transfer);
      },
    };
  } });
  const editor = document.querySelector("#chapter-source");
  editor.value = collection.files[0].source;
  const controls = {
    sourceBusy: false,
    captureProject() { collection.edit(0, collection.files[0].path, editor.value); return collection.project(); },
    checkpoint() { return JSON.stringify([collection.revision, editor.value]); },
  };
  editor.addEventListener("input", () => controls.captureProject());
  const preview = createBookPreviewControls({ root: document, controls, collection: model, worker });
  // The fixture does not load the workbench's unrelated publish/search panels.
  window.chapterFixture = { collection, worker, preview, counters,
    edit(index, source) {
      if (index === 0) editor.value = source;
      collection.edit(index, collection.files[index].path, source);
    },
    revoke() { collection.revokeImages(); },
    close() { preview.dispose(); collection.dispose(); },
  };
}
