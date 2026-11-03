// Synthetic wire/Canvas doubles isolate the new baseline and fallback boundary.
// Native and generated-WASM behavior is checked by the inline math browser gate.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";
import { createFlowAdapter } from "./flow_session.mjs";
import { box, text, vector, Session, Surface, sparsePage } from "./tests/flow_canvas_viewport_fixtures.mjs";

const size = { width: 100, height: 60 };
const code = wanted => error => error.code === wanted;
function painter(t) {
  const canvas = new Surface(), stages = [], fixture = { canvas, stages, onStage() {} };
  fixture.renderer = new FlowCanvasRenderer(canvas, { canvasFactory(w, h) {
    const stage = new Surface(w, h); stages.push(stage); fixture.onStage(stage); return stage;
  } });
  t.after(() => fixture.renderer.dispose()); return fixture;
}
const placed = (baselineY, extra = {}) => ({ ...text(), baselineY, ...extra });
const translations = stage => stage.log.filter(row => row[0] === "translate");

for (const indexed of [false, true]) test(`explicit native baseline replaces font inference in ${indexed ? "viewport" : "snapshot"} paints`, async t => {
  const f = painter(t);
  await f.renderer.render(new Session([placed(17)], indexed), size);
  assert.deepEqual(translations(f.stages[0]), [["translate", 0, 17]]);
});

test("absent and null baseline preserve legacy placement while zero remains an explicit baseline", async t => {
  const f = painter(t);
  for (const item of [text(), placed(null), placed(0), placed(20)]) await f.renderer.render(new Session([item]), size);
  assert.deepEqual(f.stages.map(stage => translations(stage)[0][2]), [13, 13, 0, 20]);
});

test("tall mixed lines honor their native baseline across fonts, clipping and scrolling", async t => {
  const a = painter(t), b = painter(t);
  const first = { ...text(10, 40), bounds: box(10, 40, 10, 40), baselineY: 68 };
  const second = { ...text(110, 40, "2"), bounds: box(110, 40, 10, 40), baselineY: 68 };
  const items = [{ kind: "clip", bounds: box(5, 44, 80, 35), childCount: 2 }, first, second];
  const options = { ...size, scrollX: 10, scrollY: 40, pixelRatio: 2 };
  await a.renderer.render(new Session(items, false), options);
  await b.renderer.render(new Session(items, true), options);
  assert.deepEqual(a.stages[0].log, b.stages[0].log);
  assert.deepEqual(translations(a.stages[0]), [["translate", 10, 68], ["translate", 110, 68]]);
  assert(a.stages[0].log.some(row => JSON.stringify(row) === JSON.stringify(["setTransform", 2, 0, 0, 2, -20, -80])));
  assert(a.stages[0].log.some(row => JSON.stringify(row) === JSON.stringify(["rect", 10, 44, 75, 35])));
});

for (const [name, value] of [
  ["NaN", NaN], ["infinite", Infinity], ["numeric string", "17"], ["object", {}],
  ["false", false], ["before line", -0.01], ["after line", 20.01], ["unbounded", 1e13],
]) test(`malformed ${name} baseline cannot replace the last complete frame`, async t => {
  const f = painter(t); await f.renderer.render(new Session([vector()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length;
  const session = new Session([placed(value)]);
  await assert.rejects(f.renderer.render(session, size), code("INVALID_WASM_RESPONSE"));
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
  assert.equal(f.stages.length, 1); assert.equal(session.outlines.length, 0);
});

test("baseline tolerance is limited to native f32 line-edge reconstruction", async t => {
  const f = painter(t);
  await f.renderer.render(new Session([placed(20.00001)]), size);
  await assert.rejects(f.renderer.render(new Session([placed(20.001)]), size), code("INVALID_WASM_RESPONSE"));
  // A distant horizontal coordinate must not widen vertical tolerance.
  await assert.rejects(f.renderer.render(new Session([{ ...placed(20.001), bounds: box(1e10, 0, 10, 20) }]), size), code("INVALID_WASM_RESPONSE"));
});

test("baseline and fallback metadata are owned before long glyph planning yields", async t => {
  const f = painter(t), s = new Session(), item = { ...text(0, 0, "1", 8192), baselineY: 17, mathFallback: "unsupported command" };
  let borrowed;
  s.viewport = query => {
    borrowed = { ...item, index: 42, effectiveClip: query.viewport };
    return sparsePage(s, query, [], { items: [borrowed] });
  };
  const pending = f.renderer.render(s, size);
  const timer = setTimeout(() => { borrowed.baselineY = NaN; borrowed.mathFallback = null; }, 0);
  try { await pending; } finally { clearTimeout(timer); }
  assert(Number.isNaN(borrowed.baselineY)); assert.equal(borrowed.mathFallback, null);
  assert.equal(translations(f.stages[0]).length, 8192);
  assert(translations(f.stages[0]).every(row => row[2] === 17));
});

for (const [name, value] of [
  ["empty", ""], ["null", null], ["number", 1], ["object", {}],
  ["too many ASCII scalars", "x".repeat(161)], ["too many supplementary scalars", "😀".repeat(161)],
  ["unpaired high surrogate", "bad\ud800"], ["unpaired low surrogate", "bad\udc00"],
]) test(`invalid ${name} math fallback diagnostics refuse before drawing`, async t => {
  const f = painter(t);
  await assert.rejects(f.renderer.render(new Session([placed(17, { mathFallback: value })]), size), code("INVALID_WASM_RESPONSE"));
  assert.equal(f.stages.length, 0);
});

test("fallback reasons admit exactly 160 Unicode scalars and remain observable through the public snapshot", async t => {
  const reason = "😀".repeat(160), source = new Session([placed(17, { mathFallback: reason })], false);
  const adapter = createFlowAdapter({ revision: "1", layoutRevision: "1", free() {},
    snapshotJson(revision, layoutRevision, offset, limit, glyphs) {
      return JSON.stringify(source.snapshot({ offset, limit, glyphs, token: { revision, layoutRevision } }));
    } });
  t.after(() => adapter.dispose());
  const item = adapter.snapshot({ glyphs: true }).items[0];
  assert.equal(item.mathFallback, reason); assert.equal(item.baselineY, 17);
  const f = painter(t); await f.renderer.render(source, size);
  assert.deepEqual(translations(f.stages[0]), [["translate", 0, 17]]);
});

test("source changes during mixed-line preparation reject the captured baseline before publication", async t => {
  const f = painter(t), s = new Session([{ ...text(0, 0, "1", 8192), baselineY: 17 }]);
  await f.renderer.render(new Session([placed(13)]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length;
  const pending = f.renderer.render(s, size), refused = assert.rejects(pending, code("STALE_LAYOUT"));
  const timer = setTimeout(() => { s.token = { revision: "2", layoutRevision: "2" }; }, 0);
  try { await refused; } finally { clearTimeout(timer); }
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
});
