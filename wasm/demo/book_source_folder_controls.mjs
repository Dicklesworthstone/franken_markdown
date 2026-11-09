import { readBookSourceFolder, BOOK_SOURCE_FOLDER_LIMITS } from "./book_source_folder.mjs";

const fail = (code, message) => Object.assign(new Error(message), { code });
const fields = new Set(["chapter-source", "chapter-path", "source-role", "chapters",
  "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]);

/** A reviewed folder is inert source data until the existing project host
 * replaces the book. This component owns no renderer, resources or handles. */
export function createBookSourceFolderControls({ root, controls, collection,
  projectCodec, confirm, onProjectOpened = () => {}, timeoutMs = 30000 }) {
  if (![controls?.checkpoint, controls?.replaceProject, controls?.subscribeSourceState,
    collection?.subscribe, projectCodec?.normalizeBookProject, projectCodec?.readBookProject,
    projectCodec?.readBookSourceFile, confirm, onProjectOpened].every(fn => typeof fn === "function")
      || !Number.isSafeInteger(collection?.sourceProjectRevision) || !Number.isSafeInteger(collection?.revision))
    throw new TypeError("Source-folder controls require the project codec, source owner and explicit replacement confirmation.");
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000)
    throw new TypeError("Use a source-folder read deadline from 1 through 120000 milliseconds.");
  const section = root.querySelector("#import-title")?.parentElement;
  if (!section || root.querySelector("#book-source-folder"))
    throw new Error("Missing or duplicate source-folder import controls.");
  const panel = root.createElement("div"); panel.id = "book-source-folder";
  panel.setAttribute("aria-labelledby", "book-source-folder-title");
  const make = (tag, id, text, parent = panel) => {
    const node = root.createElement(tag); node.id = `book-source-folder-${id}`;
    if (text !== undefined) node.textContent = text;
    parent.appendChild(node); return node;
  };
  make("h3", "title", "Reopen editable source folder");
  make("p", "help", "Choose an extracted source ZIP folder containing book.fmdbook.json and sources/. Current files provide the text; the manifest restores order, chapter/include roles and applied settings. Reopening replaces this book and clears current images and fonts. Other files are ignored. Nothing is uploaded or written to disk.");
  const label = make("label", "label", "Extracted source folder"); label.setAttribute("for", "book-source-folder-input");
  const input = make("input", "input"); input.type = "file"; input.multiple = true;
  input.webkitdirectory = true; input.setAttribute("webkitdirectory", "");
  input.setAttribute("aria-describedby", "book-source-folder-help book-source-folder-status");
  const buttons = Object.fromEntries([["review", "Review selected folder"], ["restore", "Reopen reviewed folder"],
    ["cancel", "Cancel / clear folder review"]].map(([id, text]) => {
    const button = make("button", id, text); button.type = "button";
    button.setAttribute("aria-describedby", "book-source-folder-help book-source-folder-status"); return [id, button];
  }));
  const status = make("p", "status", "Choose the folder that contains book.fmdbook.json, then review it before reopening.");
  status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  const details = make("details", "details"); details.hidden = true;
  make("summary", "summary", "Sources in saved order", details);
  const list = make("ol", "files", undefined, details);
  section.appendChild(panel);
  let disposed = false, suspended = false, generation = 0, active = null, ready = null;
  const composing = new Set(), listeners = [];
  const blocked = () => disposed || suspended || composing.size > 0 || controls.sourceBusy;
  const stamp = () => JSON.stringify([collection.sourceProjectRevision, controls.checkpoint()]);
  const sameSelection = job => input.files?.length === job.files.length
    && job.files.every((file, index) => input.files[index] === file);
  function refresh() {
    const busy = blocked() || active !== null;
    input.disabled = busy;
    buttons.review.disabled = busy || !input.files?.length;
    buttons.restore.disabled = busy || ready === null;
    buttons.cancel.disabled = disposed || suspended || (!active && !ready);
    panel.setAttribute("aria-busy", String(active !== null));
  }
  function retire(message, clearSelection = false) {
    const job = active, ticket = ++generation;
    active = ready = null;
    if (job?.timer !== undefined) clearTimeout(job.timer);
    job?.abort.abort();
    list.replaceChildren(); details.hidden = true;
    if (clearSelection) input.value = "";
    if (ticket === generation) {
      if (!disposed && message) status.textContent = message;
      refresh();
    }
    return ticket;
  }
  function fence(job) {
    if (active !== job || generation !== job.ticket || blocked() || job.checkpoint !== stamp() || !sameSelection(job))
      throw fail("STALE_SOURCE", "The book or selected folder changed. Review the folder again before reopening.");
  }
  function current(value) {
    return value && !blocked() && value.ticket === generation && value.checkpoint === stamp() && sameSelection(value);
  }
  function wait(value, job) {
    return new Promise((resolve, reject) => {
      let settled = false;
      const finish = (fn, result) => {
        if (settled) return;
        settled = true; job.abort.signal.removeEventListener("abort", abort); fn(result);
      };
      const abort = () => finish(reject, fail("IMPORT_CANCELLED", "Source-folder reopening cancelled."));
      job.abort.signal.addEventListener("abort", abort, { once: true });
      Promise.resolve(value).then(result => finish(resolve, result), error => finish(reject, error));
      if (job.abort.signal.aborted) abort();
    });
  }
  async function review() {
    if (blocked()) throw fail("BOOK_BUSY", "Finish source import or text composition before reviewing a folder.");
    const count = input.files?.length;
    if (!Number.isSafeInteger(count) || count < 1 || count > BOOK_SOURCE_FOLDER_LIMITS.selectedFiles)
      throw fail("FILE_LIMIT", "Choose a source folder containing 1 through 4096 files.");
    const files = Array.from({ length: count }, (_, index) => input.files[index]);
    const ticket = retire();
    if (ticket !== generation || blocked()) throw fail("IMPORT_CANCELLED", "Folder review was superseded.");
    const job = { ticket, files, checkpoint: stamp(), abort: new AbortController() };
    active = job;
    job.timer = setTimeout(() => {
      if (active === job) retire("Folder reading exceeded its deadline. Choose the folder again; the book is unchanged.");
    }, timeoutMs);
    refresh();
    try {
      fence(job);
      status.textContent = "Reading the manifest and its source files…";
      const result = await readBookSourceFolder(files, { projectCodec, signal: job.abort.signal });
      fence(job);
      clearTimeout(job.timer); delete job.timer;
      const rows = result.review.map(file => {
        const row = root.createElement("li");
        row.textContent = `${file.path} — ${file.role === "include" ? "include-only" : "chapter"}${file.changed ? "; edited since export" : ""}`;
        return row;
      });
      fence(job);
      ready = { ...job, result }; active = null;
      list.replaceChildren(...rows);
      details.hidden = false;
      status.textContent = `Reviewed ${result.folder}: ${result.chapters} chapters, ${result.includeSources} include-only files; ${result.changedSources} changed since the manifest was saved. ${result.ignored} other files ignored. Select Reopen reviewed folder to replace this book; image and font files require separate authorization.`;
      refresh();
      return Object.freeze({ sourceCount: result.sourceCount, changedSources: result.changedSources,
        chapters: result.chapters, includeSources: result.includeSources, ignored: result.ignored });
    } catch (error) {
      if (active === job) retire(`${error?.code ?? "SOURCE_FOLDER_FAILED"}: ${error?.message ?? "Folder reading failed."} The book is unchanged.`);
      throw error;
    }
  }
  function detached() {
    try { onProjectOpened(); } catch { /* Observer failure cannot revoke a completed source transaction. */ }
  }
  async function restore() {
    const prepared = ready;
    if (!current(prepared)) {
      retire("The book or selected folder changed. Review the folder again before reopening.");
      throw fail("STALE_SOURCE", "Review the current folder before reopening it.");
    }
    const job = { ...prepared, abort: new AbortController(), committing: false };
    ready = null; active = job; refresh();
    try {
      fence(job);
      const result = prepared.result;
      const approved = await wait(confirm(`Replace this book with ${result.chapters} chapters and ${result.includeSources} include-only files from “${result.folder}”? Current source files will replace the manifest's embedded text; saved order, roles and applied settings will be restored. Save current work first. Current images and fonts will be cleared and must be authorized again.`), job);
      fence(job);
      if (approved !== true) {
        active = null; ready = prepared;
        status.textContent = "Folder reopening declined. The current book is unchanged; the review remains available.";
        refresh(); return false;
      }
      // The existing file-aware host owns replacement, resource revocation and
      // prepared-output invalidation. Its own collection notifications are not
      // evidence of a competing edit. A partial host failure must still detach
      // the previous library/history identity if source ownership has changed.
      job.committing = true;
      const before = collection.sourceProjectRevision, revision = collection.revision;
      let committedCheckpoint;
      try {
        controls.replaceProject(result.project, JSON.parse(job.checkpoint)[1]);
        committedCheckpoint = controls.checkpoint();
        if (collection.sourceProjectRevision !== before + 1 || collection.revision !== revision + 1)
          throw fail("STALE_SOURCE", "The book changed while reopening the folder. Newer work was kept.");
      } finally { if (collection.sourceProjectRevision !== before) detached(); }
      job.committing = false;
      if (active !== job || generation !== job.ticket || blocked()
          || collection.sourceProjectRevision !== before + 1 || collection.revision !== revision + 1
          || controls.checkpoint() !== committedCheckpoint)
        throw fail("STALE_SOURCE", "The book changed while reopening the folder. Newer work was kept.");
      retire(`Reopened ${result.sourceCount} current source files with saved roles, order and settings. Authorize image and font files again before publishing.`, true);
      return true;
    } catch (error) {
      job.committing = false;
      if (active === job) retire(`${error?.code ?? "SOURCE_FOLDER_FAILED"}: ${error?.message ?? "Folder reopening failed."} Current source remains editable.`);
      throw error;
    }
  }
  function changed() {
    if (!disposed && !suspended && !active?.committing && (active || ready))
      retire("The book changed. Review the selected folder again before reopening.");
    else refresh();
  }
  function on(node, type, fn) {
    node.addEventListener(type, fn); listeners.push(() => node.removeEventListener(type, fn));
  }
  function invoke(fn) {
    const before = generation;
    void fn().catch(error => {
      if (!disposed && !suspended && generation === before && !active)
        status.textContent = `${error?.code ?? "SOURCE_FOLDER_FAILED"}: ${error?.message ?? "Folder operation failed."}`;
    });
  }
  on(input, "change", () => { retire("Folder selection changed. Review it before reopening."); refresh(); });
  on(buttons.review, "click", () => invoke(review));
  on(buttons.restore, "click", () => invoke(restore));
  on(buttons.cancel, "click", () => retire("Folder review cancelled. The current book is unchanged.", true));
  for (const type of ["input", "change"]) on(root, type, event => {
    if (fields.has(event.target?.id)) changed();
  });
  on(root, "compositionstart", event => {
    if (fields.has(event.target?.id)) { composing.add(event.target.id); changed(); }
  });
  on(root, "compositionend", event => { composing.delete(event.target?.id); refresh(); });
  const unsubscribe = collection.subscribe(changed), unsubscribeBusy = controls.subscribeSourceState(changed);
  refresh();
  return Object.freeze({ review, restore,
    cancel() { retire("Folder review cancelled. The current book is unchanged.", true); },
    suspend() { if (!disposed) { suspended = true; composing.clear(); retire("Folder review released while the page is suspended.", true); } },
    resume() { if (!disposed) { suspended = false; composing.clear(); refresh(); } },
    dispose() {
      if (disposed) return;
      disposed = true; retire(undefined, true); composing.clear(); unsubscribe(); unsubscribeBusy();
      for (const remove of listeners) remove();
    },
  });
}
