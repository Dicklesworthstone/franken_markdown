import { BookFileError, createBookFileSession } from "./book_file_session.mjs";

/** The host supplies its existing source-project codec. Nothing here owns a
 * parser, renderer, resource grant, local-library record or persistent handle. */
export function createBookFileControls({ root, controls, collection, window,
  projectCodec, onProjectOpened = () => {} }) {
  const { normalizeBookProject, readBookProject, serializeBookProject } = projectCodec ?? {};
  if (![normalizeBookProject, readBookProject, serializeBookProject, onProjectOpened,
    controls?.captureProject, controls?.replaceProject, controls?.checkpoint,
    collection?.project, collection?.subscribe].every(fn => typeof fn === "function"))
    throw new TypeError("Connected book files require the existing source-project codec and transaction host.");
  const section = root.querySelector("#publish-title")?.parentElement;
  if (!section || root.querySelector("#book-files")) throw new Error("Missing or duplicate book-file panel.");
  const panel = root.createElement("div"); panel.id = "book-files";
  panel.setAttribute("aria-labelledby", "book-file-title");
  const make = (tag, id, text) => {
    const node = root.createElement(tag); node.id = `book-file-${id}`; node.textContent = text;
    panel.appendChild(node); return node;
  };
  make("h3", "title", "Connected source-project file");
  make("p", "help", "Open one local source-project JSON file and save back to it explicitly. Source, chapter/include roles, order and applied publishing settings are saved; images, fonts, permissions and unapplied setting drafts are NOT. Use portable backup to retain resource bytes. Opening or reloading replaces the book and revokes current resources after confirmation. Handles stay in this page session and are disconnected on suspension.");
  const buttons = Object.fromEntries([
    ["open", "Open project file"], ["save", "Save source project"],
    ["save-as", "Save source project as"], ["reload", "Reload connected file"],
    ["disconnect", "Disconnect file"],
  ].map(([id, text]) => {
    const node = make("button", id, text); node.type = "button"; node.disabled = true;
    node.setAttribute("aria-describedby", "book-file-help book-file-status"); return [id, node];
  }));
  buttons.save.setAttribute("aria-keyshortcuts", "Control+s Meta+s");
  buttons["save-as"].setAttribute("aria-keyshortcuts", "Control+Shift+s Meta+Shift+s");
  const status = make("p", "status", ""); status.setAttribute("role", "status");
  make("p", "limits", "Disk changes are checked byte-for-byte before commit and the saved bytes are read back. This is not an OS-wide lock: another application can race the final check. A conflict or uncertain save needs explicit reload or a different destination; no silent overwrite or retry. Maximum JSON size is 64 MiB. Ordinary imports/downloads and local-library saves remain separate.");
  section.appendChild(panel);
  const canOpen = typeof window?.showOpenFilePicker === "function";
  const canPickSave = typeof window?.showSaveFilePicker === "function";
  let disposed = false, suspended = false, serial = 0, session = null;
  let cachedRevision = null, cachedSource = null, cachedKey = null;
  const composing = new Set(), listeners = [];
  const fields = new Set(["chapter-source", "chapter-path", "source-role", "title", "author", "lang",
    "font", "dark-mode", "font-scale", "toc", "page-numbers"]);
  const busy = () => disposed || suspended || composing.size > 0 || controls.sourceBusy;
  // Source schemas 1-4 can describe the same project. Use the current schema
  // solely for identity; disk output keeps the serializer's ordinary schema.
  const projectKey = project => serializeBookProject({ ...project, schemaVersion: 4 });
  function read() {
    if (cachedRevision !== collection.revision || cachedSource === null) {
      const project = collection.project();
      const source = serializeBookProject(project), key = projectKey(project);
      cachedSource = source; cachedKey = key; cachedRevision = collection.revision;
    }
    return { source: cachedSource, key: cachedKey, stamp: controls.checkpoint() };
  }
  function refresh(value = session?.state) {
    if (disposed || !value) return;
    const blocked = busy() || !!value.busy;
    buttons.open.disabled = blocked || !canOpen;
    buttons.save.disabled = blocked || !value.canSave;
    buttons["save-as"].disabled = blocked || !canPickSave || !value.valid;
    buttons.reload.disabled = blocked || !value.filename;
    buttons.disconnect.disabled = !value.filename && !value.busy;
    const capability = !canOpen || !canPickSave
      ? " Some direct-file operations are unavailable here; use the existing source-project import/download controls." : "";
    status.textContent = `${value.filename ?? "No connected file"} — ${value.phase}${value.busy ? ` (${value.busy})` : ""}. `
      + (value.phase === "edited" ? "Current changes are not verified on disk. " : "") + value.message + capability;
  }
  session = createBookFileSession({ read, isBlocked: busy, onState: refresh,
    decode: async file => {
      const project = normalizeBookProject(await readBookProject(file));
      return { project, key: projectKey(project) };
    },
    replace: (project, stamp) => {
      controls.replaceProject(project, stamp);
      // Detach an unrelated local-library identity and reset source history.
      // Observers cannot turn a completed host transaction into a failed write.
      try { onProjectOpened(); } catch { /* Source ownership stays with the host. */ }
    },
  });
  function admitted() {
    if (disposed || suspended) throw new BookFileError("SESSION_DISPOSED", "Connected project files are not active.");
    if (busy() || session.state.busy) throw new BookFileError("FILE_BUSY", "Finish importing, composition or the current file operation first.");
    // Capture raw UI input with the SAME validation/ownership route as export.
    // This is synchronous, so the picker remains in the initiating user gesture.
    return controls.captureProject();
  }
  const confirmOpen = ({ filename }) => window.confirm(
    `Replace the current book with source project "${filename}"? Save current work first. All current image and font access will be revoked. Source-only files do not restore resources.`);
  const confirmOverwrite = ({ filename }) => window.confirm(
    `Replace all contents of "${filename}" with the current source project? This saves source and applied settings only, not images or fonts.`);
  const pickerTypes = () => [{ description: "Franken Markdown source project", accept: { "application/json": [".json"] } }];
  function openFile() {
    admitted();
    return session.open(canOpen ? () => window.showOpenFilePicker({ multiple: false, excludeAcceptAllOption: true, types: pickerTypes() }) : null, confirmOpen);
  }
  function saveAs() {
    const project = admitted();
    const name = (project.options.title.replace(/[^a-z0-9_-]+/gi, "-").replace(/^-+|-+$/g, "").slice(0, 80) || "book") + ".fmd-project.json";
    return session.saveAs(canPickSave ? () => window.showSaveFilePicker({ suggestedName: name, excludeAcceptAllOption: true, types: pickerTypes() }) : null, confirmOverwrite);
  }
  function save() { admitted(); return session.save(); }
  function reload() { admitted(); return session.reload(confirmOpen); }
  function detach() { if (!disposed) { serial++; session.detach(); } }
  function invoke(work) {
    const ticket = ++serial;
    const report = error => {
      if (!disposed && !suspended && ticket === serial)
        status.textContent = `${error.code ?? "FILE_OPERATION_FAILED"}: ${error.message ?? "Project file access failed."} Current book source is retained.`;
    };
    try { Promise.resolve(work()).catch(report); } catch (error) { report(error); }
  }
  function on(node, type, listener) {
    node?.addEventListener(type, listener); listeners.push(() => node?.removeEventListener(type, listener));
  }
  on(buttons.open, "click", () => invoke(openFile));
  on(buttons.save, "click", () => invoke(save));
  on(buttons["save-as"], "click", () => invoke(saveAs));
  on(buttons.reload, "click", () => invoke(reload));
  on(buttons.disconnect, "click", detach);
  for (const type of ["input", "change"]) on(root, type, event => {
    if (!disposed && !suspended && fields.has(event.target?.id)) session.changed();
  });
  on(root, "compositionstart", event => {
    if (fields.has(event.target?.id)) { composing.add(event.target); session.changed(); refresh(); }
  });
  on(root, "compositionend", event => { composing.delete(event.target); refresh(); });
  on(root, "keydown", event => {
    if (disposed || suspended || event.defaultPrevented || event.isComposing || composing.size
        || event.altKey || !(event.ctrlKey || event.metaKey) || event.key?.toLowerCase() !== "s") return;
    event.preventDefault();
    invoke(() => {
      if (!canPickSave && !session.state.filename) { admitted(); return controls.prepareSource(true); }
      return event.shiftKey || !session.state.filename ? saveAs() : save();
    });
  });
  on(window, "beforeunload", event => {
    if (disposed || suspended) return;
    const value = session.state;
    if (value.busy || (value.filename && value.dirty)) { event.preventDefault(); event.returnValue = ""; }
  });
  const unsubscribe = collection.subscribe(() => { cachedRevision = null; session.changed(); });
  const unsubscribeBusy = controls.subscribeSourceState?.(() => refresh());
  // All later workbench components receive this descriptor-preserving facade.
  // A successful library/new-book recovery is an identity boundary EVEN WHEN
  // its source is byte-identical. Ordinary source/setting edits keep ownership.
  // Do not spread a controller: that snapshots its live sourceBusy/getter state.
  const descriptors = Object.getOwnPropertyDescriptors(controls);
  descriptors.replaceProject = { ...descriptors.replaceProject, value(...args) {
    const before = controls.checkpoint();
    try { const result = controls.replaceProject(...args); detach(); return result; }
    catch (error) {
      // A host can install source and then fail updating its view. Do not leave
      // the old file authorized for that partially completed project switch.
      try { if (controls.checkpoint() !== before) detach(); } catch { detach(); }
      throw error;
    }
  } };
  const connectedControls = Object.freeze(Object.defineProperties({}, descriptors));
  refresh();
  return Object.freeze({ controls: connectedControls, open: openFile, save, saveAs, reload, detach,
    get state() { return session.state; },
    suspend() {
      if (disposed) return;
      serial++; session.detach(); suspended = true; composing.clear();
      cachedRevision = cachedSource = cachedKey = null; refresh();
    },
    resume() { if (!disposed) { suspended = false; cachedRevision = null; refresh(); } },
    dispose() {
      if (disposed) return;
      disposed = true; serial++; unsubscribe(); unsubscribeBusy?.();
      for (const remove of listeners) remove(); session.dispose(); composing.clear();
      cachedRevision = cachedSource = cachedKey = null;
      for (const button of Object.values(buttons)) button.disabled = true;
    },
  });
}
