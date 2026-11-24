// Configured native PDF verification, owned by the current editor and settings.
// Results are withdrawn when their source, layout, image grant or options change.
import { decodeFlowPdfVerification, encodeFlowPdfVerification, normalizeFlowPdfVerification } from "../flow_export.mjs";
import { FlowError } from "../flow_session.mjs";
import { sameOptions } from "./flow_export_controls.mjs";

const same = (a, b) => a?.revision === b?.revision && a?.layoutRevision === b?.layoutRevision;
export function createPdfVerificationControls({
  verify, cancel, download, status, findings, sourceEditor, verifyPdf,
  readOptions = () => ({ pageNumbers: true, metadataEpochSeconds: 0 }),
  urls = URL,
}) {
  let state = null, pending = null, disposed = false, composing = false;
  let compositionTimer = null, epoch = 0, published = null, url = null;
  const available = () => !disposed && !pending && !composing
    && state?.status === "ready" && state.frame && state.images?.status !== "loading";
  function buttons() {
    verify.disabled = !available();
    cancel.disabled = disposed || !pending;
  }
  function revoke() {
    download.hidden = true;
    download.removeAttribute("href");
    download.removeAttribute("download");
    findings.textContent = "";
    published = null;
    if (url !== null) {
      const previous = url;
      url = null;
      urls.revokeObjectURL(previous);
    }
  }
  function invalidate(message = "Document changed; verify the current PDF layout again.") {
    epoch++;
    revoke();
    if (!disposed) status.textContent = message;
    buttons();
  }
  async function prepare() {
    if (!available()) return;
    const source = sourceEditor.value, expected = { ...state.frame }, ticket = epoch;
    const operation = new AbortController();
    revoke();
    pending = operation;
    buttons();
    status.textContent = "Verifying the configured PDF layout. Editing remains available.";
    try {
      const [options] = normalizeFlowPdfVerification(readOptions(), expected);
      const result = await verifyPdf(options, source, {
        signal: operation.signal, timeoutMs: 15_000,
      });
      if (disposed || ticket !== epoch || sourceEditor.value !== source || !same(expected, state?.frame))
        throw new FlowError("STALE_REVISION", "document changed before report publication");
      const [current] = normalizeFlowPdfVerification(readOptions(), expected);
      if (!sameOptions(options, current))
        throw new FlowError("STALE_OPTIONS", "PDF settings changed during verification");
      const wire = encodeFlowPdfVerification(result, expected, options.maxOutputBytes, result?.profile);
      if (wire.sourceLengthBytes !== new TextEncoder().encode(source).length)
        throw new FlowError("INVALID_WASM_RESPONSE", "verification source length does not match the editor");
      const { report } = decodeFlowPdfVerification(wire, expected, options.maxOutputBytes, result.profile);
      url = urls.createObjectURL(new Blob([wire.json], { type: "application/json; charset=utf-8" }));
      published = { source, token: expected, epoch: ticket, options };
      download.href = url;
      download.download = "document-pdf-verification.json";
      download.textContent = "Download full verification report";
      download.hidden = false;
      const entries = report.findings.slice(0, 100).map(finding =>
        `${finding.code}: ${finding.detail}`.slice(0, 2000));
      if (report.findings.length > entries.length)
        entries.push(`Showing ${entries.length} of ${report.findings.length} findings. Download the full report for all details.`);
      findings.textContent = entries.join("\n");
      status.textContent = `Source ${result.revision}: ${report.page_count} PDF pages; ${report.findings.length} findings. `
        + `${report.anchors.resolved} resolved anchors; ${report.anchors.unresolved.length} unresolved. `
        + "The full JSON includes all pages, text runs, findings and the native digest.";
    } catch (error) {
      if (!disposed && ticket === epoch) {
        revoke();
        status.textContent = `${typeof error?.code === "string" ? error.code : "VERIFICATION_FAILED"}: `
          + `${error instanceof Error ? error.message : "verification failed"}. Source remains in the editor.`;
      }
    } finally {
      if (pending === operation) pending = null;
      buttons();
    }
  }
  const onVerify = () => { void prepare(); };
  const onCancel = () => {
    if (!pending || disposed) return;
    const operation = pending;
    invalidate("Verification cancelled. Restart the preview if its active worker was stopped; source remains available.");
    operation.abort();
  };
  const onInput = () => invalidate();
  const onCompositionStart = () => {
    if (compositionTimer !== null) clearTimeout(compositionTimer);
    compositionTimer = null;
    composing = true;
    invalidate("Finish text composition before verifying the PDF layout.");
  };
  const onCompositionEnd = () => {
    if (compositionTimer !== null) clearTimeout(compositionTimer);
    composing = true;
    compositionTimer = setTimeout(() => {
      compositionTimer = null;
      if (disposed) return;
      composing = false;
      buttons();
    }, 0);
  };
  const onDownload = event => {
    try {
      if (disposed || composing || !published || published.epoch !== epoch
          || sourceEditor.value !== published.source || !same(published.token, state?.frame)
          || state.images?.status === "loading" || ["idle", "disposed", "error"].includes(state.status))
        throw new FlowError("STALE_REVISION", "verification report is stale");
      const [options] = normalizeFlowPdfVerification(readOptions(), published.token);
      if (!sameOptions(options, published.options))
        throw new FlowError("STALE_OPTIONS", "PDF settings changed");
    } catch {
      event.preventDefault();
      invalidate("The report is stale or settings are unapplied; verify the current PDF layout again.");
    }
  };
  verify.addEventListener("click", onVerify);
  cancel.addEventListener("click", onCancel);
  download.addEventListener("click", onDownload);
  sourceEditor.addEventListener("input", onInput);
  sourceEditor.addEventListener("compositionstart", onCompositionStart);
  sourceEditor.addEventListener("compositionend", onCompositionEnd);
  revoke();
  buttons();
  return Object.freeze({
    update(next) {
      if (disposed) return;
      const changed = state && (!same(state.frame, next.frame)
        || ["idle", "disposed", "error"].includes(next.status)
        || (next.images?.status === "loading" && state.images?.status !== "loading"));
      state = next;
      if (changed) invalidate();
      buttons();
    },
    invalidate,
    dispose() {
      if (disposed) return;
      disposed = true;
      epoch++;
      if (compositionTimer !== null) clearTimeout(compositionTimer);
      compositionTimer = null;
      pending?.abort();
      revoke();
      buttons();
      verify.removeEventListener("click", onVerify);
      cancel.removeEventListener("click", onCancel);
      download.removeEventListener("click", onDownload);
      sourceEditor.removeEventListener("input", onInput);
      sourceEditor.removeEventListener("compositionstart", onCompositionStart);
      sourceEditor.removeEventListener("compositionend", onCompositionEnd);
    },
  });
}
