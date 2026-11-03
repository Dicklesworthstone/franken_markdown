// Consumer contract tests. Real native-produced math paths are exercised by
// tests/run_flow_math.py; these fixtures isolate malformed/adversarial payloads.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";
import { SnapshotDeltaEncoder, SnapshotDeltaDecoder } from "./worker_transport.mjs";
import { box, vector, text, Session, Surface, sparsePage } from "./tests/flow_canvas_viewport_fixtures.mjs";

const size = { width: 100, height: 60 };
const code = wanted => error => error.code === wanted;
const path = (extra = {}) => ({ kind: "path", bounds: box(5, 5, 30, 30), colorRole: "accent", fill: null,
  enclosingSourceSpan: { startByte: 0, endByte: 12 },
  commands: [["M", 5, 5], ["L", 35, 5], ["Q", 35, 20, 30, 35], ["C", 20, 35, 5, 20, 5, 5], ["Z"]], ...extra });
const complex = (count = 65536) => path({ commands: [["M", 5, 5],
  ...Array.from({ length: count - 2 }, (_, i) => ["L", 5 + i % 30, 5 + (i >> 1) % 30]), ["Z"]] });
function surface(width, height) {
  const out = new Surface(width, height), original = out.context;
  out.context = new Proxy(original, { get(target, name) {
    return name === "bezierCurveTo" ? (...args) => out.log.push([name, ...args]) : target[name];
  } });
  return out;
}
function painter(t, limits) {
  const canvas = surface(100, 60), stages = [];
  const fixture = { canvas, stages, onStage() {} };
  fixture.renderer = new FlowCanvasRenderer(canvas, { limits,
    canvasFactory: (w, h) => { const next = surface(w, h); stages.push(next); fixture.onStage(next); return next; } });
  t.after(() => fixture.renderer.dispose());
  return fixture;
}

for (const indexed of [false, true]) test(`filled paths draw exact curves through ${indexed ? "indexed" : "snapshot"} pages`, async t => {
  const f = painter(t), s = new Session([path()], indexed);
  let imageCalls = 0;
  const frame = await f.renderer.render(s, { ...size, resolveImage() { imageCalls++; throw Error("unexpected image request"); } });
  assert.equal(frame.visibleItems, 1); assert.equal(frame.glyphs, 0); assert.equal(frame.missingImages, 0);
  assert.equal(s.outlines.length, 0); assert.equal(imageCalls, 0);
  const log = f.stages[0].log;
  assert.deepEqual(log.filter(row => ["moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "closePath"].includes(row[0])),
    [["moveTo", 5, 5], ["lineTo", 35, 5], ["quadraticCurveTo", 35, 20, 30, 35], ["bezierCurveTo", 20, 35, 5, 20, 5, 5], ["closePath"]]);
  assert(log.some(row => row[0] === "fill" && row[1] === "nonzero"));
  assert(log.some(row => row[0] === "set" && row[1] === "fillStyle" && row[2] === "#0969da"));
  assert.equal(f.renderer.cacheStats.commands, 0); // Absolute document geometry is never a font-cache entry.
});

test("explicit RGBA overrides the palette without changing color/alpha semantics", async t => {
  const f = painter(t);
  await f.renderer.render(new Session([path({ fill: [1, 0.5, 0, 0.25] }), path({ fill: [0, 0, 0, 0] })]), size);
  const styles = f.stages[0].log.filter(row => row[0] === "set" && row[1] === "fillStyle").map(row => row[2]);
  assert(styles.includes("rgba(255,127.5,0,0.25)")); assert(styles.includes("rgba(0,0,0,0)"));
});

test("multiple closed contours preserve holes and an unknown palette role uses text color", async t => {
  const f = painter(t), item = path({ colorRole: "future-math-role" });
  item.commands.push(["M", 10, 10], ["L", 10, 20], ["L", 20, 20], ["L", 20, 10], ["Z"]);
  await f.renderer.render(new Session([item]), size);
  assert.equal(f.stages[0].log.filter(row => row[0] === "fill").length, 1);
  assert.equal(f.stages[0].log.filter(row => row[0] === "closePath").length, 2);
  assert(f.stages[0].log.some(row => row[0] === "set" && row[1] === "fillStyle" && row[2] === "#1f2328"));
});

const invalidPaths = [
  ["empty path", p => { p.commands = []; }],
  ["missing commands", p => { delete p.commands; }],
  ["unknown opcode", p => { p.commands[1][0] = "A"; }],
  ["wrong tuple size", p => { p.commands[1].push(0); }],
  ["numeric string", p => { p.commands[1][1] = "35"; }],
  ["nonfinite endpoint", p => { p.commands[1][1] = Infinity; }],
  ["NaN control point", p => { p.commands[2][1] = NaN; }],
  ["line outside contour", p => { p.commands.shift(); }],
  ["unclosed contour", p => { p.commands.pop(); }],
  ["move inside open contour", p => { p.commands.splice(1, 0, ["M", 5, 5]); }],
  ["close without contour", p => { p.commands.unshift(["Z"]); }],
  ["empty contour", p => { p.commands = [["M", 5, 5], ["Z"]]; }],
  ["repeated close", p => { p.commands.push(["Z"]); }],
  ["endpoint outside bounds", p => { p.commands[1][1] = 36; }],
  ["quadratic control outside bounds", p => { p.commands[2][2] = 36; }],
  ["cubic first control outside bounds", p => { p.commands[3][1] = 36; }],
  ["cubic second control outside bounds", p => { p.commands[3][3] = 36; }],
  ["missing fill", p => { delete p.fill; }],
  ["short fill", p => { p.fill = [1, 0, 0]; }],
  ["long fill", p => { p.fill = [1, 0, 0, 1, 0]; }],
  ["string fill", p => { p.fill = "red"; }],
  ["NaN fill", p => { p.fill = [0, NaN, 0, 1]; }],
  ["negative fill", p => { p.fill = [-0.01, 0, 0, 1]; }],
  ["overbright fill", p => { p.fill = [1.01, 0, 0, 1]; }],
  ["invalid alpha", p => { p.fill = [0, 0, 0, 2]; }],
  ["missing color role", p => { delete p.colorRole; }],
];
for (const [name, change] of invalidPaths) test(`invalid display path (${name}) preserves prior publication`, async t => {
  const f = painter(t); await f.renderer.render(new Session([vector()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length, item = path(); change(item);
  await assert.rejects(f.renderer.render(new Session([item]), size), code("INVALID_WASM_RESPONSE"));
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits); assert.equal(f.stages.length, 1);
});

test("only native f32 edge-rounding tolerance is admitted", async t => {
  const f = painter(t), item = path(); item.commands[1][1] += 0.00002;
  await f.renderer.render(new Session([item]), size);
  item.commands[1][1] = 35.001;
  await assert.rejects(f.renderer.render(new Session([item]), size), code("INVALID_WASM_RESPONSE"));
});

test("per-item command bound refuses before allocating a stage", async t => {
  const f = painter(t);
  await assert.rejects(f.renderer.render(new Session([complex(65537)]), size), code("INVALID_WASM_RESPONSE"));
  assert.equal(f.stages.length, 0);
});

test("native aggregate path bound applies even if custom consumer limits would permit more", async t => {
  const f = painter(t), item = complex();
  const s = new Session(Array(5).fill(item));
  await assert.rejects(f.renderer.render(s, size), code("BUDGET_EXCEEDED"));
  assert.equal(f.stages.length, 0); assert.equal(s.outlines.length, 0);
});

for (const limit of ["maxFrameCommands", "maxDrawCommands"]) test(`${limit} charges math and text together`, async t => {
  const f = painter(t, { [limit]: 9 }); // Five display commands + five font outline commands.
  await assert.rejects(f.renderer.render(new Session([path(), text()]), size), code("BUDGET_EXCEEDED"));
  assert.equal(f.stages.length, 0);
});

test("caller mutation after the first cooperative turn cannot change admitted geometry or fill", async t => {
  const f = painter(t), s = new Session(), item = complex(8192); item.fill = [1, 0, 0, 1];
  s.viewport = query => sparsePage(s, query, [{ ...item, index: 42 }]);
  const pending = f.renderer.render(s, size);
  const timer = setTimeout(() => { item.commands[1000] = ["L", NaN, NaN]; item.fill[1] = 1; }, 0);
  try { await pending; } finally { clearTimeout(timer); }
  assert(f.stages[0].log.some(row => row[0] === "set" && row[1] === "fillStyle" && row[2] === "rgba(255,0,0,1)"));
  assert(!f.stages[0].log.some(row => row.slice(1).some(value => typeof value === "number" && !Number.isFinite(value))));
});

test("one maximal path yields during drawing and abort leaves the previous frame intact", async t => {
  const f = painter(t); await f.renderer.render(new Session([path()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length, control = new AbortController(); let timer;
  f.onStage = () => { timer = setTimeout(() => control.abort(), 0); };
  try { await assert.rejects(f.renderer.render(new Session([complex()]), { ...size, signal: control.signal }), code("ABORTED")); }
  finally { clearTimeout(timer); }
  const lines = f.stages.at(-1).log.filter(row => row[0] === "lineTo").length;
  assert(lines > 0 && lines < 65534, `drawn lines: ${lines}`);
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
});

for (const action of ["source", "layout", "session", "clear", "dispose"]) test(`${action} invalidation fences a suspended math stage`, async t => {
  const f = painter(t), s = new Session([complex()]); await f.renderer.render(new Session([path()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length; let timer;
  f.onStage = () => { timer = setTimeout(() => {
    if (action === "source") s.token = { revision: "2", layoutRevision: "2" };
    else if (action === "layout") s.reflow();
    else if (action === "session") s.disposed = true;
    else f.renderer[action]();
  }, 0); };
  const expected = { source: "STALE_LAYOUT", layout: "STALE_LAYOUT", session: "SESSION_DISPOSED",
    clear: "RENDER_SUPERSEDED", dispose: "RENDERER_DISPOSED" }[action];
  try { await assert.rejects(f.renderer.render(s, size), code(expected)); }
  finally { clearTimeout(timer); }
  assert.equal(f.canvas.log.length, blits);
  assert.equal(f.renderer.frame, action === "clear" || action === "dispose" ? null : before);
});

test("fresh source paint supersedes a suspended old formula and can complete normally", async t => {
  const f = painter(t), old = new Session([complex()]), next = new Session([path({ fill: [0, 1, 0, 1] })]);
  next.token = { revision: "2", layoutRevision: "3" }; let replacement, timer;
  f.onStage = () => { f.onStage = () => {}; timer = setTimeout(() => { replacement = f.renderer.render(next, size); }, 0); };
  try { await assert.rejects(f.renderer.render(old, size), code("RENDER_SUPERSEDED")); await replacement; }
  finally { clearTimeout(timer); }
  assert.equal(f.renderer.frame.revision, "2"); assert.equal(f.renderer.frame.layoutRevision, "3");
  assert.equal(old.disposed, false); assert.equal(next.disposed, false);
});

test("legacy and sparse clipping apply the same absolute geometry and scroll transform", async t => {
  const items = [{ kind: "clip", bounds: box(10, 8, 15, 15), childCount: 1 }, path()];
  const a = painter(t), b = painter(t), options = { ...size, scrollX: 2, scrollY: 3, pixelRatio: 2 };
  await a.renderer.render(new Session(items, false), options);
  await b.renderer.render(new Session(items, true), options);
  assert.deepEqual(a.stages[0].log, b.stages[0].log);
  assert(a.stages[0].log.some(row => JSON.stringify(row) === JSON.stringify(["rect", 10, 8, 15, 15])));
  assert(a.stages[0].log.some(row => JSON.stringify(row) === JSON.stringify(["setTransform", 2, 0, 0, 2, -4, -6])));
});

test("worker snapshot delta transport preserves path tuples, RGBA and immutable baselines", () => {
  const encoder = new SnapshotDeltaEncoder(), decoder = new SnapshotDeltaDecoder();
  const s = new Session(Array.from({ length: 32 }, () => path({ fill: [0.5, 0.25, 1, 0.5] })), false);
  const original = s.snapshot({ offset: 0, limit: 256 });
  const first = decoder.decode(structuredClone(encoder.encode(original, null)));
  first.items[0].commands[3][5] = 999;
  first.items[0].fill[0] = 0;
  const changed = structuredClone(original); changed.revision = "2";
  changed.items[12].commands[3][5] = 8;
  const packet = structuredClone(encoder.encode(changed, decoder.baseId));
  assert.equal(packet.kind, "delta"); assert.equal(packet.changes.length, 1);
  assert.deepEqual(decoder.decode(packet), changed);
});
