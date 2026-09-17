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
  onReplace, confirmReplace = () => true, urls = URL }) {
  let disposed = false, busy = false, epoch = 0, url = null, published = null;
  // textarea.value normalizes CR/CRLF to LF. Preserve the imported bytes while
  // its displayed value is unchanged; edited text follows the textarea's LF
  // convention. Returning to the exact imported view also restores its bytes.
  let originalSource = sourceEditor.value, originalView = sourceEditor.value;
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
