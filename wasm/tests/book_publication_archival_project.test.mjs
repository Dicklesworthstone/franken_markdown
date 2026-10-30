import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import {
  BOOK_PUBLICATION_FIELDS, createBookCollection, normalizeBookPublication, normalizeBookProject,
  serializeBookProject, readBookProject, readPortableBookProject, discardPortableBookProject,
} from "../demo/book_collection.mjs";
import { prepareBookInput } from "../book_session.mjs";
import { createBookCollectionCapture } from "../book_worker.mjs";
import { collection } from "./book_publication_archival_helpers.mjs";
const policy = (strict = true) => ({ pdfA: "2b", pdfAStrict: strict });
const code = expected => e => e?.code === expected;

for (const strict of [false, true]) test(`archival profile reaches every collection capture: strict=${strict}`, t => {
  const book = collection(t), before = book.snapshot(), revision = book.revision;
  const config = book.renderConfigurationRevision, resource = book.renderResourceRevision, events = [];
  book.subscribe(() => events.push(book.project()));
  book.setPublication({ ...policy(strict), baseFontSize: 12, metadataEpochSeconds: 0 }, revision);
  const after = book.snapshot();
  for (const view of [book.options, book.project().options, after.options,
    prepareBookInput(after.files, after.options).options]) {
    assert.equal(view.pdfA, "2b"); assert.equal(view.pdfAStrict, strict);
    assert.equal(view.baseFontSize, 12); assert.equal(view.metadataEpochSeconds, 0);
  }
  assert.equal(book.revision, revision + 1); assert.equal(book.renderConfigurationRevision, config + 1);
  assert.equal(book.renderResourceRevision, resource + 1); assert.equal(events.length, 1);
  assert.equal(events[0].schemaVersion, 5); assert.deepEqual(after.files, before.files);
  for (const key of ["page", "title", "images", "fontAssets", "includeSources"])
    assert.deepEqual(after.options[key], before.options[key]);
});

test("off and false use existing minimal schemas; only enabled archival policy selects version 5", t => {
  const book = createBookCollection(); t.after(() => book.dispose());
  for (const value of [{}, {pdfA: "off"}, {pdfAStrict: false}, {pdfA: "off", pdfAStrict: false}]) {
    book.setPublication(value, book.revision);
    assert.equal(book.revision, 0); assert.equal(book.project().schemaVersion, 1);
    assert.equal(Object.hasOwn(book.options, "pdfA"), false);
    assert.equal(Object.hasOwn(book.options, "pdfAStrict"), false);
  }
  book.setPublication(policy(), book.revision); assert.equal(book.project().schemaVersion, 5);
  book.setPublication({baseFontSize: 12}, book.revision); assert.equal(book.project().schemaVersion, 4);
  book.setPage({size: "a4"}, book.revision); book.setPublication({}, book.revision);
  assert.equal(book.project().schemaVersion, 3);
  book.setPage(undefined, book.revision); assert.equal(book.project().schemaVersion, 1);
});

test("archival normalization is canonical and no-op assignments do not invalidate outputs", t => {
  const book = collection(t); book.setPublication({pdfA: "2b"}, book.revision);
  assert.deepEqual(normalizeBookPublication({pdfA: "2b"}), policy(false));
  assert.deepEqual(normalizeBookPublication(normalizeBookPublication(policy())), policy());
  const before = [book.revision, book.renderConfigurationRevision, book.renderResourceRevision];
  let events = 0; book.subscribe(() => events++);
  book.setPublication({pdfAStrict: false, pdfA: "2b"}, book.revision);
  assert.equal(events, 0); assert.deepEqual([book.revision, book.renderConfigurationRevision, book.renderResourceRevision], before);
  assert(BOOK_PUBLICATION_FIELDS.includes("pdfA")); assert(BOOK_PUBLICATION_FIELDS.includes("pdfAStrict"));
});

for (const value of [{pdfA: null}, {pdfA: "2B"}, {pdfA: "3b"}, {pdfA: ""}, {pdfA: 2},
  {pdfAStrict: true}, {pdfA: "off", pdfAStrict: true}, {pdfA: "2b", pdfAStrict: "true"}]) {
  test(`invalid policy cannot partially replace a profile: ${JSON.stringify(value)}`, t => {
    const book = collection(t); book.setPublication({...policy(), baseFontSize: 11}, book.revision);
    const before = book.project(), revision = book.revision;
    assert.throws(() => book.setPublication({...value, baseFontSize: 13}, revision), code("INVALID_OPTIONS"));
    assert.deepEqual(book.project(), before); assert.equal(book.revision, revision);
  });
}

test("policy accessors and stale proxy requests cannot run or overwrite newer settings", t => {
  const book = collection(t), rev = book.revision; let reads = 0;
  for (const key of ["pdfA", "pdfAStrict"])
    assert.throws(() => book.setPublication({get [key]() {reads++; return "2b";}}, rev), code("INVALID_OPTIONS"));
  assert.equal(reads, 0);
  const value = new Proxy(policy(), {ownKeys(target) {
    book.configure({...book.options, title: "Concurrent"}); return Reflect.ownKeys(target);
  }});
  assert.throws(() => book.setPublication(value, rev), code("STALE_SOURCE"));
  assert.equal(book.options.title, "Concurrent"); assert.equal(book.options.pdfA, undefined);
});

test("editing an archival profile does not copy or authorize asset bytes", t => {
  const book = collection(t), before = book.snapshot(), slice = Uint8Array.prototype.slice;
  Uint8Array.prototype.slice = () => { throw Error("unexpected asset clone"); };
  try { book.setPublication(policy(), book.revision); }
  finally { Uint8Array.prototype.slice = slice; }
  const after = book.snapshot();
  assert.deepEqual(after.options.images, before.options.images); assert.deepEqual(after.options.fontAssets, before.options.fontAssets);
});

for (const schemaVersion of [1, 2, 3, 4]) test(`version ${schemaVersion} cannot conceal archival fields`, () => {
  for (const options of [policy(), {pdfA: "off"}, {pdfAStrict: false}, {pdfA: undefined}])
    assert.throws(() => normalizeBookProject({schemaVersion, files: [], options}), code("INVALID_PROJECT"));
});

test("version 5 is admitted, future/invalid versions remain refused without changing an existing book", t => {
  const book = collection(t); book.setPublication(policy(), book.revision);
  const before = book.project(), revision = book.revision;
  assert.deepEqual(normalizeBookProject(before), before);
  for (const schemaVersion of [0, 7, 999, "5", null])
    assert.throws(() => book.replaceProject({...before, schemaVersion}), code("INVALID_PROJECT"));
  assert.equal(book.revision, revision); assert.deepEqual(book.project(), before);
  assert.throws(() => book.replaceProject({...before, options: {pdfAStrict: true}}), code("INVALID_OPTIONS"));
  assert.deepEqual(book.project(), before);
});

for (const strict of [false, true]) test(`source download and restore keep policy without carrying resource grants: strict=${strict}`, async t => {
  const book = collection(t); book.setPublication({...policy(strict), running: {header: {left: "A\r\nB", image: {dest: "assets/logo.png"}}}}, book.revision);
  const before = book.project();
  const output = book.projectDownload(), parsed = await readBookProject(new File([output.blob], output.filename));
  assert.equal(parsed.schemaVersion, 5); assert.deepEqual(parsed.files, before.files);
  assert.equal(parsed.options.pdfAStrict, strict);
  const restored = createBookCollection(); t.after(() => restored.dispose());
  restored.replaceProject(parsed);
  assert.deepEqual(restored.project(), before);
  assert.deepEqual(restored.images, []); assert.deepEqual(restored.fonts, []);
  assert.equal(restored.snapshot().options.pdfAStrict, strict);
  assert.equal(restored.options.running.header.left, "A\r\nB");
  assert(!Object.hasOwn(parsed.options, "images")); assert(!Object.hasOwn(parsed.options, "fontAssets"));
  assert.equal(serializeBookProject(restored.project()), serializeBookProject(before));
});

for (const strict of [false, true]) test(`portable round trip is atomic and preserves policy plus authorized resources: strict=${strict}`, async t => {
  const book = collection(t); book.setPublication(policy(strict), book.revision);
  const output = await book.portableDownload(), raw = JSON.parse(await output.blob.text());
  assert.equal(raw.schemaVersion, 1); assert.equal(raw.project.schemaVersion, 5);
  const review = await readPortableBookProject(new File([output.blob], output.filename));
  t.after(() => discardPortableBookProject(review));
  const restored = collection(t), before = restored.project(), revision = restored.revision;
  assert.throws(() => restored.replacePortableProject(review, revision), code("RESOURCE_AUTHORIZATION_REQUIRED"));
  assert.deepEqual(restored.project(), before);
  assert.throws(() => restored.replacePortableProject(review, revision - 1, {authorizeResources: true}), code("STALE_SOURCE"));
  const events = []; restored.subscribe(() => events.push(restored.snapshot()));
  restored.replacePortableProject(review, revision, {authorizeResources: true});
  assert.equal(events.length, 1); assert.deepEqual(events[0], book.snapshot());
  assert.deepEqual(restored.project(), book.project());
  assert.throws(() => restored.replacePortableProject(review, restored.revision, {authorizeResources: true}), code("INVALID_PROJECT"));
});

test("bad portable project versions fail before decoding even malformed resources", async t => {
  const book = collection(t); book.setPublication(policy(), book.revision);
  const raw = JSON.parse(await (await book.portableDownload()).blob.text());
  raw.project.schemaVersion = 4; raw.images[0].data = "not base64";
  await assert.rejects(readPortableBookProject(new File([JSON.stringify(raw)], "bad.bundle.json")), e =>
    e.code === "INVALID_PROJECT" && /schema version 5/.test(e.message));
});

test("pending portable capture is invalidated when archival policy changes", async t => {
  const book = collection(t); book.setPublication(policy(), book.revision);
  const pending = book.portableDownload();
  book.setPublication(policy(false), book.revision);
  await assert.rejects(pending, code("STALE_SOURCE"));
  assert.equal(book.options.pdfAStrict, false);
});

test("source-only collection captures retain policy, and changing it forces a full capture", async t => {
  const book = collection(t); book.setPublication(policy(), book.revision);
  let revision = 0; const calls = [];
  const worker = {get retainedInputRevision() {return revision || null;}, supportsSourceSet: true,
    async render(files, format, options) {calls.push(["full", files, options]);return {retainedInputRevision: ++revision};},
    async renderSources(files, format, options) {calls.push(["sources", files, options]);return {retainedInputRevision: ++revision};},
    async renderSourceSet(files, format, options) {calls.push(["set", files, options]);return {retainedInputRevision: ++revision};},
  };
  const capture = createBookCollectionCapture(book, worker);
  await capture().render("pdf");
  book.edit(0, "guide/one.md", "Changed");
  const next = capture(); assert.equal(next.options.pdfAStrict, true); await next.render("pdf");
  book.move(0, 1); await capture().render("pdf");
  book.setPublication(policy(false), book.revision); await capture().render("pdf");
  assert.deepEqual(calls.map(call => call[0]), ["full", "sources", "set", "full"]);
  assert.equal(calls[0][2].pdfAStrict, true); assert.equal(calls[3][2].pdfAStrict, false);
  for (const call of calls.slice(1, 3)) assert.deepEqual(Object.keys(call[2]).sort(), ["expectedRevision", "includeSources"]);
});
