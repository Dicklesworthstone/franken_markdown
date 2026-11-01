// Real browser pixels/selection over unchanged BrowserFlowSession JSON emitted
// by tests/flow_math_display.rs. The optional runtime also executes the public
// Flow factory over actual matching generated WASM, without a browser TeX parser.
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { FlowReaderView, readFlowDocument, sourceSpanToUtf16 } from "../flow-reader.js";
import { FlowError } from "../flow_session.mjs";

const assert = (value, message = "assertion failed") => { if (!value) throw new Error(message); };
const equal = (a, b, message = "values differ") => assert(JSON.stringify(a) === JSON.stringify(b), message);
async function rejects(promise, code) {
  try { await promise; } catch (error) { equal(error.code, code, `expected ${code}, got ${error.code}`); return; }
  throw new Error(`expected ${code}`);
}
function deferred() {
  let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve };
}
function recordedSession(initial, indexed = true) {
  let edition = initial;
  return {
    disposed: false, supportsViewport: indexed, fontRequests: 0,
    get token() { return { revision: edition.snapshot.revision, layoutRevision: edition.snapshot.layoutRevision }; },
    get layoutOptions() { return edition.snapshot.options; },
    get source() { return edition.source; },
    adopt(next) { edition = next; },
    snapshot(query) {
      equal(query.token, this.token, "snapshot revision fence");
      assert(query.offset === 0 && query.limit >= edition.snapshot.total && query.glyphs,
        "native fixture must fit one unchanged Canvas page");
      return structuredClone(edition.snapshot);
    },
    viewport(query) {
      equal(query.token, this.token, "viewport revision fence");
      equal({ ...query.viewport, afterIndex: query.afterIndex, limit: query.limit, glyphs: query.glyphs },
        edition.viewportRequest, "Canvas query must match actual native viewport request");
      return structuredClone(edition.viewport);
    },
    readingOrder(query) {
      equal(query.token, this.token, "reading revision fence");
      assert(query.offset === 0 && query.limit >= edition.reading.total, "native reading fixture must fit one page");
      return structuredClone(edition.reading);
    },
    glyphOutlines() { this.fontRequests++; throw Error("math paths must not request browser glyph outlines"); },
    hitTest(x, y, shown) {
      if (shown.revision !== this.token.revision) throw new FlowError("STALE_REVISION", "native fixture source changed");
      if (shown.layoutRevision !== this.token.layoutRevision) throw new FlowError("STALE_LAYOUT", "native fixture layout changed");
      const hit = edition.hits.find(value => value.x === x && value.y === y);
      assert(hit, "test must query an actual native hit");
      return structuredClone(hit.response);
    },
  };
}
function painter() {
  const canvas = document.createElement("canvas"); document.body.append(canvas);
  const renderer = new FlowCanvasRenderer(canvas);
  return { canvas, renderer, dispose() { renderer.dispose(); canvas.remove(); } };
}
const pixels = canvas => canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
const samePixels = (a, b) => a.length === b.length && a.every((value, index) => value === b[index]);
function ink(data, predicate = (r, g, b) => r < 240 || g < 240 || b < 240) {
  let count = 0;
  for (let i = 0; i < data.length; i += 4) if (data[i + 3] && predicate(data[i], data[i + 1], data[i + 2])) count++;
  return count;
}
const red = (r, g, b) => r > 120 && g < 120 && b < 120;
const blue = (r, g, b) => b > 120 && r < 120 && g < 120;
const size = { width: 400, height: 600 };
function rasterDifference(expected, actual) {
  let differing = 0, maximum = 0, total = 0;
  for (let i = 0; i < actual.length; i++) {
    const difference = Math.abs(actual[i] - expected[i]);
    if (difference) { differing++; maximum = Math.max(maximum, difference); total += difference; }
  }
  return { differing, maximum, total };
}

export async function run(fixture, runtime = null) {
  assert(fixture?.schema === "fmd-flow-math-native-v1", "an actual native math fixture is required");
  const results = [], counters = {};
  let trace = null;
  const restore = [];
  for (const context of [CanvasRenderingContext2D.prototype, OffscreenCanvasRenderingContext2D.prototype])
    for (const name of ["fillText", "measureText", "moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "closePath", "fill", "setTransform"]) {
      const original = context[name];
      context[name] = function (...args) {
        if (name === "fillText" || name === "measureText") throw Error(`unexpected browser text operation ${name}`);
        counters[name] = (counters[name] ?? 0) + 1;
        if (trace) trace.push([name, ...args]);
        return original.apply(this, args);
      };
      restore.push(() => { context[name] = original; });
    }
  const test = async (name, fn) => {
    try { await fn(); results.push({ name, pass: true }); }
    catch (error) { results.push({ name, pass: false, error: String(error.stack ?? error) }); }
  };
  let nativePixels = 0, nativePaths = 0, nativeCommands = 0, scrollRaster = null, wasmEvidence = null;
  try {
    await test("native fixture contains real colored curved math, exact source and a later source revision", () => {
      const { original, updated } = fixture;
      assert(original.source !== updated.source, "replacement source must differ");
      assert(BigInt(updated.snapshot.revision) > BigInt(original.snapshot.revision), "native source revision must advance");
      for (const edition of [original, updated]) {
        const paths = edition.snapshot.items;
        assert(paths.length && paths.length < 256 && paths.every(item => item.kind === "path"), "math-only native snapshot");
        assert(paths.some(item => item.commands.some(command => command[0] === "Q")), "actual curved bundled glyphs");
        assert(paths.some(item => item.fill !== null), "actual formula color");
        assert(edition.reading.nodes.every(node => node.role === "math" && node.text.length), "native math reading leaves");
        assert(edition.snapshot.pendingAssetCount === 0, "no external math assets");
        equal(new TextEncoder().encode(edition.source).length, edition.snapshot.sourceLengthBytes, "exact raw source bytes");
        equal(edition.source.length, edition.snapshot.sourceLengthUtf16, "exact raw source UTF-16");
      }
    });

    await test("unchanged native indexed paths draw every contour with real pixels and colors", async () => {
      const f = painter(), s = recordedSession(fixture.original); let imageRequests = 0;
      for (const key of Object.keys(counters)) counters[key] = 0;
      try {
        const frame = await f.renderer.render(s, { ...size, resolveImage() { imageRequests++; throw Error("unexpected image"); } });
        nativePaths = fixture.original.viewport.items.length;
        nativeCommands = fixture.original.viewport.items.reduce((sum, item) => sum + item.commands.length, 0);
        equal(frame.visibleItems, nativePaths); equal(frame.glyphs, 0); equal(frame.missingImages, 0);
        equal(frame.queryMode, "indexed-viewport"); equal(imageRequests, 0); equal(s.fontRequests, 0);
        const operations = { M: "moveTo", L: "lineTo", Q: "quadraticCurveTo", C: "bezierCurveTo", Z: "closePath" };
        for (const [op, method] of Object.entries(operations)) {
          const expected = fixture.original.viewport.items.flatMap(item => item.commands).filter(command => command[0] === op).length;
          equal(counters[method] ?? 0, expected, `all native ${op} geometry must be drawn`);
        }
        equal(counters.fill, nativePaths, "one nonzero paint per native path");
        nativePixels = ink(pixels(f.canvas)); assert(nativePixels > 100, "actual formula ink must be visible");
        assert(ink(pixels(f.canvas), red) > 10, "native red fraction must stay red");
        for (const node of fixture.original.reading.nodes) {
          const b = node.bounds, x = Math.max(0, Math.floor(b.x)), y = Math.max(0, Math.floor(b.y));
          const w = Math.min(400 - x, Math.ceil(b.width) + 2), h = Math.min(600 - y, Math.ceil(b.height) + 2);
          assert(w > 0 && h > 0 && ink(f.canvas.getContext("2d").getImageData(x, y, w, h).data) > 10,
            "each formula, including the tall matrix, must have visible ink");
        }
      } finally { f.dispose(); }
    });

    await test("native full snapshot and indexed viewport paint identical pixels", async () => {
      const a = painter(), b = painter();
      try {
        await a.renderer.render(recordedSession(fixture.original, false), size);
        await b.renderer.render(recordedSession(fixture.original, true), size);
        assert(samePixels(pixels(a.canvas), pixels(b.canvas)), "native query modes must preserve formula ink");
      } finally { a.dispose(); b.dispose(); }
    });

    await test("native math hit retains its actual source envelope and painted revision", async () => {
      const f = painter(), s = recordedSession(fixture.original), expected = fixture.original.hits[0];
      try {
        await f.renderer.render(s, size);
        equal(await f.renderer.hitTest(expected.x, expected.y), expected.response, "actual native hit response");
        s.adopt(fixture.updated);
        await rejects(f.renderer.hitTest(expected.x, expected.y), "STALE_REVISION");
      } finally { f.dispose(); }
    });

    await test("native reading leaves expose exact selectable TeX with math semantics", async () => {
      const root = document.createElement("article"); document.body.append(root);
      const view = new FlowReaderView(root), s = recordedSession(fixture.original);
      try {
        const reading = await readFlowDocument(s); view.render(reading);
        const nodes = [...root.querySelectorAll('[role="math"]')];
        equal(nodes.length, fixture.original.reading.nodes.length);
        nodes.forEach((node, index) => {
          equal(node.textContent, fixture.original.reading.nodes[index].text, "TeX text must be exact");
          equal(getComputedStyle(node).whiteSpace, "pre-wrap", "TeX whitespace must remain visible");
          const span = sourceSpanToUtf16(fixture.original.source, fixture.original.reading.nodes[index].enclosingSourceSpan);
          assert(fixture.original.source.slice(span.start, span.end).includes(node.textContent), "source span must include exact TeX");
        });
        assert(!root.querySelector("script,img,svg,math,a,iframe"), "semantic source is inert text");
        const match = reading.find("\\frac{1}{2}").matches[0]; assert(match, "formula source must be searchable");
        equal(view.selectMatch(match), "\\frac{1}{2}"); equal(document.getSelection().toString(), "\\frac{1}{2}");
      } finally { view.dispose(); root.remove(); document.getSelection().removeAllRanges(); }
    });

    await test("actual replacement payload updates formula pixels and clears the old color", async () => {
      const f = painter(), s = recordedSession(fixture.original);
      try {
        await f.renderer.render(s, size); const before = pixels(f.canvas);
        s.adopt(fixture.updated); const frame = await f.renderer.render(s, size);
        equal(frame.revision, fixture.updated.snapshot.revision); equal(s.source, fixture.updated.source);
        const after = pixels(f.canvas); assert(!samePixels(before, after), "source replacement changes the geometry");
        assert(ink(after, blue) > 10 && ink(after, red) === 0, "fresh blue formula replaces prior red ink");
      } finally { f.dispose(); }
    });

    await test("late old native snapshot cannot publish after source revision advances", async () => {
      const f = painter(), s = recordedSession(fixture.original, false), gate = deferred();
      try {
        await f.renderer.render(s, size); const before = pixels(f.canvas), frame = f.renderer.frame;
        const snapshot = s.snapshot.bind(s); s.snapshot = query => { const old = snapshot(query); return gate.promise.then(() => old); };
        const pending = f.renderer.render(s, size); s.adopt(fixture.updated); gate.resolve();
        await rejects(pending, "STALE_LAYOUT"); assert(samePixels(before, pixels(f.canvas))); assert(f.renderer.frame === frame);
        s.snapshot = snapshot; await f.renderer.render(s, size);
        equal(f.renderer.frame.revision, fixture.updated.snapshot.revision);
      } finally { gate.resolve(); f.dispose(); }
    });

    await test("a fresh native replacement supersedes delayed old math without late resurrection", async () => {
      const f = painter(), s = recordedSession(fixture.original, false), gate = deferred();
      try {
        const snapshot = s.snapshot.bind(s); s.snapshot = query => { const old = snapshot(query); return gate.promise.then(() => old); };
        const old = f.renderer.render(s, size), refused = rejects(old, "RENDER_SUPERSEDED");
        s.adopt(fixture.updated); s.snapshot = snapshot;
        await f.renderer.render(s, size); const current = pixels(f.canvas); await refused;
        gate.resolve(); await new Promise(resolve => setTimeout(resolve, 0));
        equal(f.renderer.frame.revision, fixture.updated.snapshot.revision);
        assert(samePixels(current, pixels(f.canvas)), "late old result must not touch fresh pixels");
      } finally { gate.resolve(); f.dispose(); }
    });

    await test("native absolute contours scroll without reflow or geometry substitution", async () => {
      const a = painter(), b = painter(), wrong = painter();
      try {
        trace = [];
        await a.renderer.render(recordedSession(fixture.original, false), size);
        const initial = trace; trace = [];
        await b.renderer.render(recordedSession(fixture.original, false), { ...size, scrollX: 17, scrollY: 11 });
        const translated = trace; trace = null;
        const paths = events => events.filter(event => event[0] !== "setTransform");
        const method = { M: "moveTo", L: "lineTo", Q: "quadraticCurveTo", C: "bezierCurveTo", Z: "closePath" };
        const nativeTrace = items => items.flatMap(item => [...item.commands.map(([op, ...args]) => [method[op], ...args]), ["fill", "nonzero"]]);
        equal(paths(initial), nativeTrace(fixture.original.snapshot.items), "initial contour arguments are exact native data");
        const visible = fixture.original.snapshot.items.filter(({ bounds: b }) =>
          b.x < 417 && b.x + b.width > 17 && b.y < 611 && b.y + b.height > 11);
        equal(paths(translated), nativeTrace(visible), "every still-visible native contour keeps its exact arguments and order");
        equal(translated.filter(event => event[0] === "setTransform"), [["setTransform", 1, 0, 0, 1, -17, -11]],
          "scroll is only the explicit camera transform");
        const expected = a.canvas.getContext("2d").getImageData(17, 11, 383, 589).data;
        const actual = b.canvas.getContext("2d").getImageData(0, 0, 383, 589).data;
        const correct = rasterDifference(expected, actual);
        // Chromium's curve antialiasing rounds a few translated edge samples
        // differently even for identical contours and an integer transform.
        // Exact geometry above plus bounded pixel error catches displacement
        // without asserting a bit-identical rasterizer contract it lacks.
        const withinTolerance = ({ maximum, differing, total }) => maximum <= 8
          && differing <= actual.length / 1000 && total <= actual.length / 1000;
        assert(withinTolerance(correct), `camera rasterization error: ${JSON.stringify(correct)}`);
        await wrong.renderer.render(recordedSession(fixture.original, false), { ...size, scrollX: 18, scrollY: 11 });
        const onePixelWrong = rasterDifference(expected, wrong.canvas.getContext("2d").getImageData(0, 0, 383, 589).data);
        scrollRaster = { correct, onePixelWrong };
        assert(!withinTolerance(onePixelWrong), "pixel oracle must reject a one-pixel wrong camera translation");
      } finally { trace = null; a.dispose(); b.dispose(); wrong.dispose(); }
    });

    await test("a forged native control hull is refused before touching the prior real pixels", async () => {
      const f = painter(), s = recordedSession(fixture.original, false);
      try {
        await f.renderer.render(s, size); const before = pixels(f.canvas), frame = f.renderer.frame;
        const bad = structuredClone(fixture.original), item = bad.snapshot.items.find(value => value.commands.some(command => command[0] === "Q"));
        const command = item.commands.find(value => value[0] === "Q"); command[1] = item.bounds.x + item.bounds.width + 10;
        await rejects(f.renderer.render(recordedSession(bad, false), size), "INVALID_WASM_RESPONSE");
        assert(f.renderer.frame === frame && samePixels(before, pixels(f.canvas)), "invalid geometry cannot publish");
      } finally { f.dispose(); }
    });

    await test("semantic math treats hostile-looking TeX as literal selectable source", async () => {
      const root = document.createElement("article"); document.body.append(root);
      const view = new FlowReaderView(root), raw = String.raw`  \\bad{<img src="https://invalid.example/private" onerror="window.mathExecuted=1">}  `;
      const fake = structuredClone(fixture.original); fake.reading.nodes = [{ ...fake.reading.nodes[0], text: raw, inlineRuns: [] }];
      fake.reading.total = 1; fake.reading.nextOffset = null;
      try {
        view.render(await readFlowDocument(recordedSession(fake)));
        equal(root.querySelector('[role="math"]').textContent, raw);
        assert(!root.querySelector("img,script,a,svg,iframe") && !globalThis.mathExecuted, "literal source cannot execute or fetch");
      } finally { view.dispose(); root.remove(); }
    });

    await test("generic cubic paths preserve nonzero holes, explicit alpha and clipping in real Canvas", async () => {
      // Explicit synthetic consumer-boundary case: native math currently emits
      // quadratic contours, but the renderer-neutral wire also permits cubics.
      const f = painter(), edition = structuredClone(fixture.original);
      const item = { ...edition.snapshot.items[0], index: 1, bounds: { x: 10, y: 10, width: 50, height: 50 }, fill: [1, 0, 0, 0.5],
        commands: [["M", 10, 10], ["C", 20, 10, 40, 10, 60, 10], ["L", 60, 60], ["L", 10, 60], ["Z"],
          ["M", 25, 25], ["L", 25, 45], ["L", 45, 45], ["L", 45, 25], ["Z"]] };
      edition.snapshot.items = [{ kind: "clip", index: 0, bounds: { x: 10, y: 10, width: 40, height: 50 }, childCount: 1 }, item];
      edition.snapshot.total = 2; edition.snapshot.nextOffset = null;
      const pixel = (x, y) => [...f.canvas.getContext("2d").getImageData(x, y, 1, 1).data];
      try {
        await f.renderer.render(recordedSession(edition, false), size);
        const colored = pixel(15, 15); assert(colored[0] === 255 && Math.abs(colored[1] - 128) <= 1 && Math.abs(colored[2] - 128) <= 1);
        equal(pixel(35, 35), [255, 255, 255, 255], "reverse-wound contour must stay a hole");
        equal(pixel(55, 15), [255, 255, 255, 255], "omitted clip area must stay clear");
      } finally { f.dispose(); }
    });

    if (runtime) {
      const create = source => runtime.createFlowSession(source, { font: "sans", ...fixture.original.snapshot.options });
      const snapshot = session => session.snapshot({ offset: 0, limit: 2048, glyphs: true, token: session.token });
      const geometry = page => ({ items: page.items, totalBounds: page.totalBounds });
      const monitored = session => {
        const calls = { glyphOutlines: 0, fontBytes: 0, assetBytes: 0, images: 0 }, view = Object.create(session);
        for (const name of ["glyphOutlines", "fontBytes", "assetBytes"])
          Object.defineProperty(view, name, { value(...args) { calls[name]++; return session[name](...args); } });
        return { session: view, calls, options: { ...size, resolveImage() { calls.images++; throw Error("unexpected math image"); } } };
      };
      const noResources = calls => equal(calls, { glyphOutlines: 0, fontBytes: 0, assetBytes: 0, images: 0 },
        "formula-only rendering must not request text outlines, fonts or images");

      await test("generated WASM public Flow facade matches actual native math snapshot, viewport and reading", async () => {
        const s = await create(fixture.original.source);
        try {
          const page = snapshot(s), token = s.token;
          equal(geometry(page), geometry(fixture.original.snapshot), "native/WASM path geometry must agree");
          const { x, y, width, height, ...query } = fixture.original.viewportRequest;
          equal(s.viewport({ viewport: { x, y, width, height }, ...query, token }), fixture.original.viewport,
            "actual WASM spatial payload must match native");
          equal(s.readingOrder({ offset: 0, limit: 2048, token }), fixture.original.reading, "native/WASM semantic source must agree");
          equal(s.source, fixture.original.source);
          equal(s.copySource(0, new TextEncoder().encode(s.source).length, token.revision), fixture.original.source);
          equal(s.pendingAssets({ token }).requests, [], "math has no pending resource requests");
          wasmEvidence = { paths: page.items.length, commands: page.items.reduce((sum, item) => sum + item.commands.length, 0),
            nativeGeometryEqual: true, nativeViewportEqual: true, nativeReadingEqual: true };
        } finally { s.dispose(); }
      });

      await test("generated WASM formula-only Canvas and selectable reader use no font or asset requests", async () => {
        const s = await create(fixture.original.source), watched = monitored(s), f = painter(), reference = painter();
        const root = document.createElement("article"); document.body.append(root); const view = new FlowReaderView(root);
        try {
          await f.renderer.render(watched.session, watched.options);
          await reference.renderer.render(recordedSession(fixture.original), size);
          assert(samePixels(pixels(f.canvas), pixels(reference.canvas)), "real-WASM Canvas pixels must match native-payload Canvas pixels");
          assert(ink(pixels(f.canvas), red) > 10);
          const reading = await readFlowDocument(s); view.render(reading);
          equal([...root.querySelectorAll('[role="math"]')].map(node => node.textContent), fixture.original.reading.nodes.map(node => node.text));
          const match = reading.find("\\frac{1}{2}").matches[0]; assert(match);
          equal(view.selectMatch(match), "\\frac{1}{2}"); equal(document.getSelection().toString(), "\\frac{1}{2}");
          noResources(watched.calls);
        } finally { view.dispose(); root.remove(); document.getSelection().removeAllRanges(); f.dispose(); reference.dispose(); s.dispose(); }
      });

      await test("generated WASM source replacement, real edit and reflow preserve exact source and fresh geometry", async () => {
        const s = await create(fixture.original.source), watched = monitored(s), f = painter(); let fresh;
        try {
          const before = s.token, oldReading = await readFlowDocument(s);
          s.replaceSource(fixture.updated.source, { expectedRevision: before.revision });
          equal(geometry(snapshot(s)), geometry(fixture.updated.snapshot), "native/WASM replacement geometry must agree");
          equal(s.source, fixture.updated.source);
          await rejects(Promise.resolve().then(() => s.snapshot({ token: before })), "STALE_REVISION");
          await rejects(Promise.resolve().then(() => oldReading.assertCurrent()), "STALE_REVISION");
          await f.renderer.render(watched.session, watched.options);
          assert(ink(pixels(f.canvas), blue) > 10 && ink(pixels(f.canvas), red) === 0, "real replacement must replace prior formula color");

          const at = s.source.indexOf("a^2+b^2"); assert(at >= 0);
          const edited = s.source.slice(0, at) + "d^3+e^3" + s.source.slice(at + 7);
          s.edit(at, at + 7, "d^3+e^3", { expectedRevision: s.token.revision });
          equal(s.source, edited); fresh = await create(edited);
          equal(geometry(snapshot(s)), geometry(snapshot(fresh)), "incremental source edit must match a fresh real WASM session");
          const editedToken = s.token;
          s.reflow({ viewportWidth: 30 }, editedToken);
          equal(s.token.revision, editedToken.revision); assert(s.token.layoutRevision !== editedToken.layoutRevision);
          equal(s.source, edited, "reflow must preserve exact raw source");
          await rejects(Promise.resolve().then(() => s.snapshot({ token: editedToken })), "STALE_LAYOUT");
          const narrow = snapshot(s); assert(narrow.items.length && narrow.items.every(item => item.kind === "path"));
          assert(narrow.items.every(item => item.bounds.x >= -0.001 && item.bounds.x + item.bounds.width <= 30.001), "scaled math fits the real reflow column");
          await f.renderer.render(watched.session, { ...watched.options, width: 30, height: 200 });
          assert(ink(pixels(f.canvas), blue) > 0, "narrow formula still draws actual ink");
          s.reflow(fixture.original.snapshot.options, s.token);
          equal(geometry(snapshot(s)), geometry(snapshot(fresh)), "restoring width must restore fresh-session path geometry");
          equal(s.copySource(0, new TextEncoder().encode(edited).length, s.token.revision), edited);
          noResources(watched.calls);
        } finally { fresh?.dispose(); f.dispose(); s.dispose(); }
      });

      await test("generated WASM edit supersedes a delayed old paint and later old results cannot reappear", async () => {
        const s = await create(fixture.original.source), watched = monitored(s), delayed = Object.create(s), gate = deferred(), f = painter();
        Object.defineProperties(delayed, {
          supportsViewport: { value: false },
          snapshot: { value(query) { const page = s.snapshot(query); return gate.promise.then(() => page); } },
        });
        try {
          const old = f.renderer.render(delayed, size), refused = rejects(old, "RENDER_SUPERSEDED");
          s.replaceSource(fixture.updated.source, { expectedRevision: s.token.revision });
          await f.renderer.render(watched.session, watched.options); const shown = pixels(f.canvas);
          await refused; gate.resolve(); await new Promise(resolve => setTimeout(resolve, 0));
          equal(f.renderer.frame.revision, s.token.revision);
          assert(samePixels(shown, pixels(f.canvas)) && ink(shown, blue) > 10, "only actual updated-WASM pixels remain");
          noResources(watched.calls);
        } finally { gate.resolve(); f.dispose(); s.dispose(); }
      });

      await test("generated WASM unsupported math preserves the complete literal source and readable fallback", async () => {
        const formula = String.raw`\notARealMathCommand{<img src=x>}`, source = `$$\n${formula}\n$$\n`;
        const s = await create(source), f = painter(), root = document.createElement("article"); document.body.append(root);
        const view = new FlowReaderView(root);
        try {
          const page = snapshot(s);
          assert(page.items.length && page.items.every(item => item.kind !== "path"), "unsupported formula must not publish partial math");
          const text = page.items.filter(item => item.kind === "text").map(item => item.text).join("");
          assert(text.includes("Math source fallback:") && text.includes(formula), "actual fallback draws explanation and complete TeX");
          equal(s.source, source); equal(s.pendingAssets().requests, []);
          await f.renderer.render(s, size); assert(ink(pixels(f.canvas)) > 100, "fallback is visible using actual bundled outlines");
          view.render(await readFlowDocument(s)); equal(root.querySelector('[role="math"]').textContent, formula);
          assert(!root.querySelector("img,script,svg,a"), "fallback source cannot become executable DOM");
        } finally { view.dispose(); root.remove(); f.dispose(); s.dispose(); }
      });
    }
  } finally { for (const action of restore) action(); }
  return { passed: results.filter(result => result.pass).length, failed: results.filter(result => !result.pass).length,
    nativePaths, nativeCommands, nativePixels, scrollRaster, wasmEvidence, results };
}
