import test from "node:test";
import assert from "node:assert/strict";
import { FlowReadingDocument, readFlowDocument, sourceSpanToUtf16 } from "./flow_reading.mjs";
import { node, ReadingSession, deferred } from "./tests/flow_reading_fixtures.mjs";
const code = value => error => error.code === value;

test("collects multiple real-sized pages under one token for synchronous and async sessions", async () => {
  for (const asyncMode of [false, true]) {
    const session = new ReadingSession(Array.from({ length: 520 }, (_, i) => node("paragraph", `node ${i}`)));
    if (asyncMode) { const read = session.readingOrder.bind(session); session.readingOrder = async options => read(options); }
    const doc = await readFlowDocument(session);
    assert.equal(doc.nodes.length, 520); assert.equal(doc.roots.length, 520);
    assert.deepEqual(session.calls.map(c => c.offset), [0, 256, 512]);
    assert(session.calls.every(c => c.token.revision === "1" && c.signal === undefined));
    assert.equal(doc.nodes[519].index, 519); assert(Object.isFrozen(doc.nodes[0].bounds));
    session.roots[0].text = "changed"; assert.equal(doc.nodes[0].text, "node 0");
    assert.equal(doc.locate(519).nodeIndex, 519);
  }
});

test("semantic leaves are searched once instead of duplicated container transcripts", async () => {
  const session = new ReadingSession([node("heading", "Guide", { level: 2 }), node("table-header-row", "A | B", {
    children: [node("table-header-cell", "A"), node("table-header-cell", "B")]
  }), node("list", "Entry", { children: [node("list-item", "Entry")] })]);
  const doc = await readFlowDocument(session);
  assert.equal(doc.headings[0].level, 2); assert.equal(doc.text, "Guide\n\nA\n\nB\n\nEntry");
  assert.equal(doc.find("A").matches.length, 1); assert.equal(doc.find("Entry").matches.length, 1);
  assert.deepEqual(doc.nodes.map(n => n.index), [0, 1, 2, 3, 4, 5]);
});

test("literal search crosses visual wraps within logical text, preserves code and UTF-16 offsets", async () => {
  const doc = await readFlowDocument(new ReadingSession([node("paragraph", "A😀BC café long sentence"), node("code-block", "  a\n    b") ]));
  const match = doc.find("😀B").matches[0];
  assert.equal(match.startUtf16, 1); assert.equal(match.endUtf16, 4); assert.equal(doc.matchText(match), "😀B");
  assert.equal(doc.find("long sentence").matches.length, 1);
  assert.equal(doc.find("  a\n    b").matches.length, 1);
  assert.equal(doc.find("bc", { asciiCaseInsensitive: true }).matches.length, 1);
  assert.equal(doc.find("CAFÉ", { asciiCaseInsensitive: true }).matches.length, 0); // Not Unicode case folding.
  assert.throws(() => doc.find("\ud83d"), code("INVALID_ARGUMENT"));
  assert.throws(() => doc.find("x", { maxMatches: 1001 }), code("INVALID_ARGUMENT"));
  assert.equal(doc.find("[a-z]+").matches.length, 0); // Not regex.
  assert.equal(doc.find("").matches.length, 0);
});

test("bounded search truthfully reports additional matches and rejects forged or cross-session hits", async () => {
  const doc = await readFlowDocument(new ReadingSession([node("paragraph", "a a a")]));
  const result = doc.find("a", { maxMatches: 2 }); assert.equal(result.matches.length, 2); assert(result.truncated);
  assert.equal(doc.find("a", { maxMatches: 3 }).truncated, false);
  assert.throws(() => doc.matchText({ ...result.matches[0] }), code("INVALID_ARGUMENT"));
  const other = await readFlowDocument(new ReadingSession([node("paragraph", "a a a")]));
  assert.throws(() => other.matchText(result.matches[0]), code("INVALID_ARGUMENT"));
  assert.throws(() => new FlowReadingDocument(null), code("INVALID_ARGUMENT"));
});

test("navigation/search/selection refuse source edits, reflows and disposed sessions", async () => {
  for (const action of ["edit", "reflow", "dispose"]) {
    const s = new ReadingSession(), doc = await readFlowDocument(s), match = doc.find("text").matches[0];
    if (action === "dispose") s.disposed = true; else s[action]();
    const expected = action === "edit" ? "STALE_REVISION" : action === "reflow" ? "STALE_LAYOUT" : "SESSION_DISPOSED";
    for (const call of [() => doc.assertCurrent(), () => doc.locate(0), () => doc.find("text"), () => doc.matchText(match)]) assert.throws(call, code(expected));
  }
});

test("reading cannot mix layouts or generations across asynchronous pages", async () => {
  const s = new ReadingSession(Array.from({ length: 300 }, () => node())), read = s.readingOrder.bind(s);
  s.readingOrder = async options => { const page = read(options); if (options.offset) s.reflow(); return page; };
  await assert.rejects(readFlowDocument(s), code("STALE_LAYOUT"));
});

test("malformed page lengths, offsets, schema and revisions never publish a partial tree", async () => {
  for (const mutate of [p => { p.offset = 1; }, p => { p.total++; }, p => { p.nextOffset = 0; },
    p => { p.schemaVersion = 2; }, p => { p.revision = 1; }, p => { p.layoutRevision = "2"; }]) {
    const s = new ReadingSession(), read = s.readingOrder.bind(s);
    s.readingOrder = opts => { const p = read(opts); mutate(p); return p; };
    await assert.rejects(readFlowDocument(s), code("INVALID_READING_DATA"));
  }
});

test("descendant/text/depth budgets apply to the complete tree, not just page roots", async () => {
  const tree = node("list", "summary", { children: [node("list-item", "one"), node("list-item", "two")] });
  await assert.rejects(readFlowDocument(new ReadingSession([tree]), { limits: { maxNodes: 2 } }), code("READING_LIMIT"));
  await assert.rejects(readFlowDocument(new ReadingSession([tree]), { limits: { maxTextUnits: 3 } }), code("READING_LIMIT"));
  let deep = node(); for (let i = 0; i < 5; i++) deep = node("blockquote", "", { children: [deep] });
  await assert.rejects(readFlowDocument(new ReadingSession([deep]), { limits: { maxDepth: 2 } }), code("READING_LIMIT"));
  for (const limits of [{ maxNodes: 0 }, { maxDepth: 65 }, { surprise: 1 }]) {
    await assert.rejects(readFlowDocument(new ReadingSession(), { limits }), code("INVALID_OPTIONS"));
  }
});

test("cycles, aliases, unknown roles, impossible structure and geometry are rejected", async () => {
  const cycle = node("document"); cycle.children.push(cycle);
  const shared = node();
  for (const roots of [[cycle], [shared, shared], [node("script")], [node("heading", "x", { level: 7 })],
    [node("table-cell")], [node("table", "x", { children: [node()] })],
    [node("paragraph", "x", { children: [node()] })], [node("list", "x", { children: [node()] })],
    [node("paragraph", "\udc00")], [node("paragraph", "x", { bounds: { x: 0, y: NaN, width: 3, height: 4 } })],
    [node("paragraph", "x", { enclosingSourceSpan: { startByte: 4, endByte: 2 } })]]) {
    await assert.rejects(readFlowDocument(new ReadingSession(roots)), code("INVALID_READING_DATA"));
  }
});

test("abort stops waiting without forwarding cancellation or ignoring late rejections", async () => {
  const s = new ReadingSession(), gate = deferred(), controller = new AbortController();
  s.readingOrder = opts => { assert.equal(opts.signal, undefined); return gate.promise; };
  const pending = readFlowDocument(s, { signal: controller.signal }); controller.abort();
  await assert.rejects(pending, code("ABORTED")); assert(!s.disposed);
  gate.reject(new Error("late observed rejection")); await new Promise(resolve => setImmediate(resolve));
  await assert.rejects(readFlowDocument(s, { signal: AbortSignal.abort() }), code("ABORTED"));
});

test("empty documents and >Number.MAX_SAFE_INTEGER revisions remain valid", async () => {
  const s = new ReadingSession([]); s.revision = "9007199254740993";
  const doc = await readFlowDocument(s); assert.equal(doc.token.revision, s.revision);
  assert.equal(doc.text, ""); assert.deepEqual(doc.headings, []);
  assert.throws(() => doc.locate(0), code("INVALID_ARGUMENT"));
});

test("enclosing Markdown byte spans convert losslessly for textarea navigation", () => {
  const source = "aé😀z";
  assert.deepEqual(sourceSpanToUtf16(source, { startByte: 1, endByte: 7 }), { start: 1, end: 4 });
  assert.deepEqual(sourceSpanToUtf16(source, { startByte: 8, endByte: 8 }), { start: 5, end: 5 });
  for (const range of [{ startByte: 2, endByte: 7 }, { startByte: 1, endByte: 6 }, { startByte: 0, endByte: 9 }]) {
    assert.throws(() => sourceSpanToUtf16(source, range), code("INVALID_ARGUMENT"));
  }
  assert.throws(() => sourceSpanToUtf16("\ud800", { startByte: 0, endByte: 0 }), code("INVALID_ARGUMENT"));
});
