import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger as make } from "../demo/book_source_merge.mjs";

test("duplicate-line deletion cannot silently choose the other version's edited occurrence", () => {
  for (const terminator of ["\n", "\r", "\r\n"]) {
    const base = ["same", "same", "tail"].join(terminator) + terminator;
    const local = ["same", "tail"].join(terminator) + terminator;
    const disk = ["changed", "same", "tail"].join(terminator) + terminator;
    for (const [left, right] of [[local, disk], [disk, local]]) {
      assert.deepEqual(make()(base, left, right),
        { action: "conflict", source: null, reason: "overlapping-edits" });
      for (const method of ["plan", "planParagraphs"]) {
        const plan = make()[method](base, left, right);
        assert.deepEqual(plan.conflicts, [{
          index: 0, startLine: 0, endLine: 3, base, local: left, disk: right,
        }]);
        assert.equal(plan.resolve([{ index: 0, choice: "local" }]), left);
        assert.equal(plan.resolve([{ index: 0, choice: "disk" }]), right);
        assert.equal(plan.resolve([{ index: 0, choice: "edited", source: "agreed\n" }]), "agreed\n");
      }
    }
  }
});

test("a concurrent move cannot restore source that the other version deleted", () => {
  const base = "remove me\nkeep me\n", local = "keep me\n", disk = "keep me\nremove me\n";
  for (const [left, right] of [[local, disk], [disk, local]]) {
    const plan = make().plan(base, left, right);
    assert.equal(plan.action, "conflict");
    assert.equal(plan.conflicts.length, 1);
    assert.equal(plan.resolve([{ index: 0, choice: "local" }]), left);
    assert.equal(plan.resolve([{ index: 0, choice: "disk" }]), right);
  }
});

test("unchanged repeated headings and blank lines remain useful context for independent edits", () => {
  const base = "\ufeff# Repeated\r\n\r\nfirst 😀\r\n\r\n# Repeated\r\n\r\nlast 日本";
  const local = base.replace("first 😀", "local 😀");
  const disk = base.replace("last 日本", "disk 日本");
  const expected = local.replace("last 日本", "disk 日本");
  for (const [left, right] of [[local, disk], [disk, local]]) {
    assert.equal(make()(base, left, right).source, expected);
    assert.equal(make().plan(base, left, right).source, expected);
    assert.equal(make().planParagraphs(base, left, right).source, expected);
  }
});

test("identical and one-sided moves or repeated deletions still need no diff allowance", () => {
  const base = "same\nsame\ntail\n";
  for (const source of ["same\ntail\n", "tail\nsame\nsame\n"]) {
    const merger = make({ cells: 1, lines: 1 });
    assert.equal(merger(base, source, base).source, source);
    assert.equal(merger(base, base, source).source, source);
    assert.equal(merger(base, source, source).source, source);
    assert.equal(merger.plan(base, source, source).source, source);
  }
});

test("novel duplicate insertions keep their exact content at independent positions", () => {
  const base = "first\nmiddle\nlast\n";
  const local = "new\nnew\nfirst\nmiddle\nlast\n";
  const disk = "first\nmiddle\nlast\nend\nend\n";
  assert.equal(make()(base, local, disk).source, "new\nnew\nfirst\nmiddle\nlast\nend\nend\n");
});

test("sparse long-source diffs also preserve ambiguous versions for explicit review", () => {
  const base = Array.from({ length: 3000 }, (_, i) => "line-" + i + "\n");
  base[3] = base[4] = "repeated\n";
  const local = base.slice(), disk = base.slice();
  local[4] = ""; local[2997] = "local end\n";
  disk[3] = "disk repeated\n"; disk[2900] = "disk end\n";
  const versions = [base, local, disk].map(lines => lines.join(""));
  for (const [left, right] of [[versions[1], versions[2]], [versions[2], versions[1]]]) {
    const plan = make().plan(versions[0], left, right);
    assert.equal(plan.action, "conflict");
    assert.equal(plan.conflicts.length, 1);
    assert.equal(plan.conflicts[0].base, versions[0]);
    assert.equal(plan.resolve([{ index: 0, choice: "local" }]), left);
    assert.equal(plan.resolve([{ index: 0, choice: "disk" }]), right);
  }
});

// Independent tiny-input oracle: enumerate every longest matching coordinate
// sequence. Production uses one bounded diff plus occurrence checks, never
// this intentionally exponential enumeration.
function align(base, next) {
  const memo = new Map();
  function solve(i, j) {
    const key = i + ":" + j;
    if (memo.has(key)) return memo.get(key);
    if (i === base.length || j === next.length) return [[]];
    const found = [];
    if (base[i] === next[j])
      for (const rest of solve(i + 1, j + 1)) found.push([[i, j], ...rest]);
    found.push(...solve(i + 1, j), ...solve(i, j + 1));
    const length = Math.max(...found.map(matches => matches.length));
    const value = [...new Map(found.filter(matches => matches.length === length)
      .map(matches => [JSON.stringify(matches), matches])).values()];
    memo.set(key, value);
    return value;
  }
  return solve(0, 0).map(matches => {
    const edits = [];
    let i = 0, j = 0;
    for (const [bi, nj] of [...matches, [base.length, next.length]]) {
      if (bi !== i || nj !== j) edits.push({ start: i, end: bi, lines: next.slice(j, nj) });
      i = bi + 1; j = nj + 1;
    }
    return edits;
  });
}
const overlaps = (a, b) => a.start === a.end ? a.start >= b.start && a.start <= b.end
  : b.start === b.end ? b.start >= a.start && b.start <= a.end : a.start < b.end && b.start < a.end;
function combined(base, left, right) {
  const edits = [...left];
  for (const edit of right) {
    const touching = left.filter(other => overlaps(other, edit));
    if (touching.some(other => JSON.stringify(other) !== JSON.stringify(edit))) return null;
    if (!touching.length) edits.push(edit);
  }
  edits.sort((a, b) => a.start - b.start);
  const output = [];
  let at = 0;
  for (const edit of edits) {
    output.push(...base.slice(at, edit.start), ...edit.lines);
    at = edit.end;
  }
  output.push(...base.slice(at));
  return output.join("");
}

test("every admitted merge in 24,360 line triples agrees with all shortest alignments", t => {
  const versions = [];
  for (let n = 1; n <= 4; n++)
    for (let bits = 0; bits < 2 ** n; bits++)
      versions.push(Array.from({ length: n }, (_, i) => (bits >> i) & 1 ? "b\n" : "a\n"));
  let cases = 0, admitted = 0, checks = 0;
  for (const base of versions) for (const local of versions) for (const disk of versions) {
    const [b, l, r] = [base, local, disk].map(lines => lines.join(""));
    if (b === l || b === r || l === r) continue;
    cases++;
    const actual = make()(b, l, r);
    if (actual.action !== "merged") continue;
    admitted++;
    for (const left of align(base, local)) for (const right of align(base, disk)) {
      checks++;
      assert.equal(combined(base, left, right), actual.source, JSON.stringify({ b, l, r, left, right }));
    }
  }
  assert.equal(cases, 24360);
  assert(admitted > 0);
  assert(checks >= admitted);
  t.diagnostic(cases + " triples; " + admitted + " admitted merges; " + checks + " alignment pairs");
});
