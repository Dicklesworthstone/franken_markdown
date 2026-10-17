import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger } from "../demo/book_source_merge.mjs";

const source = n => Array.from({ length: n }, (_, i) => `original-${i}\n`);
const merge = (base, local, disk, limits) => createBookSourceMerger(limits)(base.join(""), local.join(""), disk.join(""));
function sparse(n) {
  const base = source(n), local = base.slice(), disk = base.slice(), expected = base.slice();
  local[2] = expected[2] = "local first\n";
  local[n - 3] = expected[n - 3] = "local last\n";
  disk[10] = expected[10] = "disk first\n";
  disk[n - 12] = expected[n - 12] = "disk last\n";
  return { base, local, disk, expected };
}

test("multiple distant edits in a ten-thousand-line chapter now merge exactly", () => {
  const { base, local, disk, expected } = sparse(10000);
  const value = merge(base, local, disk);
  assert.equal(value.action, "merged");
  assert.equal(value.source, expected.join(""));
  assert.equal(merge(base, disk, local).source, expected.join(""));
});
test("sparse changes preserve BOM, mixed terminators, astral text and an unterminated last line", () => {
  const base = Array.from({ length: 5000 }, (_, i) => `${i} 日本 😀${["\n", "\r\n", "\r"][i % 3]}`);
  base[0] = "\ufeff" + base[0]; base[base.length - 1] = "final 😀";
  const local = base.slice(), disk = base.slice(), expected = base.slice();
  for (const at of [1, 4900]) local[at] = expected[at] = `LOCAL ${at}\r\n`;
  for (const at of [200, 4500]) disk[at] = expected[at] = `DISK ${at}\r`;
  assert.equal(merge(base, local, disk).source, expected.join(""));
});
test("distant insertions and deletions retain unchanged intervals and EOF", () => {
  const base = source(4000), local = base.slice(), disk = base.slice(), expected = base.slice();
  local[2] = expected[2] = "added before\n" + base[2];
  local[3900] = expected[3900] = "";
  disk[200] = expected[200] = "";
  disk[3000] = expected[3000] = "replacement\n";
  disk.push("new final without newline"); expected.push("new final without newline");
  const value = merge(base, local, disk);
  assert.equal(value.action, "merged"); assert.equal(value.source, expected.join(""));
});
test("shared distant hunks are applied once, not duplicated by backtracking", () => {
  const { base, local, disk, expected } = sparse(3000);
  local[1500] = disk[1500] = expected[1500] = "shared\nextra\n";
  local[1600] = disk[1600] = expected[1600] = "";
  assert.equal(merge(base, local, disk).source, expected.join(""));
});
test("overlap still refuses even after a sparse search successfully reconstructs both sides", () => {
  const { base, local, disk } = sparse(3000);
  local[1500] = "our edit\n"; disk[1500] = "their edit\n";
  assert.deepEqual(merge(base, local, disk), { action: "conflict", source: null, reason: "overlapping-edits" });
});
test("insertions at replacement boundaries remain conflicts on the sparse path", () => {
  const { base, local, disk } = sparse(3000);
  local[1500] = "replace\n"; disk[1501] = "insertion\n" + disk[1501];
  assert.equal(merge(base, local, disk).reason, "overlapping-edits");
});
test("repeated unchanged lines are not removed or normalized by a sparse merge", () => {
  const base = Array.from({ length: 3000 }, (_, i) => i % 2 ? "\n" : `section ${i}\n`);
  const local = base.slice(), disk = base.slice(), expected = base.slice();
  for (const at of [2, 2900]) local[at] = expected[at] = `local ${at}\n`;
  for (const at of [100, 2700]) disk[at] = expected[at] = `disk ${at}\n`;
  assert.equal(merge(base, local, disk).source, expected.join(""));
});
test("dense divergent edits still stop within the shared work allowance", () => {
  const base = source(3000), local = base.map((_, i) => `local-${i}\n`), disk = base.map((_, i) => `disk-${i}\n`);
  const m = createBookSourceMerger({ cells: 1000 });
  assert.deepEqual(m(base.join(""), local.join(""), disk.join("")),
    { action: "conflict", source: null, reason: "merge-budget" });
  assert.equal(m("base", "base", "new disk").source, "new disk");
});
test("frontier work is shared across files and is not replenished after refusal", () => {
  const { base, local, disk } = sparse(256), inputs = [base, local, disk].map(x => x.join(""));
  const m = createBookSourceMerger({ cells: 5000 });
  assert.equal(m(...inputs).action, "merged");
  let refusal;
  for (let i = 0; i < 20; i++) {
    const value = m(...inputs);
    if (value.action === "conflict") { refusal = value; break; }
  }
  assert.equal(refusal?.reason, "merge-budget");
  assert.equal(m(...inputs).reason, "merge-budget");
  assert.equal(createBookSourceMerger({ cells: 5000 })(...inputs).action, "merged");
});
test("a narrow source/line budget cannot be bypassed by the fallback", () => {
  const { base, local, disk } = sparse(256);
  assert.equal(merge(base, local, disk, { cells: 10 }).reason, "merge-budget");
  assert.equal(merge(base, local, disk, { lines: 767 }).reason, "merge-budget");
});
test("seeded sparse insert/delete/replace combinations agree with an independent source oracle", () => {
  let seed = 0x534f5552;
  const random = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  for (let trial = 0; trial < 500; trial++) {
    const { base, local, disk, expected } = sparse(256);
    for (let at = 20; at < 220; at += 20) {
      switch (random(7)) {
        case 0: local[at] = expected[at] = `local ${trial}-${at}\n`; break;
        case 1: disk[at] = expected[at] = `disk ${trial}-${at}\n`; break;
        case 2: local[at] = disk[at] = expected[at] = `shared ${at}\n`; break;
        case 3: local[at] = expected[at] = `insert ${at}\n` + base[at]; break;
        case 4: disk[at] = expected[at] = ""; break;
        case 5: local[at] = disk[at] = expected[at] = ""; break;
      }
    }
    for (const [a, b] of [[local, disk], [disk, local]]) {
      const value = merge(base, a, b, { cells: 16000 });
      assert.equal(value.action, "merged", `trial ${trial}: ${value.reason}`);
      assert.equal(value.source, expected.join(""), `trial ${trial}`);
    }
  }
});
