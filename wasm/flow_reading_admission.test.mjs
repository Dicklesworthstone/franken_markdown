// Production semantic admission with explicit native-page fixtures. Timers are
// real event-loop tasks; no Markdown parser, native engine or timing benchmark.
import assert from "node:assert/strict";
import test from "node:test";
import { readFlowDocument } from "./flow_reading.mjs";
import { node, ReadingSession, deferred } from "./tests/flow_reading_fixtures.mjs";
const code = value => error => error.code === value;
const style = extra => ({ bold: false, italic: false, code: false, strikethrough: false, ...extra });
async function aborted(roots, inspect = () => {}) {
  const session = new ReadingSession(roots), control = new AbortController();
  const pending = readFlowDocument(session, { signal: control.signal });
  const refusal = assert.rejects(pending, code("ABORTED"));
  const timer = setTimeout(() => { inspect(session); control.abort(); }, 0);
  try { await refusal; assert(!session.disposed); }
  finally { clearTimeout(timer); }
  return session;
}

test("small admission preserves the no-timer path and immutable public data", async () => {
  const original = globalThis.setTimeout; let timers = 0;
  globalThis.setTimeout = (...args) => { timers++; return original(...args); };
  try {
    const s = new ReadingSession([node("heading", "Title", { level: 2, anchorId: "title" }), node("paragraph", "body")]);
    const doc = await readFlowDocument(s);
    assert.equal(timers, 0); assert.equal(doc.text, "Title\n\nbody"); assert.equal(doc.locateFragment("#title").nodeIndex, 0);
    assert.equal(doc.headings[0], doc.nodes[0]); assert(Object.isFrozen(doc.nodes[1]));
    s.roots[0].text = "mutated"; assert.equal(doc.headings[0].text, "Title");
  } finally { globalThis.setTimeout = original; }
});

test("a single near-limit plain-text leaf yields before its full scalar scan", async () => {
  const text = "x".repeat(1000000), original = String.prototype.charCodeAt; let checked = 0;
  String.prototype.charCodeAt = function(...args) { if (this.length === text.length) checked++; return original.apply(this, args); };
  try {
    await aborted([node("paragraph", text)]);
    assert(checked > 0 && checked <= 20000, `validated ${checked} units before cancellation`);
  } finally { String.prototype.charCodeAt = original; }
});

test("one styled leaf cooperates inside its inline metadata, not just its text", async () => {
  const control = new AbortController(); let visited = 0, timer;
  const runs = Array.from({ length: 50000 }, (_, i) => ({
    get startByte() { if (++visited === 1) timer = setTimeout(() => control.abort(), 0); return i; },
    endByte: i + 1, style: style(), link: null,
  }));
  try {
    await assert.rejects(readFlowDocument(new ReadingSession([node("paragraph", "x".repeat(50000), { inlineRuns: runs })]),
      { signal: control.signal }), code("ABORTED"));
    assert(visited > 0 && visited < 1024, `processed ${visited} runs before cancellation`);
  } finally { clearTimeout(timer); }
});

test("a single table subtree yields before traversing all descendants", async () => {
  let children = 0;
  const rows = Array.from({ length: 4999 }, () => {
    const row = node("table-row", "", { children: [node("table-cell", "cell")] });
    Object.defineProperty(row, "role", { get() { children++; return "table-row"; } }); return row;
  });
  await aborted([node("table", "", { children: rows })]);
  assert(children > 0 && children < 4999);
});

test("synchronous root pages do not chain through the whole inventory before input", async () => {
  const s = await aborted(Array.from({ length: 10000 }, () => node("paragraph", "")));
  assert(s.calls.length > 0 && s.calls.length < 40);
  assert(s.calls.every(call => !Object.hasOwn(call, "signal")));
});

test("source edits, reflows and disposal fence resumed admission before publication", async () => {
  for (const action of ["edit", "reflow", "dispose"]) {
    const s = new ReadingSession([node("paragraph", "x".repeat(900000))]);
    const pending = readFlowDocument(s);
    const refused = assert.rejects(pending, code(action === "edit" ? "STALE_REVISION" : action === "reflow" ? "STALE_LAYOUT" : "SESSION_DISPOSED"));
    const timer = setTimeout(() => { if (action === "dispose") s.disposed = true; else s[action](); }, 0);
    try { await refused; } finally { clearTimeout(timer); }
  }
});

test("long links and anchor IDs are cooperative even with tiny reading text", async () => {
  const target = "/" + "x".repeat(400000);
  await aborted([node("paragraph", "x", { inlineRuns: [{ startByte: 0, endByte: 1, style: style(),
    link: { target, activeTarget: target } }] })]);
  await aborted([node("heading", "short", { level: 1, anchorId: "a".repeat(1000000) })]);
});

test("link trimming cooperates and keeps the same conservative activation rule", async () => {
  const target = " ".repeat(800000) + "#ok";
  const doc = await readFlowDocument(new ReadingSession([node("paragraph", "link", { inlineRuns: [
    { startByte: 0, endByte: 4, style: style(), link: { target, activeTarget: "#ok" } },
  ] })]));
  assert.equal(doc.nodes[0].inlineRuns[0].link.target, target);
  assert.equal(doc.nodes[0].inlineRuns[0].link.activeTarget, "#ok");
  const unsafe = "javascript:" + "x".repeat(50000);
  await assert.rejects(readFlowDocument(new ReadingSession([node("paragraph", "x", { inlineRuns: [
    { startByte: 0, endByte: 1, style: style(), link: { target: unsafe, activeTarget: unsafe } },
  ] })])), code("INVALID_READING_DATA"));
});

test("surrogates on chunk boundaries preserve exact UTF-8 and UTF-16 inline coordinates", async () => {
  const text = "a".repeat(4095) + "😀é" + "b".repeat(30000);
  const length = new TextEncoder().encode(text).length;
  const doc = await readFlowDocument(new ReadingSession([node("paragraph", text, { inlineRuns: [
    { startByte: 0, endByte: length, style: style({ italic: true }), link: null },
  ] })]));
  const run = doc.nodes[0].inlineRuns[0]; assert.equal(run.endUtf16, text.length);
  const match = doc.find("😀é").matches[0]; assert.equal(match.startUtf16, 4095); assert.equal(match.endUtf16, 4098);
  assert.equal(doc.matchText(match), "😀é");
  for (const bad of ["a".repeat(16383) + "\ud800x", "a".repeat(16384) + "\udc00", "a".repeat(20000) + "\ud800"])
    await assert.rejects(readFlowDocument(new ReadingSession([node("paragraph", bad)])), code("INVALID_READING_DATA"));
});

test("yielding cannot substitute unvalidated node primitives after they were captured", async () => {
  const raw = node("paragraph", "a".repeat(50000)), s = new ReadingSession([raw]); let ran = false;
  const timer = setTimeout(() => {
    ran = true; raw.role = "script"; raw.text = "\ud800"; raw.bounds.width = NaN; raw.enclosingSourceSpan.startByte = -1;
  }, 0);
  try {
    const doc = await readFlowDocument(s); assert(ran);
    assert.equal(doc.nodes[0].role, "paragraph"); assert.equal(doc.nodes[0].text.length, 50000);
    assert.equal(doc.nodes[0].bounds.width, 300); assert.equal(doc.nodes[0].enclosingSourceSpan.startByte, 0);
  } finally { clearTimeout(timer); }
});

test("style and range values copied before a large offset scan cannot change during a yield", async () => {
  const originalStyle = style({ bold: true }); let timer, touched = false;
  const run = { startByte: 0, endByte: 80000, style: originalStyle,
    get link() { timer = setTimeout(() => { touched = true; originalStyle.bold = "invalid"; run.endByte = -1; }, 0); return null; } };
  try {
    const doc = await readFlowDocument(new ReadingSession([node("paragraph", "x".repeat(80000), { inlineRuns: [run] })]));
    assert(touched); assert.equal(doc.nodes[0].inlineRuns[0].style.bold, true);
    assert.equal(doc.nodes[0].inlineRuns[0].endByte, 80000); assert.equal(doc.nodes[0].inlineRuns[0].endUtf16, 80000);
  } finally { clearTimeout(timer); }
});

test("page and child membership are captured before suspension, but later values still get validated", async () => {
  const children = [node("paragraph", "child")], root = node("blockquote", "x".repeat(50000), { children });
  const s = new ReadingSession([root]), original = s.readingOrder.bind(s); let page;
  s.readingOrder = opts => (page = original(opts));
  const timer = setTimeout(() => { children.push(node("script", "bad")); page.nodes.push(node("script", "bad")); }, 0);
  try {
    const result = await readFlowDocument(s); assert.equal(result.nodes.length, 2); assert.equal(result.nodes[1].text, "child");
  } finally { clearTimeout(timer); }
  const later = node("paragraph", "child");
  const invalidating = setTimeout(() => { later.role = "script"; }, 0);
  try {
    await assert.rejects(readFlowDocument(new ReadingSession([node("blockquote", "x".repeat(50000), { children: [later] })])),
      code("INVALID_READING_DATA"));
  } finally { clearTimeout(invalidating); }
});

test("bounds, aliases, cycles and impossible semantic structure are still refused", async () => {
  const cycle = node("blockquote", ""); cycle.children.push(cycle); const alias = node();
  for (const roots of [[cycle], [alias, alias], [node("paragraph", "", { children: [node()] })],
    [node("table", "", { children: [node()] })], [node("table-cell")], [node("heading", "bad", { level: 7 })],
    [node("paragraph", "", { bounds: { x: 0, y: 0, width: -1, height: 1 } })]]) {
    await assert.rejects(readFlowDocument(new ReadingSession(roots)), code("INVALID_READING_DATA"));
  }
});

test("text, metadata, depth and descendant limits remain admission failures, not truncation", async () => {
  for (const [roots, limits] of [
    [[node("paragraph", "long")], { maxTextUnits: 3 }],
    [[node("table-row", "", { children: [node("table-cell"), node("table-cell")] })], { maxNodes: 2 }],
    [[node("heading", "h", { level: 1, anchorId: "long" })], { maxAnchorUnits: 3 }],
    [[node("paragraph", "x", { inlineRuns: Array.from({ length: 2 }, () => ({ startByte: 0, endByte: 1, style: style(), link: null })) })], { maxInlineRuns: 1 }],
  ]) await assert.rejects(readFlowDocument(new ReadingSession(roots), { limits }), code("READING_LIMIT"));
  let deep = node(); for (let i = 0; i < 65; i++) deep = node("blockquote", "", { children: [deep] });
  await assert.rejects(readFlowDocument(new ReadingSession([deep])), code("READING_LIMIT"));
});

test("list ownership is validated across pages after cooperative collection", async () => {
  const path = i => [{ listId: "9007199254740993", ordered: true, start: "9007199254740993", itemIndex: i, task: null }];
  const roots = Array.from({ length: 600 }, (_, i) => node("list-item", `Item ${i}`, { listPath: path(i) }));
  const s = new ReadingSession(roots), doc = await readFlowDocument(s);
  assert.equal(doc.listEntryCount, 600); assert.equal(doc.nodes[599].listPath[0].itemIndex, 599);
  assert.deepEqual(s.calls.map(call => call.offset), [0, 256, 512]);
  roots[512].listPath[0].itemIndex = 900;
  await assert.rejects(readFlowDocument(new ReadingSession(roots)), code("INVALID_READING_DATA"));
});

test("cooperative collection releases every abort listener on success and cancellation", async () => {
  for (const abort of [false, true]) {
    const controller = new AbortController(), signal = controller.signal; let balance = 0;
    const add = signal.addEventListener.bind(signal), remove = signal.removeEventListener.bind(signal);
    signal.addEventListener = (...args) => { balance++; return add(...args); };
    signal.removeEventListener = (...args) => { balance--; return remove(...args); };
    const pending = readFlowDocument(new ReadingSession([node("paragraph", "x".repeat(20000))]), { signal });
    let timer;
    try {
      if (abort) { const refused = assert.rejects(pending, code("ABORTED")); timer = setTimeout(() => controller.abort(), 0); await refused; }
      else await pending;
      assert.equal(balance, 0);
    } finally { clearTimeout(timer); }
  }
});

test("pending native responses are abandoned safely without sending worker cancellation", async () => {
  const gate = deferred(), controller = new AbortController(), s = new ReadingSession();
  s.readingOrder = options => { assert(!Object.hasOwn(options, "signal")); return gate.promise; };
  const pending = readFlowDocument(s, { signal: controller.signal }); controller.abort();
  await assert.rejects(pending, code("ABORTED")); gate.reject(new Error("late observed failure"));
  await new Promise(resolve => setImmediate(resolve)); assert(!s.disposed);
});
