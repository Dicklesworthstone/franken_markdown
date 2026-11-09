import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { createBookSourceArchive, BOOK_SOURCE_ARCHIVE_PROJECT, BOOK_SOURCE_ARCHIVE_LIMITS } from "../demo/book_source_archive.mjs";
import { normalizeBookProject, serializeBookProject, readBookProject } from "../demo/book_collection.mjs";
import { createBookEditionBundle } from "../demo/book_edition.mjs";
import { createBookZip, bookZipCrc32 } from "../demo/book_zip.mjs";
import { readSourceZip } from "./book_source_archive_helpers.mjs";

const projectCodec = { normalizeBookProject, serializeBookProject };
const encoder = new TextEncoder();
const text = bytes => new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
const options = { projectCodec };
const project = (files = [{ path: "guide/日本😀.md", source: "\ufeff# Café 😀\r\n\r\n{{#include ../parts/shared.txt}}\r\n" },
  { path: "parts/shared.txt", source: "First\rSecond\nThird\r\n", role: "include" },
  { path: "empty.md", source: "" }]) => ({ schemaVersion: 7, files,
  options: { title: "Captured book", profile: "gfm-plus", page: { size: "a4", margins: { leftPt: 54 } },
    baselineGridPt: 14, running: { header: { image: { dest: "logo.png" } } } } });

test("source ZIP preserves exact UTF-8 files, roles, order and applied settings with independently verified records", async () => {
  const input = project(), expected = normalizeBookProject(input);
  const result = await createBookSourceArchive(input, options);
  assert.equal(result.format, "book-source-archive"); assert.equal(result.filename, "book-sources.zip");
  assert.equal(result.mimeType, "application/zip"); assert.equal(result.byteLength, result.blob.size);
  assert.equal(result.sourceCount, 3); assert(Object.isFrozen(result));
  const entries = await readSourceZip(result.blob);
  assert.deepEqual([...entries.keys()], [...input.files.map(file => `sources/${file.path}`), BOOK_SOURCE_ARCHIVE_PROJECT]);
  for (const file of input.files) assert.deepEqual(entries.get(`sources/${file.path}`), encoder.encode(file.source));
  assert.equal(result.sourceBytes, input.files.reduce((sum, file) => sum + encoder.encode(file.source).length, 0));
  assert.equal(text(entries.get(BOOK_SOURCE_ARCHIVE_PROJECT)), serializeBookProject(input));
  const reopened = await readBookProject(new File([entries.get(BOOK_SOURCE_ARCHIVE_PROJECT)], BOOK_SOURCE_ARCHIVE_PROJECT));
  assert.deepEqual(reopened, expected);
  assert.deepEqual(Object.keys(reopened), ["schemaVersion", "files", "options"]);
  assert.equal(reopened.options.running.header.image.dest, "logo.png");
  assert(!entries.has("logo.png")); assert(!entries.has("sources/logo.png"));
});

test("manifest namespace cannot collide with a source and include-only books can be saved", async () => {
  const input = project([{ path: "book.fmdbook.json", source: "the user's actual source", role: "include" },
    { path: "sources/nested.txt", source: "nested", role: "include" }]);
  const entries = await readSourceZip((await createBookSourceArchive(input, options)).blob);
  assert.equal(text(entries.get("sources/book.fmdbook.json")), "the user's actual source");
  assert.equal(text(entries.get("sources/sources/nested.txt")), "nested");
  assert.equal(JSON.parse(text(entries.get(BOOK_SOURCE_ARCHIVE_PROJECT))).files[0].role, "include");
});

test("source and setting mutations during CRC cannot retarget the captured archive", async () => {
  const input = project(), expected = serializeBookProject(input);
  const pending = createBookSourceArchive(input, options);
  input.files[0].source = "changed"; input.files[0].path = "changed.md"; input.files.reverse();
  input.options.page.margins.leftPt = 99; input.options.title = "changed";
  const entries = await readSourceZip((await pending).blob);
  assert.equal(text(entries.get(BOOK_SOURCE_ARCHIVE_PROJECT)), expected);
  assert.match(text(entries.get("sources/guide/日本😀.md")), /^\ufeff# Café 😀\r\n/);
  assert(!entries.has("sources/changed.md"));
});

test("unchanged source captures produce deterministic ZIP bytes without clock or crypto dependence", async () => {
  const first = await createBookSourceArchive(project(), options);
  const second = await createBookSourceArchive(project(), options);
  assert.deepEqual(await first.blob.arrayBuffer(), await second.blob.arrayBuffer());
  const changed = project(); changed.files.reverse();
  assert.notDeepEqual(await first.blob.arrayBuffer(), await (await createBookSourceArchive(changed, options)).blob.arrayBuffer());
});

test("portable paths reject traversal, platform aliases and file-directory collisions before packaging", async () => {
  for (const path of ["../escape.md", "/absolute.md", "a\\b.md", "a//b.md", "a/./b.md", "a/../b.md", "C:drive.md",
    "a%2fb.md", "a#b.md", "a\0b.md", "CON.md", "parts/LPT1.txt", "bad./a.md", "trailing .md "]) {
    await assert.rejects(createBookSourceArchive(project([{ path, source: "x", role: "include" }]), options),
      error => error.code === "INVALID_PATH", path);
  }
  for (const paths of [["A.md", "a.md"], ["café.md", "cafe\u0301.md"], ["part.md", "PART.md/body.md"]])
    await assert.rejects(createBookSourceArchive(project(paths.map(path => ({ path, source: "x" }))), options), { code: "PATH_COLLISION" });
});

test("source-only admission rejects resource fields, malformed Unicode and existing workbench limits", async () => {
  let grantsRead = 0;
  const resources = project(); Object.defineProperty(resources, "images", { enumerable: true, get() { grantsRead++; throw Error("no authority"); } });
  await assert.rejects(createBookSourceArchive(resources, options), { code: "INVALID_PROJECT" });
  const fonts = project(); Object.defineProperty(fonts.options, "fontAssets", { enumerable: true, get() { grantsRead++; throw Error("no authority"); } });
  await assert.rejects(createBookSourceArchive(fonts, options), { code: "INVALID_OPTIONS" });
  assert.equal(grantsRead, 0);
  await assert.rejects(createBookSourceArchive(project([{ path: "a.md", source: "\ud800" }]), options), /malformed Unicode/);
  await assert.rejects(createBookSourceArchive(project([{ path: "a.md", source: "x".repeat(BOOK_SOURCE_ARCHIVE_LIMITS.fileBytes + 1) }]), options), /UTF-8 budget/);
  const large = "x".repeat(BOOK_SOURCE_ARCHIVE_LIMITS.fileBytes);
  await assert.rejects(createBookSourceArchive(project(Array.from({ length: 4 }, (_, i) => ({ path: `${i}.md`, source: large }))), options), { code: "BOOK_LIMIT" });
  await assert.rejects(createBookSourceArchive(project(Array.from({ length: 257 }, (_, i) => ({ path: `${i}.txt`, source: "", role: "include" }))), options), { code: "BOOK_LIMIT" });
  await assert.rejects(createBookSourceArchive(project([]), options), { code: "BOOK_LIMIT" });
});

test("cancellation settles before admission or during chunked CRC without publishing partial bytes", async () => {
  const early = new AbortController(); early.abort();
  await assert.rejects(createBookSourceArchive(project(), { signal: early.signal }), { code: "EXPORT_CANCELLED" });
  const abort = new AbortController();
  const pending = createBookSourceArchive(project([{ path: "large.md", source: "x".repeat(1024 * 1024) }]), { ...options, signal: abort.signal });
  const rejected = assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  setTimeout(() => abort.abort(), 0); await rejected;
  assert.equal((await createBookSourceArchive(project(), options)).sourceCount, 3);
});

test("shared writer checks payload metadata and final size before allocating the archive", async () => {
  const blob = new Blob(["hello"]), check = () => {}, wait = promise => promise;
  const crc = await bookZipCrc32(blob, { check, wait });
  assert.throws(() => createBookZip([{ name: "hello.txt", blob, size: 4, crc }], 1000), { code: "INVALID_ZIP_ENTRY" });
  assert.throws(() => createBookZip([{ name: "hello.txt", blob, size: 5, crc }], 22), { code: "OUTPUT_LIMIT" });
  assert.throws(() => createBookZip([{ name: "hello.txt", blob, size: 5, crc: -1 }], 1000), { code: "INVALID_ZIP_ENTRY" });
  assert.equal((await readSourceZip(createBookZip([{ name: "hello.txt", blob, size: 5, crc }], 1000))).size, 1);
});

test("factoring the shared writer preserves the previously emitted edition ZIP byte for byte", async () => {
  const formats = ["pdf", "epub", "site"], types = ["application/pdf", "application/epub+zip", "application/zip"];
  const payloads = [[37, 80, 68, 70, 45, 0, 255, 128], [80, 75, 3, 4, 254, 0, 128], [80, 75, 3, 4, 42, 255]];
  const result = await createBookEditionBundle(formats.map((format, i) => ({ format, blob: new Blob([new Uint8Array(payloads[i])], { type: types[i] }) })), { sourceLength: 27 });
  const bytes = new Uint8Array(await result.blob.arrayBuffer());
  // Recorded from the unmodified writer before extraction; independent format
  // checks remain in book_edition_bundle.test.mjs, with every old assertion.
  assert.equal(bytes.length, 1150);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), "db1bea6fe7ef2d2cb7c9f6711e2ffac401300c97869ba119381142ba3595c2e6");
});
