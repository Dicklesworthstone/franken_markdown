import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import {
  createBookCollection, readPortableBookProject, discardPortableBookProject,
  normalizeBookProject, PORTABLE_BOOK_FORMAT, PORTABLE_BOOK_MAX_BYTES,
} from "../demo/book_collection.mjs";

// Byte ownership tests, not image/font parser or native rendering substitutes.
const bytes = (n = 300001) => Uint8Array.from({ length: n }, (_, i) => i % 251);
const file = value => new File([typeof value === "string" ? value : JSON.stringify(value)], "book.fmdbook.bundle.json");
const importBlob = blob => readPortableBookProject(new File([blob], "book.fmdbook.bundle.json"));
function fixture() {
  const book = createBookCollection();
  const backing = new Uint8Array([255, 0, 128, 42, 254]);
  book.append({
    chapters: [
      { path: "guide/end.md", source: "\ufeff# É😀\r\n\r\n![plot](../assets/plot.png)\rLast\n" },
      { path: "guide/start.md", source: "# Start\n\n{{#include ../parts/shared.rs}}\n" },
    ],
    includeSources: [{ path: "parts/shared.rs", source: "fn main() {}\r\n" }],
    images: [
      { destination: "assets/plot.png", bytes: backing.subarray(1, 4) },
      { destination: "assets/photo.jpeg", bytes: bytes(31) },
      { destination: "assets/diagram.svg", bytes: new TextEncoder().encode('<svg><text>é</text></svg>') },
    ],
  });
  book.setFonts([
    { slot: "body-regular", name: "Variable.ttf", weight: 450, bytes: bytes(70003) },
    { slot: "mono-regular", name: "Mono.ttf", bytes: backing.subarray(2, 4) },
  ], book.revision);
  book.configure({ title: "Portable 日本語", author: "A", lang: "fr", font: "serif", fontScale: 1.25, toc: false });
  backing.fill(99);
  return book;
}
const restore = (book, review, revision = book.revision) =>
  book.replacePortableProject(review, revision, { authorizeResources: true });
async function envelope() {
  return JSON.parse(await (await fixture().portableDownload()).blob.text());
}

test("portable round trip preserves exact source, roles, order, settings and resource views", async () => {
  const original = fixture(), target = createBookCollection();
  const output = await original.portableDownload();
  assert.equal(output.filename, "book.fmdbook.bundle.json");
  assert.equal(output.blob.type, "application/json");
  assert.equal(JSON.parse(await output.blob.text()).format, PORTABLE_BOOK_FORMAT);
  assert.equal(output.revision, original.revision);
  const review = await importBlob(output.blob);
  assert.equal(review.chapters, 2);
  assert.equal(review.includeSources, 1);
  assert.deepEqual(review.images.map(i => i.destination), original.images.map(i => i.destination));
  assert.deepEqual(review.fonts, original.fonts);
  assert(!JSON.stringify(review).includes("data"));
  const observed = [];
  target.subscribe(() => observed.push({ project: target.project(), images: target.images, fonts: target.fonts }));
  assert.equal(restore(target, review), 1);
  assert.deepEqual(observed, [{ project: original.project(), images: original.images, fonts: original.fonts }]);
  assert.deepEqual(target.snapshot(), original.snapshot());
  assert.deepEqual(target.snapshot().options.images[0].bytes, new Uint8Array([0, 128, 42]));
  target.edit(0, target.files[0].path, target.files[0].source.replace(/\r\n?/g, "\n"));
  assert.equal(target.files[0].source, original.files[0].source);
  assert.deepEqual(await (await target.portableDownload()).blob.text(), await output.blob.text());
  discardPortableBookProject(review); // Cannot dispose resources transferred to target.
  assert.deepEqual(target.snapshot(), original.snapshot());
});

test("source-only projects and library snapshots stay resource-free and reject portable envelopes", async () => {
  const book = fixture();
  assert.deepEqual(Object.keys(book.project()).sort(), ["files", "options", "schemaVersion"]);
  const source = JSON.parse(await book.projectDownload().blob.text());
  assert.equal(source.schemaVersion, 2);
  const portable = await envelope();
  assert.throws(() => normalizeBookProject(portable), { code: "INVALID_PROJECT" });
  await assert.rejects(readPortableBookProject(file(source)), { code: "INVALID_PROJECT" });
  book.replaceProject(source);
  assert.equal(book.images.length, 0);
  assert.equal(book.fonts.length, 0);
});

test("empty books and include-only collections can be backed up before they can publish", async () => {
  for (const include of [false, true]) {
    const book = createBookCollection();
    if (include) book.append({ chapters: [], images: [], includeSources: [{ path: "part.txt", source: "" }] });
    const review = await importBlob((await book.portableDownload()).blob);
    assert.equal(review.chapters, 0);
    assert.equal(review.includeSources, Number(include));
    const target = createBookCollection();
    restore(target, review);
    assert.deepEqual(target.project(), book.project());
    assert.throws(() => target.snapshot(), { code: "EMPTY_BOOK" });
  }
});

test("restoring needs explicit authorization, a current revision and a live private review", async () => {
  const book = fixture(), original = book.snapshot(), revision = book.revision;
  const review = await importBlob((await book.portableDownload()).blob);
  assert(Object.isFrozen(review) && Object.isFrozen(review.images) && Object.isFrozen(review.images[0]));
  for (const authorized of [undefined, false, "true", 1]) {
    assert.throws(() => book.replacePortableProject(review, revision, { authorizeResources: authorized }),
      { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
  }
  for (const stale of [undefined, revision - 1, revision + 1, NaN])
    assert.throws(() => book.replacePortableProject(review, stale, { authorizeResources: true }), { code: "STALE_SOURCE" });
  assert.throws(() => restore(book, { ...review }), { code: "INVALID_PROJECT" });
  assert.deepEqual(book.snapshot(), original);
  assert.equal(book.revision, revision);
  discardPortableBookProject(review);
  discardPortableBookProject(review);
  assert.throws(() => restore(book, review), { code: "INVALID_PROJECT" });
  const next = await importBlob((await book.portableDownload()).blob);
  restore(book, next);
  assert.throws(() => restore(book, next), { code: "INVALID_PROJECT" });
  assert.deepEqual(book.snapshot(), original);
});

test("complete resource validation precedes authorization and mutation", async () => {
  const book = fixture(), before = book.snapshot(), revision = book.revision;
  const base = await envelope();
  const cases = [
    x => x.schemaVersion = 2,
    x => x.format = "other",
    x => x.script = "alert(1)",
    x => x.project.files[0].path = "../escape.md",
    x => x.project.files[0].source = "\ud800",
    x => x.project.options.fontScale = 0,
    x => x.images.push({ ...x.images[0] }),
    x => x.images[0].destination = "https://example.com/a.png",
    x => x.images[0].destination = "assets/a.html",
    x => x.images[0].fetch = "https://example.com",
    x => x.images[0].encoding = "url",
    x => x.images = Array(129).fill(x.images[0]),
    x => x.fontAssets.push({ ...x.fontAssets[0] }),
    x => x.fontAssets[0].slot = "other",
    x => x.fontAssets[0].name = "bad\nname.ttf",
    x => x.fontAssets[0].weight = 1001,
    x => x.fontAssets[0].weight = null,
    x => x.fontAssets = Array(6).fill(x.fontAssets[0]),
  ];
  for (const mutate of cases) {
    const value = structuredClone(base); mutate(value);
    await assert.rejects(readPortableBookProject(file(value)));
    assert.equal(book.revision, revision);
    assert.deepEqual(book.snapshot(), before);
  }
});

test("canonical base64 rejects invalid padding, hidden whitespace and trailing data", async () => {
  const base = await envelope();
  for (const data of ["", "AA", "A===", "AB==", "AAB=", "AA==\n", "AA A", "AA-_", "!!!!", "AA==AAAA", "AAAA".repeat(4095) + "AA==" + "AAAA"]) {
    const value = structuredClone(base); value.images[0].data = data;
    await assert.rejects(readPortableBookProject(file(value)), { code: "INVALID_PROJECT" }, data);
  }
  for (const n of [1, 2, 3, 12287, 12288, 12289, 262145]) {
    const value = structuredClone(base), expected = bytes(n);
    value.images = [{ destination: "x.png", encoding: "base64", data: Buffer.from(expected).toString("base64") }];
    const target = createBookCollection();
    restore(target, await readPortableBookProject(file(value)));
    assert.deepEqual(target.snapshot().options.images[0].bytes, expected);
  }
});

test("file and decoded resource size limits reject before reading or base64 decoding", async () => {
  const oversized = new File([], "huge.json");
  let reads = 0;
  Object.defineProperty(oversized, "size", { value: PORTABLE_BOOK_MAX_BYTES + 1 });
  oversized.arrayBuffer = () => { reads++; throw new Error("must not read"); };
  await assert.rejects(readPortableBookProject(oversized), { code: "FILE_LIMIT" });
  assert.equal(reads, 0);
  const base = await envelope(), atobBefore = globalThis.atob;
  let decodes = 0;
  globalThis.atob = () => { decodes++; throw new Error("must admit size first"); };
  try {
    for (const fonts of [false, true]) {
      const value = structuredClone(base), key = fonts ? "fontAssets" : "images";
      value[key][0].data = "AAAA".repeat(Math.floor(8 * 1024 * 1024 / 3) + 1);
      await assert.rejects(readPortableBookProject(file(value)), { code: "BOOK_LIMIT" });
    }
    const value = structuredClone(base);
    value.images = Array.from({ length: 5 }, (_, i) => ({
      destination: `${i}.png`, encoding: "base64", data: "AAAA".repeat(7 * 1024 * 1024 / 4),
    }));
    // 5 * 5.25 MiB is below 32; use seven entries (36.75 MiB).
    value.images.push({ ...value.images[0], destination: "5.png" }, { ...value.images[0], destination: "6.png" });
    await assert.rejects(readPortableBookProject(file(value)), { code: "BOOK_LIMIT" });
    assert.equal(decodes, 0);
  } finally { globalThis.atob = atobBefore; }
});

test("invalid UTF-8, malformed JSON and inconsistent File bytes produce no review", async () => {
  for (const content of [new Uint8Array([0xff]), "{", "null", "[]"])
    await assert.rejects(readPortableBookProject(new File([content], "bad.json")), { code: "INVALID_PROJECT" });
  const bad = new File(["x"], "bad.json");
  bad.arrayBuffer = async () => new ArrayBuffer(2);
  await assert.rejects(readPortableBookProject(bad), { code: "FILE_READ_FAILED" });
});

test("in-flight export rejects intervening source edits and disposal", async () => {
  for (const change of [book => book.edit(0, book.files[0].path, "new"), book => book.dispose()]) {
    const book = fixture(), pending = book.portableDownload();
    change(book);
    await assert.rejects(pending, error => ["STALE_SOURCE", "SESSION_DISPOSED"].includes(error.code));
  }
});

test("cancellation interrupts waiting reads and long encode/decode without unhandled late work", async () => {
  const book = fixture();
  const controller = new AbortController(), pending = book.portableDownload({ signal: controller.signal });
  controller.abort();
  await assert.rejects(pending, { code: "ABORTED" });
  const slow = new File(["x"], "slow.json");
  let rejectRead;
  slow.arrayBuffer = () => new Promise((_, reject) => { rejectRead = reject; });
  const readAbort = new AbortController(), readPending = readPortableBookProject(slow, { signal: readAbort.signal });
  readAbort.abort();
  await assert.rejects(readPending, { code: "ABORTED" });
  rejectRead(new Error("late I/O failure"));
  const raw = await envelope(); raw.images[0].data = Buffer.from(bytes()).toString("base64");
  const decodeAbort = new AbortController(), decoding = readPortableBookProject(file(raw), { signal: decodeAbort.signal });
  setTimeout(() => decodeAbort.abort(), 0);
  await assert.rejects(decoding, { code: "ABORTED" });
  await assert.rejects(book.portableDownload({ signal: {} }), { code: "INVALID_OPTIONS" });
  assert.equal(book.files.length, 3);
});

test("returned resource snapshots cannot mutate installed portable bytes", async () => {
  const book = fixture(), expected = book.snapshot();
  const target = createBookCollection();
  restore(target, await importBlob((await book.portableDownload()).blob));
  const exposed = target.snapshot();
  exposed.options.images[0].bytes.fill(255);
  exposed.options.fontAssets[0].bytes.fill(255);
  assert.deepEqual(target.snapshot(), expected);
  book.dispose();
  assert.deepEqual(target.snapshot(), expected);
});
