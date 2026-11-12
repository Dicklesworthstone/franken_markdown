// Exercise the production Canvas ingress with synthetic native wire fixtures.
// Rust projection and generated-WASM output have their own integration gates.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";
import { box, text, vector, Session, Surface, sparsePage } from "./tests/flow_canvas_viewport_fixtures.mjs";

const dimensions = { width: 100, height: 60 };
const code = wanted => error => error.code === wanted;
function painter(t, options = {}) {
  const canvas = new Surface(), stages = [];
  const renderer = new FlowCanvasRenderer(canvas, { canvasFactory(w, h) {
    const stage = new Surface(w, h); stages.push(stage); return stage;
  }, ...options });
  t.after(() => renderer.dispose()); return { canvas, stages, renderer };
}
function sourceHints(source = "\u00ad\u200b", y = 0) {
  let bytes = 0;
  for (const value of source) bytes += value === "\u00ad" ? 2 : 3;
  const item = text(0, y);
  item.text = source; item.bounds = box(0, y, 0, 20);
  item.fontRun = { ...item.fontRun, totalAdvance: 0, glyphCount: 0, clusterCount: 1,
    glyphs: [], clusters: [{ index: 0, bytes: [0, bytes], utf16: [0, source.length],
      glyphs: [0, 0], xStart: 0, xEnd: 0, fontId: "1" }] };
  return item;
}
const translations = stage => stage.log.filter(row => row[0] === "translate");

for (const indexed of [false, true]) test(
  "source-only authored hints retain their frame without painted glyphs through " +
    (indexed ? "indexed viewports" : "snapshots"), async t => {
    for (const source of ["\u00ad", "\u200b", "\u00ad\u200b".repeat(130)]) {
      const f = painter(t), session = new Session([sourceHints(source)], indexed);
      const frame = await f.renderer.render(session, dimensions);
      assert.equal(frame.visibleItems, 1); assert.equal(frame.glyphs, 0);
      assert.deepEqual(translations(f.stages[0]), []);
      assert.deepEqual(session.outlines, [["1", []]], "request truthful font metrics without inventing a glyph");
      assert.equal(session.items[0].text, source);
      assert.deepEqual(session.items[0].fontRun.clusters[0].utf16, [0, source.length]);
    }
  },
);

test("source-only and real glyph fragments preserve snapshot/viewport painting parity", async t => {
  const items = [text(0, 0), sourceHints("\u00ad\u200b", 0), text(20, 0)];
  const a = painter(t), b = painter(t);
  const first = await a.renderer.render(new Session(items, false), dimensions);
  const second = await b.renderer.render(new Session(items, true), dimensions);
  assert.equal(first.glyphs, 2); assert.equal(second.glyphs, 2);
  assert.equal(first.visibleItems, 3); assert.equal(second.visibleItems, 3);
  assert.deepEqual(a.stages[0].log, b.stages[0].log);
  assert.equal(translations(a.stages[0]).length, 2);
});

const malformed = [
  ["visible source", item => { item.text = "ab"; }],
  ["word joiner", item => { item.text = "\u2060\u00ad"; }],
  ["newline", item => { item.text = "\n\u00ad"; }],
  ["empty source", item => { item.text = ""; }],
  ["non-string source", item => { item.text = 1; }],
  ["surrogate source", item => { item.text = "\ud800\u00ad"; }],
  ["missing byte range", item => { delete item.fontRun.clusters[0].bytes; }],
  ["wrong byte start", item => { item.fontRun.clusters[0].bytes[0] = 1; }],
  ["wrong byte end", item => { item.fontRun.clusters[0].bytes[1] = 4; }],
  ["fractional byte end", item => { item.fontRun.clusters[0].bytes[1] = 5.5; }],
  ["string byte end", item => { item.fontRun.clusters[0].bytes[1] = "5"; }],
  ["missing UTF-16 range", item => { delete item.fontRun.clusters[0].utf16; }],
  ["wrong UTF-16 start", item => { item.fontRun.clusters[0].utf16[0] = 1; }],
  ["wrong UTF-16 end", item => { item.fontRun.clusters[0].utf16[1] = 3; }],
  ["extra source range element", item => { item.fontRun.clusters[0].bytes.push(5); }],
  ["nonempty glyph range", item => { item.fontRun.clusters[0].glyphs[1] = 1; }],
  ["nonzero glyph start", item => { item.fontRun.clusters[0].glyphs[0] = 1; }],
  ["wrong cluster identity", item => { item.fontRun.clusters[0].index = 1; }],
  ["wrong font identity", item => { item.fontRun.clusters[0].fontId = "2"; }],
  ["nonzero cluster start", item => { item.fontRun.clusters[0].xStart = 1; }],
  ["nonzero cluster end", item => { item.fontRun.clusters[0].xEnd = 1; }],
  ["nonfinite cluster geometry", item => { item.fontRun.clusters[0].xEnd = NaN; }],
  ["nonzero advance", item => { item.fontRun.totalAdvance = 1; }],
  ["missing advance", item => { delete item.fontRun.totalAdvance; }],
  ["nonzero bounds width", item => { item.bounds.width = 1; }],
  ["extra cluster", item => { item.fontRun.clusters.push({ ...item.fontRun.clusters[0] }); item.fontRun.clusterCount = 2; }],
];
for (const [name, mutate] of malformed) test(
  "malformed " + name + " cannot replace the last complete Canvas frame", async t => {
    const f = painter(t);
    await f.renderer.render(new Session([vector()]), dimensions);
    const previous = f.renderer.frame, blits = f.canvas.log.length;
    const item = sourceHints(); mutate(item);
    const session = new Session([item], false);
    await assert.rejects(f.renderer.render(session, dimensions), code("INVALID_WASM_RESPONSE"));
    assert.equal(f.renderer.frame, previous); assert.equal(f.canvas.log.length, blits);
    assert.equal(f.stages.length, 1); assert.equal(session.outlines.length, 0);
  },
);

test("ordinary glyph-bearing clusters still reject empty ownership ranges", async t => {
  const f = painter(t), item = text();
  item.fontRun.clusters[0].glyphs = [0, 0];
  await assert.rejects(f.renderer.render(new Session([item], false), dimensions),
    code("INVALID_WASM_RESPONSE"));
  assert.equal(f.stages.length, 0);
});

test("a truthful zero-advance glyph still follows normal host glyph admission", async t => {
  const f = painter(t), item = text();
  item.bounds.width = 0;
  item.fontRun.totalAdvance = 0; item.fontRun.clusters[0].xEnd = 0;
  item.fontRun.glyphs[0].xAdvance = 0;
  const frame = await f.renderer.render(new Session([item]), dimensions);
  assert.equal(frame.glyphs, 1); assert.equal(translations(f.stages[0]).length, 1);
});

test("hint validation has its own exact frame ceiling without consuming the glyph budget", async t => {
  const f = painter(t, { limits: { maxGlyphs: 0 } });
  const frame = await f.renderer.render(new Session([sourceHints("\u00ad".repeat(65536))]), dimensions);
  assert.equal(frame.glyphs, 0);
  const previous = f.renderer.frame, blits = f.canvas.log.length;
  for (const items of [
    [sourceHints("\u00ad".repeat(65537))],
    [sourceHints("\u00ad".repeat(32768)), sourceHints("\u200b".repeat(32769), 20)],
  ]) {
    const session = new Session(items, false);
    await assert.rejects(f.renderer.render(session, dimensions), code("BUDGET_EXCEEDED"));
    assert.equal(session.outlines.length, 0);
    assert.equal(f.renderer.frame, previous); assert.equal(f.canvas.log.length, blits);
  }
});

test("a long hint-only source scan cooperates with cancellation before any glyph requests", async t => {
  const f = painter(t), controller = new AbortController();
  await f.renderer.render(new Session([vector()]), dimensions);
  const previous = f.renderer.frame, blits = f.canvas.log.length;
  const session = new Session([sourceHints("\u00ad\u200b".repeat(32768))], false);
  const pending = f.renderer.render(session, { ...dimensions, signal: controller.signal });
  const refused = assert.rejects(pending, code("ABORTED"));
  const timer = setTimeout(() => controller.abort(), 0);
  try { await refused; } finally { clearTimeout(timer); }
  assert.equal(session.outlines.length, 0); assert.equal(f.stages.length, 1);
  assert.equal(f.renderer.frame, previous); assert.equal(f.canvas.log.length, blits);
});

test("borrowed source-only metadata is captured before cooperative scan yields", async t => {
  const f = painter(t), session = new Session();
  const item = sourceHints("\u00ad\u200b".repeat(4096));
  let borrowed;
  session.viewport = query => {
    borrowed = { ...item, index: 42, effectiveClip: query.viewport };
    return sparsePage(session, query, [], { items: [borrowed] });
  };
  const pending = f.renderer.render(session, dimensions);
  const timer = setTimeout(() => {
    borrowed.text = "visible replacement";
    borrowed.fontRun.totalAdvance = 100;
    borrowed.fontRun.clusters[0].bytes[1] = 0;
    borrowed.fontRun.clusters[0].xEnd = 100;
  }, 0);
  let frame;
  try { frame = await pending; } finally { clearTimeout(timer); }
  assert.equal(borrowed.text, "visible replacement");
  assert.equal(frame.glyphs, 0); assert.equal(translations(f.stages[0]).length, 0);
});
