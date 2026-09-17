// Real Chromium Canvas pixels; deterministic synthetic outlines isolate the
// presentation contract. This does NOT claim Rust parsing or font-decoder proof.
import { FlowCanvasRenderer } from "../flow-canvas.js";

const results = [];
const equal = (a, b, message = "values differ") => { if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${message}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`); };
const assert = (condition, message = "assertion failed") => { if (!condition) throw new Error(message); };
const rejects = async (promise, code) => {
  try { await promise; } catch (error) { if (code) equal(error.code, code); return; }
  throw new Error(`expected rejection ${code ?? ""}`);
};
async function test(name, f) { try { await f(); results.push({ name, pass: true }); } catch (e) { results.push({ name, pass: false, error: String(e.stack ?? e) }); } }
const box = (x = 0, y = 0, width = 100, height = 20) => ({ x, y, width, height });
function text(x = 0, y = 0, specs = [{ id: 1, advance: 10, font: "1" }], fontId = "1") {
  let cursor = 0;
  return { kind: "text", bounds: box(x, y, specs.reduce((s, g) => s + g.advance, 0), 20),
    text: "x".repeat(specs.length), fontSize: 10, colorRole: "text", enclosingSourceSpan: { startByte: 0, endByte: 1 },
    fontRun: { coordinateSpace: "fragment-utf8-and-utf16", fontId, fontSize: 10,
      direction: "ltr", fontOrigin: "bundled", script: "DFLT", language: "dflt",
      totalAdvance: specs.reduce((s, g) => s + g.advance, 0), glyphCount: specs.length, clusterCount: specs.length,
      clusters: specs.map((g, i) => { const start = cursor; cursor += g.advance;
        return { index: i, bytes: [i, i + 1], utf16: [i, i + 1], glyphs: [i, i + 1],
          xStart: start, xEnd: cursor, fontId: g.font ?? fontId }; }),
      glyphs: specs.map((g, i) => ({ glyphId: g.id, clusterIndex: i, fontId: g.font ?? fontId,
        xAdvance: g.advance, yAdvance: 0, xOffset: g.dx ?? 0, yOffset: g.dy ?? 0 })) } };
}
function paths(fontId, ids) {
  const unit = fontId === "2" ? 20 : 10;
  return { schemaVersion: 1, fontId, unitsPerEm: unit, ascent: fontId === "2" ? 24 : 8,
    descent: fontId === "2" ? -4 : -2, lineGap: 0,
    glyphs: ids.map(glyphId => ({ glyphId, commands: glyphId === 4 ? [] : glyphId === 3
      ? [["M", 0, 0], ["Q", 10, 20, 20, 0], ["L", 0, 0], ["Z"]]
      : [["M", 0, 0], ["L", unit, 0], ["L", unit, unit], ["L", 0, unit], ["Z"],
        ...(glyphId === 2 ? [["M", 3, 3], ["L", 3, 7], ["L", 7, 7], ["L", 7, 3], ["Z"]] : [])] })) };
}
function session(items) {
  const s = { token: { revision: "1", layoutRevision: "1" }, disposed: false,
    layoutOptions: { viewportWidth: 100, bodySize: 10, codeSize: 10, lineHeight: 20 }, calls: [],
    snapshot({ offset, limit, glyphs, token }) {
      assert(glyphs, "renderer must request glyphs"); equal(token, s.token, "stale snapshot request");
      return { schemaVersion: 1, ...s.token, offset, total: items.length,
        nextOffset: offset + limit < items.length ? offset + limit : null,
        totalBounds: box(0, 0, 500, 500), shapingProfile: "bundled-simple-ltr",
        items: structuredClone(items.slice(offset, offset + limit).map((item, i) => ({ ...item, index: offset + i }))) };
    },
    glyphOutlines(id, ids) { s.calls.push([id, [...ids]]); return paths(id, ids); },
    hitTest(x, y, shown) { equal(shown.revision, s.token.revision); equal(shown.layoutRevision, s.token.layoutRevision);
      return { schemaVersion: 1, ...s.token, linkTarget: "#target", hit: { x, y } }; }
  };
  return s;
}
function painter(options = {}) {
  const canvas = document.createElement("canvas"); document.body.append(canvas);
  const renderer = new FlowCanvasRenderer(canvas, { colors: { text: "#000000", heading: "#000000", accent: "#00ff00", link: "#ff0000" }, ...options });
  return { canvas, renderer };
}
const pixel = (c, x, y) => [...c.getContext("2d").getImageData(x, y, 1, 1).data];
const black = [0, 0, 0, 255], white = [255, 255, 255, 255];
const dimensions = { width: 100, height: 60 };
const vector = (bounds, color = "text") => ({ kind: "vector", bounds, shape: "horizontal-rule", strokeWidth: 1, colorRole: color });
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
const originalFill = CanvasRenderingContext2D.prototype.fillText;
const originalMeasure = CanvasRenderingContext2D.prototype.measureText;
CanvasRenderingContext2D.prototype.fillText = () => { throw new Error("browser text shaping is forbidden in exact-glyph rendering"); };
CanvasRenderingContext2D.prototype.measureText = () => { throw new Error("browser measurement is forbidden"); };
try {
  await test("font-space paths flip Y around a metric-derived baseline", async () => {
    const { canvas, renderer } = painter(); const s = session([text(5, 5)]);
    await renderer.render(s, dimensions);
    equal(pixel(canvas, 7, 10), black); equal(pixel(canvas, 7, 20), white);
    equal(pixel(canvas, 4, 10), white); equal(s.calls, [["1", [1]]]);
  });
  await test("quadratic contours render actual curves without fillText", async () => {
    const { canvas, renderer } = painter();
    await renderer.render(session([text(0, 0, [{ id: 3, advance: 20 }])]), dimensions);
    equal(pixel(canvas, 10, 5), black); equal(pixel(canvas, 1, 5), white);
  });
  await test("nonzero fill preserves counters in glyph contours", async () => {
    const { canvas, renderer } = painter();
    await renderer.render(session([text(0, 0, [{ id: 2, advance: 10 }])]), dimensions);
    equal(pixel(canvas, 1, 8), black); equal(pixel(canvas, 5, 8), white);
  });
  await test("shaped spaces, advances and offsets place ink, not nominal outline widths", async () => {
    const { canvas, renderer } = painter();
    await renderer.render(session([text(0, 0, [{ id: 4, advance: 5 }, { id: 1, advance: 7, dx: 2, dy: 1 }])]), dimensions);
    equal(pixel(canvas, 5, 6), white); equal(pixel(canvas, 8, 6), black); equal(pixel(canvas, 8, 13), white);
  });
  await test("mixed face units share one baseline even across snapshot pages", async () => {
    const anchors = Array.from({ length: 255 }, () => ({ kind: "anchor", bounds: box() }));
    const { canvas, renderer } = painter();
    await renderer.render(session([...anchors, text(0, 0), text(20, 0, [{ id: 1, advance: 10, font: "2" }], "2")]), dimensions);
    equal(pixel(canvas, 5, 6), black); equal(pixel(canvas, 25, 6), black);
    equal(pixel(canvas, 5, 14), black); equal(pixel(canvas, 25, 14), black);
    equal(pixel(canvas, 5, 16), white); equal(pixel(canvas, 25, 16), white);
  });
  await test("page-spanning nested clips expire at their declared item indices", async () => {
    const items = Array.from({ length: 254 }, () => ({ kind: "anchor", bounds: box() }));
    items.push({ kind: "clip", bounds: box(0, 0, 10, 20), childCount: 3 },
      { kind: "clip", bounds: box(0, 0, 5, 20), childCount: 1 },
      vector(box(0, 0, 20, 10)), vector(box(0, 10, 20, 10), "accent"), vector(box(0, 20, 20, 10), "link"));
    const { canvas, renderer } = painter(); await renderer.render(session(items), dimensions);
    equal(pixel(canvas, 2, 5), black); equal(pixel(canvas, 7, 5), white);
    equal(pixel(canvas, 7, 15), [0, 255, 0, 255]); equal(pixel(canvas, 15, 15), white);
    equal(pixel(canvas, 15, 25), [255, 0, 0, 255]);
  });
  await test("scroll and device-pixel transforms match hit-test coordinates", async () => {
    const { canvas, renderer } = painter(); const s = session([text(100, 100)]);
    const frame = await renderer.render(s, { width: 20, height: 20, scrollX: 100, scrollY: 100, pixelRatio: 2 });
    equal([canvas.width, canvas.height], [40, 40]); equal(pixel(canvas, 4, 10), black);
    equal(renderer.documentPoint(2, 3), { x: 102, y: 103 }); equal(renderer.documentPoint(20, 3), null);
    equal((await renderer.hitTest(2, 3)).hit, { x: 102, y: 103 }); equal(frame.pixelRatio, 2);
  });
  await test("warm paths survive caller response mutation and obey LRU limits", async () => {
    const { canvas, renderer } = painter({ limits: { maxCachedGlyphs: 1, maxCachedCommands: 10 } });
    const s = session([text()]); let response;
    s.glyphOutlines = (id, ids) => { s.calls.push([id, ids]); response = paths(id, ids); return response; };
    await renderer.render(s, dimensions); response.glyphs[0].commands[1][1] = 0;
    await renderer.render(s, dimensions); equal(s.calls.length, 1); equal(pixel(canvas, 5, 8), black);
    await renderer.render(session([text(0, 0, [{ id: 2, advance: 10 }])]), dimensions);
    equal(renderer.cacheStats.glyphs, 1); assert(renderer.cacheStats.commands <= 10);
  });
  await test("invalid path data preserves the last displayed frame and pixels", async () => {
    const { canvas, renderer } = painter(); await renderer.render(session([text()]), dimensions);
    const before = renderer.frame, s = session([text(0, 0, [{ id: 3, advance: 20 }])]);
    s.glyphOutlines = (id, ids) => { const result = paths(id, ids); result.glyphs[0].commands[1][1] = NaN; return result; };
    await rejects(renderer.render(s, dimensions), "INVALID_WASM_RESPONSE");
    assert(renderer.frame === before); equal(pixel(canvas, 5, 8), black);
  });
  await test("pixel, visible-glyph and command budgets refuse before publication", async () => {
    for (const limits of [{ maxPixels: 1 }, { maxGlyphs: 0 }, { maxDrawCommands: 1 }, { maxFrameCommands: 1 }, { maxVisibleItems: 0 }]) {
      const { canvas, renderer } = painter({ limits }); canvas.width = 3; canvas.height = 3;
      const c = canvas.getContext("2d"); c.fillStyle = "#00ff00"; c.fillRect(0, 0, 3, 3);
      await rejects(renderer.render(session([text()]), dimensions), "BUDGET_EXCEEDED");
      equal(pixel(canvas, 1, 1), [0, 255, 0, 255]); equal([canvas.width, canvas.height], [3, 3]);
    }
  });
  await test("stale asynchronous outline completion never replaces current pixels", async () => {
    const { canvas, renderer } = painter(); await renderer.render(session([text()]), dimensions);
    const before = renderer.frame, s = session([text(30, 0, [{ id: 3, advance: 20 }])]), d = deferred();
    s.glyphOutlines = () => d.promise;
    const paint = renderer.render(s, dimensions); await tick(); s.token.layoutRevision = "2";
    d.resolve(paths("1", [3])); await rejects(paint, "STALE_LAYOUT");
    assert(renderer.frame === before); equal(pixel(canvas, 5, 8), black);
  });
  await test("latest paint wins without killing the shared editing session", async () => {
    const { canvas, renderer } = painter(); const old = session([text()]), d = deferred();
    old.glyphOutlines = () => d.promise;
    const first = renderer.render(old, dimensions).catch(e => e.code); await tick();
    const next = session([vector(box(0, 0, 50, 50), "accent")]);
    await renderer.render(next, dimensions); equal(await first, "RENDER_SUPERSEDED");
    d.resolve(paths("1", [1])); await tick(); equal(pixel(canvas, 5, 8), [0, 255, 0, 255]);
    assert(!old.disposed && !next.disposed);
  });
  await test("paint abort settles promptly without passing cancellation into the worker", async () => {
    const { renderer } = painter(); const s = session([text()]), d = deferred(), control = new AbortController();
    s.glyphOutlines = () => d.promise;
    const paint = renderer.render(s, { ...dimensions, signal: control.signal }); await tick(); control.abort();
    await rejects(paint, "ABORTED"); assert(!s.disposed); d.resolve(paths("1", [1]));
  });
  await test("images are host-resolved only and caller-owned sources are not destroyed", async () => {
    const { canvas, renderer } = painter(); const image = new OffscreenCanvas(2, 2);
    const c = image.getContext("2d"); c.fillStyle = "#00ff00"; c.fillRect(0, 0, 2, 2);
    const items = [{ kind: "image", bounds: box(0, 0, 20, 20), requestId: "1", destination: "never-fetch://secret", altText: "private", isResolved: true },
      { kind: "image", bounds: box(30, 0, 20, 20), requestId: "2", destination: "never-fetch://other", altText: "other", isResolved: false }];
    let calls = 0; const previousFetch = globalThis.fetch; globalThis.fetch = () => { throw new Error("unexpected fetch"); };
    try { const frame = await renderer.render(session(items), { ...dimensions, resolveImage: (info, token) => {
      calls++; equal(info.requestId, "1"); equal(token.revision, "1"); return image;
    } }); equal(calls, 1); equal(frame.missingImages, 1); equal(pixel(canvas, 5, 5), [0, 255, 0, 255]); }
    finally { globalThis.fetch = previousFetch; }
    renderer.dispose(); equal(image.width, 2);
  });
  await test("image-resolution failures keep the old frame intact", async () => {
    const { canvas, renderer } = painter(); await renderer.render(session([text()]), dimensions);
    const s = session([{ kind: "image", bounds: box(), requestId: "1", destination: "x", altText: "x", isResolved: true }]);
    await rejects(renderer.render(s, { ...dimensions, resolveImage: async () => { throw new Error("denied"); } }));
    equal(pixel(canvas, 5, 8), black);
  });
  await test("selection overlays require the displayed token and remain behind glyph ink", async () => {
    const { canvas, renderer } = painter(); const s = session([text()]);
    await renderer.render(s, { ...dimensions, selection: { ...s.token, rectangles: [box(0, 0, 20, 20)] } });
    equal(pixel(canvas, 5, 8), black); equal(pixel(canvas, 15, 8), [200, 225, 255, 255]);
    await rejects(renderer.render(s, { ...dimensions, selection: { revision: "0", layoutRevision: "1", rectangles: [] } }), "STALE_LAYOUT");
  });
  await test("clear and disposal remove revoked pixels and never own the session", async () => {
    const { canvas, renderer } = painter(); const s = session([text()]); await renderer.render(s, dimensions);
    renderer.clear(); equal(renderer.frame, null); equal(pixel(canvas, 5, 8), [0, 0, 0, 0]);
    await renderer.render(s, dimensions); renderer.dispose(); renderer.dispose();
    equal(pixel(canvas, 5, 8), [0, 0, 0, 0]); assert(!s.disposed);
    await rejects(renderer.render(s, dimensions), "RENDERER_DISPOSED");
  });
  await test("OffscreenCanvas targets and an empty document present correctly", async () => {
    const canvas = new OffscreenCanvas(100, 60), renderer = new FlowCanvasRenderer(canvas, { colors: { text: "#000" } });
    await renderer.render(session([text()]), dimensions); equal(pixel(canvas, 5, 8), black);
    const frame = await renderer.render(session([]), dimensions); equal(frame.visibleItems, 0); equal(pixel(canvas, 5, 8), white);
  });
  await test("unsupported primitives and missing glyph arrays fail rather than approximate", async () => {
    const { renderer } = painter(); const t = text(); delete t.fontRun.glyphs;
    await rejects(renderer.render(session([t]), dimensions), "INVALID_WASM_RESPONSE");
    await rejects(renderer.render(session([{ ...vector(box()), shape: "diagram-arrow" }]), dimensions), "UNSUPPORTED_DISPLAY_ITEM");
  });
  await test("bad staging factories cannot draw over the target before validation", async () => {
    const canvas = document.createElement("canvas"), renderer = new FlowCanvasRenderer(canvas, { canvasFactory: () => canvas });
    await rejects(renderer.render(session([]), dimensions), "INVALID_OPTIONS");
  });
} finally {
  CanvasRenderingContext2D.prototype.fillText = originalFill;
  CanvasRenderingContext2D.prototype.measureText = originalMeasure;
}
const target = document.querySelector("#result");
target.textContent = JSON.stringify({ passed: results.filter(r => r.pass).length, failed: results.filter(r => !r.pass).length, results });
target.dataset.status = results.every(r => r.pass) ? "pass" : "fail";
