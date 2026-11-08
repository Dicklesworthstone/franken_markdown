// Explicit native boundary double for report ownership and dispatch. The byte
// payloads observe calls; they are not PDFs or evidence of renderer behavior.
import assert from "node:assert/strict";
import { nativeBookFixture } from "./book_pdfa_fixture.mjs";

export function renderDiagnosticsFixture({ legacy = false } = {}) {
  const fixture = nativeBookFixture(), { NativeBook, state } = fixture;
  Object.assign(state, { reportedCalls: [], legacyCalls: [], results: [], reportFreed: 0,
    beforeReport: null, changeResult: null });
  for (const method of ["renderPdf", "renderPdfWithPage", "renderPdfArchival"]) {
    const original = NativeBook.prototype[method];
    NativeBook.prototype[method] = function (...args) {
      state.legacyCalls.push(method);
      return original.apply(this, args);
    };
  }
  if (!legacy) for (const method of ["renderPdfReported", "renderPdfWithPageReported", "renderPdfArchivalReported"]) {
    NativeBook.prototype[method] = function (...args) {
      state.reportedCalls.push({ method, args: structuredClone(args) });
      state.beforeReport?.();
      const [profile, strict, page] = method === "renderPdfArchivalReported" ? args : ["off", false, args[0]];
      const bytes = this.emit("pdf", page, profile, strict);
      const notes = [{ code: "unresolved_image", severity: "warning", start: 0, end: 0,
        scope: "document", message: `Render ${state.reportedCalls.length}: image unavailable — α 😀` }];
      const record = { notes, freed: 0, result: null };
      record.result = { format: "pdf", mimeType: "application/pdf", extension: "pdf", sourceLength: this.sourceLength,
        bytes, diagnosticsJson: () => JSON.stringify(notes),
        free() { assert.equal(record.freed++, 0, "release the report handle exactly once"); state.reportFreed++; },
      };
      state.results.push(record);
      state.changeResult?.(record.result, record);
      return record.result;
    };
  }
  return fixture;
}
