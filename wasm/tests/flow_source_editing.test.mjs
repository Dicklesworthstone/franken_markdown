import test from "node:test";
import assert from "node:assert/strict";
import { FLOW_SOURCE_LIMIT } from "../flow_session.mjs";
import { createSourceHistory, findSourceMatches, replaceSourceMatches } from "../demo/flow_document.mjs";
const code = expected => error => error.code === expected;

test("history reverses edits and restores both selections without whole-document checkpoints", () => {
  const history = createSourceHistory("one 😀 three");
  const before = { start: 4, end: 6, direction: "backward" }, after = { start: 5, end: 5, direction: "none" };
  assert.equal(history.record("one X three", { before, after }), true);
  assert.deepEqual(history.undo(), { source: "one 😀 three", selection: before });
  assert.deepEqual(history.redo(), { source: "one X three", selection: after });
  assert.equal(history.redo(), null);
  assert.equal(history.state.entries, 1);
  assert.equal(history.state.payloadBytes, 134);
});

test("no-op edits preserve redo while a new edit branches from the current source", () => {
  const history = createSourceHistory("a");
  history.record("ab"); history.record("abc"); history.undo();
  assert.equal(history.record("ab"), false);
  assert.equal(history.state.canRedo, true);
  history.record("abd");
  assert.equal(history.state.canRedo, false);
  assert.equal(history.undo().source, "ab");
  assert.equal(history.undo().source, "a");
  assert.equal(history.undo(), null);
});

test("invalid source or selection does not discard redo or alter history", () => {
  const history = createSourceHistory("original");
  history.record("changed"); history.undo();
  const before = history.state;
  assert.throws(() => history.record("\ud800"), code("INVALID_UNICODE"));
  assert.throws(() => history.record("valid", { after: { start: 6 } }), code("INVALID_SELECTION"));
  assert.throws(() => history.record("valid", { before: { start: -1 } }), code("INVALID_SELECTION"));
  assert.throws(() => history.record("valid", { secret: "not source" }), code("INVALID_ARGUMENT"));
  assert.deepEqual(history.state, before);
  assert.equal(history.source, "original");
  assert.equal(history.redo().source, "changed");
});

test("entry and byte limits evict only the oldest undo steps", () => {
  const history = createSourceHistory("", { maxEntries: 2, maxBytes: 300 });
  history.record("a"); history.record("ab"); history.record("abc");
  assert.equal(history.state.entries, 2);
  assert.equal(history.state.payloadBytes, 260);
  assert.equal(history.undo().source, "ab");
  assert.equal(history.undo().source, "a");
  assert.equal(history.undo(), null);
  assert.equal(history.redo().source, "ab");
  assert.equal(history.redo().source, "abc");
  const bytes = createSourceHistory("", { maxBytes: 260 });
  bytes.record("a"); bytes.record("ab"); bytes.record("abc");
  assert.equal(bytes.state.entries, 2);
  assert.ok(bytes.state.payloadBytes <= 260);
});

test("an individually oversized history entry is refused transactionally", () => {
  const history = createSourceHistory("x", { maxBytes: 132 });
  history.record("y"); history.undo();
  const before = history.state;
  assert.throws(() => history.record("longer"), code("BUDGET_EXCEEDED"));
  assert.equal(history.source, "x"); assert.deepEqual(history.state, before);
  assert.equal(history.redo().source, "y");
});

test("a one-character edit of a maximum-size document retains only the changed payload", () => {
  const source = "a".repeat(FLOW_SOURCE_LIMIT);
  const history = createSourceHistory(source);
  history.record(source.slice(0, -1) + "b");
  assert.equal(history.state.payloadBytes, 132);
  assert.equal(history.undo().source, source);
});

test("default history can undo a full replacement between two maximum-size sources", () => {
  const source = "a".repeat(FLOW_SOURCE_LIMIT), next = "b".repeat(FLOW_SOURCE_LIMIT);
  const history = createSourceHistory(source); history.record(next);
  assert.equal(history.undo().source, source);
  assert.equal(history.redo().source, next);
});

test("patch boundaries preserve surrogate pairs, BOM and decomposed Unicode exactly", () => {
  for (const [before, after] of [["😀", "😁"], ["𐀀", "𐐀"], ["x😀y", "x😁y"], ["😀x", "x"],
    ["", "😀"], ["a\ufeffb", "aXb"], ["e\u0301\r\n", "é\n"], ["😀", "😀😀"]]) {
    const history = createSourceHistory(before); history.record(after);
    assert.equal(history.undo().source, before);
    assert.equal(history.redo().source, after);
  }
});

test("seeded Unicode edit sequences round-trip all retained transactions", () => {
  let state = 0x6d2b79f5;
  const random = n => { state = (Math.imul(state, 1664525) + 1013904223) >>> 0; return state % n; };
  const alphabet = ["a", "b", "😀", "😁", "𐀀", "𐐀", "\ufeff", "\r", "\n", "é", "e\u0301", "[", "]"];
  for (let run = 0; run < 30; run++) {
    const history = createSourceHistory(""), versions = [""];
    for (let edit = 0; edit < 40; edit++) {
      const chars = Array.from(versions.at(-1)), start = random(chars.length + 1);
      chars.splice(start, random(chars.length - start + 1), alphabet[random(alphabet.length)]);
      const next = chars.join("");
      if (history.record(next)) versions.push(next);
    }
    for (let i = versions.length - 2; i >= 0; i--) assert.equal(history.undo().source, versions[i]);
    for (let i = 1; i < versions.length; i++) assert.equal(history.redo().source, versions[i]);
  }
});

test("history validates limits and releases all operations on disposal", () => {
  for (const options of [null, [], { maxEntries: 0 }, { maxEntries: 1001 }, { maxBytes: 127 }, { maxBytes: Infinity }, { extra: 1 }]) {
    assert.throws(() => createSourceHistory("", options), code("INVALID_ARGUMENT"));
  }
  const history = createSourceHistory("private"); history.record("other"); history.dispose(); history.dispose();
  for (const action of [() => history.source, () => history.state, () => history.undo(), () => history.redo(), () => history.record("x")]) {
    assert.throws(action, code("SESSION_DISPOSED"));
  }
});

test("literal search uses original UTF-16 coordinates and non-overlapping matches", () => {
  assert.deepEqual(findSourceMatches("😀 [x] [x]", "[x]"), {
    matches: [{ start: 3, end: 6 }, { start: 7, end: 10 }], truncated: false
  });
  assert.deepEqual(findSourceMatches("aaaaa", "aa").matches, [{ start: 0, end: 2 }, { start: 2, end: 4 }]);
  assert.deepEqual(findSourceMatches("a\nb\na\nb", "a\nb").matches, [{ start: 0, end: 3 }, { start: 4, end: 7 }]);
  assert.equal(findSourceMatches("<script>x</script>", "<script>").matches.length, 1);
  assert.equal(findSourceMatches(".* $1 \\a", ".*").matches.length, 1);
});

test("ASCII case folding never shifts Unicode offsets or changes non-ASCII matching", () => {
  const source = "İ 😀 Aa aA Ä ä";
  assert.deepEqual(findSourceMatches(source, "aa", { ignoreCase: true }).matches, [{ start: 5, end: 7 }, { start: 8, end: 10 }]);
  assert.equal(findSourceMatches(source, "ä", { ignoreCase: true }).matches.length, 1);
  assert.equal(findSourceMatches(source, "aa").matches.length, 0);
});

test("search rejects empty, oversized, malformed and unsupported inputs", () => {
  assert.throws(() => findSourceMatches("a", ""), code("EMPTY_QUERY"));
  assert.throws(() => findSourceMatches("a", "a".repeat(1025)), code("QUERY_TOO_LONG"));
  assert.throws(() => findSourceMatches("a", "\ud800"), code("INVALID_UNICODE"));
  assert.throws(() => findSourceMatches("\udc00", "a"), code("INVALID_UNICODE"));
  assert.throws(() => findSourceMatches("a", null), code("INVALID_ARGUMENT"));
  assert.throws(() => findSourceMatches("a", "a", { ignoreCase: 1 }), code("INVALID_ARGUMENT"));
  assert.throws(() => findSourceMatches("a", "a", { regex: true }), code("INVALID_ARGUMENT"));
});

test("match inventory is bounded and replace-all never silently uses a truncated result", () => {
  const exact = "a".repeat(10000), over = exact + "a";
  assert.equal(findSourceMatches(exact, "a").truncated, false);
  const found = findSourceMatches(over, "a");
  assert.equal(found.matches.length, 10000); assert.equal(found.truncated, true);
  assert.equal(replaceSourceMatches(exact, "a", "").source, "");
  assert.throws(() => replaceSourceMatches(over, "a", "b"), code("TOO_MANY_MATCHES"));
});

test("repetitive maximum-size input supports a long nearly matching query", () => {
  const source = "a".repeat(FLOW_SOURCE_LIMIT), query = "a".repeat(1023) + "b";
  assert.deepEqual(findSourceMatches(source, query), { matches: [], truncated: false });
});

test("replacement text is literal, including dollar expansion syntax, slashes and HTML", () => {
  const replacement = "$& $1 \\ <script>";
  const result = replaceSourceMatches("A a", "a", replacement, { ignoreCase: true });
  assert.deepEqual(result, { source: `${replacement} ${replacement}`, count: 2 });
  assert.deepEqual(replaceSourceMatches("unchanged", "missing", "x"), { source: "unchanged", count: 0 });
  assert.deepEqual(replaceSourceMatches("aa", "a", "a"), { source: "aa", count: 2 });
});

test("replace-all admits exact UTF-8 byte boundaries and refuses expansion before joining", () => {
  const source = "a".repeat(FLOW_SOURCE_LIMIT - 4) + "X";
  assert.equal(new TextEncoder().encode(replaceSourceMatches(source, "X", "😀").source).length, FLOW_SOURCE_LIMIT);
  assert.throws(() => replaceSourceMatches(source, "X", "😀x"), code("BUDGET_EXCEEDED"));
  assert.throws(() => replaceSourceMatches("a".repeat(10000), "a", "x".repeat(1000)), code("BUDGET_EXCEEDED"));
  assert.throws(() => replaceSourceMatches("a", "a", "\ud800"), code("INVALID_UNICODE"));
});

test("literal search agrees with indexOf on a seeded source/query corpus", () => {
  let state = 73;
  const random = n => { state = (Math.imul(state, 1103515245) + 12345) >>> 0; return state % n; };
  const alphabet = ["a", "b", "A", "😀", "[", "]", "\n", "é"];
  for (let run = 0; run < 200; run++) {
    const source = Array.from({ length: 80 }, () => alphabet[random(alphabet.length)]).join("");
    const query = Array.from({ length: 1 + random(5) }, () => alphabet[random(alphabet.length)]).join("");
    const matches = []; let at = 0;
    while ((at = source.indexOf(query, at)) !== -1) { matches.push({ start: at, end: at + query.length }); at += query.length; }
    assert.deepEqual(findSourceMatches(source, query).matches, matches);
    assert.equal(replaceSourceMatches(source, query, "$$").source, source.split(query).join("$$"));
  }
});
