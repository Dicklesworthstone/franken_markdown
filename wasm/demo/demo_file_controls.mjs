// The ordinary demo shares Flow's bounded file transaction engine. This panel
// adapts the demo's source owner; handles never enter renderers or draft storage.
import { createFileDocumentSession } from "./flow_file_session.mjs";
import { documentName, trackSourceAccess } from "./flow_document.mjs";

const failure = (code, message) => Object.assign(new Error(message), { code });
const actions = ["open", "save", "save-as", "reload", "keep", "backup", "disconnect"];
const types = [{ description: "UTF-8 Markdown", accept: { "text/plain": [".md", ".markdown", ".txt"] } }];

export function createDemoFileControls({ root, window, sourceEditor, readDocument, replaceDocument,
  renameDocument, downloadMarkdown, beforeOpen = () => {}, isBlocked = () => false, onState = () => {} }) {
  const el = Object.fromEntries([...actions, "status"].map(key => [key, root.querySelector(`#direct-file-${key}`)]));
  if (Object.values(el).some(value => !value)) throw new TypeError("Direct Markdown file controls are missing.");
  if ([readDocument, replaceDocument, renameDocument, downloadMarkdown, beforeOpen, isBlocked, onState].some(value => typeof value !== "function"))
    throw new TypeError("Direct file controls require source-owner callbacks.");
  const canOpen = window.isSecureContext === true && typeof window.showOpenFilePicker === "function";
  const canChoose = window.isSecureContext === true && typeof window.showSaveFilePicker === "function";
  let disposed = false, suspended = false, composing = false, touched = false, unloadListening = false, session;
  const listeners = [];
  const access = trackSourceAccess([sourceEditor], () => { session?.changed(); refresh(); });
  const blocked = () => disposed || suspended || composing || access.blocked() || isBlocked();
  const unavailable = () => failure("FILE_ACCESS_CLOSED", "File access is disconnected. Return to the active editor and choose the file again.");
  function beforeUnload(event) {
    const state = session.state;
    if (!disposed && !suspended && touched && (state.dirty || state.busy || ["conflict", "uncertain"].includes(state.phase))) {
      event.preventDefault(); event.returnValue = "";
    }
  }
  function warn(enabled) {
    if (enabled === unloadListening) return;
    unloadListening = enabled;
    window[enabled ? "addEventListener" : "removeEventListener"]("beforeunload", beforeUnload);
  }
  function refresh(state = session?.state) {
    if (disposed || suspended || !state) return;
    const busy = blocked() || !!state.busy;
    el.open.disabled = busy || !canOpen;
    el.save.disabled = busy || !state.canSave;
    el["save-as"].disabled = busy || !canChoose || !state.valid;
    el.reload.disabled = busy || !state.filename;
    el.disconnect.disabled = !state.filename && !state.busy;
    el.backup.disabled = blocked() || !state.valid;
    el.keep.disabled = blocked();
    const phases = {
      unlinked: "No file is connected. Source downloads and browser drafts are separate copies.",
      saved: "Current Markdown matches the last verified file save. External changes are checked when saving.",
      edited: "The current Markdown has unsaved file changes.",
      renamed: "The source filename changed. Use Save Markdown as to choose the intended file.",
      conflict: "The file changed externally. Current-file saving is paused; keep editing, save to another file, or reload explicitly.",
      uncertain: "The save could not be verified. Keep a recovery copy, then reload explicitly or save to a different file.",
      invalid: "The source or filename exceeds the supported Markdown file limits. Your editor text is retained.",
    };
    el.status.textContent = (!canOpen && !canChoose ? "Direct file pickers are unavailable here. Open Markdown and Download Markdown still work. " : "")
      + (state.filename ? `Connected file: ${state.filename}. ` : "") + (phases[state.phase] ?? "") + ` ${state.message}`;
    warn(touched && (state.dirty || !!state.busy || ["conflict", "uncertain"].includes(state.phase)));
    onState(state);
  }
  function hostTransaction(owner, action) {
    const revision = access.synchronize();
    if (owner !== session || blocked()) throw unavailable();
    const result = action();
    // Input/focus callbacks may lock, detach, suspend or replace this owner.
    // Their newer state is retained; it cannot acquire an older file handle.
    if (owner !== session || access.synchronize() !== revision || blocked())
      throw failure("STALE_SOURCE", "Editor access changed during file installation; no file was connected.");
    return result;
  }
  function start() {
    const owner = createFileDocumentSession({
      readDocument,
      replaceDocument: next => hostTransaction(owner, () => replaceDocument(next)),
      renameDocument: name => hostTransaction(owner, () => renameDocument(name)),
      isBlocked: () => { access.synchronize(); return owner !== session || blocked(); },
      onState: state => { if (session === owner) refresh(state); },
    });
    session = owner;
    refresh();
  }
  function execute(action, replacing = false) {
    try {
      access.synchronize();
      if (disposed || suspended) throw unavailable();
      if (blocked() || session.state.busy) throw failure("FILE_BUSY", "Finish the current file operation, source import or text composition first.");
      const owner = session;
      if (replacing) beforeOpen();
      if (owner !== session || blocked()) throw unavailable();
      touched = true;
      // Keep pickers in the original click/key task, before any promise hop.
      return Promise.resolve(action(owner));
    } catch (error) { return Promise.reject(error); }
  }
  const confirmOpen = ({ filename }) => window.confirm(
    `Replace the editor with ${filename}? Download Markdown first to keep the current source. Raw HTML will be turned off and local image access revoked.`,
  );
  const open = () => execute(owner => owner.open(canOpen ? () => window.showOpenFilePicker({ multiple: false,
    excludeAcceptAllOption: true, types }) : undefined, confirmOpen), true);
  const reload = () => execute(owner => owner.reload(confirmOpen), true);
  function backup() {
    if (blocked()) throw unavailable();
    if (downloadMarkdown() !== true) throw failure("SOURCE_DOWNLOAD_FAILED", "Markdown download could not start. The source remains in the editor.");
    el.status.textContent = "Recovery Markdown download requested. Check your downloads. This did not overwrite the connected file or mark it saved.";
    return Object.freeze({ downloaded: true });
  }
  function saveAs() {
    if (!canChoose) {
      try { return Promise.resolve(backup()); } catch (error) { return Promise.reject(error); }
    }
    return execute(owner => owner.saveAs(name => window.showSaveFilePicker({
      suggestedName: documentName(/\.(md|markdown|txt)$/i.test(name) ? name : `${name}.md`),
      excludeAcceptAllOption: true, types,
    }), ({ filename }) => window.confirm(`Overwrite the existing contents of ${filename} with this Markdown source? This changes the selected file on disk.`)));
  }
  const save = () => session.state.filename && session.state.phase !== "renamed"
    ? execute(owner => owner.save()) : saveAs();
  function changed() { if (!disposed && !suspended) { touched = true; session.changed(); } }
  function replaced() { if (!disposed && !suspended) { touched = true; session.detach(); } }
  function report(error, owner) {
    if (disposed || suspended || session !== owner) return;
    refresh();
    el.status.textContent += ` ${error?.code ?? "FILE_OPERATION_FAILED"}: ${error?.message ?? "File access failed; download Markdown to keep a separate copy."}`;
  }
  function invoke(action, event) {
    if (disposed || suspended || event?.defaultPrevented || event?.currentTarget?.disabled) return;
    const owner = session;
    try { Promise.resolve(action()).catch(error => report(error, owner)); }
    catch (error) { report(error, owner); }
  }
  const listen = (target, type, callback) => { target.addEventListener(type, callback); listeners.push(() => target.removeEventListener(type, callback)); };
  for (const [id, action] of Object.entries({ open, save, "save-as": saveAs, reload,
    keep: () => sourceEditor.focus(), backup, disconnect: () => session.detach() }))
    listen(el[id], "click", event => invoke(action, event));
  listen(sourceEditor, "input", changed);
  listen(sourceEditor, "compositionstart", () => { composing = true; changed(); refresh(); });
  listen(sourceEditor, "compositionend", () => { composing = false; changed(); refresh(); });
  listen(window, "focus", () => refresh());
  listen(window, "keydown", event => {
    if (disposed || suspended || event.defaultPrevented || event.altKey || !(event.ctrlKey || event.metaKey)
      || event.key?.toLowerCase() !== "s" || event.isComposing || event.keyCode === 229 || composing) return;
    event.preventDefault();
    if (!event.repeat) invoke(event.shiftKey ? saveAs : save);
  });
  function pause() {
    suspended = true;
    composing = false;
    session.dispose();
    warn(false);
    for (const id of actions) el[id].disabled = true;
    el.status.textContent = "Direct file access disconnected. Source is retained; choose the file again after returning to this page.";
    onState(null);
  }
  start();
  return Object.freeze({
    open, save, saveAs, reload, backup, changed, replaced, refresh,
    disconnect() { if (!disposed && !suspended) session.detach(); },
    get state() { return session.state; },
    suspend() { if (!disposed && !suspended) pause(); },
    resume() { if (!disposed && suspended) { suspended = false; start(); } },
    dispose() {
      if (disposed) return;
      pause(); disposed = true;
      access.dispose();
      for (const remove of listeners) remove();
    },
  });
}
