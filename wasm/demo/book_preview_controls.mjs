import { parseBookPreview, resolveBookPreviewLink } from "../book_site_preview.mjs";
import { bookPreviewFrame } from "../book_preview_frame.mjs";
const failure = (code, message) => Object.assign(new Error(message), { code });
const blank = "<!doctype html><title>Book preview</title><p>Build a preview to review the current book.</p>";

/** A separate disposable worker isolates preview cancellation from exports.
 * No automatic rendering until explicitly enabled. Source remains host-owned.
 */
export function createBookPreviewControls({ root, controls, collection, worker,
  window = globalThis.window, crypto = globalThis.crypto, timers = globalThis,
  debounceMs = 600, frameTimeoutMs = 10000 }) {
  if (!Number.isInteger(debounceMs) || debounceMs < 1 || debounceMs > 60000
      || !Number.isInteger(frameTimeoutMs) || frameTimeoutMs < 1 || frameTimeoutMs > 60000) throw new TypeError("Invalid preview timing limits.");
  const ids = ["preview-build", "preview-auto", "preview-cancel", "preview-pages", "preview-previous", "preview-next", "preview-frame", "preview-status"];
  const el = Object.fromEntries(ids.map(id => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some(value => !value)) throw new Error("Missing book preview controls.");
  let disposed = false, suspended = false, composing = false, generation = 0, busy = false;
  let pending = null, deadline = null, preview = null, stamp = null, selected = 0, channel = null;
  const unlisten = [];
  const alive = () => { if (disposed || suspended) throw failure("PREVIEW_CLOSED", "Book preview is not active."); };
  function buttons() {
    const blocked = disposed || suspended || composing;
    el["preview-build"].disabled = blocked || busy || !collection.files.length;
    el["preview-cancel"].disabled = blocked || (!busy && pending === null && !preview);
    el["preview-pages"].disabled = blocked || !preview;
    el["preview-previous"].disabled = blocked || !preview || selected === 0;
    el["preview-next"].disabled = blocked || !preview || selected >= preview.pages.length - 1;
    el["preview-auto"].disabled = disposed || suspended;
  }
  function clearTimers() { timers.clearTimeout(pending); timers.clearTimeout(deadline); pending = deadline = null; }
  function retire() {
    generation++; clearTimers(); worker.cancel(); busy = false; preview = null; stamp = null; channel = null;
    el["preview-frame"].srcdoc = blank; el["preview-pages"].replaceChildren();
  }
  function report(error) {
    if (!disposed) el["preview-status"].textContent = `${error.code ?? "PREVIEW_ERROR"}: ${error.message ?? "Preview failed."} Source and exports are unchanged.`;
  }
  function current() {
    alive();
    if (!preview || stamp !== controls.checkpoint()) {
      retire(); buttons(); throw failure("STALE_SOURCE", "The book changed; rebuild its preview.");
    }
  }
  function show(index, fragment = "") {
    current();
    if (!Number.isInteger(index) || index < 0 || index >= preview.pages.length) throw failure("INVALID_SELECTION", "Choose a preview chapter.");
    const bytes = new Uint8Array(16); crypto.getRandomValues(bytes);
    const token = Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
    const page = preview.pages[index], document = bookPreviewFrame(page.html, token, fragment);
    timers.clearTimeout(deadline); selected = index; channel = token;
    el["preview-pages"].value = String(index);
    el["preview-frame"].srcdoc = document;
    el["preview-status"].textContent = `Opening ${page.source} in the isolated reader…`;
    deadline = timers.setTimeout(() => {
      if (disposed || channel !== token) return;
      retire(); el["preview-auto"].checked = false; buttons(); report(failure("PREVIEW_FRAME_FAILED", "The isolated reader did not acknowledge loading. Check browser content-security restrictions."));
    }, frameTimeoutMs);
    buttons();
  }
  async function build() {
    let ticket = null;
    try {
      alive(); if (composing) throw failure("BOOK_BUSY", "Finish composing text before rebuilding the preview.");
      retire(); controls.captureProject();
      const input = collection.snapshot(), captured = controls.checkpoint();
      ticket = ++generation; busy = true; buttons();
      el["preview-status"].textContent = "Rendering the complete book site in a worker. Edits cancel this snapshot.";
      const result = await worker.render(input.files, "preview", input.options);
      if (disposed || suspended || ticket !== generation || captured !== controls.checkpoint()) throw failure("STALE_SOURCE", "The book changed while its preview was rendering.");
      if (result.format !== "book-preview") throw failure("INVALID_BOOK_PREVIEW", "The worker returned a different output format.");
      const decoded = parseBookPreview(result.bytes);
      // The native mapping must describe this exact collection, not a preview
      // cached for a different book. HTML filenames themselves remain Rust-owned.
      if (decoded.pages.length !== input.files.length || decoded.pages.some((page, index) => page.source !== input.files[index].path)) {
        throw failure("INVALID_BOOK_PREVIEW", "The generated chapter map does not match the captured book.");
      }
      preview = decoded; stamp = captured; busy = false;
      el["preview-pages"].replaceChildren(...preview.pages.map((page, index) => {
        const option = root.createElement("option"); option.value = String(index); option.textContent = `${index + 1}. ${page.title} — ${page.source}`; return option;
      }));
      show(Math.min(selected, preview.pages.length - 1)); return preview;
    } catch (error) {
      if (!disposed && (ticket === null || ticket === generation)) { retire(); el["preview-auto"].checked = false; report(error); buttons(); }
      throw error;
    } finally { if (!disposed && ticket === generation) { busy = false; buttons(); } }
  }
  function changed() {
    if (disposed) return; retire();
    el["preview-status"].textContent = "Book changed. The previous preview was cleared; rebuild to review current source.";
    if (!suspended && !composing && el["preview-auto"].checked && collection.files.length) {
      pending = timers.setTimeout(() => { pending = null; void build().catch(() => {}); }, debounceMs);
    }
    buttons();
  }
  function invoke(work) { try { work(); } catch (error) { report(error); } }
  function on(target, type, listener) { target.addEventListener(type, listener); unlisten.push(() => target.removeEventListener(type, listener)); }
  el["preview-frame"].setAttribute("sandbox", "allow-scripts");
  el["preview-frame"].setAttribute("referrerpolicy", "no-referrer"); el["preview-frame"].removeAttribute("src");
  el["preview-auto"].checked = false;
  const unsubscribe = collection.subscribe(changed);
  for (const id of ["chapter-source", "chapter-path", "chapters", "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]) {
    const input = root.querySelector(`#${id}`);
    on(input, "input", changed); on(input, "change", changed);
  }
  on(root.querySelector("#chapter-source"), "compositionstart", () => { composing = true; changed(); });
  on(root.querySelector("#chapter-source"), "compositionend", () => { composing = false; changed(); });
  on(el["preview-build"], "click", () => { void build().catch(() => {}); });
  on(el["preview-auto"], "change", changed);
  on(el["preview-cancel"], "click", () => { el["preview-auto"].checked = false; retire(); buttons(); el["preview-status"].textContent = "Preview cancelled and cleared. Automatic preview is off."; });
  on(el["preview-pages"], "change", () => invoke(() => show(Number(el["preview-pages"].value))));
  on(el["preview-previous"], "click", () => invoke(() => show(selected - 1)));
  on(el["preview-next"], "click", () => invoke(() => show(selected + 1)));
  on(window, "message", event => {
    const message = event.data;
    if (disposed || !channel || event.source !== el["preview-frame"].contentWindow || event.origin !== "null"
        || !message || message.schemaVersion !== 1 || message.channel !== channel) return;
    invoke(() => {
      current();
      if (message.kind === "ready") {
        timers.clearTimeout(deadline); deadline = null;
        el["preview-status"].textContent = `Reviewing ${preview.pages[selected].source} (${selected + 1}/${preview.pages.length}). HTML-site preview, not PDF pagination. External resources and active content are disabled.`;
      } else if (message.kind === "link") {
        const destination = resolveBookPreviewLink(preview, preview.pages[selected].path, message.href);
        if (!destination) { el["preview-status"].textContent = "That link is external, unresolved, or not a book chapter. No navigation was performed."; return; }
        show(preview.pages.findIndex(page => page.path === destination.path), destination.fragment);
      } else if (message.kind === "missing-fragment") {
        el["preview-status"].textContent = "The chapter exists, but that fragment was not found in its generated HTML.";
      } else if (message.kind === "error") {
        retire(); el["preview-auto"].checked = false; buttons(); report(failure("PREVIEW_FRAME_FAILED", "The isolated reader could not display this chapter."));
      }
    });
  });
  retire(); buttons();
  return Object.freeze({ build,
    suspend() { if (disposed) return; suspended = true; composing = false; el["preview-auto"].checked = false; retire(); buttons(); },
    resume() { if (disposed) return; suspended = false; buttons(); },
    dispose() { if (disposed) return; disposed = true; retire(); unsubscribe(); for (const remove of unlisten) remove(); worker.dispose(); }
  });
}
