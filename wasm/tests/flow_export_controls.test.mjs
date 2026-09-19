// Production controls using EventTarget/Blob and explicit element/URL doubles.
// Native navigation, browser downloads and generated WASM are separate gates.
import test from "node:test";
import assert from "node:assert/strict";
import { createExportControls } from "../demo/flow_export_controls.mjs";
const gate = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
class Element extends EventTarget {
  disabled = false; hidden = false; value = "old"; textContent = ""; href = ""; download = "";
  removeAttribute(key) { delete this[key]; }
  click() { const event = new Event("click", { cancelable: true }); this.dispatchEvent(event); return event; }
}
function fixture(readOptions) {
  const html = new Element(), pdf = new Element(), download = new Element(), status = new Element(), sourceEditor = new Element();
  const calls = [], created = [], revoked = [], token = { revision: "1", layoutRevision: "2" }; let hold = null, error = null;
  const state = { status: "ready", frame: { ...token }, images: null };
  const result = { schemaVersion: 1, ...token, format: "pdf", mimeType: "application/pdf", bytes: new Uint8Array([1, 2]),
    diagnostics: [], font: "sans", sourceLengthBytes: 3, assetCount: 0, assetBytes: 0 };
  const controls = createExportControls({ html, pdf, download, status, sourceEditor, readOptions,
    urls: { createObjectURL(blob) { created.push(blob); return `blob:test-${created.length}`; }, revokeObjectURL(url) { revoked.push(url); } },
    async exportDocument(format, options, source) {
      calls.push({ format, options, source }); if (hold) await hold.promise; if (error) throw error;
      return { ...result, format, mimeType: format === "pdf" ? "application/pdf" : "text/html; charset=utf-8" };
    }
  });
  controls.update(state);
  return { html, pdf, download, status, sourceEditor, controls, calls, state, result, created, revoked,
    hold(value) { hold = value; }, error(value) { error = value; } };
}
test("prepares an owned Blob, never auto-clicks, and makes a separate download available", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); let clicks = 0;
  f.download.addEventListener("click", () => clicks++); f.pdf.click(); await tick();
  assert.equal(clicks, 0); assert.equal(f.download.hidden, false); assert.equal(f.download.download, "document.pdf");
  assert.equal(f.created[0].type, "application/pdf"); assert.deepEqual(new Uint8Array(await f.created[0].arrayBuffer()), f.result.bytes);
  assert.deepEqual(f.calls[0], { format: "pdf", options: { pageNumbers: true, metadataEpochSeconds: 0, maxOutputBytes: 64 * 1024 * 1024 }, source: "old" });
  assert.equal(f.download.click().defaultPrevented, false);
});
test("new exports revoke the previous object URL and keep only one downloadable document", async t => {
  const f = fixture(); t.after(() => f.controls.dispose());
  f.pdf.click(); await tick(); f.html.click(); await tick();
  assert.deepEqual(f.revoked, ["blob:test-1"]); assert.equal(f.download.href, "blob:test-2");
  assert.equal(f.download.download, "document.html");
});
test("typing immediately revokes the old download, before any preview update", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); f.pdf.click(); await tick();
  f.sourceEditor.value = "new"; f.sourceEditor.dispatchEvent(new Event("input"));
  assert.equal(f.download.hidden, true); assert.deepEqual(f.revoked, ["blob:test-1"]);
  assert.equal(f.download.click().defaultPrevented, true);
});
test("programmatic source edits are caught again on download click", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); f.pdf.click(); await tick();
  f.sourceEditor.value = "new without input event"; assert.equal(f.download.click().defaultPrevented, true);
  assert.equal(f.download.hidden, true); assert.equal(f.revoked.length, 1);
});
test("late export after typing/restart/disposal is discarded without creating a URL", async () => {
  for (const change of [f => { f.sourceEditor.value = "new"; f.sourceEditor.dispatchEvent(new Event("input")); },
    f => f.controls.update({ status: "idle", frame: null }), f => f.controls.dispose()]) {
    const f = fixture(), hold = gate(); f.hold(hold); f.pdf.click(); change(f); hold.resolve(); await tick();
    assert.equal(f.created.length, 0); assert.equal(f.download.hidden, true); f.controls.dispose();
  }
});
test("one physical export at a time, even after invalidation; image loading disables admission", async t => {
  const f = fixture(), hold = gate(); t.after(() => f.controls.dispose()); f.hold(hold);
  f.pdf.click(); f.controls.invalidate(); f.html.click(); assert.equal(f.calls.length, 1);
  hold.resolve(); await tick(); assert.equal(f.created.length, 0);
  f.controls.update({ ...f.state, images: { status: "loading" } }); f.pdf.click(); assert.equal(f.calls.length, 1);
});
test("scroll reuses downloads; new geometry revokes them", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); f.pdf.click(); await tick();
  f.controls.update({ ...f.state, status: "busy" }); assert.equal(f.download.hidden, false);
  f.controls.update({ ...f.state, frame: { revision: "1", layoutRevision: "3" } });
  assert.equal(f.download.hidden, true); assert.equal(f.revoked.length, 1);
});
test("errors/diagnostics become inert text and disposal removes listeners and URLs", async () => {
  const f = fixture(); f.error(Object.assign(new Error("<script>native</script>"), { code: "EXPORT_FAILED" }));
  f.pdf.click(); await tick(); assert(f.status.textContent.includes("<script>native</script>")); assert.equal(f.created.length, 0);
  f.error(null); f.result.diagnostics = [{ severity: "warning", start: 0, end: 1, message: "Review image" }];
  f.pdf.click(); await tick(); assert(f.status.textContent.includes("Review image"));
  f.controls.dispose(); assert.equal(f.revoked.length, 1); const count = f.calls.length;
  f.pdf.click(); await tick(); assert.equal(f.calls.length, count); assert.equal(f.download.hidden, true);
});

test("applied publishing options are captured and frozen before renderer work", async t => {
  const settings = { title: "Current title", lang: "de-DE", toc: true, tocDepth: 3, baseFontSize: 13, pageNumbers: false };
  const f = fixture(() => settings), hold = gate(); t.after(() => f.controls.dispose()); f.hold(hold);
  f.pdf.click(); settings.title = "Later title"; settings.baseFontSize = 18;
  assert.equal(f.calls[0].options.title, "Current title"); assert.equal(f.calls[0].options.baseFontSize, 13);
  assert.equal(f.calls[0].options.pageNumbers, false); assert.equal(Object.isFrozen(f.calls[0].options), true);
  assert.equal(f.calls[0].options.metadataEpochSeconds, 0);
  hold.resolve(); await tick(); assert.equal(f.created.length, 1);
});
test("HTML receives its own options, not PDF-only typography", async t => {
  const f = fixture(format => format === "html" ? { darkMode: "disabled", title: "HTML title" } : { author: "PDF author", baseFontSize: 12 });
  t.after(() => f.controls.dispose()); f.html.click(); await tick(); f.pdf.click(); await tick();
  assert.equal(f.calls[0].options.darkMode, "disabled"); assert.equal(f.calls[0].options.author, undefined);
  assert.equal(f.calls[1].options.author, "PDF author"); assert.equal(f.calls[1].options.darkMode, undefined);
});
test("unknown and unsafe publishing options fail before invoking the renderer", async () => {
  for (const options of [{ allowRawHtml: true }, { font: "serif" }, { pdfImages: [] }, { lang: "bad tag" }, { fitToPages: 0 }, { baseFontSize: NaN }]) {
    const f = fixture(() => options); f.pdf.click(); await tick();
    assert.equal(f.calls.length, 0); assert.equal(f.created.length, 0); assert.match(f.status.textContent, /INVALID_OPTIONS/);
    assert.equal(f.sourceEditor.value, "old"); f.controls.dispose();
  }
});
test("applying new settings revokes prepared downloads and rejects older pending exports", async t => {
  const f = fixture(() => ({ title: "Applied title" })); t.after(() => f.controls.dispose());
  f.pdf.click(); await tick(); f.controls.invalidate("Publishing settings changed.");
  assert.equal(f.download.hidden, true); assert.equal(f.revoked.length, 1);
  const hold = gate(); f.hold(hold); f.pdf.click(); f.controls.invalidate("New settings applied."); hold.resolve(); await tick();
  assert.equal(f.created.length, 1); assert.equal(f.status.textContent, "New settings applied.");
});
