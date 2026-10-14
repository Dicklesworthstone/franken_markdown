import assert from "node:assert/strict";
import test from "node:test";
import { createBookSourceHistory } from "../demo/book_source_history.mjs";

const file = (path, source = path, role) => ({ path, source, ...(role === "include" ? { role } : {}) });
const create = (files, limits) => createBookSourceHistory(files, limits, { trackStructure: true });
function step(history, direction) {
  const plan = history.plan(direction);
  history.commit(plan, plan.files);
  return plan.files;
}
const initial = [file("a.md", "\ufeff# A\r\n😀"), file("part.txt", "shared\r", "include"), file("b.md", "# B")];

for (const [name, after] of [
  ["add chapter", [...initial, file("c.md", "# C")]],
  ["add include", [...initial, file("code.rs", "fn main() {}", "include")]],
  ["remove chapter", initial.slice(1)],
  ["remove every source", []],
  ["reorder", [...initial].reverse()],
  ["rename", initial.map((f, i) => i === 0 ? { ...f, path: "guide/a.md" } : f)],
  ["change role", initial.map((f, i) => i === 0 ? { ...f, role: "include" } : f)],
  ["mixed structural/source transaction", [file("x.md", "# New"), initial[2], { ...initial[0], source: "revised" }]],
  ["draft paths and exact UTF-16", [file("", "\ud800\r\n"), file("", "\ufeff", "include")]],
]) {
  test(`${name} is one reversible transaction and keeps older typing recovery`, () => {
    const beforeTyping = initial.map((f, i) => i === 0 ? { ...f, source: "before typing" } : f);
    const h = create(beforeTyping);
    h.record(initial, { group: "typing" });
    assert.equal(h.record(after, { label: name }), "structure-recorded");
    assert.equal(h.state.undo, 2);
    const plan = h.plan("undo");
    assert.equal(plan.structure, true); assert.equal(plan.label, name);
    assert.deepEqual(plan.files, initial); assert.equal(h.state.undo, 2);
    h.commit(plan, initial);
    assert.deepEqual(step(h, "undo"), beforeTyping);
    assert.deepEqual(step(h, "redo"), initial);
    assert.deepEqual(step(h, "redo"), after);
  });
}

test("empty book can gain, lose and regain its first chapter", () => {
  const h = create([]), one = [file("one.md", "# First")];
  h.record(one); assert.deepEqual(step(h, "undo"), []);
  assert.deepEqual(step(h, "redo"), one);
  h.record([]); assert.deepEqual(step(h, "undo"), one);
  assert.deepEqual(step(h, "redo"), []);
});

test("large-book reorders and renames retain metadata, not whole source copies", () => {
  const a = file("a.md", "a".repeat(2 * 1024 * 1024)), b = file("b.md", "b".repeat(2 * 1024 * 1024));
  const h = create([a, b], { bytes: 2048 });
  h.record([b, a]); h.record([b, { ...a, path: "renamed.md", role: "include" }]);
  assert.equal(h.state.undo, 2); assert(h.state.bytes < 1024);
  assert.deepEqual(step(h, "undo"), [b, a]);
  assert.deepEqual(step(h, "undo"), [a, b]);
  assert.deepEqual(step(h, "redo"), [b, a]);
  assert.deepEqual(step(h, "redo"), [b, { ...a, path: "renamed.md", role: "include" }]);
});

test("identical source text with distinct roles/paths is reconstructed without identity confusion", () => {
  const before = [file("a.md", "same"), file("b.md", "same"), file("x.txt", "same", "include")];
  const after = [file("x.txt", "same", "include"), file("renamed.md", "same"), file("a.md", "same")];
  const h = create(before); h.record(after);
  assert.deepEqual(step(h, "undo"), before); assert.deepEqual(step(h, "redo"), after);
});

test("typing groups never swallow a structural transaction", () => {
  const h = create([file("a.md", "A")]);
  h.record([file("a.md", "AB")], { group: "same" });
  h.record([file("b.md", "AB")], { group: "same" });
  h.record([file("b.md", "ABC")], { group: "same" });
  assert.equal(h.state.undo, 3);
  assert.deepEqual(step(h, "undo"), [file("b.md", "AB")]);
  assert.deepEqual(step(h, "undo"), [file("a.md", "AB")]);
  assert.deepEqual(step(h, "undo"), [file("a.md", "A")]);
});

test("divergent tree edits retire old plans and the redo branch only", () => {
  const h = create(initial); h.record([...initial].reverse()); h.record([]);
  step(h, "undo"); const stale = h.plan("redo");
  h.record([...initial].reverse().slice(1));
  assert.equal(h.state.redo, 0); assert.equal(h.state.undo, 2);
  assert.throws(() => h.commit(stale, []), { code: "HISTORY_CONFLICT" });
  assert.deepEqual(step(h, "undo"), [...initial].reverse());
  assert.deepEqual(step(h, "undo"), initial);
});

test("failed host installation leaves structural undo retryable and rejects partial trees", () => {
  const h = create(initial); h.record([]);
  const plan = h.plan("undo"), before = h.state;
  assert.throws(() => h.commit(plan, initial.slice(1)), { code: "HISTORY_CONFLICT" });
  assert.deepEqual(h.state, before);
  h.commit(plan, initial); assert.deepEqual(step(h, "redo"), []);
});

test("structural byte and entry budgets trim complete transactions and account for redo", () => {
  const h = create(initial, { entries: 2, bytes: 4096 });
  h.record([]); h.record([file("n.md", "new")]); h.record([file("m.md", "new")]);
  assert.equal(h.state.undo, 2); assert.equal(h.state.discarded, 1);
  const bytes = h.state.bytes; step(h, "undo"); assert.equal(h.state.bytes, bytes);
  assert.deepEqual(step(h, "undo"), []);
  assert.throws(() => h.plan("undo"), { code: "NO_HISTORY" });
  assert.deepEqual(step(h, "redo"), [file("n.md", "new")]);
});

test("oversized deleted text is an explicit history boundary, never a partial restoration", () => {
  const h = create(initial, { bytes: 256 });
  const large = [file("large.md", "x".repeat(1000))];
  h.record(large); assert.equal(h.state.undo, 0); assert.equal(h.state.bytes, 0);
  h.record([]); assert.equal(h.state.undo, 0); assert(h.state.discarded >= 2);
  h.record([file("a.md", "a")]); assert.deepEqual(step(h, "undo"), []);
});

test("structural no-op preserves plan and redo while reset retires both", () => {
  const h = create(initial); h.record([]); step(h, "undo");
  const plan = h.plan("redo"); assert.equal(h.record(structuredClone(initial)), "unchanged");
  h.commit(plan, []); h.reset(initial);
  assert.equal(h.state.undo, 0); assert.equal(h.state.redo, 0); assert.equal(h.state.bytes, 0);
  assert.throws(() => h.commit(plan, []), { code: "HISTORY_CONFLICT" });
});

test("caller edits and accessors cannot retarget the recorded tree or execute during capture", () => {
  const before = structuredClone(initial), h = create(before), after = [file("x.md", "x")];
  h.record(after); before[0].source = "bad"; after[0].path = "bad";
  const plan = h.plan("undo"); assert.deepEqual(plan.files, initial);
  assert(Object.isFrozen(plan.files)); assert(Object.isFrozen(plan.files[0]));
  let calls = 0;
  assert.throws(() => h.record([{ get path() { calls++; return "evil"; }, source: "x" }]), { code: "INVALID_HISTORY_SOURCE" });
  assert.equal(calls, 0); h.commit(plan, initial);
});

test("legacy source-only callers retain their structure-reset contract", () => {
  const h = createBookSourceHistory(initial); h.record(initial.map(f => ({ ...f, source: f.source + "!" })));
  assert.equal(h.record([]), "source-set-changed"); assert.equal(h.state.undo, 0);
  assert.throws(() => createBookSourceHistory([], undefined, { trackStructure: "yes" }), { code: "INVALID_HISTORY_OPTIONS" });
});

test("seeded mixed add/remove/rename/reorder/role/text histories match a snapshot oracle", () => {
  let seed = 0x71eecafe, serial = 0;
  const random = n => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return (seed >>> 0) % n; };
  for (let run = 0; run < 30; run++) {
    let value = structuredClone(initial), past = [], future = [];
    const h = create(value);
    for (let action = 0; action < 250; action++) {
      const op = random(8);
      if (op === 0 && past.length) {
        future.push(value); value = past.pop(); assert.deepEqual(step(h, "undo"), value);
      } else if (op === 1 && future.length) {
        past.push(value); value = future.pop(); assert.deepEqual(step(h, "redo"), value);
      } else {
        const next = value.map(f => ({ ...f })), index = random(Math.max(1, next.length));
        if (!next.length || op === 2) next.splice(index, 0, file(`new-${++serial}.md`, ["", "same", "\ufeff😀\r\n"][random(3)]));
        else if (op === 3) next.splice(index, 1);
        else if (op === 4) next.reverse();
        else if (op === 5) next[index].path = `moved-${++serial}.md`;
        else if (op === 6) { if (next[index].role) delete next[index].role; else next[index].role = "include"; }
        else next[index].source += ["X", "\ud800", "\r", "日本"][random(4)];
        if (JSON.stringify(next) !== JSON.stringify(value)) {
          past.push(value); if (past.length > 100) past.shift(); future = []; value = next;
        }
        h.record(next);
      }
      assert(h.matches(value)); assert.equal(h.state.undo, past.length); assert.equal(h.state.redo, future.length);
      assert(h.state.bytes >= 0);
    }
  }
});
