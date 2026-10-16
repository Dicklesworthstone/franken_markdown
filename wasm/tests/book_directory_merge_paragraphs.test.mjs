import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger, planBookSourceTreeMerge } from "../demo/book_source_merge.mjs";
const merge = (...args) => createBookSourceMerger()(...args);
const plan = (...args) => createBookSourceMerger().plan(...args);
function merged(base, local, disk, expected) {
  for (const [left, right] of [[local, disk], [disk, local]]) {
    assert.deepEqual(merge(base, left, right), { action: "merged", source: expected, reason: null });
    assert.deepEqual(plan(base, left, right), { action: "merged", source: expected, reason: null });
  }
}

test("independent word edits on one paragraph are combined in both review APIs", () => {
  merged("The red fox follows the blue bird.\n",
    "The quick fox follows the blue bird.\n", "The red fox follows the green bird.\n",
    "The quick fox follows the green bird.\n");
});
test("paragraph insertions separated by stable words retain both edits", () => {
  merged("red fox follows blue bird\n", "red quick fox follows blue bird\n",
    "red fox follows blue bright bird\n", "red quick fox follows blue bright bird\n");
});
test("independent deletions do not restore removed words", () => {
  merged("the bright red fox follows the swift blue bird\n",
    "the red fox follows the swift blue bird\n", "the bright red fox follows the blue bird\n",
    "the red fox follows the blue bird\n");
});
for (const newline of ["", "\n", "\r", "\r\n"]) {
  test(`exact paragraph bytes retain BOM, tabs, Unicode and ${JSON.stringify(newline)}`, () => {
    merged(`\ufeffred\tfox  café 日本 😀 family👨‍👩‍👧‍👦 blue bird${newline}`,
      `\ufeffgreen\tfox  café 日本 😀 family👨‍👩‍👧‍👦 blue bird${newline}`,
      `\ufeffred\tfox  café 日本 😀 family👨‍👩‍👧‍👦 gold bird${newline}`,
      `\ufeffgreen\tfox  café 日本 😀 family👨‍👩‍👧‍👦 gold bird${newline}`);
  });
}
test("an edited line ending can coexist with a distant word change without normalization", () => {
  merged("red fox follows blue bird\r\n", "green fox follows blue bird\r\n",
    "red fox follows blue bird\n", "green fox follows blue bird\n");
});
test("shared words changed identically are emitted once alongside independent edits", () => {
  merged("red fox walks past blue bird and tall tree\n",
    "quick fox walks past gold bird and tall tree\n", "red fox walks past gold bird and small tree\n",
    "quick fox walks past gold bird and small tree\n");
});
test("same-token edits never invent a word, grapheme, identifier or Markdown destination", () => {
  for (const [base, local, disk] of [
    ["abcdef rest end", "Abcdef rest end", "abcdeF rest end"],
    ["cafe\u0301 rest end", "Cafe\u0301 rest end", "cafe\u0300 rest end"],
    ["👨‍👩‍👧‍👦 rest end", "👩‍👩‍👧‍👦 rest end", "👨‍👩‍👦‍👦 rest end"],
    ["[title](old.md) rest end", "[new](old.md) rest end", "[title](next.md) rest end"],
  ]) {
    const p = plan(base, local, disk);
    assert.equal(p.action, "conflict"); assert.equal(p.conflicts.length, 1);
    assert.equal(p.resolve([{ index: 0, choice: "local" }]), local);
    assert.equal(p.resolve([{ index: 0, choice: "disk" }]), disk);
  }
});
test("overlapping word replacements, insertion points and edited boundaries stay explicit", () => {
  for (const [base, local, disk] of [
    ["red fox blue bird", "green fox blue bird", "gold fox blue bird"],
    ["red fox blue bird", "red quick fox blue bird", "red swift fox blue bird"],
    ["red fox blue bird", "red fox blue bird", null],
    [null, "red fox blue bird", "red fox green bird"],
    ["red fox blue bird", "red blue bird", "red quick fox blue bird"],
  ]) assert.equal(merge(base, local, disk).action, "conflict");
});
test("line structure changes and multi-line overlaps retain original conflict regions", () => {
  const inputs = [
    ["red fox\nblue bird\n", "green fox\ngold bird\n", "red fox\nsmall bird\n"],
    ["red fox follows blue bird\n", "green fox\nfollows blue bird\n", "red fox follows gold bird\n"],
    ["red fox\n", "red fox\nnew line\n", "green fox\n"],
  ];
  for (const [base, local, disk] of inputs) {
    const p = plan(base, local, disk);
    assert.equal(p.action, "conflict");
    assert(p.conflicts?.length);
    assert.equal(p.resolve(p.conflicts.map(({ index }) => ({ index, choice: "local" }))), local);
  }
});
test("successful paragraph merges survive alongside unresolved lines in the same plan", () => {
  const p = plan("red fox blue bird\nstable\noriginal tail\n",
    "green fox blue bird\nstable\nlocal tail\n", "red fox gold bird\nstable\ndisk tail\n");
  assert.equal(p.conflicts.length, 1); assert.equal(p.conflicts[0].startLine, 2);
  assert.equal(p.resolve([{ index: 0, choice: "disk" }]), "green fox gold bird\nstable\ndisk tail\n");
});
test("low remaining work allowance keeps a resolvable line conflict instead of a guessed paragraph", () => {
  const m = createBookSourceMerger({ cells: 8 });
  const p = m.plan("red fox blue bird\n", "green fox blue bird\n", "red fox gold bird\n");
  assert.equal(p.action, "conflict"); assert.equal(p.reason, "overlapping-edits");
  assert.equal(p.resolve([{ index: 0, choice: "disk" }]), "red fox gold bird\n");
  assert.equal(m("old", "old", "one-sided").source, "one-sided");
});
test("source-tree reconciliation preserves paragraph edits with independently changed roles and membership", () => {
  const base = [{ path: "a.md", source: "red fox blue bird\r\n" }, { path: "shared.md", source: "one two three four\n", role: "include" }];
  const local = structuredClone(base); local[0].source = "green fox blue bird\r\n";
  local[1].source = "first two three four\n";
  const incoming = structuredClone(base); incoming[0].source = "red fox gold bird\r\n";
  incoming[1].source = "one two last four\n"; delete incoming[1].role;
  incoming.push({ path: "end.md", source: "# New\n" });
  const p = planBookSourceTreeMerge(base, local, incoming);
  assert.deepEqual(p.conflicts, []);
  assert.deepEqual(p.resolve([]), [
    { path: "a.md", source: "green fox gold bird\r\n" },
    { path: "shared.md", source: "first two last four\n" },
    { path: "end.md", source: "# New\n" },
  ]);
  assert.equal(base[0].source, "red fox blue bird\r\n");
});
test("source-tree unresolved regions keep independent merged paragraphs when choosing a version", () => {
  const base = [{ path: "a.md", source: "red fox blue bird\nseparator\noriginal tail\n" }];
  const local = [{ path: "a.md", source: "green fox blue bird\nseparator\nlocal tail\n" }];
  const incoming = [{ path: "a.md", source: "red fox gold bird\nseparator\nincoming tail\n" }];
  const p = planBookSourceTreeMerge(base, local, incoming);
  assert.equal(p.conflicts.length, 1); assert.equal(p.conflicts[0].hunks.length, 1);
  assert.equal(p.resolve([{ index: 0, choice: "regions", regions: [{ index: 0, choice: "disk" }] }])[0].source,
    "green fox gold bird\nseparator\nincoming tail\n");
});
test("500 seeded separated paragraph replacements match an independent token-position oracle", () => {
  let seed = 0x759afb;
  const random = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  for (let round = 0; round < 500; round++) {
    const base = Array.from({ length: 16 }, (_, i) => `word-${i}`);
    const l = base.slice(), r = base.slice(), expected = base.slice();
    const a = random(6), b = 9 + random(6);
    l[a] = expected[a] = `local-${round}`; r[b] = expected[b] = `incoming-${round}`;
    const separator = [" ", "\t", "  "][random(3)], end = ["", "\n", "\r", "\r\n"][random(4)];
    merged(base.join(separator) + end, l.join(separator) + end, r.join(separator) + end, expected.join(separator) + end);
  }
});

test("a long paragraph with sparse distant edits uses the bounded sparse diff", () => {
  const base = Array.from({ length: 5000 }, (_, i) => `word-${i}`);
  const local = base.slice(), disk = base.slice(), expected = base.slice();
  for (const i of [2, 2500, 4998]) local[i] = expected[i] = `LOCAL-${i}`;
  for (const i of [20, 2400, 4980]) disk[i] = expected[i] = `DISK-${i}`;
  merged(base.join(" ") + "\r\n", local.join(" ") + "\r\n", disk.join(" ") + "\r\n", expected.join(" ") + "\r\n");
});
test("refinement work is shared across files rather than renewed per paragraph", () => {
  const args = ["red fox blue bird\n", "green fox blue bird\n", "red fox gold bird\n"];
  const m = createBookSourceMerger({ cells: 500 });
  assert.equal(m.plan(...args).action, "merged");
  const later = m.plan(...args);
  assert.equal(later.action, "conflict");
  assert.equal(later.resolve([{ index: 0, choice: "disk" }]), args[2]);
  assert.equal(m("original", "local", "original").source, "local");
});
test("oversized refinement retains the complete reviewable line without allocating a diff matrix", () => {
  const padding = "x".repeat(700000);
  const base = `red ${padding} blue\n`, local = `green ${padding} blue\n`, disk = `red ${padding} gold\n`;
  const p = plan(base, local, disk);
  assert.equal(p.action, "conflict"); assert.equal(p.reason, "overlapping-edits");
  assert.equal(p.conflicts[0].base, base);
  assert.equal(p.resolve([{ index: 0, choice: "local" }]), local);
});
test("dense unrelated paragraph rewrites exhaust work conservatively and stay manually resolvable", () => {
  const text = prefix => Array.from({ length: 1000 }, (_, i) => `${prefix}${i}`).join(" ");
  const p = createBookSourceMerger({ cells: 40000 }).plan(text("b"), text("l"), text("r"));
  assert.equal(p.action, "conflict"); assert.equal(p.reason, "overlapping-edits");
  assert.equal(p.resolve([{ index: 0, choice: "edited", source: "agreed\r\n" }]), "agreed\r\n");
});
test("repeated words with ambiguous opposing deletions are not guessed", () => {
  for (const args of [
    ["a a b", "a b", "b a b"],
    ["word word word", "word word", "word"],
    ["a b a b", "a b", "a b a c"],
  ]) {
    const a = merge(...args), b = merge(args[0], args[2], args[1]);
    assert.equal(a.action, "conflict", JSON.stringify(args)); assert.equal(b.action, "conflict");
  }
});
test("one-character punctuation, spacing and endpoint changes do not normalize untouched input", () => {
  merged("red fox : blue bird\n", "red  fox : blue bird\n", "red fox : gold bird\n", "red  fox : gold bird\n");
  merged("red fox : blue bird\n", "red fox ; blue bird\n", "red fox : blue finch\n", "red fox ; blue finch\n");
});

test("changed repeated words keep a line conflict even when extreme alignments agree", () => {
  for (const args of [["a a b", "a b", "a a c"], ["a a a", "b a", "a a b"],
    ["a", "b a", "a a b"]]) assert.equal(merge(...args).action, "conflict");
});
