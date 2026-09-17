import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { documentName, documentSnapshot, readMarkdownFile, markdownDownload, createSourceControls } from "../demo/flow_document.mjs";
const code = expected => error => error.code === expected;
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { resolve, promise }; };
class Element extends EventTarget {
  value = ""; disabled = false; hidden = false; textContent = "";
  removeAttribute(name) { delete this[name]; }
}
function controls(normalizeNewlines = false) {
  const sourceEditor = new Element(), filename = new Element(), open = new Element(), prepare = new Element(), download = new Element(), status = new Element();
  sourceEditor.value = "# Keep me"; filename.value = "old.md";
  if (normalizeNewlines) {
    let value = sourceEditor.value;
    Object.defineProperty(sourceEditor, "value", { get: () => value, set: next => { value = next.replace(/\r\n?/g, "\n"); } });
  }
  const urls = [], revoked = [], replacements = []; let confirmation = () => true;
  const api = createSourceControls({ sourceEditor, filename, open, prepare, download, status,
    onReplace: value => replacements.push(value), confirmReplace: value => confirmation(value),
    urls: { createObjectURL(blob) { urls.push(blob); return `blob:${urls.length}`; }, revokeObjectURL(url) { revoked.push(url); } } });
  return { api, sourceEditor, filename, open, prepare, download, status, urls, revoked, replacements, confirm(fn) { confirmation = fn; } };
}
test("valid UTF-8 round-trips BOM, CRLF, emoji and decomposed Unicode without normalization", async () => {
  const source = "\ufeff# Café\r\n\r\n😀 e\u0301\r\n";
  const original = new TextEncoder().encode(source);
  const doc = await readMarkdownFile(new File([original], "report.md"));
  assert.equal(doc.source, source);
  assert.deepEqual(new Uint8Array(await markdownDownload(doc).blob.arrayBuffer()), original);
});
test("malformed and oversized files reject without lossy decoding or starting an oversized read", async () => {
  await assert.rejects(readMarkdownFile(new File([new Uint8Array([0xc3, 0x28])], "bad.md")), code("INVALID_UNICODE"));
  let reads = 0;
  class LargeFile extends File { async arrayBuffer() { reads++; return super.arrayBuffer(); } }
  await assert.rejects(readMarkdownFile(new LargeFile([new Uint8Array(4 * 1024 * 1024 + 1)], "large.md")), code("BUDGET_EXCEEDED"));
  assert.equal(reads, 0);
  await assert.rejects(readMarkdownFile(new Blob(["# text"])), code("INVALID_FILE"));
});
test("filenames are bounded basenames and source downloads never use an HTML extension", () => {
  for (const name of ["", "../secret", "C:\\x.md", "a/b", "a\n.md", ".", "..", "x.", "x ", "é".repeat(121)]) assert.throws(() => documentName(name));
  assert.equal(markdownDownload({ filename: "page.html", source: "<script>hi</script>" }).filename, "page.html.md");
  assert.equal(markdownDownload({ filename: "REPORT.MD", source: "" }).filename, "REPORT.MD");
  assert.throws(() => documentSnapshot({ filename: "a.md", source: "\ud800" }), code("INVALID_UNICODE"));
});
test("snapshots strip non-source state rather than persisting assets or renderer authority", () => {
  assert.deepEqual(documentSnapshot({ filename: "a.md", source: "x", imageGrants: ["secret"], worker: {} }), { filename: "a.md", source: "x" });
});
test("opening a file replaces source only after confirmation and invokes grant revocation", async t => {
  const f = controls(); t.after(() => f.api.dispose()); let inputs = 0;
  f.sourceEditor.addEventListener("input", () => { assert.equal(f.replacements.length, 1); inputs++; });
  assert.equal(await f.api.openFile(new File(["# New"], "new.md")), true);
  assert.equal(f.sourceEditor.value, "# New"); assert.equal(f.filename.value, "new.md"); assert.equal(inputs, 1);
});
test("declined open and decode failure leave the original source, name and image authority intact", async t => {
  const f = controls(); t.after(() => f.api.dispose()); f.confirm(() => false);
  assert.equal(await f.api.openFile(new File(["new"], "new.md")), false);
  await assert.rejects(f.api.openFile(new File([new Uint8Array([0xff])], "bad.md")));
  assert.equal(f.sourceEditor.value, "# Keep me"); assert.equal(f.filename.value, "old.md"); assert.equal(f.replacements.length, 0);
});
test("typing or programmatic renaming during a physical file read prevents stale replacement", async t => {
  for (const change of [f => { f.sourceEditor.value = "typed"; f.sourceEditor.dispatchEvent(new Event("input")); }, f => { f.filename.value = "renamed.md"; }]) {
    const f = controls(), hold = gate(); t.after(() => f.api.dispose());
    class SlowFile extends File { async arrayBuffer() { await hold.promise; return super.arrayBuffer(); } }
    const pending = f.api.openFile(new SlowFile(["late"], "late.md"));
    await assert.rejects(f.api.openFile(new File(["other"], "other.md")), code("DOCUMENT_BUSY"));
    change(f); hold.resolve(); await assert.rejects(pending, code("STALE_SOURCE")); assert.equal(f.replacements.length, 0);
  }
});
test("confirmation races and disposal cannot overwrite an editor", async t => {
  for (const change of [f => { f.sourceEditor.value = "edited"; }, f => f.api.dispose()]) {
    const f = controls(), entered = gate(), hold = gate(); t.after(() => f.api.dispose());
    f.confirm(() => { entered.resolve(); return hold.promise; });
    const pending = f.api.openFile(new File(["new"], "new.md"));
    await entered.promise; change(f); hold.resolve(true); await assert.rejects(pending); assert.equal(f.replacements.length, 0);
  }
});
test("source downloads need no renderer and are invalidated on edit, rename or replacement", async t => {
  const f = controls(); t.after(() => f.api.dispose());
  f.api.prepareDownload(); assert.equal(await f.urls[0].text(), "# Keep me"); assert.equal(f.download.hidden, false);
  f.sourceEditor.dispatchEvent(new Event("input")); assert.equal(f.download.hidden, true); assert.deepEqual(f.revoked, ["blob:1"]);
  f.api.prepareDownload(); f.filename.value = "changed.md";
  const click = new Event("click", { cancelable: true }); f.download.dispatchEvent(click);
  assert.equal(click.defaultPrevented, true); assert.equal(f.download.hidden, true);
  f.api.prepareDownload(); f.api.replace({ filename: "new.md", source: "new" }); assert.equal(f.download.hidden, true);
});
test("disposal revokes the sole URL and removes element listeners", async () => {
  const f = controls(); f.api.prepareDownload(); f.api.dispose(); f.api.dispose();
  assert.deepEqual(f.revoked, ["blob:1"]); assert.equal(f.download.hidden, true);
  f.prepare.dispatchEvent(new Event("click")); assert.equal(f.urls.length, 1);
  assert.throws(() => f.api.snapshot(), code("SESSION_DISPOSED"));
});
test("untouched imports preserve original mixed line endings despite textarea API normalization", async t => {
  const f = controls(true); t.after(() => f.api.dispose());
  const original = "\ufeff# A\r\nB\rC\n";
  await f.api.openFile(new File([original], "mixed.md"));
  assert.equal(f.sourceEditor.value, "\ufeff# A\nB\nC\n"); assert.equal(f.api.snapshot().source, original);
  f.api.prepareDownload();
  assert.deepEqual(new Uint8Array(await f.urls[0].arrayBuffer()), new TextEncoder().encode(original));
  const click = new Event("click", { cancelable: true }); f.download.dispatchEvent(click); assert.equal(click.defaultPrevented, false);
  f.sourceEditor.value += "edited"; f.sourceEditor.dispatchEvent(new Event("input"));
  assert.equal(f.api.snapshot().source, f.sourceEditor.value);
});

test("source byte preservation can survive controller recreation without replacing the editor", () => {
  const sourceEditor = new Element(), filename = new Element(), open = new Element(), prepare = new Element(), download = new Element(), status = new Element();
  sourceEditor.value = "a\nb\n"; filename.value = "a.md";
  const options = { sourceEditor, filename, open, prepare, download, status, onReplace() { assert.fail("not a replacement"); },
    initialDocument: { source: "a\r\nb\r", filename: "a.md" } };
  const api = createSourceControls(options);
  assert.equal(api.snapshot().source, "a\r\nb\r"); api.dispose();
  assert.throws(() => createSourceControls({ ...options, initialDocument: { source: "stale", filename: "a.md" } }), code("STALE_SOURCE"));
});
