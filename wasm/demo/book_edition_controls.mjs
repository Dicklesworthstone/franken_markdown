import { createBookEditionSession } from "./book_edition.mjs";

const mounted = new WeakSet();
const choices = Object.freeze([["pdf", "PDF"], ["epub", "EPUB"], ["site", "HTML-site ZIP"]]);
const fail = (code, message) => Object.assign(new Error(message), { code });

export function createBookEditionPanel(root) {
  const existing = root.querySelector("#book-edition-panel");
  if (existing) return existing;
  const parent = root.querySelector("#publish-title")?.parentElement;
  if (!parent) throw fail("MISSING_CONTROLS", "The book publication section is unavailable.");
  const make = (tag, id, text) => {
    const node = root.createElement(tag);
    if (id) node.id = id;
    if (text) node.textContent = text;
    return node;
  };
  const panel = make("section", "book-edition-panel");
  panel.setAttribute("aria-labelledby", "book-edition-title");
  const heading = make("h3", "book-edition-title", "Publish a complete edition");
  const help = make("p", "book-edition-help",
    "Prepare selected formats from one captured book, including applied settings and currently authorized images/fonts. All selected formats must succeed before any edition download appears. After rendering, optionally package the exact published files in one delivery ZIP with a SHA-256 manifest. Packaging does not rerender. Checksums detect changes; they are not signatures. These downloads are not an editable project or portable backup. SHA-256 packaging requires a secure browser context. No file is saved automatically. The combined output limit is 128 MiB with one two-minute publication deadline.");
  const formats = make("fieldset", "book-edition-formats");
  formats.append(make("legend", null, "Formats in this edition"));
  for (const [key, text] of choices) {
    const label = make("label"), input = make("input", `book-edition-${key}`);
    input.type = "checkbox"; input.checked = true;
    input.setAttribute("aria-describedby", "book-edition-help");
    label.append(input, root.createTextNode(` ${text}`)); formats.append(label);
  }
  const actions = make("p"), prepare = make("button", "book-edition-prepare", "Prepare selected edition");
  const cancel = make("button", "book-edition-cancel", "Cancel / discard edition");
  prepare.type = cancel.type = "button"; cancel.disabled = true;
  prepare.setAttribute("aria-describedby", "book-edition-help book-edition-status");
  const bundle = make("button", "book-edition-bundle", "Prepare delivery ZIP + SHA-256 manifest");
  bundle.type = "button"; bundle.disabled = true;
  bundle.setAttribute("aria-describedby", "book-edition-help book-edition-status");
  actions.append(prepare, bundle, cancel);
  const status = make("p", "book-edition-status");
  status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  status.setAttribute("aria-atomic", "true");
  const downloads = make("div", "book-edition-downloads");
  downloads.hidden = true; downloads.setAttribute("role", "group");
  downloads.setAttribute("aria-label", "Prepared edition downloads");
  panel.append(heading, help, formats, actions, status, downloads);
  parent.append(panel);
  return panel;
}

/** A dedicated publisher is supplied by book.js. This controller never cancels
 * the ordinary export, preview, PDF-proof, inspection or font workers.
 * Source capture uses the existing source host and collection normalizer.
 */
export function createBookEditionControls({ root, controls, collection, publisher, urls = URL, crypto = globalThis.crypto }) {
  if (!controls || ["captureProject", "checkpoint", "subscribeSourceState"].some(key => typeof controls[key] !== "function")
      || !collection || ["snapshot", "subscribe"].some(key => typeof collection[key] !== "function"))
    throw fail("INVALID_OPTIONS", "Edition controls require the workbench source host and collection.");
  const panel = createBookEditionPanel(root);
  if (mounted.has(panel)) throw fail("ALREADY_MOUNTED", "Edition controls are already installed.");
  const names = ["prepare", "bundle", "cancel", "formats", "status", "downloads", ...choices.map(([key]) => key)];
  const el = Object.fromEntries(names.map(key => [key, root.querySelector(`#book-edition-${key}`)]));
  if (Object.values(el).some(node => !node)) throw fail("MISSING_CONTROLS", "Edition controls are incomplete.");
  let disposed = false, suspended = false, session = null;
  let links = [];
  const unlisten = [], composing = new Set(), finishing = new Map();
  const blocked = () => suspended || composing.size > 0 || controls.sourceBusy
    || !collection.files.some(file => file.role !== "include");
  const checkpoint = () => JSON.stringify([collection.revision, controls.checkpoint(), selection()]);
  function clearLinks() {
    // Also strip detached nodes retained by a menu/event handler before revocation.
    for (const link of links) { link.removeAttribute("href"); link.removeAttribute("download"); }
    links = [];
    el.downloads.replaceChildren(); el.downloads.hidden = true;
  }
  function show(state) {
    const inactive = disposed || suspended || state.phase === "disposed" || state.phase === "suspended";
    el.prepare.disabled = inactive || !state.canPrepare;
    el.bundle.disabled = inactive || !state.canBundle;
    el.cancel.disabled = inactive || (!state.busy && state.downloads.length === 0);
    el.formats.disabled = inactive;
    panel.setAttribute("aria-busy", String(state.busy));
    el.status.textContent = state.message;
    const sameLinks = links.length === state.downloads.length
      && links.every((link, index) => link.getAttribute("href") === state.downloads[index].href);
    if (sameLinks) return;
    clearLinks();
    // Insert the complete edition in one DOM transaction; never show partial sets.
    const next = state.downloads.map(output => {
      const row = root.createElement("p"), link = root.createElement("a");
      link.href = output.href; link.download = output.filename;
      link.type = output.mimeType; link.textContent = `Download ${output.filename} (${output.byteLength} bytes)`;
      link.setAttribute("aria-describedby", "book-edition-status");
      row.append(link);
      return { row, link };
    });
    el.downloads.replaceChildren(...next.map(value => value.row));
    links = next.map(value => value.link); el.downloads.hidden = links.length === 0;
  }
  function listen(target, name, handler, capture = false) {
    target.addEventListener(name, handler, capture);
    unlisten.push(() => target.removeEventListener(name, handler, capture));
  }
  function clearComposition() {
    for (const timer of finishing.values()) clearTimeout(timer);
    finishing.clear(); composing.clear();
  }
  function selection() { return choices.filter(([key]) => el[key].checked).map(([key]) => key); }
  function syncSelection() { session.setFormats(selection()); }
  async function prepare() {
    // Read checked values even when changed programmatically without an event.
    syncSelection();
    return session.prepare();
  }
  async function prepareBundle() {
    syncSelection();
    return session.prepareBundle();
  }
  function report(cause) {
    if (!disposed && !suspended && !session.state.busy && session.state.downloads.length === 0 && session.state.phase !== "error")
      el.status.textContent = `${cause?.code ?? "EDITION_FAILED"}: ${cause?.message ?? "Edition preparation failed."}`;
  }
  try {
    session = createBookEditionSession({ publisher, urls, crypto, checkpoint, isBlocked: blocked, onState: show,
      capture() {
        controls.captureProject();
        const before = checkpoint();
        const input = collection.snapshot();
        if (before !== checkpoint()) throw fail("STALE_SOURCE", "The book changed during edition capture.");
        return input;
      },
    });
    mounted.add(panel);
    unlisten.push(collection.subscribe(() => session.changed()));
    unlisten.push(controls.subscribeSourceState(() => session.invalidate()));
    listen(el.prepare, "click", () => { void prepare().catch(report); });
    listen(el.bundle, "click", () => { void prepareBundle().catch(report); });
    listen(el.cancel, "click", () => session.cancel());
    // Capture before other components can reject a half-edited path/setting.
    // Any workbench input is conservative invalidation, including unapplied drafts.
    const edit = event => {
      if (disposed || suspended) return;
      if (panel.contains(event.target)) {
        if (choices.some(([key]) => event.target === el[key])) {
          try { syncSelection(); } catch (cause) { report(cause); }
        }
      } else session.invalidate();
    };
    listen(root, "input", edit, true); listen(root, "change", edit, true);
    listen(root, "compositionstart", event => {
      if (disposed || suspended || panel.contains(event.target)) return;
      clearTimeout(finishing.get(event.target)); finishing.delete(event.target);
      composing.add(event.target); session.invalidate();
    }, true);
    listen(root, "compositionend", event => {
      if (disposed || suspended || !composing.has(event.target)) return;
      const target = event.target;
      clearTimeout(finishing.get(target));
      // Keep capture blocked through the final input following compositionend.
      const timer = setTimeout(() => {
        if (disposed || suspended || finishing.get(target) !== timer) return;
        finishing.delete(target); composing.delete(target); session.invalidate();
      }, 0);
      finishing.set(target, timer);
    }, true);
    const activate = event => {
      const link = event.target?.closest?.("a");
      if (!link || !el.downloads.contains(link)) return;
      // Check format controls as well as the source checkpoint at activation.
      try { syncSelection(); } catch { event.preventDefault(); return; }
      if (!session.authorizeDownload(link.getAttribute("href"))) event.preventDefault();
    };
    listen(el.downloads, "click", activate, true); listen(el.downloads, "auxclick", activate, true);
    syncSelection(); show(session.state);
  } catch (cause) {
    for (const remove of unlisten.splice(0).reverse()) remove();
    clearComposition(); mounted.delete(panel);
    session?.dispose(); clearLinks();
    throw cause;
  }
  return Object.freeze({
    prepare,
    prepareBundle,
    get state() { return session.state; },
    suspend() {
      if (disposed || suspended) return;
      suspended = true; clearComposition(); session.suspend();
    },
    resume() {
      if (disposed || !suspended) return;
      suspended = false; session.resume();
    },
    dispose() {
      if (disposed) return;
      disposed = true; clearComposition();
      for (const remove of unlisten.splice(0).reverse()) remove();
      mounted.delete(panel);
      try { session.dispose(); } finally { clearLinks(); }
    },
  });
}
