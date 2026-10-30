import { bookPreviewFrame } from "../book_preview_frame.mjs";
import { parseBookPreview, resolveBookPreviewLink } from "../book_site_preview.mjs";
import { parseBookChapterPreview } from "../book_session.mjs";
import { createBookCollectionCapture } from "../book_worker.mjs";

const failure = (code, message) => Object.assign(new Error(message), { code });
const blank =
  "<!doctype html><title>Book preview</title><p>Build a preview to review the current book.</p>";

/** A separate worker isolates preview cancellation and retained state from exports.
 * No automatic rendering until explicitly enabled. Source remains host-owned.
 */
export function createBookPreviewControls({
  root,
  controls,
  collection,
  worker,
  window = globalThis.window,
  crypto = globalThis.crypto,
  timers = globalThis,
  debounceMs = 600,
  frameTimeoutMs = 10000,
}) {
  if (
    !Number.isInteger(debounceMs) ||
    debounceMs < 1 ||
    debounceMs > 60000 ||
    !Number.isInteger(frameTimeoutMs) ||
    frameTimeoutMs < 1 ||
    frameTimeoutMs > 60000
  )
    throw new TypeError("Invalid preview timing limits.");
  const ids = [
    "preview-build",
    "preview-auto",
    "preview-cancel",
    "preview-pages",
    "preview-previous",
    "preview-next",
    "preview-frame",
    "preview-status",
  ];
  const el = Object.fromEntries(ids.map((id) => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some((value) => !value)) throw new Error("Missing book preview controls.");
  let disposed = false,
    suspended = false,
    composing = false,
    capturing = false,
    generation = 0,
    busy = false;
  let pending = null,
    deadline = null,
    preview = null,
    stamp = null,
    selected = 0,
    channel = null;
  // Keep capture ownership with this controller. The helper reuses the worker's
  // acknowledged source baseline, never an independent image/font or HTML cache.
  const captureBook = createBookCollectionCapture(collection, worker);
  const chapterPreview = worker.supportsChapterPreview === true;
  let selectedSource = null, previewMode = null;
  let configurationRevision = collection.renderConfigurationRevision;
  const unlisten = [];
  const alive = () => {
    if (disposed || suspended) throw failure("PREVIEW_CLOSED", "Book preview is not active.");
  };
  function buttons() {
    const blocked = disposed || suspended || composing || controls.sourceBusy;
    el["preview-build"].disabled = blocked || busy || !collection.files.some(file => file.role !== "include");
    el["preview-cancel"].disabled = blocked || (!busy && pending === null && !preview && !worker.hasRetainedPreview);
    el["preview-pages"].disabled = blocked || !preview;
    el["preview-previous"].disabled = blocked || !preview || selected === 0;
    el["preview-next"].disabled = blocked || !preview || selected >= preview.pages.length - 1;
    el["preview-auto"].disabled = disposed || suspended;
  }
  function clearTimers() {
    timers.clearTimeout(pending);
    timers.clearTimeout(deadline);
    pending = deadline = null;
  }
  function retire(keepIdle = false) {
    generation++;
    clearTimers();
    if (keepIdle && typeof worker.cancelPending === "function") worker.cancelPending();
    else worker.cancel();
    busy = false;
    preview = null;
    previewMode = null;
    stamp = null;
    channel = null;
    el["preview-frame"].srcdoc = blank;
    el["preview-pages"].replaceChildren();
  }
  function report(error) {
    if (!disposed)
      el["preview-status"].textContent =
        `${error.code ?? "PREVIEW_ERROR"}: ${error.message ?? "Preview failed."} Source and exports are unchanged.`;
  }
  function current() {
    alive();
    if (!preview || stamp !== controls.checkpoint()) {
      retire();
      buttons();
      throw failure("STALE_SOURCE", "The book changed; rebuild its preview.");
    }
  }
  function show(index, fragment = "") {
    current();
    if (!Number.isInteger(index) || index < 0 || index >= preview.pages.length)
      throw failure("INVALID_SELECTION", "Choose a preview chapter.");
    if (chapterPreview && index !== preview.selected)
      throw failure("INVALID_SELECTION", "This chapter has not been rendered for the current source.");
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    const token = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    const page = preview.pages[index],
      document = bookPreviewFrame(chapterPreview ? preview.html : page.html, token, fragment);
    timers.clearTimeout(deadline);
    selected = index;
    selectedSource = page.source;
    channel = token;
    el["preview-pages"].value = String(index);
    el["preview-frame"].srcdoc = document;
    el["preview-status"].textContent = `Opening ${page.source} in the isolated reader…`;
    deadline = timers.setTimeout(() => {
      if (disposed || channel !== token) return;
      retire();
      el["preview-auto"].checked = false;
      buttons();
      report(
        failure(
          "PREVIEW_FRAME_FAILED",
          "The isolated reader did not acknowledge loading. Check browser content-security restrictions.",
        ),
      );
    }, frameTimeoutMs);
    buttons();
  }
  function navigate(index, fragment = "") {
    current();
    if (!Number.isInteger(index) || index < 0 || index >= preview.pages.length)
      throw failure("INVALID_SELECTION", "Choose a preview chapter.");
    if (chapterPreview && index !== preview.selected) {
      // Source identity, not an old numeric slot, determines the target if the
      // collection changes during capture. Late failures are handled by their
      // own generation and must not overwrite a newer reader's status.
      void renderSelection(preview.pages[index].source, fragment).catch(() => {});
    } else show(index, fragment);
  }
  function build() {
    return renderSelection();
  }
  async function renderSelection(requestedSource, fragment = "") {
    let ticket = null;
    try {
      alive();
      if (composing || controls.sourceBusy)
        throw failure("BOOK_BUSY", "Finish importing or composing text before rebuilding the preview.");
      retire(true);
      capturing = true;
      try { controls.captureProject(); } finally { capturing = false; }
      alive();
      const input = captureBook(), captured = controls.checkpoint();
      // Preserve the current chapter across reorder/source edits. If it was
      // removed, an explicit rebuild chooses the nearest remaining chapter;
      // navigation to a removed source instead fails rather than opening another.
      const target = requestedSource ?? selectedSource;
      let index = input.files.findIndex(file => file.path === target);
      if (index < 0 && requestedSource !== undefined)
        throw failure("STALE_SOURCE", "The requested chapter was removed; rebuild the preview.");
      if (index < 0) index = Math.min(selected, input.files.length - 1);
      ticket = ++generation;
      busy = true;
      buttons();
      el["preview-status"].textContent = chapterPreview
        ? "Rendering the selected chapter in a worker. Edits cancel this snapshot."
        : "Rendering the complete book site in a worker. Edits cancel this snapshot.";
      const format = chapterPreview ? "chapter-preview" : "preview";
      const result = await input.render(format, chapterPreview ? { selectedChapter: index } : undefined);
      if (disposed || suspended || ticket !== generation || captured !== controls.checkpoint())
        throw failure("STALE_SOURCE", "The book changed while its preview was rendering.");
      if (result.format !== `book-${format}`)
        throw failure("INVALID_BOOK_PREVIEW", "The worker returned a different output format.");
      if (chapterPreview && (result.selectedChapter !== index
          || !["native-chapter", "site-fallback"].includes(result.previewMode)))
        throw failure("INVALID_BOOK_PREVIEW", "The worker returned an invalid chapter selection or mode.");
      const decoded = chapterPreview
        ? parseBookChapterPreview(result.bytes, index, input.files.length)
        : parseBookPreview(result.bytes);
      // The native mapping must describe this exact collection, not a preview
      // cached for a different book. HTML filenames themselves remain Rust-owned.
      if (
        decoded.pages.length !== input.files.length ||
        decoded.pages.some((page, index) => page.source !== input.files[index].path)
      ) {
        throw failure(
          "INVALID_BOOK_PREVIEW",
          "The generated chapter map does not match the captured book.",
        );
      }
      preview = decoded;
      previewMode = chapterPreview ? result.previewMode : null;
      stamp = captured;
      busy = false;
      el["preview-pages"].replaceChildren(
        ...preview.pages.map((page, index) => {
          const option = root.createElement("option");
          option.value = String(index);
          option.textContent = `${index + 1}. ${page.title} — ${page.source}`;
          return option;
        }),
      );
      show(index, fragment);
      return preview;
    } catch (error) {
      if (!disposed && (ticket === null || ticket === generation)) {
        retire();
        el["preview-auto"].checked = false;
        report(error);
        buttons();
      }
      throw error;
    } finally {
      if (!disposed && ticket === generation) {
        busy = false;
        buttons();
      }
    }
  }
  function changed(keepIdle = false) {
    if (disposed) return;
    retire(keepIdle === true);
    el["preview-status"].textContent =
      "Book changed. The previous preview was cleared; rebuild to review current source.";
    if (!suspended && !composing && !capturing && !controls.sourceBusy
        && el["preview-auto"].checked && collection.files.some(file => file.role !== "include")) {
      pending = timers.setTimeout(() => {
        pending = null;
        void build().catch(() => {});
      }, debounceMs);
    }
    buttons();
  }
  function invoke(work) {
    try {
      work();
    } catch (error) {
      report(error);
    }
  }
  function on(target, type, listener) {
    target.addEventListener(type, listener);
    unlisten.push(() => target.removeEventListener(type, listener));
  }
  el["preview-frame"].setAttribute("sandbox", "allow-scripts");
  el["preview-frame"].setAttribute("referrerpolicy", "no-referrer");
  el["preview-frame"].removeAttribute("src");
  el["preview-auto"].checked = false;
  const unsubscribe = collection.subscribe(() => {
    const next = collection.renderConfigurationRevision;
    const sourceOnly = Number.isSafeInteger(next) && next === configurationRevision;
    configurationRevision = next;
    changed(sourceOnly);
  });
  const unsubscribeSource = controls.subscribeSourceState?.(() => {
    if (controls.sourceBusy) changed();
    else buttons();
  });
  for (const id of [
    "chapter-source",
    "chapter-path",
    "chapters",
    "title",
    "author",
    "lang",
    "font",
    "dark-mode",
    "font-scale",
    "toc",
    "page-numbers",
  ]) {
    const input = root.querySelector(`#${id}`);
    const listener = () => changed(["chapter-source", "chapter-path", "chapters"].includes(id));
    on(input, "input", listener);
    on(input, "change", listener);
  }
  on(root.querySelector("#chapter-source"), "compositionstart", () => {
    composing = true;
    changed();
  });
  on(root.querySelector("#chapter-source"), "compositionend", () => {
    composing = false;
    changed();
  });
  on(el["preview-build"], "click", () => {
    void build().catch(() => {});
  });
  on(el["preview-auto"], "change", changed);
  on(el["preview-cancel"], "click", () => {
    el["preview-auto"].checked = false;
    retire();
    buttons();
    el["preview-status"].textContent = "Preview cancelled and cleared. Automatic preview is off.";
  });
  on(el["preview-pages"], "change", () => invoke(() => navigate(Number(el["preview-pages"].value))));
  on(el["preview-previous"], "click", () => invoke(() => navigate(selected - 1)));
  on(el["preview-next"], "click", () => invoke(() => navigate(selected + 1)));
  on(window, "message", (event) => {
    const message = event.data;
    if (
      disposed ||
      !channel ||
      event.source !== el["preview-frame"].contentWindow ||
      event.origin !== "null" ||
      !message ||
      message.schemaVersion !== 1 ||
      message.channel !== channel
    )
      return;
    invoke(() => {
      current();
      if (message.kind === "ready") {
        timers.clearTimeout(deadline);
        deadline = null;
        el["preview-status"].textContent =
          `Reviewing ${preview.pages[selected].source} (${selected + 1}/${preview.pages.length}). HTML-site preview, not PDF pagination. External resources and active content are disabled.${previewMode === "site-fallback" ? " This older renderer uses the whole-site fallback." : ""}`;
      } else if (message.kind === "link") {
        const destination = resolveBookPreviewLink(
          preview,
          preview.pages[selected].path,
          message.href,
        );
        if (!destination) {
          el["preview-status"].textContent =
            "That link is external, unresolved, or not a book chapter. No navigation was performed.";
          return;
        }
        navigate(
          preview.pages.findIndex((page) => page.path === destination.path),
          destination.fragment,
        );
      } else if (message.kind === "missing-fragment") {
        el["preview-status"].textContent =
          "The chapter exists, but that fragment was not found in its generated HTML.";
      } else if (message.kind === "error") {
        retire();
        el["preview-auto"].checked = false;
        buttons();
        report(
          failure("PREVIEW_FRAME_FAILED", "The isolated reader could not display this chapter."),
        );
      }
    });
  });
  retire();
  buttons();
  return Object.freeze({
    build,
    suspend() {
      if (disposed) return;
      suspended = true;
      composing = false;
      el["preview-auto"].checked = false;
      retire();
      buttons();
    },
    resume() {
      if (disposed) return;
      suspended = false;
      buttons();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      retire();
      unsubscribe();
      unsubscribeSource?.();
      for (const remove of unlisten) remove();
      worker.dispose();
    },
  });
}
