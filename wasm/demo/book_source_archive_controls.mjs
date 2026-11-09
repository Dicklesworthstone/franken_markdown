import { createBookSourceArchive } from "./book_source_archive.mjs";

const fail = (code, message) => Object.assign(new Error(message), { code });
const fields = new Set(["chapter-source", "chapter-path", "source-role", "chapters",
  "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]);

/** Source-only downloads remain usable without filesystem access or WASM.
 * The host owns the source model and codec; this view owns only its ZIP job/URL.
 */
export function createBookSourceArchiveControls({ root, controls, collection, projectCodec, urls = URL }) {
  if (![controls?.checkpoint, collection?.project, collection?.subscribe,
    projectCodec?.normalizeBookProject, projectCodec?.serializeBookProject,
    urls?.createObjectURL, urls?.revokeObjectURL].every(fn => typeof fn === "function"))
    throw new TypeError("Source ZIP controls require the source host, project codec and URL owner.");
  const section = root.querySelector("#publish-title")?.parentElement;
  const editor = root.querySelector("#chapter-source"), pathEditor = root.querySelector("#chapter-path");
  const roleEditor = root.querySelector("#source-role");
  if (!section || !editor || !pathEditor || !roleEditor || root.querySelector("#book-source-archive"))
    throw new Error("Missing or duplicate source ZIP controls.");
  const panel = root.createElement("div"); panel.id = "book-source-archive";
  panel.setAttribute("aria-labelledby", "book-source-archive-title");
  const make = (tag, id, text) => {
    const node = root.createElement(tag); node.id = `book-source-archive-${id}`;
    if (text !== undefined) node.textContent = text;
    panel.appendChild(node); return node;
  };
  make("h3", "title", "Editable source ZIP");
  make("p", "help", "Download all chapters and include-only files in an ordinary sources/ folder, plus book.fmdbook.json for source order, roles and applied publishing settings. The current editor text is included without applying pending publishing drafts. Original BOMs and line endings are preserved when unchanged. Images and fonts are excluded; use the portable backup to keep those resources. Nothing is uploaded or written to a connected folder.");
  const prepareButton = make("button", "prepare", "Prepare source ZIP"); prepareButton.type = "button";
  const cancelButton = make("button", "cancel", "Cancel / clear source ZIP"); cancelButton.type = "button";
  for (const button of [prepareButton, cancelButton]) button.setAttribute("aria-describedby", "book-source-archive-help book-source-archive-status");
  const status = make("p", "status", "Source ZIP is available without direct folder access or a rendering engine.");
  status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  const download = make("a", "download", "Download source ZIP"); download.hidden = true;
  download.setAttribute("aria-describedby", "book-source-archive-help book-source-archive-status");
  make("p", "limits", "Up to 256 source files, 4 MiB each and 16 MiB of source/path bytes. Paths must be portable and distinct, including after case folding and Unicode normalization. Extract sources/ to edit the ordinary files; reopen book.fmdbook.json to recover this captured project. Later external file edits do not update that project file automatically.");
  section.appendChild(panel);
  let disposed = false, suspended = false, generation = 0, active = null, ready = null;
  const listeners = [], composing = new Set();
  const createUrl = urls.createObjectURL.bind(urls), revokeUrl = urls.revokeObjectURL.bind(urls);
  const blocked = () => disposed || suspended || composing.size > 0 || controls.sourceBusy;
  const stamp = () => JSON.stringify([collection.sourceProjectRevision, controls.checkpoint()]);
  function refresh() {
    const disabled = blocked();
    prepareButton.disabled = disabled || !!active || (!disposed && !collection.files.length);
    cancelButton.disabled = disposed || suspended || (!active && !ready);
  }
  function retire(message) {
    const job = active, previous = ready, ticket = ++generation;
    active = ready = null;
    download.hidden = true; download.removeAttribute("href"); download.removeAttribute("download");
    job?.abort.abort();
    if (previous) revokeUrl(previous.url);
    if (ticket === generation) {
      if (!disposed && message) status.textContent = message;
      refresh();
    }
    return ticket;
  }
  function capture() {
    const checkpoint = stamp(), project = collection.project(), selected = controls.currentChapter;
    if (project.files.length) {
      if (!Number.isInteger(selected) || !project.files[selected])
        throw fail("INVALID_SOURCE", "Choose a current source before preparing its ZIP.");
      const file = project.files[selected], raw = editor.value, path = pathEditor.value, role = roleEditor.value;
      if (typeof raw !== "string" || typeof path !== "string")
        throw fail("INVALID_SOURCE", "The source editor is not readable.");
      // A textarea normalizes CR/CRLF to LF. A matching displayed view is not
      // an edit and must not discard an imported file's exact original bytes.
      if (raw !== file.source && raw !== file.source.replace(/\r\n?/g, "\n")) file.source = raw;
      file.path = path;
      if (role === "include") {
        file.role = "include"; project.schemaVersion = Math.max(2, project.schemaVersion);
      } else if (role === "chapter") delete file.role;
      else throw fail("INVALID_ROLE", "Choose chapter or include-only source before preparing a ZIP.");
    }
    if (checkpoint !== stamp() || blocked())
      throw fail("STALE_SOURCE", "The editor changed while capturing source files.");
    return { project, checkpoint };
  }
  function fence(job) {
    if (active !== job || generation !== job.ticket || blocked() || job.checkpoint !== stamp())
      throw fail("STALE_SOURCE", "The book or editor changed. Prepare the source ZIP again.");
  }
  async function prepare() {
    if (blocked()) throw fail("BOOK_BUSY", "Finish source import or text composition before preparing a source ZIP.");
    const ticket = retire();
    if (ticket !== generation || blocked()) throw fail("EXPORT_CANCELLED", "Source ZIP preparation was superseded.");
    const job = { ticket, abort: new AbortController(), checkpoint: null };
    active = job; refresh();
    let url = null;
    try {
      const captured = capture(); job.checkpoint = captured.checkpoint;
      fence(job);
      status.textContent = "Preparing exact source files and project settings. Cancel or edit to stop this download.";
      const output = await createBookSourceArchive(captured.project, { projectCodec, signal: job.abort.signal });
      fence(job);
      url = createUrl(output.blob);
      fence(job); // URL adapters can reenter source/lifecycle ownership.
      ready = { url, checkpoint: job.checkpoint, ticket };
      active = null;
      download.href = url; download.download = output.filename; download.hidden = false;
      url = null;
      status.textContent = `${output.sourceCount} source files and book.fmdbook.json prepared (${output.byteLength.toLocaleString()} ZIP bytes). Click Download source ZIP to save it. Images and fonts are excluded; preparation has not saved a file.`;
      refresh();
      return output;
    } catch (error) {
      if (url !== null) revokeUrl(url);
      if (active === job) {
        active = null;
        status.textContent = `${error?.code ?? "SOURCE_ZIP_FAILED"}: ${error?.message ?? "Source ZIP preparation failed."} Current source is retained.`;
        refresh();
      }
      throw error;
    }
  }
  function changed() {
    if (!disposed && !suspended) retire("Source or editor state changed. Prepare a new source ZIP from the current book.");
  }
  function on(node, type, listener) {
    node.addEventListener(type, listener); listeners.push(() => node.removeEventListener(type, listener));
  }
  on(prepareButton, "click", () => {
    const before = generation;
    void prepare().catch(error => {
      if (!disposed && !suspended && generation === before && !active && !ready)
        status.textContent = `${error?.code ?? "SOURCE_ZIP_FAILED"}: ${error?.message ?? "Source ZIP preparation failed."} Current source is retained.`;
    });
  });
  on(cancelButton, "click", () => retire("Source ZIP cancelled and cleared. Current source and connected folders are unchanged."));
  on(download, "click", event => {
    if (!ready || blocked() || ready.ticket !== generation || ready.checkpoint !== stamp()) {
      event.preventDefault();
      retire("The source ZIP is stale. Prepare it again before downloading.");
    }
  });
  for (const type of ["input", "change"]) on(root, type, event => {
    if (fields.has(event.target?.id)) changed();
  });
  on(root, "compositionstart", event => {
    if (fields.has(event.target?.id)) { composing.add(event.target); changed(); }
  });
  on(root, "compositionend", event => { composing.delete(event.target); if (!disposed) refresh(); });
  const unsubscribe = collection.subscribe(changed);
  const unsubscribeBusy = controls.subscribeSourceState?.(() => {
    if (disposed || suspended) return;
    if (controls.sourceBusy) changed(); else refresh();
  });
  refresh();
  return Object.freeze({ prepare,
    cancel() { if (!disposed) retire("Source ZIP cancelled and cleared. Current source is retained."); },
    suspend() { if (!disposed) { suspended = true; composing.clear(); retire("Source ZIP cleared while this page is inactive."); } },
    resume() { if (!disposed) { suspended = false; refresh(); } },
    dispose() {
      if (disposed) return;
      disposed = true; composing.clear(); retire(); unsubscribe(); unsubscribeBusy?.();
      for (const remove of listeners) remove();
    },
  });
}
