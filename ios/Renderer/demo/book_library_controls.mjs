import { createBookLibrarySession } from "./book_library_session.mjs";
import { createBookLibraryStore } from "./book_library_store.mjs";

/** Only explicit Save/Autosave choices persist source. Listing is read-only;
 * selecting a saved book is not permission to replace the current editor. */
export function createBookLibraryControls({
  root,
  controls,
  collection,
  window = globalThis.window,
  store = createBookLibraryStore(),
  confirm = (text) => window.confirm(text),
  debounceMs = 1000,
}) {
  const ids = [
    "library-books",
    "library-versions",
    "library-name",
    "library-save",
    "library-copy",
    "library-autosave",
    "library-open",
    "library-delete",
    "library-refresh",
    "library-new",
    "library-status",
    "library-current",
  ];
  const el = Object.fromEntries(ids.map((id) => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some((value) => !value))
    throw new Error("Missing local book library controls.");
  let disposed = false,
    warning = false,
    inventory = "",
    currentState,
    session;
  const listeners = [];
  const beforeUnload = (event) => {
    if (!disposed && session.state.dirty) {
      event.preventDefault();
      event.returnValue = "";
    }
  };
  function versions() {
    const entry = currentState.entries.find((value) => value.id === el["library-books"].value);
    const previous = el["library-versions"].value;
    const items = entry
      ? [
          { value: "", label: `Latest saved revision (${entry.revision})` },
          ...entry.versions
            .slice(1)
            .map((version) => ({
              value: String(version.revision),
              label: `Revision ${version.revision} — ${new Date(version.savedAt).toLocaleString()}`,
            })),
        ]
      : [];
    el["library-versions"].replaceChildren(
      ...items.map((item) => {
        const option = root.createElement("option");
        option.value = item.value;
        option.textContent = item.label;
        return option;
      }),
    );
    el["library-versions"].value = items.some((item) => item.value === previous) ? previous : "";
    const blocked = currentState.busy || currentState.suspended || currentState.composing;
    el["library-open"].disabled =
      el["library-delete"].disabled =
      el["library-versions"].disabled =
        blocked || !entry;
  }
  function render(state) {
    if (disposed) return;
    currentState = state;
    const selected = el["library-books"].value,
      signature = JSON.stringify(state.entries);
    if (inventory !== signature) {
      inventory = signature;
      el["library-books"].replaceChildren(
        ...state.entries.map((entry) => {
          const option = root.createElement("option");
          option.value = entry.id;
          option.textContent = `${entry.name} — revision ${entry.revision}`;
          return option;
        }),
      );
      el["library-books"].value = state.entries.some((entry) => entry.id === selected)
        ? selected
        : (state.entries[0]?.id ?? "");
    }
    if (el["library-name"].value !== state.name) el["library-name"].value = state.name;
    el["library-autosave"].checked = state.autosave;
    const blocked = state.busy || state.suspended || state.composing;
    for (const key of [
      "library-save",
      "library-copy",
      "library-new",
      "library-refresh",
      "library-books",
    ])
      el[key].disabled = blocked;
    el["library-autosave"].disabled = blocked || !state.binding || state.conflict;
    el["library-save"].disabled ||= state.conflict;
    el["library-current"].textContent = state.binding
      ? `Current save target: ${state.name}, observed revision ${state.binding.revision}. Selecting another book below does not change this target.`
      : "This editor is not attached to a saved book. Saving creates a new named copy.";
    el["library-status"].textContent = state.message;
    versions();
    // Do not permanently install beforeunload on a clean page. It is only an
    // additional warning: mobile/browser teardown may omit the event entirely.
    const nextWarning = state.dirty && !state.suspended;
    if (nextWarning !== warning) {
      warning = nextWarning;
      if (warning) window.addEventListener("beforeunload", beforeUnload);
      else window.removeEventListener("beforeunload", beforeUnload);
    }
  }
  function report(error) {
    if (!disposed)
      el["library-status"].textContent =
        `${error.code ?? "LIBRARY_ERROR"}: ${error.message ?? "Operation failed."} Source remains in the editor.`;
  }
  function invoke(work) {
    try {
      Promise.resolve(work()).catch(report);
    } catch (error) {
      report(error);
    }
  }
  function on(element, type, listener) {
    element.addEventListener(type, listener);
    listeners.push(() => element.removeEventListener(type, listener));
  }
  session = createBookLibrarySession({
    store,
    host: controls,
    confirm,
    onState: render,
    debounceMs,
  });
  const unsubscribe = collection.subscribe(() => session.changed());
  for (const key of [
    "chapter-source",
    "chapter-path",
    "title",
    "author",
    "lang",
    "font",
    "dark-mode",
    "font-scale",
    "toc",
    "page-numbers",
  ]) {
    const input = root.querySelector(`#${key}`);
    // Invalid typed settings may not reach the collection; they still count as
    // unsaved edits and must invalidate recovery confirmations and clean state.
    on(input, "input", () => session.changed());
    on(input, "change", () => session.changed());
  }
  on(root.querySelector("#chapter-source"), "compositionstart", () => session.composition(true));
  on(root.querySelector("#chapter-source"), "compositionend", () => session.composition(false));
  on(el["library-name"], "input", () => invoke(() => session.rename(el["library-name"].value)));
  on(el["library-autosave"], "change", () =>
    invoke(() => session.setAutosave(el["library-autosave"].checked)),
  );
  on(el["library-save"], "click", () => invoke(() => session.save()));
  on(el["library-copy"], "click", () => invoke(() => session.save({ copy: true })));
  on(el["library-refresh"], "click", () => invoke(() => session.refresh()));
  on(el["library-new"], "click", () => invoke(() => session.newBook()));
  on(el["library-books"], "change", () => {
    el["library-versions"].value = "";
    versions();
  });
  on(el["library-open"], "click", () =>
    invoke(() =>
      session.open(
        el["library-books"].value,
        el["library-versions"].value === "" ? null : Number(el["library-versions"].value),
      ),
    ),
  );
  on(el["library-delete"], "click", () =>
    invoke(() => {
      const selected = session.state.entries.find(
        (entry) => entry.id === el["library-books"].value,
      );
      return session.remove(selected?.id, selected?.revision);
    }),
  );
  render(session.state);
  if (collection.files.length) session.changed();
  const ready = session.refresh().catch(report);
  return Object.freeze({
    ready,
    session,
    detach() {
      session.detach();
    },
    suspend() {
      session.suspend();
    },
    resume() {
      session.resume();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      unsubscribe();
      for (const remove of listeners) remove();
      window.removeEventListener("beforeunload", beforeUnload);
      session.dispose();
    },
  });
}
