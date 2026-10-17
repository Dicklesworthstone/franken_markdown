import test from "node:test";
import assert from "node:assert/strict";
import { captureBookSourceTreeForMerge as capture, planBookSourceTreeMerge as plan,
  BOOK_SOURCE_TREE_MERGE_LIMITS as LIMITS } from "../demo/book_source_merge.mjs";

const file = (path, source = path, role) => ({ path, source, ...(role ? { role } : {}) });
const book = (...paths) => paths.map(path => file(`${path}.md`));
const paths = files => files.map(file => file.path);
const result = (base, local, incoming, limits) => plan(base, local, incoming, limits).resolve([]);
const answers = (review, choice = "local") => review.conflicts.map(({ index }) => ({ index, choice }));
const invalid = { code: "INVALID_SOURCE_TREE_MERGE" };

test("source trees are captured as immutable owned values without settings or resources", () => {
  const input = [file("a.md", "\ufeffA\r\n", "chapter"), file("parts/x.txt", "part", "include")];
  const copy = capture(input); input[0].source = "changed"; input.reverse();
  assert.deepEqual(copy, [file("a.md", "\ufeffA\r\n"), file("parts/x.txt", "part", "include")]);
  assert(Object.isFrozen(copy)); assert(Object.isFrozen(copy[0]));
  for (const extra of ["images", "settings", "handle", "options"])
    assert.throws(() => capture([{ ...file("a.md"), [extra]: {} }]), invalid);
});
test("same trees, empty books, and all one-sided edits resolve exactly", () => {
  const base = book("a", "b", "c");
  const next = [file("d.md", "new"), file("c.md", "updated", "include"), base[0]];
  for (const [local, incoming] of [[base, next], [next, base], [next, next]])
    assert.deepEqual(result(base, local, incoming), next);
  assert.deepEqual(result(base, [], base), []);
  assert.deepEqual(result(base, base, []), []);
  assert.deepEqual(result([], [], []), []);
  assert.deepEqual(result([], [], next), next);
});
test("disjoint edits in the same source merge with exact Unicode, BOM and newlines", () => {
  const base = [file("a.md", "\ufeffA\r\nB\rC\n日本 😀")];
  const local = [file("a.md", "\ufeffLOCAL\r\nB\rC\n日本 😀")];
  const incoming = [file("a.md", "\ufeffA\r\nB\rDISK\n日本 😀")];
  assert.equal(result(base, local, incoming)[0].source, "\ufeffLOCAL\r\nB\rDISK\n日本 😀");
});
test("independent additions, removals and source edits combine rather than replace a whole project", () => {
  const base = book("a", "b", "c", "d");
  const local = [file("a.md", "ours"), file("x.md", "new-local"), base[1], base[3]];
  const incoming = [base[0], base[2], file("y.md", "new-saved"), file("d.md", "theirs")];
  const expected = [local[0], local[1], incoming[2], incoming[3]];
  assert.deepEqual(result(base, local, incoming), expected);
  assert.deepEqual(result(base, incoming, local), expected);
});
test("independent role and text edits to one path both survive", () => {
  const base = [file("a.md", "old")];
  assert.deepEqual(result(base, [file("a.md", "new")], [file("a.md", "old", "include")]),
    [file("a.md", "new", "include")]);
  assert.deepEqual(result(base, [file("a.md", "old", "include")], [file("a.md", "new")]),
    [file("a.md", "new", "include")]);
});
test("identical independently added paths converge but different sources need explicit resolution", () => {
  const incoming = [file("a.md", "theirs")], local = [file("a.md", "ours")];
  assert.deepEqual(result([], incoming, incoming), incoming);
  const review = plan([], local, incoming);
  assert.equal(review.conflicts[0].kind, "source"); assert.equal(review.conflicts[0].base, null);
  assert.throws(() => review.resolve([]), invalid);
  assert.deepEqual(review.resolve(answers(review, "incoming")), incoming);
});
test("new same-path role differences are resolved separately from source text", () => {
  const review = plan([], [file("a.md", "ours")], [file("a.md", "theirs", "include")]);
  assert.deepEqual(review.conflicts.map(c => c.kind), ["source", "role"]);
  assert.deepEqual(review.resolve(review.conflicts.map(c => ({ index: c.index,
    choice: c.kind === "source" ? "local" : "incoming" }))), [file("a.md", "ours", "include")]);
});
test("edit-versus-delete preserves an explicit choice and never equates empty text with absence", () => {
  const base = [file("a.md", "old")], local = [file("a.md", "")];
  for (const [left, right] of [[local, []], [[], local]]) {
    const review = plan(base, left, right);
    assert.equal(review.conflicts[0].kind, "membership");
    assert.deepEqual(review.resolve(answers(review, "local")), left);
    assert.deepEqual(review.resolve(answers(review, "incoming")), right);
    assert.throws(() => review.resolve([{ index: 0, choice: "edited", source: "new" }]), invalid);
  }
});
test("role changes and reorderings count as edits before accepting the other side's deletion", () => {
  const base = book("a", "b", "c");
  const role = [file("a.md", "a.md", "include"), ...base.slice(1)];
  assert.equal(plan(base, role, base.slice(1)).conflicts[0].kind, "membership");
  const reordered = [base[1], base[0], base[2]];
  const review = plan(base, reordered, base.slice(1));
  assert.equal(review.conflicts[0].kind, "membership");
  assert.deepEqual(review.resolve(answers(review)), reordered);
  assert.deepEqual(review.resolve(answers(review, "incoming")), base.slice(1));
});
test("renames are explicit delete/add identities, with a conflict for an edit to the old path", () => {
  const base = [file("old.md", "base")];
  const local = [file("renamed.md", "base")], incoming = [file("old.md", "edited")];
  const review = plan(base, local, incoming);
  assert.equal(review.conflicts[0].path, "old.md");
  assert.deepEqual(paths(review.resolve(answers(review, "incoming"))), ["old.md", "renamed.md"]);
  assert.deepEqual(review.resolve(answers(review)), local);
});
test("nonoverlapping reorders merge and anchored additions do not look like reorders", () => {
  const base = book("a", "b", "c", "d");
  const local = [base[1], base[0], base[2], base[3]];
  const incoming = [base[0], base[1], base[3], base[2]];
  assert.deepEqual(paths(result(base, local, incoming)), ["b.md", "a.md", "d.md", "c.md"]);
  assert.deepEqual(paths(result(base, [base[0], file("new.md"), ...base.slice(1)], incoming)),
    ["a.md", "new.md", "b.md", "d.md", "c.md"]);
});
test("simultaneous insertions at the same anchor have deterministic path order", () => {
  const base = book("a", "b");
  const local = [base[0], file("z.md"), base[1]], incoming = [base[0], file("y.md"), base[1]];
  const expected = [base[0], incoming[1], local[1], base[1]];
  assert.deepEqual(result(base, local, incoming), expected);
  assert.deepEqual(result(base, incoming, local), expected);
});
test("contradictory moves expose an order conflict instead of silently picking a side", () => {
  const base = book("a", "b", "c");
  const local = [base[1], base[0], base[2]], incoming = [base[0], base[2], base[1]];
  const review = plan(base, local, incoming);
  assert.equal(review.conflicts.at(-1).kind, "order"); assert.equal(review.order.merged, null);
  assert.deepEqual(review.resolve(answers(review)), local);
  assert.deepEqual(review.resolve(answers(review, "incoming")), incoming);
});
test("an explicit order choice preserves and appends the other side's unique sources", () => {
  const base = book("a", "b", "c");
  const local = [base[1], base[0], base[2], file("local.md")];
  const incoming = [base[0], base[2], base[1], file("incoming.md")];
  const review = plan(base, local, incoming);
  assert.deepEqual(review.resolve(answers(review)), [...local, incoming.at(-1)]);
  assert.deepEqual(review.resolve(answers(review, "incoming")), [...incoming, local.at(-1)]);
});
test("incompatible order for independently created common paths is not guessed", () => {
  const local = book("a", "b"), incoming = [...local].reverse();
  const review = plan([], local, incoming);
  assert.equal(review.conflicts[0].kind, "order");
  assert.deepEqual(review.resolve(answers(review)), local);
});
test("regional source choices retain independent edits elsewhere in the same chapter", () => {
  const base = [file("a.md", "a\nb\nc\nd\ne\n")];
  const local = [file("a.md", "A\nb\nLOCAL\nd\ne\n")];
  const incoming = [file("a.md", "a\nb\nINCOMING\nd\nE\n")];
  const review = plan(base, local, incoming), conflict = review.conflicts[0];
  assert.equal(conflict.hunks.length, 1);
  const resolved = review.resolve([{ index: conflict.index, choice: "regions",
    regions: [{ index: 0, choice: "disk" }] }]);
  assert.equal(resolved[0].source, "A\nb\nINCOMING\nd\nE\n");
  assert.equal(review.resolve([{ index: 0, choice: "edited", source: "" }])[0].source, "");
});
test("distant edits in a long chapter share the bounded sparse text merger", () => {
  const lines = Array.from({ length: 10000 }, (_, i) => `line-${i}\n`);
  const local = lines.slice(), incoming = lines.slice(), expected = lines.slice();
  for (const i of [10, 9000]) local[i] = expected[i] = `local-${i}\n`;
  for (const i of [100, 9500]) incoming[i] = expected[i] = `incoming-${i}\n`;
  assert.equal(result([file("a.md", lines.join(""))], [file("a.md", local.join(""))],
    [file("a.md", incoming.join(""))])[0].source, expected.join(""));
});
test("region exposure is bounded across all sources, not merely per file", () => {
  const texts = prefix => Array.from({ length: 300 }, (_, i) => i % 2 ? `same-${i}\n` : `${prefix}-${i}\n`).join("");
  const base = [file("a.md", texts("base")), file("b.md", texts("base"))];
  const local = base.map(f => file(f.path, texts("local"))), incoming = base.map(f => file(f.path, texts("incoming")));
  const review = plan(base, local, incoming);
  assert.equal(review.conflicts.length, 2);
  assert(review.conflicts[0].hunks.length > 0); assert.equal(review.conflicts[1].hunks, null);
  assert.deepEqual(review.resolve(answers(review)), local);
});
test("exhausted text budgets retain complete file choices instead of a partial merge", () => {
  const texts = prefix => Array.from({ length: 2000 }, (_, i) => `${prefix}-${i}\n`).join("");
  const base = [file("a.md", texts("base"))], local = [file("a.md", texts("local"))], incoming = [file("a.md", texts("incoming"))];
  const review = plan(base, local, incoming);
  assert.equal(review.conflicts[0].reason, "merge-budget"); assert.equal(review.conflicts[0].hunks, null);
  assert.deepEqual(review.resolve(answers(review, "incoming")), incoming);
});
test("combined file counts, roles and UTF-8 bytes are checked before a result can escape", () => {
  const cap = { chapters: 1, includeSources: 1, fileBytes: 20, sourceBytes: 20, pathBytes: 10 };
  const base = [], local = [file("a.md", "x")], incoming = [file("b.md", "y")];
  assert.throws(() => result(base, local, incoming, cap), { code: "BOOK_LIMIT" });
  assert.throws(() => result([], [file("a.md", "😀😀")], [file("b.md", "😀😀", "include")], cap), { code: "BOOK_LIMIT" });
  const review = plan([file("a.md", "a")], [file("a.md", "x")], [file("a.md", "y")], cap);
  assert.throws(() => review.resolve([{ index: 0, choice: "edited", source: "😀".repeat(6) }]), { code: "BOOK_LIMIT" });
});
test("reconciliation never normalizes path identity or malformed Unicode", () => {
  const distinct = [file("é.md", "é"), file("e\u0301.md", "e\u0301"), file("A.md", "A"), file("a.md", "a")];
  assert.deepEqual(result([], distinct, []), distinct);
  for (const source of ["\ud800", "\udfff", "a\ud800b"]) assert.throws(() => capture([file("a.md", source)]), invalid);
  assert.throws(() => capture([file("\ud800.md", "text")]), invalid);
});
test("invalid, ambiguous and unbounded trees cannot become merge baselines", () => {
  for (const value of [null, {}, [file("a.md"), file("a.md")], new Array(1),
    [file("../a.md")], [file("/a.md")], [file("a%20.md")], [file("a.md", "x", "wrong")], [file("part.txt")],
    [file("a.md", 123)], [file("a.md", "x".repeat(LIMITS.fileBytes + 1))]]) assert.throws(() => capture(value));
  for (const cap of [{ chapters: 0 }, { chapters: 129 }, { sourceBytes: Infinity }, { files: 1 }])
    assert.throws(() => capture([], cap), invalid);
});
test("source and decision accessors are rejected without executing their code", () => {
  let reads = 0;
  const source = { path: "a.md", get source() { reads++; return "text"; } };
  assert.throws(() => capture([source]), invalid);
  const review = plan([file("a.md", "a")], [file("a.md", "x")], [file("a.md", "y")]);
  assert.throws(() => review.resolve([{ index: 0, get choice() { reads++; return "local"; } }]), invalid);
  assert.throws(() => review.resolve([{ index: 0, choice: "edited", get source() { reads++; return "x"; } }]), invalid);
  assert.equal(reads, 0);
});
test("missing, duplicate, unrecognized and malformed choices cannot partially resolve a tree", () => {
  const base = book("a", "b"), local = base.map(f => file(f.path, "local")), incoming = base.map(f => file(f.path, "incoming"));
  const review = plan(base, local, incoming);
  for (const values of [[], new Array(2), [{ index: 0, choice: "local" }, { index: 0, choice: "incoming" }],
    [{ index: 0, choice: "local" }, { index: 9, choice: "local" }],
    [{ index: 0, choice: "local" }, { index: 1, choice: "unknown" }],
    [{ index: 0, choice: "local" }, { index: 1, choice: "edited", source: "\ud800" }]])
    assert.throws(() => review.resolve(values), invalid);
  assert.deepEqual(review.resolve(answers(review)), local);
});
test("plans own their inputs and can be resolved repeatedly without consuming or mutating them", () => {
  const base = [file("a.md", "a")], local = [file("a.md", "local")], incoming = [file("a.md", "incoming")];
  const review = plan(base, local, incoming), saved = JSON.stringify(review);
  local[0].source = "late"; incoming.pop(); base[0].path = "moved.md";
  assert.equal(review.resolve(answers(review))[0].source, "local");
  assert.equal(review.resolve(answers(review, "incoming"))[0].source, "incoming");
  assert.equal(JSON.stringify(review), saved); assert(Object.isFrozen(review.conflicts));
});
test("500 seeded independent source/role/delete/add edits match a snapshot oracle in both directions", () => {
  let seed = 0x59fe1;
  const random = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  for (let iteration = 0; iteration < 500; iteration++) {
    const base = Array.from({ length: 12 }, (_, i) => file(`${String(i).padStart(2, "0")}.md`, `base ${i}\r\n`));
    const local = [], incoming = [], expected = [];
    for (const f of base) {
      const choice = random(6);
      const edit = file(f.path, choice === 0 || choice === 1 ? `changed ${iteration}\r\n` : f.source,
        choice === 2 || choice === 3 ? "include" : undefined);
      if (choice !== 4) local.push(choice === 0 || choice === 2 ? edit : f);
      if (choice !== 5) incoming.push(choice === 1 || choice === 3 ? edit : f);
      if (choice !== 4 && choice !== 5) expected.push(edit);
    }
    local.push(file("new-local.md", "new")); incoming.push(file("new-incoming.md", "new"));
    expected.push(incoming.at(-1), local.at(-1));
    assert.deepEqual(result(base, local, incoming), expected);
    assert.deepEqual(result(base, incoming, local), expected);
  }
});
