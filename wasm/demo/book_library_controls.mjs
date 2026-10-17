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
  let projectEpoch = collection.sourceProjectRevision;
  const composing = new Set();
  const sourceHost = typeof controls.applySourceTree === "function" && Number.isSafeInteger(projectEpoch)
    ? librarySourceHost(root, controls, collection) : null;
  const mergePanel = sourceHost && el["library-status"].parentElement
    ? installLibraryMergePanel(root, el["library-status"].parentElement,
      () => sourceHost.isBlocked(), value => session?.composition(value || composing.size > 0)) : null;
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
    mergePanel?.refresh(state);
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
    if (!element) return;
    element.addEventListener(type, listener);
    listeners.push(() => element.removeEventListener(type, listener));
  }
  session = createBookLibrarySession({
    store,
    host: controls,
    sources: sourceHost,
    confirm,
    onState: render,
    debounceMs,
  });
  const unsubscribe = collection.subscribe(() => {
    mergePanel?.cancel();
    const epoch = collection.sourceProjectRevision;
    if (sourceHost && epoch !== projectEpoch) { projectEpoch = epoch; session.detach(); }
    else session.changed();
  });
  const unsubscribeBusy = controls.subscribeSourceState?.(() => {
    if (controls.sourceBusy) mergePanel?.cancel();
    if (!disposed) render(session.state);
  });
  for (const key of [
    "chapter-source",
    "chapter-path",
    "source-role",
    "chapters",
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
    on(input, "input", () => { mergePanel?.cancel(); session.changed(); });
    on(input, "change", () => { mergePanel?.cancel(); session.changed(); });
    on(input, "compositionstart", () => { composing.add(input); mergePanel?.cancel(); session.composition(true); });
    on(input, "compositionend", () => { composing.delete(input); session.composition(composing.size > 0); });
  }
  on(el["library-name"], "input", () => {
    // Cancelling a review emits state and can repaint this field. Capture the
    // user's new name first so that repaint cannot consume their keystroke.
    const value = el["library-name"].value;
    mergePanel?.cancel(); invoke(() => session.rename(value));
  });
  if (mergePanel) on(mergePanel.button, "click", () => invoke(() => session.reconcile(mergePanel.review)));
  for (const type of ["input", "change"]) on(root, type, event => {
    if (!mergePanel?.contains(event.target)) mergePanel?.cancel();
  });
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
      mergePanel?.cancel();
      session.detach();
    },
    suspend() {
      mergePanel?.cancel(); composing.clear();
      session.suspend();
    },
    resume() {
      session.resume();
    },
    dispose() {
      if (disposed) return;
      mergePanel?.dispose(); composing.clear();
      disposed = true;
      unsubscribe();
      unsubscribeBusy?.();
      for (const remove of listeners) remove();
      window.removeEventListener("beforeunload", beforeUnload);
      session.dispose();
    },
  });
}

const mergeError = (code, message) => Object.assign(new Error(message), { code });
const sourceView = text => text.replace(/\r\n?/g, "\n");
// Read source drafts without captureProject(), which also applies/validates
// publishing settings. A source merge must not commit unrelated control drafts.
function librarySourceHost(root, controls, collection) {
  const editor = root.querySelector("#chapter-source"), path = root.querySelector("#chapter-path");
  const role = root.querySelector("#source-role");
  const read = () => {
    const checkpoint = controls.checkpoint(), identity = collection.sourceProjectRevision;
    const files = collection.files, selected = controls.currentChapter;
    if (!Array.isArray(files) || files.length > 256)
      throw mergeError("BOOK_LIMIT", "Source merge requires a bounded source tree.");
    const next = files.map(file => ({ path: file.path, source: file.source,
      ...(file.role === "include" ? { role: "include" } : {}) }));
    if (next.length) {
      if (!Number.isInteger(selected) || !next[selected] || typeof editor?.value !== "string" || typeof path?.value !== "string")
        throw mergeError("INVALID_SOURCE_TREE_MERGE", "Choose a readable source before reviewing saved changes.");
      const current = next[selected], raw = editor.value;
      if (raw !== current.source && raw !== sourceView(current.source)) current.source = raw;
      current.path = path.value;
      if (role) {
        if (!["chapter", "include"].includes(role.value)) throw mergeError("INVALID_ROLE", "Choose a source role before merging.");
        if (role.value === "include") current.role = "include";
        else delete current.role;
      }
    }
    if (checkpoint !== controls.checkpoint() || identity !== collection.sourceProjectRevision)
      throw mergeError("STALE_SOURCE", "Source changed during merge capture.");
    return { files: next, identity, stamp: JSON.stringify([identity, checkpoint]) };
  };
  return {
    read,
    isBlocked: () => controls.sourceBusy === true,
    apply(files, stamp) {
      if (controls.sourceBusy || read().stamp !== stamp)
        throw mergeError("STALE_SOURCE", "Source changed before merge application.");
      const applied = controls.applySourceTree(files, controls.checkpoint());
      if (applied && typeof applied.then === "function") {
        Promise.resolve(applied).catch(() => {});
        throw mergeError("LIBRARY_RECONCILE_UNCERTAIN", "The source-tree host must finish synchronously.");
      }
      return read().stamp;
    },
  };
}

function installLibraryMergePanel(root, parent, isBlocked, composition) {
  if (root.querySelector("#library-merge")) throw new Error("Duplicate library source merge panel.");
  const panel = root.createElement("div"); panel.id = "library-merge";
  const make = (tag, id, text, into = panel) => {
    const element = root.createElement(tag);
    if (id) element.id = id;
    if (text !== undefined) element.textContent = text;
    into.appendChild(element); return element;
  };
  make("h3", "library-merge-title", "Reconcile source changes from the local library");
  panel.setAttribute("aria-labelledby", "library-merge-title");
  make("p", "library-merge-help", "Merge the current save target, not the selected library row. Review chapter/include text, additions, removals, roles and reading order against the last observed saved revision. Current publishing settings, unapplied drafts, local name, images and fonts stay current: incoming settings are NOT imported. Nothing is saved by this action. A later Save creates a full project revision using the current settings. Autosave remains off until explicitly enabled again.");
  const button = make("button", "library-reconcile", "Review saved source changes"); button.type = "button";
  button.setAttribute("aria-describedby", "library-merge-help library-status");
  const summary = make("pre", "library-merge-review", "No source merge is under review.");
  summary.setAttribute("aria-label", "Complete source actions and proposed reading order");
  const fields = make("div", "library-merge-conflicts");
  const approve = make("button", "library-merge-approve", "Apply reviewed source merge"); approve.type = "button";
  approve.setAttribute("aria-describedby", "library-merge-help library-merge-review library-merge-limits");
  const cancel = make("button", "library-merge-cancel", "Cancel source merge"); cancel.type = "button";
  make("p", "library-merge-limits", "Whole-source choices discard the competing text in that source. Regional choices retain independent changes from both sides. A membership choice can remove a source from the editor, never a disk or saved file. An order choice uses that side first, then appends other-side-only sources. Renames are delete/add identities; links are not rewritten. Excerpts below are at most 1,024 Unicode scalars, not complete diffs. Complete sources are retained for resolution. Limits: 128 chapters, 128 includes, 4 MiB each and 16 MiB combined source/path bytes; at most 256 conflict regions per review. Invalid or over-budget results cannot partially apply.");
  parent.appendChild(panel);
  let pending = null, state = null, disposed = false;
  const listeners = [], composing = new Set();
  const contains = target => { for (let node = target; node; node = node.parentElement) if (node === panel) return true; return false; };
  function on(node, type, fn) { node.addEventListener(type, fn); listeners.push(() => node.removeEventListener(type, fn)); }
  function excerpt(value) {
    if (value === null) return "[Absent]";
    if (Array.isArray(value)) return value.join("\n");
    if (typeof value === "object") return `[${value.role ?? "chapter"}] ${value.path}\n${excerpt(value.source)}`;
    let text = "", count = 0;
    for (const scalar of value) { if (count++ === 1024) return text + "\n[Excerpt truncated]"; text += scalar; }
    return text;
  }
  function select(id, choices, into) {
    const node = make("select", id, undefined, into);
    for (const [value, text] of [["", "Choose explicitly…"], ...choices]) make("option", null, text, node).value = value;
    node.value = ""; return node;
  }
  function refresh(next = state) {
    if (disposed) return;
    state = next;
    button.disabled = !state?.canReconcile || isBlocked();
    approve.hidden = cancel.hidden = !pending;
    cancel.disabled = !pending;
    approve.disabled = !pending || !!state?.suspended || !!state?.composing || isBlocked() || composing.size > 0;
    if (pending) for (const row of pending.rows) {
      const choice = row.choice.value;
      if (!row.allowed.has(choice) || (choice === "regions" && (!row.regions || row.regions.some(node => !["local", "disk"].includes(node.value))))) approve.disabled = true;
      if (row.manual) row.manual.hidden = choice !== "edited";
      if (row.regionPanel) row.regionPanel.hidden = choice !== "regions";
    }
  }
  function finish(value, focus = false) {
    if (!pending) return;
    const old = pending; pending = null;
    fields.replaceChildren(); composing.clear(); composition(false);
    summary.textContent = "No source merge is under review.";
    old.resolve(value); refresh();
    if (focus && old.focus?.isConnected !== false) old.focus?.focus?.();
  }
  function review(value) {
    if (disposed || pending || isBlocked() || state?.suspended || state?.composing) return Promise.resolve(false);
    const rows = [];
    summary.textContent = [
      `Current save target: ${value.name}; common revision ${value.baseRevision}; incoming saved revision ${value.revision} (${value.savedName}).`,
      ...value.changes.map(file => `${file.action}: ${file.path}${file.proposed ? ` [${file.proposed.role ?? "chapter"}]` : ""}`),
      "Proposed complete source order (before membership decisions):",
      ...(value.order.merged ?? ["Explicit order choice required."]),
      `${value.conflicts.length} conflict decision(s). Nothing is applied or saved until approval.`,
    ].join("\n");
    for (const entry of value.conflicts) {
      const group = make("fieldset", null, undefined, fields);
      const legend = `Conflict ${entry.index + 1}: ${entry.kind}${entry.path ? ` — ${entry.path}` : ""}`;
      make("legend", null, legend, group);
      for (const [key, label] of [["base", "Common ancestor"], ["local", "Current editor"], ["incoming", "Incoming saved version"]])
        make("pre", null, `${label} (source excerpts):\n${excerpt(entry[key])}`, group);
      const id = `library-merge-choice-${entry.index}`;
      make("label", null, "Resolution", group).setAttribute("for", id);
      const labels = side => entry.kind === "membership" ? `${side === "local" ? "Editor" : "Saved"}: ${entry[side] ? "keep this source" : "remove this source from the editor"}`
        : entry.kind === "order" ? `${side === "local" ? "Editor" : "Saved"} order first; append other-side-only sources`
          : `Use ${side === "local" ? "editor" : "saved"} ${entry.kind === "source" ? "complete source" : "role"}`;
      const choices = [["local", labels("local")], ["incoming", labels("incoming")]];
      if (entry.kind === "source") choices.push(["edited", "Write a complete replacement source"]);
      if (entry.hunks) choices.push(["regions", "Choose each conflicting region; keep independent edits"]);
      rows.push({ entry, group, choice: select(id, choices, group), allowed: new Set(choices.map(item => item[0])),
        manual: null, typed: false, regionPanel: null, regions: null });
    }
    return new Promise(resolve => {
      pending = { resolve, rows, focus: root.activeElement };
      refresh(); (rows[0]?.choice ?? approve).focus?.();
    });
  }
  on(panel, "change", () => {
    if (!pending) return;
    for (const row of pending.rows) {
      if (row.choice.value === "edited" && !row.manual) {
        const id = `library-merge-source-${row.entry.index}`;
        make("label", null, "Complete replacement source (empty means empty source, not removal)", row.group).setAttribute("for", id);
        row.manual = make("textarea", id, undefined, row.group);
        row.manual.value = row.entry.local; row.manualView = row.manual.value;
        row.manual.setAttribute("aria-describedby", "library-merge-limits");
      }
      if (row.choice.value === "regions" && !row.regions) {
        row.regionPanel = make("div", null, undefined, row.group);
        row.regions = row.entry.hunks.map(hunk => {
          const group = make("fieldset", null, undefined, row.regionPanel);
          make("legend", null, `Region ${hunk.index + 1}: base line offsets [${hunk.startLine}, ${hunk.endLine}) (zero-based)`, group);
          for (const key of ["base", "local", "disk"]) make("pre", null, `${key === "disk" ? "saved" : key} (excerpt):\n${excerpt(hunk[key])}`, group);
          const id = `library-merge-region-${row.entry.index}-${hunk.index}`;
          make("label", null, "Region resolution", group).setAttribute("for", id);
          return select(id, [["local", "Editor region"], ["disk", "Saved region"]], group);
        });
      }
    }
    refresh();
  });
  on(panel, "input", event => {
    for (const row of pending?.rows ?? []) if (row.manual === event.target) row.typed = true;
    refresh();
  });
  on(panel, "compositionstart", event => {
    if (pending?.rows.some(row => row.manual === event.target)) { composing.add(event.target); composition(true); refresh(); }
  });
  on(panel, "compositionend", event => { composing.delete(event.target); composition(composing.size > 0); refresh(); });
  on(approve, "click", () => {
    refresh(); if (approve.disabled || !pending) return;
    const choices = pending.rows.map(row => {
      const choice = row.choice.value, value = { index: row.entry.index, choice };
      if (choice === "edited") value.source = !row.typed && row.manual.value === row.manualView ? row.entry.local : row.manual.value;
      if (choice === "regions") value.regions = row.regions.map((node, index) => ({ index, choice: node.value }));
      return value;
    });
    finish(choices, true);
  });
  on(cancel, "click", () => finish(false, true));
  on(root, "keydown", event => {
    if (pending && event.key === "Escape" && !event.defaultPrevented) { event.preventDefault(); finish(false, true); }
  });
  refresh();
  return Object.freeze({ button, review, refresh, contains, cancel: () => finish(false),
    dispose() { if (!disposed) { finish(false); disposed = true; for (const remove of listeners) remove();
      for (const node of [button, approve, cancel]) node.disabled = true; } },
  });
}
