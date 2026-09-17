// Export admission over the preview controller's private state. No rendering or
// source reconstruction; the session exports its own captured native document.
import { FlowError } from "../flow_session.mjs";
import { validateFlowExportResult } from "../flow_export.mjs";
const same = (a, b) => a?.revision === b?.revision && a?.layoutRevision === b?.layoutRevision;
const fail = (code, message) => { throw new FlowError(code, message); };
export function createPreviewExport(readState) {
  let job = null;
  return async function exportDocument(format, options, source) {
    const before = readState();
    if (before.disposed) fail("SESSION_DISPOSED", "preview is disposed");
    if (job) fail("EXPORT_BUSY", "a previous export has not settled");
    const current = before.session;
    if (!current || current.disposed || before.status !== "ready"
        || source !== before.source || source !== before.desiredSource) {
      fail("STALE_REVISION", "export requires the current editor source to be applied successfully");
    }
    if (before.imagesBusy) fail("ASSET_BUSY", "finish the current authorized image batch before exporting");
    const expected = Object.freeze({ ...current.token });
    if (!same(expected, before.frame)) fail("STALE_LAYOUT", "export does not match the displayed document");
    const ticket = {}; job = ticket;
    try {
      // No abort tied to ordinary typing/scrolling: an in-flight worker abort
      // would destroy the session. Stale results are discarded instead.
      const output = await current.exportDocument(format, options, expected);
      const after = readState();
      if (after.disposed) fail("SESSION_DISPOSED", "preview was disposed during export");
      if (after.epoch !== before.epoch || after.session !== current || current.disposed
          || after.source !== source || after.desiredSource !== source) {
        fail("STALE_REVISION", "preview source or session changed during export");
      }
      if (!same(current.token, expected) || !same(after.frame, expected)) {
        fail("STALE_LAYOUT", "preview assets or layout changed during export");
      }
      if (output?.format !== format) fail("INVALID_WASM_RESPONSE", "export format does not match the request");
      return validateFlowExportResult(output, expected, options?.maxOutputBytes);
    } finally { if (job === ticket) job = null; }
  };
}
