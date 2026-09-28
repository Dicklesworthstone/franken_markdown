import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookCollection } from "../demo/book_collection.mjs";
import * as publisher from "../demo/book_controls.mjs";
import { createBookPdfProof } from "../book_pdf_proof.mjs";
const { createBookControls, createBookPageControls } = publisher;
import { normalizePdfPage, pdfPageGeometry } from "../pdf_page.mjs";

// Actual publisher/collection/page validation; DOM, URL ownership and native
// renderer are explicit adapters. The tests do not claim PDF visual acceptance.
const IDS = "source-role add-include import-includes import-include-folder chapters chapter-path chapter-source add-chapter move-up move-down remove-chapter import-files import-folder open-project save-project save-chapter revoke-images image-list title author lang font dark-mode font-scale toc page-numbers export-pdf export-epub export-site cancel-export download status publish-title".split(" ");
class Element extends EventTarget {
  constructor(root, tag = "input") { super(); this.root = root; this.tagName = tag; this.children = []; this.attrs = {}; this._value = ""; this.disabled = false; this.files = []; }
  set id(value) { this._id = value; this.root.elements[value] = this; }
  get id() { return this._id; }
  set value(value) { this._value = this.tagName === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
  get value() { return this._value; }
  set innerHTML(_) { assert.fail("page setup must not parse markup"); }
  setAttribute(key, value) { this.attrs[key] = String(value); }
  getAttribute(key) { return this.attrs[key]; }
  removeAttribute(key) { delete this.attrs[key]; delete this[key]; }
  append(...nodes) { for (const node of nodes) { node.parentElement = this; this.children.push(node); } }
  replaceChildren(...nodes) { this.children = []; this.append(...nodes); }
  emit(type) { const event = new Event(type, { cancelable: true }); this.dispatchEvent(event); return event; }
  click() { if (!this.disabled) return this.emit("click"); }
  focus() {}
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
}
function fixture({ panel = true, render } = {}) {
  const root = { elements: {}, querySelector(selector) { return this.elements[selector.slice(1)] ?? null; }, createElement(tag) { return new Element(this, tag); } };
  for (const id of IDS) {
    if (id === "publish-title" && !panel) continue;
    const el = root.createElement(id === "chapter-source" ? "textarea" : "input"); el.id = id;
  }
  if (panel) root.createElement("section").append(root.elements["publish-title"]);
  const collection = createBookCollection();
  collection.append({ chapters: [{ path: "a.md", source: "\ufeff# A\r\n\r\n{{#include shared.txt}}\r" }],
    includeSources: [{ path: "shared.txt", source: "shared\r\n" }], images: [{ destination: "x.png", bytes: new Uint8Array([1, 2]) }] });
  collection.setFonts([{ slot: "body-regular", name: "a.ttf", bytes: new Uint8Array([3, 4]) }], collection.revision);
  const exports = [], blobs = new Map(), revoked = [];
  let serial = 0;
  const controls = createBookControls({ root, collection, confirm: () => true,
    urls: { createObjectURL(blob) { const key = `blob:page/${++serial}`; blobs.set(key, blob); return key; }, revokeObjectURL(key) { blobs.delete(key); revoked.push(key); } },
    worker: { cancel() {}, dispose() {}, async render(files, format, options) {
      exports.push({ files, format, options });
      if (render) return render(files, format, options);
      return { extension: format === "site" ? "zip" : format, blob: () => new Blob(["recording renderer, not PDF"]) };
    } },
  });
  const el = root.elements, field = id => el[`book-page-${id}`];
  const change = (id, value) => { field(id).value = value; field(id).emit("change"); };
  const input = (id, value) => { field(id).value = value; field(id).emit("input"); };
  const apply = () => field("apply").click();
  return { root, el, collection, controls, exports, blobs, revoked, field, change, input, apply };
}
const expected = () => normalizePdfPage({ size: { widthPt: 432, heightPt: 648 }, orientation: "landscape", margins: { topPt: 36, rightPt: 18, bottomPt: 54, leftPt: 72 } });
function draft(f) {
  f.change("size", "trade"); f.change("orientation", "landscape"); f.change("unit", "in");
  f.input("top", "0.5"); f.input("right", "0.25"); f.input("bottom", "0.75"); f.input("left", "1");
}

test("publisher installs labelled page controls without changing default book settings", () => {
  const f = fixture();
  assert.equal(f.field("size").value, "default");
  assert.equal(f.field("width").disabled, true);
  assert.equal(f.collection.options.page, undefined);
  assert.match(f.field("help").textContent, /Drafts do not affect/);
  assert.equal(f.field("status").getAttribute("role"), "status");
  const hosted = fixture({ panel: false });
  assert.equal(hosted.field("apply"), undefined);
  assert.equal(hosted.controls.captureProject().schemaVersion, 2);
});

test("applying paper and asymmetric margins captures raw editor state and preserves resources", () => {
  const f = fixture(), before = f.collection.snapshot();
  draft(f);
  f.el["chapter-source"].value = "new unsent source"; f.el.title.value = "New title";
  f.apply();
  assert.deepEqual(f.collection.options.page, expected());
  assert.equal(f.collection.files[0].source, "new unsent source");
  assert.equal(f.collection.options.title, "New title");
  assert.deepEqual(f.collection.snapshot().options.images, before.options.images);
  assert.deepEqual(f.collection.snapshot().options.fontAssets, before.options.fontAssets);
  assert.equal(f.collection.files[1].source, before.options.includeSources[0].source);
});

test("page drafts stay unapplied while captures and every publication retain committed geometry", async () => {
  const f = fixture(); draft(f); f.apply();
  const revision = f.collection.revision;
  f.change("size", "a4");
  assert.equal(f.collection.revision, revision);
  for (const format of ["pdf", "epub", "site"]) {
    await f.controls.prepare(format);
    assert.deepEqual(f.exports.at(-1).options.page, expected());
    assert.equal(f.collection.revision, revision);
  }
  assert.equal(f.controls.captureProject().schemaVersion, 3);
  assert.deepEqual(f.collection.options.page, expected());
  assert.equal(f.field("size").value, "a4");
});

test("metadata, source and chapter navigation never erase an applied page", () => {
  const f = fixture(); draft(f); f.apply();
  f.el.title.value = "title"; f.el.title.emit("input");
  f.el["chapter-source"].value = "edited"; f.el["chapter-source"].emit("input");
  f.el.chapters.value = "1"; f.el.chapters.emit("change");
  f.controls.captureProject();
  assert.deepEqual(f.collection.options.page, expected());
  assert.equal(f.controls.currentChapter, 1);
});

test("unapplied and invalid page drafts do not retire a prepared download", async () => {
  const f = fixture(); await f.controls.prepare("pdf");
  const download = f.el.download.href, revision = f.collection.revision;
  f.change("size", "custom"); f.input("width", "bad number");
  f.el["chapter-source"].value = "unsent text";
  f.apply();
  assert.match(f.field("status").textContent, /INVALID_OPTIONS/);
  assert.equal(f.collection.revision, revision);
  assert.equal(f.collection.files[0].source.startsWith("\ufeff"), true);
  assert(f.blobs.has(download));
  // Ordinary publisher stale-source checks remain active at download activation.
  assert.equal(f.el.download.click().defaultPrevented, true);
});

test("applied page changes revoke old downloads and stale in-flight results", async () => {
  let settle;
  const f = fixture({ render: () => new Promise(resolve => { settle = resolve; }) });
  const pending = f.controls.prepare("pdf");
  draft(f); f.apply();
  settle({ extension: "pdf", blob: () => new Blob(["old output"]) });
  await assert.rejects(pending, { code: "STALE_SOURCE" });
  assert.equal(f.blobs.size, 0);
  const g = fixture(); await g.controls.prepare("pdf"); const old = g.el.download.href;
  draft(g); g.apply();
  assert(g.revoked.includes(old)); assert.equal(g.el.download.hidden, true);
});

test("repeated unit changes preserve exact imported geometry and no-op revision", () => {
  const f = fixture();
  const geometry = normalizePdfPage({ size: { widthPt: 595.2755905511812, heightPt: 841.8897637795276 },
    margins: { topPt: 23.12345678901234, rightPt: 31.98765432109876, bottomPt: 42.1234567898765, leftPt: 35.7654321098765 } });
  f.collection.setPage(geometry, f.collection.revision);
  const revision = f.collection.revision;
  for (let i = 0; i < 25; i++) for (const unit of ["mm", "in", "pt"]) f.change("unit", unit);
  f.apply();
  assert.deepEqual(f.collection.options.page, geometry);
  assert.equal(f.collection.revision, revision);
});

test("unit changes reject incomplete numeric drafts without erasing fields", () => {
  const f = fixture(); f.change("size", "custom");
  f.input("left", ""); f.input("width", "6."); f.change("unit", "mm");
  assert.equal(f.field("unit").value, "pt");
  assert.equal(f.field("left").value, ""); assert.equal(f.field("width").value, "6.");
  assert.equal(f.collection.options.page, undefined);
});

test("invalid margins, numbers, orientation and silent unit changes cannot be applied", () => {
  const f = fixture(); draft(f); f.apply(); const before = f.collection.snapshot();
  for (const invalid of ["", "-1", "NaN", "Infinity", "0x48", "12px", "1,5", "1e999", "99999"]) {
    f.change("size", "custom"); f.input("left", invalid); f.apply();
    assert.deepEqual(f.collection.snapshot(), before, invalid);
    f.field("discard").click();
  }
  f.field("unit").value = "mm"; f.apply();
  assert.deepEqual(f.collection.snapshot(), before);
  f.field("discard").click(); f.field("orientation").value = "bogus"; f.apply();
  assert.deepEqual(f.collection.snapshot(), before);
});

test("source imports and recovery reload page drafts, including an identical old page", async () => {
  const f = fixture(); draft(f); f.apply(); const project = f.collection.project();
  f.change("size", "a4");
  f.controls.replaceProject(project, f.controls.checkpoint());
  assert.equal(f.field("size").value, "trade");
  const other = { schemaVersion: 3, files: [{ path: "new.md", source: "new\r\n" }], options: { page: { size: "a4" } } };
  await f.controls.importFiles([new File([JSON.stringify(other)], "book.fmdbook.json")], "project");
  assert.equal(f.field("size").value, "a4");
  assert.deepEqual(f.controls.captureProject().options.page, normalizePdfPage({ size: "a4" }));
});

test("source and portable downloads retain page setup through the actual controls", async () => {
  const f = fixture(); draft(f); f.apply();
  f.controls.prepareSource(true);
  const source = JSON.parse(await f.blobs.get(f.el.download.href).text());
  assert.equal(source.schemaVersion, 3); assert.deepEqual(source.options.page, expected());
  await f.controls.preparePortable();
  const bundle = f.blobs.get(f.el.download.href), g = fixture();
  await g.controls.importFiles([new File([bundle], "book.fmdbook.bundle.json")], "portable");
  assert.deepEqual(g.collection.snapshot(), f.collection.snapshot());
  assert.equal(g.field("size").value, "trade");
});

test("renderer-default reset restores old project versions without changing source or grants", () => {
  const f = fixture(), original = f.controls.captureProject(), assets = f.collection.snapshot().options;
  draft(f); f.apply(); f.change("size", "default");
  assert(f.collection.options.page);
  f.apply();
  assert.deepEqual(f.controls.captureProject(), original);
  assert.equal(pdfPageGeometry(f.collection.snapshot().options.page).length, 0);
  assert.deepEqual(f.collection.snapshot().options.images, assets.images);
});

test("composition and imports block page apply; suspension discards drafts and disposal detaches", async () => {
  const f = fixture(); draft(f);
  f.el["chapter-source"].emit("compositionstart");
  assert(f.field("apply").disabled); f.field("apply").emit("click");
  assert.equal(f.collection.options.page, undefined);
  f.el["chapter-source"].emit("compositionend"); assert(!f.field("apply").disabled);
  let finish;
  const file = new File(["x"], "slow.md"); file.arrayBuffer = () => new Promise(resolve => { finish = resolve; });
  const reading = f.controls.importFiles([file]);
  assert(f.field("apply").disabled); f.field("apply").emit("click");
  assert.equal(f.collection.options.page, undefined);
  finish(new TextEncoder().encode("x").buffer); await reading;
  assert(!f.field("apply").disabled);
  f.controls.suspend(); assert.equal(f.field("size").value, "default");
  f.controls.dispose(); assert(f.field("apply").disabled);
});

test("page host guards reject reentrant changes to the draft during source capture", () => {
  const f = fixture({ panel: false });
  const heading = f.root.createElement("h2"); heading.id = "publish-title";
  f.root.createElement("section").append(heading);
  let page;
  page = createBookPageControls({ root: f.root, collection: f.collection, isBusy: () => false,
    capture() { f.root.elements["book-page-left"].value = "123"; } });
  f.root.elements["book-page-size"].value = "letter";
  assert.throws(() => page.apply(), { code: "STALE_SOURCE" });
  assert.equal(f.collection.options.page, undefined);
  page.dispose(); assert.throws(() => page.apply(), { code: "SESSION_DISPOSED" });
});


test("embedding captures cannot silently discard an imported page even without the page panel", () => {
  const f = fixture({ panel: false });
  f.collection.setPage(expected(), f.collection.revision);
  const revision = f.collection.revision;
  assert.deepEqual(f.controls.captureProject().options.page, expected());
  assert.equal(f.collection.revision, revision);
});

// Result bytes exercise proof ownership/envelope checks only, not PDF validity.
const proofResult = () => ({ format: "book-pdf", extension: "pdf", mimeType: "application/pdf",
  sourceLength: 30, bytes: new TextEncoder().encode("%PDF-1.7\nrecording adapter, not a rendered PDF") });

test("actual proof session and publication capture identical complete page-configured books", async () => {
  const f = fixture(), requests = [];
  const proof = createBookPdfProof({ controls: f.controls, collection: f.collection,
    worker: { dispose() {}, async render(files, format, options) {
      requests.push({ files, format, options }); return proofResult();
    } } });
  draft(f); f.apply();
  const retained = await proof.render();
  await f.controls.prepare("pdf");
  assert.deepEqual(requests, [f.exports[0]]);
  assert.deepEqual([...pdfPageGeometry(requests[0].options.page)], [648, 432, 36, 18, 54, 72]);
  assert.equal(retained.chapters, 1);
  assert.equal(requests[0].options.includeSources.length, 1);
  assert.equal(requests[0].options.fontAssets.length, 1);
  assert.equal(requests[0].options.images.length, 1);
  f.change("size", "a4");
  assert.equal(proof.current, retained); assert.equal(await proof.render(), retained);
  assert.equal(requests.length, 1);
  f.apply();
  assert.equal(proof.current, null);
  await proof.render();
  assert.deepEqual(requests[1].options.page, f.collection.options.page);
  proof.dispose(); f.controls.dispose();
});

test("applying geometry aborts only the old proof and its late result cannot restore it", async () => {
  const f = fixture(); let settle, signal;
  const proof = createBookPdfProof({ controls: f.controls, collection: f.collection,
    worker: { dispose() {}, render(files, format, options, context) {
      signal = context.signal; return new Promise(resolve => { settle = resolve; });
    } } });
  const pending = proof.render();
  draft(f); assert.equal(signal.aborted, false);
  f.apply();
  assert.equal(signal.aborted, true);
  await assert.rejects(pending, { code: "STALE_SOURCE" });
  settle(proofResult()); await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(proof.current, null);
  // Ordinary export still owns an independent worker and sees current geometry.
  await f.controls.prepare("pdf");
  assert.deepEqual(f.exports[0].options.page, expected());
  proof.dispose(); f.controls.dispose();
});
