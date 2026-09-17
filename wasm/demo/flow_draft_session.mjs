// Opt-in, latest-source autosave. One physical operation and one current editor
// state, not a growing queue of full documents. Storage acknowledgments never
// replace newer editor text. No worker or image authorization is persisted.
import { FlowError } from "../flow_session.mjs";
import { documentSnapshot } from "./flow_document.mjs";
const same = (a, b) => a?.source === b?.source && a?.filename === b?.filename;
const fail = (code, message) => { throw new FlowError(code, message); };
export function createDraftSession({ store, readDocument, restoreDocument,
  confirmRestore = () => true, onState = () => {}, delayMs = 400 }) {
  if (!Number.isInteger(delayMs) || delayMs < 0 || delayMs > 60000) fail("INVALID_OPTIONS", "Invalid autosave delay.");
  let disposed = false, ready = false, enabled = false, revision = null, candidate = null;
  let saved = null, dirty = true, epoch = 0, operation = null, operationKind = null, timer = null, failure = null;
  const alive = () => { if (disposed) fail("SESSION_DISPOSED", "Draft session is disposed."); };
  const snapshot = () => documentSnapshot(readDocument());
  const state = () => Object.freeze({ disposed, ready, enabled, dirty, busy: operation !== null,
    operation: operationKind, version: revision, available: candidate !== null,
    storedFilename: candidate?.filename ?? saved?.filename ?? null,
    error: failure ? Object.freeze({ code: failure.code ?? "STORAGE_UNAVAILABLE", message: failure.message ?? "Draft storage failed." }) : null });
  const notify = () => { try { onState(state()); } catch { /* UI observers cannot undo a committed save. */ } };
  const stopTimer = () => { if (timer !== null) clearTimeout(timer); timer = null; };
  const validate = result => {
    if (!result || result.schemaVersion !== 1 || !Number.isSafeInteger(result.version) || result.version < 0
        || (result.document !== null && result.document === undefined)) fail("STORAGE_PROTOCOL_ERROR", "Invalid draft acknowledgment.");
    return { version: result.version, document: result.document === null ? null : documentSnapshot(result.document) };
  };
  function schedule() {
    stopTimer();
    if (!disposed && enabled && dirty && operation === null) timer = setTimeout(() => { timer = null; void flush().catch(() => {}); }, delayMs);
  }
  function launch(kind, work) {
    alive();
    if (operation !== null) fail("DRAFT_BUSY", "The previous draft operation has not settled.");
    operationKind = kind;
    // Assign ownership BEFORE invoking callbacks, including synchronous stores.
    const job = Promise.resolve().then(work).catch(error => {
      if (!disposed) { failure = error; enabled = false; dirty = true; }
      throw error;
    }).finally(() => {
      if (operation === job) { operation = null; operationKind = null; }
      if (!disposed) { notify(); schedule(); }
    });
    operation = job;
    notify();
    return job;
  }
  async function flush() {
    alive(); stopTimer();
    if (!enabled || !dirty) return state();
    if (operation !== null) {
      if (operationKind !== "save") fail("DRAFT_BUSY", "Recovery is in progress.");
      await operation; // Only the newest desired source is captured afterward.
      return flush();
    }
    let value;
    try { value = snapshot(); }
    catch (error) { failure = error; enabled = false; dirty = true; notify(); throw error; }
    const ticket = epoch, expected = revision;
    if (!ready || candidate !== null || expected === null) fail("RECOVERY_PENDING", "Inspect and restore or forget the existing draft first.");
    await launch("save", async () => {
      const result = validate(await store.save(value, expected));
      if (result.version !== expected + 1 || !same(result.document, value)) fail("STORAGE_PROTOCOL_ERROR", "Save acknowledgment does not match the submitted source.");
      if (disposed) return;
      revision = result.version; saved = value; failure = null;
      // Even a programmatic edit without an input event must remain dirty.
      dirty = ticket !== epoch || !same(value, snapshot());
    });
    if (enabled && dirty) return flush();
    return state();
  }
  function pause() { alive(); enabled = false; stopTimer(); notify(); }
  function inspect() {
    pause();
    return launch("inspect", async () => {
      const result = validate(await store.read());
      if (disposed) return;
      revision = result.version; candidate = result.document; ready = true; saved = null; failure = null;
      dirty = true; // Finding a copy does not mean the host restored it.
    });
  }
  function restore() {
    pause(); const before = snapshot(), ticket = epoch;
    return launch("restore", async () => {
      const result = validate(await store.read());
      alive();
      if (ticket !== epoch || !same(before, snapshot())) fail("STALE_SOURCE", "Editor changed while reading recovery; nothing was replaced.");
      if (result.document === null) fail("NO_DRAFT", "The stored draft was removed; current Markdown is unchanged.");
      const approved = await confirmRestore(result.document);
      alive();
      if (ticket !== epoch || !same(before, snapshot())) fail("STALE_SOURCE", "Editor changed during recovery confirmation; nothing was replaced.");
      if (approved !== true) return;
      // The host revokes old image authority while installing this source.
      restoreDocument(result.document);
      revision = result.version; saved = result.document; candidate = null; ready = true; failure = null;
      dirty = !same(saved, snapshot());
    });
  }
  function forget() {
    pause();
    if (!ready || revision === null) fail("RECOVERY_PENDING", "Inspect the stored revision before removing it.");
    const expected = revision;
    return launch("forget", async () => {
      const result = validate(await store.clear(expected));
      if (result.version !== expected + 1 || result.document !== null) fail("STORAGE_PROTOCOL_ERROR", "Removal acknowledgment does not match the requested revision.");
      if (disposed) return;
      revision = result.version; candidate = null; saved = null; dirty = true; failure = null;
    });
  }
  return Object.freeze({
    get state() { return state(); },
    inspect, restore, forget, flush,
    setEnabled(value) {
      alive();
      if (typeof value !== "boolean") fail("INVALID_OPTIONS", "Autosave choice must be boolean.");
      if (!value) { pause(); return; }
      if (operation !== null) fail("DRAFT_BUSY", "Wait for the current draft operation before enabling autosave.");
      if (!ready || candidate !== null) fail("RECOVERY_PENDING", "Restore or explicitly forget the stored draft before enabling autosave.");
      if (failure?.code === "DRAFT_CONFLICT") fail("DRAFT_CONFLICT", "Refresh recovery before resolving another tab's changes.");
      const nextDirty = !same(saved, snapshot());
      failure = null; enabled = true; dirty = nextDirty; notify(); schedule();
    },
    changed() {
      if (disposed) return;
      epoch++; dirty = true; notify(); schedule();
    },
    dispose() {
      if (disposed) return;
      disposed = true; enabled = false; stopTimer();
      store.dispose(); candidate = null; saved = null; notify();
    }
  });
}
