import { createBookPdfProof } from "../book_pdf_proof.mjs";

/** Install a native-viewer surface without touching the restricted HTML-site
 * iframe or the publisher's ordinary PDF/EPUB/site download link. */
export function createBookPdfPanel(root) {
  const existing = root.querySelector("#book-pdf-proof");
  if (existing) return existing;
  const publish = root.querySelector("#publish-title")?.parentElement;
  if (!publish?.parentElement) throw new Error("Missing book publication section.");
  const panel = root.createElement("section");
  panel.id = "book-pdf-proof";
  panel.setAttribute("aria-labelledby", "book-pdf-title");
  const make = (tag, id, text, parent = panel) => {
    const node = root.createElement(tag);
    if (id) node.id = id;
    if (text) node.textContent = text;
    parent.appendChild(node);
    return node;
  };
  make("h2", "book-pdf-title", "Review the actual book PDF");
  make("p", "book-pdf-help", "Generate the complete paginated PDF with the current chapters, includes, book settings, images and supplied fonts. This is the Rust book PDF, not a print of the HTML preview. Download this proof saves the same bytes without rendering again.");
  const actions = make("div");
  for (const [id, text] of [["build", "Generate book PDF proof"], ["cancel", "Cancel PDF rendering"], ["release", "Clear PDF proof"]]) {
    const node = make("button", `book-pdf-${id}`, text, actions);
    node.type = "button";
    node.setAttribute("aria-describedby", "book-pdf-help book-pdf-status");
  }
  const download = make("a", "book-pdf-download", "Download this PDF proof", actions);
  download.hidden = true;
  download.setAttribute("aria-describedby", "book-pdf-status");
  const status = make("p", "book-pdf-status", "Generate a proof after adding chapters. No file is saved automatically.");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  status.style.overflowWrap = "anywhere";
  make("p", "book-pdf-viewer-help", "Embedded PDF viewing depends on your browser. A blank or unavailable viewer does not prevent downloading the exact proof for inspection in your PDF viewer. No remote viewer or service is loaded. The proof is released on edits or page suspension; it is not a saved source project.");
  make("div", "book-pdf-viewport");
  publish.parentElement.insertBefore(panel, publish);
  return panel;
}

export function createBookPdfControls({ root, controls, collection, worker,
  urls = URL, navigator = globalThis.navigator }) {
  const panel = createBookPdfPanel(root);
  const ids = ["build", "cancel", "release", "download", "status", "viewport"];
  const el = Object.fromEntries(ids.map(id => [id, root.querySelector(`#book-pdf-${id}`)]));
  if (Object.values(el).some(value => !value)) throw new Error("Missing book PDF proof controls.");
  const session = createBookPdfProof({ controls, collection, worker });
  const unlisten = [];
  let disposed = false, suspended = false, composing = false, operation = null;
  let shown = null, url = null;

  function releaseView() {
    const previous = url;
    url = null;
    shown = null;
    // Detach native viewing and download references before revoking the URL.
    el.download.hidden = true;
    el.download.removeAttribute("href");
    el.download.removeAttribute("download");
    el.viewport.replaceChildren();
    if (previous !== null) urls.revokeObjectURL(previous);
  }
  function active() { return !disposed && !suspended && !composing && !controls.sourceBusy; }
  function refresh() {
    const proof = session.current;
    if (shown && (proof !== shown || !active())) {
      releaseView();
      el.status.textContent = "Book changed. The old PDF proof was released; generate a new one.";
    }
    const enabled = active();
    el.build.disabled = !enabled || operation !== null || session.pending
      || !collection.files.some(file => file.role !== "include");
    el.cancel.disabled = !session.pending;
    el.release.disabled = !shown && !session.pending && !worker.hasRetainedBook;
    el.download.hidden = !enabled || !proof || shown !== proof || url === null;
    panel.setAttribute("aria-busy", String(session.pending));
  }
  function invalidate(message, keepIdle = false) {
    operation = null;
    session.invalidate(message, keepIdle);
    releaseView();
    if (!disposed) { el.status.textContent = message; refresh(); }
  }
  function show(proof) {
    if (proof === shown && url !== null) {
      el.status.textContent = `Current PDF proof reused without rendering: ${proof.size.toLocaleString()} bytes. Nothing has been saved yet.`;
      return;
    }
    releaseView();
    let next = null;
    try {
      next = urls.createObjectURL(proof.blob);
      let embedded = false;
      if (navigator?.pdfViewerEnabled !== false) {
        try {
          const viewer = root.createElement("object");
          viewer.id = "book-pdf-viewer";
          viewer.type = "application/pdf";
          viewer.setAttribute("aria-label", "Generated book PDF proof");
          viewer.setAttribute("aria-describedby", "book-pdf-viewer-help");
          viewer.style.cssText = "display:block;width:100%;height:65vh;min-height:320px;border:1px solid #d0d7de";
          viewer.data = next;
          const fallback = root.createElement("p");
          fallback.textContent = "Embedded PDF viewer unavailable. Use Download this PDF proof above.";
          viewer.appendChild(fallback);
          el.viewport.appendChild(viewer);
          embedded = true;
        } catch {
          // CSP/viewer/DOM failure must not strand an otherwise valid export.
          el.viewport.replaceChildren();
        }
      }
      if (session.current !== proof || !active())
        throw Object.assign(new Error("Book changed before the PDF viewer was installed."), { code: "STALE_SOURCE" });
      el.download.href = next;
      el.download.download = proof.filename;
      el.download.textContent = `Download this PDF proof (${proof.size.toLocaleString()} bytes)`;
      shown = proof;
      url = next;
      next = null;
      el.status.textContent = `PDF generated from ${proof.chapters} chapters: ${proof.size.toLocaleString()} bytes. Download this proof preserves exactly this output. Nothing has been saved yet.`
        + (embedded ? " Embedded viewing is provided by your browser." : " Embedded viewing is unavailable; download the PDF to inspect it.");
      refresh();
    } catch (reason) {
      if (next !== null) urls.revokeObjectURL(next);
      releaseView();
      throw reason;
    }
  }
  async function build() {
    if (!active()) throw Object.assign(new Error("Finish importing or composing text before proofing."), { code: "BOOK_BUSY" });
    if (operation) throw Object.assign(new Error("A PDF proof is already rendering."), { code: "BOOK_BUSY" });
    const ticket = {};
    operation = ticket;
    el.status.textContent = "Generating the complete native book PDF in its own cancellable worker…";
    try {
      const proof = await session.render();
      if (operation !== ticket || !active() || session.current !== proof)
        throw Object.assign(new Error("The book changed; this proof was not displayed."), { code: "STALE_SOURCE" });
      show(proof);
      return proof;
    } catch (reason) {
      if (!disposed && operation === ticket) {
        el.status.textContent = `${reason?.code ?? "PDF_PROOF_ERROR"}: ${String(reason?.message ?? "PDF proof failed.").slice(0, 2048)} Source and other exports are unchanged; retry explicitly.`;
      }
      throw reason;
    } finally {
      if (operation === ticket) operation = null;
      if (!disposed) refresh();
    }
  }
  function guardDownload(event) {
    if (!active() || !shown || session.current !== shown || url === null) {
      event.preventDefault();
      invalidate("Book changed. Generate a current PDF proof before downloading.");
    }
    // This ordinary user-activated anchor neither rebuilds the PDF nor claims
    // that the browser's download request actually saved a file to disk.
  }
  const on = (node, type, listener) => {
    if (!node) return;
    node.addEventListener(type, listener);
    unlisten.push(() => node.removeEventListener(type, listener));
  };
  const unsubscribe = session.subscribe(() => { if (!disposed) refresh(); });
  on(el.build, "click", () => { void build().catch(() => {}); });
  for (const id of ["cancel", "release"]) on(el[id], "click", () => {
    operation = null;
    session.cancel();
    releaseView();
    el.status.textContent = "PDF proof cancelled and released. Source and other exports are unchanged.";
    refresh();
  });
  for (const type of ["click", "auxclick", "contextmenu"]) on(el.download, type, guardDownload);
  on(panel, "focusin", refresh);
  for (const id of ["chapter-source", "chapter-path", "source-role", "chapters", "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]) {
    const input = root.querySelector(`#${id}`);
    for (const type of ["input", "change"]) on(input, type, () => {
      invalidate("Source or settings changed. Generate a new book PDF proof.",
        id === "chapter-source" || id === "chapters");
    });
  }
  on(root.querySelector("#chapter-source"), "compositionstart", () => {
    composing = true;
    invalidate("Text composition started. Generate a proof after finishing the edit.");
  });
  on(root.querySelector("#chapter-source"), "compositionend", () => { composing = false; refresh(); });
  releaseView();
  refresh();
  return Object.freeze({
    build,
    suspend() {
      if (disposed) return;
      suspended = true; composing = false; operation = null;
      session.suspend(); releaseView(); refresh();
      el.status.textContent = "Book suspended; the PDF proof was released. Generate again after returning.";
    },
    resume() { if (!disposed) { suspended = false; session.resume(); refresh(); } },
    dispose() {
      if (disposed) return;
      disposed = true; operation = null;
      unsubscribe();
      for (const remove of unlisten) remove();
      releaseView(); session.dispose();
      el.build.disabled = el.cancel.disabled = el.release.disabled = true;
      panel.setAttribute("aria-busy", "false");
    },
  });
}
