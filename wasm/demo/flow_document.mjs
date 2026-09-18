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
