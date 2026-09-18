// Original-source I/O only. Never render, interpret HTML, fetch a URL, or infer
// image authorization from a file's path. Shares the flow facade's UTF-8 limit.
import { FlowError, FLOW_SOURCE_LIMIT, sourceText } from "../flow_session.mjs";
const fail = (code, message, cause) => { throw new FlowError(code, message, { cause }); };

export function documentName(value) {
  if (typeof value !== "string" || !value.trim() || value.length > 240
      || /[\x00-\x1f\x7f<>:"/\\|?*]/.test(value) || /[. ]$/.test(value)
      || value === "." || value === "..") {
    fail("INVALID_FILENAME", "Use a nonempty filename without path separators or control characters.");
  }
  sourceText(value, "filename");
  if (new TextEncoder().encode(value).length > 240) fail("INVALID_FILENAME", "Filename exceeds 240 UTF-8 bytes.");
  return value;
}

export function documentSnapshot(value) {
  if (!value || typeof value !== "object") fail("INVALID_ARGUMENT", "A source document is required.");
  // Return only these fields: persistence must never retain images, handles,
  // authorization grants, worker tokens, rendered HTML or export buffers.
  return Object.freeze({ filename: documentName(value.filename), source: sourceText(value.source) });
}

export async function readMarkdownFile(file) {
  if (!file || Object.prototype.toString.call(file) !== "[object File]"
      || typeof file.arrayBuffer !== "function" || !Number.isSafeInteger(file.size) || file.size < 0) {
    fail("INVALID_FILE", "Choose a local UTF-8 Markdown file.");
  }
  const filename = documentName(file.name), size = file.size;
  if (size > FLOW_SOURCE_LIMIT) fail("BUDGET_EXCEEDED", "Markdown file exceeds the 4 MiB source limit.");
  let bytes;
  try { bytes = await file.arrayBuffer(); }
  catch (error) { fail("FILE_READ_FAILED", "The local file could not be read; the editor is unchanged.", error); }
  if (Object.prototype.toString.call(bytes) !== "[object ArrayBuffer]" || bytes.byteLength !== size) {
    fail("INVALID_FILE", "The file read did not match its admitted byte length.");
  }
  let source;
  try {
    // Preserve a UTF-8 BOM and all CR/LF bytes. Never silently replace malformed
    // encoding, normalize Unicode, or turn CRLF into the platform's line ending.
    source = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch (error) { fail("INVALID_UNICODE", "The file is not valid UTF-8 Markdown.", error); }
  return documentSnapshot({ filename, source });
}

export function markdownDownload(document) {
  const value = documentSnapshot(document);
  // A Markdown source containing HTML must not accidentally download as .html.
  const filename = /\.(md|markdown)$/i.test(value.filename) ? value.filename : `${value.filename}.md`;
  return { filename, blob: new Blob([value.source], { type: "text/markdown; charset=utf-8", endings: "transparent" }) };
}

/** The textarea is authoritative, even if the renderer has failed. The host's
 * onReplace revokes the previous document's image grants before input is emitted.
 * Confirmation may be async; both file-read and confirmation races are fenced.
 */
export function createSourceControls({ sourceEditor, filename, open, prepare, download, status,
  onReplace, confirmReplace = () => true, urls = URL, initialDocument }) {
  let disposed = false, busy = false, epoch = 0, url = null, published = null;
  // textarea.value normalizes CR/CRLF to LF. Preserve the imported bytes while
  // its displayed value is unchanged; edited text follows the textarea's LF
  // convention. Returning to the exact imported view also restores its bytes.
  let originalSource = sourceEditor.value, originalView = sourceEditor.value;
  if (initialDocument !== undefined) {
    const initial = documentSnapshot(initialDocument);
    if (initial.filename !== filename.value || initial.source.replace(/\r\n?/g, "\n") !== originalView) {
      fail("STALE_SOURCE", "The preserved source no longer matches the editor.");
    }
    originalSource = initial.source;
  }
  const current = () => ({ source: sourceEditor.value, filename: filename.value });
  const snapshot = () => documentSnapshot({ filename: filename.value,
    source: sourceEditor.value === originalView ? originalSource : sourceEditor.value });
  const matches = value => value.source === sourceEditor.value && value.filename === filename.value;
  function revoke() {
    published = null; download.hidden = true;
    download.removeAttribute("href"); download.removeAttribute("download");
    if (url !== null) { const previous = url; url = null; urls.revokeObjectURL(previous); }
  }
  function changed() { epoch++; revoke(); }
  function alive() { if (disposed) fail("SESSION_DISPOSED", "Source controls are disposed."); }
  function replace(value) {
    alive(); const next = documentSnapshot(value), previous = current();
    const previousSource = originalSource, previousView = originalView;
    sourceEditor.value = next.source; filename.value = next.filename;
    originalSource = next.source; originalView = sourceEditor.value;
    try { onReplace(next); }
    catch (error) {
      sourceEditor.value = previous.source; filename.value = previous.filename;
      originalSource = previousSource; originalView = previousView; throw error;
    }
    changed();
    sourceEditor.dispatchEvent(new Event("input", { bubbles: true }));
  }
  function message(error) {
    if (!disposed) status.textContent = `${error?.code ?? "DOCUMENT_ERROR"}: ${error instanceof Error ? error.message : "Document operation failed."}`;
  }
  async function openFile(file) {
    alive();
    if (busy) fail("DOCUMENT_BUSY", "The previous file read has not settled.");
    const before = current(), ticket = epoch;
    busy = true; open.disabled = true;
    const fence = () => {
      alive();
      if (ticket !== epoch || !matches(before)) fail("STALE_SOURCE", "The editor changed during file loading; the file was not opened.");
    };
    try {
      const next = await readMarkdownFile(file);
      fence();
      const approved = await confirmReplace(next);
      fence();
      if (approved !== true) return false;
      replace(next);
      status.textContent = `Opened ${next.filename}. Previous image access was revoked; authorize images for this document separately.`;
      return true;
    } finally { busy = false; if (!disposed) open.disabled = false; }
  }
  function prepareDownload() {
    alive(); const value = snapshot(), output = markdownDownload(value);
    revoke();
    url = urls.createObjectURL(output.blob); published = current();
    download.href = url; download.download = output.filename;
    download.textContent = `Download ${output.filename}`; download.hidden = false;
    status.textContent = "Original Markdown prepared. Click Download to save it; no file has been overwritten.";
  }
  const onOpen = () => {
    const file = open.files?.[0];
    if (!file) return;
    void openFile(file).catch(message).finally(() => { if (!disposed) open.value = ""; });
  };
  const onPrepare = () => { try { prepareDownload(); } catch (error) { revoke(); message(error); } };
  const onDownload = event => {
    if (disposed || !published || !matches(published)) {
      event.preventDefault(); changed();
      if (!disposed) status.textContent = "The prepared source is stale. Prepare Markdown again.";
    }
  };
  sourceEditor.addEventListener("input", changed); filename.addEventListener("input", changed);
  open.addEventListener("change", onOpen); prepare.addEventListener("click", onPrepare); download.addEventListener("click", onDownload);
  revoke(); open.disabled = false; prepare.disabled = false;
  return Object.freeze({
    snapshot() { alive(); return snapshot(); },
    replace, openFile, prepareDownload,
    dispose() {
      if (disposed) return;
      disposed = true; changed(); open.disabled = true; prepare.disabled = true;
      originalSource = originalView = "";
      sourceEditor.removeEventListener("input", changed); filename.removeEventListener("input", changed);
      open.removeEventListener("change", onOpen); prepare.removeEventListener("click", onPrepare); download.removeEventListener("click", onDownload);
    }
  });
}

const HISTORY_PAYLOAD_LIMIT = 20 * 1024 * 1024;
const SOURCE_MATCH_LIMIT = 10000;
function editingOptions(value, allowed) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || Object.keys(value).some(key => !allowed.includes(key))) {
    fail("INVALID_ARGUMENT", "Unsupported source editing options.");
  }
}
function editInteger(value, min, max, name) {
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    fail("INVALID_ARGUMENT", `${name} must be an integer from ${min} to ${max}.`);
  }
  return value;
}
function editSelection(value, length) {
  const { start = 0, end = start, direction = "none" } = value ?? {};
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start > end || end > length
      || !["none", "forward", "backward"].includes(direction)) {
    fail("INVALID_SELECTION", "Selection must be a valid UTF-16 range in the source.");
  }
  return Object.freeze({ start, end, direction });
}
const lowSurrogate = code => code >= 0xdc00 && code <= 0xdfff;

/** Bounded, document-local undo/redo. Store reversible changes, not one full
 * document per keystroke. Payload accounting includes both directions and a
 * fixed per-entry allowance; it is not a measurement of the JS engine's heap.
 * No filename, image authority, worker state or persistent storage lives here.
 */
export function createSourceHistory(initialSource, options = {}) {
  editingOptions(options, ["maxEntries", "maxBytes"]);
  const maxEntries = editInteger(options.maxEntries ?? 100, 1, 1000, "maxEntries");
  const maxBytes = editInteger(options.maxBytes ?? HISTORY_PAYLOAD_LIMIT, 128, HISTORY_PAYLOAD_LIMIT, "maxBytes");
  let current = sourceText(initialSource), past = [], future = [], payloadBytes = 0, disposed = false;
  const alive = () => { if (disposed) fail("SESSION_DISPOSED", "Source history is disposed."); };
  // Force small patches to own their bytes instead of retaining a sliced parent
  // document. ignoreBOM preserves a BOM that happens to be inside a patch.
  const encoder = new TextEncoder(), decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
  const own = value => decoder.decode(encoder.encode(value));
  function record(next, selections = {}) {
    alive(); sourceText(next); editingOptions(selections, ["before", "after"]);
    const before = editSelection(selections.before, current.length), after = editSelection(selections.after, next.length);
    if (next === current) return false;
    let start = 0, oldEnd = current.length, newEnd = next.length;
    while (start < oldEnd && start < newEnd && current.charCodeAt(start) === next.charCodeAt(start)) start++;
    if (lowSurrogate(current.charCodeAt(start)) || lowSurrogate(next.charCodeAt(start))) start--;
    while (oldEnd > start && newEnd > start && current.charCodeAt(oldEnd - 1) === next.charCodeAt(newEnd - 1)) { oldEnd--; newEnd--; }
    if (lowSurrogate(current.charCodeAt(oldEnd)) || lowSurrogate(next.charCodeAt(newEnd))) { oldEnd++; newEnd++; }
    const cost = 128 + 2 * (oldEnd - start + newEnd - start);
    if (cost > maxBytes) fail("BUDGET_EXCEEDED", "This edit exceeds the undo-history payload limit; nothing was recorded.");
    const entry = { start, removed: own(current.slice(start, oldEnd)), inserted: own(next.slice(start, newEnd)), before, after, cost };
    // Only mutate after validating and allocating the complete new entry.
    for (const item of future) payloadBytes -= item.cost;
    future = []; past.push(entry); payloadBytes += cost; current = next;
    while (past.length > maxEntries || payloadBytes > maxBytes) payloadBytes -= past.shift().cost;
    return true;
  }
  function travel(backward) {
    alive();
    const from = backward ? past : future, to = backward ? future : past;
    const entry = from.at(-1);
    if (!entry) return null;
    const remove = backward ? entry.inserted : entry.removed, insert = backward ? entry.removed : entry.inserted;
    const source = current.slice(0, entry.start) + insert + current.slice(entry.start + remove.length);
    from.pop(); to.push(entry); current = source;
    return Object.freeze({ source, selection: backward ? entry.before : entry.after });
  }
  return Object.freeze({
    get source() { alive(); return current; },
    get state() { alive(); return Object.freeze({ canUndo: past.length > 0, canRedo: future.length > 0,
      entries: past.length + future.length, payloadBytes }); },
    record, undo: () => travel(true), redo: () => travel(false),
    dispose() { disposed = true; current = ""; past = []; future = []; payloadBytes = 0; }
  });
}

/** Non-overlapping literal matches in original UTF-16 coordinates. The optional
 * case fold is ASCII-only: Unicode lowercasing can expand text and corrupt source
 * offsets. KMP bounds repetitive-input work without interpreting a user regex.
 */
export function findSourceMatches(source, query, options = {}) {
  editingOptions(options, ["ignoreCase"]);
  const { ignoreCase = false } = options;
  if (typeof ignoreCase !== "boolean") fail("INVALID_ARGUMENT", "ignoreCase must be a boolean.");
  if (typeof query !== "string") fail("INVALID_ARGUMENT", "Find text must be a string.");
  if (query.length === 0) fail("EMPTY_QUERY", "Enter nonempty literal text to find.");
  if (query.length > 1024) fail("QUERY_TOO_LONG", "Find text exceeds 1024 UTF-16 code units.");
  sourceText(source); sourceText(query, "query");
  const fold = code => ignoreCase && code >= 65 && code <= 90 ? code + 32 : code;
  const needle = new Uint16Array(query.length), prefix = new Uint16Array(query.length);
  for (let i = 0; i < query.length; i++) needle[i] = fold(query.charCodeAt(i));
  for (let i = 1, matched = 0; i < needle.length; i++) {
    while (matched > 0 && needle[i] !== needle[matched]) matched = prefix[matched - 1];
    if (needle[i] === needle[matched]) matched++;
    prefix[i] = matched;
  }
  const matches = [];
  for (let i = 0, matched = 0; i < source.length; i++) {
    const code = fold(source.charCodeAt(i));
    while (matched > 0 && code !== needle[matched]) matched = prefix[matched - 1];
    if (code === needle[matched]) matched++;
    if (matched === needle.length) {
      if (matches.length === SOURCE_MATCH_LIMIT) return Object.freeze({ matches: Object.freeze(matches), truncated: true });
      matches.push(Object.freeze({ start: i + 1 - needle.length, end: i + 1 }));
      matched = 0; // Replace-all consumes non-overlapping occurrences exactly once.
    }
  }
  return Object.freeze({ matches: Object.freeze(matches), truncated: false });
}

/** Plan a complete literal replacement before modifying any editor. Reject a
 * truncated inventory rather than presenting a partial replacement as "all".
 * ASCII folding preserves each match's UTF-8 length, so admission precedes join.
 */
export function replaceSourceMatches(source, query, replacement, options = {}) {
  const found = findSourceMatches(source, query, options);
  sourceText(replacement, "replacement");
  if (found.truncated) fail("TOO_MANY_MATCHES", "More than 10000 matches; narrow the search before replacing all. The source is unchanged.");
  const count = found.matches.length;
  if (!count) return Object.freeze({ source, count: 0 });
  const encoder = new TextEncoder();
  const bytes = encoder.encode(source).length + count * (encoder.encode(replacement).length - encoder.encode(query).length);
  if (bytes > FLOW_SOURCE_LIMIT) fail("BUDGET_EXCEEDED", "Replacement would exceed the 4 MiB source limit; the source is unchanged.");
  const chunks = []; let offset = 0;
  for (const match of found.matches) {
    chunks.push(source.slice(offset, match.start), replacement); offset = match.end;
  }
  chunks.push(source.slice(offset));
  return Object.freeze({ source: chunks.join(""), count });
}

/** Source editing is independent of the renderer and storage. Every accepted
 * mutation emits input, including undo/redo, so preview, draft scheduling and
 * prepared-download invalidation observe the same authoritative textarea.
 */
export function createSourceEditingControls({ sourceEditor, undo, redo, query, ignoreCase,
  previous, next, replacement, replace, replaceAll, status }) {
  let history = null, disposed = false, composing = false, before = null, last = null, cache = null;
  const listeners = [];
  const selection = () => editSelection({ start: sourceEditor.selectionStart ?? 0,
    end: sourceEditor.selectionEnd ?? 0, direction: sourceEditor.selectionDirection ?? "none" }, sourceEditor.value.length);
  const bounded = (value, length) => ({ start: Math.min(value?.start ?? 0, length),
    end: Math.min(value?.end ?? 0, length), direction: value?.direction ?? "none" });
  const report = error => { status.textContent = `${error?.code ?? "EDIT_ERROR"}: ${error instanceof Error ? error.message : "Source operation failed."}`; };
  const writable = () => !disposed && !composing && !sourceEditor.disabled && !sourceEditor.readOnly;
  const ready = () => {
    if (disposed) fail("SESSION_DISPOSED", "Source editing controls are disposed.");
    if (!writable()) fail("EDITOR_BUSY", "Finish composing text or unlock the editor before using source editing commands.");
  };
  function inventory() {
    const source = sourceEditor.value, text = query.value, fold = ignoreCase.checked;
    if (!cache || cache.source !== source || cache.query !== text || cache.fold !== fold) {
      cache = { source, query: text, fold, found: text === "" ? { matches: [], truncated: false }
        : findSourceMatches(source, text, { ignoreCase: fold }) };
    }
    return cache.found;
  }
  const selectedIndex = found => found.matches.findIndex(match => match.start === sourceEditor.selectionStart && match.end === sourceEditor.selectionEnd);
  function refresh(announce = false) {
    const active = writable() && history !== null && history.source === sourceEditor.value;
    undo.disabled = !active || !history.state.canUndo;
    redo.disabled = !active || !history.state.canRedo;
    previous.disabled = next.disabled = replace.disabled = replaceAll.disabled = true;
    if (!active) return;
    try {
      const found = inventory(), count = found.matches.length;
      previous.disabled = next.disabled = count === 0;
      replace.disabled = selectedIndex(found) < 0;
      replaceAll.disabled = count === 0 || found.truncated;
      if (announce) status.textContent = query.value === "" ? "Source editing ready. Undo history is local to this document."
        : found.truncated ? "More than 10000 matches. Navigation shows the first 10000; narrow the query before replacing all."
          : `${count} source ${count === 1 ? "match" : "matches"}.`;
    } catch (error) { report(error); }
  }
  // A programmatic input without beforeinput (including image insertion) is one
  // transaction too. Invalid or over-budget input is left visible, never erased;
  // history commands pause until the user repairs it to an admitted source.
  function synchronize() {
    const after = selection();
    if (history === null) history = createSourceHistory(sourceEditor.value);
    else history.record(sourceEditor.value, { before: bounded(before ?? last, history.source.length), after });
    last = after; before = null; cache = null;
  }
  function input(event) {
    if (disposed || composing || event?.isComposing) return;
    try { synchronize(); refresh(true); }
    catch (error) { before = null; cache = null; refresh(); report(error); }
  }
  function reset() {
    history?.dispose(); history = null; composing = false; before = last = cache = null;
    input();
  }
  function publish(value) {
    sourceEditor.value = value.source;
    sourceEditor.setSelectionRange(value.selection.start, value.selection.end, value.selection.direction);
    last = value.selection; before = null; cache = null;
    sourceEditor.dispatchEvent(new Event("input", { bubbles: true }));
    sourceEditor.focus(); refresh();
  }
  function travel(backward) {
    ready(); synchronize();
    const value = backward ? history.undo() : history.redo();
    if (value) publish(value);
    status.textContent = value ? `${backward ? "Undid" : "Redid"} one source edit.` : `No source edit to ${backward ? "undo" : "redo"}.`;
    refresh(); return value !== null;
  }
  function navigate(backward) {
    ready(); synchronize();
    const found = inventory(), count = found.matches.length;
    if (!count) { refresh(true); return false; }
    const range = selection();
    let index = backward ? found.matches.findLastIndex(match => match.end <= range.start)
      : found.matches.findIndex(match => match.start >= range.end);
    const wrapped = index < 0;
    if (wrapped) index = backward ? count - 1 : 0;
    const match = found.matches[index];
    sourceEditor.setSelectionRange(match.start, match.end); sourceEditor.focus(); last = selection();
    refresh();
    status.textContent = `Source match ${index + 1} of ${count}${found.truncated ? "+ (first 10000 only)" : ""}${wrapped ? "; wrapped" : ""}.`;
    return true;
  }
  function commit(source, after) {
    // Textareas normalize newlines. Record exactly the value the editor will
    // expose, not a CRLF spelling that would make history and source diverge.
    source = sourceText(source.replace(/\r\n?/g, "\n"));
    after = editSelection(after, source.length);
    if (!history.record(source, { before: selection(), after })) return false;
    publish({ source, selection: after }); return true;
  }
  function replaceSelected() {
    ready(); synchronize();
    const found = inventory(), index = selectedIndex(found);
    if (index < 0) fail("NO_SELECTED_MATCH", "Select a current source match with Previous or Next before replacing it.");
    const match = found.matches[index], text = sourceText(replacement.value, "replacement").replace(/\r\n?/g, "\n");
    const source = sourceEditor.value;
    const changed = commit(source.slice(0, match.start) + text + source.slice(match.end), { start: match.start, end: match.start + text.length });
    status.textContent = changed ? "Replaced the selected source match. Undo restores it." : "The selected match already equals the replacement.";
    return changed;
  }
  function replaceEvery() {
    ready(); synchronize();
    const result = replaceSourceMatches(sourceEditor.value, query.value, replacement.value.replace(/\r\n?/g, "\n"), { ignoreCase: ignoreCase.checked });
    const changed = commit(result.source, bounded(selection(), result.source.length));
    status.textContent = changed ? `Replaced ${result.count} source matches in one undoable edit.` : "No source text changed.";
    return changed;
  }
  const run = action => event => {
    if (event?.defaultPrevented || disposed) return;
    try { action(); } catch (error) { refresh(); report(error); }
  };
  const listen = (element, type, handler) => { element.addEventListener(type, handler); listeners.push([element, type, handler]); };
  const selectionChanged = () => {
    if (!disposed && !composing && before === null && history?.source === sourceEditor.value) last = selection();
    refresh();
  };
  listen(sourceEditor, "input", input);
  listen(sourceEditor, "fmd-document-replaced", reset);
  listen(sourceEditor, "beforeinput", event => {
    if (event.defaultPrevented || composing || event.isComposing || !writable()) return;
    if (event.cancelable && ["historyUndo", "historyRedo"].includes(event.inputType)) {
      event.preventDefault(); run(() => travel(event.inputType === "historyUndo"))();
    } else before = selection();
  });
  listen(sourceEditor, "compositionstart", () => { before = selection(); composing = true; refresh(); });
  listen(sourceEditor, "compositionend", () => { composing = false; input(); });
  for (const type of ["select", "keyup", "click"]) listen(sourceEditor, type, selectionChanged);
  listen(sourceEditor, "keydown", event => {
    if (event.defaultPrevented || !writable() || event.isComposing || event.keyCode === 229 || event.altKey || !(event.ctrlKey || event.metaKey)) return;
    const key = event.key?.toLowerCase();
    if (key === "z" || (key === "y" && !event.shiftKey)) {
      event.preventDefault(); run(() => travel(key === "z" && !event.shiftKey))();
    } else if (key === "f") {
      event.preventDefault();
      const range = selection(), selected = sourceEditor.value.slice(range.start, range.end);
      if (selected.length > 0 && selected.length <= 1024 && !/[\r\n]/.test(selected)) query.value = selected;
      const panel = query.closest?.("details");
      if (panel) panel.open = true;
      query.focus(); query.select(); refresh(true);
    }
  });
  listen(query, "keydown", event => {
    if (!event.defaultPrevented && !event.isComposing && event.key === "Enter") {
      event.preventDefault(); run(() => navigate(event.shiftKey))();
    }
  });
  listen(query, "input", () => { cache = null; refresh(true); });
  listen(ignoreCase, "change", () => { cache = null; refresh(true); });
  listen(undo, "click", run(() => travel(true))); listen(redo, "click", run(() => travel(false)));
  listen(previous, "click", run(() => navigate(true))); listen(next, "click", run(() => navigate(false)));
  listen(replace, "click", run(replaceSelected)); listen(replaceAll, "click", run(replaceEvery));
  reset();
  return Object.freeze({
    undo: () => travel(true), redo: () => travel(false), findNext: () => navigate(false), findPrevious: () => navigate(true),
    replaceSelected, replaceAll: replaceEvery,
    dispose() {
      if (disposed) return;
      disposed = true; history?.dispose(); history = null; before = last = cache = null;
      for (const [element, type, handler] of listeners) element.removeEventListener(type, handler);
      listeners.length = 0; refresh();
    }
  });
}
