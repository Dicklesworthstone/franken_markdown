// Production renderer, actual JS ingress and deterministic drawing traces.
// Native index/codec and real browser rasterization are separate test gates.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";
import { createFlowAdapter } from "./flow_session.mjs";
import { withGlyphOutlines } from "./flow_outlines.mjs";
import { box, vector, text, image, paths, Session, Surface, sparsePage, deferred } from "./tests/flow_canvas_viewport_fixtures.mjs";
const dimensions = { width: 100, height: 60 };
const code = wanted => error => error.code === wanted;
const tick = () => new Promise(resolve => setImmediate(resolve));
function painter(t, options = {}) {
  const canvas = new Surface(), stages = [];
  const renderer = new FlowCanvasRenderer(canvas, { canvasFactory: (w, h) => {
    const stage = new Surface(w, h); stages.push(stage); return stage;
  }, ...options });
  t.after(() => renderer.dispose()); return { renderer, canvas, stages };
}

test("advertised native viewport paints a million-item document without scanning its prefix", async t => {
  const session = new Session(), { renderer } = painter(t, { limits: { maxScannedItems: 32 } });
  session.snapshot = () => assert.fail("must not fetch the full display inventory");
  session.viewport = function (...args) {
    assert.equal(args.length, 1); const [query] = args; this.queries.push(query);
    assert.equal(query.glyphs, true); assert.equal(query.limit, 256); assert.deepEqual(query.token, this.token);
    return sparsePage(this, query, [{ ...text(0, 19000000), index: 950000 }]);
  };
  const frame = await renderer.render(session, { ...dimensions, scrollY: 19000000 });
  assert.equal(session.queries.length, 1); assert.equal(frame.totalItems, 1000000);
  assert.equal(frame.scannedItems, 32); assert.equal(frame.receivedItems, 1);
  assert.equal(frame.queryMode, "indexed-viewport"); assert.equal(frame.visibleItems, 1);
  assert.equal((await renderer.hitTest(5, 6)).hit.itemIndex, 950000);
  assert.equal(session.hits[0].y, 19000006);
});

test("sparse pagination uses original item cursors rather than returned-item counts", async t => {
  const s = new Session(), { renderer } = painter(t);
  s.viewport = query => {
    s.queries.push(query);
    if (query.afterIndex === 0) return sparsePage(s, query,
      Array.from({ length: 256 }, (_, i) => ({ ...vector(box(0, 0, 1, 1)), index: 1000 + i * 2 })),
      { visitedEntries: 300, nextIndex: 1511 });
    assert.equal(query.afterIndex, 1511);
    return sparsePage(s, query, [{ ...vector(box(0, 1, 1, 1)), index: 999999 }]);
  };
  const f = await renderer.render(s, dimensions);
  assert.deepEqual(s.queries.map(q => q.afterIndex), [0, 1511]); assert.equal(f.receivedItems, 257);
  assert.equal(f.scannedItems, 332); assert.equal(f.visibleItems, 257); assert.equal(s.snapshots.length, 0);
});

test("indexed and dense draws agree for nested, crossing and expired clip scopes", async t => {
  const items = Array.from({ length: 254 }, () => ({ kind: "anchor", bounds: box() }));
  items.push({ kind: "clip", bounds: box(0, 0, 10, 20), childCount: 3 },
    { kind: "clip", bounds: box(0, 0, 5, 20), childCount: 1 },
    vector(box(0, 0, 20, 10)), vector(box(0, 10, 20, 10), "accent"), vector(box(0, 20, 20, 10), "link"));
  for (const x of [0, 3, 11]) {
    const a = painter(t), b = painter(t);
    await a.renderer.render(new Session(items, true), { ...dimensions, scrollX: x });
    await b.renderer.render(new Session(items, false), { ...dimensions, scrollX: x });
    assert.deepEqual(a.stages[0].log, b.stages[0].log);
  }
});

test("offscreen and fully clipped text still controls a shared baseline across sparse pages", async t => {
  const items = [text(0, 0), ...Array.from({ length: 255 }, () => text(200, 0)),
    { kind: "clip", bounds: box(0, 100, 0, 0), childCount: 1 }, text(500, 0, "2")];
  const a = painter(t), b = painter(t), s = new Session(items);
  await a.renderer.render(s, dimensions); await b.renderer.render(new Session(items, false), dimensions);
  assert.deepEqual(a.stages[0].log, b.stages[0].log);
  assert.equal(s.queries.length, 2); assert.equal(a.stages[0].log.find(row => row[0] === "translate")[2], 15);
});

test("legacy sessions use snapshots without probing unadvertised viewport methods", async t => {
  for (const advertised of [false, undefined, "true"]) {
    const s = new Session([text()]); s.supportsViewport = advertised;
    s.viewport = () => assert.fail("not negotiated");
    const { renderer } = painter(t); const frame = await renderer.render(s, dimensions);
    assert.equal(frame.queryMode, "snapshot"); assert.equal(frame.totalItems, 1);
    assert.equal(frame.scannedItems, 1); assert.equal(frame.receivedItems, 1); assert.equal(s.snapshots.length, 1);
  }
});

test("inconsistent advertised support fails before requests and never downgrades", async t => {
  const s = new Session([text()]), { renderer } = painter(t); s.viewport = undefined;
  await assert.rejects(renderer.render(s, dimensions), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(s.snapshots.length, 0);
  s.viewport = () => { throw Object.assign(new Error("old artifact"), { code: "UNSUPPORTED_WASM_PACKAGE" }); };
  await assert.rejects(renderer.render(s, dimensions), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(s.snapshots.length, 0);
});

test("indexed replies are checked independently of a custom session facade", async t => {
  const mutators = [p => p.queryKind = "wrong", p => p.afterIndex++, p => p.viewport.y++, p => p.revision = "2",
    p => p.items[0].effectiveClip.width = -1, p => delete p.items[0].effectiveClip,
    p => p.items[0].index = p.total, p => p.items[0].kind = "clip", p => p.visitedEntries = 0,
    p => p.visitedEntries = p.total + 1, p => p.nextIndex = 0, p => p.total = 1000001,
    p => p.items.push({ ...p.items[0] }), p => p.items[0].bounds.y = NaN];
  for (const mutate of mutators) {
    const { renderer, canvas } = painter(t), s = new Session([vector()]);
    await renderer.render(s, dimensions); const frame = renderer.frame, count = canvas.log.length;
    s.viewport = q => { const p = structuredClone(sparsePage(s, q, [{ ...text(), index: 50 }])); mutate(p); return p; };
    await assert.rejects(renderer.render(s, dimensions), code("INVALID_WASM_RESPONSE"));
    assert.equal(renderer.frame, frame); assert.equal(canvas.log.length, count); assert.equal(s.snapshots.length, 0);
  }
});

test("later-page metadata changes cannot publish a partial frame", async t => {
  for (const mutate of [p => p.total--, p => p.totalBounds.height--]) {
    const { renderer, canvas } = painter(t), s = new Session([vector()]);
    await renderer.render(s, dimensions); const before = renderer.frame, count = canvas.log.length;
    s.viewport = q => {
      if (!q.afterIndex) return sparsePage(s, q, Array.from({ length: 256 }, (_, i) => ({ ...vector(), index: i })),
        { visitedEntries: 256, nextIndex: 256 });
      const p = sparsePage(s, q, [{ ...vector(), index: 257 }]); mutate(p); return p;
    };
    await assert.rejects(renderer.render(s, dimensions), code("INVALID_WASM_RESPONSE"));
    assert.equal(renderer.frame, before); assert.equal(canvas.log.length, count);
  }
});

test("native candidate work is accumulated across pages, not confused with document size", async t => {
  const { renderer, stages } = painter(t, { limits: { maxScannedItems: 300 } }), s = new Session();
  s.viewport = q => !q.afterIndex ? sparsePage(s, q, Array.from({ length: 256 }, (_, i) => ({ ...vector(), index: i })),
    { visitedEntries: 256, nextIndex: 256 }) : sparsePage(s, q, [{ ...vector(), index: 900 }], { visitedEntries: 45 });
  await assert.rejects(renderer.render(s, dimensions), code("BUDGET_EXCEEDED"));
  assert.equal(stages.length, 0); assert.equal(s.snapshots.length, 0);
});

test("visible, glyph, path and pixel budgets still apply on the indexed path", async t => {
  for (const limits of [{ maxVisibleItems: 0 }, { maxGlyphs: 0 }, { maxDrawCommands: 1 }, { maxFrameCommands: 1 }, { maxPixels: 1 }]) {
    const { renderer } = painter(t, { limits });
    await assert.rejects(renderer.render(new Session([text()]), dimensions), code("BUDGET_EXCEEDED"));
    assert.equal(renderer.frame, null);
  }
});

test("an empty viewport paints a valid background while retaining full document bounds", async t => {
  const { renderer, stages } = painter(t), s = new Session();
  s.viewport = q => sparsePage(s, q, [], { visitedEntries: 0 });
  const f = await renderer.render(s, dimensions);
  assert.equal(f.totalItems, 1000000); assert.equal(f.visibleItems, 0); assert.equal(f.scannedItems, 0);
  assert.equal(f.totalBounds.height, 20000000); assert.equal(stages.length, 1); assert.equal(s.outlines.length, 0);
});

test("outward f32 query rounding never widens actual paint clipping or camera coordinates", async t => {
  for (const x of [0.1, -0.1, 16777215.7]) {
    const { renderer, stages } = painter(t), s = new Session();
    const view = { x, y: 0.1, width: 20.1, height: 20.1 };
    s.viewport = q => {
      assert(q.viewport.x <= view.x); assert(q.viewport.y <= view.y);
      assert(q.viewport.x + q.viewport.width >= view.x + view.width);
      assert(q.viewport.y + q.viewport.height >= view.y + view.height);
      return sparsePage(s, q, [{ ...vector(box(x, 0, 50, 50)), index: 42 }]);
    };
    const f = await renderer.render(s, { width: view.width, height: view.height, scrollX: view.x, scrollY: view.y });
    const clip = stages[0].log.find(row => row[0] === "rect");
    assert.deepEqual(clip.slice(0, 3), ["rect", view.x, view.y]);
    // Subtracting large f64 camera coordinates can lose a few low bits; it
    // must not use the much wider outward-rounded f32 query rectangle.
    assert(Math.abs(clip[3] - view.width) <= Math.max(1e-12, Math.abs(view.x) * Number.EPSILON * 2));
    assert.equal(clip[4], view.height);
    assert.equal(f.scrollX, x); assert.deepEqual(renderer.documentPoint(1, 1), { x: x + 1, y: 1.1 });
  }
});

test("a late reflow fences an asynchronous indexed page without changing prior pixels", async t => {
  const { renderer, canvas } = painter(t), s = new Session([vector()]);
  await renderer.render(s, dimensions); const before = renderer.frame, count = canvas.log.length, gate = deferred();
  const old = s.viewport.bind(s); s.viewport = async q => { const page = old(q); await gate.promise; return page; };
  const pending = renderer.render(s, dimensions); await tick(); s.reflow(); gate.resolve();
  await assert.rejects(pending, code("STALE_LAYOUT")); assert.equal(renderer.frame, before); assert.equal(canvas.log.length, count);
});

test("aborted indexed waits never forward worker cancellation and observe late failures", async t => {
  const { renderer } = painter(t), s = new Session(), gate = deferred(), controller = new AbortController();
  let called = 0;
  s.viewport = (...args) => { called++; assert.equal(args.length, 1); assert(!("signal" in args[0])); return gate.promise; };
  const pending = renderer.render(s, { ...dimensions, signal: controller.signal });
  await tick(); controller.abort(); await assert.rejects(pending, code("ABORTED"));
  gate.reject(new Error("late native failure")); await tick();
  assert.equal(called, 1); assert.equal(s.disposed, false); assert.equal(s.snapshots.length, 0);
});

test("superseding, clearing and disposing revoke a pending indexed paint", async t => {
  for (const action of ["supersede", "clear", "dispose"]) {
    const { renderer } = painter(t), old = new Session(), gate = deferred(); old.viewport = () => gate.promise;
    const pending = renderer.render(old, dimensions), refused = assert.rejects(pending,
      code(action === "dispose" ? "RENDERER_DISPOSED" : "RENDER_SUPERSEDED"));
    await tick();
    if (action === "supersede") await renderer.render(new Session([vector()]), dimensions); else renderer[action]();
    await refused; gate.reject(new Error("late")); await tick(); assert.equal(old.disposed, false);
  }
});

test("images preserve exact occurrence identity and host-owned resolution on indexed pages", async t => {
  const { renderer } = painter(t), s = new Session([image(), { ...image(box(20, 0)), isResolved: false }]);
  const borrowed = {}, calls = [];
  const f = await renderer.render(s, { ...dimensions, resolveImage: (info, token) => {
    calls.push({ info, token }); return borrowed;
  } });
  assert.equal(calls.length, 1); assert.equal(calls[0].info.requestId, "9007199254740993");
  assert.equal(f.missingImages, 1); assert.deepEqual(calls[0].token, s.token);
});

test("actual direct adapter and outline facade drive the indexed renderer", async t => {
  const wire = new Session([text()]); let freed = false;
  const raw = { revision: "1", layoutRevision: "1", free() { freed = true; },
    snapshotJson() { assert.fail("adapter must not scan snapshots"); },
    viewportJson(revision, layoutRevision, x, y, width, height, afterIndex, limit, glyphs) {
      assert.deepEqual({ revision, layoutRevision }, wire.token);
      return JSON.stringify(wire.viewport({ viewport: box(x, y, width, height), afterIndex, limit, glyphs }));
    } };
  const s = withGlyphOutlines(createFlowAdapter(raw), (font, ids) => JSON.stringify(paths(font, [...ids])));
  const { renderer } = painter(t); const f = await renderer.render(s, dimensions);
  assert.equal(f.queryMode, "indexed-viewport"); assert.equal(f.glyphs, 1);
  renderer.dispose(); assert.equal(freed, false); s.dispose(); assert.equal(freed, true);
});

test("seeded mixed-primitive scenes have identical indexed and dense drawing commands", async t => {
  let state = 0x12345678;
  const rand = n => { state = (Math.imul(state, 1664525) + 1013904223) >>> 0; return state % n; };
  for (let round = 0; round < 30; round++) {
    const items = [];
    for (let i = 0; i < 600; i++) {
      const b = box(rand(400) - 200, rand(2000) - 500, 10 + rand(100), 20);
      const kind = rand(8);
      if (kind === 0) items.push({ kind: "clip", bounds: b, childCount: Math.min(rand(30), 599 - i) });
      else if (kind === 1) items.push({ kind: "anchor", bounds: b });
      else if (kind < 4) items.push(text(b.x, b.y, kind === 2 ? "1" : "2"));
      else if (kind === 4) items.push({ ...image(b), isResolved: false });
      else items.push(vector(b));
    }
    const a = painter(t), b = painter(t), view = { ...dimensions, scrollX: rand(100), scrollY: rand(1500) };
    await a.renderer.render(new Session(items), view); await b.renderer.render(new Session(items, false), view);
    assert.deepEqual(a.stages[0].log, b.stages[0].log, `scene ${round}`);
  }
});
