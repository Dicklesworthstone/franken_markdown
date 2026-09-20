import test from "node:test";
import assert from "node:assert/strict";
import { readFlowDocument } from "./flow_reading.mjs";

// Explicit native-wire fixtures, not a second Markdown parser.
const item = (listId = "1", itemIndex = 0, extra = {}) => ({ listId, itemIndex, ordered: false, start: "1", task: null, ...extra });
const node = (role = "list-item", text = "item", listPath = [item()], extra = {}) => ({ role, text, listPath,
  bounds: { x: 0, y: 0, width: 100, height: 20 }, enclosingSourceSpan: { startByte: 0, endByte: 100 }, children: [], ...extra });
function session(roots, asyncMode = false) {
  return { disposed: false, token: { revision: "1", layoutRevision: "1" }, calls: [],
    readingOrder(options) {
      this.calls.push(options);
      const { offset, limit } = options, end = Math.min(roots.length, offset + limit);
      const result = { ...this.token, schemaVersion: 1, offset, total: roots.length, nextOffset: end < roots.length ? end : null, nodes: roots.slice(offset, end) };
      return asyncMode ? Promise.resolve(result) : result;
    } };
}
const read = (roots, options) => readFlowDocument(session(roots), options);
const reject = (roots, code = "INVALID_READING_DATA", options) => assert.rejects(read(roots, options), { code });

test("nested ordered lists retain starts, item positions and task state without guessing from bounds", async () => {
  const outer = item("1", 0, { ordered: true, start: "7" }), child = item("2", 0, { task: true });
  const roots = [node("list-item", "first", [outer]), node("paragraph", "continuation", [outer]),
    node("list-item", "[x] child", [outer, child]), node("code-block", " code\n", [outer, child]),
    node("paragraph", "after child", [outer]), node("list-item", "second", [{ ...outer, itemIndex: 1 }]),
    node("paragraph", "outside", [])];
  const doc = await read(roots);
  assert.equal(doc.nodes[0].listPath[0].start, "7");
  assert.equal(doc.nodes[2].listPath[1].task, true);
  assert.equal(doc.listEntryCount, 8);
  assert.equal(doc.find("continuation").matches.length, 1);
  assert.equal(doc.matchText(doc.find(" code\n").matches[0]), " code\n");
  assert.equal(doc.nodes[2].bounds.x, 0); // No geometry-based nesting inference.
  roots[0].listPath[0].start = "99";
  assert.equal(doc.nodes[0].listPath[0].start, "7");
  assert(Object.isFrozen(doc.nodes[0].listPath) && Object.isFrozen(doc.nodes[0].listPath[0]));
});

test("lists and continuation ownership survive real-sized synchronous and async page boundaries", async () => {
  const roots = Array.from({ length: 520 }, (_, i) => node(i === 0 ? "list-item" : "paragraph", `block ${i}`));
  for (const asyncMode of [false, true]) {
    const s = session(roots, asyncMode), doc = await readFlowDocument(s);
    assert.deepEqual(s.calls.map(o => o.offset), [0, 256, 512]);
    assert.equal(doc.listEntryCount, 520);
    assert.equal(doc.nodes[519].listPath[0].itemIndex, 0);
    assert.equal(doc.find("block 519").matches.length, 1);
  }
});

test("separate nested lists keep distinct IDs even with identical source spans and list type", async () => {
  const parent = item(), a = item("2"), b = item("3");
  const doc = await read([node(), node("list-item", "a", [parent, a]), node("list-item", "b", [parent, b])]);
  assert.equal(doc.nodes[1].enclosingSourceSpan.startByte, doc.nodes[2].enclosingSourceSpan.startByte);
  assert.notEqual(doc.nodes[1].listPath[1].listId, doc.nodes[2].listPath[1].listId);
});

test("empty/code-first items, images, headings and table rows retain one owner", async () => {
  const child = node("table-cell", "cell", []);
  const roots = [node("list-item", ""), node("code-block", "literal\n"), node("image", "alt"),
    node("heading", "heading", [item()], { level: 3 }), node("table-row", "cell", [item()], { children: [child] })];
  const doc = await read(roots);
  assert.equal(doc.nodes.length, 6);
  assert.equal(doc.headings[0].listPath[0].listId, "1");
  assert.deepEqual(doc.nodes[5].listPath, []);
  assert.equal(doc.find("cell").matches.length, 1);
});

test("legacy data stays distinct from known empty ownership", async () => {
  const old = node(); delete old.listPath;
  const doc = await read([old]);
  assert.equal(doc.nodes[0].listPath, null);
  assert.equal(doc.listEntryCount, 0);
  assert.deepEqual((await read([node("paragraph", "known outside", [])])).nodes[0].listPath, []);
  await reject([old, node("paragraph", "new", [])]);
  await reject([node("paragraph", "new", []), old]);
});

test("malformed and sparse paths or list fields reject admission", async () => {
  for (const listPath of [null, {}, new Array(1), [null], [item("0")], [item("01")], [item("-1")], [item(1)],
    [item("18446744073709551616")], [item("1", -1)], [item("1", 0.5)], [item("1", NaN)],
    [item("1", 0, { start: 3 })], [item("1", 0, { start: "03" })], [item("1", 0, { ordered: 1 })],
    [item("1", 0, { task: "checked" })], [item("1", 0, { extra: true })]]) await reject([node("list-item", "x", listPath)]);
  const missing = item(); delete missing.task;
  await reject([node("list-item", "x", [missing])]);
});

test("missing parents, missing item-leading blocks and duplicate markers are refused", async () => {
  await reject([node("list-item", "child", [item(), item("2")])]);
  await reject([node("paragraph", "continuation")]);
  await reject([node(), node()]);
  await reject([node("list-item", "ownerless", [])]);
  await reject([node("list-item", "wrong first position", [item("1", 2)])]);
  await reject([node("list-item", "parent", [item()], { children: [node("paragraph", "child", [])] })]);
});

test("item positions cannot skip, move backward or change list metadata", async () => {
  for (const frame of [item("1", 2), item("1", 1, { ordered: true }), item("1", 1, { start: "2" })]) {
    await reject([node(), node("list-item", "bad", [frame])]);
  }
  await reject([node(), node("list-item", "next", [item("1", 1)]), node("list-item", "back", [item()])]);
  await reject([node(), node("paragraph", "changed", [item("1", 0, { task: true })])]);
});

test("closed lists, cross-parent IDs and cyclic ancestry cannot be resurrected", async () => {
  await reject([node(), node("paragraph", "outside", []), node("list-item", "reused", [item("1", 1)])]);
  await reject([node(), node("list-item", "cycle", [item(), item()])]);
  await reject([node(), node("list-item", "nested", [item(), item("2")]),
    node("list-item", "next parent", [item("1", 1)]), node("list-item", "foreign", [item("1", 1), item("2")])]);
  await reject([node(), node("list-item", "second list", [item("2")]), node("list-item", "first again", [item()])]);
});

test("cells and nested reading descendants cannot independently claim root-list membership", async () => {
  await reject([node(), node("table-row", "cell", [item()], { children: [node("table-cell", "cell")] })]);
  await reject([node("blockquote", "", [], { children: [node()] })]);
});

test("ancestry budget counts repeated paths across all pages before retention", async () => {
  const roots = Array.from({ length: 257 }, (_, i) => node(i === 0 ? "list-item" : "paragraph"));
  await reject(roots, "READING_LIMIT", { limits: { maxListEntries: 256 } });
  assert.equal((await read(roots, { limits: { maxListEntries: 257 } })).listEntryCount, 257);
  await reject([node()], "INVALID_OPTIONS", { limits: { maxListEntries: 0 } });
  await reject([node()], "INVALID_OPTIONS", { limits: { maxListEntries: 50001 } });
  await reject([node(), node("list-item", "child", [item(), item("2")])], "READING_LIMIT", { limits: { maxDepth: 1 } });
});

test("large identities and ordinals remain exact decimal strings", async () => {
  const large = item("9007199254740993", 0, { ordered: true, start: "18446744073709551615" });
  const doc = await read([node("list-item", "large", [large])]);
  assert.equal(doc.nodes[0].listPath[0].start, large.start);
  assert.equal(doc.nodes[0].listPath[0].listId, large.listId);
  await reject([node("list-item", "large", [large]), node("list-item", "overflow", [{ ...large, itemIndex: 1 }])]);
});

test("list metadata never weakens source or layout revision fencing", async () => {
  const s = session([node()]), doc = await readFlowDocument(s), match = doc.find("item").matches[0];
  s.token = { revision: "1", layoutRevision: "2" };
  assert.throws(() => doc.locate(0), { code: "STALE_LAYOUT" });
  assert.throws(() => doc.matchText(match), { code: "STALE_LAYOUT" });
  s.token = { revision: "2", layoutRevision: "2" };
  assert.throws(() => doc.find("item"), { code: "STALE_REVISION" });
});

test("malformed ownership on a late page rejects the whole snapshot", async () => {
  const roots = Array.from({ length: 257 }, (_, i) => node(i === 0 ? "list-item" : "paragraph"));
  roots[256].listPath[0] = item("2");
  await reject(roots);
});
