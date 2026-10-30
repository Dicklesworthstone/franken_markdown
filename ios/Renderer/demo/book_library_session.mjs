// Recovery/autosave orchestration. Hosts own editing and explicit confirmation;
// the store owns atomic revisions. No Markdown rendering or database emulation.
import { bookError } from "../book_worker.mjs";

export function createBookLibrarySession({
  store,
  host,
  confirm = () => false,
  onState = () => {},
  debounceMs = 1000,
  timers = globalThis,
}) {
  if (!Number.isInteger(debounceMs) || debounceMs < 1 || debounceMs > 60000)
    throw bookError("INVALID_OPTIONS", "Invalid autosave delay.");
  let binding = null,
    name = "My book",
    entries = [],
    dirty = false,
    busy = false,
    autosave = false,
    conflict = false;
  let disposed = false,
    suspended = false,
    composing = false,
    timer = null,
    documentGeneration = 0,
    lifecycle = 0;
  let message = "Local saves are optional. Save a named copy to start; autosave is off.";
  const state = () =>
    Object.freeze({
      binding: binding && { ...binding },
      name,
      entries: structuredClone(entries),
      dirty,
      busy,
      autosave,
      conflict,
      suspended,
      composing,
      message,
    });
  const emit = () => {
    if (!disposed) {
      try {
        onState(state());
      } catch {
        /* An observer cannot undo a storage acknowledgment. */
      }
    }
  };
  const alive = () => {
    if (disposed) throw bookError("LIBRARY_CLOSED", "The library session is closed.");
    if (suspended)
      throw bookError("LIBRARY_SUSPENDED", "Return to the active page before using local storage.");
  };
  function clearTimer() {
    if (timer !== null) {
      timers.clearTimeout(timer);
      timer = null;
    }
  }
  function schedule() {
    clearTimer();
    if (
      !disposed &&
      !suspended &&
      !busy &&
      !composing &&
      autosave &&
      dirty &&
      binding &&
      !conflict
    ) {
      timer = timers.setTimeout(() => {
        timer = null;
        void save().catch(() => {});
      }, debounceMs);
    }
  }
  function failed(error) {
    autosave = false;
    clearTimer();
    if (error?.code === "LIBRARY_CONFLICT") conflict = true;
    message = `${error?.code ?? "LIBRARY_ERROR"}: ${error?.message ?? "Local save failed."} Autosave is off. Source is still in the editor.`;
  }
  async function run(work) {
    alive();
    if (busy || composing)
      throw bookError(
        "BOOK_BUSY",
        "Finish the active library operation or text composition first.",
      );
    const generation = documentGeneration;
    busy = true;
    clearTimer();
    emit();
    try {
      return await work();
    } catch (error) {
      if (!disposed) {
        if (generation === documentGeneration) failed(error);
        else
          message =
            "An operation for the previous book failed. The imported collection remains separate and unsaved; save a named copy.";
      }
      throw error;
    } finally {
      busy = false;
      if (!disposed) {
        emit();
        schedule();
      }
    }
  }
  function guard(stamp, generation, life, capturedName) {
    alive();
    if (
      generation !== documentGeneration ||
      life !== lifecycle ||
      capturedName !== name ||
      stamp !== host.checkpoint()
    ) {
      throw bookError(
        "STALE_SOURCE",
        "The current book changed during recovery or confirmation. Nothing was replaced.",
      );
    }
  }
  async function save({ copy = false } = {}) {
    return run(async () => {
      if (!copy && conflict)
        throw bookError(
          "LIBRARY_CONFLICT",
          "Refresh and reopen the saved book, or save a separate copy to preserve both versions.",
        );
      const generation = documentGeneration,
        project = host.captureProject(),
        stamp = host.checkpoint(),
        capturedName = name;
      const previous = copy ? null : binding;
      message =
        "Saving a captured source snapshot. Waiting for the storage transaction to complete.";
      emit();
      const receipt = await store.save({
        id: previous?.id ?? null,
        expectedRevision: previous?.revision ?? null,
        name: capturedName,
        project,
      });
      if (disposed) return receipt;
      entries = [receipt, ...entries.filter((entry) => entry.id !== receipt.id)];
      // Importing a different project while a save is pending must never attach
      // that new document to the old book's acknowledgment or autosave target.
      if (generation !== documentGeneration) {
        message =
          "The previous book was saved. The current imported collection is separate and has not been saved here.";
        return receipt;
      }
      binding = { id: receipt.id, revision: receipt.revision };
      conflict = false;
      dirty = stamp !== host.checkpoint() || capturedName !== name;
      if (capturedName === name) name = receipt.name;
      message = dirty
        ? `Saved revision ${receipt.revision}; newer edits are still unsaved.`
        : `Saved locally as “${receipt.name}”, revision ${receipt.revision}. Images are not stored. Keep a downloaded backup.`;
      return receipt;
    });
  }
  return Object.freeze({
    get state() {
      return state();
    },
    save,
    changed() {
      if (disposed) return;
      dirty = true;
      if (!busy && !conflict)
        message = autosave
          ? "Unsaved changes; autosave is scheduled."
          : "Unsaved changes. Save locally or download a source project.";
      emit();
      schedule();
    },
    rename(value) {
      alive();
      if (typeof value !== "string")
        throw bookError("INVALID_NAME", "The local book name must be text.");
      if (value === name) return;
      name = value;
      dirty = true;
      emit();
      schedule();
    },
    setAutosave(value) {
      alive();
      if (busy)
        throw bookError(
          "BOOK_BUSY",
          "Wait for the active library operation before changing autosave.",
        );
      if (typeof value !== "boolean")
        throw bookError("INVALID_OPTIONS", "Autosave requires an explicit boolean choice.");
      if (value && (!binding || conflict))
        throw bookError(
          "LIBRARY_CONFLICT",
          "Save a named copy or reopen a saved book before enabling autosave.",
        );
      autosave = value;
      message = value
        ? "Autosave enabled for this named book only. Changes are saved after transaction acknowledgment."
        : "Autosave is off. In-progress transactions may still complete; the last acknowledged revision is shown.";
      emit();
      schedule();
    },
    async refresh() {
      return run(async () => {
        const listed = await store.list();
        if (disposed) return listed;
        entries = listed;
        if (
          binding &&
          entries.find((entry) => entry.id === binding.id)?.revision !== binding.revision
        ) {
          conflict = true;
          autosave = false;
          message =
            "LIBRARY_CONFLICT: The saved book changed or was deleted elsewhere. Current source is untouched. Reopen it or save a separate copy.";
        } else
          message = `${entries.length} saved books available. Refresh never replaces the editor or advances its save revision.`;
        return listed;
      });
    },
    async open(id, revision = null) {
      return run(async () => {
        autosave = false;
        const stamp = host.checkpoint(),
          generation = documentGeneration,
          life = lifecycle,
          capturedName = name;
        const saved = await store.read(id, revision);
        guard(stamp, generation, life, capturedName);
        const approved = await confirm(
          `Open “${saved.name}”, revision ${saved.snapshotRevision}? This replaces the current editor, including unsaved edits, and revokes all image access. Download or save current work first.`,
        );
        guard(stamp, generation, life, capturedName);
        if (approved !== true) {
          message = "Recovery cancelled. Current source is unchanged; autosave remains off.";
          return false;
        }
        host.replaceProject(saved.project, stamp);
        documentGeneration++;
        binding = { id: saved.id, revision: saved.revision };
        name = saved.name;
        conflict = false;
        dirty = saved.snapshotRevision !== saved.revision;
        message = dirty
          ? `Opened historical revision ${saved.snapshotRevision}. Save creates a new revision; the current saved head was not overwritten. Autosave is off.`
          : `Opened saved revision ${saved.revision}. Reauthorize images. Autosave is off until enabled again.`;
        return true;
      });
    },
    async newBook() {
      return run(async () => {
        autosave = false;
        const stamp = host.checkpoint(),
          generation = documentGeneration,
          life = lifecycle,
          capturedName = name;
        const approved = await confirm(
          "Start an empty book? Save or download the current collection first. This replaces the editor and revokes images, but does not delete saved books.",
        );
        guard(stamp, generation, life, capturedName);
        if (approved !== true) {
          message = "New book cancelled. Current source is unchanged.";
          return false;
        }
        host.replaceProject({ schemaVersion: 1, files: [], options: {} }, stamp);
        documentGeneration++;
        binding = null;
        name = "My book";
        dirty = false;
        conflict = false;
        message =
          "New empty book. Existing library books are untouched. Save a named copy when ready.";
        return true;
      });
    },
    async remove(id, expectedRevision) {
      return run(async () => {
        const life = lifecycle;
        const entry = entries.find(
          (value) => value.id === id && value.revision === expectedRevision,
        );
        if (!entry)
          throw bookError("LIBRARY_CONFLICT", "Refresh the saved-book list before deleting.");
        const approved = await confirm(
          `Delete “${entry.name}” and all its retained local revisions? This cannot be undone. The current editor and downloaded files will not be deleted.`,
        );
        alive();
        if (life !== lifecycle)
          throw bookError(
            "STALE_SOURCE",
            "The page was suspended during deletion confirmation. Nothing was deleted.",
          );
        if (approved !== true) {
          message = "Deletion cancelled.";
          return false;
        }
        await store.remove(id, expectedRevision);
        if (disposed) return true;
        entries = entries.filter((value) => value.id !== id);
        if (binding?.id === id) {
          binding = null;
          autosave = false;
          conflict = false;
          dirty = true;
          documentGeneration++;
        }
        message =
          "Saved book and its retained revisions deleted. Current editor source is unchanged.";
        return true;
      });
    },
    detach() {
      if (disposed) return;
      documentGeneration++;
      binding = null;
      autosave = false;
      conflict = false;
      dirty = true;
      clearTimer();
      message =
        "Imported project is separate from the previously saved book. Save a named copy; autosave is off.";
      emit();
    },
    composition(value) {
      if (disposed) return;
      composing = value === true;
      if (composing) clearTimer();
      else schedule();
      emit();
    },
    suspend() {
      if (disposed) return;
      suspended = true;
      composing = false;
      lifecycle++;
      autosave = false;
      clearTimer();
      message =
        "Page suspended. Autosave is off; unacknowledged edits are not guaranteed saved. Reopen the library to inspect recovery.";
      emit();
    },
    resume() {
      if (!disposed) {
        suspended = false;
        message =
          "Returned to the editor. Autosave remains off; refresh recovery before continuing.";
        emit();
      }
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      lifecycle++;
      clearTimer();
      store.close();
    },
  });
}
