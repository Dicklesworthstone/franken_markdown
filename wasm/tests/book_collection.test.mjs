import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import {
  bookPath,
  createBookCollection,
  BOOK_WORKBENCH_LIMITS as L,
  readBookFiles,
  readBookProject,
} from "../demo/book_collection.mjs";

const code = (expected) => (error) => error.code === expected;
const chapter = (path = "first.md", source = "# First") => ({ path, source });
const append = (book, chapters = [chapter()], images = []) => book.append({ chapters, images });
test("ordered editing, movement and deletion feed one engine snapshot", () => {
  const book = createBookCollection();
  append(book, [chapter(), chapter("end.md", "# End")]);
  book.edit(0, "guide/start.md", "# Edited");
  assert.equal(book.move(0, 1), 1);
  assert.deepEqual(book.snapshot().files, [
    chapter("end.md", "# End"),
    chapter("guide/start.md", "# Edited"),
  ]);
  book.remove(0);
  assert.equal(book.snapshot().files.length, 1);
  book.remove(0);
  assert.throws(() => book.snapshot(), code("EMPTY_BOOK"));
});
test("failed imports are atomic across source and image ownership", () => {
  const book = createBookCollection(),
    bytes = new Uint8Array([1]);
  append(book);
  const before = book.revision;
  assert.throws(
    () => append(book, [chapter()], [{ destination: "x.png", bytes }]),
    code("DUPLICATE_PATH"),
  );
  assert.equal(book.revision, before);
  assert.equal(book.images.length, 0);
  assert.throws(
    () => append(book, [chapter("new.md")], [{ destination: "bad.gif", bytes }]),
    code("INVALID_IMAGE"),
  );
  assert.equal(book.files.length, 1);
  append(book, [], [{ destination: "x.png", bytes }]);
  bytes[0] = 9;
  const snapshot = book.snapshot();
  assert.equal(snapshot.options.images[0].bytes[0], 1);
  snapshot.options.images[0].bytes[0] = 5;
  assert.equal(book.snapshot().options.images[0].bytes[0], 1);
});
test("source projects round-trip original bytes, order and settings but never image grants", async () => {
  const book = createBookCollection(),
    source = "\ufeff# A\r\nB\rC\n😀";
  append(
    book,
    [chapter("z.md", source), chapter("a.md")],
    [{ destination: "x.svg", bytes: new Uint8Array([60]) }],
  );
  book.configure({ title: "A manual", lang: "fr", font: "serif", toc: true, pageNumbers: false });
  const project = await readBookProject(
    new File([book.projectDownload().blob], "book.fmdbook.json"),
  );
  assert(!JSON.stringify(project).includes("x.svg"));
  const other = createBookCollection();
  append(other, [chapter()], [{ destination: "old.png", bytes: new Uint8Array([1]) }]);
  other.replaceProject(project);
  assert.deepEqual(other.files, book.files);
  assert.deepEqual(other.options, book.options);
  assert.equal(other.images.length, 0);
  assert.deepEqual(
    new Uint8Array(await other.chapterDownload(0).blob.arrayBuffer()),
    new TextEncoder().encode(source),
  );
});
test("textarea newline normalization preserves untouched imports and undo-to-original bytes", async () => {
  const book = createBookCollection(),
    original = "\ufeffA\r\nB\r";
  append(book, [chapter("a.md", original)]);
  book.edit(0, "a.md", "\ufeffA\nB\n");
  assert.equal(book.files[0].source, original);
  book.edit(0, "a.md", "changed");
  assert.equal(book.files[0].source, "changed");
  book.edit(0, "a.md", "\ufeffA\nB\n");
  assert.deepEqual(
    new Uint8Array(await book.chapterDownload(0).blob.arrayBuffer()),
    new TextEncoder().encode(original),
  );
});
test("temporarily invalid typed source is retained, with all publication paths refused", () => {
  const book = createBookCollection();
  append(book);
  book.edit(0, "first.md", "invalid\ud800");
  assert.equal(book.files[0].source, "invalid\ud800");
  for (const operation of [
    () => book.snapshot(),
    () => book.projectDownload(),
    () => book.chapterDownload(0),
  ])
    assert.throws(operation);
  book.edit(0, "first.md", "repaired");
  assert.equal(book.snapshot().files[0].source, "repaired");
});
test("invalid or authority-bearing projects leave the existing collection untouched", () => {
  const book = createBookCollection();
  append(book);
  const revision = book.revision;
  for (const project of [
    {},
    { schemaVersion: 3, files: [] },
    { schemaVersion: 1, files: [chapter()], options: { images: [] } },
    { schemaVersion: 1, files: [chapter(), chapter()] },
    { schemaVersion: 1, files: [chapter("../a.md")] },
  ])
    assert.throws(() => book.replaceProject(project));
  assert.equal(book.revision, revision);
  assert.deepEqual(book.files, [chapter()]);
});
test("folder reading preserves nested keys beneath one root and counts ignored files", async () => {
  const file = (relative, source) => {
    const value = new File([source], relative.split("/").at(-1));
    Object.defineProperty(value, "webkitRelativePath", { value: relative });
    return value;
  };
  const result = await readBookFiles(
    [
      file("root/guide/start.md", "\ufeff# Hello\r\n"),
      file("root/guide/figure.svg", "<svg/>"),
      file("root/private.txt", "ignored"),
    ],
    { folder: true },
  );
  assert.deepEqual(result.chapters, [chapter("guide/start.md", "\ufeff# Hello\r\n")]);
  assert.equal(result.images[0].destination, "guide/figure.svg");
  assert.equal(result.ignored, 1);
  await assert.rejects(
    readBookFiles([file("root/a.md", ""), file("other/b.md", "")], { folder: true }),
    code("INVALID_PATH"),
  );
});
test("all byte and count admission precedes physical file reads", async () => {
  let reads = 0;
  class LargeFile extends File {
    get size() {
      return L.chapterBytes + 1;
    }
    async arrayBuffer() {
      reads++;
      return super.arrayBuffer();
    }
  }
  await assert.rejects(readBookFiles([new LargeFile(["x"], "a.md")]), code("FILE_LIMIT"));
  assert.equal(reads, 0);
  class Counted extends File {
    async arrayBuffer() {
      reads++;
      return super.arrayBuffer();
    }
  }
  await assert.rejects(
    readBookFiles(Array.from({ length: L.chapters + 1 }, (_, i) => new Counted(["x"], `${i}.md`))),
    code("BOOK_LIMIT"),
  );
  assert.equal(reads, 0);
});
test("malformed UTF-8, duplicate paths and invalid project encoding fail explicitly", async () => {
  await assert.rejects(
    readBookFiles([new File([new Uint8Array([0xff])], "a.md")]),
    code("INVALID_UNICODE"),
  );
  await assert.rejects(
    readBookFiles([new File(["A"], "a.md"), new File(["B"], "a.md")]),
    code("DUPLICATE_PATH"),
  );
  await assert.rejects(
    readBookProject(new File([new Uint8Array([0xff])], "x.json")),
    code("INVALID_PROJECT"),
  );
});
test("canonical paths reject traversal, URLs, reserved characters and malformed Unicode", () => {
  for (const path of [
    "",
    "/a.md",
    "../a.md",
    "a/./b.md",
    "C:\\a.md",
    "https://x/a.md",
    "a#b.md",
    "a%20b.md",
    " a.md",
    "a\n.md",
    "\ud800.md",
  ])
    assert.throws(() => bookPath(path));
  assert.equal(bookPath("guide/中文.md"), "guide/中文.md");
});
test("aggregate source and project serialization remain bounded", async () => {
  const book = createBookCollection(),
    source = "a".repeat(L.chapterBytes);
  assert.throws(
    () =>
      append(
        book,
        Array.from({ length: 4 }, (_, i) => chapter(`${i}.md`, source)),
      ),
    code("BOOK_LIMIT"),
  );
  append(book, [chapter("a.md", "\u0001".repeat(L.chapterBytes))]);
  const output = book.projectDownload();
  assert(output.blob.size <= L.projectBytes);
  assert.equal(
    (await readBookProject(new File([output.blob], "x.json"))).files[0].source.length,
    L.chapterBytes,
  );
});
test("revocation invalidates snapshots and disposal releases the collection", () => {
  const book = createBookCollection();
  append(book, [chapter()], [{ destination: "x.png", bytes: new Uint8Array([1]) }]);
  let changes = 0;
  book.subscribe(() => {
    changes++;
  });
  book.revokeImages();
  assert.equal(changes, 1);
  assert.equal(book.snapshot().options.images.length, 0);
  book.dispose();
  book.dispose();
  assert.throws(() => book.files, code("SESSION_DISPOSED"));
});
