// Browser interaction fixture: real modules/DOM/Files/Workers, explicit source
// host and protocol-only preflight adapter. It does NOT parse a TrueType font.
import { createBookFontPanel, createBookFontControls } from "../demo/book_font_controls.mjs";
import { host } from "./book_font_test_support.mjs";

export function mount() {
  const book = host(), editor = document.querySelector("#chapter-source");
  editor.value = book.collection.files[0].source;
  const controls = {
    get sourceBusy() { return book.controls.sourceBusy; },
    captureProject() {
      if (editor.value !== book.collection.files[0].source.replace(/\r\n?/g, "\n")) book.edit(editor.value);
      return book.controls.captureProject();
    },
    checkpoint: () => JSON.stringify([book.controls.checkpoint(), editor.value]),
    subscribeSourceState: listener => book.controls.subscribeSourceState(listener),
  };
  editor.addEventListener("input", () => book.edit(editor.value));
  editor.addEventListener("compositionstart", () => book.busy(true));
  editor.addEventListener("compositionend", () => book.busy(false));
  const stats = { started: 0, terminated: 0, active: 0, disposed: 0, jobs: [] };
  const mode = { reject: false, delay: 0 };
  const active = new Set();
  const script = `self.onmessage = ({data}) => { setTimeout(() => {
    if (data.reject) self.postMessage({error: 'Test native-admission rejection'});
    else self.postMessage({format:'book-pdf', mimeType:'application/pdf',
      bytes: new TextEncoder().encode('%PDF-protocol-fixture-not-a-document')});
  }, data.delay); };`;
  const worker = {
    render(files, format, options, { signal }) {
      stats.started++;
      stats.jobs.push({ files, format, slots: options.fontAssets.map(asset => ({
        slot: asset.slot, weight: asset.weight, length: asset.bytes.length, bytes: [...asset.bytes],
      })) });
      const url = URL.createObjectURL(new Blob([script], { type: "text/javascript" }));
      const endpoint = new Worker(url); stats.active++;
      return new Promise((resolve, reject) => {
        let done = false;
        const abort = () => finish(Object.assign(Error("worker aborted"), { code: "FONT_CANCELLED" }));
        function finish(error, result) {
          if (done) return; done = true; active.delete(abort);
          signal.removeEventListener("abort", abort); endpoint.terminate(); URL.revokeObjectURL(url);
          stats.terminated++; stats.active--;
          if (error) reject(error); else resolve(result);
        }
        active.add(abort); signal.addEventListener("abort", abort, { once: true });
        endpoint.onmessage = event => event.data.error
          ? finish(Object.assign(Error(event.data.error), { code: "INVALID_FONT" })) : finish(null, event.data);
        endpoint.onerror = () => finish(Error("fixture worker failed"));
        if (signal.aborted) abort();
        else endpoint.postMessage({ files, format, options, ...mode });
      });
    },
    dispose() { stats.disposed++; for (const abort of [...active]) abort(); },
  };
  createBookFontPanel(document);
  const ui = createBookFontControls({ root: document, collection: book.collection, controls, worker,
    confirm: text => window.confirm(text) });
  window.addEventListener("pagehide", () => { ui.suspend(); book.collection.revokeFonts(); });
  window.addEventListener("pageshow", () => ui.resume());
  globalThis.fixture = { book, ui, stats, mode };
}
