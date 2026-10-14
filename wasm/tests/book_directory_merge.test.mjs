import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger, BOOK_SOURCE_MERGE_LIMITS } from "../demo/book_source_merge.mjs";
const merge = (...args) => createBookSourceMerger()(...args);

test("unchanged, one-sided and identical edits need no diff budget", () => {
  const m = createBookSourceMerger({ cells: 1, lines: 1 });
  assert.deepEqual(m("A", "A", "A"), { action: "unchanged", source: "A", reason: null });
  assert.equal(m("A", "local", "A").source, "local");
  assert.equal(m("A", "A", "disk").action, "disk");
  assert.equal(m("A", "same", "same").source, "same");
  assert.equal(m(null, "", null).source, "");
});
test("absence is not empty source and is never an automatic editor deletion", () => {
  assert.equal(merge("a", "a", null).reason, "deleted-on-disk");
  assert.equal(merge(null, "local", "").reason, "created-on-disk");
  assert.equal(merge("a", "a", "").source, "");
  assert.equal(merge(null, "same", "same").action, "unchanged");
});
test("disjoint and adjacent line replacements merge without normalizing source", () => {
  const base = "\ufeffA\r\nB\rC\n日本 😀";
  const local = "\ufeffLOCAL\r\nB\rC\n日本 😀";
  const disk = "\ufeffA\r\nB\rDISK\n日本 😀";
  assert.equal(merge(base, local, disk).source, "\ufeffLOCAL\r\nB\rDISK\n日本 😀");
  assert.equal(merge("a\nb\nc", "A\nb\nc", "a\nB\nc").source, "A\nB\nc");
});
test("non-overlapping insertions and deletions preserve their exact ordering", () => {
  assert.equal(merge("a\nb\nc\nd\n", "a\nb\n", "A\nb\nc\nd\n").source, "A\nb\n");
  assert.equal(merge("a\nb\n", "start\na\nb\n", "a\nb\nend").source, "start\na\nb\nend");
  assert.equal(merge("a\nb\nc\nd", "a\nb\nx\nc\nd", "A\nb\nc\nd").source, "A\nb\nx\nc\nd");
});
test("overlap, insertion collisions and touching edit boundaries remain explicit conflicts", () => {
  for (const inputs of [
    ["a\nb\nc\n", "a\nX\nc\n", "a\nY\nc\n"],
    ["a\nb\n", "a\nx\nb\n", "a\ny\nb\n"],
    ["a\nb\n", "A\nb\n", "a\nx\nb\n"],
    ["a\nb\nc\n", "a\nc\n", "a\nx\nb\nc\n"],
    ["a\nb\nc\n", "a\nx\nc\n", "a\ny\n"],
  ]) assert.equal(merge(...inputs).reason, "overlapping-edits", JSON.stringify(inputs));
});
test("identical shared hunks are applied once alongside independent edits", () => {
  const base = "a\nb\nc\nd\ne\n";
  assert.equal(merge(base, "A\nb\nC\nd\ne\n", "a\nb\nC\nd\nE\n").source, "A\nb\nC\nd\nE\n");
  assert.equal(merge(base, "A\nb\nx\nc\nd\ne\n", "a\nb\nx\nc\nd\nE\n").source, "A\nb\nx\nc\nd\nE\n");
});
test("budgets are aggregate, deterministic and never convert a complex edit into a guessed result", () => {
  const m = createBookSourceMerger({ cells: 7, lines: 20 });
  assert.equal(m("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n").reason, "merge-budget");
  // One-sided updates remain safe even after the diff budget has been spent.
  assert.equal(m("a", "a", "disk").source, "disk");
  const many = createBookSourceMerger({ lines: 18 });
  const args = ["a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n"];
  assert.equal(many(...args).action, "merged"); assert.equal(many(...args).action, "merged");
  assert.equal(many(...args).reason, "merge-budget");
});
test("tiny changed regions in long documents do not allocate a whole-document diff matrix", () => {
  const middle = Array.from({ length: 5000 }, (_, i) => `line ${i}\n`).join("");
  const base = "a\n" + middle + "z\n";
  const value = createBookSourceMerger({ cells: 8 })(base, "A\n" + middle + "z\n", "a\n" + middle + "Z\n");
  assert.equal(value.source, "A\n" + middle + "Z\n");
});
test("budgets and source types cannot request unbounded computation", () => {
  for (const limits of [{ cells: 0 }, { lines: -1 }, { cells: 1.1 }, { lines: Infinity }, { cells: BOOK_SOURCE_MERGE_LIMITS.cells + 1 }])
    assert.throws(() => createBookSourceMerger(limits), { code: "INVALID_SOURCE_MERGE" });
  for (const args of [[undefined, "", ""], ["", null, ""], ["", "", {}], ["", "x".repeat(4 * 1024 * 1024 + 1), ""]])
    assert.throws(() => merge(...args), { code: "INVALID_SOURCE_MERGE" });
});
test("seeded disjoint edits agree with an independent line-replacement oracle in both directions", () => {
  let seed = 0x719cdc;
  const random = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  for (let iteration = 0; iteration < 1000; iteration++) {
    const base = Array.from({ length: 12 }, (_, i) => `original-${i}${["\n", "\r\n", "\r"][random(3)]}`);
    const local = base.slice(), disk = base.slice(), expected = base.slice();
    for (let i = 0; i < base.length; i++) {
      const side = random(4);
      if (side === 0) local[i] = expected[i] = `local-${i}\n`;
      if (side === 1) disk[i] = expected[i] = `disk-${i}\n`;
      if (side === 2) local[i] = disk[i] = expected[i] = `shared-${i}\n`;
    }
    const a = merge(base.join(""), local.join(""), disk.join(""));
    const b = merge(base.join(""), disk.join(""), local.join(""));
    // Adjacent shared and independent changes can coalesce into overlapping
    // hunks: conservative conflicts are allowed, invented merged text is not.
    for (const value of [a, b]) if (value.action !== "conflict") assert.equal(value.source, expected.join(""));
    assert.equal(a.action === "conflict", b.action === "conflict");
  }
});
