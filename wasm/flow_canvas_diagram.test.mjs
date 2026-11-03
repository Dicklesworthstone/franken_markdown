// Consumer contract tests with explicit wire and recording Canvas doubles.
// The generated-WASM diagram gate separately proves the native producer.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";
import { SnapshotDeltaEncoder, SnapshotDeltaDecoder } from "./worker_transport.mjs";
import { box, vector, text, Session, Surface, sparsePage } from "./tests/flow_canvas_viewport_fixtures.mjs";

const size = { width: 100, height: 60 };
const code = wanted => error => error.code === wanted;
const stroke = (extra = {}) => ({ color: [0.2, 0.4, 0.6, 0.5], width: 2, dash: [4, 2],
  dashOffset: -1.5, cap: "round", join: "round", miterLimit: 2, ...extra });
const path = (extra = {}) => ({ kind: "path", bounds: box(8, 8, 34, 34), colorRole: "accent", fill: null,
  enclosingSourceSpan: { startByte: 0, endByte: 32 }, stroke: stroke(),
  commands: [["M", 10, 10], ["L", 40, 10], ["Q", 40, 25, 25, 40], ["C", 20, 40, 10, 20, 10, 10]], ...extra });
function surface(width, height) {
  const out = new Surface(width, height), original = out.context;
  out.context = new Proxy(original, { get(target, name) {
    return ["bezierCurveTo", "setLineDash"].includes(name)
      ? (...args) => out.log.push([name, ...args.map(value => Array.isArray(value) ? [...value] : value)])
      : target[name];
  } });
  return out;
}
function painter(t, limits) {
  const canvas = surface(100, 60), stages = [], fixture = { canvas, stages, onStage() {} };
  fixture.renderer = new FlowCanvasRenderer(canvas, { limits,
    canvasFactory(w, h) { const stage = surface(w, h); stages.push(stage); fixture.onStage(stage); return stage; } });
  t.after(() => fixture.renderer.dispose()); return fixture;
}
function borrowed(item) {
  const session = new Session();
  session.viewport = query => {
    item.index = 42; item.effectiveClip = query.viewport;
    return sparsePage(session, query, [], { items: [item] });
  };
  return session;
}
const property = (log, name) => log.filter(row => row[0] === "set" && row[1] === name).map(row => row[2]);

for (const indexed of [false, true]) test(`open diagram curves retain real stroke geometry in ${indexed ? "viewport" : "snapshot"} paints`, async t => {
  const f = painter(t), s = new Session([path()], indexed);
  await f.renderer.render(s, { ...size, resolveImage() { throw Error("diagram must not request an image"); } });
  const log = f.stages[0].log;
  assert.deepEqual(log.filter(row => ["moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "closePath"].includes(row[0])),
    [["moveTo", 10, 10], ["lineTo", 40, 10], ["quadraticCurveTo", 40, 25, 25, 40], ["bezierCurveTo", 20, 40, 10, 20, 10, 10]]);
  assert.equal(log.filter(row => row[0] === "fill").length, 0);
  assert.equal(log.filter(row => row[0] === "stroke").length, 1);
  assert.deepEqual(log.filter(row => row[0] === "setLineDash"), [["setLineDash", [4, 2]]]);
  assert(property(log, "strokeStyle").includes("rgba(51,102,153,0.5)"));
  assert(property(log, "lineWidth").includes(2));
  assert(property(log, "lineDashOffset").includes(-1.5));
  assert(property(log, "lineCap").includes("round"));
  assert(property(log, "lineJoin").includes("round"));
  assert(property(log, "miterLimit").includes(2));
  assert.equal(s.outlines.length, 0); assert.equal(f.renderer.cacheStats.commands, 0);
});

test("explicit fill and stroke both paint while absent or null stroke preserves legacy palette fill", async t => {
  const f = painter(t), filled = path({ fill: [1, 0.5, 0, 0.25] }); filled.commands.push(["Z"]);
  for (const item of [filled, { ...filled, fill: null, stroke: null }, { ...filled, fill: null, stroke: undefined }])
    await f.renderer.render(new Session([item]), size);
  assert.equal(f.stages[0].log.filter(row => row[0] === "fill").length, 1);
  assert.equal(f.stages[0].log.filter(row => row[0] === "stroke").length, 1);
  assert(property(f.stages[0].log, "fillStyle").includes("rgba(255,127.5,0,0.25)"));
  for (const stage of f.stages.slice(1)) {
    assert.equal(stage.log.filter(row => row[0] === "fill").length, 1);
    assert.equal(stage.log.filter(row => row[0] === "stroke").length, 0);
    assert(property(stage.log, "fillStyle").includes("#0969da"));
  }
});

test("stroke-only paths admit multiple nonempty open and closed subpaths without closing open routes", async t => {
  const f = painter(t), item = path();
  item.commands.push(["M", 12, 12], ["L", 16, 20], ["Z"], ["M", 30, 30], ["L", 32, 32]);
  await f.renderer.render(new Session([item]), size);
  assert.equal(f.stages[0].log.filter(row => row[0] === "moveTo").length, 3);
  assert.equal(f.stages[0].log.filter(row => row[0] === "closePath").length, 1);
  assert.equal(f.stages[0].log.filter(row => row[0] === "stroke").length, 1);
});

for (const [name, change] of [
  ["empty stroke", p => { p.stroke = {}; }],
  ["missing width", p => { delete p.stroke.width; }],
  ["missing dash", p => { delete p.stroke.dash; }],
  ["unknown stroke field", p => { p.stroke.opacity = 0.5; }],
  ["symbol stroke field", p => { p.stroke[Symbol("extra")] = 1; }],
  ["array stroke", p => { p.stroke = []; }],
  ["zero width", p => { p.stroke.width = 0; }],
  ["negative width", p => { p.stroke.width = -1; }],
  ["unbounded width", p => { p.stroke.width = 1e6 + 1; }],
  ["infinite width", p => { p.stroke.width = Infinity; }],
  ["string width", p => { p.stroke.width = "2"; }],
  ["invalid cap", p => { p.stroke.cap = "flat"; }],
  ["invalid join", p => { p.stroke.join = "arcs"; }],
  ["small miter limit", p => { p.stroke.miterLimit = 0.9; }],
  ["unbounded miter limit", p => { p.stroke.miterLimit = 1e6 + 1; }],
  ["NaN miter limit", p => { p.stroke.miterLimit = NaN; }],
  ["infinite dash offset", p => { p.stroke.dashOffset = Infinity; }],
  ["unbounded dash offset", p => { p.stroke.dashOffset = 1e12 + 1; }],
  ["negative dash", p => { p.stroke.dash = [-1, 2]; }],
  ["NaN dash", p => { p.stroke.dash = [1, NaN]; }],
  ["all-zero dash", p => { p.stroke.dash = [0, 0]; }],
  ["unbounded dash", p => { p.stroke.dash = [1e6 + 1]; }],
  ["too many dashes", p => { p.stroke.dash = Array(17).fill(1); }],
  ["sparse dash", p => { p.stroke.dash = Array(2); p.stroke.dash[0] = 1; }],
  ["invalid stroke alpha", p => { p.stroke.color[3] = 2; }],
  ["empty open contour", p => { p.commands = [["M", 10, 10]]; }],
  ["empty intermediate contour", p => { p.commands.unshift(["M", 10, 10]); }],
  ["open filled contour", p => { p.fill = [1, 0, 0, 1]; }],
  ["implicit closing legacy path", p => { p.stroke = null; }],
  ["stroke hull misses halo", p => { p.bounds = box(10, 10, 30, 30); }],
]) test(`invalid diagram paint (${name}) preserves the last complete frame`, async t => {
  const f = painter(t); await f.renderer.render(new Session([vector()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length, item = path(); change(item);
  await assert.rejects(f.renderer.render(borrowed(item), size), code("INVALID_WASM_RESPONSE"));
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits); assert.equal(f.stages.length, 1);
});

for (const [name, target] of [
  ["stroke field", item => [item, "stroke"]],
  ["stroke width", item => [item.stroke, "width"]],
  ["stroke color", item => [item.stroke, "color"]],
  ["stroke color component", item => [item.stroke.color, "1"]],
  ["dash component", item => [item.stroke.dash, "0"]],
  ["path command", item => [item.commands, "1"]],
  ["coordinate", item => [item.commands[1], "1"]],
]) test(`paint admission refuses ${name} accessors without executing them`, async t => {
  const f = painter(t), item = path(), [object, key] = target(item); let reads = 0;
  Object.defineProperty(object, key, { enumerable: true, configurable: true, get() { reads++; throw Error("accessor executed"); } });
  await assert.rejects(f.renderer.render(borrowed(item), size), code("INVALID_WASM_RESPONSE"));
  assert.equal(reads, 0); assert.equal(f.stages.length, 0);
});

test("miter and square-cap halos use conservative native bounds and f32 edge tolerance", async t => {
  const f = painter(t);
  for (const [join, cap, miterLimit, halo] of [["miter", "butt", 4, 8], ["bevel", "square", 1, 2 * Math.SQRT2]]) {
    const item = path({ stroke: stroke({ width: 4, join, cap, miterLimit }), bounds: box(10 - halo, 10 - halo, 30 + halo * 2, 30 + halo * 2) });
    await f.renderer.render(new Session([item]), size);
    item.bounds.x += 0.00001; item.bounds.width -= 0.00001;
    await f.renderer.render(new Session([item]), size);
    item.bounds.x += 0.01; item.bounds.width -= 0.01;
    await assert.rejects(f.renderer.render(new Session([item]), size), code("INVALID_WASM_RESPONSE"));
  }
});

test("solid and odd-length dash patterns preserve literal Canvas stroke semantics", async t => {
  const f = painter(t);
  for (const dash of [[], [0, 2, 3], Array(16).fill(1)])
    await f.renderer.render(new Session([path({ stroke: stroke({ dash }) })]), size);
  assert.deepEqual(f.stages.map(stage => stage.log.find(row => row[0] === "setLineDash")[1]), [[], [0, 2, 3], Array(16).fill(1)]);
});

test("stroke colors and dashes are owned before a long path yields", async t => {
  const f = painter(t), item = path({ commands: [["M", 10, 10], ...Array.from({ length: 8191 }, (_, i) => ["L", 10 + i % 30, 10 + (i >> 1) % 30])] });
  const pending = f.renderer.render(borrowed(item), size);
  const timer = setTimeout(() => { item.stroke.color[0] = 1; item.stroke.dash[0] = 99; item.stroke.width = NaN; item.commands[4000][1] = NaN; }, 0);
  try { await pending; } finally { clearTimeout(timer); }
  assert(Number.isNaN(item.stroke.width));
  assert(property(f.stages[0].log, "strokeStyle").includes("rgba(51,102,153,0.5)"));
  assert.deepEqual(f.stages[0].log.find(row => row[0] === "setLineDash"), ["setLineDash", [4, 2]]);
  assert(property(f.stages[0].log, "lineWidth").includes(2));
  assert(!f.stages[0].log.some(row => row.slice(1).some(value => typeof value === "number" && !Number.isFinite(value))));
});

test("native label fill preserves alpha and baseline and is owned before glyph planning yields", async t => {
  const f = painter(t), item = { ...text(0, 0, "1", 8192), baselineY: 17, fill: [0.75, 0.25, 0.5, 0.125] };
  const pending = f.renderer.render(borrowed(item), size);
  const timer = setTimeout(() => { item.fill[0] = 0; item.fill[3] = NaN; }, 0);
  try { await pending; } finally { clearTimeout(timer); }
  assert(property(f.stages[0].log, "fillStyle").includes("rgba(191.25,63.75,127.5,0.125)"));
  assert(f.stages[0].log.filter(row => row[0] === "translate").every(row => row[2] === 17));
});

for (const fill of [null, "red", [1, 0, 0], [0, 0, 0, 1, 1], [0, NaN, 0, 1], [0, 0, 0, -1]])
  test(`malformed label fill ${String(fill)} refuses before outlines or publication`, async t => {
    const f = painter(t), s = new Session([{ ...text(), fill }]);
    await assert.rejects(f.renderer.render(s, size), code("INVALID_WASM_RESPONSE"));
    assert.equal(s.outlines.length, 0); assert.equal(f.stages.length, 0);
  });

test("label fill accessor refuses without executing host code", async t => {
  const f = painter(t), item = text(); let reads = 0;
  Object.defineProperty(item, "fill", { get() { reads++; return [1, 0, 0, 1]; } });
  await assert.rejects(f.renderer.render(borrowed(item), size), code("INVALID_WASM_RESPONSE"));
  assert.equal(reads, 0);
});

test("worker deltas preserve nested stroke metadata and label colors with isolated ownership", () => {
  const encoder = new SnapshotDeltaEncoder(), decoder = new SnapshotDeltaDecoder();
  const s = new Session(Array.from({ length: 32 }, () => path()), false);
  const original = s.snapshot({ offset: 0, limit: 256 });
  const first = decoder.decode(structuredClone(encoder.encode(original, null)));
  first.items[0].stroke.dash[0] = 100; first.items[0].stroke.color[0] = 1;
  const changed = structuredClone(original); changed.revision = "2";
  changed.items[12].stroke.width = 3;
  const packet = structuredClone(encoder.encode(changed, decoder.baseId));
  assert.equal(packet.kind, "delta"); assert.equal(packet.changes.length, 1);
  assert.deepEqual(decoder.decode(packet), changed);
});
