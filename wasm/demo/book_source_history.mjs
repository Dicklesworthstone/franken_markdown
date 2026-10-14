// Exact source history, not a Markdown parser, renderer, or project/resource store.
export const BOOK_SOURCE_HISTORY_LIMITS = Object.freeze({ entries: 100, bytes: 16 * 1024 * 1024 });
const failure = (code, message) => Object.assign(new Error(message), { code });
const own = (value, key) => {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  if (!descriptor || !Object.hasOwn(descriptor, "value"))
    throw failure("INVALID_HISTORY_SOURCE", "History requires owned source data, not accessors.");
  return descriptor.value;
};
function capture(files) {
  if (!Array.isArray(files)) throw failure("INVALID_HISTORY_SOURCE", "History needs a source array.");
  const count = own(files, "length");
  if (count > 256) throw failure("INVALID_HISTORY_SOURCE", "History supports at most 256 sources.");
  const result = [];
  for (let i = 0; i < count; i++) {
    const file = own(files, String(i));
    if (!file || typeof file !== "object") throw failure("INVALID_HISTORY_SOURCE", "Invalid history source.");
    const path = own(file, "path"), source = own(file, "source");
    const role = Object.hasOwn(file, "role") ? own(file, "role") : undefined;
    if (typeof path !== "string" || typeof source !== "string" || (role !== undefined && role !== "chapter" && role !== "include"))
      throw failure("INVALID_HISTORY_SOURCE", "History requires string path/source and a chapter or include role.");
    // Do not normalize newlines, Unicode or draft paths. Replay admission remains
    // the collection's job; history must not damage temporarily incomplete text.
    result.push(Object.freeze({ path, source, ...(role === "include" ? { role } : {}) }));
  }
  return Object.freeze(result);
}
const sameSet = (a, b) => a.length === b.length && a.every((file, i) => file.path === b[i].path && file.role === b[i].role);
const sameSource = (a, b) => sameSet(a, b) && a.every((file, i) => file.source === b[i].source);
function difference(before, after, index) {
  if (before === after) return null;
  let start = 0, end = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  while (end < before.length - start && end < after.length - start && before[before.length - 1 - end] === after[after.length - 1 - end]) end++;
  return Object.freeze({ index, start, removed: before.slice(start, before.length - end), inserted: after.slice(start, after.length - end) });
}
function splice(source, change, undo) {
  const expected = undo ? change.inserted : change.removed;
  const insert = undo ? change.removed : change.inserted;
  if (source.slice(change.start, change.start + expected.length) !== expected)
    throw failure("HISTORY_CONFLICT", "Source no longer matches this history entry.");
  return source.slice(0, change.start) + insert + source.slice(change.start + expected.length);
}
const entryBytes = changes => changes.reduce((sum, change) => sum + 2 * (change.removed.length + change.inserted.length), 0);

// Structural entries refer to unchanged text by position in the opposite tree.
// A reorder or rename retains path/role metadata, not another copy of every
// chapter. Only genuinely added/deleted/changed text contributes payload bytes.
// Exact source equality is sufficient: identical-text files need no unique IDs.
function treeSide(from, to) {
  const sourceIndex = new Map(from.map((file, index) => [file.source, index]));
  return Object.freeze(to.map(file => Object.freeze({
    path: file.path, ...(file.role === "include" ? { role: file.role } : {}),
    ...(sourceIndex.has(file.source) ? { from: sourceIndex.get(file.source) } : { source: file.source }),
  })));
}
const treeBytes = side => side.reduce((sum, file) =>
  sum + 16 + 2 * (file.path.length + (file.source?.length ?? 0)), 0);
function applyTree(current, side) {
  return Object.freeze(side.map(file => Object.freeze({
    path: file.path, ...(file.role === "include" ? { role: file.role } : {}),
    source: Object.hasOwn(file, "from") ? current[file.from].source : file.source,
  })));
}

/** Retains ONE current source snapshot plus bounded replacement deltas, not a
 * whole-book copy per keystroke. bytes bounds UTF-16 delta payloads, excluding
 * the current snapshot, engine string representation and JS object overhead.
 * Opt-in structural entries also charge path strings and 16 bytes per slot.
 * Hosts enabling trackStructure must replay complete trees atomically and reset
 * on project replacement. The default source-only API keeps its old boundaries.
 * No assets or settings are accepted. Plans never change history until a host
 * has installed the exact result and calls commit; failed writes remain retryable.
 */
export function createBookSourceHistory(files, limits = BOOK_SOURCE_HISTORY_LIMITS,
  { trackStructure = false } = {}) {
  if (typeof trackStructure !== "boolean")
    throw failure("INVALID_HISTORY_OPTIONS", "trackStructure must be a boolean.");
  const cap = { ...BOOK_SOURCE_HISTORY_LIMITS, ...limits };
  for (const key of Object.keys(BOOK_SOURCE_HISTORY_LIMITS)) {
    if (!Number.isSafeInteger(cap[key]) || cap[key] < 1 || cap[key] > BOOK_SOURCE_HISTORY_LIMITS[key])
      throw failure("INVALID_HISTORY_LIMIT", "History limits must be positive integers no larger than the defaults.");
  }
  let current = capture(files), past = [], future = [], bytes = 0, pending = null;
  let grouping = null, lost = 0, disposed = false;
  const alive = () => { if (disposed) throw failure("HISTORY_CLOSED", "Source history is disposed."); };
  function clear(next) {
    current = next; past = []; future = []; bytes = 0; grouping = null; pending = null;
  }
  function trim() {
    while (past.length > cap.entries || bytes > cap.bytes) {
      const entry = past.shift(); bytes -= entry.bytes; lost++;
    }
  }
  const api = {
    get state() {
      alive();
      return Object.freeze({ undo: past.length, redo: future.length, bytes, discarded: lost,
        undoLabel: past.at(-1)?.label ?? null, redoLabel: future.at(-1)?.label ?? null });
    },
    matches(value) { alive(); return sameSource(current, capture(value)); },
    reset(value) { alive(); clear(capture(value)); lost = 0; },
    breakGroup() { alive(); grouping = null; },
    record(value, { group = null, label = "Source edit" } = {}) {
      alive();
      if ((group !== null && typeof group !== "string") || (group !== null && group.length > 128)
          || typeof label !== "string" || label.length > 256)
        throw failure("INVALID_HISTORY_GROUP", "Use bounded history group and label strings.");
      const next = capture(value);
      if (!sameSet(current, next)) {
        if (!trackStructure) { clear(next); lost = 0; return "source-set-changed"; }
        const undo = treeSide(next, current), redo = treeSide(current, next);
        const entry = Object.freeze({ tree: Object.freeze({ undo, redo }),
          bytes: treeBytes(undo) + treeBytes(redo), group: null, label });
        pending = null; grouping = null;
        for (const old of future) bytes -= old.bytes;
        future = []; past.push(entry); bytes += entry.bytes; current = next;
        trim();
        return "structure-recorded";
      }
      const changes = [];
      for (let i = 0; i < next.length; i++) {
        const change = difference(current[i].source, next[i].source, i);
        if (change) changes.push(change);
      }
      if (!changes.length) return "unchanged";
      pending = null;
      for (const entry of future) bytes -= entry.bytes;
      future = [];
      const previous = past.at(-1);
      // A group can merge one edited source only. A multi-file replacement is
      // always one isolated transaction, even when a caller reuses a group key.
      if (group !== null && grouping === group && previous?.group === group && !previous.tree
          && changes.length === 1 && previous.changes.length === 1
          && changes[0].index === previous.changes[0].index) {
        const index = changes[0].index;
        const original = splice(current[index].source, previous.changes[0], true);
        const combined = difference(original, next[index].source, index);
        past.pop(); bytes -= previous.bytes;
        changes.splice(0, changes.length, ...(combined ? [combined] : []));
      }
      if (changes.length) {
        const entry = Object.freeze({ changes: Object.freeze(changes), bytes: entryBytes(changes), group, label });
        past.push(entry); bytes += entry.bytes;
      }
      grouping = group;
      current = next;
      trim();
      return "recorded";
    },
    plan(direction) {
      alive();
      if (direction !== "undo" && direction !== "redo") throw failure("INVALID_HISTORY_DIRECTION", "Choose undo or redo.");
      const entry = (direction === "undo" ? past : future).at(-1);
      if (!entry) throw failure("NO_HISTORY", "No source edit is available in this direction.");
      const next = entry.tree ? applyTree(current, entry.tree[direction]) : current.slice();
      if (!entry.tree) for (const change of entry.changes) {
        const file = current[change.index];
        next[change.index] = Object.freeze({ ...file, source: splice(file.source, change, direction === "undo") });
      }
      const changed = entry.tree ? next.flatMap((file, index) =>
        !current[index] || !sameSource([current[index]], [file]) ? [index] : [])
        : entry.changes.map(change => change.index);
      const plan = Object.freeze({ direction, label: entry.label, files: Object.freeze(next),
        ...(entry.tree ? { structure: true } : {}), changed: Object.freeze(changed) });
      pending = { plan, entry };
      return plan;
    },
    commit(plan, installed) {
      alive();
      if (!pending || pending.plan !== plan || !sameSource(plan.files, capture(installed)))
        throw failure("HISTORY_CONFLICT", "The planned source transaction was not installed exactly; history was not advanced.");
      const { entry } = pending;
      const from = plan.direction === "undo" ? past : future, to = plan.direction === "undo" ? future : past;
      if (from.at(-1) !== entry) throw failure("HISTORY_CONFLICT", "Source history changed before commit.");
      from.pop(); to.push(entry); current = plan.files; pending = null; grouping = null;
    },
    dispose() {
      if (disposed) return;
      clear(Object.freeze([])); disposed = true;
    },
  };
  return Object.freeze(api);
}

/** Wire chronological source undo to the existing atomic applySources host.
 * This controller has no worker, asset store, project serializer or persistence.
 * The host must capture normal input before notifying collection subscribers.
 */
export function createBookSourceHistoryControls({ root, controls, collection,
  limits = BOOK_SOURCE_HISTORY_LIMITS, now = () => performance.now() }) {
  const editor = root.querySelector("#chapter-source"), path = root.querySelector("#chapter-path");
  const role = root.querySelector("#source-role");
  if (!editor?.parentElement || !path || !role || typeof controls?.applySources !== "function"
      || typeof controls?.checkpoint !== "function" || typeof collection?.subscribe !== "function")
    throw new TypeError("Book source history requires the source editor and its transaction host.");
  if (root.querySelector("#book-source-history")) throw new Error("Book source history is already installed.");
  const history = createBookSourceHistory(collection.files, limits);
  const panel = root.createElement("div"); panel.id = "book-source-history";
  const make = (tag, id, text) => {
    const node = root.createElement(tag); node.id = `book-history-${id}`; node.textContent = text;
    panel.appendChild(node); return node;
  };
  make("h3", "title", "Book-wide source undo");
  panel.setAttribute("aria-labelledby", "book-history-title");
  const undo = make("button", "undo", "Undo source edit"), redo = make("button", "redo", "Redo source edit");
  const clear = make("button", "clear", "Clear source history");
  for (const node of [undo, redo, clear]) { node.type = "button"; node.setAttribute("aria-describedby", "book-history-help book-history-status"); }
  undo.setAttribute("aria-keyshortcuts", "Control+z Meta+z");
  redo.setAttribute("aria-keyshortcuts", "Control+Shift+z Meta+Shift+z Control+y");
  make("p", "help", "Undo typing and source transactions across chapters and include files. In the source editor use Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z, or Ctrl+Y. Settings and image/font grants are never restored. Source additions, removals, renames, role/order changes and different-source project recovery reset history. History is session-only, not a saved backup; page suspension clears it.");
  const status = make("p", "status", ""); status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  editor.parentElement.insertBefore(panel, editor.nextSibling);
  let disposed = false, suspended = false, composing = false, installing = false;
  let config = collection.renderConfigurationRevision, serial = 0, context = null;
  let before = null, burst = null, compositionGroup = null, compositionTail = null;
  let message = "Source history starts at the current book.";
  const unlisten = [];
  const blocked = () => disposed || suspended || composing || installing || controls.sourceBusy;
  function check() {
    if (disposed || suspended) throw failure("HISTORY_CLOSED", "Source history is not active.");
    if (composing || installing || controls.sourceBusy) throw failure("BOOK_BUSY", "Finish importing or composing text before undo/redo.");
  }
  function breakGroup() {
    burst = null; before = null; context = null; compositionTail = null;
    if (!disposed) history.breakGroup();
  }
  function refresh() {
    if (disposed) return;
    const state = history.state, disabled = blocked();
    undo.disabled = disabled || !state.undo; redo.disabled = disabled || !state.redo;
    clear.disabled = disabled || (!state.undo && !state.redo);
    status.textContent = `${message} ${state.undo} undo / ${state.redo} redo; ${state.bytes.toLocaleString()} retained delta bytes.`
      + (state.discarded ? ` ${state.discarded} older entries exceeded the history budget.` : "");
  }
  function reset(text) {
    history.reset(collection.files); config = collection.renderConfigurationRevision;
    breakGroup(); message = text; refresh();
  }
  function observe() {
    if (disposed || suspended || installing) return;
    const files = collection.files, nextConfig = collection.renderConfigurationRevision;
    if (nextConfig !== config) {
      config = nextConfig;
      if (!history.matches(files)) {
        reset("Source structure or project changed; history starts from this source set."); return;
      }
      breakGroup(); // Presentation/resource changes do not erase source recovery.
    } else {
      const previous = history.state.discarded;
      const result = history.record(files, context ?? { label: "Book source transaction" });
      if (result === "source-set-changed") message = "Source membership changed; history was reset.";
      else if (result === "recorded") message = history.state.discarded !== previous
        ? "Source edit recorded within the history budget." : "Source edit recorded.";
    }
    refresh();
  }
  function aligned() {
    // Never replace undispatched keystrokes. Unlike captureProject this does
    // not require current drafts/settings to validate before undo can repair
    // them, and never clones resource payloads or serializes a whole project.
    const index = controls.currentChapter, file = collection.files[index];
    if (file && (editor.value !== file.source.replace(/\r\n?/g, "\n") || path.value !== file.path
        || role.value !== (file.role ?? "chapter")))
      throw failure("STALE_SOURCE", "The source editor has uncaptured changes. Finish the input before undo/redo; no text was overwritten.");
  }
  function step(direction) {
    check(); aligned(); observe(); breakGroup();
    const checkpoint = controls.checkpoint(), old = collection.files, plan = history.plan(direction);
    check();
    let installed;
    installing = true;
    try {
      installed = controls.applySources(plan.files, checkpoint);
      if (disposed) throw failure("HISTORY_CLOSED", "Source history was disposed during the transaction.");
      if (suspended || installed !== collection.revision)
        throw failure("HISTORY_CONFLICT", "Another change followed the source transaction; review current source before continuing.");
      history.commit(plan, collection.files);
      config = collection.renderConfigurationRevision;
    } catch (error) {
      if (!disposed && !suspended && !history.matches(collection.files))
        reset("Source changed during undo/redo; history was reset without overwriting the newer state.");
      throw error;
    } finally { installing = false; if (!disposed) refresh(); }
    // The host's original-source navigation handles CRLF and updates the actual
    // editor, not merely its model. A navigation failure cannot undo a successful
    // transaction, so report that separately without replaying the source again.
    const index = plan.changed.includes(controls.currentChapter) ? controls.currentChapter : plan.changed[0];
    const target = plan.files[index].source;
    let offset = difference(old[index].source, target, index)?.start ?? 0;
    if (offset > 0 && /[\udc00-\udfff]/.test(target[offset] ?? "") && /[\ud800-\udbff]/.test(target[offset - 1])) offset--;
    message = `${direction === "undo" ? "Undid" : "Redid"} ${plan.label} in ${plan.changed.length} source file(s). Settings and resources were preserved.`;
    try { controls.selectSourceRange?.(index, offset, offset, controls.checkpoint()); }
    catch { message += " Source was changed, but automatic cursor navigation was unavailable."; }
    refresh(); return installed;
  }
  function invoke(work) {
    try { return work(); }
    catch (error) { if (!disposed) { message = `${error.code ?? "HISTORY_ERROR"}: ${error.message}`; refresh(); } }
  }
  function on(node, type, listener, options) {
    if (!node) return;
    node.addEventListener(type, listener, options);
    unlisten.push(() => node.removeEventListener(type, listener, options));
  }
  const historyDirection = type => type === "historyUndo" ? "undo" : type === "historyRedo" ? "redo" : null;
  on(editor, "beforeinput", event => {
    const direction = historyDirection(event.inputType);
    if (direction && event.cancelable && !composing && !event.isComposing) {
      event.preventDefault(); invoke(() => step(direction)); return;
    }
    before = { start: editor.selectionStart, end: editor.selectionEnd, type: event.inputType };
  }, true);
  on(editor, "input", event => {
    if (disposed || suspended) return;
    const time = now(), file = collection.files[controls.currentChapter];
    const type = event.inputType ?? before?.type ?? "";
    const isTyping = ["insertText", "deleteContentBackward", "deleteContentForward"].includes(type);
    let group = null;
    if (composing || event.isComposing) group = compositionGroup ??= `composition-${++serial}`;
    else if (compositionTail && ["insertFromComposition", "insertCompositionText"].includes(type)) group = compositionTail;
    else if (isTyping && before && Number.isFinite(time)) {
      const adjacent = burst && burst.path === file?.path && burst.type === type
        && time >= burst.time && time - burst.time <= 1000
        && before.start === before.end && before.start === burst.end;
      group = adjacent ? burst.group : `typing-${++serial}`;
    }
    context = { group, label: composing || event.isComposing ? "Text composition" : "Source edit" };
    burst = isTyping ? { group, path: file?.path, time, type, end: editor.selectionEnd } : null;
    before = null; compositionTail = null;
  }, true); // Reserve the group BEFORE the existing input-capture listener.
  on(editor, "input", () => { observe(); context = null; });
  on(editor, "compositionstart", () => {
    breakGroup(); composing = true; compositionGroup = `composition-${++serial}`; refresh();
  }, true);
  on(editor, "compositionend", () => {
    composing = false; compositionTail = compositionGroup; compositionGroup = null;
    burst = null; refresh();
  });
  on(editor, "keydown", event => {
    if (event.defaultPrevented || event.isComposing || composing) return;
    const key = event.key?.toLowerCase();
    const command = (event.ctrlKey || event.metaKey) && !event.altKey;
    const direction = command && key === "z" ? (event.shiftKey ? "redo" : "undo")
      : event.ctrlKey && !event.metaKey && !event.altKey && key === "y" ? "redo" : null;
    if (direction) { event.preventDefault(); invoke(() => step(direction)); }
    else if (["arrowleft", "arrowright", "arrowup", "arrowdown", "home", "end", "pageup", "pagedown"].includes(key)) breakGroup();
  });
  for (const type of ["pointerdown", "blur"]) on(editor, type, breakGroup);
  on(root.querySelector("#chapters"), "change", breakGroup);
  on(undo, "click", () => invoke(() => step("undo")));
  on(redo, "click", () => invoke(() => step("redo")));
  on(clear, "click", () => invoke(() => { check(); reset("Source history cleared. The book is unchanged."); }));
  const unsubscribe = collection.subscribe(observe), unsubscribeBusy = controls.subscribeSourceState?.(refresh);
  refresh();
  return Object.freeze({
    undo: () => step("undo"), redo: () => step("redo"),
    get state() { return history.state; },
    clear() { check(); reset("Source history starts at the current book."); },
    // A successful project import may notify while its source host is busy.
    reset() { if (!disposed && !suspended) reset("Source history starts at the imported book."); },
    suspend() {
      if (disposed) return;
      suspended = true; composing = false; compositionGroup = null;
      breakGroup(); history.reset([]); message = "Source history cleared on suspension; current book source is retained."; refresh();
    },
    resume() { if (!disposed) { suspended = false; reset("Source history starts at the resumed book."); } },
    dispose() {
      if (disposed) return;
      disposed = true; unsubscribe(); unsubscribeBusy?.();
      for (const remove of unlisten) remove();
      history.dispose(); context = before = burst = compositionGroup = compositionTail = null;
      undo.disabled = redo.disabled = clear.disabled = true;
    },
  });
}
