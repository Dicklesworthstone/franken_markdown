import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import {
  BOOK_SOURCE_PROJECT_VERSION, createBookCollection, normalizeBookProject,
  normalizeBookPublication, readBookProject, readPortableBookProject,
  discardPortableBookProject, serializeBookProject,
} from "../demo/book_collection.mjs";
import { prepareBookInput } from "../book_session.mjs";
import { collection, setup } from "./book_publication_archival_helpers.mjs";

for (const profile of ["commonmark-gfm", "gfm-plus"]) {
  test(`${profile} reaches every project capture as one source-preserving transaction`, t => {
    const book = collection(t), before = book.snapshot();
    const revisions = [book.revision, book.renderConfigurationRevision, book.renderResourceRevision];
    const events = []; book.subscribe(() => events.push(book.project()));
    book.setPublication({ profile, baselineGridPt: 13, pdfA: "2b", pdfAStrict: true }, book.revision);
    const after = book.snapshot();
    for (const options of [book.options, book.project().options, after.options,
      prepareBookInput(after.files, after.options).options]) {
      assert.equal(options.profile, profile);
      assert.equal(options.baselineGridPt, 13);
      assert.equal(options.pdfAStrict, true);
    }
    assert.deepEqual([book.revision, book.renderConfigurationRevision, book.renderResourceRevision],
      revisions.map(value => value + 1));
    assert.equal(events.length, 1); assert.equal(events[0].schemaVersion, 7);
    assert.deepEqual(after.files, before.files);
    for (const key of ["page", "title", "images", "fontAssets", "includeSources"])
      assert.deepEqual(after.options[key], before.options[key]);
    book.setPublication({ profile, baselineGridPt: 13, pdfAStrict: true, pdfA: "2b" }, book.revision);
    assert.equal(events.length, 1, "identical explicit dialect does not invalidate outputs");
  });

  test(`${profile} survives source and portable saves with separate resource authorization`, async t => {
    const book = collection(t);
    book.setPublication({ profile, baselineGridPt: 13, pdfA: "2b", pdfAStrict: true,
      running: { header: { left: "A\r\nB", image: { dest: "assets/logo.png" } } } }, book.revision);
    const output = book.projectDownload();
    const parsed = await readBookProject(new File([output.blob], output.filename));
    assert.equal(parsed.schemaVersion, 7); assert.equal(parsed.options.profile, profile);
    assert.deepEqual(parsed.files, book.project().files);
    const restored = createBookCollection(); t.after(() => restored.dispose());
    restored.replaceProject(parsed);
    assert.deepEqual(restored.project(), book.project());
    assert.deepEqual(restored.images, []); assert.deepEqual(restored.fonts, []);
    assert.equal(serializeBookProject(restored.project()), serializeBookProject(book.project()));
    const portable = await book.portableDownload();
    const review = await readPortableBookProject(new File([portable.blob], portable.filename));
    t.after(() => discardPortableBookProject(review));
    assert.throws(() => restored.replacePortableProject(review, restored.revision),
      { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
    restored.replacePortableProject(review, restored.revision, { authorizeResources: true });
    assert.deepEqual(restored.snapshot(), book.snapshot());
  });
}

test("only explicit dialects require schema 7; removing one restores the minimal schema", t => {
  const book = createBookCollection(); t.after(() => book.dispose());
  assert.equal(BOOK_SOURCE_PROJECT_VERSION, 7);
  for (const options of [{}, { profile: undefined }]) {
    book.setPublication(options, book.revision);
    assert.equal(book.revision, 0); assert.equal(book.project().schemaVersion, 1);
    assert.equal(Object.hasOwn(book.options, "profile"), false);
  }
  for (const [settings, expected] of [[{ baselineGridPt: 13 }, 6], [{ pdfA: "2b" }, 5],
    [{ baseFontSize: 12 }, 4], [{}, 1]]) {
    book.setPublication({ ...settings, profile: "gfm-plus" }, book.revision);
    assert.equal(book.project().schemaVersion, 7);
    book.setPublication(settings, book.revision);
    assert.equal(book.project().schemaVersion, expected);
  }
  book.setPage({ size: "a4" }, book.revision);
  book.setPublication({ profile: "commonmark-gfm" }, book.revision);
  book.setPublication({}, book.revision);
  assert.equal(book.project().schemaVersion, 3);
});

test("invalid dialects reject the whole settings transaction without invoking accessors", t => {
  const book = collection(t);
  book.setPublication({ profile: "gfm-plus", baseFontSize: 12 }, book.revision);
  const before = book.project(), revision = book.revision;
  for (const profile of [null, false, 1, "", "GFM-PLUS", " commonmark-gfm", "commonmark", {}]) {
    assert.throws(() => book.setPublication({ profile, baseFontSize: 16 }, revision),
      { code: "INVALID_OPTIONS" });
    assert.deepEqual(book.project(), before); assert.equal(book.revision, revision);
  }
  let reads = 0;
  assert.throws(() => normalizeBookPublication({ get profile() { reads++; return "gfm-plus"; } }),
    { code: "INVALID_OPTIONS" });
  assert.equal(reads, 0);
});

test("changing dialect neither clones nor authorizes resource bytes", t => {
  const book = collection(t), before = book.snapshot(), slice = Uint8Array.prototype.slice;
  Uint8Array.prototype.slice = () => { throw Error("unexpected resource copy"); };
  try { book.setPublication({ profile: "commonmark-gfm" }, book.revision); }
  finally { Uint8Array.prototype.slice = slice; }
  assert.deepEqual(book.snapshot().options.images, before.options.images);
  assert.deepEqual(book.snapshot().options.fontAssets, before.options.fontAssets);
});

test("older schemas cannot hide a dialect and unsupported versions remain rejected", () => {
  for (const schemaVersion of [1, 2, 3, 4, 5, 6]) {
    for (const profile of ["commonmark-gfm", "gfm-plus", undefined]) {
      assert.throws(() => normalizeBookProject({ schemaVersion, files: [], options: { profile } }),
        { code: "INVALID_PROJECT", message: /schema version 7/ });
    }
  }
  const project = normalizeBookProject({ schemaVersion: 7, files: [], options: { profile: "gfm-plus" } });
  assert.equal(project.options.profile, "gfm-plus");
  for (const schemaVersion of [0, 8, 999, 6.5, "7", null])
    assert.throws(() => normalizeBookProject({ ...project, schemaVersion }), { code: "INVALID_PROJECT" });
});

test("portable project admission rejects a concealed dialect before decoding resources", async t => {
  const book = collection(t); book.setPublication({ profile: "commonmark-gfm" }, book.revision);
  const portable = await book.portableDownload(), raw = JSON.parse(await portable.blob.text());
  raw.project.schemaVersion = 6; raw.images[0].data = "not base64";
  await assert.rejects(readPortableBookProject(new File([JSON.stringify(raw)], "bad.bundle.json")),
    { code: "INVALID_PROJECT", message: /schema version 7/ });
});

test("dialect control stages, applies once, preserves other settings and discards unsaved drafts", t => {
  const h = setup(t, { baselineGridPt: 13, pdfA: "2b", pdfAStrict: true });
  const before = h.book.snapshot(), revision = h.book.revision;
  assert.equal(h.el("profile").value, "");
  h.input("profile", "commonmark-gfm");
  assert.equal(h.book.options.profile, undefined); assert.equal(h.book.revision, revision);
  h.ui.apply();
  assert.equal(h.book.options.profile, "commonmark-gfm");
  assert.equal(h.book.options.baselineGridPt, 13); assert.equal(h.book.options.pdfAStrict, true);
  assert.equal(h.book.revision, revision + 1); assert.equal(h.book.project().schemaVersion, 7);
  assert.deepEqual(h.book.snapshot().files, before.files); assert.equal(h.captures, 0);
  h.ui.apply(); assert.equal(h.book.revision, revision + 1);
  h.input("profile", "gfm-plus"); h.ui.discard();
  assert.equal(h.el("profile").value, "commonmark-gfm");
  h.input("baseFontSize", "12"); h.ui.apply();
  assert.equal(h.book.options.profile, "commonmark-gfm");
  h.input("profile", "gfm-plus"); h.ui.suspend(); h.ui.resume();
  assert.equal(h.el("profile").value, "commonmark-gfm");
  h.input("profile", ""); h.ui.apply();
  assert.equal(h.book.options.profile, undefined); assert.equal(h.book.project().schemaVersion, 6);
});

test("busy or stale dialect drafts cannot overwrite the applied book", t => {
  const h = setup(t, { profile: "gfm-plus" });
  h.input("profile", "commonmark-gfm"); h.busy(true);
  assert.throws(() => h.ui.apply(), { code: "BOOK_BUSY" });
  assert.equal(h.book.options.profile, "gfm-plus"); h.busy(false);
  h.book.setPage({ size: "letter" }, h.book.revision);
  assert.throws(() => h.ui.apply(), { code: "STALE_SOURCE" });
  assert.equal(h.book.options.profile, "gfm-plus");
  h.ui.discard(); h.input("profile", "commonmark-gfm"); h.ui.apply();
  assert.equal(h.book.options.profile, "commonmark-gfm");
});
