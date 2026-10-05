// Recording-Canvas unit tests, not a browser raster or generated-WASM proof.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowCanvasRenderer } from "./flow-canvas.js";

const palette = {
  "tok-kw": "#cf222e", "tok-ty": "#953800", "tok-fn": "#8250df",
  "tok-st": "#0a3069", "tok-nu": "#0550ae", "tok-cm": "#6e7781",
  "tok-op": "#1f2328", "tok-pn": "#1f2328",
};
const bounds = (x = 0, width = 10) => ({ x, y: 0, width, height: 20 });
function surface(width = 1, height = 1) {
  const stack = [];
  const context = {
    fills: [], blits: [], x: 0, y: 0,
    save() { stack.push([this.fillStyle, this.strokeStyle, this.x, this.y]); },
    restore() { [this.fillStyle, this.strokeStyle, this.x, this.y] = stack.pop(); },
    translate(x, y) { this.x += x; this.y += y; },
    fill(rule) { this.fills.push({ color: this.fillStyle, x: this.x, y: this.y, rule }); },
    drawImage(...args) { this.blits.push(args); },
    beginPath() {}, moveTo() {}, lineTo() {}, closePath() {}, quadraticCurveTo() {},
    rect() {}, clip() {}, setTransform() {}, scale() {}, fillRect() {},
    strokeRect() {}, stroke() {},
  };
  return { width, height, context, getContext: () => context };
}
function text(index, colorRole) {
  return {
    kind: "text", index, colorRole, text: "x", fontSize: 13,
    bounds: bounds(index * 10),
    fontRun: {
      coordinateSpace: "fragment-utf8-and-utf16", fontOrigin: "bundled",
      direction: "ltr", fontId: "1", fontSize: 13, glyphCount: 1, clusterCount: 1,
      clusters: [{ index: 0, glyphs: [0, 1], xStart: 0, xEnd: 10, fontId: "1" }],
      glyphs: [{ glyphId: 1, fontId: "1", clusterIndex: 0,
        xAdvance: 10, yAdvance: 0, xOffset: 0, yOffset: 0 }],
    },
  };
}
function session(roles) {
  return {
    token: { revision: "1", layoutRevision: "1" }, disposed: false,
    layoutOptions: { viewportWidth: 200 }, items: roles.map((role, i) => text(i, role)),
    outlineCalls: 0,
    snapshot({ offset, limit }) {
      const items = this.items.slice(offset, offset + limit);
      return {
        schemaVersion: 1, ...this.token, offset, total: this.items.length, items,
        nextOffset: offset + items.length < this.items.length ? offset + items.length : null,
        totalBounds: bounds(0, this.items.length * 10), shapingProfile: "bundled-simple-ltr",
      };
    },
    glyphOutlines(fontId, requested) {
      this.outlineCalls++;
      return {
        schemaVersion: 1, fontId, unitsPerEm: 1000, ascent: 800, descent: -200, lineGap: 0,
        glyphs: requested.map(glyphId => ({
          glyphId, commands: [["M", 0, 0], ["L", 500, 0], ["L", 0, 500], ["Z"]],
        })),
      };
    },
  };
}
function harness(options = {}) {
  const target = surface(), stages = [];
  const renderer = new FlowCanvasRenderer(target, {
    ...options,
    canvasFactory(width, height) {
      const stage = surface(width, height);
      stages.push(stage);
      return stage;
    },
  });
  return { target, renderer, stages, fills: () => stages.at(-1).context.fills };
}
const paint = { width: 200, height: 40 };
const error = code => value => value?.code === code;

test("all shared lexer roles paint glyph paths without moving their positions", async () => {
  const roles = [...Object.keys(palette), "code", "unknown-role", "__proto__"];
  const colored = harness(), plain = harness();
  const result = await colored.renderer.render(session(roles), paint);
  await plain.renderer.render(session(roles.map(() => "code")), paint);
  assert.equal(result.glyphs, roles.length);
  assert.deepEqual(colored.fills().map(fill => fill.color), [
    ...Object.values(palette), "#1f2328", "#1f2328", "#1f2328",
  ]);
  assert.deepEqual(colored.fills().map(({ x, y }) => [x, y]),
    plain.fills().map(({ x, y }) => [x, y]));
  assert.equal(colored.target.context.blits.length, 1);
  assert(colored.fills().every(fill => fill.rule === "nonzero"));
});

test("every syntax role accepts an explicit host palette override", async () => {
  const colors = Object.fromEntries(Object.keys(palette).map((role, i) => [role, `#12345${i}`]));
  const host = harness({ colors });
  await host.renderer.render(session(Object.keys(colors)), paint);
  assert.deepEqual(host.fills().map(fill => fill.color), Object.values(colors));
  assert.throws(() => harness({ colors: { "tok-missing": "red" } }), error("INVALID_OPTIONS"));
  assert.throws(() => harness({ colors: { "tok-kw": 42 } }), error("INVALID_OPTIONS"));
  assert.throws(() => harness({ colors: { "tok-kw": "x".repeat(129) } }), error("INVALID_OPTIONS"));
  assert.throws(() => harness({ colors: JSON.parse('{"__proto__":"red"}') }), error("INVALID_OPTIONS"));
});

test("different token colors reuse one glyph cache without retaining stale colors", async () => {
  const host = harness(), source = session(["tok-kw", "tok-st"]);
  await host.renderer.render(source, paint);
  const before = host.fills().map(({ x, y }) => [x, y]);
  assert.equal(source.outlineCalls, 1);
  source.items = [text(0, "tok-cm"), text(1, "tok-nu")];
  await host.renderer.render(source, paint);
  assert.equal(source.outlineCalls, 1);
  assert.deepEqual(host.fills().map(fill => fill.color), [palette["tok-cm"], palette["tok-nu"]]);
  assert.deepEqual(host.fills().map(({ x, y }) => [x, y]), before);
  assert.equal(host.renderer.cacheStats.glyphs, 1);
});

test("fragment item budgets still preserve the previously published frame on failure", async () => {
  const host = harness({ limits: { maxVisibleItems: 1 } });
  const source = session(["tok-kw"]);
  const previous = await host.renderer.render(source, paint);
  source.items.push(text(1, "tok-st"));
  await assert.rejects(host.renderer.render(source, paint), error("BUDGET_EXCEEDED"));
  assert.equal(host.renderer.frame, previous);
  assert.equal(host.target.context.blits.length, 1);
  assert.equal(host.stages.length, 1);
});

test("syntax colors cannot bypass glyph ownership validation", async () => {
  const host = harness(), source = session(["tok-kw"]);
  source.items[0].fontRun.glyphs[0].clusterIndex = 9;
  await assert.rejects(host.renderer.render(source, paint), error("INVALID_WASM_RESPONSE"));
  assert.equal(host.renderer.frame, null);
  assert.equal(host.target.context.blits.length, 0);
});
