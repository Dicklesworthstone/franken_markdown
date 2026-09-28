import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookCollection, normalizeBookProject, serializeBookProject,
  readBookProject, readPortableBookProject, discardPortableBookProject } from "../demo/book_collection.mjs";
import { normalizePdfPage, pdfPageGeometry } from "../pdf_page.mjs";

function fixture(include = true) {
  const book = createBookCollection();
  book.append({ chapters: [{ path: "a.md", source: "\ufeff# Héllo\r\n{{#include shared.txt}}\r" }],
    images: [{ destination: "x.png", bytes: new Uint8Array([1, 2, 3]) }],
    ...(include ? { includeSources: [{ path: "shared.txt", source: "shared\r\n" }] } : {}) });
  book.setFonts([{ slot: "body-bold", name: "a.ttf", weight: 650, bytes: new Uint8Array([4, 5]) }], book.revision);
  return book;
}
const page = () => ({ size: { widthPt: 432, heightPt: 648 }, orientation: "landscape",
  margins: { topPt: 30, rightPt: 42, bottomPt: 54, leftPt: 66 } });

test("book page changes preserve source, order, settings and exact resource bytes in one revision", () => {
  const book = fixture(), before = book.snapshot(), revision = book.revision, seen = [];
  book.subscribe(() => seen.push(book.snapshot()));
  assert.equal(book.setPage(page(), revision), revision + 1);
  const expected = { ...before, options: { ...before.options, page: normalizePdfPage(page()) } };
  assert.deepEqual(seen, [expected]);
  assert.deepEqual(book.snapshot(), expected);
  assert.equal(book.project().schemaVersion, 3);
  assert.deepEqual([...pdfPageGeometry(book.snapshot().options.page)], [648, 432, 30, 42, 54, 66]);
});

test("explicit geometry is owned and deeply frozen in reads, snapshots and source projects", () => {
  const book = fixture(), supplied = page();
  book.configure({ ...book.options, page: supplied });
  supplied.size.widthPt = 900; supplied.margins.leftPt = 0;
  for (const p of [book.options.page, book.snapshot().options.page, book.project().options.page]) {
    assert(Object.isFrozen(p) && Object.isFrozen(p.size) && Object.isFrozen(p.margins));
    assert.throws(() => p.margins.leftPt = 0, TypeError);
    assert.deepEqual(p, normalizePdfPage(page()));
  }
});

test("equal normalized geometry does not advance revision or notify observers", () => {
  const book = fixture();
  book.setPage({ size: "letter" }, book.revision);
  const revision = book.revision;
  let calls = 0; book.subscribe(() => calls++);
  assert.equal(book.setPage({ size: { widthPt: 612, heightPt: 792 }, margins: 72 }, revision), revision);
  assert.equal(calls, 0);
});

test("default books preserve v1/v2 serialization and the legacy PDF ABI path", () => {
  for (const include of [false, true]) {
    const book = fixture(include), original = serializeBookProject(book.project());
    assert.equal(book.project().schemaVersion, include ? 2 : 1);
    assert(!Object.hasOwn(book.options, "page"));
    assert.equal(pdfPageGeometry(book.snapshot().options.page).length, 0);
    book.setPage(page(), book.revision);
    book.setPage(undefined, book.revision);
    assert.equal(serializeBookProject(book.project()), original);
    assert.equal(pdfPageGeometry(book.snapshot().options.page).length, 0);
  }
});

test("source downloads and JSON-library snapshots round trip geometry with no resource grant", async () => {
  const source = fixture(); source.setPage(page(), source.revision);
  const downloaded = source.projectDownload();
  const parsed = await readBookProject(new File([downloaded.blob], downloaded.filename));
  const saved = normalizeBookProject(JSON.parse(serializeBookProject(parsed)));
  const target = fixture(); target.replaceProject(saved);
  assert.deepEqual(target.options, source.options);
  assert.deepEqual(target.files, source.files);
  assert.deepEqual(target.images, []); assert.deepEqual(target.fonts, []);
  assert.deepEqual(Object.keys(saved).sort(), ["files", "options", "schemaVersion"]);
});

test("portable backup restores geometry and resources atomically", async () => {
  const book = fixture(); book.setPage(page(), book.revision);
  const backup = await book.portableDownload();
  const envelope = JSON.parse(await backup.blob.text());
  assert.equal(envelope.schemaVersion, 1); assert.equal(envelope.project.schemaVersion, 3);
  const review = await readPortableBookProject(new File([backup.blob], backup.filename));
  const target = fixture(), before = target.revision, seen = [];
  target.subscribe(() => seen.push(target.snapshot()));
  target.replacePortableProject(review, before, { authorizeResources: true });
  assert.deepEqual(seen, [book.snapshot()]);
  discardPortableBookProject(review);
});

test("page-bearing projects require v3 and malformed geometry rejects before replacement", () => {
  const book = fixture(), before = book.snapshot(), revision = book.revision;
  for (const schemaVersion of [1, 2]) {
    const project = { schemaVersion, files: [{ path: "x.md", source: "new" }], options: { page: page() } };
    assert.throws(() => book.replaceProject(project), { code: "INVALID_PROJECT" });
  }
  for (const geometry of [null, [], { size: "a5" }, { margins: 500 },
    { size: { widthPt: 1, heightPt: 500 } }, { margins: { topPt: NaN } }, { typo: 5 }]) {
    assert.throws(() => book.replaceProject({ schemaVersion: 3, files: [], options: { page: geometry } }));
    assert.deepEqual(book.snapshot(), before); assert.equal(book.revision, revision);
  }
  assert.throws(() => normalizeBookProject({ schemaVersion: 4, files: [] }), { code: "INVALID_PROJECT" });
});

test("stale, disposed and reentrant page transactions cannot overwrite newer state", () => {
  const book = fixture(), revision = book.revision;
  for (const stale of [undefined, -1, revision - 1, revision + 1, NaN])
    assert.throws(() => book.setPage(page(), stale), { code: "STALE_SOURCE" });
  const value = new Proxy({}, { ownKeys() {
    book.configure({ ...book.options, title: "newer" }); return [];
  } });
  assert.throws(() => book.setPage(value, revision), { code: "STALE_SOURCE" });
  assert.equal(book.options.title, "newer"); assert.equal(book.options.page, undefined);
  book.dispose();
  assert.throws(() => book.setPage(page(), book.revision), { code: "SESSION_DISPOSED" });
});

test("page accessors are rejected without invocation and invalid updates keep the old page", () => {
  const book = fixture(); book.setPage(page(), book.revision);
  const before = book.snapshot(), revision = book.revision;
  let calls = 0;
  const invalid = { get size() { calls++; return "a4"; } };
  assert.throws(() => book.setPage(invalid, revision), { code: "INVALID_OPTIONS" });
  assert.equal(calls, 0); assert.deepEqual(book.snapshot(), before); assert.equal(book.revision, revision);
});

test("asymmetric margins do not rotate and f32 content limits are enforced", () => {
  const book = fixture(); book.setPage(page(), book.revision);
  const geometry = book.options.page;
  assert.equal(geometry.margins.leftPt, 66); assert.equal(geometry.margins.topPt, 30);
  assert.throws(() => book.setPage({ size: { widthPt: 144, heightPt: 144 }, margins: 36.000001 }, book.revision));
  book.setPage({ size: { widthPt: 144, heightPt: 144 }, margins: 36 }, book.revision);
  assert.deepEqual([...pdfPageGeometry(book.options.page)], [144, 144, 36, 36, 36, 36]);
});
