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

/** Retains ONE current source snapshot plus bounded replacement deltas, not a
 * whole-book copy per keystroke. bytes bounds UTF-16 delta payloads, excluding
 * the current snapshot, engine string representation and JS object overhead.
 * No assets or settings are accepted. Plans never change history until a host
 * has installed the exact result and calls commit; failed writes remain retryable.
 */
export function createBookSourceHistory(files, limits = BOOK_SOURCE_HISTORY_LIMITS) {
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
      if (!sameSet(current, next)) { clear(next); return "source-set-changed"; }
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
      if (group !== null && grouping === group && previous?.group === group
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
      const next = current.slice();
      for (const change of entry.changes) {
        const file = current[change.index];
        next[change.index] = Object.freeze({ ...file, source: splice(file.source, change, direction === "undo") });
      }
      const plan = Object.freeze({ direction, label: entry.label, files: Object.freeze(next),
        changed: Object.freeze(entry.changes.map(change => change.index)) });
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
