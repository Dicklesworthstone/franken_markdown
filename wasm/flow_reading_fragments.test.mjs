// Tests execute the real reading admission/navigation code with explicit wire
// doubles. They do not claim execution of Rust/WASM or a browser DOM.
import test from "node:test";
import assert from "node:assert/strict";
import { readFlowDocument } from "./flow_reading.mjs";
import { node, box, ReadingSession } from "./tests/flow_reading_fixtures.mjs";
const code = value => error => error.code === value;
const heading = (id, text = "Same heading", extra = {}) => node("heading", text, { level: 2, anchorId: id, ...extra });

test("local navigation uses engine IDs, not reconstructed heading slugs or source spans", async () => {
  const roots = [heading("repeat"), heading("repeat-2"), heading("repeat-3"),
    heading("fmd:note:1", "[1]", { level: 6, bounds: box(500), enclosingSourceSpan: { startByte: 100, endByte: 120 } })];
  const document = await readFlowDocument(new ReadingSession(roots));
  for (let index = 0; index < roots.length; index++) {
    assert.deepEqual(document.locateFragment(`#${roots[index].anchorId}`), document.locate(index));
  }
  assert.equal(document.locateFragment("#same-heading"), null);
  assert.equal(document.locateFragment("#Repeat"), null);
  assert.equal(document.locateFragment("#fmd:note:1").bounds.y, 500);
  assert.equal(document.locateFragment("#fmd:note:1").enclosingSourceSpan.startByte, 100);
});

test("Unicode fragments decode once, preserve plus and case, and reject malformed escapes", async () => {
  const ids = ["é中🙂", "a+b", "%41", "A", "fmd:note:2"];
  const document = await readFlowDocument(new ReadingSession(ids.map(id => heading(id))));
  for (let index = 0; index < ids.length; index++) {
    assert.equal(document.locateFragment(`#${encodeURIComponent(ids[index])}`).nodeIndex, index);
  }
  assert.equal(document.locateFragment("#%2541").nodeIndex, 2);
  assert.equal(document.locateFragment("#%41").nodeIndex, 3);
  assert.equal(document.locateFragment("#a+b").nodeIndex, 1);
  for (const value of ["#a%20b", "#%", "#%0", "#%GG", "#%FF", "#%ED%A0%80", "#%00"]) {
    assert.equal(document.locateFragment(value), null);
  }
});

test("external, relative, query-bearing and empty URLs never become local navigation", async () => {
  const document = await readFlowDocument(new ReadingSession([heading("target")]));
  for (const target of ["", "#", "target", "page.md#target", "?q=1#target", "//host/#target",
    "https://example.com/#target", "javascript:alert(1)#target", " #target", "%23target"]) {
    assert.equal(document.locateFragment(target), null);
  }
  for (const target of [null, {}, 7, "#\ud800", "#" + "x".repeat(4096)]) {
    assert.throws(() => document.locateFragment(target), code("INVALID_ARGUMENT"));
  }
});

test("legacy snapshots remain readable without guessing missing destinations", async () => {
  const document = await readFlowDocument(new ReadingSession([
    node("heading", "Known title", { level: 1 }), heading(null), node("paragraph", "body")
  ]));
  assert.equal(document.headings.length, 2);
  assert.equal(document.nodes[0].anchorId, null);
  assert.equal(document.nodes[1].anchorId, null);
  assert.equal(document.nodes[2].anchorId, null);
  assert.equal(document.anchorUnits, 0);
  assert.equal(document.locateFragment("#known-title"), null);
  assert.equal(document.find("body").matches.length, 1);
});

test("duplicate IDs are rejected even when they cross reading-page boundaries", async () => {
  const roots = Array.from({ length: 257 }, (_, index) => heading(`section-${index}`));
  roots[256].anchorId = roots[0].anchorId;
  const session = new ReadingSession(roots);
  await assert.rejects(readFlowDocument(session), code("INVALID_READING_DATA"));
  assert.deepEqual(session.calls.map(call => call.offset), [0, 256]);
});

test("destination metadata is bounded and validated before publication", async () => {
  for (const value of ["", 3, {}, "bad\n", "bad\u0085", "\udc00"]) {
    await assert.rejects(readFlowDocument(new ReadingSession([heading(value)])), code("INVALID_READING_DATA"));
  }
  await assert.rejects(readFlowDocument(new ReadingSession([node("paragraph", "x", { anchorId: "wrong-role" })])), code("INVALID_READING_DATA"));
  const session = new ReadingSession([heading("ab"), heading("c")]);
  const exact = await readFlowDocument(session, { limits: { maxAnchorUnits: 3 } });
  assert.equal(exact.anchorUnits, 3);
  await assert.rejects(readFlowDocument(session, { limits: { maxAnchorUnits: 2 } }), code("READING_LIMIT"));
  for (const limit of [0, -1, 1048577, 1.5, NaN]) {
    await assert.rejects(readFlowDocument(session, { limits: { maxAnchorUnits: limit } }), code("INVALID_OPTIONS"));
  }
});

test("navigation is fenced after source edits, reflow, and session disposal", async () => {
  for (const action of ["edit", "reflow", "dispose"]) {
    const session = new ReadingSession([heading("target")]), document = await readFlowDocument(session);
    if (action === "dispose") session.disposed = true; else session[action]();
    const expected = action === "edit" ? "STALE_REVISION" : action === "reflow" ? "STALE_LAYOUT" : "SESSION_DISPOSED";
    for (const target of ["#target", "#absent", "https://example.com"]) {
      assert.throws(() => document.locateFragment(target), code(expected));
    }
  }
});

test("anchor index is immutable, session-local, and based on preorder node indices", async () => {
  const roots = [node("table-header-row", "A | B", { children: [node("table-header-cell", "A"), node("table-header-cell", "B")] }),
    heading("target", "First", { bounds: box(70) })];
  const session = new ReadingSession(roots), document = await readFlowDocument(session);
  assert.equal(document.locateFragment("#target").nodeIndex, 3);
  roots[1].anchorId = "mutated"; roots[1].bounds.y = 999; roots[1].text = "Changed";
  assert.equal(document.locateFragment("#target").bounds.y, 70);
  assert.equal(document.locateFragment("#mutated"), null);
  assert.throws(() => { document.nodes[3].anchorId = "forged"; }, TypeError);
  const other = await readFlowDocument(new ReadingSession([heading("target", "Other", { bounds: box(20) })]));
  assert.equal(other.locateFragment("#target").nodeIndex, 0);
  assert.equal(other.locateFragment("#target").bounds.y, 20);
  assert(Object.isFrozen(document.locateFragment("#target")));
});

test("sync and async paginated sessions resolve all engine destinations identically", async () => {
  for (const asyncMode of [false, true]) {
    const roots = Array.from({ length: 520 }, (_, i) => heading(`note-${i}`, `[${i}]`, { level: 6, bounds: box(i * 20) }));
    const session = new ReadingSession(roots);
    if (asyncMode) { const read = session.readingOrder.bind(session); session.readingOrder = async options => read(options); }
    const document = await readFlowDocument(session);
    assert.deepEqual(session.calls.map(call => call.offset), [0, 256, 512]);
    for (let i = 0; i < roots.length; i++) {
      assert.equal(document.locateFragment(`#note-${i}`).nodeIndex, i);
      assert.equal(document.locateFragment(`#note-${i}`).bounds.y, i * 20);
    }
  }
});

test("a revision change between anchor pages rejects the entire navigation index", async () => {
  const session = new ReadingSession(Array.from({ length: 257 }, (_, i) => heading(`h-${i}`)));
  const read = session.readingOrder.bind(session);
  session.readingOrder = async options => {
    const result = read(options);
    if (options.offset) session.edit();
    return result;
  };
  await assert.rejects(readFlowDocument(session), code("STALE_REVISION"));
});
