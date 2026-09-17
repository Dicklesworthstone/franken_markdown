// DOM wiring for opt-in source recovery. It imports no renderer/WASM entrypoint,
// so a broken preview cannot take away the user's source or recovery controls.
import { FlowError } from "../flow_session.mjs";
import { createDraftSession } from "./flow_draft_session.mjs";
import { openDraftStore } from "./flow_draft_store.mjs";

export function createDraftControls({ sourceEditor, filename, remember, refresh, restore, forget, save, status,
  readDocument, restoreDocument, confirmRestore, confirmForget = () => true,
  openStore = openDraftStore, delayMs = 400 }) {
  let disposed = false, owner = null, operation = null, failure = null;
  const alive = () => { if (disposed) throw new FlowError("SESSION_DISPOSED", "Draft controls are disposed."); };
  function render() {
    const value = owner?.state, busy = operation !== null || value?.busy === true;
    remember.checked = value?.enabled === true;
    remember.disabled = disposed || busy || !value?.ready || value.available || value.error?.code === "DRAFT_CONFLICT";
    refresh.disabled = disposed || busy;
    restore.disabled = disposed || busy || !value?.available;
    forget.disabled = disposed || busy || !value?.ready || value.storedFilename === null;
    save.disabled = disposed || busy || !value?.enabled || !value.dirty;
    if (disposed) return;
    const error = failure ?? value?.error;
    if (error) status.textContent = `${error.code ?? "STORAGE_UNAVAILABLE"}: ${error.message ?? "Draft storage failed."} Download Markdown to keep an independent copy; Refresh recovery retries storage.`;
    else if (busy) status.textContent = "Waiting for a local draft transaction. The current source remains editable.";
    else if (value?.available) status.textContent = `A local draft (${value.storedFilename}) is available. Restore or explicitly forget it before enabling autosave. The editor has not been replaced.`;
    else if (!value?.ready) status.textContent = "Local draft storage is not ready. Opening and downloading Markdown still work.";
    else if (value.dirty) status.textContent = value.enabled
      ? "Changes are awaiting a local draft commit. Closing before acknowledgment may lose these changes."
      : "Autosave is off. Current changes are not acknowledged in draft storage; download Markdown or enable local autosave.";
    else status.textContent = `Current source is acknowledged in local draft revision ${value.version}. Autosave is ${value.enabled ? "on" : "off"}. Browser storage is not a backup.`;
  }
  function perform(work) {
    alive();
    if (operation !== null || owner?.state.busy) throw new FlowError("DRAFT_BUSY", "The previous draft operation has not settled.");
    failure = null;
    const job = Promise.resolve().then(work).catch(error => {
      if (!disposed) failure = error;
      throw error;
    }).finally(() => {
      if (operation === job) operation = null;
      render();
    });
    operation = job; render(); return job;
  }
  function refreshRecovery() {
    return perform(async () => {
      owner?.dispose(); owner = null;
      const store = await openStore();
      if (disposed) { store.dispose(); return; }
      try {
        owner = createDraftSession({ store, readDocument, restoreDocument, confirmRestore,
          delayMs, onState: render });
      } catch (error) { store.dispose(); throw error; }
      await owner.inspect(); // Read only; never automatically enable or restore.
    });
  }
  function restoreRecovery() {
    return perform(async () => {
      alive();
      if (!owner) throw new FlowError("STORAGE_UNAVAILABLE", "Refresh recovery to open draft storage.");
      await owner.restore();
    });
  }
  function forgetRecovery() {
    return perform(async () => {
      alive();
      const current = owner;
      if (!current) throw new FlowError("STORAGE_UNAVAILABLE", "Refresh recovery to open draft storage.");
      current.setEnabled(false); // Do not race a debounced save with confirmation.
      const approved = await confirmForget(current.state.storedFilename);
      alive();
      if (approved === true) await current.forget();
    });
  }
  function flush() {
    return perform(async () => { alive(); if (owner) await owner.flush(); });
  }
  function invoke(work) {
    try { Promise.resolve(work()).catch(() => {}); }
    catch (error) { failure = error; render(); }
  }
  const onRemember = () => {
    try {
      alive();
      if (operation !== null || !owner) throw new FlowError("DRAFT_BUSY", "Draft storage is not ready for a new choice.");
      failure = null; owner.setEnabled(remember.checked);
    } catch (error) { failure = error; render(); }
  };
  const onRefresh = () => invoke(refreshRecovery), onRestore = () => invoke(restoreRecovery);
  const onForget = () => invoke(forgetRecovery), onSave = () => invoke(flush);
  const onInput = () => { if (!disposed) owner?.changed(); };
  remember.addEventListener("change", onRemember); refresh.addEventListener("click", onRefresh);
  restore.addEventListener("click", onRestore); forget.addEventListener("click", onForget); save.addEventListener("click", onSave);
  sourceEditor.addEventListener("input", onInput); filename.addEventListener("input", onInput);
  invoke(refreshRecovery);
  return Object.freeze({
    get state() { return owner?.state ?? null; },
    refresh: refreshRecovery, restore: restoreRecovery, forget: forgetRecovery, flush,
    async whenIdle() { if (operation) await operation; },
    dispose() {
      if (disposed) return;
      disposed = true; owner?.dispose(); owner = null; render();
      remember.removeEventListener("change", onRemember); refresh.removeEventListener("click", onRefresh);
      restore.removeEventListener("click", onRestore); forget.removeEventListener("click", onForget); save.removeEventListener("click", onSave);
      sourceEditor.removeEventListener("input", onInput); filename.removeEventListener("input", onInput);
    }
  });
}
