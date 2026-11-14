// Actual native fixture + matching generated WASM + real Chromium Canvas/Copy.
// This suite requires generated bindings; recorded sessions only replay native
// pages for the independent Canvas pixel comparison and one labeled dash control.
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { FlowReaderView, readFlowDocument, sourceSpanToUtf16 } from "../flow-reader.js";

const assert = (value, message = "assertion failed") => { if (!value) throw Error(message); };
const equal = (a, b, message = "values differ") => assert(JSON.stringify(a) === JSON.stringify(b), message);
const near = (a, b, message) => assert(Math.abs(a - b) <= 1e-6 * Math.max(1, Math.abs(b)), message);
const bytes = value => new TextEncoder().encode(value).length;
const size = { width: 400, height: 800 };
const snapshot = session => session.snapshot({ offset: 0, limit: 2048, glyphs: true, token: session.token });
const geometry = page => ({ items: page.items, totalBounds: page.totalBounds });
const paths = page => page.items.filter(item => item.kind === "path");
const pixels = canvas => canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
const samePixels = (a, b) => a.length === b.length && a.every((value, index) => value === b[index]);
function ink(data) {
  let count = 0;
  for (let i = 0; i < data.length; i += 4)
    if (data[i + 3] && Math.min(data[i], data[i + 1], data[i + 2]) < 240) count++;
  return count;
}
async function rejects(action, code) {
  try { await action(); }
  catch (error) { equal(error.code, code, "expected " + code + ", got " + error.code); return; }
  throw Error("expected " + code);
}
function painter() {
  const canvas = document.createElement("canvas"); document.body.append(canvas);
  const renderer = new FlowCanvasRenderer(canvas);
  return { canvas, renderer, dispose() { renderer.dispose(); canvas.remove(); } };
}
function recorded(edition, indexed = true) {
  return {
    disposed: false, supportsViewport: indexed,
    get token() { return { revision: edition.snapshot.revision, layoutRevision: edition.snapshot.layoutRevision }; },
    get source() { return edition.source; },
    get layoutOptions() { return edition.snapshot.options; },
    snapshot(query) {
      equal(query.token, this.token, "native snapshot revision fence");
      assert(query.offset === 0 && query.limit >= edition.snapshot.total && query.glyphs,
        "native fixture must fit a complete unchanged page");
      return structuredClone(edition.snapshot);
    },
    viewport(query) {
      equal(query.token, this.token, "native viewport revision fence");
      equal({ ...query.viewport, afterIndex: query.afterIndex, limit: query.limit, glyphs: query.glyphs },
        edition.viewportRequest, "native fixture and actual Canvas query must agree");
      return structuredClone(edition.viewport);
    },
    glyphOutlines(fontId, ids) {
      const batch = edition.glyphOutlines.find(value => value.fontId === fontId);
      assert(batch, "each visible font must have actual native outlines");
      return structuredClone({ ...batch, glyphs: ids.map(id => {
        const glyph = batch.glyphs.find(value => value.glyphId === id);
        assert(glyph, "each requested glyph must be in the actual native fixture");
        return glyph;
      }) });
    },
  };
}
function watched(session, indexed = true) {
  const proxy = Object.create(session), calls = { glyphOutlines: 0, fontBytes: 0, assetBytes: 0 };
  Object.defineProperty(proxy, "supportsViewport", { value: indexed });
  for (const name of Object.keys(calls)) Object.defineProperty(proxy, name, {
    value(...args) { calls[name]++; return session[name](...args); },
  });
  return { session: proxy, calls };
}
function canvasColor(color) {
  const context = document.createElement("canvas").getContext("2d");
  context.fillStyle = "rgba(" + color.slice(0, 3).map(value => value * 255).join(",") + "," + color[3] + ")";
  return context.fillStyle;
}
function assertStroke(event, item) {
  const expected = item.stroke;
  equal(event.commands, item.commands, "every stroked contour is unchanged native geometry");
  equal(event.color, canvasColor(expected.color), "native stroke color");
  equal(event.cap, expected.cap, "native line cap");
  equal(event.join, expected.join, "native line join");
  near(event.width, expected.width, "native stroke width");
  near(event.miterLimit, expected.miterLimit, "native miter limit");
  near(event.dashOffset, expected.dashOffset, "native dash offset");
  // Canvas doubles an odd-length dash array and stores lengths at its own
  // graphics precision. Geometry and wire data are still compared exactly.
  const dash = expected.dash.length % 2 ? expected.dash.concat(expected.dash) : expected.dash;
  equal(event.dash.length, dash.length, "native dash count");
  event.dash.forEach((value, index) => near(value, dash[index], "native dash length"));
}

export async function run(fixture, runtime, mode = "enabled") {
  equal(fixture?.schema, "fmd-flow-diagram-native-v1", "actual native diagram fixture required");
  assert(runtime && typeof runtime.createFlowSession === "function", "matching generated WASM is required");
  assert(["enabled", "lean"].includes(mode), "unknown build profile");
  equal(fixture.scenes.map(scene => scene.name), ["flowchart", "class", "sequence", "pie", "unsupported", "ascii"]);
  const results = [], evidence = { mode, scenes: {}, browserTextCalls: 0, fetchCalls: 0 };
  const restore = [], currentPaths = new WeakMap();
  let trace = null;
  const test = async (name, action) => {
    try { await action(); results.push({ name, pass: true }); }
    catch (error) { results.push({ name, pass: false, error: String(error.stack ?? error) }); }
    finally { trace = null; }
  };
  const create = (edition, source = edition.source, options = {}) =>
    runtime.createFlowSession(source, { font: "sans", ...edition.snapshot.options, ...options });
  const contexts = [CanvasRenderingContext2D.prototype];
  if (typeof OffscreenCanvasRenderingContext2D === "function")
    contexts.push(OffscreenCanvasRenderingContext2D.prototype);
  for (const prototype of contexts) {
    const ops = { moveTo: "M", lineTo: "L", quadraticCurveTo: "Q", bezierCurveTo: "C", closePath: "Z" };
    for (const name of ["fillText", "strokeText", "measureText", "beginPath", ...Object.keys(ops), "stroke", "fill"]) {
      const original = prototype[name];
      prototype[name] = function (...args) {
        if (["fillText", "strokeText", "measureText"].includes(name)) {
          evidence.browserTextCalls++;
          throw Error("unexpected browser font operation: " + name);
        }
        if (trace) {
          if (name === "beginPath") currentPaths.set(this, []);
          else if (Object.hasOwn(ops, name)) currentPaths.get(this)?.push([ops[name], ...args]);
          else if (name === "stroke") trace.push({
            kind: "stroke", commands: structuredClone(currentPaths.get(this) ?? []),
            color: this.strokeStyle, width: this.lineWidth, dash: this.getLineDash(),
            dashOffset: this.lineDashOffset, cap: this.lineCap, join: this.lineJoin, miterLimit: this.miterLimit,
          });
          else if (name === "fill") trace.push({ kind: "fill", color: this.fillStyle });
        }
        return original.apply(this, args);
      };
      restore.push(() => { prototype[name] = original; });
    }
  }
  const originalFetch = globalThis.fetch;
  globalThis.fetch = () => { evidence.fetchCalls++; throw Error("diagram rendering must not fetch resources"); };
  restore.push(() => { globalThis.fetch = originalFetch; });
  try {
    for (const edition of fixture.scenes) {
      await test("generated " + edition.name + " geometry, reading, source bytes, outlines and hits equal native output", async () => {
        const session = await create(edition);
        try {
          const page = snapshot(session);
          equal(geometry(page), geometry(edition.snapshot), "native/WASM display parity");
          equal(page.options, edition.snapshot.options);
          equal(page.sourceLengthBytes, bytes(edition.source));
          equal(page.sourceLengthUtf16, edition.source.length);
          const { x, y, width, height, ...query } = edition.viewportRequest;
          equal(session.viewport({ viewport: { x, y, width, height }, ...query, token: session.token }),
            edition.viewport, "native/WASM indexed viewport parity");
          equal(session.readingOrder({ offset: 0, limit: 2048, token: session.token }), edition.reading,
            "native/WASM semantic source parity");
          for (const batch of edition.glyphOutlines)
            equal(session.glyphOutlines(batch.fontId, batch.glyphs.map(glyph => glyph.glyphId)), batch);
          for (const hit of edition.hits) equal(session.hitTest(hit.x, hit.y, session.token), hit.response);
          equal(session.copySource(0, bytes(session.source), session.token.revision), edition.source);
          equal(session.pendingAssets().requests, [], "diagrams have no pending image resources");
          const nodes = edition.reading.nodes.filter(node => node.role === (edition.name === "ascii" ? "code-block" : "diagram"));
          assert(nodes.some(node => node.text === edition.codeSource), "complete original code must survive");
          if (edition.supported) {
            assert(paths(page).length > 2, "supported diagrams must produce real vector paths");
            assert(page.items.some(item => item.kind === "text" && item.fill && item.fontRun?.glyphs.length),
              "diagram labels need explicit native color and actual glyphs");
          } else assert(!paths(page).length, "fallback must not invent a partial diagram");
          evidence.scenes[edition.name] = { nativeParity: true, paths: paths(page).length };
        } finally { session.dispose(); }
      });
    }

    for (const edition of fixture.scenes.filter(scene => scene.supported)) {
      await test("real Canvas " + edition.name + " strokes, native outlines and pixels agree with native fixture", async () => {
        const session = await create(edition), proxy = watched(session), actual = painter(), expected = painter(), full = painter();
        try {
          trace = [];
          const frame = await actual.renderer.render(proxy.session, { ...size,
            resolveImage() { throw Error("diagram must not request a browser image"); } });
          const actualTrace = trace; trace = null;
          await expected.renderer.render(recorded(edition), size);
          await full.renderer.render(watched(session, false).session, size);
          equal(frame.queryMode, "indexed-viewport"); equal(frame.missingImages, 0);
          assert(frame.glyphs > 0 && proxy.calls.glyphOutlines > 0, "labels are real bundled outlines");
          equal(proxy.calls.fontBytes, 0); equal(proxy.calls.assetBytes, 0);
          assert(samePixels(pixels(actual.canvas), pixels(expected.canvas)), "native and WASM pixels must match");
          assert(samePixels(pixels(actual.canvas), pixels(full.canvas)), "indexed and complete display pixels must match");
          const nativeStrokes = paths(edition.viewport).filter(item => item.stroke);
          const paintedStrokes = actualTrace.filter(event => event.kind === "stroke");
          equal(paintedStrokes.length, nativeStrokes.length, "one real Canvas stroke per native stroke path");
          nativeStrokes.forEach((item, index) => assertStroke(paintedStrokes[index], item));
          const nativeFills = edition.viewport.items.flatMap(item => {
            if (item.kind === "text") {
              assert(item.fill && item.fontRun?.glyphs.length, "each diagram label has explicit color and glyphs");
              return Array(item.fontRun.glyphs.length).fill(canvasColor(item.fill));
            }
            if (item.kind === "path" && (item.fill !== null || item.stroke == null)) {
              assert(item.fill, "compiled diagram fills must carry actual RGBA");
              return [canvasColor(item.fill)];
            }
            return [];
          });
          equal(actualTrace.filter(event => event.kind === "fill").map(event => event.color), nativeFills,
            "every glyph and filled shape retains its native color in paint order");
          const sceneInk = ink(pixels(actual.canvas)); assert(sceneInk > 100, "actual diagram ink must be visible");
          evidence.scenes[edition.name] = { ...evidence.scenes[edition.name], inkPixels: sceneInk,
            strokes: nativeStrokes.length, dashedStrokes: nativeStrokes.filter(item => item.stroke.dash.length).length,
            outlineRequests: proxy.calls.glyphOutlines };
        } finally { trace = null; actual.dispose(); expected.dispose(); full.dispose(); session.dispose(); }
      });
    }

    await test("real dashed diagram ink differs from an explicit solid-stroke negative control", async () => {
      const edition = fixture.scenes.find(scene => scene.name === "flowchart");
      const session = await create(edition), actual = painter(), solid = painter();
      try {
        assert(paths(edition.viewport).some(item => item.stroke?.dash.length), "fixture must include visible native dashes");
        const control = structuredClone(edition);
        for (const page of [control.snapshot, control.viewport])
          for (const item of paths(page)) if (item.stroke) item.stroke.dash = [];
        await actual.renderer.render(session, size);
        await solid.renderer.render(recorded(control), size);
        assert(!samePixels(pixels(actual.canvas), pixels(solid.canvas)), "discarding native dashes must change the real raster");
        evidence.dashPixelControl = true;
      } finally { actual.dispose(); solid.dispose(); session.dispose(); }
    });

    await test("semantic source regions support exact native Copy, whitespace and safe literal labels", async () => {
      let copied = 0;
      for (const edition of fixture.scenes) {
        const session = await create(edition), root = document.createElement("article");
        document.body.append(root); const view = new FlowReaderView(root);
        try {
          const reading = await readFlowDocument(session); await view.renderAsync(reading);
          const role = edition.name === "ascii" ? "code-block" : "diagram";
          const node = reading.nodes.find(value => value.role === role && value.text === edition.codeSource);
          assert(node, "exact code-source reading leaf");
          const sourceElement = role === "diagram"
            ? root.querySelector('section[role="region"][aria-label="Diagram source"] pre code')
            : root.querySelector("pre code");
          assert(sourceElement); equal(sourceElement.textContent, edition.codeSource);
          equal(getComputedStyle(sourceElement.parentElement).whiteSpace, "pre");
          const match = reading.find(edition.codeSource).matches.find(value => value.nodeIndex === node.index);
          assert(match, "complete diagram source must be searchable");
          equal(view.selectMatch(match), edition.codeSource);
          equal(document.getSelection().toString(), edition.codeSource, "native browser Copy retains the final newline");
          const envelope = sourceSpanToUtf16(session.source, node.enclosingSourceSpan);
          assert(session.source.slice(envelope.start, envelope.end).includes(edition.codeSource), "source envelope owns the code");
          equal(session.copySource(0, bytes(session.source), session.token.revision), edition.source);
          assert(!root.querySelector("svg,img,script,iframe,a"), "source transcript must remain inert");
          copied++;
        } finally { view.dispose(); root.remove(); document.getSelection().removeAllRanges(); session.dispose(); }
      }
      evidence.exactCopyScenes = copied;
    });

    await test("unsupported Mermaid remains complete inert source and creates no synthetic diagram", async () => {
      const fence = String.fromCharCode(96).repeat(3);
      const code = 'classDiagram\n<script src="https://invalid.example/x.js">alert(1)</script>\nA -- B\n';
      const source = fence + "mermaid\n" + code + fence + "\n";
      const session = await create(fixture.original, source), f = painter(), root = document.createElement("article");
      document.body.append(root); const view = new FlowReaderView(root);
      try {
        assert(paths(snapshot(session)).length === 0, "unsupported DSL must not become a partial graphic");
        const reading = await readFlowDocument(session); view.render(reading);
        const node = reading.nodes.find(value => value.role === "diagram"); equal(node.text, code);
        const match = reading.find(code).matches.find(value => value.nodeIndex === node.index); assert(match);
        equal(view.selectMatch(match), code); equal(document.getSelection().toString(), code);
        assert(!root.querySelector("script,svg,img,a,iframe"));
        await f.renderer.render(session, size); assert(ink(pixels(f.canvas)) > 50, "fallback stays visibly readable");
        equal(session.pendingAssets().requests, []);
      } finally { f.dispose(); view.dispose(); root.remove(); document.getSelection().removeAllRanges(); session.dispose(); }
    });

    await test("source replacement removes old paths and publishes the actual native edited diagram", async () => {
      const session = await create(fixture.original), f = painter(), expected = painter(); let fresh;
      try {
        await f.renderer.render(session, size);
        const before = pixels(f.canvas), old = session.token, oldPaths = paths(snapshot(session));
        session.replaceSource(fixture.updated.source, { expectedRevision: old.revision });
        equal(geometry(snapshot(session)), geometry(fixture.updated.snapshot), "replacement matches actual native edited edition");
        equal(session.readingOrder({ offset: 0, limit: 2048, token: session.token }), fixture.updated.reading,
          "replacement publishes the matching native source transcript");
        equal(session.copySource(0, bytes(session.source), session.token.revision), fixture.updated.source);
        assert(JSON.stringify(paths(snapshot(session))) !== JSON.stringify(oldPaths), "replacement changes diagram paths");
        await rejects(() => session.snapshot({ token: old }), "STALE_REVISION");
        await f.renderer.render(session, size); await expected.renderer.render(recorded(fixture.updated), size);
        assert(!samePixels(before, pixels(f.canvas)), "old diagram ink is replaced");
        assert(samePixels(pixels(f.canvas), pixels(expected.canvas)), "replacement has only fresh native ink");
        equal(f.renderer.frame.revision, session.token.revision);
        fresh = await create(fixture.updated); equal(geometry(snapshot(session)), geometry(snapshot(fresh)));
        evidence.replacementParity = true;
      } finally { fresh?.dispose(); f.dispose(); expected.dispose(); session.dispose(); }
    });

    await test("UTF-16 edits and rejected native source changes retain transactional diagram state", async () => {
      const session = await create(fixture.original), f = painter(); let fresh;
      try {
        const at = session.source.startsWith("\ufeff") ? 1 : 0, prefix = "# Edited diagram\n\n";
        const beforeEdit = session.token;
        session.edit(at, at, prefix, { expectedRevision: beforeEdit.revision });
        fresh = await create(fixture.original, session.source);
        equal(geometry(snapshot(session)), geometry(snapshot(fresh))); fresh.dispose(); fresh = null;
        await f.renderer.render(session, size);
        const before = snapshot(session), source = session.source, token = session.token, frame = f.renderer.frame, prior = pixels(f.canvas);
        await rejects(() => session.replaceSource(source + "\n\n\u05d0\u05d1\n", { expectedRevision: token.revision }), "LAYOUT_ERROR");
        await rejects(() => session.edit(0, 0, "stale", { expectedRevision: beforeEdit.revision }), "STALE_REVISION");
        await rejects(() => session.reflow({ viewportWidth: 180 }, beforeEdit), "STALE_REVISION");
        equal(session.source, source); equal(session.token, token);
        equal(geometry(snapshot(session)), geometry(before)); equal(f.renderer.frame, frame);
        assert(samePixels(pixels(f.canvas), prior));
        await f.renderer.render(session, size); assert(samePixels(pixels(f.canvas), prior));
        equal(session.copySource(0, bytes(source), token.revision), source);
        evidence.editRollback = true;
      } finally { fresh?.dispose(); f.dispose(); session.dispose(); }
    });

    await test("diagram reflow matches a fresh narrow native layout and failed reflow preserves it", async () => {
      const edition = fixture.scenes.find(scene => scene.name === "flowchart");
      const session = await create(edition), f = painter(); let fresh;
      try {
        const wide = snapshot(session), old = session.token;
        session.reflow({ viewportWidth: 180 }, old);
        await rejects(() => session.snapshot({ token: old }), "STALE_LAYOUT");
        const narrow = snapshot(session);
        fresh = await create(edition, edition.source, { viewportWidth: 180 });
        equal(geometry(narrow), geometry(snapshot(fresh)), "reflow and fresh native layout agree");
        equal(session.readingOrder({ token: session.token }).nodes, fresh.readingOrder({ token: fresh.token }).nodes,
          "reflow also updates the native semantic bounds");
        assert(JSON.stringify(paths(wide)) !== JSON.stringify(paths(narrow)), "available width changes real diagram geometry");
        for (const item of paths(narrow))
          assert(item.bounds.x >= -0.002 && item.bounds.x + item.bounds.width <= 180.002, "native diagram fits its viewport");
        await f.renderer.render(session, { ...size, width: 180 });
        const prior = pixels(f.canvas), token = session.token, options = session.layoutOptions;
        await rejects(() => session.reflow({ viewportWidth: 0 }, token), "INVALID_OPTIONS");
        await rejects(() => session.reflow({ viewportWidth: 400 }, old), "STALE_LAYOUT");
        equal(session.token, token); equal(session.layoutOptions, options);
        equal(geometry(snapshot(session)), geometry(narrow)); assert(samePixels(pixels(f.canvas), prior));
        session.reflow(edition.snapshot.options, token);
        equal(geometry(snapshot(session)), geometry(wide), "wide layout is restored without source drift");
        equal(session.source, edition.source);
        evidence.reflowParity = true;
      } finally { fresh?.dispose(); f.dispose(); session.dispose(); }
    });
    await test("actual syntax highlighting preserves diagram geometry and highlights complete fallback and Rust source", async () => {
      const flow = fixture.scenes.find(scene => scene.name === "flowchart").codeSource;
      const fallback = fixture.scenes.find(scene => scene.name === "unsupported").codeSource;
      const rust = "let answer = 42; // keep ordinary code\n", fence = String.fromCharCode(96).repeat(3);
      const source = fixture.original.source + "\n" + fence + "mermaid\n" + fallback + fence
        + "\n\n" + fence + "rust\n" + rust + fence + "\n";
      const session = await create(fixture.original, source), f = painter(); let fresh;
      const labels = page => page.items.filter(item => item.kind === "text" && item.fill);
      const sameSpan = (a, b) => a.startByte === b.startByte && a.endByte === b.endByte;
      const textAt = (page, node) => page.items.filter(item =>
        item.kind === "text" && sameSpan(item.enclosingSourceSpan, node.enclosingSourceSpan));
      const checkHighlighted = (page, reading, rustSource) => {
        const figure = reading.nodes.find(node => node.role === "diagram" && node.text === flow);
        const code = reading.nodes.find(node => node.role === "diagram" && node.text === fallback);
        const ordinary = reading.nodes.find(node => node.role === "code-block" && node.text === rustSource);
        assert(figure && code && ordinary, "all three original reading leaves survive highlighting");
        assert(paths(page).length && paths(page).every(item => sameSpan(item.enclosingSourceSpan, figure.enclosingSourceSpan)),
          "only the supported figure owns diagram paths");
        const figureText = textAt(page, figure), fallbackText = textAt(page, code), ordinaryText = textAt(page, ordinary);
        assert(figureText.length && figureText.every(item => item.colorRole === "diagram" && item.fill),
          "generated labels keep their explicit native colors and never become source tokens");
        const literal = items => items.filter(item => item.colorRole === "code" || item.colorRole.startsWith("tok-"))
          .map(item => item.text).join("");
        equal(literal(fallbackText), fallback.replace(/\n/g, ""), "highlighted fallback retains every source character");
        assert(fallbackText.some(item => item.colorRole.startsWith("tok-")), "fallback has real Mermaid token colors");
        assert(fallbackText.map(item => item.text).join("").includes("Diagram source fallback:"), "fallback explanation remains visible");
        equal(literal(ordinaryText), rustSource.replace(/\n/g, ""), "highlighted Rust retains complete source");
        assert(ordinaryText.some(item => item.colorRole === "tok-kw"));
        assert(ordinaryText.some(item => item.colorRole === "tok-nu"));
        return figure;
      };
      try {
        assert(session.supportsCodeHighlighting); equal(session.codeHighlighting, false);
        const plain = snapshot(session), plainReading = session.readingOrder({ token: session.token }), token = session.token;
        await f.renderer.render(session, size);
        const figure = plainReading.nodes.find(node => node.role === "diagram" && node.text === flow);
        assert(figure && figure.bounds.y === 0);
        const figureHeight = Math.ceil(figure.bounds.height);
        const originalInk = f.canvas.getContext("2d").getImageData(0, 0, size.width, figureHeight).data;
        session.setCodeHighlighting(true, token);
        equal(session.codeHighlighting, true); equal(session.token.revision, token.revision);
        await rejects(() => session.snapshot({ token }), "STALE_LAYOUT");
        const highlighted = snapshot(session), reading = session.readingOrder({ token: session.token });
        equal(paths(highlighted), paths(plain), "syntax paint must preserve every native path and stroke");
        equal(labels(highlighted), labels(plain), "syntax paint must preserve actual label runs and explicit fills");
        equal(highlighted.totalBounds, plain.totalBounds); equal(reading.nodes, plainReading.nodes);
        checkHighlighted(highlighted, reading, rust);
        equal(session.copySource(0, bytes(source), session.token.revision), source);
        await f.renderer.render(session, size);
        assert(samePixels(originalInk, f.canvas.getContext("2d").getImageData(0, 0, size.width, figureHeight).data),
          "highlighting literal source below a diagram must not change the figure's real pixels");

        session.setCodeHighlighting(false, session.token);
        equal(session.codeHighlighting, false); equal(geometry(snapshot(session)), geometry(plain));
        session.setCodeHighlighting(true, session.token);
        const at = session.source.indexOf("42; // keep ordinary code");
        assert(at >= 0); session.edit(at, at + 2, "123", { expectedRevision: session.token.revision });
        const changed = source.slice(0, at) + "123" + source.slice(at + 2), changedRust = rust.replace("42", "123");
        session.reflow({ viewportWidth: 180 }, session.token);
        equal(session.codeHighlighting, true); equal(session.source, changed);
        const narrowed = snapshot(session), narrowedReading = session.readingOrder({ token: session.token });
        checkHighlighted(narrowed, narrowedReading, changedRust);
        fresh = await create(fixture.original, changed, { viewportWidth: 180 });
        fresh.setCodeHighlighting(true, fresh.token);
        equal(geometry(narrowed), geometry(snapshot(fresh)), "highlighted edits and reflow match a fresh actual-WASM session");
        equal(narrowedReading.nodes, fresh.readingOrder({ token: fresh.token }).nodes);
        equal(session.copySource(0, bytes(changed), session.token.revision), changed);
        await f.renderer.render(session, { ...size, width: 180 });
        assert(ink(pixels(f.canvas)) > 100);
        session.setCodeHighlighting(false, session.token);
        const unpainted = snapshot(session);
        equal(paths(unpainted), paths(narrowed)); equal(labels(unpainted), labels(narrowed));
        assert(!unpainted.items.some(item => item.kind === "text" && item.colorRole.startsWith("tok-")));
        equal(session.copySource(0, bytes(changed), session.token.revision), changed);
        evidence.highlightingParity = true;
      } finally { fresh?.dispose(); f.dispose(); session.dispose(); }
    });
    equal(evidence.browserTextCalls, 0); equal(evidence.fetchCalls, 0);
  } finally { for (const undo of restore.reverse()) undo(); }
  return { passed: results.filter(result => result.pass).length, failed: results.filter(result => !result.pass).length, evidence, results };
}
