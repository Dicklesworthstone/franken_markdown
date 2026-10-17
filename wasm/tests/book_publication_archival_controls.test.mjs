import assert from "node:assert/strict";
import test from "node:test";
import { createBookPublicationPanel } from "../demo/book_pdf_controls.mjs";
import { setup } from "./book_publication_archival_helpers.mjs";

// Real profile controller, model and serializer; DOM/source-host are explicit
// adapters. These controls do not render or claim PDF/A certification.
test("archival choices install once with a labeled, bounded policy selection", t => {
  const h = setup(t), before = h.book.project();
  assert.equal(createBookPublicationPanel(h.root), h.el("settings"));
  assert.deepEqual(h.el("pdfPolicy").children.map(node => node.value), ["off", "2b", "2b-strict"]);
  assert.equal(h.el("pdfPolicy").value, "off");
  assert.match(h.el("pdfPolicy").getAttribute("aria-describedby"), /archival-help/);
  assert.match(h.el("archival-help").textContent, /not PDF\/A validation or certification/);
  assert.match(h.el("archival-help").textContent, /separate from checking internal book links/);
  assert.match(h.el("archival-help").textContent, /version 5/);
  assert.deepEqual(h.book.project(), before); assert.equal(h.captures, 0);
});

for (const strict of [false, true]) test(`policy stays draft until a single atomic Apply: strict=${strict}`, t => {
  const h = setup(t), before = h.book.snapshot(), revision = h.book.revision, events = [];
  h.book.subscribe(() => events.push(h.book.project()));
  h.input("pdfPolicy", strict ? "2b-strict" : "2b");
  assert.equal(h.book.revision, revision); assert.deepEqual(h.book.snapshot(), before);
  assert.match(h.el("status").textContent, /Unapplied/);
  assert.equal(h.ui.apply(), revision + 1); assert.equal(events.length, 1);
  assert.equal(events[0].schemaVersion, 5); assert.equal(h.book.options.pdfA, "2b");
  assert.equal(h.book.options.pdfAStrict, strict);
  const after = h.book.snapshot();
  for (const key of ["images", "fontAssets", "includeSources", "page", "title"])
    assert.deepEqual(after.options[key], before.options[key]);
  assert.deepEqual(after.files, before.files); assert.equal(h.captures, 0);
  assert.equal(JSON.parse(h.el("current").textContent).pdfAStrict, strict);
});

test("ordinary mode is explicit and staged, while Discard restores strict archival settings", t => {
  const h = setup(t, { pdfA: "2b", pdfAStrict: true }), revision = h.book.revision;
  h.input("pdfPolicy", "off"); assert.equal(h.book.options.pdfAStrict, true);
  h.ui.discard(); assert.equal(h.el("pdfPolicy").value, "2b-strict");
  assert.equal(h.book.revision, revision);
  h.input("pdfPolicy", "off"); h.ui.apply();
  assert.equal(h.book.options.pdfA, undefined); assert.equal(h.book.options.pdfAStrict, undefined);
  assert.equal(h.book.project().schemaVersion, 3); assert.equal(h.captures, 0);
});

test("unrelated typography and template edits preserve strictness and exact imported text", t => {
  const h = setup(t, { pdfA: "2b", pdfAStrict: true,
    running: { header: { left: "A\r\nB\rC" } } });
  const source = h.book.files;
  h.input("baseFontSize", "12"); h.ui.apply();
  assert.equal(h.book.options.pdfAStrict, true);
  assert.equal(h.book.options.running.header.left, "A\r\nB\rC");
  assert.deepEqual(h.book.files, source);
  h.input("pdfPolicy", "2b"); h.ui.apply();
  assert.equal(h.book.options.pdfAStrict, false); assert.equal(h.book.options.baseFontSize, 12);
  assert.equal(h.book.options.running.header.left, "A\r\nB\rC");
});

for (const value of ["", "2B", "3b", "corrupt"]) test(`invalid/missing selection cannot downgrade an applied archive: ${value}`, t => {
  const h = setup(t, { pdfA: "2b", pdfAStrict: true }), before = h.book.project();
  h.input("pdfPolicy", value);
  assert.throws(() => h.ui.apply(), { code: "INVALID_OPTIONS" });
  assert.deepEqual(h.book.project(), before);
  assert.match(h.el("status").textContent, /supported PDF output policy/);
  assert.equal(h.captures, 0);
});

test("native defaults are staged, then reset only the profile without touching source or grants", t => {
  const h = setup(t, { pdfA: "2b", pdfAStrict: true, baseFontSize: 14 });
  const before = h.book.snapshot(), revision = h.book.revision;
  h.ui.defaults(); assert.equal(h.el("pdfPolicy").value, "off");
  assert.equal(h.book.revision, revision); assert.equal(h.book.options.pdfA, "2b");
  h.ui.apply(); const after = h.book.snapshot();
  assert.equal(after.options.pdfA, undefined); assert.equal(after.options.baseFontSize, undefined);
  for (const key of ["page", "images", "fontAssets", "includeSources", "title"])
    assert.deepEqual(after.options[key], before.options[key]);
  assert.deepEqual(after.files, before.files); assert.equal(h.captures, 0);
});

test("silent archival draft edits are included in stale-profile fencing", t => {
  const h = setup(t); h.input("pdfPolicy", "2b-strict", null);
  h.book.configure({ ...h.book.options, title: "Concurrent change" });
  assert.equal(h.el("pdfPolicy").value, "2b-strict"); assert.equal(h.el("apply").disabled, true);
  assert.throws(() => h.ui.apply(), { code: "STALE_SOURCE" });
  assert.equal(h.book.options.pdfA, undefined);
  h.ui.discard(); assert.equal(h.el("pdfPolicy").value, "off"); assert.equal(h.el("apply").disabled, false);
});

test("source/include edits retain the policy draft and never capture or rewrite the editor", t => {
  const h = setup(t); h.input("pdfPolicy", "2b-strict");
  h.book.edit(0, "guide/one.md", "typing"); h.book.edit(2, "shared.txt", "include edit");
  assert.equal(h.el("apply").disabled, false); h.ui.apply();
  assert.equal(h.book.files[0].source, "typing"); assert.equal(h.book.files[2].source, "include edit");
  assert.equal(h.book.options.pdfAStrict, true); assert.equal(h.captures, 0);
});

test("pristine controls follow restored archival profiles; dirty controls require explicit reload", t => {
  const h = setup(t), saved = h.book.project();
  saved.schemaVersion = 5; saved.options.pdfA = "2b"; saved.options.pdfAStrict = true;
  h.book.replaceProject(saved); assert.equal(h.el("pdfPolicy").value, "2b-strict");
  assert.deepEqual(h.book.images, []); assert.deepEqual(h.book.fonts, []);
  h.input("pdfPolicy", "off"); saved.options.pdfAStrict = false;
  h.book.replaceProject(saved);
  assert.equal(h.el("pdfPolicy").value, "off"); assert.equal(h.el("apply").disabled, true);
  assert.throws(() => h.ui.apply(), { code: "STALE_SOURCE" });
  h.ui.discard(); assert.equal(h.el("pdfPolicy").value, "2b");
  const revision = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, revision);
});

test("source import/composition and suspension block policy application without downgrading", t => {
  const h = setup(t, { pdfA: "2b", pdfAStrict: true });
  h.input("pdfPolicy", "off"); h.busy(true);
  assert.equal(h.el("pdfPolicy").disabled, true);
  assert.throws(() => h.ui.apply(), { code: "BOOK_BUSY" });
  h.busy(false); h.el("header-left").emit("compositionstart");
  assert.throws(() => h.ui.apply(), { code: "BOOK_BUSY" });
  h.el("header-left").emit("compositionend"); h.ui.suspend();
  assert.equal(h.el("pdfPolicy").disabled, true); assert.equal(h.book.options.pdfAStrict, true);
  assert.throws(() => h.ui.apply(), { code: "PROFILE_CLOSED" });
  h.ui.resume(); assert.equal(h.el("pdfPolicy").value, "2b-strict");
  const revision = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, revision);
});

test("disposal removes policy event handlers and subscriptions", t => {
  const h = setup(t, { pdfA: "2b" }), revision = h.book.revision;
  h.ui.dispose(); h.ui.dispose();
  assert.equal(h.listeners.size, 0); assert.equal(h.el("pdfPolicy").disabled, true);
  h.input("pdfPolicy", "off"); h.el("apply").emit("click");
  assert.equal(h.book.revision, revision); assert.equal(h.book.options.pdfA, "2b");
  assert.throws(() => h.ui.apply(), { code: "PROFILE_CLOSED" });
});

test("observer edits during Apply cannot leave an incorrect strictness summary", t => {
  const h = setup(t); h.input("pdfPolicy", "2b-strict"); let once = false;
  h.book.subscribe(() => { if (!once) {
    once = true; h.book.setPublication({ pdfA: "2b", pdfAStrict: false }, h.book.revision);
  } });
  h.ui.apply(); assert.equal(h.book.options.pdfAStrict, false);
  assert.equal(h.el("pdfPolicy").value, "2b");
  assert.equal(JSON.parse(h.el("current").textContent).pdfAStrict, false);
  assert.match(h.el("status").textContent, /changed again/);
});
