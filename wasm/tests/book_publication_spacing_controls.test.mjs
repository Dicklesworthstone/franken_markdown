import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { BOOK_PUBLICATION_FIELDS, normalizeBookPublication, readBookProject,
  readPortableBookProject, discardPortableBookProject } from "../demo/book_collection.mjs";
import { createBookPublicationPanel } from "../demo/book_pdf_controls.mjs";
import { prepareBookInput } from "../book_session.mjs";
import { setup } from "./book_publication_archival_helpers.mjs";

// Production UI, collection, admission and project codecs. The shared DOM
// adapter does not render a PDF or claim geometry/conformance validation.
const retained = "theme-spacing,line-height=2,table-padding-x=1.25,table-padding-y=0,footnote-backlinks,homogeneous";
const tokens = h => (h.book.options.typography ?? "").split(",").filter(Boolean);
const publication = book => Object.fromEntries(BOOK_PUBLICATION_FIELDS
  .filter(key => book.options[key] !== undefined).map(key => [key, book.options[key]]));

test("footnote continuation checkbox applies atomically and survives source-project reopening", async t => {
  const h = setup(t, { typography: "homogeneous,split-footnotes,footnote-backlinks" });
  const revision = h.book.revision;
  assert.equal(h.el("type-split-footnotes").checked, true);
  h.input("type-split-footnotes", false);
  assert.equal(h.book.revision, revision);
  h.ui.apply();
  assert.equal(h.book.options.typography, "homogeneous,footnote-backlinks");
  h.input("type-split-footnotes", true); h.ui.discard();
  assert.equal(h.el("type-split-footnotes").checked, false);
  h.input("type-split-footnotes", true); h.ui.apply();
  const before = h.book.project(), output = h.book.projectDownload();
  h.book.replaceProject(await readBookProject(new File([output.blob], output.filename)));
  assert.deepEqual(h.book.project(), before);
  assert.equal(h.el("type-split-footnotes").checked, true);
  const captured = prepareBookInput(h.book.snapshot().files, h.book.options);
  assert(captured.options.typography.split(",").includes("split-footnotes"));
  h.busy(true); assert.equal(h.el("type-split-footnotes").disabled, true); h.busy(false);
  h.ui.defaults(); assert.equal(h.el("type-split-footnotes").checked, false);
  h.ui.discard(); assert.equal(h.el("type-split-footnotes").checked, true);
  assert.equal(h.captures, 0);
});

test("spacing and return-navigation controls expose the applied native values", t => {
  const h = setup(t, { typography: retained }), revision = h.book.revision;
  assert.equal(createBookPublicationPanel(h.root), h.el("settings"));
  assert.equal(h.el("type-theme-spacing").checked, true);
  assert.equal(h.el("type-footnote-backlinks").checked, true);
  for (const [key, value] of [["line-height", "2"], ["table-padding-x", "1.25"], ["table-padding-y", "0"]]) {
    assert.equal(h.el(`type-${key}`).value, value);
    assert.equal(h.el(`type-${key}`).type, "text", "invalid input must remain visible, not become a default");
    assert.match(h.el(`type-${key}`).getAttribute("aria-describedby"), /spacing-help/);
  }
  assert.match(h.el("spacing-help").textContent, /Each override enables theme spacing/);
  assert.equal(h.ui.apply(), revision);
  assert.equal(h.book.options.typography, retained);
  assert.equal(h.captures, 0);
});

test("editing Pareto preserves every imported spacing and footnote token", t => {
  const h = setup(t, { typography: retained }), before = h.book.snapshot();
  const revision = h.book.revision, events = [];
  h.book.subscribe(() => events.push(h.book.snapshot()));
  h.input("type-pareto", true);
  assert.equal(h.book.revision, revision);
  assert.deepEqual(h.book.snapshot(), before);
  h.ui.apply();
  assert.equal(events.length, 1);
  assert.equal(h.book.revision, revision + 1);
  assert(tokens(h).includes("pareto"));
  assert.equal(tokens(h).filter(token => token !== "pareto").join(","), retained);
  assert.deepEqual(h.book.snapshot().files, before.files);
  for (const key of ["page", "title", "images", "fontAssets", "includeSources"])
    assert.deepEqual(h.book.snapshot().options[key], before.options[key]);
  const input = h.book.snapshot();
  assert.equal(prepareBookInput(input.files, input.options).options.typography, h.book.options.typography);
  assert.equal(h.captures, 0);
  h.input("type-pareto", false); h.ui.apply();
  assert.equal(h.book.options.typography, retained);
});

test("unrelated edits preserve exact admitted duplicate tokens and microtype precedence", t => {
  const initial = normalizeBookPublication({
    typography: "line-height=1.5, expansion, line-height=2e0,protrusion,table-padding-y=0,table-padding-y=.5,homogeneous,homogeneous",
    running: { footer: { left: "Exact\r\nfooter\rtext" } },
  });
  const h = setup(t, initial), revision = h.book.revision;
  assert.equal(h.el("type-line-height").value, "2", "Rust applies the last value");
  assert.equal(h.el("type-table-padding-y").value, "0.5");
  assert.equal(h.el("type-theme-spacing").checked, true, "numeric overrides enable spacing even without its flag");
  assert.equal(h.ui.apply(), revision);
  h.input("baseFontSize", "12"); h.ui.apply();
  assert.equal(h.book.options.typography, initial.typography);
  h.input("type-footnote-backlinks", true); h.ui.apply();
  assert.equal(tokens(h).filter(token => token !== "footnote-backlinks").join(","), initial.typography);
  assert.equal(h.book.options.running.footer.left, initial.running.footer.left);
  h.input("type-line-height", "2.25"); h.ui.apply();
  assert.deepEqual(tokens(h).filter(token => token.startsWith("line-height=")), ["line-height=2.25"]);
  assert.deepEqual(tokens(h).filter(token => token === "expansion" || token === "protrusion"), ["expansion", "protrusion"]);
  assert.deepEqual(tokens(h).filter(token => token.startsWith("table-padding-y=")), ["table-padding-y=0", "table-padding-y=0.5"]);
  assert.equal(tokens(h).filter(token => token === "homogeneous").length, 2);
});

test("every spacing value and footnote flag can be authored in one atomic profile", t => {
  const h = setup(t), revision = h.book.revision, before = h.book.snapshot(), events = [];
  h.book.subscribe(() => events.push(h.book.project()));
  h.input("type-line-height", "2e0");
  assert.equal(h.el("type-theme-spacing").checked, true, "entering an override shows its effective spacing mode");
  h.input("type-table-padding-x", "+1.25"); h.input("type-table-padding-y", "0");
  h.input("type-footnote-backlinks", true);
  assert.equal(h.book.revision, revision);
  h.ui.apply();
  assert.equal(events.length, 1); assert.equal(events[0].schemaVersion, 4, "existing typography tokens need no schema extension");
  assert.deepEqual(new Set(tokens(h)), new Set(["theme-spacing", "footnote-backlinks", "line-height=2", "table-padding-x=1.25", "table-padding-y=0"]));
  assert.deepEqual(h.book.snapshot().files, before.files);
  assert.deepEqual(h.book.snapshot().options.includeSources, before.options.includeSources); assert.equal(h.captures, 0);
  const applied = h.book.revision; assert.equal(h.ui.apply(), applied);
  h.input("type-footnote-backlinks", false); h.ui.apply();
  assert(!tokens(h).includes("footnote-backlinks"));
  assert(tokens(h).includes("table-padding-y=0"));
});

test("blank overrides restore theme values and spacing can be explicitly disabled", t => {
  const h = setup(t, { typography: "line-height=2,table-padding-x=0,footnote-backlinks" });
  h.input("type-line-height", ""); h.ui.apply();
  assert(!tokens(h).some(token => token.startsWith("line-height=")));
  assert.equal(h.el("type-theme-spacing").checked, true);
  h.input("type-table-padding-x", " "); h.ui.apply();
  assert.deepEqual(new Set(tokens(h)), new Set(["theme-spacing", "footnote-backlinks"]));
  h.input("type-theme-spacing", false); h.ui.apply();
  assert.equal(h.book.options.typography, "footnote-backlinks");
  h.input("type-footnote-backlinks", false); h.ui.apply();
  assert.equal(h.book.options.typography, undefined);
});

test("turning spacing off with overrides refuses the draft without erasing values", t => {
  const h = setup(t, { typography: retained }), before = h.book.project();
  h.input("type-theme-spacing", false);
  assert.throws(() => h.ui.apply(), { code: "INVALID_OPTIONS", message: /Clear.*overrides/ });
  assert.deepEqual(h.book.project(), before);
  assert.equal(h.el("type-line-height").value, "2"); assert.equal(h.el("type-table-padding-y").value, "0");
  for (const key of ["line-height", "table-padding-x", "table-padding-y"]) h.input(`type-${key}`, "");
  h.ui.apply();
  assert.equal(h.book.options.typography, "footnote-backlinks,homogeneous");
});

test("invalid spacing stays in the draft and is rejected at native host precision", t => {
  const h = setup(t, { typography: retained }), before = h.book.project();
  const bad = [["line-height", "0"], ["line-height", "4.000000000000001"], ["line-height", "NaN"],
    ["line-height", "0x2"], ["line-height", "2,pareto"], ["line-height", "1e309"],
    ["table-padding-x", "-0.01"], ["table-padding-y", "8.000000000000002"],
    ["table-padding-y", "Infinity"], ["table-padding-x", "1".repeat(65)]];
  for (const [key, value] of bad) {
    h.ui.discard(); h.input(`type-${key}`, value);
    assert.throws(() => h.ui.apply(), { code: "INVALID_OPTIONS" });
    assert.deepEqual(h.book.project(), before); assert.equal(h.el(`type-${key}`).value, value);
    assert.match(h.el("status").textContent, /unchanged/);
  }
  h.ui.discard();
  for (const [key, value] of [["line-height", "1"], ["table-padding-x", "8"], ["table-padding-y", "-0"]]) h.input(`type-${key}`, value);
  h.ui.apply();
  assert(tokens(h).includes("line-height=1")); assert(tokens(h).includes("table-padding-x=8")); assert(tokens(h).includes("table-padding-y=0"));
  h.input("type-line-height", "4"); h.ui.apply(); assert(tokens(h).includes("line-height=4"));
});

test("native defaults clear all typography even when original algorithm switches were off", t => {
  const h = setup(t, { typography: "theme-spacing,line-height=2,table-padding-x=1.25,table-padding-y=0,footnote-backlinks" });
  const before = h.book.snapshot(), revision = h.book.revision;
  h.ui.defaults();
  assert.equal(h.book.revision, revision); assert.equal(h.book.options.typography, before.options.typography);
  assert.equal(h.el("type-theme-spacing").checked, false); assert.equal(h.el("type-footnote-backlinks").checked, false);
  assert.equal(h.el("type-line-height").value, "");
  h.ui.apply();
  assert.deepEqual(publication(h.book), {}); assert.equal(h.book.project().schemaVersion, 3);
  const after = h.book.snapshot(); assert.deepEqual(after.files, before.files);
  for (const key of ["page", "title", "images", "fontAssets", "includeSources"]) assert.deepEqual(after.options[key], before.options[key]);
});

test("new edits after Defaults use the empty profile, while Discard restores saved tokens", t => {
  const h = setup(t, { typography: retained });
  h.ui.defaults(); h.input("type-pareto", true); h.ui.apply();
  assert.equal(h.book.options.typography, "pareto");
  h.book.setPublication({ typography: retained }, h.book.revision);
  h.ui.defaults(); h.input("type-line-height", "1.5"); h.ui.discard();
  assert.equal(h.el("type-line-height").value, "2");
  assert.equal(h.el("type-footnote-backlinks").checked, true);
  const revision = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, revision);
  assert.equal(h.book.options.typography, retained);
});

test("new spacing controls keep composition and source-busy transactions atomic", t => {
  const h = setup(t, { typography: retained }), before = h.book.project();
  h.el("type-line-height").emit("compositionstart");
  assert.equal(h.el("type-line-height").disabled, false);
  assert.equal(h.el("type-footnote-backlinks").disabled, true);
  h.input("type-line-height", "2.5");
  for (const action of [h.ui.apply, h.ui.defaults, h.ui.discard]) assert.throws(action, { code: "BOOK_BUSY" });
  assert.deepEqual(h.book.project(), before);
  h.el("type-line-height").emit("compositionend");
  h.busy(true); assert.throws(() => h.ui.apply(), { code: "BOOK_BUSY" });
  h.busy(false); h.ui.apply(); assert(tokens(h).includes("line-height=2.5"));
  assert.equal(h.captures, 0);
});

test("stale drafts and suspension never overwrite recovered spacing settings", t => {
  const h = setup(t, { typography: retained });
  h.input("type-table-padding-y", "2", null);
  h.book.setPublication({ typography: "line-height=3,footnote-backlinks" }, h.book.revision);
  assert.equal(h.el("apply").disabled, true);
  assert.throws(() => h.ui.apply(), { code: "STALE_SOURCE" });
  assert.equal(h.el("type-table-padding-y").value, "2");
  h.ui.discard(); assert.equal(h.el("type-line-height").value, "3"); assert.equal(h.el("type-table-padding-y").value, "");
  h.ui.defaults(); h.ui.suspend(); h.ui.resume();
  assert.equal(h.el("type-line-height").value, "3"); assert.equal(h.el("type-footnote-backlinks").checked, true);
  const revision = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, revision);
});

for (const portable of [false, true]) test(`${portable ? "portable" : "source-only"} projects restore spacing controls and exact admitted typography`, async t => {
  const h = setup(t, { typography: retained, running: { footer: { left: "Saved\r\nfooter" } } });
  h.input("type-pareto", true); h.input("type-table-padding-x", "1.75"); h.ui.apply();
  const before = h.book.snapshot(), project = h.book.project(), typography = h.book.options.typography;
  assert.equal(project.schemaVersion, 4);
  if (portable) {
    const output = await h.book.portableDownload();
    const review = await readPortableBookProject(new File([output.blob], output.filename));
    t.after(() => discardPortableBookProject(review));
    assert.throws(() => h.book.replacePortableProject(review, h.book.revision), { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
    h.book.replacePortableProject(review, h.book.revision, { authorizeResources: true });
    assert.deepEqual(h.book.snapshot(), before);
  } else {
    const output = h.book.projectDownload();
    h.book.replaceProject(await readBookProject(new File([output.blob], output.filename)));
    assert.deepEqual(h.book.images, []); assert.deepEqual(h.book.fonts, []);
  }
  assert.deepEqual(h.book.project(), project);
  assert.equal(h.el("type-line-height").value, "2"); assert.equal(h.el("type-table-padding-x").value, "1.75");
  assert.equal(h.el("type-table-padding-y").value, "0"); assert.equal(h.el("type-footnote-backlinks").checked, true);
  const revision = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, revision);
  assert.equal(h.book.options.typography, typography);
  assert.equal(h.book.options.running.footer.left, "Saved\r\nfooter");
  assert.deepEqual(h.book.snapshot().files, before.files); assert.equal(h.captures, 0);
});
