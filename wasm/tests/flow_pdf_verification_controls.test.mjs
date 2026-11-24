// Production controls with explicit DOM/URL and native report doubles.
import assert from "node:assert/strict";
import test from "node:test";
import { createPdfVerificationControls } from "../demo/flow_pdf_verification_controls.mjs";

const turn = () => new Promise(resolve => setImmediate(resolve));
const gate = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
class Element extends EventTarget {
  value = "é中😀"; disabled = false; hidden = false; textContent = "";
  removeAttribute(key) { delete this[key]; }
  click() { const event = new Event("click", { cancelable: true }); this.dispatchEvent(event); return event; }
}
function fixture() {
  const elements = Object.fromEntries(["verify", "cancel", "download", "status", "findings", "sourceEditor"]
    .map(name => [name, new Element()]));
  const calls = [], blobs = [], revoked = [];
  const state = { status: "ready", frame: { revision: "1", layoutRevision: "2" }, images: null };
  const result = { schemaVersion: 1, format: "pdf-verification", ...state.frame,
    font: "serif", sourceLengthBytes: new TextEncoder().encode(elements.sourceEditor.value).length,
    assetCount: 1, assetBytes: 3,
    report: { schema_version: "1", target: "pdf", verdict: "findings", page_count: 1,
      anchors: { resolved: 4, unresolved: ["missing"] },
      findings: [{ code: "pdf.fixture", detail: "<script>literal native finding</script>" }],
      pages: [{ number: 1, runs: [{ text: "full native text", x: 72, y: 700, size: 11, kind: "body" }] }],
      digest: "fnv1a64:0123456789abcdef", additional: { retained: true } } };
  const f = { ...elements, state, result, calls, blobs, revoked, held: null, failure: null,
    options: { typography: { themeSpacing: true, lineHeight: 1.4 }, page: { size: "a5" } } };
  f.controls = createPdfVerificationControls({
    ...elements, readOptions: () => f.options,
    urls: { createObjectURL(blob) { blobs.push(blob); return `blob:report-${blobs.length}`; },
      revokeObjectURL(value) { revoked.push(value); } },
    async verifyPdf(options, source, control) {
      calls.push({ options, source, control });
      if (f.held) await f.held.promise;
      if (f.failure) throw f.failure;
      return result;
    },
  });
  f.controls.update(state);
  f.run = async () => { f.verify.click(); await turn(); };
  return f;
}

test("shows native findings as text and offers the complete report only on explicit download", async t => {
  const f = fixture(); t.after(() => f.controls.dispose());
  let downloads = 0; f.download.addEventListener("click", () => downloads++);
  await f.run();
  assert.equal(downloads, 0); assert.equal(f.download.hidden, false);
  assert.equal(f.download.download, "document-pdf-verification.json");
  assert.deepEqual(JSON.parse(await f.blobs[0].text()), f.result.report);
  assert.match(f.findings.textContent, /<script>literal native finding<\/script>/);
  assert.equal(Object.hasOwn(f.findings, "innerHTML"), false);
  assert.match(f.status.textContent, /1 PDF pages; 1 findings/);
  assert.match(f.status.textContent, /4 resolved anchors; 1 unresolved/);
  assert.equal(f.calls[0].control.timeoutMs, 15000);
  assert.equal(f.calls[0].control.signal.aborted, false);
  assert.equal(f.download.click().defaultPrevented, false);
});

test("visible finding bounds do not truncate the downloaded native JSON", async t => {
  const f = fixture(); t.after(() => f.controls.dispose());
  f.result.report.findings = Array.from({ length: 120 }, (_, i) => ({ code: `finding.${i}`, detail: "x".repeat(2500) }));
  await f.run();
  assert.match(f.findings.textContent, /Showing 100 of 120 findings/);
  assert(f.findings.textContent.length < 201000);
  const report = JSON.parse(await f.blobs[0].text());
  assert.equal(report.findings.length, 120); assert.equal(report.findings[119].detail.length, 2500);
});

for (const change of [
  f => { f.sourceEditor.value += "edit"; f.sourceEditor.dispatchEvent(new Event("input")); },
  f => f.controls.invalidate("Settings changed"),
  f => f.controls.update({ ...f.state, frame: { revision: "2", layoutRevision: "3" } }),
  f => f.controls.update({ ...f.state, images: { status: "loading" } }),
  f => f.controls.update({ status: "idle", frame: null }),
]) test("editor/settings/resource ownership changes revoke and clear an existing report", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); await f.run(); change(f);
  assert.equal(f.download.hidden, true); assert.equal(f.findings.textContent, "");
  assert.deepEqual(f.revoked, ["blob:report-1"]); assert.equal(f.download.click().defaultPrevented, true);
});

test("unannounced source and nested option changes are rechecked on download click", async t => {
  for (const change of [f => { f.sourceEditor.value += "programmatic"; },
    f => { f.options.typography.lineHeight = 2; }]) {
    const f = fixture(); t.after(() => f.controls.dispose()); await f.run(); change(f);
    assert.equal(f.download.click().defaultPrevented, true); assert.equal(f.download.hidden, true);
  }
});

test("pending reports snapshot options and reject silent nested edits before publication", async t => {
  const f = fixture(), held = gate(); f.held = held; t.after(() => f.controls.dispose());
  f.verify.click(); f.options.typography.lineHeight = 2;
  assert.equal(f.calls[0].options.typography.lineHeight, 1.4);
  held.resolve(); await turn();
  assert.equal(f.blobs.length, 0); assert.match(f.status.textContent, /STALE_OPTIONS/);
});

test("ordinary edits discard late reports without aborting their worker and retain the physical slot", async t => {
  const f = fixture(), held = gate(); f.held = held; t.after(() => f.controls.dispose());
  f.verify.click(); f.sourceEditor.value += "edit"; f.sourceEditor.dispatchEvent(new Event("input"));
  f.verify.click(); assert.equal(f.calls.length, 1); assert.equal(f.calls[0].control.signal.aborted, false);
  held.resolve(); await turn(); assert.equal(f.blobs.length, 0);
});

test("explicit cancellation aborts only the captured verification and suppresses its late completion", async t => {
  const f = fixture(), held = gate(); f.held = held; t.after(() => f.controls.dispose());
  assert.equal(f.cancel.disabled, true); f.verify.click(); assert.equal(f.cancel.disabled, false);
  f.cancel.click(); assert.equal(f.calls[0].control.signal.aborted, true);
  held.resolve(); await turn(); assert.equal(f.blobs.length, 0); assert.equal(f.cancel.disabled, true);
  assert.match(f.status.textContent, /cancelled/);
  f.held = null; await f.run(); assert.equal(f.download.hidden, false);
});

test("scroll-only painting preserves a report and late completion for the same layout", async t => {
  const f = fixture(), held = gate(); f.held = held; t.after(() => f.controls.dispose());
  f.verify.click(); f.controls.update({ ...f.state, status: "busy" }); held.resolve(); await turn();
  assert.equal(f.download.hidden, false); assert.equal(f.download.click().defaultPrevented, false);
  f.controls.update(f.state); assert.equal(f.revoked.length, 0);
});

test("report schema, source ownership, strict errors and byte budgets never yield a misleading download", async t => {
  for (const [change, code] of [
    [f => { f.result.report.page_count = 2; }, "INVALID_WASM_RESPONSE"],
    [f => { f.result.sourceLengthBytes = 0; }, "INVALID_WASM_RESPONSE"],
    [f => { f.options.maxOutputBytes = 32; }, "BUDGET_EXCEEDED"],
    [f => { f.failure = Object.assign(new Error("exact plan refused"), { code: "PAGINATION_INFEASIBLE" }); }, "PAGINATION_INFEASIBLE"],
  ]) {
    const f = fixture(); t.after(() => f.controls.dispose()); change(f); await f.run();
    assert.equal(f.blobs.length, 0); assert.equal(f.download.hidden, true);
    assert.match(f.status.textContent, new RegExp(code)); assert.equal(f.sourceEditor.value, "é中😀");
  }
});

test("composition and disposal prevent stale publication and release report URLs", async () => {
  const f = fixture(); f.sourceEditor.dispatchEvent(new Event("compositionstart"));
  await f.run(); assert.equal(f.calls.length, 0);
  f.sourceEditor.dispatchEvent(new Event("compositionend"));
  assert.equal(f.verify.disabled, true); await new Promise(resolve => setTimeout(resolve, 5));
  await f.run(); assert.equal(f.download.hidden, false);
  f.controls.dispose(); assert.deepEqual(f.revoked, ["blob:report-1"]);
  assert.equal(f.verify.disabled, true); assert.equal(f.cancel.disabled, true);
  await f.run(); assert.equal(f.calls.length, 1);
});
