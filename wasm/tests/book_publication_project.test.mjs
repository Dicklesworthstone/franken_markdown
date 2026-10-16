import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import {
  BOOK_PUBLICATION_FIELDS, createBookCollection, normalizeBookPublication,
  normalizeBookProject, serializeBookProject, readBookProject, readPortableBookProject,
} from "../demo/book_collection.mjs";
import { prepareBookInput } from "../book_session.mjs";

// Collection/resource ownership and production option admission, not a native
// render. Resource bytes below are explicit byte-store fixtures, not fonts/PNG.
const full = () => ({ typography: "homogeneous,antiriver,pareto,optimal-pagination,protrusion,expansion",
  baseFontSize: 12, headingScale: 1.25, tableFontSize: 9, tocDepth: 3,
  fitToPages: 40, codeLineNumbers: true, metadataEpochSeconds: 0, htmlFontFormat: "woff2",
  running: { header: { left: "{title}", right: "{author}", rule: true,
    image: { dest: "assets/logo.png", position: "left", heightPt: 18 } },
    footer: { center: "{page} / {pages}" }, skipFirstPage: true } });
const profile = options => Object.fromEntries(BOOK_PUBLICATION_FIELDS
  .filter(key => options[key] !== undefined).map(key => [key, options[key]]));
function fixture(t) {
  const book = createBookCollection();
  book.append({ chapters: [{ path: "a.md", source: "\ufeff# Title\r\n本文 😀\r\n" }],
    includeSources: [{ path: "notes.txt", source: "keep\r\nexact" }],
    images: [{ destination: "assets/logo.png", bytes: new Uint8Array([1, 2, 3]) }] });
  book.setFonts([{ slot: "body-regular", name: "fixture.ttf", bytes: new Uint8Array([4, 5]) }], book.revision);
  t.after(() => book.dispose());
  return book;
}

test("legacy/default projects keep their original minimal schema and option set", t => {
  const book = createBookCollection(); t.after(() => book.dispose());
  assert.equal(book.project().schemaVersion, 1);
  assert.deepEqual(profile(book.options), {});
  book.setPublication(undefined, 0); assert.equal(book.revision, 0);
  book.append({ chapters: [], includeSources: [{ path: "a.txt", source: "" }], images: [] });
  assert.equal(book.project().schemaVersion, 2);
  book.setPage({ size: "a4" }, book.revision); assert.equal(book.project().schemaVersion, 3);
  for (const key of BOOK_PUBLICATION_FIELDS) assert.equal(book.options[key], undefined);
});

test("complete profile reaches the existing renderer capture without changing book content", t => {
  const book = fixture(t), before = book.snapshot(), initial = book.revision;
  const config = book.renderConfigurationRevision, events = [];
  book.subscribe(() => events.push(book.project()));
  assert.equal(book.setPublication(full(), initial), initial + 1);
  assert.equal(book.revision, initial + 1); assert.equal(book.renderConfigurationRevision, config + 1);
  const after = book.snapshot(), expected = normalizeBookPublication(full());
  assert.deepEqual(profile(after.options), expected);
  assert.deepEqual(after.files, before.files);
  for (const key of ["images", "fontAssets", "includeSources", "title", "author", "font", "pageNumbers"])
    assert.deepEqual(after.options[key], before.options[key]);
  assert.equal(events.length, 1); assert.equal(events[0].schemaVersion, 4);
  assert.deepEqual(profile(prepareBookInput(after.files, after.options).options), expected);
});

test("profile changes do not clone authorized asset payloads", t => {
  const book = fixture(t), before = book.snapshot();
  const slice = Uint8Array.prototype.slice;
  Uint8Array.prototype.slice = () => { throw Error("unexpected image/font clone"); };
  try { book.setPublication(full(), book.revision); }
  finally { Uint8Array.prototype.slice = slice; }
  assert.deepEqual(book.snapshot().options.images, before.options.images);
  assert.deepEqual(book.snapshot().options.fontAssets, before.options.fontAssets);
});

test("running bands, image records and all exposed snapshots are deeply owned", t => {
  const book = fixture(t), input = full();
  book.setPublication(input, book.revision);
  input.running.header.left = "mutated"; input.running.header.image.dest = "other.png";
  input.running.footer.center = "mutated"; input.baseFontSize = 22;
  assert.equal(book.options.baseFontSize, 12);
  for (const options of [book.options, book.project().options, book.snapshot().options]) {
    assert.equal(options.running.header.left, "{title}");
    assert.equal(options.running.header.image.dest, "assets/logo.png");
    for (const value of [options.running, options.running.header, options.running.footer, options.running.header.image])
      assert(Object.isFrozen(value));
    assert.throws(() => { options.running.header.rule = false; }, TypeError);
    options.baseFontSize = 20;
  }
  assert.equal(book.options.baseFontSize, 12);
});

test("normalization and no-op transactions are stable across key order and round trips", t => {
  const book = fixture(t); book.setPublication(full(), book.revision);
  const before = book.revision, config = book.renderConfigurationRevision;
  let calls = 0; book.subscribe(() => calls++);
  const reversed = Object.fromEntries(Object.entries(full()).reverse());
  assert.equal(book.setPublication(reversed, before), before);
  book.setPublication(JSON.parse(JSON.stringify(profile(book.options))), before);
  assert.equal(calls, 0); assert.equal(book.renderConfigurationRevision, config);
  assert.deepEqual(normalizeBookPublication(normalizeBookPublication(full())), normalizeBookPublication(full()));
});

test("empty profiles reset only publishing choices, leaving paper and metadata intact", t => {
  const book = fixture(t); book.configure({ ...book.options, title: "Kept", font: "serif" });
  book.setPage({ size: "a4", margins: 54 }, book.revision);
  const before = book.snapshot();
  book.setPublication(full(), book.revision); assert.equal(book.project().schemaVersion, 4);
  book.setPublication({}, book.revision); assert.equal(book.project().schemaVersion, 3);
  assert.deepEqual(book.snapshot(), before);
  book.setPage(undefined, book.revision); assert.equal(book.project().schemaVersion, 2);
});

test("metadata and page edits cannot erase an installed publishing profile", t => {
  const book = fixture(t); book.setPublication(full(), book.revision);
  const expected = profile(book.options);
  book.configure({ ...book.options, title: "New title" });
  book.setPage({ size: "letter", margins: 60 }, book.revision);
  book.setPage(undefined, book.revision);
  assert.deepEqual(profile(book.options), expected); assert.equal(book.project().schemaVersion, 4);
});

test("source-project downloads preserve profiles but do not authorize referenced logos", async t => {
  const book = fixture(t); book.setPublication(full(), book.revision);
  const file = new File([book.projectDownload().blob], "book.json");
  const parsed = await readBookProject(file);
  assert.equal(parsed.schemaVersion, 4);
  assert.equal(Object.hasOwn(parsed.options, "images"), false);
  assert.equal(Object.hasOwn(parsed.options, "fontAssets"), false);
  const restored = createBookCollection(); t.after(() => restored.dispose());
  restored.replaceProject(parsed);
  assert.deepEqual(restored.project(), book.project());
  assert.deepEqual(restored.images, []); assert.deepEqual(restored.fonts, []);
  assert.deepEqual(restored.snapshot().options.images, []);
  assert.equal(restored.options.running.header.image.dest, "assets/logo.png");
});

test("portable round trips preserve profile and require explicit resource authorization", async t => {
  const book = fixture(t); book.setPublication(full(), book.revision);
  const bundle = await book.portableDownload();
  const review = await readPortableBookProject(new File([bundle.blob], "book.bundle.json"));
  const restored = createBookCollection(); t.after(() => restored.dispose());
  assert.throws(() => restored.replacePortableProject(review, 0), { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
  assert.equal(restored.revision, 0);
  restored.replacePortableProject(review, 0, { authorizeResources: true });
  assert.deepEqual(restored.project(), book.project());
  assert.deepEqual(restored.snapshot(), book.snapshot());
});

test("publishing schemas reject downgrade, unknown version and concealed new options", () => {
  for (const schemaVersion of [1, 2, 3]) {
    for (const options of [{ tocDepth: 2 }, { running: {} }, { codeLineNumbers: false }])
      assert.throws(() => normalizeBookProject({ schemaVersion, files: [], options }), { code: "INVALID_PROJECT" });
  }
  assert.throws(() => normalizeBookProject({ schemaVersion: 6, files: [], options: {} }), { code: "INVALID_PROJECT" });
  assert.equal(normalizeBookProject({ schemaVersion: 4, files: [], options: {} }).schemaVersion, 4);
});

test("numeric and enum admission uses the native facade and rejects the whole transaction", t => {
  const book = fixture(t), before = book.project(), revision = book.revision;
  const bad = [{ baseFontSize: 5.99 }, { baseFontSize: NaN }, { baseFontSize: "12" },
    { headingScale: 2.01 }, { tableFontSize: 4 }, { tocDepth: 1.5 }, { tocDepth: 7 },
    { fitToPages: 0 }, { fitToPages: 0x100000000 }, { metadataEpochSeconds: -1 },
    { metadataEpochSeconds: Number.MAX_SAFE_INTEGER + 1 }, { codeLineNumbers: 1 },
    { typography: "optimal" }, { htmlFontFormat: "woff3" }];
  for (const change of bad) {
    assert.throws(() => book.setPublication({ ...full(), ...change }, revision));
    assert.equal(book.revision, revision); assert.deepEqual(book.project(), before);
  }
  for (const options of [{ baseFontSize: 6, headingScale: 1.05, tableFontSize: 5, tocDepth: 1 },
    { baseFontSize: 24, headingScale: 2, tableFontSize: 24, tocDepth: 6, fitToPages: 0xffffffff },
    { metadataEpochSeconds: Number.MAX_SAFE_INTEGER }]) assert.doesNotThrow(() => normalizeBookPublication(options));
});

test("malformed Unicode and oversized running templates never reach the collection", t => {
  const book = fixture(t), revision = book.revision;
  for (const text of ["\ud800", "😀".repeat(1025)])
    assert.throws(() => book.setPublication({ running: { header: { left: text } } }, revision));
  assert.equal(book.revision, revision);
  book.setPublication({ running: { header: { left: "😀".repeat(1024) } } }, revision);
  assert.equal(book.options.running.header.left.length, 2048);
});

test("image references obey the workbench path policy without granting new access", t => {
  const book = fixture(t), revision = book.revision;
  for (const dest of ["../logo.png", "/logo.png", "https://example.test/logo.png", "data:image/png,abc", "x%20.png", "x.ttf"])
    assert.throws(() => book.setPublication({ running: { header: { image: { dest } } } }, revision));
  for (const change of [{ position: "center" }, { heightPt: 0 }, { heightPt: 1.5 }, { heightPt: 65536 }])
    assert.throws(() => book.setPublication({ running: { header: { image: { dest: "logo.svg", ...change } } } }, revision));
  const before = book.images;
  book.setPublication({ running: { header: { image: { dest: "not-imported.svg" } } } }, revision);
  assert.deepEqual(book.images, before);
  book.revokeImages(); assert.equal(book.options.running.header.image.dest, "not-imported.svg");
});

test("unknown fields, resources and accessors are rejected without execution", t => {
  const book = fixture(t), revision = book.revision;
  let invoked = 0;
  for (const field of ["baseFontSize", "images", "fontAssets", "toJSON"])
    assert.throws(() => book.setPublication({ get [field]() { invoked++; return 12; } }, revision), { code: "INVALID_OPTIONS" });
  assert.throws(() => book.setPublication({ running: { header: { get left() { invoked++; return "bad"; } } } }, revision));
  for (const value of [null, [], new Date(), Object.create({ tocDepth: 2 }), { title: "other" }, { page: {} },
    { images: [] }, { fontAssets: [] }, { includeSources: [] }, { customCss: "x" }, { [Symbol()]: true }])
    assert.throws(() => book.setPublication(value, revision), { code: "INVALID_OPTIONS" });
  assert.equal(invoked, 0); assert.equal(book.revision, revision);
});

test("stale and disposed operations reject before inspecting user values", t => {
  const book = fixture(t); let inspected = 0;
  const proxy = new Proxy({}, { ownKeys() { inspected++; return []; } });
  assert.throws(() => book.setPublication(proxy, book.revision - 1), { code: "STALE_SOURCE" });
  assert.equal(inspected, 0);
  book.dispose(); assert.throws(() => book.setPublication(proxy, 0), { code: "SESSION_DISPOSED" });
  assert.equal(inspected, 0);
});

test("proxy reentrancy cannot overwrite a newer collection revision", t => {
  const book = fixture(t), revision = book.revision;
  const proxy = new Proxy({ tocDepth: 2 }, { ownKeys(target) {
    book.configure({ ...book.options, title: "Concurrent edit" }); return Reflect.ownKeys(target);
  } });
  assert.throws(() => book.setPublication(proxy, revision), { code: "STALE_SOURCE" });
  assert.equal(book.options.title, "Concurrent edit"); assert.equal(book.options.tocDepth, undefined);
});

test("source-only edits retain a profile and its render configuration identity", t => {
  const book = fixture(t); book.setPublication(full(), book.revision);
  const expected = profile(book.options), configuration = book.renderConfigurationRevision;
  book.edit(0, "a.md", "changed");
  const files = book.files; files[1].source = "changed include"; book.replaceSources(files, book.revision);
  assert.deepEqual(profile(book.options), expected); assert.equal(book.renderConfigurationRevision, configuration);
});

test("empty/false bands preserve the legacy path but first-page suppression is retained", () => {
  assert.deepEqual(normalizeBookPublication({ typography: "", codeLineNumbers: false,
    running: { header: { left: "", rule: false }, skipFirstPage: false } }), {});
  const value = normalizeBookPublication({ running: { skipFirstPage: true } });
  assert.equal(value.running.skipFirstPage, true);
  assert.equal(normalizeBookPublication({ metadataEpochSeconds: 0 }).metadataEpochSeconds, 0);
});

test("source serialization retains escaped template text without a second format", t => {
  const book = fixture(t); const text = '\\n"<script>😀</script>\n';
  book.setPublication({ running: { footer: { right: text } } }, book.revision);
  const serialized = serializeBookProject(book.project());
  assert.deepEqual(normalizeBookProject(JSON.parse(serialized)), book.project());
  assert.equal(JSON.parse(serialized).options.running.footer.right, text);
});
