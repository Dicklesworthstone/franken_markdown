// Production collection, project codec, search and transport; no Rust renderer
// or native browser is substituted by these tests. Worker engines are explicit doubles.

import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import { createBookBindings } from "../book_session.mjs";
import { installBookWorker } from "../book_worker.mjs";
import {
  createBookCollection,
  BOOK_WORKBENCH_LIMITS as L,
  normalizeBookProject,
  readBookFiles,
  readBookProject,
  serializeBookProject,
} from "../demo/book_collection.mjs";
import { findBookSource, planBookReplacement } from "../demo/book_source_search.mjs";

const file = (path, source = "# Source\n") => ({ path, source });
const include = (path, source) => ({ ...file(path, source), role: "include" });
function collection() {
  const book = createBookCollection();
  book.append({
    chapters: [
      file("guide/start.md", "# Start\n\n{{#include ../parts/shared.md}}\n"),
      file("end.md"),
    ],
    includeSources: [file("parts/shared.md", "\ufeffShared 😀\r\nsecond\r")],
    images: [{ destination: "x.svg", bytes: new Uint8Array([60]) }],
  });
  return book;
}
test("publication partitions mixed editor order without promoting snippets", () => {
  const book = collection();
  book.move(2, -1);
  const input = book.snapshot();
  assert.deepEqual(
    input.files.map((f) => f.path),
    ["guide/start.md", "end.md"],
  );
  assert.deepEqual(input.options.includeSources, [
    file("parts/shared.md", "\ufeffShared 😀\r\nsecond\r"),
  ]);
  assert.equal(book.files[1].role, "include");
  input.options.includeSources[0].source = "mutated";
  assert.notEqual(book.snapshot().options.includeSources[0].source, "mutated");
});
test("schema 2 source download and recovery preserve roles, order and original bytes", async () => {
  const book = collection();
  book.move(2, -1);
  const project = await readBookProject(
    new File([book.projectDownload().blob], "book.fmdbook.json"),
  );
  assert.equal(project.schemaVersion, 2);
  assert.equal(project.files[1].role, "include");
  assert.equal(JSON.stringify(project).includes("x.svg"), false);
  const restored = collection();
  restored.replaceProject(project);
  assert.deepEqual(restored.files, book.files);
  assert.equal(restored.images.length, 0);
  assert.deepEqual(
    restored.snapshot().options.includeSources,
    book.snapshot().options.includeSources,
  );
  restored.edit(1, "parts/shared.md", "\ufeffShared 😀\nsecond\n");
  assert.deepEqual(restored.files, book.files);
  assert.deepEqual(
    new Uint8Array(await restored.chapterDownload(1).blob.arrayBuffer()),
    new TextEncoder().encode(book.files[1].source),
  );
});
test("legacy schema 1 remains round-trippable and does not silently accept roles", () => {
  const project = normalizeBookProject({ schemaVersion: 1, files: [file("a.md")] });
  assert.equal(JSON.parse(serializeBookProject(project)).schemaVersion, 1);
  assert.throws(() => normalizeBookProject({ ...project, files: [include("a.md")] }), {
    code: "INVALID_PROJECT",
  });
  for (const role of ["asset", "", null, true])
    assert.throws(
      () => normalizeBookProject({ schemaVersion: 2, files: [{ ...file("a.md"), role }] }),
      { code: "INVALID_ROLE" },
    );
});
test("role changes are atomic, reversible, and retain source bytes and image authority", () => {
  const book = collection(),
    before = book.files,
    image = book.snapshot().options.images;
  let changed = 0;
  book.subscribe(() => changed++);
  book.setRole(1, "include");
  assert.equal(book.snapshot().files.length, 1);
  book.setRole(1, "include");
  assert.equal(changed, 1);
  book.setRole(1, "chapter");
  assert.deepEqual(book.files, before);
  assert.deepEqual(book.snapshot().options.images, image);
  book.setRole(0, "include");
  book.setRole(1, "include");
  assert.throws(() => book.snapshot(), { code: "EMPTY_BOOK" });
  assert.equal(book.project().files.length, 3, "include-only drafts remain saveable");
});
test("non-Markdown includes are downloadable but cannot become chapters until renamed", async () => {
  const book = collection();
  book.append({
    chapters: [],
    images: [],
    includeSources: [file("code/example.rs", "fn main() {}\r\n")],
  });
  const revision = book.revision;
  assert.throws(() => book.setRole(3, "chapter"), { code: "INVALID_PATH" });
  assert.equal(book.revision, revision);
  const output = book.chapterDownload(3);
  assert.equal(output.filename, "example.rs");
  assert.equal(output.blob.type, "text/plain; charset=utf-8");
  book.edit(3, "bad/path/../", "still retained");
  assert.equal(book.chapterDownload(3).filename, "source-4.txt");
});
test("chapter/include path collisions and failed resource imports install nothing", () => {
  const book = collection(),
    project = book.project(),
    images = book.images,
    revision = book.revision;
  for (const resource of [
    file("end.md"),
    file("parts/shared.md"),
    file("../escape.txt"),
    file("a.txt", "bad\ud800"),
  ]) {
    assert.throws(() =>
      book.append({ chapters: [file("new.md")], includeSources: [resource], images: [] }),
    );
    assert.equal(book.revision, revision);
    assert.deepEqual(book.project(), project);
    assert.deepEqual(book.images, images);
  }
});
test("explicit include imports read UTF-8 source without granting image authority", async () => {
  const result = await readBookFiles(
    [new File(["fn main() {}\r\n"], "code.rs"), new File(["<svg/>"], "x.svg")],
    { role: "include" },
  );
  assert.equal(result.chapters.length, 0);
  assert.equal(result.images.length, 0);
  assert.deepEqual(result.includeSources, [
    file("code.rs", "fn main() {}\r\n"),
    file("x.svg", "<svg/>"),
  ]);
  await assert.rejects(
    readBookFiles([new File([new Uint8Array([255])], "binary.dat")], { role: "include" }),
    { code: "INVALID_UNICODE" },
  );
  const normal = await readBookFiles([new File(["unselected"], "code.rs")]);
  assert.equal(normal.ignored, 1);
});
test("include folder imports retain relative paths and reject multiple roots", async () => {
  const local = (path) => {
    const f = new File(["source"], path.split("/").at(-1));
    Object.defineProperty(f, "webkitRelativePath", { value: path });
    return f;
  };
  const result = await readBookFiles([local("root/code/x.rs")], { folder: true, role: "include" });
  assert.equal(result.includeSources[0].path, "code/x.rs");
  await assert.rejects(
    readBookFiles([local("a/x.txt"), local("b/y.txt")], { folder: true, role: "include" }),
    { code: "INVALID_PATH" },
  );
});
test("resource admission shares source bytes and rejects excess counts before file reads", async () => {
  let reads = 0;
  class Counted extends File {
    async arrayBuffer() {
      reads++;
      return super.arrayBuffer();
    }
  }
  await assert.rejects(
    readBookFiles(
      Array.from({ length: L.includeSources + 1 }, (_, i) => new Counted(["x"], `${i}.txt`)),
      { role: "include" },
    ),
    { code: "BOOK_LIMIT" },
  );
  assert.equal(reads, 0);
  const book = collection(),
    before = book.revision,
    large = "x".repeat(L.chapterBytes);
  assert.throws(
    () =>
      book.append({
        chapters: [],
        images: [],
        includeSources: Array.from({ length: 4 }, (_, i) => file(`${i}.txt`, large)),
      }),
    { code: "BOOK_LIMIT" },
  );
  assert.equal(book.revision, before);
});
test("each role has its own count ceiling while sharing one aggregate budget", () => {
  const book = createBookCollection();
  book.append({
    chapters: Array.from({ length: 128 }, (_, i) => file(`${i}.md`, "")),
    includeSources: Array.from({ length: 128 }, (_, i) => file(`${i}.txt`, "")),
    images: [],
  });
  assert.equal(book.files.length, 256);
  assert.equal(book.snapshot().files.length, 128);
  assert.throws(() => book.setRole(0, "include"), { code: "BOOK_LIMIT" });
});
test("search, replace and undo preserve include roles and imported newline conventions", () => {
  const book = collection(),
    search = findBookSource(book.project(), "Shared"),
    revision = book.revision;
  assert.equal(search.matches[0].chapter, 2);
  const plan = planBookReplacement(search, "Common\ntext");
  assert.equal(plan.after[2].role, "include");
  book.replaceSources(plan.after, revision);
  assert.match(book.snapshot().options.includeSources[0].source, /Common\r\ntext/);
  book.replaceSources(plan.before, book.revision);
  assert.deepEqual(book.files, search.files);
});
test("source-only transactions cannot accidentally promote or demote a file", () => {
  const book = collection(),
    revision = book.revision;
  const changed = book.files.map((f) => ({ path: f.path, source: f.source }));
  assert.throws(() => book.replaceSources(changed, revision), { code: "INVALID_TRANSACTION" });
  assert.equal(book.revision, revision);
  const files = book.files;
  files[0].role = "include";
  assert.throws(() => book.replaceSources(files, revision), { code: "INVALID_TRANSACTION" });
});
for (const format of ["pdf", "epub", "site", "preview"])
  test(`${format} worker receives only chapters plus explicit includeSources`, async () => {
    const book = collection(),
      input = book.snapshot();
    let receive, reply;
    const scope = {
      addEventListener(type, listener) {
        receive = listener;
      },
      postMessage(message) {
        reply = message;
      },
    };
    const method = {
      pdf: "renderBookPdf",
      epub: "renderBookEpub",
      site: "renderBookSite",
      preview: "renderBookPreview",
    }[format];
    installBookWorker(scope, {
      async [method](files, options) {
        assert.deepEqual(files, input.files);
        assert.deepEqual(options.includeSources, input.options.includeSources);
        return { bytes: new Uint8Array([1]), sourceLength: 10 };
      },
    });
    await receive({
      data: {
        schemaVersion: 1,
        id: 1,
        files: input.files,
        options: input.options,
        format,
        maxOutputBytes: 100,
      },
    });
    assert.equal(reply.error, undefined);
    assert.deepEqual([...reply.bytes], [1]);
  });
test("workbench source snapshot reaches the production facade expansion constructor", async () => {
  const book = collection(),
    input = book.snapshot();
  let captured,
    frees = 0;
  class Engine {
    static fromSources(...args) {
      captured = args;
      return new Engine();
    }
    setMetadata() {}
    setCustomCss() {}
    setTheme() {}
    setNavigation() {}
    setFontScale() {}
    setImage() {}
    renderPdf() {
      return new Uint8Array([37]);
    }
    sourceLength = 10;
    free() {
      frees++;
    }
  }
  const api = createBookBindings(async () => Engine);
  await api.renderBookPdf(input.files, input.options);
  assert.deepEqual(captured[0], ["guide/start.md", "end.md"]);
  assert.deepEqual(captured[2], ["parts/shared.md"]);
  assert.equal(frees, 1);
});
