// Production preview admission around the real Flow export/verification adapter.
// The native parser/layout renderer is the explicit fixture, not generated WASM.
import assert from "node:assert/strict";
import test from "node:test";
import { createPreviewExport } from "../demo/flow_preview_export.mjs";
import { createVerificationFixture } from "./flow_pdf_verification_fixture.mjs";

const code = expected => error => error.code === expected;
const gate = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
function fixture(configuration = {}) {
  const native = createVerificationFixture(configuration, undefined, { profile: "commonmark-gfm" });
  let receivedControl;
  const session = {
    get token() { return native.api.token; },
    get disposed() { return native.api.disposed; },
    get supportsPdfVerification() { return native.api.supportsPdfVerification; },
    verifyPdf(options, token, control) { receivedControl = control; return native.api.verifyPdf(options, token); },
    exportDocument: (...args) => native.api.exportDocument(...args),
  };
  const state = { session, source: native.state.source, desiredSource: native.state.source,
    font: "serif", profile: "commonmark-gfm", status: "ready", frame: { ...session.token },
    imagesBusy: false, layoutPending: false, layoutVersion: 1, epoch: 1, disposed: false };
  const publish = createPreviewExport(() => ({ ...state }));
  return { native, session, state, publish, control: () => receivedControl,
    verify: options => publish.verifyPdf(options, native.state.source) };
}

test("verification returns the full owned report and forwards request-scoped cancellation/deadline", async () => {
  const f = fixture(), abort = new AbortController();
  const control = { signal: abort.signal, timeoutMs: 15000 };
  const result = await f.publish.verifyPdf({ page: { size: "a5" }, typography: { lineHeight: 1.4 } }, f.state.source, control);
  assert.equal(f.control(), control);
  assert.equal(result.format, "pdf-verification");
  assert.equal(result.font, "serif"); assert.equal(result.profile, "commonmark-gfm");
  assert.equal(result.report.pages[0].runs[0].text, f.state.source);
  assert.deepEqual(result.report.probe.images, [["offscreen.png", [1, 2, 3]]]);
  assert.equal(result.report.digest, "fnv1a64:0123456789abcdef");
  f.native.state.report.findings[0].detail = "native reuse";
  assert.notEqual(result.report.findings[0].detail, "native reuse");
});

test("unapplied source, layout, images and missing verifier are refused before native work", async () => {
  for (const [change, expected] of [
    [f => { f.state.desiredSource += "unapplied"; }, "STALE_REVISION"],
    [f => { f.state.layoutPending = true; }, "STALE_LAYOUT"],
    [f => { f.state.imagesBusy = true; }, "ASSET_BUSY"],
    [f => { f.state.frame.layoutRevision = "1"; }, "STALE_LAYOUT"],
  ]) {
    const f = fixture(); change(f);
    await assert.rejects(f.verify({}), code(expected)); assert.equal(f.native.calls.length, 0);
  }
  const legacy = fixture({ legacy: true });
  await assert.rejects(legacy.verify({}), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(legacy.native.calls.length, 0);
});

test("every source/layout/session ownership change rejects a late report without aborting ordinary edits", async () => {
  for (const [change, expected] of [
    [f => { f.state.desiredSource += "edited"; }, "STALE_REVISION"],
    [f => { f.state.epoch++; }, "STALE_REVISION"],
    [f => { f.state.session = { ...f.session }; }, "STALE_REVISION"],
    [f => { f.state.layoutVersion++; }, "STALE_LAYOUT"],
    [f => { f.state.imagesBusy = true; }, "ASSET_BUSY"],
    [f => { f.state.disposed = true; }, "SESSION_DISPOSED"],
  ]) {
    const f = fixture(), held = gate(); f.native.state.hold = held.promise;
    const abort = new AbortController();
    const pending = f.publish.verifyPdf({}, f.state.source, { signal: abort.signal });
    change(f); held.resolve(); await assert.rejects(pending, code(expected));
    assert.equal(abort.signal.aborted, false);
  }
});

test("verification and publication retain one physical slot until stale work settles", async () => {
  const f = fixture(), held = gate(); f.native.state.hold = held.promise;
  const pending = f.verify({});
  await assert.rejects(f.publish("pdf", {}, f.state.source), code("EXPORT_BUSY"));
  await assert.rejects(f.verify({}), code("EXPORT_BUSY"));
  f.state.epoch++; held.resolve(); await assert.rejects(pending, code("STALE_REVISION"));
  f.native.state.hold = null;
  assert.equal((await f.publish("pdf", {}, f.state.source)).format, "pdf");
  const exported = gate(); f.native.state.hold = exported.promise;
  const publication = f.publish("pdf", {}, f.state.source);
  await assert.rejects(f.verify({}), code("EXPORT_BUSY")); exported.resolve(); await publication;
});

test("scroll-only paint preserves ownership, but metadata and lower byte-budget mismatches do not", async () => {
  const f = fixture(), held = gate(); f.native.state.hold = held.promise;
  const pending = f.verify({}); f.state.status = "busy"; held.resolve();
  assert.equal((await pending).format, "pdf-verification");
  f.state.status = "ready"; f.native.state.hold = null;
  await assert.rejects(f.verify({ maxOutputBytes: 1 }), code("BUDGET_EXCEEDED"));
  for (const field of ["font", "sourceLengthBytes", "format"]) {
    const fresh = fixture(), original = fresh.session.verifyPdf;
    fresh.session.verifyPdf = async (...args) => ({ ...await original(...args),
      [field]: { font: "sans", sourceLengthBytes: 0, format: "pdf" }[field] });
    await assert.rejects(fresh.verify({}), code("INVALID_WASM_RESPONSE"));
  }
});

test("verification does not create an undocumented export format alias", async () => {
  const f = fixture();
  await assert.rejects(f.publish("pdf-verification", {}, f.state.source), code("INVALID_OPTIONS"));
  assert.equal(f.native.calls.length, 0);
});
