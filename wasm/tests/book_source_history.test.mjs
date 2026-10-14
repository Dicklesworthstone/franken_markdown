import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceHistory, BOOK_SOURCE_HISTORY_LIMITS } from "../demo/book_source_history.mjs";

const sources = (...text) => text.map((source, i) => ({ path: `${i}.md`, source }));
const replace = (files, index, source) => files.map((file, i) => i === index ? { ...file, source } : { ...file });
function step(history, direction) {
  const plan = history.plan(direction);
  history.commit(plan, plan.files);
  return plan.files;
}

test("source edits across chapters and includes undo and redo in chronological order", () => {
  const initial = [{ path: "start.md", source: "# Start\n" }, { path: "part.txt", role: "include", source: "shared" }];
  const history = createBookSourceHistory(initial);
  const a = replace(initial, 0, "# Changed\n"), b = replace(a, 1, "shared changed");
  history.record(a); history.record(b);
  assert.deepEqual(step(history, "undo"), a);
  assert.deepEqual(step(history, "undo"), initial);
  assert.deepEqual(step(history, "redo"), a);
  assert.deepEqual(step(history, "redo"), b);
});

test("a multi-file replacement replays as one atomic source plan", () => {
  const before = sources("cat", "cat cat", "untouched");
  const after = sources("dog", "dog dog", "untouched");
  const history = createBookSourceHistory(before);
  history.record(after, { label: "Replace across book" });
  assert.equal(history.state.undo, 1);
  const plan = history.plan("undo");
  assert.deepEqual(plan.changed, [0, 1]); assert.deepEqual(plan.files, before);
  assert.equal(plan.label, "Replace across book");
  assert.equal(history.state.undo, 1, "preparing does not acknowledge installation");
  history.commit(plan, before);
  assert.deepEqual(step(history, "redo"), after);
});

test("ordinary edits after a batch do not erase recovery of the batch", () => {
  const initial = sources("A", "B"), batch = sources("a", "b"), typing = sources("a!", "b");
  const history = createBookSourceHistory(initial);
  history.record(batch); history.record(typing);
  assert.deepEqual(step(history, "undo"), batch);
  assert.deepEqual(step(history, "undo"), initial);
});

test("no-op captures preserve redo and do not invalidate a prepared plan", () => {
  const history = createBookSourceHistory(sources("a")); history.record(sources("ab"));
  step(history, "undo");
  const plan = history.plan("redo");
  assert.equal(history.record(sources("a")), "unchanged");
  history.commit(plan, sources("ab"));
  assert.equal(history.state.undo, 1); assert.equal(history.state.redo, 0);
});

test("a divergent edit after undo discards only the redo branch", () => {
  const history = createBookSourceHistory(sources("a"));
  history.record(sources("b")); history.record(sources("c")); step(history, "undo");
  history.record(sources("d"));
  assert.equal(history.state.redo, 0); assert.equal(history.state.undo, 2);
  assert.deepEqual(step(history, "undo"), sources("b"));
  assert.deepEqual(step(history, "undo"), sources("a"));
});

test("typing groups merge reversibly, including insertions, deletion and cancellation", () => {
  const initial = sources("abc"), history = createBookSourceHistory(initial);
  history.record(sources("abXc"), { group: "typing" });
  history.record(sources("abXYc"), { group: "typing" });
  history.record(sources("aXYc"), { group: "typing" });
  assert.equal(history.state.undo, 1);
  assert.deepEqual(step(history, "undo"), initial);
  assert.deepEqual(step(history, "redo"), sources("aXYc"));
  history.breakGroup(); history.record(sources("abc"), { group: "typing" });
  history.record(sources("aXYc"), { group: "typing" });
  assert.equal(history.state.undo, 1, "typing back to the group baseline removes only that group");
});

test("group boundaries prevent merging separate typing bursts or different files", () => {
  const history = createBookSourceHistory(sources("a", "b"));
  history.record(sources("aa", "b"), { group: "t" }); history.breakGroup();
  history.record(sources("aaa", "b"), { group: "t" });
  history.record(sources("aaa", "bb"), { group: "t" });
  assert.equal(history.state.undo, 3);
  assert.deepEqual(step(history, "undo"), sources("aaa", "b"));
});

test("multi-file edits are never absorbed by a typing group", () => {
  const history = createBookSourceHistory(sources("a", "b"));
  history.record(sources("aa", "b"), { group: "t" });
  history.record(sources("AAA", "BBB"), { group: "t" });
  history.record(sources("AAAA", "BBB"), { group: "t" });
  assert.equal(history.state.undo, 3);
});

test("BOM, mixed line endings, astral characters and incomplete draft UTF-16 remain exact", () => {
  for (const before of ["\ufeff# One\r\nText\rEnd\n", "😀😅日本語", "\ud800x\udc00", "", "ab"]) {
    for (const after of ["\ufeff# Two\r\nText\rEnd\n", "😁😅日本語!", "\ud800y\udc00", "", "abc"]) {
      if (before === after) continue;
      const history = createBookSourceHistory(sources(before)); history.record(sources(after));
      assert.deepEqual(step(history, "undo"), sources(before));
      assert.deepEqual(step(history, "redo"), sources(after));
    }
  }
});

test("one-character edits to large books retain small deltas rather than book snapshots", () => {
  const source = "a".repeat(2 * 1024 * 1024), history = createBookSourceHistory(sources(source, source));
  history.record(sources(source + "X", source));
  history.record(sources(source + "XY", source));
  assert.equal(history.state.bytes, 4);
  assert.deepEqual(step(history, "undo"), sources(source + "X", source));
  assert.deepEqual(step(history, "undo"), sources(source, source));
});

test("entry and delta-byte budgets evict oldest entries without breaking the remaining chain", () => {
  const history = createBookSourceHistory(sources("a"), { entries: 2, bytes: 8 });
  for (const source of ["b", "c", "d"]) history.record(sources(source));
  assert.equal(history.state.undo, 2); assert.equal(history.state.bytes, 8); assert.equal(history.state.discarded, 1);
  assert.deepEqual(step(history, "undo"), sources("c"));
  assert.deepEqual(step(history, "undo"), sources("b"));
  assert.throws(() => step(history, "undo"), { code: "NO_HISTORY" });
  assert.deepEqual(step(history, "redo"), sources("c"));
  assert.deepEqual(step(history, "redo"), sources("d"));
});

test("an over-budget transaction becomes an explicit boundary, not a partial undo", () => {
  const history = createBookSourceHistory(sources("a"), { bytes: 4 });
  history.record(sources("b")); history.record(sources("a very large change"));
  assert.equal(history.state.undo, 0); assert.equal(history.state.bytes, 0);
  assert.equal(history.matches(sources("a very large change")), true);
  history.record(sources("a very large change!"));
  assert.deepEqual(step(history, "undo"), sources("a very large change"));
});

test("redo payloads count toward the same budget and branching releases them", () => {
  const history = createBookSourceHistory(sources("a"));
  history.record(sources("b")); history.record(sources("c"));
  step(history, "undo"); assert.equal(history.state.bytes, 8);
  history.record(sources("bb")); assert.equal(history.state.bytes, 6);
});

test("source membership, path, role and order changes establish explicit boundaries", () => {
  for (const changed of [[], sources("x", "y"), [{ path: "renamed.md", source: "b" }],
    [{ path: "0.md", role: "include", source: "b" }]]) {
    const history = createBookSourceHistory(sources("a")); history.record(sources("b"));
    assert.equal(history.record(changed), "source-set-changed");
    assert.equal(history.state.undo, 0); assert.equal(history.state.redo, 0);
  }
});

test("failed and mismatched installation cannot acknowledge a plan", () => {
  const history = createBookSourceHistory(sources("a")); history.record(sources("b"));
  const plan = history.plan("undo"), before = history.state;
  assert.throws(() => history.commit(plan, sources("unrelated")), { code: "HISTORY_CONFLICT" });
  assert.deepEqual(history.state, before);
  history.commit(plan, sources("a")); assert.equal(history.state.redo, 1);
  assert.throws(() => history.commit(plan, sources("a")), { code: "HISTORY_CONFLICT" });
});

test("later edits, reset or a newer plan retire old plan identities", () => {
  for (const mutation of [h => h.record(sources("c")), h => h.reset(sources("b")), h => h.plan("undo")]) {
    const history = createBookSourceHistory(sources("a")); history.record(sources("b"));
    const old = history.plan("undo"); mutation(history);
    assert.throws(() => history.commit(old, sources("a")), { code: "HISTORY_CONFLICT" });
  }
});

test("owned snapshots and plans cannot be retargeted by their callers", () => {
  const files = sources("a"), history = createBookSourceHistory(files); files[0].source = "evil";
  const next = sources("b"); history.record(next); next[0].path = "other.md";
  const plan = history.plan("undo");
  assert.throws(() => { plan.files[0].source = "evil"; }, TypeError);
  assert.deepEqual(plan.files, sources("a")); history.commit(plan, plan.files);
});

test("source accessors and custom array iterators cannot run during capture", () => {
  let calls = 0;
  const bad = [{ path: "a.md", get source() { calls++; return "x"; } }];
  assert.throws(() => createBookSourceHistory(bad), { code: "INVALID_HISTORY_SOURCE" });
  const files = sources("a"); files[Symbol.iterator] = () => { calls++; throw Error("iterator"); };
  const history = createBookSourceHistory(files);
  assert.equal(history.matches(sources("a")), true); assert.equal(calls, 0);
});

test("limit validation, explicit reset, and disposal", () => {
  for (const limits of [{ entries: 0 }, { bytes: -1 }, { entries: 101 }, { bytes: BOOK_SOURCE_HISTORY_LIMITS.bytes + 1 }])
    assert.throws(() => createBookSourceHistory([], limits), { code: "INVALID_HISTORY_LIMIT" });
  const history = createBookSourceHistory(sources("a")); history.record(sources("b"));
  history.reset(sources("c")); assert.equal(history.state.undo, 0);
  assert.throws(() => history.plan("sideways"), { code: "INVALID_HISTORY_DIRECTION" });
  history.dispose(); history.dispose();
  assert.throws(() => history.state, { code: "HISTORY_CLOSED" });
  assert.throws(() => history.record([]), { code: "HISTORY_CLOSED" });
});

test("seeded differential edit/undo/redo sequences agree with a full-snapshot oracle", () => {
  let seed = 0x51c0ffee;
  const random = n => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return (seed >>> 0) % n; };
  for (let run = 0; run < 20; run++) {
    let value = sources("\ufeffalpha\r\n😀", "beta\n", "shared"), past = [], future = [];
    const history = createBookSourceHistory(value);
    for (let operation = 0; operation < 200; operation++) {
      const direction = random(4);
      if (direction === 0 && past.length) {
        future.push(value); value = past.pop(); assert.deepEqual(step(history, "undo"), value);
      } else if (direction === 1 && future.length) {
        past.push(value); value = future.pop(); assert.deepEqual(step(history, "redo"), value);
      } else {
        const index = random(value.length), old = value[index].source, at = random(old.length + 1);
        const insert = ["X", "😀", "\r\n", "", "日本", "\ud800"][random(6)];
        const next = replace(value, index, old.slice(0, at) + insert + old.slice(Math.min(old.length, at + random(3))));
        if (next[index].source !== old) { past.push(value); if (past.length > 100) past.shift(); future = []; value = next; }
        history.record(next);
      }
      assert.equal(history.matches(value), true);
      assert.equal(history.state.undo, past.length); assert.equal(history.state.redo, future.length);
    }
  }
});
