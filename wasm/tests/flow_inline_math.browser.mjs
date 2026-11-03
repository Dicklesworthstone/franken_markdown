// Real Chromium over unchanged native BrowserFlowSession pages and actual
// bundled glyph outlines. Optional runtime cases execute generated WASM through
// public createFlowSession. Synthetic malformed/clip controls are labeled.
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { FlowReaderView, readFlowDocument, sourceSpanToUtf16 } from "../flow-reader.js";
import { FlowError } from "../flow_session.mjs";

const assert = (value, message = "assertion failed") => { if (!value) throw Error(message); };
const equal = (a, b, message = "values differ") => assert(JSON.stringify(a) === JSON.stringify(b), message);
async function rejects(promise, code) {
  try { await promise; } catch (error) { equal(error.code, code, `expected ${code}, got ${error.code}`); return; }
  throw Error(`expected ${code}`);
}
const deferred = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const bytes = text => new TextEncoder().encode(text).length;
const size = { width: 500, height: 600 };
const leaves = nodes => nodes.flatMap(node => node.children.length ? leaves(node.children) : [node]);
const mathRuns = nodes => leaves(nodes).flatMap(node => (node.inlineRuns ?? []).filter(run => run.math)
  .map(run => ({ node, run, text: new TextDecoder().decode(new TextEncoder().encode(node.text).slice(run.startByte, run.endByte)) })));

function recordedSession(initial, indexed = true) {
  let edition = initial;
  return {
    disposed: false, supportsViewport: indexed, outlineRequests: 0,
    get token() { return { revision: edition.snapshot.revision, layoutRevision: edition.snapshot.layoutRevision }; },
    get layoutOptions() { return edition.snapshot.options; },
    get source() { return edition.source; },
    adopt(next) { edition = next; },
    snapshot(query) {
      equal(query.token, this.token, "snapshot revision fence");
      assert(query.offset === 0 && query.limit >= edition.snapshot.total && query.glyphs, "fixture must fit one unchanged page");
      return structuredClone(edition.snapshot);
    },
    viewport(query) {
      equal(query.token, this.token, "viewport revision fence");
      equal({ ...query.viewport, afterIndex: query.afterIndex, limit: query.limit, glyphs: query.glyphs },
        edition.viewportRequest, "query must match native viewport fixture");
      return structuredClone(edition.viewport);
    },
    readingOrder(query) {
      equal(query.token, this.token, "reading revision fence");
      assert(query.offset === 0 && query.limit >= edition.reading.total, "reading fixture must fit one page");
      return structuredClone(edition.reading);
    },
    glyphOutlines(fontId, ids) {
      this.outlineRequests++;
      const batch = edition.glyphOutlines.find(batch => batch.fontId === fontId);
      assert(batch, "only actual native font outlines may be requested");
      return structuredClone({ ...batch, glyphs: ids.map(id => {
        const glyph = batch.glyphs.find(glyph => glyph.glyphId === id);
        assert(glyph, "every requested outline must exist in native inventory"); return glyph;
      }) });
    },
    hitTest(x, y, shown) {
      if (shown.revision !== this.token.revision) throw new FlowError("STALE_REVISION", "source changed");
      if (shown.layoutRevision !== this.token.layoutRevision) throw new FlowError("STALE_LAYOUT", "layout changed");
      const hit = edition.hits.find(hit => hit.x === x && hit.y === y); assert(hit, "query must be an actual native hit");
      return structuredClone(hit.response);
    },
  };
}
function painter() {
  const canvas = document.createElement("canvas"); document.body.append(canvas);
  const renderer = new FlowCanvasRenderer(canvas);
  return { canvas, renderer, dispose() { renderer.dispose(); canvas.remove(); } };
}
function reader(options) {
  const root = document.createElement("article"); document.body.append(root);
  const view = new FlowReaderView(root, options);
  return { root, view, dispose() { view.dispose(); root.remove(); document.getSelection().removeAllRanges(); } };
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
function baselineTranslations(items, trace) {
  const drawn = trace.filter(row => row[0] === "translate"); let offset = 0, explicit = 0;
  for (const item of items) {
    if (item.kind !== "text") continue;
    const glyphs = item.fontRun.glyphs;
    if (item.baselineY !== undefined && item.baselineY !== null) {
      const expected = new Array(glyphs.length);
      for (const cluster of item.fontRun.clusters) {
        let advance = 0;
        for (let i = cluster.glyphs[0]; i < cluster.glyphs[1]; i++) {
          const glyph = glyphs[i];
          expected[i] = ["translate", item.bounds.x + cluster.xStart + advance + glyph.xOffset,
            item.baselineY - glyph.yOffset]; advance += glyph.xAdvance;
        }
      }
      equal(drawn.slice(offset, offset + glyphs.length), expected, "every native baseline and glyph offset must be exact");
      explicit += glyphs.length;
    }
    offset += glyphs.length;
  }
  equal(drawn.length, offset, "all actual text glyphs are drawn exactly once"); assert(explicit > 0); return explicit;
}

export async function run(fixture, runtime = null) {
  assert(fixture?.schema === "fmd-flow-inline-math-native-v1", "actual native inline-math fixture required");
  const results = [], restores = [], evidence = {}; let trace = null;
  for (const prototype of [CanvasRenderingContext2D.prototype, OffscreenCanvasRenderingContext2D.prototype])
    for (const name of ["fillText", "measureText", "translate", "scale", "setTransform", "rect",
      "moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "closePath", "fill"]) {
      const original = prototype[name];
      prototype[name] = function (...args) {
        if (name === "fillText" || name === "measureText") throw Error(`browser text operation ${name} is forbidden`);
        if (trace) trace.push([name, ...args]); return original.apply(this, args);
      };
      restores.push(() => { prototype[name] = original; });
    }
  const test = async (name, action) => {
    try { await action(); results.push({ name, pass: true }); }
    catch (error) { trace = null; results.push({ name, pass: false, error: String(error.stack ?? error) }); }
  };
  try {
    await test("native mixed source supplies real paths, font outlines, baselines, styles and fallback diagnostics", () => {
      for (const edition of [fixture.original, fixture.updated]) {
        const items = edition.snapshot.items;
        assert(items.length < 256 && items.some(item => item.kind === "path") && items.some(item => item.kind === "text"));
        assert(items.some(item => item.kind === "path" && item.commands.some(command => command[0] === "Q")));
        assert(items.some(item => item.kind === "text" && Number.isFinite(item.baselineY)));
        assert(edition.glyphOutlines.length && edition.glyphOutlines.some(batch => batch.glyphs.some(glyph => glyph.commands.length)));
        equal(edition.snapshot.sourceLengthBytes, bytes(edition.source)); equal(edition.snapshot.sourceLengthUtf16, edition.source.length);
        equal(edition.snapshot.pendingAssetCount, 0); assert(mathRuns(edition.reading.nodes).length > 0);
      }
      const runs = mathRuns(fixture.original.reading.nodes);
      assert(runs.some(({ run }) => run.style.bold)); assert(runs.some(({ run }) => run.link?.activeTarget === "/guide"));
      assert(runs.some(({ node }) => node.role === "table-cell"));
      assert(fixture.original.snapshot.items.some(item => typeof item.mathFallback === "string" && item.mathFallback.length));
      assert(BigInt(fixture.updated.snapshot.revision) > BigInt(fixture.original.snapshot.revision));
      evidence.nativePaths = fixture.original.snapshot.items.filter(item => item.kind === "path").length;
      evidence.nativeMathRuns = runs.length;
    });

    await test("native indexed Canvas draws actual mixed-line baselines and formula colors without browser fonts", async () => {
      const f = painter(), session = recordedSession(fixture.original); let images = 0;
      try {
        trace = []; const frame = await f.renderer.render(session, { ...size, resolveImage() { images++; throw Error("unexpected image"); } });
        const drawn = trace; trace = null;
        evidence.explicitBaselineGlyphs = baselineTranslations(fixture.original.viewport.items, drawn);
        assert(session.outlineRequests > 0, "prose must use actual bundled outlines"); equal(images, 0); equal(frame.missingImages, 0);
        equal(frame.queryMode, "indexed-viewport"); assert(ink(pixels(f.canvas)) > 100); assert(ink(pixels(f.canvas), red) > 5);
        evidence.nativeGlyphs = frame.glyphs;
      } finally { trace = null; f.dispose(); }
    });

    await test("native snapshot and indexed paths preserve identical mixed text/formula pixels and clips", async () => {
      const a = painter(), b = painter();
      try {
        await a.renderer.render(recordedSession(fixture.original, false), size);
        await b.renderer.render(recordedSession(fixture.original), size);
        assert(samePixels(pixels(a.canvas), pixels(b.canvas)), "query mode must not move baselines or formula ink");
      } finally { a.dispose(); b.dispose(); }
    });

    await test("native unsupported inline math exposes a bounded reason and complete literal source", async () => {
      const formula = String.raw`\notARealMathCommand{x}`;
      const fallback = fixture.original.snapshot.items.filter(item => item.mathFallback !== undefined);
      assert(fallback.length && fallback.every(item => item.kind === "text" && [...item.mathFallback].length <= 160));
      equal(fallback.map(item => item.text).join(""), formula, "source fallback cannot truncate or replace TeX");
      assert(mathRuns(fixture.original.reading.nodes).some(run => run.text === formula));
      assert(leaves(fixture.original.reading.nodes).some(node => (node.inlineRuns ?? []).some(run => run.style.code && !run.math)),
        "literal code keeps ordinary code semantics");
    });

    await test("native reader preserves exact inline TeX inside bold, linked and table text with continuous Copy", async () => {
      const f = reader(), session = recordedSession(fixture.original);
      try {
        const reading = await readFlowDocument(session); await f.view.renderAsync(reading);
        const spans = [...f.root.querySelectorAll('span[role="math"]')], expected = mathRuns(fixture.original.reading.nodes);
        equal(spans.map(span => span.textContent), expected.map(run => run.text));
        spans.forEach((span, index) => {
          equal(getComputedStyle(span).whiteSpace, "pre-wrap");
          if (expected[index].run.style.bold) assert(span.closest("strong"));
          if (expected[index].node.role === "table-cell") assert(span.closest("td,th"));
          const source = sourceSpanToUtf16(fixture.original.source, expected[index].node.enclosingSourceSpan);
          assert(fixture.original.source.slice(source.start, source.end).includes(span.textContent));
        });
        const node = reading.nodes.find(node => node.text.startsWith("Before")); assert(node);
        const match = reading.find(node.text).matches[0]; assert(match);
        equal(f.view.selectMatch(match), node.text); equal(document.getSelection().toString(), node.text);
        equal(document.getSelection().getRangeAt(0).toString(), node.text);
        assert(!f.root.querySelector("img,script,svg,math,iframe"), "TeX must remain text");
      } finally { f.dispose(); }
    });

    await test("ordinary prose, headings, lists, quotes and cells preserve repeated spaces and hard breaks in actual Copy", async () => {
      // Synthetic AST-text control isolates whitespace fidelity independently
      // of formulas. Code retains its existing pre/code whitespace policy.
      const raw = "a  b\nc";
      for (const role of ["paragraph", "heading", "list-item", "blockquote", "table-cell", "table-header-cell", "code-block"]) {
        const edition = structuredClone(fixture.original), f = reader();
        const node = { ...leaves(edition.reading.nodes)[0], role, text: raw, children: [], inlineRuns: [], ...(role === "heading" ? { level: 2 } : {}) };
        if (role === "list-item") node.listPath = [{ listId: "1", ordered: false, start: "1", itemIndex: 0, task: null }];
        edition.reading.nodes = [role.startsWith("table-") ? { ...node,
          role: role === "table-header-cell" ? "table-header-row" : "table-row", children: [node] } : node];
        edition.reading.total = 1; edition.reading.nextOffset = null;
        try {
          const reading = await readFlowDocument(recordedSession(edition)); await f.view.renderAsync(reading);
          const match = reading.find(raw).matches[0]; equal(f.view.selectMatch(match), raw);
          equal(document.getSelection().toString(), raw, `${role} Copy must retain admitted text exactly`);
          const textRoot = f.root.querySelector(`[data-flow-node="${match.nodeIndex}"]`);
          if (role === "code-block") {
            equal(textRoot.tagName, "PRE"); equal(textRoot.style.whiteSpace, ""); equal(textRoot.querySelector("code").style.whiteSpace, "");
          } else equal(getComputedStyle(textRoot).whiteSpace, "pre-wrap");
        } finally { f.dispose(); }
      }
    });

    await test("linked formula uses one inert host-controlled target and rejects activation after source replacement", async () => {
      const calls = [], f = reader({ onLink(value) { calls.push(value); } }), session = recordedSession(fixture.original);
      try {
        const reading = await readFlowDocument(session); f.view.render(reading);
        const anchor = [...f.root.querySelectorAll('a[role="link"]')].find(anchor => anchor.querySelector('[role="math"]'));
        assert(anchor && !anchor.hasAttribute("href")); anchor.click(); equal(calls.length, 1); equal(calls[0].target, "/guide");
        equal(anchor.textContent, String.raw`\sqrt{x^2+1}`);
        anchor.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); equal(calls.length, 2);
        session.adopt(fixture.updated); anchor.click(); equal(calls.length, 2);
        await rejects(Promise.resolve().then(() => reading.assertCurrent()), "STALE_REVISION");
      } finally { f.dispose(); }
    });

    await test("native formula hit retains its source envelope and refuses a stale painted token", async () => {
      const f = painter(), session = recordedSession(fixture.original), hit = fixture.original.hits[0];
      try {
        await f.renderer.render(session, size); equal(await f.renderer.hitTest(hit.x, hit.y), hit.response);
        session.adopt(fixture.updated); await rejects(f.renderer.hitTest(hit.x, hit.y), "STALE_REVISION");
      } finally { f.dispose(); }
    });

    await test("scrolling changes only camera and clip coordinates while every native glyph/path argument remains exact", async () => {
      const f = painter(), session = recordedSession(fixture.original, false);
      try {
        trace = []; await f.renderer.render(session, size); const initial = trace;
        trace = []; await f.renderer.render(session, { ...size, scrollX: 3, scrollY: 1, pixelRatio: 2 }); const scrolled = trace; trace = null;
        const geometry = rows => rows.filter(row => !["setTransform", "rect"].includes(row[0]));
        equal(geometry(initial), geometry(scrolled), "scroll/DPR must not rebase or reshape native geometry");
        equal(scrolled.filter(row => row[0] === "setTransform"), [["setTransform", 2, 0, 0, 2, -6, -2]]);
        assert(scrolled.filter(row => row[0] === "rect").every(row => row[1] >= 3 && row[2] >= 1 && row[1] + row[3] <= 503 && row[2] + row[4] <= 601));
        baselineTranslations(fixture.original.snapshot.items, scrolled);
      } finally { trace = null; f.dispose(); }
    });

    await test("one-pixel wrong baseline is detected by the native placement oracle and changes visible ink", async () => {
      // Synthetic consumer negative: alter exactly one valid native baseline,
      // retaining all actual contours. The unchanged native item is the oracle.
      const altered = structuredClone(fixture.original), target = altered.snapshot.items.find(item =>
        item.kind === "text" && Number.isFinite(item.baselineY) && item.fontRun.glyphs.length && item.baselineY + 1 < item.bounds.y + item.bounds.height);
      assert(target); target.baselineY += 1;
      const a = painter(), b = painter();
      try {
        await a.renderer.render(recordedSession(fixture.original, false), size);
        trace = []; await b.renderer.render(recordedSession(altered, false), size); const drawn = trace; trace = null;
        let rejected = false; try { baselineTranslations(fixture.original.snapshot.items, drawn); } catch { rejected = true; }
        assert(rejected, "native oracle must reject a one-pixel error");
        const before = pixels(a.canvas), after = pixels(b.canvas);
        evidence.wrongBaselineChangedChannels = after.reduce((count, value, index) => count + (value !== before[index]), 0);
        assert(evidence.wrongBaselineChangedChannels > 0, "control must change actual text ink");
      } finally { trace = null; a.dispose(); b.dispose(); }
    });

    await test("invalid explicit baseline and malformed fallback reason cannot replace the last complete frame", async () => {
      const f = painter();
      try {
        await f.renderer.render(recordedSession(fixture.original, false), size); const before = pixels(f.canvas), frame = f.renderer.frame;
        for (const change of [item => { item.baselineY = item.bounds.y - 1; }, item => { item.baselineY = NaN; },
          item => { item.mathFallback = "x".repeat(161); }, item => { item.mathFallback = "bad\ud800"; }]) {
          const bad = structuredClone(fixture.original); change(bad.snapshot.items.find(item => item.kind === "text"));
          await rejects(f.renderer.render(recordedSession(bad, false), size), "INVALID_WASM_RESPONSE");
          assert(f.renderer.frame === frame && samePixels(before, pixels(f.canvas)));
        }
      } finally { f.dispose(); }
    });

    await test("late old native payload cannot publish after source replacement and a fresh retry succeeds", async () => {
      const f = painter(), session = recordedSession(fixture.original, false), gate = deferred();
      try {
        await f.renderer.render(session, size); const before = pixels(f.canvas), frame = f.renderer.frame;
        const snapshot = session.snapshot.bind(session); session.snapshot = query => { const page = snapshot(query); return gate.promise.then(() => page); };
        const pending = f.renderer.render(session, size); session.adopt(fixture.updated); gate.resolve();
        await rejects(pending, "STALE_LAYOUT"); assert(f.renderer.frame === frame && samePixels(before, pixels(f.canvas)));
        session.snapshot = snapshot; await f.renderer.render(session, size);
        equal(f.renderer.frame.revision, fixture.updated.snapshot.revision); assert(ink(pixels(f.canvas), blue) > 5 && ink(pixels(f.canvas), red) === 0);
      } finally { gate.resolve(); f.dispose(); }
    });

    await test("literal hostile TeX remains selectable text and math annotations do not bypass safe-link admission", async () => {
      // Synthetic consumer-boundary case, kept separate from native evidence.
      const edition = structuredClone(fixture.original), f = reader();
      const raw = '  \\bad{<img src="https://invalid.example/x" onerror="window.mathRan=1">}\n  \\frac{ 1 }{ 2 }  ';
      const node = { ...leaves(edition.reading.nodes)[0], text: raw, children: [], inlineRuns: [{ startByte: 0, endByte: bytes(raw),
        style: { bold: false, italic: false, code: false, strikethrough: false }, link: null, math: "inline" }] };
      edition.reading.nodes = [node]; edition.reading.total = 1; edition.reading.nextOffset = null;
      try {
        const reading = await readFlowDocument(recordedSession(edition)); f.view.render(reading);
        equal(f.root.querySelector('[role="math"]').textContent, raw); equal(f.view.selectMatch(reading.find(raw).matches[0]), raw);
        equal(document.getSelection().toString(), raw, "math whitespace and multiline source must survive ordinary Copy");
        assert(!f.root.querySelector("img,script,svg,iframe") && !globalThis.mathRan);
        node.inlineRuns[0].link = { target: "javascript:alert(1)", activeTarget: "javascript:alert(1)" };
        await rejects(readFlowDocument(recordedSession(edition)), "INVALID_READING_DATA");
      } finally { f.dispose(); }
    });

    if (runtime) {
      const create = source => runtime.createFlowSession(source, { font: "sans", ...fixture.original.snapshot.options });
      const snapshot = session => session.snapshot({ offset: 0, limit: 2048, glyphs: true, token: session.token });
      const geometry = page => ({ items: page.items, totalBounds: page.totalBounds });
      const monitor = session => {
        const calls = { glyphOutlines: 0, fontBytes: 0, assetBytes: 0, images: 0 }, watched = Object.create(session);
        for (const name of ["glyphOutlines", "fontBytes", "assetBytes"])
          Object.defineProperty(watched, name, { value(...args) { calls[name]++; return session[name](...args); } });
        return { session: watched, calls, options: { ...size, resolveImage() { calls.images++; throw Error("unexpected image"); } } };
      };
      const noExternal = calls => { assert(calls.glyphOutlines > 0); equal(calls.fontBytes, 0); equal(calls.assetBytes, 0); equal(calls.images, 0); };

      await test("generated WASM public session matches actual native mixed geometry, baselines, reading and outlines", async () => {
        const s = await create(fixture.original.source);
        try {
          equal(geometry(snapshot(s)), geometry(fixture.original.snapshot), "native/WASM mixed geometry must agree");
          const { x, y, width, height, ...query } = fixture.original.viewportRequest;
          equal(s.viewport({ viewport: { x, y, width, height }, ...query, token: s.token }), fixture.original.viewport);
          equal(s.readingOrder({ offset: 0, limit: 2048, token: s.token }), fixture.original.reading);
          for (const batch of fixture.original.glyphOutlines)
            equal(s.glyphOutlines(batch.fontId, batch.glyphs.map(glyph => glyph.glyphId)), batch);
          equal(s.copySource(0, bytes(s.source), s.token.revision), fixture.original.source); equal(s.pendingAssets().requests, []);
          evidence.wasmNativeGeometryEqual = true;
        } finally { s.dispose(); }
      });

      await test("generated WASM paints identical native pixels and exact inline semantics using bundled outlines only", async () => {
        const s = await create(fixture.original.source), watched = monitor(s), f = painter(), reference = painter(), r = reader();
        try {
          await f.renderer.render(watched.session, watched.options); await reference.renderer.render(recordedSession(fixture.original), size);
          assert(samePixels(pixels(f.canvas), pixels(reference.canvas)));
          const document = await readFlowDocument(s); r.view.render(document);
          equal([...r.root.querySelectorAll('span[role="math"]')].map(span => span.textContent), mathRuns(fixture.original.reading.nodes).map(run => run.text));
          const formula = String.raw`\sqrt{x^2+1}`; equal(r.view.selectMatch(document.find(formula).matches[0]), formula);
          noExternal(watched.calls);
        } finally { r.dispose(); f.dispose(); reference.dispose(); s.dispose(); }
      });

      await test("generated WASM source replacement, range edit and narrow reflow retain source and fresh geometry", async () => {
        const s = await create(fixture.original.source), watched = monitor(s), f = painter(); let fresh;
        try {
          const old = s.token, oldReading = await readFlowDocument(s);
          s.replaceSource(fixture.updated.source, { expectedRevision: old.revision });
          equal(geometry(snapshot(s)), geometry(fixture.updated.snapshot));
          await rejects(Promise.resolve().then(() => s.snapshot({ token: old })), "STALE_REVISION");
          await rejects(Promise.resolve().then(() => oldReading.assertCurrent()), "STALE_REVISION");
          await f.renderer.render(watched.session, watched.options); assert(ink(pixels(f.canvas), blue) > 5 && ink(pixels(f.canvas), red) === 0);
          const at = s.source.indexOf("a^2+b^2"); assert(at >= 0); s.edit(at, at + 7, "d^3+e^3", { expectedRevision: s.token.revision });
          const edited = fixture.updated.source.replace("a^2+b^2", "d^3+e^3"); equal(s.source, edited); fresh = await create(edited);
          equal(geometry(snapshot(s)), geometry(snapshot(fresh)), "range edit must match fresh real layout");
          const before = s.token; s.reflow({ viewportWidth: 100 }, before);
          equal(s.token.revision, before.revision); assert(s.token.layoutRevision !== before.layoutRevision); equal(s.source, edited);
          await rejects(Promise.resolve().then(() => s.snapshot({ token: before })), "STALE_LAYOUT");
          assert(snapshot(s).items.some(item => item.kind === "path"), "narrow reflow keeps actual formula geometry");
          await f.renderer.render(watched.session, { ...watched.options, width: 100 }); assert(ink(pixels(f.canvas), blue) > 0);
          s.reflow(fixture.original.snapshot.options, s.token); equal(geometry(snapshot(s)), geometry(snapshot(fresh)));
          equal(s.copySource(0, bytes(edited), s.token.revision), edited); noExternal(watched.calls);
        } finally { fresh?.dispose(); f.dispose(); s.dispose(); }
      });

      await test("generated WASM fresh replacement supersedes delayed old mixed-line publication", async () => {
        const s = await create(fixture.original.source), watched = monitor(s), delayed = Object.create(s), gate = deferred(), f = painter();
        Object.defineProperties(delayed, { supportsViewport: { value: false }, snapshot: { value(query) {
          const page = s.snapshot(query); return gate.promise.then(() => page);
        } } });
        try {
          const old = f.renderer.render(delayed, size), refused = rejects(old, "RENDER_SUPERSEDED");
          s.replaceSource(fixture.updated.source, { expectedRevision: s.token.revision });
          await f.renderer.render(watched.session, watched.options); const shown = pixels(f.canvas);
          await refused; gate.resolve(); await new Promise(resolve => setTimeout(resolve, 0));
          equal(f.renderer.frame.revision, s.token.revision); assert(samePixels(shown, pixels(f.canvas))); noExternal(watched.calls);
        } finally { gate.resolve(); f.dispose(); s.dispose(); }
      });
    }
  } finally { trace = null; for (const restore of restores) restore(); }
  return { passed: results.filter(result => result.pass).length, failed: results.filter(result => !result.pass).length, evidence, results };
}
