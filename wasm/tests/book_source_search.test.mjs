// Production source search/planning/collection; no renderer or DOM substitutes.

import assert from "node:assert/strict";
import test from "node:test";
import { BOOK_WORKBENCH_LIMITS as B, createBookCollection } from "../demo/book_collection.mjs";
import {
  bookEditorOffset,
  findBookSource,
  planBookReplacement,
} from "../demo/book_source_search.mjs";

const project = (sources, paths = sources.map((_, i) => `chapter-${i}.md`)) => ({
  schemaVersion: 1,
  files: sources.map((source, i) => ({ path: paths[i], source })),
});
function collection(t, sources = ["cat cat", "Cat cat"]) {
  const model = createBookCollection();
  model.replaceProject(project(sources));
  t.after(() => model.dispose());
  return model;
}
test("search returns all non-overlapping matches in explicit reading order", () => {
  const result = findBookSource(project(["banana banana", "banana"], ["z.md", "a.md"]), "ana");
  assert.deepEqual(
    result.matches.map((m) => [m.chapter, m.start, m.end, m.line, m.column]),
    [
      [0, 1, 4, 1, 2],
      [0, 8, 11, 1, 9],
      [1, 1, 4, 1, 2],
    ],
  );
  assert.equal(result.files[0].path, "z.md");
  assert(Object.isFrozen(result.matches[0]));
  assert(Object.isFrozen(result.files));
});
test("regex metacharacters and replacement tokens are ordinary literal text", () => {
  const needle = ".*+?^${}()|[]\\";
  const result = findBookSource(project([needle + " and " + needle]), needle);
  assert.equal(result.matches.length, 2);
  assert.equal(planBookReplacement(result, "$&$1\\n").after[0].source, "$&$1\\n and $&$1\\n");
});
test("Unicode case folding reports ORIGINAL offsets without lossy lowercase copies", () => {
  const result = findBookSource(project(["İi I 𐐀𐐨 Kk"]), "𐐨", { matchCase: false });
  assert.deepEqual(
    result.matches.map((m) => [m.start, m.end, m.column]),
    [
      [5, 7, 6],
      [7, 9, 7],
    ],
  );
  assert.equal(planBookReplacement(result, "x").after[0].source, "İi I xx Kk");
  assert.equal(findBookSource(project(["İi I"]), "i", { matchCase: false }).matches.length, 2);
  assert.equal(findBookSource(project(["Cat cat"]), "cat").matches.length, 1);
});
test("line/column and source offsets handle BOM, emoji, CRLF, CR and LF", () => {
  const source = "\ufeff😀x\r\nx\rx\nx";
  const result = findBookSource(project([source]), "x");
  assert.deepEqual(
    result.matches.map((m) => [m.start, m.line, m.column]),
    [
      [3, 1, 3],
      [6, 2, 1],
      [8, 3, 1],
      [10, 4, 1],
    ],
  );
  assert.deepEqual(
    result.matches.map((m) => bookEditorOffset(source, m.start)),
    [3, 5, 7, 9],
  );
});
test("multiline query matches all newline conventions without normalizing unmatched bytes", () => {
  const result = findBookSource(
    project(["\ufeffkeep\r\na\r\nb\r\nend\nx", "a\nb", "a\rb"]),
    "a\nb",
  );
  const plan = planBookReplacement(result, "c\nd");
  assert.deepEqual(
    plan.after.map((f) => f.source),
    ["\ufeffkeep\r\nc\r\nd\r\nend\nx", "c\nd", "c\rd"],
  );
  assert.equal(plan.count, 3);
  assert.equal(plan.beforeBytes, plan.afterBytes);
});
test("chapter scope and single-match replacement leave every other match untouched", () => {
  const input = project(["cat cat", "cat cat"]);
  const scoped = findBookSource(input, "cat", { chapter: 1 });
  assert.equal(scoped.matches.length, 2);
  assert(scoped.matches.every((m) => m.chapter === 1));
  const plan = planBookReplacement(scoped, "dog", 1);
  assert.deepEqual(
    plan.after.map((f) => f.source),
    ["cat cat", "cat dog"],
  );
  assert.deepEqual(
    plan.chapters.map((c) => [c.index, c.count]),
    [[1, 1]],
  );
});
test("deletion and adjacent replacements preserve the exact remainder", () => {
  const result = findBookSource(project(["😀😀😀tail"]), "😀");
  assert.equal(planBookReplacement(result, "").after[0].source, "tail");
  assert.equal(planBookReplacement(result, "😀x").after[0].source, "😀x😀x😀xtail");
});
test("search snapshots do not retain mutable project containers", () => {
  const input = project(["cat"]);
  const result = findBookSource(input, "cat");
  input.files[0].source = "dog";
  input.files.push({ path: "b.md", source: "cat" });
  const plan = planBookReplacement(result, "new");
  assert.equal(plan.before[0].source, "cat");
  assert(Object.isFrozen(plan.after[0]));
  assert.throws(() => {
    plan.after[0].source = "hijack";
  }, TypeError);
});
test("empty, malformed, oversized and invalid-scope searches fail explicitly", () => {
  for (const query of ["", "bad\ud800", "x".repeat(4097)])
    assert.throws(() => findBookSource(project(["text"]), query));
  for (const chapter of [-1, 1, 0.1, "0"])
    assert.throws(() => findBookSource(project(["text"]), "x", { chapter }));
  assert.throws(() => findBookSource(project(["text"]), "x", { matchCase: "yes" }));
  assert.throws(() => findBookSource(project(["bad\udfff"]), "bad"));
});
test("the 10,000-result limit never returns an incomplete replace-all set", () => {
  assert.equal(findBookSource(project(["x".repeat(10000)]), "x").matches.length, 10000);
  assert.throws(() => findBookSource(project(["x".repeat(10001)]), "x"), { code: "SEARCH_LIMIT" });
  assert.throws(() => findBookSource(project(["x".repeat(6000), "x".repeat(6000)]), "x"), {
    code: "SEARCH_LIMIT",
  });
});
test("replacement rejects forged searches, bad selectors and malformed or oversized input", () => {
  const result = findBookSource(project(["a"]), "a");
  assert.throws(() => planBookReplacement({ ...result }, "b"), { code: "INVALID_SEARCH" });
  for (const replacement of ["bad\ud800", "x".repeat(65537)])
    assert.throws(() => planBookReplacement(result, replacement));
  for (const index of [-1, 1, 0.5, "0"])
    assert.throws(() => planBookReplacement(result, "b", index));
});
test("a no-op is not a mutation, including case-insensitive partial no-ops", () => {
  assert.throws(() => planBookReplacement(findBookSource(project(["abc"]), "x"), "y"), {
    code: "NO_CHANGE",
  });
  assert.throws(() => planBookReplacement(findBookSource(project(["abc"]), "a"), "a"), {
    code: "NO_CHANGE",
  });
  const plan = planBookReplacement(
    findBookSource(project(["cat Cat"]), "cat", { matchCase: false }),
    "cat",
  );
  assert.equal(plan.count, 1);
  assert.equal(plan.after[0].source, "cat cat");
});
test("chapter expansion is rejected before installing any earlier replacement", (t) => {
  const model = collection(t, ["needle", "x".repeat(B.chapterBytes - 6) + "needle"]);
  const before = model.files,
    revision = model.revision;
  assert.throws(
    () => planBookReplacement(findBookSource(model.project(), "needle"), "a longer replacement"),
    { code: "BOOK_LIMIT" },
  );
  assert.deepEqual(model.files, before);
  assert.equal(model.revision, revision);
});
test("aggregate output admission includes paths and all chapters", () => {
  const source = "x".repeat(B.chapterBytes - 100) + "needle";
  const result = findBookSource(project(Array(4).fill(source)), "needle");
  // Each chapter fits with this replacement; their combined paths/source do not.
  assert.throws(() => planBookReplacement(result, "y".repeat(100)), { code: "BOOK_LIMIT" });
});
test("transaction installs every chapter together, preserves settings/images, and notifies once", (t) => {
  const model = collection(t);
  model.append({
    chapters: [],
    images: [{ destination: "figure.svg", bytes: new Uint8Array([1, 2, 3]) }],
  });
  const settings = model.options,
    before = model.files,
    revision = model.revision,
    seen = [];
  model.subscribe(() => seen.push(model.files.map((f) => f.source)));
  const plan = planBookReplacement(
    findBookSource(model.project(), "cat", { matchCase: false }),
    "dog",
  );
  assert.equal(model.replaceSources(plan.after, revision), revision + 1);
  assert.deepEqual(seen, [["dog dog", "dog dog"]]);
  assert.deepEqual(model.options, settings);
  assert.deepEqual(model.snapshot().options.images[0].bytes, new Uint8Array([1, 2, 3]));
  model.replaceSources(plan.before, model.revision);
  assert.deepEqual(model.files, before);
});
test("validation failures cannot partially mutate a source transaction", (t) => {
  const model = collection(t),
    before = model.files,
    revision = model.revision;
  const bad = [
    [
      { ...before[0], source: "new" },
      { ...before[1], source: "bad\ud800" },
    ],
    [
      { ...before[0], source: "new" },
      { ...before[1], path: "renamed.md" },
    ],
    [...before].reverse(),
    before.slice(0, 1),
    [...before, { path: "new.md", source: "" }],
  ];
  for (const files of bad) {
    assert.throws(() => model.replaceSources(files, revision));
    assert.deepEqual(model.files, before);
    assert.equal(model.revision, revision);
  }
});
test("stale revisions, including external changes during validation, never overwrite new source", (t) => {
  const model = collection(t),
    revision = model.revision,
    old = model.files;
  model.edit(0, old[0].path, "newer");
  assert.throws(() => model.replaceSources(old, revision), { code: "STALE_SOURCE" });
  assert.equal(model.files[0].source, "newer");
  const current = model.revision;
  const input = model.files;
  Object.defineProperty(input[0], "source", {
    get() {
      model.edit(0, old[0].path, "newest");
      return "stale";
    },
  });
  assert.throws(() => model.replaceSources(input, current), { code: "STALE_SOURCE" });
  assert.equal(model.files[0].source, "newest");
});
test("transaction and no-op preserve imported line endings on subsequent untouched editor capture", (t) => {
  const model = collection(t, ["\ufeffcat\r\nnext\r\n", "second\r\n"]),
    revision = model.revision;
  assert.equal(model.replaceSources(model.files, revision), revision);
  const plan = planBookReplacement(findBookSource(model.project(), "cat"), "dog");
  model.replaceSources(plan.after, revision);
  for (const [i, file] of model.files.entries())
    model.edit(i, file.path, file.source.replace(/\r\n?/g, "\n"));
  assert.deepEqual(
    model.files.map((f) => f.source),
    ["\ufeffdog\r\nnext\r\n", "second\r\n"],
  );
  assert.equal(model.revision, revision + 1);
});
test("mutable transaction inputs cannot mutate installed source afterward", (t) => {
  const model = collection(t),
    input = model.files;
  input[0].source = "new";
  model.replaceSources(input, model.revision);
  input[0].source = "later";
  input.length = 0;
  assert.equal(model.files[0].source, "new");
  assert.equal(model.files.length, 2);
});
test("an observer exception cannot roll back an installed transaction", (t) => {
  const model = collection(t);
  model.subscribe(() => {
    throw new Error("observer");
  });
  const input = model.files;
  input[0].source = "changed";
  const revision = model.revision;
  assert.equal(model.replaceSources(input, revision), revision + 1);
  assert.equal(model.files[0].source, "changed");
});
test("editor offset conversion rejects broken surrogate selections and bad ranges", () => {
  for (const value of [-1, 3, 0.5, "0", 1]) assert.throws(() => bookEditorOffset("😀", value));
  assert.equal(bookEditorOffset("😀", 2), 2);
});
test("disposed collections reject source transactions", (t) => {
  const model = collection(t),
    revision = model.revision,
    files = model.files;
  model.dispose();
  assert.throws(() => model.replaceSources(files, revision), { code: "SESSION_DISPOSED" });
});
test("multiline literal matching never backtracks CRLF into two newline tokens", () => {
  const source = "\r\n".repeat(200) + "tail";
  assert.equal(findBookSource(project([source]), "\n".repeat(80) + "missing").matches.length, 0);
  const result = findBookSource(project(["\r\nx"]), "\n\nx");
  assert.equal(result.matches.length, 0);
});
