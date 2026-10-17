import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger } from "../demo/book_source_merge.mjs";
const plan = (...args) => createBookSourceMerger().plan(...args);
const choice = (index, side) => ({ index, choice: side });
const base = "a\nspace-1\nb\nspace-2\nc\nspace-3\nd\nend\n";
const local = "LOCAL A\nspace-1\nLOCAL B\nspace-2\nc\nspace-3\nLOCAL D\nend\n";
const disk = "a\nspace-1\nDISK B\nspace-2\nDISK C\nspace-3\nDISK D\nend\n";

test("region choices retain all independent changes from both versions", () => {
  const p = plan(base, local, disk);
  assert.equal(p.action, "conflict"); assert.equal(p.conflicts.length, 2);
  assert.deepEqual(p.conflicts.map(h => [h.startLine, h.endLine]), [[2, 3], [6, 7]]);
  assert.equal(p.resolve([choice(0, "disk"), choice(1, "local")]),
    "LOCAL A\nspace-1\nDISK B\nspace-2\nDISK C\nspace-3\nLOCAL D\nend\n");
  assert.equal(p.resolve([choice(1, "disk"), choice(0, "local")]),
    "LOCAL A\nspace-1\nLOCAL B\nspace-2\nDISK C\nspace-3\nDISK D\nend\n");
});
test("the ordinary merge result stays a conflict without exposing or choosing a partial source", () => {
  assert.deepEqual(createBookSourceMerger()(base, local, disk),
    { action: "conflict", source: null, reason: "overlapping-edits" });
});
test("manual per-region edits and deletion are explicit and preserve surrounding bytes", () => {
  const p = plan(base, local, disk);
  const text = p.resolve([{ index: 0, choice: "edited", source: "agreed 日本 😀\r\n" },
    { index: 1, choice: "edited", source: "" }]);
  assert.equal(text, "LOCAL A\nspace-1\nagreed 日本 😀\r\nspace-2\nDISK C\nspace-3\nend\n");
});
test("one deletion spanning multiple opposing edits is one complete conflict region", () => {
  const b = Array.from({ length: 10 }, (_, i) => `${i}\n`);
  const l = b.slice(0, 2).concat(b.slice(8));
  const r = b.slice(); r[3] = "disk-3\n"; r[6] = "disk-6\n";
  const p = plan(b.join(""), l.join(""), r.join(""));
  assert.equal(p.conflicts.length, 1);
  assert.deepEqual(p.conflicts[0], { index: 0, startLine: 2, endLine: 8,
    base: "2\n3\n4\n5\n6\n7\n", local: "", disk: "2\ndisk-3\n4\n5\ndisk-6\n7\n" });
  assert.equal(p.resolve([choice(0, "local")]), l.join(""));
  assert.equal(p.resolve([choice(0, "disk")]), r.join(""));
});
test("an insertion at an edited boundary connects the neighboring conflict ranges", () => {
  const p = plan("a\nb\nc\n", "A\nb\nC\n", "a\ninsertion\nb\nc\n");
  assert.equal(p.conflicts.length, 1);
  assert.equal(p.conflicts[0].base, "a\n");
  assert.equal(p.resolve([choice(0, "disk")]), "a\ninsertion\nb\nC\n");
});
test("distinct insertions at one position are not silently concatenated", () => {
  const p = plan("a\nb", "a\nours\nb", "a\ntheirs\nb");
  assert.deepEqual(p.conflicts[0], { index: 0, startLine: 1, endLine: 1, base: "", local: "ours\n", disk: "theirs\n" });
  assert.equal(p.resolve([choice(0, "disk")]), "a\ntheirs\nb");
});
test("adjacent nonempty replacements and identical shared changes need no resolution", () => {
  assert.equal(plan("a\nb\nc\nd\n", "A\nB\nc\nd\n", "a\nb\nC\nd\n").source, "A\nB\nC\nd\n");
  const p = plan("a\nspace\nb\nspace2\nc\n", "A\nspace\nB\nspace2\nc\n", "a\nspace\nB\nspace2\nC\n");
  assert.equal(p.action, "merged"); assert.equal(p.source, "A\nspace\nB\nspace2\nC\n");
});
test("BOM, mixed newlines and unterminated Unicode source remain exact", () => {
  const p = plan("\ufeffa\r\nb\rc\nlast 😀", "\ufeffA\r\nb\rLOCAL\nlast 😀", "\ufeffa\r\nDISK\rOTHER\nlast 😀");
  assert.equal(p.resolve([choice(0, "disk")]), "\ufeffA\r\nDISK\rOTHER\nlast 😀");
});
test("every region requires one owned explicit choice; duplicates and accessors are refused", () => {
  const p = plan(base, local, disk);
  let calls = 0;
  for (const values of [[], [choice(0, "local")], [choice(0, "local"), choice(0, "disk")],
    [choice(0, "local"), choice(2, "disk")], [choice(0, "local"), choice(1, "both")],
    [choice(0, "local"), { index: 1, get choice() { calls++; return "disk"; } }],
    [choice(0, "local"), { index: 1, choice: "edited", get source() { calls++; return "x"; } }], new Array(2)])
    assert.throws(() => p.resolve(values), { code: "INVALID_SOURCE_MERGE" });
  assert.equal(calls, 0);
});
test("manual output Unicode and total length are checked before returning any text", () => {
  const p = plan(base, local, disk);
  for (const source of [null, {}, "\ud800", "\udc00", "x".repeat(4 * 1024 * 1024)])
    assert.throws(() => p.resolve([{ index: 0, choice: "edited", source }, choice(1, "local")]), { code: "INVALID_SOURCE_MERGE" });
});
test("plans and source regions are immutable and resolution does not consume the plan", () => {
  const p = plan(base, local, disk), choices = [choice(0, "local"), choice(1, "disk")];
  assert(Object.isFrozen(p)); assert(Object.isFrozen(p.conflicts)); assert(Object.isFrozen(p.conflicts[0]));
  assert.throws(() => { p.conflicts[0].disk = "forged"; }, TypeError);
  const first = p.resolve(choices); assert.equal(p.resolve(choices), first);
  choices[0].choice = "disk"; assert.notEqual(p.resolve(choices), first);
});
test("creation, deletion and budget refusals have no invented per-region resolution", () => {
  for (const p of [plan(null, "a", "b"), plan("a", "a", null),
    createBookSourceMerger({ cells: 1 }).plan(base, local, disk)]) {
    assert.equal(p.action, "conflict"); assert.equal(p.resolve, undefined); assert.equal(p.conflicts, undefined);
  }
});
test("detailed planning shares the line allowance across files", () => {
  const m = createBookSourceMerger({ lines: 24 });
  assert.equal(m.plan(base, local, disk).conflicts.length, 2);
  assert.equal(m.plan(base, local, disk).reason, "merge-budget");
});
test("excessive conflict regions refuse as a whole instead of truncating review", () => {
  const b = Array.from({ length: 514 }, (_, i) => `line-${i}\n`);
  const l = b.map((line, i) => i % 2 ? line : `local-${i}\n`);
  const r = b.map((line, i) => i % 2 ? line : `disk-${i}\n`);
  const p = plan(b.join(""), l.join(""), r.join(""));
  assert.deepEqual(p, { action: "conflict", source: null, reason: "merge-hunks" });
});
test("sparse long-source plans preserve independent edits around a real conflict", () => {
  const b = Array.from({ length: 10000 }, (_, i) => `line-${i}\n`), l = b.slice(), r = b.slice(), expected = b.slice();
  l[2] = expected[2] = "local top\n"; l[9997] = expected[9997] = "local bottom\n";
  r[20] = expected[20] = "disk top\n"; r[9900] = expected[9900] = "disk bottom\n";
  l[5000] = "ours\n"; r[5000] = expected[5000] = "theirs\n";
  const p = plan(b.join(""), l.join(""), r.join(""));
  assert.equal(p.conflicts.length, 1); assert.equal(p.resolve([choice(0, "disk")]), expected.join(""));
});
