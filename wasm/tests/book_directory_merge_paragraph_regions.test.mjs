import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger as make, planBookSourceTreeMerge as tree } from "../demo/book_source_merge.mjs";
const base = "red fox follows blue bird near tall tree\r\n";
const local = "green fox follows gold bird near tall tree\r\n";
const disk = "red fox follows small bird near short tree\r\n";
const plan = (...args) => make().planParagraphs(...args);
const choice = (index, side) => ({ index, choice: side });
const expected = word => `green fox follows ${word} bird near short tree\r\n`;

test("paragraph-region selection keeps independent edits from both sides of a contested word", () => {
  const p = plan(base, local, disk);
  assert.equal(p.action, "conflict"); assert.equal(p.source, null);
  assert.deepEqual(p.conflicts, [{ index: 0, startLine: 0, endLine: 1,
    base: "blue", local: "gold", disk: "small", startColumn: 16, endColumn: 20 }]);
  assert.equal(p.resolve([choice(0, "local")]), expected("gold"));
  assert.equal(p.resolve([choice(0, "disk")]), expected("small"));
  assert.equal(p.resolve([{ index: 0, choice: "edited", source: "agreed" }]), expected("agreed"));
  assert.equal(p.resolve([{ index: 0, choice: "edited", source: "" }]), expected(""));
});
test("old plan callers and ordinary merge results retain their complete-line choice scope", () => {
  const p = make().plan(base, local, disk);
  assert.equal(p.conflicts[0].base, base); assert.equal(p.conflicts[0].startColumn, undefined);
  assert.equal(p.resolve([choice(0, "local")]), local);
  assert.equal(p.resolve([choice(0, "disk")]), disk);
  assert.deepEqual(make()(base, local, disk), { action: "conflict", source: null, reason: "overlapping-edits" });
});
test("several disputes on one line get separate mandatory decisions", () => {
  const p = plan("red fox sees blue bird near tall tree at calm lake\n",
    "green fox sees gold bird near high tree at calm lake\n",
    "red fox sees small bird near short tree at deep lake\n");
  assert.equal(p.conflicts.length, 2);
  assert.equal(p.resolve([choice(1, "disk"), choice(0, "local")]),
    "green fox sees gold bird near short tree at deep lake\n");
  assert.throws(() => p.resolve([choice(0, "local")]));
});
test("nested conflicts and unchanged whole-line conflicts share one global index sequence", () => {
  const p = plan(`original\nseparator\n${base}separator-two\noriginal-tail\n`,
    `ours\nseparator\n${local}separator-two\nours-tail\n`,
    `theirs\nseparator\n${disk}separator-two\ntheirs-tail\n`);
  assert.deepEqual(p.conflicts.map(h => h.index), [0, 1, 2]);
  assert.equal(p.conflicts[1].startLine, 2); assert.equal(p.conflicts[1].startColumn, 16);
  assert.equal(p.resolve([choice(2, "disk"), choice(0, "local"), choice(1, "local")]),
    `ours\nseparator\n${expected("gold")}separator-two\ntheirs-tail\n`);
});
test("source offsets count UTF-16 code units without splitting emoji or combining runs", () => {
  const prefix = "\ufeff😀 café́\t";
  const p = plan(prefix + base, prefix + local, prefix + disk);
  assert.equal(p.conflicts[0].startColumn, prefix.length + 16);
  assert.equal(p.conflicts[0].endColumn, prefix.length + 20);
  assert.equal(p.resolve([choice(0, "disk")]), prefix + expected("small"));
});
test("independent whitespace and final-newline edits survive a word resolution verbatim", () => {
  const p = plan("red fox sees blue bird\r\n", "red\tfox sees gold bird\r\n", "red fox sees small bird\n");
  assert.equal(p.conflicts[0].base, "blue");
  assert.equal(p.resolve([choice(0, "local")]), "red\tfox sees gold bird\n");
});
test("a disputed line terminator falls back to the original complete-line choice", () => {
  const p = plan("red fox\r\n", "green fox\n", "red fox\r");
  assert.equal(p.conflicts[0].startColumn, undefined);
  assert.equal(p.resolve([choice(0, "local")]), "green fox\n");
});
test("insertions at the same stable point require a choice without concatenating either side", () => {
  const p = plan("red\tfox  sees blue bird\n", "green\tfox  quick\tsees blue bird\n", "red\tfox  small\tsees blue bird\n");
  assert.equal(p.conflicts.length, 1); assert.equal(p.conflicts[0].base, "");
  assert.equal(p.resolve([choice(0, "local")]), "green\tfox  quick\tsees blue bird\n");
  assert.equal(p.resolve([choice(0, "disk")]), "green\tfox  small\tsees blue bird\n");
});
test("ambiguous repeated-word changes remain whole-line reviews in the fine-grained API", () => {
  for (const args of [["a a b", "a b", "a a c"], ["a", "b a", "a a b"]]) {
    const p = plan(...args);
    assert.equal(p.conflicts[0].startColumn, undefined);
    assert.equal(p.resolve([choice(0, "local")]), args[1]);
  }
});
test("low paragraph budgets retain full source choices and share allowance across calls", () => {
  const m = make({ cells: 8 });
  const p = m.planParagraphs(base, local, disk);
  assert.equal(p.conflicts[0].base, base);
  assert.equal(p.resolve([choice(0, "disk")]), disk);
  assert.equal(m.planParagraphs("old", "old", "new").source, "new");
});
test("nested choices are immutable, reusable and reject missing, duplicate or accessor values", () => {
  const p = plan(base, local, disk);
  assert(Object.isFrozen(p)); assert(Object.isFrozen(p.conflicts)); assert(Object.isFrozen(p.conflicts[0]));
  let invoked = 0;
  for (const choices of [[], [choice(1, "local")], [choice(0, "both")],
    [{ index: 0, get choice() { invoked++; return "local"; } }],
    [{ index: 0, choice: "edited", get source() { invoked++; return "x"; } }], new Array(1)])
    assert.throws(() => p.resolve(choices), { code: "INVALID_SOURCE_MERGE" });
  assert.equal(invoked, 0);
  assert.equal(p.resolve([choice(0, "local")]), expected("gold"));
  assert.equal(p.resolve([choice(0, "local")]), expected("gold"));
});
test("nested manual edits retain Unicode and aggregate-output admission", () => {
  const p = plan(base, local, disk);
  for (const source of ["\ud800", "\udc00", "x".repeat(4 * 1024 * 1024)])
    assert.throws(() => p.resolve([{ index: 0, choice: "edited", source }]), { code: "INVALID_SOURCE_MERGE" });
  assert.equal(p.resolve([{ index: 0, choice: "edited", source: "😀\nnew line" }]), expected("😀\nnew line"));
});
test("source-tree region choices keep surrounding changes and current chapter/include roles", () => {
  const f = source => [{ path: "chapter.md", source }, { path: "shared.txt", source: "retained", role: "include" }];
  const p = tree(f(base), f(local), f(disk));
  assert.equal(p.conflicts[0].hunks[0].base, "blue");
  assert.equal(p.conflicts[0].hunks[0].startColumn, 16);
  assert.deepEqual(p.resolve([{ index: 0, choice: "regions", regions: [choice(0, "disk")] }]), f(expected("small")));
  // Whole-file version choices deliberately remain whole-file choices.
  assert.deepEqual(p.resolve([choice(0, "local")]), f(local));
});
test("source-tree output bounds still apply after an explicit nested edit", () => {
  const f = source => [{ path: "a.md", source }];
  const p = tree(f(base), f(local), f(disk), { fileBytes: 100 });
  assert.throws(() => p.resolve([{ index: 0, choice: "regions", regions: [{ index: 0, choice: "edited", source: "x".repeat(100) }] }]),
    { code: "BOOK_LIMIT" });
});
test("200 seeded paragraph disputes preserve independent substitutions in either side order", () => {
  for (let round = 0; round < 200; round++) {
    const b = Array.from({ length: 15 }, (_, i) => `word-${i}`), l = b.slice(), r = b.slice(), e = b.slice();
    l[round % 4] = e[round % 4] = `local-${round}`;
    r[10 + round % 4] = e[10 + round % 4] = `disk-${round}`;
    l[7] = `ours-${round}`; r[7] = `theirs-${round}`;
    for (const swapped of [false, true]) {
      const p = plan(b.join(" "), (swapped ? r : l).join(" "), (swapped ? l : r).join(" "));
      assert.equal(p.conflicts.length, 1); assert.equal(p.conflicts[0].base, "word-7");
      e[7] = l[7];
      assert.equal(p.resolve([choice(0, swapped ? "disk" : "local")]), e.join(" "));
    }
  }
});

test("ambiguous whitespace insertion coordinates do not acquire smaller review authority", () => {
  const p = plan("red fox sees blue bird\n", "green fox sees quick blue bird\n", "red fox sees small blue bird\n");
  assert.equal(p.conflicts[0].startColumn, undefined);
  assert.equal(p.resolve([choice(0, "disk")]), "red fox sees small blue bird\n");
});
test("nested regions share the 256-region ceiling and keep an oversized next line whole", () => {
  const text = prefix => Array.from({ length: 4 }, (_, n) =>
    Array.from({ length: 80 }, (_, i) => `${prefix}-${n}-${i}`).join(" ") + `\nseparator-${n}\n`).join("");
  const p = plan(text("base"), text("local"), text("disk"));
  assert.equal(p.conflicts.length, 241);
  assert.equal(p.conflicts[239].startColumn !== undefined, true);
  assert.equal(p.conflicts[240].startColumn, undefined);
  assert.equal(p.resolve(p.conflicts.map(h => choice(h.index, "local"))), text("local"));
});
