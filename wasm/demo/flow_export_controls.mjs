// User-initiated export and download. At most one physical export and one Blob
// URL; no synthetic click, auto-save, remote navigation, or innerHTML is used.
import { FlowError } from "../flow_session.mjs";
import { normalizeFlowExport, validateFlowExportResult } from "../flow_export.mjs";
const sameOptions = (a, b) => Object.keys(a).length === Object.keys(b).length && Object.keys(a).every(key => a[key] === b[key]);
const same = (a, b) => a?.revision === b?.revision && a?.layoutRevision === b?.layoutRevision;
export function createExportControls({ html, pdf, download, status, sourceEditor, exportDocument, urls = URL,
  readOptions = format => format === "pdf" ? { pageNumbers: true, metadataEpochSeconds: 0 } : {} }) {
  let state = null, pending = false, disposed = false, epoch = 0, published = null, url = null;
  const available = () => !disposed && !pending && state?.status === "ready" && state.images?.status !== "loading";
  const buttons = () => { html.disabled = pdf.disabled = !available(); };
  function revoke() {
    download.hidden = true; download.removeAttribute("href"); download.removeAttribute("download");
    published = null;
    if (url !== null) { const previous = url; url = null; urls.revokeObjectURL(previous); }
  }
  function invalidate(message = "Document changed; prepare a fresh export.") {
    epoch++; revoke();
    if (!disposed) status.textContent = message;
    buttons();
  }
  async function prepare(format) {
    if (!available()) return;
    const source = sourceEditor.value, expected = { ...state.frame }, ticket = epoch;
    revoke(); pending = true; buttons();
    status.textContent = `Preparing ${format.toUpperCase()} in the worker. Editing remains available.`;
    try {
      // Capture and validate the complete applied settings in the click task.
      // Caller mutations and unapplied form text cannot change an in-flight job.
      const [, options] = normalizeFlowExport(format, readOptions(format), expected);
      const result = await exportDocument(format, options, source);
      if (disposed || epoch !== ticket || sourceEditor.value !== source || !same(expected, state?.frame)) {
        throw new FlowError("STALE_REVISION", "document changed before download publication");
      }
      const [, currentOptions] = normalizeFlowExport(format, readOptions(format), expected);
      if (!sameOptions(options, currentOptions)) throw new FlowError("STALE_OPTIONS", "publishing settings changed during export");
      validateFlowExportResult(result, expected, options.maxOutputBytes);
      if (result.format !== format) throw new FlowError("INVALID_WASM_RESPONSE", "unexpected export format");
      const blob = new Blob([result.bytes], { type: result.mimeType });
      url = urls.createObjectURL(blob);
      published = { source, token: expected, epoch: ticket, format, options };
      download.href = url; download.download = `document.${format}`;
      download.textContent = `Download ${format.toUpperCase()} (${result.bytes.length.toLocaleString()} bytes)`;
      download.hidden = false;
      const first = result.diagnostics[0];
      status.textContent = `Prepared source ${result.revision}. ${result.assetCount} embedded asset payloads; ${result.diagnostics.length} renderer diagnostics.`
        + (first ? ` First ${first.severity}: ${first.message}` : " Click Download to save the document.");
    } catch (error) {
      if (!disposed && epoch === ticket) {
        revoke(); status.textContent = `${typeof error?.code === "string" ? error.code : "EXPORT_FAILED"}: ${error instanceof Error ? error.message : "export failed"}. Source remains in the editor.`;
      }
    } finally { pending = false; buttons(); }
  }
  const onHtml = () => { void prepare("html"); }, onPdf = () => { void prepare("pdf"); };
  const onInput = () => invalidate();
  const onDownload = event => {
    if (disposed || !published || published.epoch !== epoch || sourceEditor.value !== published.source
        || !same(published.token, state?.frame) || ["idle", "disposed", "error"].includes(state?.status)) {
      event.preventDefault(); invalidate("The prepared download is stale; prepare it again.");
      return;
    }
    try {
      const [, options] = normalizeFlowExport(published.format, readOptions(published.format), published.token);
      if (!sameOptions(options, published.options)) throw new FlowError("STALE_OPTIONS", "publishing settings changed");
    } catch {
      event.preventDefault(); invalidate("Publishing settings changed or are unapplied; prepare a fresh export.");
    }
  };
  html.addEventListener("click", onHtml); pdf.addEventListener("click", onPdf);
  sourceEditor.addEventListener("input", onInput); download.addEventListener("click", onDownload);
  revoke(); buttons();
  return Object.freeze({
    update(next) {
      if (disposed) return;
      const changed = state && (!same(state.frame, next.frame) || ["idle", "disposed", "error"].includes(next.status));
      state = next;
      if (changed) invalidate();
      buttons();
    },
    invalidate,
    dispose() {
      if (disposed) return;
      disposed = true; epoch++; revoke(); buttons();
      html.removeEventListener("click", onHtml); pdf.removeEventListener("click", onPdf);
      sourceEditor.removeEventListener("input", onInput); download.removeEventListener("click", onDownload);
    }
  });
}
