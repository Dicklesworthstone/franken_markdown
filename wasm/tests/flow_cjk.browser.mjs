// Real CJK contours and source ownership from native BrowserFlowSession JSON.
// Generated-runtime cases use actual enabled/lean WASM; the optional old runtime
// is an ordinary Latin pixel oracle, never a substitute CJK implementation.
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { FlowReaderView, readFlowDocument, sourceSpanToUtf16 } from "../flow-reader.js";
import { FlowError } from "../flow_session.mjs";

const assert = (value, message = "assertion failed") => { if (!value) throw Error(message); };
const equal = (a, b, message = "values differ") => assert(JSON.stringify(a) === JSON.stringify(b), message);
async function rejects(promise, code) {
  try { await promise; } catch (error) { equal(error.code, code, `expected ${code}, got ${error.code}`); return; }
  throw Error(`expected ${code}`);
}
const bytes = value => new TextEncoder().encode(value).length;
const size = { width: 400, height: 600 };
const cjk = /[\u3040-\u30ff\u3400-\u9fff]/u;
const geometry = page => ({ items: page.items, totalBounds: page.totalBounds });
const snapshot = session => session.snapshot({ offset: 0, limit: 2048, glyphs: true, token: session.token });
const LATIN_CONTROLS = [
  "A simple café paragraph with punctuation, digits 123 and descenders gyp.",
  "# Heading\n\nPlain **bold** and *italic*, ~~strike~~ and [a link](#heading).  \nA second line with `code`.",
  "```rust\nlet value = 123;\n// café\nprintln!(\"hello\");\n```\n\nAfter code.",
];

function recordedSession(initial, indexed = true) {
  let edition = initial;
  return {
    disposed: false, supportsViewport: indexed, outlineRequests: 0,
    get token() { return { revision: edition.snapshot.revision, layoutRevision: edition.snapshot.layoutRevision }; },
    get source() { return edition.source; },
    get layoutOptions() { return edition.snapshot.options; },
    adopt(next) { edition = next; },
    snapshot(query) {
      equal(query.token, this.token); assert(query.offset === 0 && query.limit >= edition.snapshot.total && query.glyphs);
      return structuredClone(edition.snapshot);
    },
    viewport(query) {
      equal(query.token, this.token);
      equal({ ...query.viewport, afterIndex: query.afterIndex, limit: query.limit, glyphs: query.glyphs }, edition.viewportRequest,
        "query must match the actual native viewport");
      return structuredClone(edition.viewport);
    },
    readingOrder(query) {
      equal(query.token, this.token); assert(query.offset === 0 && query.limit >= edition.reading.total);
      return structuredClone(edition.reading);
    },
    glyphOutlines(fontId, ids) {
      this.outlineRequests++;
      const batch = edition.glyphOutlines.find(batch => batch.fontId === fontId); assert(batch, "native font inventory required");
      return structuredClone({ ...batch, glyphs: ids.map(id => {
        const glyph = batch.glyphs.find(glyph => glyph.glyphId === id); assert(glyph, "actual native glyph contour required"); return glyph;
      }) });
    },
    hitTest(x, y, token) {
      if (token.revision !== this.token.revision) throw new FlowError("STALE_REVISION", "source changed");
      if (token.layoutRevision !== this.token.layoutRevision) throw new FlowError("STALE_LAYOUT", "layout changed");
      const entry = edition.hits.find(hit => hit.x === x && hit.y === y); assert(entry, "actual native hit required");
      return structuredClone(entry.response);
    },
  };
}
function painter() {
  const canvas = document.createElement("canvas"); document.body.append(canvas); const renderer = new FlowCanvasRenderer(canvas);
  return { canvas, renderer, dispose() { renderer.dispose(); canvas.remove(); } };
}
function reader(options) {
  const root = document.createElement("article"); document.body.append(root); const view = new FlowReaderView(root, options);
  return { root, view, dispose() { view.dispose(); root.remove(); document.getSelection().removeAllRanges(); } };
}
const pixels = canvas => canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
const samePixels = (a, b) => a.length === b.length && a.every((value, index) => value === b[index]);
function ink(data) {
  let count = 0; for (let i = 0; i < data.length; i += 4) if (data[i + 3] && Math.min(data[i], data[i + 1], data[i + 2]) < 240) count++;
  return count;
}
function monitor(session) {
  const calls = { glyphOutlines: 0, fontBytes: 0, assetBytes: 0, images: 0 }, watched = Object.create(session);
  for (const name of ["glyphOutlines", "fontBytes", "assetBytes"])
    Object.defineProperty(watched, name, { value(...args) { calls[name]++; return session[name](...args); } });
  return { session: watched, calls, options: { ...size, resolveImage() { calls.images++; throw Error("unexpected image request"); } } };
}
function noExternal(calls) {
  assert(calls.glyphOutlines > 0, "Canvas must request the real bundled contours");
  equal(calls.fontBytes, 0); equal(calls.assetBytes, 0); equal(calls.images, 0);
}

// Capture actual raw pixels in a disposable context. Candidate contexts receive
// only these small pixel records, never a second live WebAssembly runtime.
export async function captureLatinBaseline(runtime, layout) {
  const controls = [], restores = [];
  for (const prototype of [CanvasRenderingContext2D.prototype, OffscreenCanvasRenderingContext2D.prototype])
    for (const name of ["fillText", "measureText"]) {
      const original = prototype[name];
      prototype[name] = function () { throw Error("unexpected browser text shaping: " + name); };
      restores.push(() => { prototype[name] = original; });
    }
  try {
    for (const source of LATIN_CONTROLS) {
      const session = await runtime.createFlowSession(source, { font: "sans", ...layout });
      const watched = monitor(session), f = painter();
      try {
        await f.renderer.render(watched.session, watched.options); noExternal(watched.calls);
        const data = pixels(f.canvas), chunks = [];
        for (let i = 0; i < data.length; i += 8192)
          chunks.push(String.fromCharCode(...data.subarray(i, Math.min(i + 8192, data.length))));
        controls.push({ source, width: f.canvas.width, height: f.canvas.height,
          rgbaBase64: btoa(chunks.join("")), readingText: (await readFlowDocument(session)).text,
          sourceText: session.source,
          explicitBaselines: snapshot(session).items.filter(item => item.kind === "text" && item.baselineY !== undefined).length });
      } finally { f.dispose(); session.dispose(); }
    }
  } finally { for (const restore of restores) restore(); }
  return { schema: "fmd-flow-latin-raw-pixels-v1", layout, controls };
}
function referencePixels(record) {
  let binary = atob(record.rgbaBase64); const data = new Uint8Array(binary.length);
  for (let i = 0; i < data.length; i++) data[i] = binary.charCodeAt(i);
  binary = null; return data;
}

export async function run(fixture, runtime = null, baseline = null, mode = "enabled") {
  assert(fixture?.schema === "fmd-flow-cjk-native-v1", "actual CJK native fixture required");
  assert(mode === "enabled" || mode === "lean");
  const results = [], evidence = {}, restores = []; let trace = null;
  const test = async (name, action) => {
    try { await action(); results.push({ name, pass: true }); }
    catch (error) { trace = null; results.push({ name, pass: false, error: String(error.stack ?? error) }); }
  };
  for (const prototype of [CanvasRenderingContext2D.prototype, OffscreenCanvasRenderingContext2D.prototype])
    for (const name of ["fillText", "measureText", "setTransform", "translate", "scale", "moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "closePath", "fill"]) {
      const original = prototype[name];
      prototype[name] = function (...args) {
        if (name === "fillText" || name === "measureText") throw Error(`unexpected browser text shaping: ${name}`);
        if (trace) trace.push([name, ...args]); return original.apply(this, args);
      };
      restores.push(() => { prototype[name] = original; });
    }
  const create = source => runtime.createFlowSession(source, { font: "sans", ...fixture.original.snapshot.options });
  try {
    if (mode === "enabled") {
      await test("native CJK fixture retains BOM/CRLF source and real nonzero bundled fallback glyph identities", () => {
        assert(fixture.original.source.startsWith("\ufeff") && fixture.original.source.includes("\r\n"));
        const fontIds = new Set(); let clusters = 0;
        for (const edition of [fixture.original, fixture.updated]) {
          equal(edition.snapshot.sourceLengthBytes, bytes(edition.source)); equal(edition.snapshot.sourceLengthUtf16, edition.source.length);
          assert(bytes(edition.source) > edition.source.length); equal(edition.snapshot.pendingAssetCount, 0);
          assert(edition.snapshot.items.length < 256 && edition.snapshot.items.some(item => item.kind === "path"));
          for (const item of edition.snapshot.items) {
            if (item.kind !== "text") continue;
            equal(item.fontRun.fontOrigin, "bundled");
            for (const cluster of item.fontRun.clusters) {
              if (!cjk.test(item.text.slice(...cluster.utf16))) continue;
              assert(cluster.fontId !== item.fontRun.fontId, "CJK must use its actual optional fallback face");
              fontIds.add(cluster.fontId); clusters++;
              const batch = edition.glyphOutlines.find(batch => batch.fontId === cluster.fontId); assert(batch);
              for (const glyph of item.fontRun.glyphs.slice(...cluster.glyphs)) {
                assert(glyph.glyphId !== 0, "no notdef substitution");
                assert(batch.glyphs.find(outline => outline.glyphId === glyph.glyphId)?.commands.length, "real CJK ink required");
              }
            }
          }
        }
        assert(clusters > 10 && fontIds.size > 0); evidence.cjkClusters = clusters; evidence.cjkFontIds = [...fontIds];
      });

      await test("native indexed Canvas draws actual CJK outlines with mixed Latin, styles, code and math", async () => {
        const f = painter(), session = recordedSession(fixture.original); let images = 0;
        try {
          const frame = await f.renderer.render(session, { ...size, resolveImage() { images++; throw Error("unexpected image"); } });
          equal(frame.queryMode, "indexed-viewport"); equal(images, 0); equal(frame.missingImages, 0); assert(session.outlineRequests > 1);
          evidence.nativeGlyphs = frame.glyphs; evidence.nativeInkPixels = ink(pixels(f.canvas)); assert(evidence.nativeInkPixels > 1000);
          const heading = fixture.original.reading.nodes.find(node => node.role === "heading"); assert(heading);
          const bounds = heading.bounds;
          assert(ink(f.canvas.getContext("2d").getImageData(0, Math.floor(bounds.y), size.width, Math.ceil(bounds.height)).data) > 100,
            "CJK heading must draw visible actual ink");
        } finally { f.dispose(); }
      });

      await test("native snapshot and indexed viewport paint identical CJK glyphs, text baselines and table clips", async () => {
        const a = painter(), b = painter();
        try {
          await a.renderer.render(recordedSession(fixture.original, false), size); await b.renderer.render(recordedSession(fixture.original), size);
          assert(samePixels(pixels(a.canvas), pixels(b.canvas)));
        } finally { a.dispose(); b.dispose(); }
      });

      await test("native reading preserves exact CJK selection, styles, links, code and original source spans", async () => {
        const calls = [], f = reader({ onLink(value) { calls.push(value); } }), session = recordedSession(fixture.original);
        try {
          const reading = await readFlowDocument(session); await f.view.renderAsync(reading);
          const entries = reading.nodes.filter(entry => !entry.children.length && cjk.test(entry.text));
          assert(entries.length >= 6);
          for (const entry of entries) {
            const match = reading.find(entry.text).matches.find(match => match.nodeIndex === entry.index); assert(match);
            equal(f.view.selectMatch(match), entry.text); equal(document.getSelection().toString(), entry.text);
            const span = sourceSpanToUtf16(fixture.original.source, entry.enclosingSourceSpan);
            assert(cjk.test(fixture.original.source.slice(span.start, span.end)));
          }
          assert(f.root.querySelector("strong")?.textContent.includes("中文")); assert(f.root.querySelector("em")?.textContent.includes("日本語"));
          assert(f.root.querySelector("pre code")?.textContent.includes("中文")); assert(f.root.querySelector('span[role="math"]'));
          const link = f.root.querySelector('a[role="link"]'); assert(link && !link.hasAttribute("href"));
          equal(link.textContent, "リンク"); link.click(); equal(calls.length, 1); equal(calls[0].target, "/guide");
          assert(!f.root.querySelector("img,svg,script,iframe"));
        } finally { f.dispose(); }
      });

      await test("code Copy preserves zero, one and multiple terminal LFs without sentinel text or extra copied characters", async () => {
        // Explicit consumer-boundary control: unchanged native CJK evidence
        // above stays intact; these cases isolate terminal code line boxes.
        for (const raw of ["", "plain", "中文\t=\tカナ", "中文 = カナ\n", "a\tb  c\nd\n", "abc\n\n", "\n", "\n\n"]) {
          const edition = structuredClone(fixture.original), f = reader();
          const source = "\ufeff~~~text\r\n" + raw + "\r\n~~~\r\n";
          const node = edition.reading.nodes.find(node => node.role === "code-block"); assert(node);
          node.text = raw; node.enclosingSourceSpan = { startByte: 3, endByte: bytes(source) };
          edition.source = source; edition.reading.nodes = [node]; edition.reading.total = 1; edition.reading.nextOffset = null;
          try {
            const reading = await readFlowDocument(recordedSession(edition)); await f.view.renderAsync(reading);
            const pre = f.root.querySelector("pre"), code = pre.querySelector("code"); assert(code);
            equal(code.textContent, raw); equal(pre.textContent, raw);
            equal(code.querySelectorAll("br").length, Number(raw.endsWith("\n")));
            for (const br of code.querySelectorAll("br")) equal(br.getAttribute("aria-hidden"), "true");
            equal(code.style.whiteSpace, ""); equal(pre.style.whiteSpace, "", "code keeps its pre/code whitespace policy");
            if (raw.length) {
              equal(f.view.selectMatch(reading.find(raw).matches[0]), raw);
              equal(document.getSelection().toString(), raw, "mapped selection must include every terminal LF");
              equal(document.getSelection().getRangeAt(0).toString(), raw);
            }
            for (const whole of [code, pre]) {
              const range = document.createRange(); range.selectNodeContents(whole);
              document.getSelection().removeAllRanges(); document.getSelection().addRange(range);
              equal(document.getSelection().toString(), raw, "whole-element Copy must not gain a helper newline");
              equal(range.toString(), raw);
            }
            const span = sourceSpanToUtf16(source, reading.nodes[0].enclosingSourceSpan);
            equal(span.start, 1); equal(span.end, source.length); equal(source.slice(span.start, span.end), source.slice(1));
          } finally { f.dispose(); }
        }
      });

      await test("native CJK hit and selection payloads retain actual fragment UTF-16 and enclosing source", async () => {
        const f = painter(), session = recordedSession(fixture.original), hit = fixture.original.hits[0];
        try {
          await f.renderer.render(session, size); equal(await f.renderer.hitTest(hit.x, hit.y), hit.response);
          equal(hit.response.hit.coordinateSpace, "fragment-utf16");
          for (const selection of fixture.original.selections) {
            const item = fixture.original.snapshot.items.find(item => item.index === selection.itemIndex); assert(item?.kind === "text");
            equal(selection.response.coordinateSpace, "fragment-utf16"); equal(selection.response.text, item.text.slice(selection.startUtf16, selection.endUtf16));
            assert(cjk.test(selection.response.text) && selection.response.rectangles.length);
          }
        } finally { f.dispose(); }
      });

      await test("native replacement changes CJK pixels and invalidates previous reading/hit ownership", async () => {
        const f = painter(), session = recordedSession(fixture.original), hit = fixture.original.hits[0];
        try {
          await f.renderer.render(session, size); const before = pixels(f.canvas), reading = await readFlowDocument(session);
          session.adopt(fixture.updated); await rejects(f.renderer.hitTest(hit.x, hit.y), "STALE_REVISION");
          await rejects(Promise.resolve().then(() => reading.assertCurrent()), "STALE_REVISION");
          await f.renderer.render(session, size); equal(f.renderer.frame.revision, fixture.updated.snapshot.revision);
          assert(!samePixels(before, pixels(f.canvas)) && ink(pixels(f.canvas)) > 100);
        } finally { f.dispose(); }
      });

      await test("scroll and DPR preserve every native CJK outline and placement argument", async () => {
        const f = painter(), session = recordedSession(fixture.original, false);
        try {
          trace = []; await f.renderer.render(session, size); const before = trace;
          trace = []; await f.renderer.render(session, { ...size, scrollX: 3, scrollY: 1, pixelRatio: 2 }); const after = trace; trace = null;
          equal(after.filter(row => row[0] === "setTransform"), [["setTransform", 2, 0, 0, 2, -6, -2]]);
          equal(after.filter(row => row[0] !== "setTransform"), before.filter(row => row[0] !== "setTransform"));
        } finally { trace = null; f.dispose(); }
      });

      if (runtime) {
        await test("enabled WASM matches native CJK snapshot, viewport, reading, outlines, carets and source bytes", async () => {
          const s = await create(fixture.original.source);
          try {
            equal(geometry(snapshot(s)), geometry(fixture.original.snapshot));
            const { x, y, width, height, ...query } = fixture.original.viewportRequest;
            equal(s.viewport({ viewport: { x, y, width, height }, ...query, token: s.token }), fixture.original.viewport);
            equal(s.readingOrder({ offset: 0, limit: 2048, token: s.token }), fixture.original.reading);
            for (const batch of fixture.original.glyphOutlines) equal(s.glyphOutlines(batch.fontId, batch.glyphs.map(glyph => glyph.glyphId)), batch);
            for (const hit of fixture.original.hits) equal(s.hitTest(hit.x, hit.y, s.token), hit.response);
            for (const entry of fixture.original.selections) equal(s.selectText(entry.itemIndex, entry.startUtf16, entry.endUtf16, s.token), entry.response);
            equal(s.source, fixture.original.source); equal(s.copySource(0, bytes(s.source), s.token.revision), fixture.original.source);
            const start = bytes(s.source.slice(0, s.source.indexOf("中")));
            await rejects(Promise.resolve().then(() => s.copySource(start + 1, start + 3, s.token.revision)), "INVALID_SELECTION");
            equal(s.pendingAssets().requests, []); evidence.wasmNativeGeometryEqual = true;
          } finally { s.dispose(); }
        });

        await test("enabled WASM CJK pixels and exact semantic Copy match native using bundled contours only", async () => {
          const s = await create(fixture.original.source), watched = monitor(s), f = painter(), reference = painter(), r = reader();
          try {
            await f.renderer.render(watched.session, watched.options); await reference.renderer.render(recordedSession(fixture.original), size);
            assert(samePixels(pixels(f.canvas), pixels(reference.canvas))); noExternal(watched.calls);
            const reading = await readFlowDocument(s); r.view.render(reading);
            const match = reading.find("English 中文 and 日本語 with リンク and 代码.").matches[0]; assert(match);
            equal(r.view.selectMatch(match), "English 中文 and 日本語 with リンク and 代码.");
            equal(document.getSelection().toString(), "English 中文 and 日本語 with リンク and 代码.");
          } finally { r.dispose(); f.dispose(); reference.dispose(); s.dispose(); }
        });

        await test("enabled WASM CJK replacement, UTF-16 range edit and narrow wrapping agree with fresh layout", async () => {
          const s = await create(fixture.original.source), f = painter(), watched = monitor(s); let fresh;
          try {
            const original = s.token; s.replaceSource(fixture.updated.source, { expectedRevision: original.revision });
            equal(geometry(snapshot(s)), geometry(fixture.updated.snapshot));
            await rejects(Promise.resolve().then(() => s.snapshot({ token: original })), "STALE_REVISION");
            const at = s.source.indexOf("かな"); assert(at >= 0 && bytes(s.source.slice(0, at)) !== at);
            const edited = s.source.slice(0, at) + "東京" + s.source.slice(at + 2);
            s.edit(at, at + 2, "東京", { expectedRevision: s.token.revision }); equal(s.source, edited); fresh = await create(edited);
            equal(geometry(snapshot(s)), geometry(snapshot(fresh)));
            const before = s.token, wide = snapshot(s); s.reflow({ viewportWidth: 90 }, before);
            equal(s.token.revision, before.revision); assert(s.token.layoutRevision !== before.layoutRevision);
            await rejects(Promise.resolve().then(() => s.snapshot({ token: before })), "STALE_LAYOUT");
            const narrow = snapshot(s); assert(narrow.totalBounds.height > wide.totalBounds.height);
            assert(narrow.items.every(item => item.bounds.x >= -0.001 && item.bounds.x + item.bounds.width <= 90.001), "wrapped CJK and formulas fit the column");
            equal(s.copySource(0, bytes(edited), s.token.revision), edited);
            await f.renderer.render(watched.session, { ...watched.options, width: 90 }); assert(ink(pixels(f.canvas)) > 100); noExternal(watched.calls);
            s.reflow(fixture.original.snapshot.options, s.token); equal(geometry(snapshot(s)), geometry(snapshot(fresh)));
          } finally { fresh?.dispose(); f.dispose(); s.dispose(); }
        });

        await test("enabled CJK does not silently accept unsupported bidi, variation selectors or combining kana", async () => {
          const s = await create(fixture.updated.source);
          try {
            const before = snapshot(s), token = s.token, source = s.source;
            for (const invalid of ["אב", "中\ufe00", "か\u3099", "😀"])
              await rejects(Promise.resolve().then(() => s.replaceSource(invalid, { expectedRevision: token.revision })), "LAYOUT_ERROR");
            equal(s.token, token); equal(s.source, source); equal(geometry(snapshot(s)), geometry(before));
            s.replaceSource("中文 and 日本語", { expectedRevision: token.revision }); equal(s.source, "中文 and 日本語");
            assert(snapshot(s).items.some(item => item.kind === "text" && cjk.test(item.text)));
          } finally { s.dispose(); }
        });
      }
    } else {
      assert(runtime, "lean phase requires an actual generated feature-off runtime");
      await test("lean WASM explicitly refuses optional CJK with LAYOUT_ERROR", async () => {
        for (const source of [fixture.original.source, "中文", "日本語", "かなカナ"])
          await rejects(create(source), "LAYOUT_ERROR");
        evidence.featureOffRefused = true;
      });
      await test("lean WASM preserves its successful Latin source, token and pixels after refused CJK edits", async () => {
        const s = await create("café and **plain** text."), f = painter(), watched = monitor(s);
        try {
          const before = snapshot(s), token = s.token, source = s.source;
          await f.renderer.render(watched.session, watched.options); const shown = pixels(f.canvas);
          await rejects(Promise.resolve().then(() => s.replaceSource("中文", { expectedRevision: token.revision })), "LAYOUT_ERROR");
          await rejects(Promise.resolve().then(() => s.edit(0, 0, "日本語", { expectedRevision: token.revision })), "LAYOUT_ERROR");
          equal(s.source, source); equal(s.token, token); equal(geometry(snapshot(s)), geometry(before));
          await f.renderer.render(watched.session, watched.options); assert(samePixels(shown, pixels(f.canvas))); noExternal(watched.calls);
          s.replaceSource("Fresh Latin text.", { expectedRevision: token.revision }); equal(s.source, "Fresh Latin text.");
        } finally { f.dispose(); s.dispose(); }
      });
    }

    if (runtime && baseline) await test(`${mode} ordinary Latin pixels remain identical to the actual pre-CJK WASM package`, async () => {
      equal(baseline.schema, "fmd-flow-latin-raw-pixels-v1");
      equal(baseline.layout, fixture.original.snapshot.options);
      equal(baseline.controls.map(record => record.source), LATIN_CONTROLS, "all original Latin controls must be retained");
      let oldExplicit = 0, newExplicit = 0;
      for (const reference of baseline.controls) {
        const current = await runtime.createFlowSession(reference.source, { font: "sans", ...fixture.original.snapshot.options });
        const f = painter();
        try {
          oldExplicit += reference.explicitBaselines;
          newExplicit += snapshot(current).items.filter(item => item.kind === "text" && item.baselineY !== undefined).length;
          await f.renderer.render(current, size);
          equal([f.canvas.width, f.canvas.height], [reference.width, reference.height]);
          const expected = referencePixels(reference); equal(expected.length, reference.width * reference.height * 4);
          assert(samePixels(expected, pixels(f.canvas)), "ordinary Latin pixels changed: " + JSON.stringify(reference.source));
          equal(reference.readingText, (await readFlowDocument(current)).text); equal(reference.sourceText, current.source);
        } finally { f.dispose(); current.dispose(); }
      }
      assert(newExplicit > oldExplicit, "control must exercise new explicit baselines against old implicit ones");
      evidence.latinParity = { controls: LATIN_CONTROLS.length, oldExplicitBaselines: oldExplicit, newExplicitBaselines: newExplicit, exactPixels: true };
    });
  } finally { trace = null; for (const restore of restores) restore(); }
  return { passed: results.filter(result => result.pass).length, failed: results.filter(result => !result.pass).length, evidence, results };
}
