import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import {
  createBookCollection, normalizeBookPublication, normalizeBookProject,
  readBookProject, readPortableBookProject, discardPortableBookProject,
} from "../demo/book_collection.mjs";
import { createBookBindings, prepareBookInput } from "../book_session.mjs";
import { createBookCollectionCapture } from "../book_worker.mjs";
import { collection, setup } from "./book_publication_archival_helpers.mjs";
import { nativeBookFixture } from "./book_pdfa_fixture.mjs";

// Production collection, project codecs, profile controls and native adapter.
// DOM and native exports below are explicit boundary doubles, not PDF proof.
test("body grid reaches every book capture without touching source or resources", t => {
  const book = collection(t), before = book.snapshot();
  const revision = book.revision, configuration = book.renderConfigurationRevision, events = [];
  book.subscribe(() => events.push(book.project()));
  book.setPublication({ baselineGridPt: 13.1254, typography: "strict-pagination,pareto" }, revision);
  const after = book.snapshot();
  for (const view of [book.options, book.project().options, after.options,
    prepareBookInput(after.files, after.options).options]) {
    assert.equal(view.baselineGridPt, 13.1254, "native renderer owns the single milli-point rounding");
    assert.equal(view.typography, "strict-pagination,pareto");
  }
  assert.equal(book.revision, revision + 1); assert.equal(book.renderConfigurationRevision, configuration + 1);
  assert.equal(events.length, 1); assert.equal(events[0].schemaVersion, 6);
  assert.deepEqual(after.files, before.files);
  for (const key of ["page", "title", "images", "fontAssets", "includeSources"])
    assert.deepEqual(after.options[key], before.options[key]);
  book.setPublication({ typography: "strict-pagination,pareto", baselineGridPt: 13.1254 }, book.revision);
  assert.equal(events.length, 1, "an unchanged grid does not invalidate prepared output");
});

test("disabled grids retain existing schemas and can coexist with a page-count target", t => {
  const book = createBookCollection(); t.after(() => book.dispose());
  for (const baselineGridPt of [undefined, 0]) {
    book.setPublication({ baselineGridPt }, book.revision);
    assert.equal(book.revision, 0); assert.equal(book.project().schemaVersion, 1);
    assert.equal(Object.hasOwn(book.options, "baselineGridPt"), false);
  }
  book.setPublication({ baselineGridPt: 14, pdfA: "2b", pdfAStrict: true }, book.revision);
  assert.equal(book.project().schemaVersion, 6);
  book.setPublication({ baselineGridPt: 0, pdfA: "2b", pdfAStrict: true }, book.revision);
  assert.equal(book.project().schemaVersion, 5); assert.equal(book.options.pdfAStrict, true);
  assert.deepEqual(normalizeBookPublication({ baselineGridPt: 0, fitToPages: 3 }), { fitToPages: 3 });
});

test("invalid grids and page-fitting conflicts reject the entire profile before mutation", t => {
  const book = collection(t); book.setPublication({ baselineGridPt: 12, baseFontSize: 11 }, book.revision);
  const before = book.project(), revision = book.revision;
  for (const baselineGridPt of [null, "12", NaN, Infinity, -1, 0.5, 144.001]) {
    assert.throws(() => book.setPublication({ baselineGridPt, baseFontSize: 13 }, revision), { code: "INVALID_OPTIONS" });
    assert.deepEqual(book.project(), before); assert.equal(book.revision, revision);
  }
  assert.throws(() => book.setPublication({ baselineGridPt: 12, fitToPages: 2 }, revision),
    { code: "INVALID_OPTIONS", message: /pdf_baseline_grid_incompatible/ });
  assert.deepEqual(book.project(), before);
  let reads = 0;
  assert.throws(() => book.setPublication({ get baselineGridPt() { reads++; return 12; } }, revision), { code: "INVALID_OPTIONS" });
  assert.equal(reads, 0);
  for (const baselineGridPt of [1, 144])
    assert.equal(normalizeBookPublication({ baselineGridPt }).baselineGridPt, baselineGridPt);
});

test("grid-bearing projects require version 6 even when a concealed grid spells disabled", () => {
  for (const schemaVersion of [1, 2, 3, 4, 5]) {
    for (const baselineGridPt of [14, 0, undefined])
      assert.throws(() => normalizeBookProject({ schemaVersion, files: [], options: { baselineGridPt } }),
        { code: "INVALID_PROJECT", message: /schema version 6/ });
  }
  const project = normalizeBookProject({ schemaVersion: 6, files: [],
    options: { baselineGridPt: 14, pdfA: "2b", pdfAStrict: true } });
  assert.equal(project.options.baselineGridPt, 14); assert.equal(project.options.pdfAStrict, true);
  assert.throws(() => normalizeBookProject({ ...project, schemaVersion: 7 }), { code: "INVALID_PROJECT" });
});

test("source-only and portable project round trips preserve grids and explicit resource authority", async t => {
  const book = collection(t); book.setPublication({ baselineGridPt: 13.125, typography: "strict-pagination" }, book.revision);
  const output = book.projectDownload(), parsed = await readBookProject(new File([output.blob], output.filename));
  assert.equal(parsed.schemaVersion, 6); assert.equal(parsed.options.baselineGridPt, 13.125);
  const restored = createBookCollection(); t.after(() => restored.dispose());
  restored.replaceProject(parsed);
  assert.deepEqual(restored.project(), book.project());
  assert.deepEqual(restored.images, []); assert.deepEqual(restored.fonts, []);
  const portable = await book.portableDownload();
  const review = await readPortableBookProject(new File([portable.blob], portable.filename));
  t.after(() => discardPortableBookProject(review));
  assert.throws(() => restored.replacePortableProject(review, restored.revision), { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
  restored.replacePortableProject(review, restored.revision, { authorizeResources: true });
  assert.deepEqual(restored.snapshot(), book.snapshot());
});

test("workbench captures configure the native grid and fail explicitly with an older engine", async t => {
  const book = collection(t); book.setPublication({ baselineGridPt: 13.125, typography: "strict-pagination" }, book.revision);
  const { NativeBook, state } = nativeBookFixture();
  class GridBook extends NativeBook {
    setBaselineGridPt(value) { this.settings.baselineGridPt = value; }
  }
  const modern = createBookBindings(async () => GridBook), input = book.snapshot();
  const native = await modern.createBook(input.files, input.options);
  t.after(() => native.dispose());
  assert.equal(state.instances[0].settings.baselineGridPt, 13.125);
  assert.equal(state.instances[0].settings.pdf[0], "strict-pagination");
  const old = createBookBindings(async () => NativeBook), constructions = state.created;
  await assert.rejects(old.createBook(input.files, input.options), { code: "UNSUPPORTED_BOOK_OPTIONS" });
  assert.equal(state.created, constructions, "unsupported grid is refused before constructing a native book");
});

test("source edits retain the grid while changed grid settings force a complete worker capture", async t => {
  const book = collection(t); book.setPublication({ baselineGridPt: 12 }, book.revision);
  let revision = 0; const calls = [];
  const worker = { get retainedInputRevision() { return revision || null; }, supportsSourceSet: true,
    async render(files, format, options) { calls.push(["full", files, options]); return { retainedInputRevision: ++revision }; },
    async renderSources(files, format, options) { calls.push(["sources", files, options]); return { retainedInputRevision: ++revision }; },
    async renderSourceSet(files, format, options) { calls.push(["set", files, options]); return { retainedInputRevision: ++revision }; },
  };
  const capture = createBookCollectionCapture(book, worker);
  await capture().render("pdf");
  book.edit(0, "guide/one.md", "Changed source");
  assert.equal(capture().options.baselineGridPt, 12); await capture().render("pdf");
  book.setPublication({ baselineGridPt: 14 }, book.revision); await capture().render("pdf");
  assert.deepEqual(calls.map(call => call[0]), ["full", "sources", "full"]);
  assert.equal(calls[0][2].baselineGridPt, 12); assert.equal(calls[2][2].baselineGridPt, 14);
  assert.deepEqual(Object.keys(calls[1][2]).sort(), ["expectedRevision", "includeSources"]);
});

test("grid and strict-pagination controls remain drafts until a single atomic Apply", t => {
  const h = setup(t), before = h.book.snapshot(), sources = h.book.files, revision = h.book.revision, events = [];
  h.book.subscribe(() => events.push(h.book.project()));
  assert.equal(h.el("baselineGridPt")?.type, "text", "expose the native baseline grid without input sanitization");
  assert.match(h.el("grid-help").textContent, /page-count target/);
  h.input("baselineGridPt", "13.1254"); h.input("type-strict-pagination", true);
  assert.equal(h.book.revision, revision); assert.deepEqual(h.book.snapshot(), before);
  h.ui.apply();
  assert.equal(events.length, 1); assert.equal(events[0].schemaVersion, 6);
  assert.equal(h.book.options.baselineGridPt, 13.1254); assert.equal(h.book.options.typography, "strict-pagination");
  assert.deepEqual(h.book.files, sources); assert.equal(h.captures, 0);
  const applied = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, applied);
});

test("changing another typography flag never silently disables imported strict pagination", t => {
  const h = setup(t, { typography: "strict-pagination" });
  h.input("type-pareto", true); h.ui.apply();
  assert.equal(h.book.options.typography, "pareto,strict-pagination");
  assert.equal(h.el("type-strict-pagination").checked, true);
  h.input("type-strict-pagination", false); h.ui.apply();
  assert.equal(h.book.options.typography, "pareto", "strict mode changes only when explicitly deselected");
});

test("grid controls preserve invalid drafts and require page fitting to be cleared before application", t => {
  const h = setup(t, { fitToPages: 2 }), before = h.book.project();
  h.input("baselineGridPt", "12");
  assert.throws(() => h.ui.apply(), { code: "INVALID_OPTIONS", message: /pdf_baseline_grid_incompatible/ });
  assert.deepEqual(h.book.project(), before); assert.equal(h.el("baselineGridPt").value, "12");
  h.input("fitToPages", ""); h.ui.apply(); assert.equal(h.book.options.baselineGridPt, 12);
  const current = h.book.project();
  for (const value of ["0.5", "145", "NaN", "-1"]) {
    h.input("baselineGridPt", value); assert.throws(() => h.ui.apply());
    assert.deepEqual(h.book.project(), current); assert.equal(h.el("baselineGridPt").value, value);
  }
  h.ui.discard(); assert.equal(h.el("baselineGridPt").value, "12");
  h.input("baselineGridPt", "0"); h.ui.apply();
  assert.equal(h.book.options.baselineGridPt, undefined); assert.equal(h.book.project().schemaVersion, 3);
});

test("composition and stale profile ownership protect the new grid and strict controls", t => {
  const h = setup(t, { baselineGridPt: 12 });
  h.el("baselineGridPt").emit("compositionstart");
  assert.equal(h.el("baselineGridPt").disabled, false); assert.equal(h.el("type-strict-pagination").disabled, true);
  h.input("baselineGridPt", "14"); assert.throws(() => h.ui.apply(), { code: "BOOK_BUSY" });
  h.el("baselineGridPt").emit("compositionend"); h.ui.apply();
  h.input("baselineGridPt", "16");
  h.book.setPublication({ baselineGridPt: 18, typography: "strict-pagination" }, h.book.revision);
  assert.throws(() => h.ui.apply(), { code: "STALE_SOURCE" });
  h.ui.discard(); assert.equal(h.el("baselineGridPt").value, "18"); assert.equal(h.el("type-strict-pagination").checked, true);
  h.input("baselineGridPt", "20"); h.ui.suspend(); h.ui.resume();
  assert.equal(h.el("baselineGridPt").value, "18");
});
