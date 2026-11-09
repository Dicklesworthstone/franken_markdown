import assert from "node:assert/strict";
import test from "node:test";
import { readBookSourceFolder } from "../demo/book_source_folder.mjs";
import { createBookCollection } from "../demo/book_collection.mjs";
import { codec, sample, selectedFile, exportedFolder, deferred } from "./book_source_folder_helpers.mjs";

const open = (files, options = {}) => readBookSourceFolder(files, { projectCodec: codec, ...options });
const tick = () => new Promise(resolve => setImmediate(resolve));
const manifest = project => selectedFile("book.fmdbook.json", codec.serializeBookProject(project));

test("exported source tree round-trips current file bytes, manifest order, roles and applied settings", async () => {
  const original = sample(), edited = "\ufeff# Edited café 日本 😀\r\n\rSecond line\n";
  const files = await exportedFolder(original, { "sources/guide/start.md": edited,
    "sources/parts/shared.txt": "Shared edit\r\n" });
  let ignoredReads = 0;
  const ignored = ["images/plot#1.svg", "images/scale%100.svg", "images/CON.png"].map(path => selectedFile(path, "not an authorized resource"));
  for (const file of ignored) Object.defineProperty(file, "arrayBuffer", { value() { ignoredReads++; throw Error("Do not read unrelated files"); } });
  files.push(...ignored, selectedFile("notes.md", "Not in this book"));
  const result = await open(files);
  assert.equal(result.folder, "Edited book 😀"); assert.equal(result.changedSources, 2);
  assert.equal(result.chapters, 2); assert.equal(result.includeSources, 1); assert.equal(result.ignored, 4);
  assert.equal(ignoredReads, 0);
  const expected = codec.normalizeBookProject(original);
  expected.files[0].source = edited; expected.files[1].source = "Shared edit\r\n";
  assert.deepEqual(result.project, expected);
  assert.deepEqual(result.review.map(item => [item.path, item.role, item.changed]), [
    ["guide/start.md", "chapter", true], ["parts/shared.txt", "include", true], ["last.md", "chapter", false],
  ]);
  const collection = createBookCollection(); collection.replaceProject(result.project);
  assert.deepEqual(collection.project(), expected); assert.deepEqual(collection.images, []); assert.deepEqual(collection.fonts, []);
  collection.dispose();
});

test("unmodified exported bytes and a source named like the manifest remain distinct", async () => {
  const project = { schemaVersion: 2, files: [{ path: "book.fmdbook.json", source: "\ufeffliteral\rtext\r\n", role: "include" }], options: {} };
  const result = await open(await exportedFolder(project));
  assert.equal(result.changedSources, 0); assert.equal(result.chapters, 0); assert.equal(result.includeSources, 1);
  assert.deepEqual(result.project, codec.normalizeBookProject(project));
});

test("missing manifest, nested wrong root and missing current source refuse fallback to embedded source", async () => {
  const files = await exportedFolder();
  await assert.rejects(open(files.filter(file => !file.webkitRelativePath.endsWith("/book.fmdbook.json"))), { code: "SOURCE_MANIFEST_MISSING" });
  await assert.rejects(open(files.map(file => selectedFile("extra/" + file.webkitRelativePath.split("/").slice(1).join("/"), file))), { code: "SOURCE_MANIFEST_MISSING" });
  const missing = files.filter(file => !file.webkitRelativePath.endsWith("sources/parts/shared.txt"));
  await assert.rejects(open(missing), error => error.code === "SOURCE_FILE_MISSING" && /sources\/parts\/shared.txt/.test(error.message));
});

test("required paths reject duplicates, case/NFC aliases and file-directory ambiguity before source reads", async () => {
  const project = { schemaVersion: 1, files: [{ path: "café.md", source: "" }], options: {} };
  for (const pair of [["sources/café.md", "sources/café.md"], ["sources/café.md", "Sources/CAFÉ.md"],
    ["sources/café.md", "sources/cafe\u0301.md"], ["sources/café.md", "sources/café.md/child.txt"],
    ["sources/café.md", "sources"], ["sources/café.md", "sources./café.md"],
    ["sources/café.md", "sources/café.md."]]) {
    let reads = 0;
    const sources = pair.map(path => selectedFile(path, "source"));
    for (const file of sources) Object.defineProperty(file, "arrayBuffer", { value() { reads++; throw Error("Premature source read"); } });
    await assert.rejects(open([manifest(project), ...sources]), { code: "PATH_COLLISION" }); assert.equal(reads, 0);
  }
  for (const path of ["sources/../escape.md", "sources/a\\b.md"])
    await assert.rejects(open([manifest(sample()), selectedFile(path, "source")]), { code: "INVALID_PATH" });
  for (const path of ["CON.md", "end./a.md"]) {
    const invalid = { schemaVersion: 1, files: [{ path, source: "" }], options: {} };
    // A manifest is ordinary untrusted JSON; portable admission happens before
    // any declared file could be read, even for legacy project paths.
    await assert.rejects(open([selectedFile("book.fmdbook.json", JSON.stringify(invalid))]), { code: "INVALID_PATH" });
  }
});

test("manifest alias or file-directory ambiguity is rejected before manifest IO", async () => {
  for (const path of ["book.fmdbook.json", "BOOK.FMDBOOK.JSON", "book.fmdbook.json/child.txt", "book.fmdbook.json."]) {
    let reads = 0;
    const file = manifest(sample());
    Object.defineProperty(file, "arrayBuffer", { value() { reads++; throw Error("Premature manifest read"); } });
    await assert.rejects(open([file, selectedFile(path, "ignored")]), { code: "PATH_COLLISION" });
    assert.equal(reads, 0);
  }
});

test("manifest-declared aliases fail even when each file could be selected separately", async () => {
  const project = { schemaVersion: 1, files: [{ path: "A.md", source: "A" }, { path: "a.md", source: "a" }], options: {} };
  await assert.rejects(open([manifest(project)]), { code: "PATH_COLLISION" });
});

test("mixed root folders and flat file selections never acquire folder authority", async () => {
  const files = await exportedFolder();
  files.push(selectedFile("extra.txt", "other", "Another root"));
  await assert.rejects(open(files), { code: "INVALID_SOURCE_FOLDER" });
  await assert.rejects(open([new File(["{}"], "book.fmdbook.json")]), { code: "INVALID_SOURCE_FOLDER" });
  await assert.rejects(open({ length: 4097 }), { code: "FILE_LIMIT" });
  await assert.rejects(open([]), { code: "FILE_LIMIT" });
});

test("current byte budgets are checked for the complete required source set before any source read", async () => {
  const project = { schemaVersion: 1, files: Array.from({ length: 5 }, (_, i) => ({ path: `${i}.md`, source: "" })), options: {} };
  let reads = 0;
  const sources = project.files.map(item => {
    const file = selectedFile(`sources/${item.path}`, "");
    Object.defineProperty(file, "size", { value: 4 * 1024 * 1024 });
    Object.defineProperty(file, "arrayBuffer", { value() { reads++; throw Error("Premature source read"); } });
    return file;
  });
  await assert.rejects(open([manifest(project), ...sources]), { code: "BOOK_LIMIT" }); assert.equal(reads, 0);
  const oversized = selectedFile("sources/guide/start.md", "");
  Object.defineProperty(oversized, "size", { value: 4 * 1024 * 1024 + 1 });
  await assert.rejects(open([manifest(sample()), oversized]), { code: "FILE_LIMIT" });
  const hugeManifest = selectedFile("book.fmdbook.json", "{}");
  Object.defineProperty(hugeManifest, "size", { value: 64 * 1024 * 1024 + 1 });
  await assert.rejects(open([hugeManifest]), { code: "FILE_LIMIT" });
});

test("invalid UTF-8 and changed-size reads use the ordinary importer rejection", async () => {
  const invalid = await exportedFolder(sample(), { "sources/parts/shared.txt": new Uint8Array([0xc3, 0x28]) });
  await assert.rejects(open(invalid), { code: "INVALID_UNICODE" });
  const files = await exportedFolder(), source = files.find(file => file.name === "last.md");
  Object.defineProperty(source, "arrayBuffer", { value: async () => new ArrayBuffer(0) });
  await assert.rejects(open(files), { code: "FILE_READ_FAILED" });
  await assert.rejects(open([selectedFile("book.fmdbook.json", '{"schemaVersion":99}')]), { code: "INVALID_PROJECT" });
});

test("selection is snapshotted before awaiting manifest IO", async () => {
  const files = await exportedFolder();
  const pending = open(files); files.fill(selectedFile("different.md", "replacement"));
  const result = await pending;
  assert.deepEqual(result.project, codec.normalizeBookProject(sample()));
});

test("cancelling a pending source read settles immediately and never reads the next file", async () => {
  const files = await exportedFolder(), entered = deferred(), held = deferred();
  const first = files.find(file => file.name === "start.md"), next = files.find(file => file.name === "shared.txt");
  const original = first.arrayBuffer.bind(first); let laterReads = 0;
  Object.defineProperty(first, "arrayBuffer", { value() { entered.resolve(); return held.promise; } });
  Object.defineProperty(next, "arrayBuffer", { value() { laterReads++; throw Error("Retired read continued"); } });
  const abort = new AbortController(), pending = open(files, { signal: abort.signal });
  const rejected = assert.rejects(pending, { code: "IMPORT_CANCELLED" });
  await entered.promise; abort.abort(); await rejected;
  held.resolve(await original()); await tick(); assert.equal(laterReads, 0);
  await assert.rejects(open(files, { signal: abort.signal }), { code: "IMPORT_CANCELLED" });
});
