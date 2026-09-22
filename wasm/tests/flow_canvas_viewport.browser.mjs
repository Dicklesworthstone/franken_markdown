// Actual Chromium Canvas rasterization and event-loop scheduling. The native
// provider and glyph contours are explicitly synthetic, not generated WASM.
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { Session, box, text, vector, image, paths } from "./flow_canvas_viewport_fixtures.mjs";
const results = [];
const assert = (condition, message = "assertion failed") => { if (!condition) throw new Error(message); };
const equal = (a, b) => assert(JSON.stringify(a) === JSON.stringify(b), `${JSON.stringify(a)} != ${JSON.stringify(b)}`);
const pixels = canvas => canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
const samePixels = (a, b) => { assert(a.length === b.length); for (let i = 0; i < a.length; i++) assert(a[i] === b[i], `pixel byte ${i}: ${a[i]} != ${b[i]}`); };
const pixel = (canvas, x, y) => [...canvas.getContext("2d").getImageData(x, y, 1, 1).data];
async function test(name, fn) {
  try { await fn(); results.push({ name, pass: true }); }
  catch (error) { results.push({ name, pass: false, error: String(error.stack ?? error) }); }
}
async function rejects(pending, expected) {
  try { await pending; } catch (error) { equal(error.code, expected); return; }
  throw new Error(`expected ${expected}`);
}
function painter(onStage = () => {}) {
  const canvas = document.createElement("canvas"), stages = [];
  const renderer = new FlowCanvasRenderer(canvas, { canvasFactory: (width, height) => {
    const stage = new OffscreenCanvas(width, height); stages.push(stage); onStage(stage); return stage;
  } });
  return { canvas, stages, renderer };
}
const size = { width: 100, height: 60 };
function complexSession() {
  const s = new Session([text()]);
  s.glyphOutlines = (id, ids) => {
    const result = paths(id, ids);
    for (const glyph of result.glyphs) glyph.commands = [
      ["M", 0, 0], ...Array.from({ length: 65534 }, (_, i) => ["L", i % 10, (i >>> 1) % 10]), ["Z"]
    ];
    return result;
  };
  return s;
}

await test("indexed and legacy pixels agree for clipping, images, selection, scroll and pixel ratio", async () => {
  const borrowed = new OffscreenCanvas(2, 2), c = borrowed.getContext("2d");
  c.fillStyle = "#00ff00"; c.fillRect(0, 0, 2, 2);
  const items = Array.from({ length: 254 }, () => ({ kind: "anchor", bounds: box() }));
  items.push({ kind: "clip", bounds: box(0, 0, 10, 20), childCount: 3 },
    { kind: "clip", bounds: box(0, 0, 5, 20), childCount: 1 },
    vector(box(0, 0, 20, 10)), vector(box(0, 10, 20, 10), "accent"), vector(box(0, 20, 20, 10), "link"),
    text(40, 0), text(240, 0, "2"), image(box(35, 35, 15, 10)));
  for (const scrollX of [0, 3, 11, 200]) for (const pixelRatio of [1, 2]) {
    const a = painter(), b = painter();
    try {
      const indexed = new Session(items), legacy = new Session(items, false);
      const options = { ...size, scrollX, pixelRatio, resolveImage: () => borrowed,
        selection: { ...indexed.token, rectangles: [box(40, 0, 10, 20)] } };
      const frame = await a.renderer.render(indexed, options); await b.renderer.render(legacy, options);
      equal(frame.queryMode, "indexed-viewport"); equal(indexed.snapshots.length, 0);
      samePixels(pixels(a.canvas), pixels(b.canvas));
      if (!scrollX && pixelRatio === 1) equal(pixel(a.canvas, 40, 40), [0, 255, 0, 255]);
    } finally { a.renderer.dispose(); b.renderer.dispose(); }
  }
});

await test("offscreen and fully clipped mixed faces preserve pixel baselines across spatial pages", async () => {
  const items = [text(0, 0), ...Array.from({ length: 255 }, () => text(200, 0)),
    { kind: "clip", bounds: box(0, 100, 0, 0), childCount: 1 }, text(500, 0, "2")];
  const a = painter(), b = painter();
  try {
    const indexed = new Session(items);
    await a.renderer.render(indexed, size); await b.renderer.render(new Session(items, false), size);
    equal(indexed.queries.length, 2); samePixels(pixels(a.canvas), pixels(b.canvas));
    equal(pixel(a.canvas, 5, 14), [31, 35, 40, 255]); equal(pixel(a.canvas, 5, 16), [255, 255, 255, 255]);
  } finally { a.renderer.dispose(); b.renderer.dispose(); }
});

await test("DOM input can abort a warm cached contour while prior target pixels stay intact", async () => {
  let interrupt = false, timer;
  const control = new AbortController(), button = document.createElement("button");
  button.addEventListener("click", () => control.abort());
  const f = painter(() => { if (interrupt) timer = setTimeout(() => button.click(), 0); });
  try {
    const session = complexSession(); await f.renderer.render(session, size);
    const before = new Uint8ClampedArray(pixels(f.canvas)), frame = f.renderer.frame;
    interrupt = true;
    await rejects(f.renderer.render(session, { ...size, signal: control.signal }), "ABORTED");
    samePixels(pixels(f.canvas), before); assert(f.renderer.frame === frame); assert(!session.disposed);
  } finally { clearTimeout(timer); f.renderer.dispose(); }
});

await test("new paint replaces a suspended contour without late pixel resurrection", async () => {
  let replace = false, replacement, timer;
  const next = new Session([vector(box(0, 0, 20, 20), "link")]);
  next.token = { revision: "2", layoutRevision: "9" };
  const f = painter(() => {
    if (replace) { replace = false; timer = setTimeout(() => { replacement = f.renderer.render(next, size); }, 0); }
  });
  try {
    const old = complexSession(); await f.renderer.render(old, size); replace = true;
    await rejects(f.renderer.render(old, size), "RENDER_SUPERSEDED"); await replacement;
    equal(f.renderer.frame.revision, "2"); equal(pixel(f.canvas, 5, 5), [9, 105, 218, 255]);
    await new Promise(resolve => setTimeout(resolve, 5));
    equal(pixel(f.canvas, 5, 5), [9, 105, 218, 255]); assert(!old.disposed);
  } finally { clearTimeout(timer); f.renderer.dispose(); }
});

await test("cooperative paths preserve quadratic curves and nonzero-winding counters", async () => {
  const patterns = [
    [["M", 0, 0], ["L", 10, 0], ["L", 10, 10], ["L", 0, 10], ["Z"],
      ["M", 3, 3], ["L", 3, 7], ["L", 7, 7], ["L", 7, 3], ["Z"]],
    [["M", 0, 0], ["Q", 10, 20, 20, 0], ["L", 0, 0], ["Z"]]
  ];
  for (const pattern of patterns) {
    const f = painter(), reference = document.createElement("canvas");
    const commands = Array.from({ length: 1200 }, () => pattern).flat();
    const session = new Session([text()]);
    session.glyphOutlines = (id, ids) => {
      const result = paths(id, ids);
      for (const glyph of result.glyphs) glyph.commands = commands;
      return result;
    };
    try {
      // Same contour stream drawn synchronously by a direct Canvas reference.
      // Repeating curves can change browser edge antialiasing relative to one
      // contour, so compare identical geometry rather than different paths.
      reference.width = size.width; reference.height = size.height;
      const c = reference.getContext("2d");
      c.fillStyle = "#ffffff"; c.fillRect(0, 0, size.width, size.height);
      c.beginPath(); c.rect(0, 0, size.width, size.height); c.clip();
      c.fillStyle = "#1f2328"; c.translate(0, 13); c.scale(1, -1); c.beginPath();
      for (const [op, a, b, d, e] of commands) {
        if (op === "M") c.moveTo(a, b);
        else if (op === "L") c.lineTo(a, b);
        else if (op === "Q") c.quadraticCurveTo(a, b, d, e);
        else c.closePath();
      }
      c.fill("nonzero");
      await f.renderer.render(session, size); samePixels(pixels(f.canvas), pixels(reference));
      if (pattern.length === 10) {
        equal(pixel(f.canvas, 1, 8), [31, 35, 40, 255]); equal(pixel(f.canvas, 5, 8), [255, 255, 255, 255]);
      } else {
        equal(pixel(f.canvas, 10, 5), [31, 35, 40, 255]); equal(pixel(f.canvas, 1, 5), [255, 255, 255, 255]);
      }
    } finally { f.renderer.dispose(); }
  }
});

const report = { passed: results.filter(r => r.pass).length, failed: results.filter(r => !r.pass).length, results };
const output = document.querySelector("#result"); output.textContent = JSON.stringify(report);
output.dataset.status = report.failed ? "failed" : "passed";
