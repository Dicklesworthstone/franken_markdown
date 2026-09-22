// Real event-loop tasks exercise production cooperative work. Session/font and
// recording Canvas fixtures remain synthetic; no native/WASM performance claim.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";
import { box, vector, text, paths, Session, Surface, sparsePage } from "./tests/flow_canvas_viewport_fixtures.mjs";
const size = { width: 100, height: 60 };
const code = wanted => error => error.code === wanted;
function painter(t) {
  const canvas = new Surface(), stages = [];
  const fixture = { canvas, stages, onStage: () => {} };
  fixture.renderer = new FlowCanvasRenderer(canvas, { canvasFactory: (w, h) => {
    const stage = new Surface(w, h); stages.push(stage); fixture.onStage(stage); return stage;
  } });
  t.after(() => fixture.renderer.dispose()); return fixture;
}
const many = n => Array.from({ length: n }, () => vector(box(0, 0, 1, 1)));
function complexSession(count = 1, commands = 20000) {
  const s = new Session([text(0, 0, "1", count)]);
  s.glyphOutlines = (id, ids) => {
    s.outlines.push([id, [...ids]]);
    const batch = paths(id, ids);
    for (const glyph of batch.glyphs) glyph.commands = commands === 0 ? [] : [
      ["M", 0, 0], ...Array.from({ length: commands - 2 }, (_, i) => ["L", i % 10, (i >>> 1) % 10]), ["Z"]
    ];
    return batch;
  };
  return s;
}

test("small indexed paints stay on the no-timer fast path", async t => {
  const f = painter(t), original = globalThis.setTimeout; let scheduled = 0;
  globalThis.setTimeout = (...args) => { scheduled++; return original(...args); };
  try { await f.renderer.render(new Session([text(), vector()]), size); }
  finally { globalThis.setTimeout = original; }
  assert.equal(scheduled, 0); assert.equal(f.stages.length, 1);
});

test("a timer can abort a synchronous legacy inventory before all pages are scanned", async t => {
  const f = painter(t); await f.renderer.render(new Session([vector()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length;
  const s = new Session(Array.from({ length: 40000 }, () => ({ kind: "anchor", bounds: box(0, 1000) })), false);
  const control = new AbortController(), pending = f.renderer.render(s, { ...size, signal: control.signal });
  const refused = assert.rejects(pending, code("ABORTED"));
  const timer = setTimeout(() => control.abort(), 0);
  try { await refused; } finally { clearTimeout(timer); }
  assert(s.snapshots.length > 0 && s.snapshots.length < Math.ceil(40000 / 256));
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
  assert.equal(f.stages.length, 1); assert.equal(s.disposed, false);
});

test("single oversized text fragments yield during glyph planning, not after it", async t => {
  const f = painter(t), s = new Session(), run = text(0, 0, "1", 50000); let examined = 0;
  for (const glyph of run.fontRun.glyphs) Object.defineProperty(glyph, "xAdvance", { get() { examined++; return 10; } });
  s.viewport = q => sparsePage(s, q, [{ ...run, index: 42 }]);
  const control = new AbortController(), pending = f.renderer.render(s, { ...size, signal: control.signal });
  const refused = assert.rejects(pending, code("ABORTED"));
  const timer = setTimeout(() => control.abort(), 0);
  try { await refused; } finally { clearTimeout(timer); }
  assert(examined > 0 && examined <= 4096, `examined ${examined} glyphs`);
  assert.equal(s.outlines.length, 0); assert.equal(f.stages.length, 0);
});

test("a single warm cached contour yields mid-path and never publishes a partial glyph", async t => {
  const f = painter(t), s = complexSession(1, 65536);
  await f.renderer.render(s, size);
  const before = f.renderer.frame, blits = f.canvas.log.length, calls = s.outlines.length;
  const control = new AbortController(); let timer;
  f.onStage = () => { timer = setTimeout(() => control.abort(), 0); };
  try { await assert.rejects(f.renderer.render(s, { ...size, signal: control.signal }), code("ABORTED")); }
  finally { clearTimeout(timer); }
  const drawn = f.stages.at(-1).log.filter(row => row[0] === "lineTo").length;
  assert(drawn > 0 && drawn < 65534); assert.equal(s.outlines.length, calls);
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
  assert.equal(s.disposed, false);
});

test("zero-outline glyphs still consume cooperative drawing work", async t => {
  const f = painter(t), s = complexSession(5000, 0), control = new AbortController(); let timer;
  f.onStage = () => { timer = setTimeout(() => control.abort(), 0); };
  try { await assert.rejects(f.renderer.render(s, { ...size, signal: control.signal }), code("ABORTED")); }
  finally { clearTimeout(timer); }
  const drawn = f.stages[0].log.filter(row => row[0] === "scale").length;
  assert(drawn > 0 && drawn < 5000); assert.equal(f.renderer.frame, null);
});

test("vector-only paints yield after staging begins without changing target pixels", async t => {
  const f = painter(t); await f.renderer.render(new Session([text()]), size);
  const before = f.renderer.frame, blits = f.canvas.log.length, control = new AbortController(); let timer;
  f.onStage = () => { timer = setTimeout(() => control.abort(), 0); };
  try { await assert.rejects(f.renderer.render(new Session(many(10000)), { ...size, signal: control.signal }), code("ABORTED")); }
  finally { clearTimeout(timer); }
  assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
  const fills = f.stages.at(-1).log.filter(row => row[0] === "fillRect").length;
  assert(fills > 1 && fills < 10001);
});

test("source/layout changes during suspended drawing fence the stage before publication", async t => {
  for (const edit of [false, true]) {
    const f = painter(t), s = complexSession(); await f.renderer.render(s, size);
    const before = f.renderer.frame, blits = f.canvas.log.length; let timer;
    f.onStage = () => { timer = setTimeout(() => {
      s.reflow(); if (edit) s.token.revision = "2";
    }, 0); };
    try { await assert.rejects(f.renderer.render(s, size), code("STALE_LAYOUT")); }
    finally { clearTimeout(timer); }
    assert.equal(f.renderer.frame, before); assert.equal(f.canvas.log.length, blits);
  }
});

test("a replacement paint wins while the older cached contour is suspended", async t => {
  const f = painter(t), old = complexSession(); await f.renderer.render(old, size);
  const next = new Session([vector()]); next.token = { revision: "2", layoutRevision: "7" };
  let replacement, timer;
  f.onStage = () => {
    f.onStage = () => {};
    timer = setTimeout(() => { replacement = f.renderer.render(next, size); }, 0);
  };
  try { await assert.rejects(f.renderer.render(old, size), code("RENDER_SUPERSEDED")); await replacement; }
  finally { clearTimeout(timer); }
  assert.equal(f.renderer.frame.revision, "2"); assert.equal(f.renderer.frame.layoutRevision, "7");
  assert.equal(old.disposed, false); assert.equal(next.disposed, false);
});

test("clear and dispose interrupt suspended stages without resurrecting pixels", async t => {
  for (const action of ["clear", "dispose"]) {
    const f = painter(t), s = complexSession(); await f.renderer.render(s, size);
    const blits = f.canvas.log.length; let timer;
    f.onStage = () => { timer = setTimeout(() => f.renderer[action](), 0); };
    try { await assert.rejects(f.renderer.render(s, size), code(action === "clear" ? "RENDER_SUPERSEDED" : "RENDERER_DISPOSED")); }
    finally { clearTimeout(timer); }
    assert.equal(f.renderer.frame, null); assert.equal(f.canvas.log.length, blits); assert.equal(s.disposed, false);
    if (action === "dispose") assert.deepEqual(f.renderer.cacheStats, { glyphs: 0, commands: 0 });
  }
});

test("outline snapshots stay immutable across post-copy cooperative cancellation", async t => {
  const f = painter(t), s = complexSession(1, 65536), original = s.glyphOutlines.bind(s); let borrowed;
  s.glyphOutlines = (id, ids) => { borrowed = original(id, ids); return borrowed; };
  const control = new AbortController(), pending = f.renderer.render(s, { ...size, signal: control.signal });
  const refused = assert.rejects(pending, code("ABORTED"));
  const timer = setTimeout(() => { control.abort(); if (borrowed) borrowed.glyphs[0].commands[1] = ["L", NaN, NaN]; }, 0);
  try { await refused; } finally { clearTimeout(timer); }
  assert(borrowed); assert.equal(f.stages.length, 0);
  await f.renderer.render(s, size);
  assert.equal(s.outlines.length, 1); assert.equal(f.renderer.frame.glyphs, 1);
  assert(f.stages[0].log.every(row => !row.slice(1).some(value => typeof value === "number" && Number.isNaN(value))));
});

test("selection-only drawing cooperates before touching the target", async t => {
  const f = painter(t), s = new Session(), control = new AbortController(); let timer;
  f.onStage = () => { timer = setTimeout(() => control.abort(), 0); };
  try {
    await assert.rejects(f.renderer.render(s, { ...size, signal: control.signal,
      selection: { ...s.token, rectangles: Array.from({ length: 10000 }, () => box(0, 0, 1, 1)) }
    }), code("ABORTED"));
  } finally { clearTimeout(timer); }
  const fills = f.stages[0].log.filter(row => row[0] === "fillRect").length;
  assert(fills > 1 && fills < 10001); assert.equal(f.canvas.log.length, 0); assert.equal(f.renderer.frame, null);
});
