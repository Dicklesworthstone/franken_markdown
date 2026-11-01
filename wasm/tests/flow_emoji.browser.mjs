// Real native fixture, actual generated WASM, and real Chromium Canvas/Copy.
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { FlowReaderView, readFlowDocument, sourceSpanToUtf16 } from "../flow-reader.js";

const assert = (value, message = "assertion failed") => { if (!value) throw Error(message); };
const equal = (a, b, message = "values differ") => assert(JSON.stringify(a) === JSON.stringify(b), message);
const bytes = value => new TextEncoder().encode(value).length;
const snapshot = session => session.snapshot({ offset: 0, limit: 2048, glyphs: true, token: session.token });
const geometry = page => ({ items: page.items, totalBounds: page.totalBounds });
const size = { width: 400, height: 600 };
async function rejects(action, code) {
  try { await action(); } catch (error) { equal(error.code, code, `expected ${code}, got ${error.code}`); return; }
  throw Error(`expected ${code}`);
}
function recorded(edition) {
  return {
    disposed: false, supportsViewport: true,
    get token() { return { revision: edition.snapshot.revision, layoutRevision: edition.snapshot.layoutRevision }; },
    get source() { return edition.source; },
    get layoutOptions() { return edition.snapshot.options; },
    snapshot(query) {
      equal(query.token, this.token); assert(query.offset === 0 && query.limit >= edition.snapshot.total && query.glyphs);
      return structuredClone(edition.snapshot);
    },
    viewport(query) {
      equal(query.token, this.token);
      equal({ ...query.viewport, afterIndex: query.afterIndex, limit: query.limit, glyphs: query.glyphs }, edition.viewportRequest);
      return structuredClone(edition.viewport);
    },
    glyphOutlines(fontId, ids) {
      const batch = edition.glyphOutlines.find(item => item.fontId === fontId); assert(batch);
      return structuredClone({ ...batch, glyphs: ids.map(id => {
        const glyph = batch.glyphs.find(item => item.glyphId === id); assert(glyph); return glyph;
      }) });
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
function ink(data) {
  let count = 0;
  for (let i = 0; i < data.length; i += 4) if (data[i + 3] && Math.min(data[i], data[i + 1], data[i + 2]) < 240) count++;
  return count;
}
function watched(session) {
  const proxy = Object.create(session), calls = { glyphOutlines: 0, fontBytes: 0, assetBytes: 0 };
  for (const name of Object.keys(calls)) Object.defineProperty(proxy, name, {
    value(...args) { calls[name]++; return session[name](...args); },
  });
  return { session: proxy, calls };
}

export async function run(fixture, runtime, mode = "enabled") {
  equal(fixture.schema, "fmd-flow-emoji-native-v1");
  assert(runtime && ["enabled", "lean"].includes(mode));
  const results = [], evidence = {}, restores = [];
  const test = async (name, action) => {
    try { await action(); results.push({ name, pass: true }); }
    catch (error) { results.push({ name, pass: false, error: String(error.stack ?? error) }); }
  };
  const create = source => runtime.createFlowSession(source, { font: "sans", ...fixture.original.snapshot.options });
  for (const prototype of [CanvasRenderingContext2D.prototype, OffscreenCanvasRenderingContext2D.prototype]) {
    for (const name of ["fillText", "measureText"]) {
      const original = prototype[name];
      prototype[name] = function () { throw Error("unexpected browser font shaping: " + name); };
      restores.push(() => { prototype[name] = original; });
    }
  }
  try {
    if (mode === "enabled") {
      await test("generated emoji, variation clusters, outlines, viewport, hits and selections equal actual native output", async () => {
        const session = await create(fixture.original.source);
        try {
          equal(geometry(snapshot(session)), geometry(fixture.original.snapshot));
          const { x, y, width, height, ...query } = fixture.original.viewportRequest;
          equal(session.viewport({ viewport: { x, y, width, height }, ...query, token: session.token }), fixture.original.viewport);
          equal(session.readingOrder({ offset: 0, limit: 2048, token: session.token }), fixture.original.reading);
          for (const batch of fixture.original.glyphOutlines)
            equal(session.glyphOutlines(batch.fontId, batch.glyphs.map(glyph => glyph.glyphId)), batch);
          for (const hit of fixture.original.hits) equal(session.hitTest(hit.x, hit.y, session.token), hit.response);
          for (const selection of fixture.original.selections)
            equal(session.selectText(selection.itemIndex, selection.startUtf16, selection.endUtf16, session.token), selection.response);
          equal(session.copySource(0, bytes(session.source), session.token.revision), fixture.original.source);
          equal(session.pendingAssets().requests, []);
          let clusters = 0;
          for (const item of snapshot(session).items) if (item.kind === "text") {
            for (const cluster of item.fontRun.clusters) if (cluster.fontId === fixture.emojiFontId) {
              clusters++;
              const source = item.text.slice(...cluster.utf16);
              assert(source.length && !source.startsWith("\ufe0f"));
              const glyphs = item.fontRun.glyphs.slice(...cluster.glyphs);
              assert(glyphs.length === 1 && glyphs[0].glyphId !== 0);
            }
          }
          assert(clusters >= 12); evidence.emojiClusters = clusters; evidence.nativeParity = true;
        } finally { session.dispose(); }
      });
      await test("Canvas paints identical native and WASM emoji ink without system fonts or image requests", async () => {
        const session = await create(fixture.original.source), proxy = watched(session), actual = painter(), expected = painter();
        try {
          const frame = await actual.renderer.render(proxy.session, { ...size,
            resolveImage() { throw Error("emoji must not request an image"); } });
          await expected.renderer.render(recorded(fixture.original), size);
          equal(frame.queryMode, "indexed-viewport"); equal(frame.missingImages, 0);
          assert(samePixels(pixels(actual.canvas), pixels(expected.canvas)));
          assert(proxy.calls.glyphOutlines > 1); equal(proxy.calls.fontBytes, 0); equal(proxy.calls.assetBytes, 0);
          evidence.inkPixels = ink(pixels(actual.canvas)); assert(evidence.inkPixels > 1000);
        } finally { actual.dispose(); expected.dispose(); session.dispose(); }
      });
      await test("semantic reader Copy keeps selectors, supplementary emoji, combining marks, links and code", async () => {
        const session = await create(fixture.original.source), root = document.createElement("article"), links = [];
        document.body.append(root); const view = new FlowReaderView(root, { onLink(link) { links.push(link); } });
        try {
          const document = await readFlowDocument(session); await view.renderAsync(document);
          const entries = document.nodes.filter(node => !node.children.length && /[\p{Extended_Pictographic}\u2705]/u.test(node.text));
          assert(entries.length >= 7);
          for (const entry of entries) {
            const match = document.find(entry.text).matches.find(found => found.nodeIndex === entry.index); assert(match);
            equal(view.selectMatch(match), entry.text); equal(globalThis.document.getSelection().toString(), entry.text);
            const range = sourceSpanToUtf16(session.source, entry.enclosingSourceSpan);
            assert(session.source.slice(range.start, range.end).length > 0);
          }
          assert(root.querySelector("strong")?.textContent.includes("✅"));
          assert(root.querySelector("em")?.textContent.includes("⚠\ufe0f"));
          assert(root.querySelector("pre code")?.textContent.includes("🚀\tlaunch"));
          const link = root.querySelector('a[role="link"]'); assert(link && !link.hasAttribute("href"));
          equal(link.textContent, "📦 package"); link.click(); equal(links[0].target, "/guide");
          assert(!root.querySelector("img,svg,script,iframe"));
          evidence.exactCopyNodes = entries.length;
        } finally { view.dispose(); root.remove(); globalThis.document.getSelection().removeAllRanges(); session.dispose(); }
      });
      await test("source replacement and UTF-16 edits agree with rebuilt sessions and reject split surrogate pairs", async () => {
        const session = await create(fixture.original.source); let fresh;
        try {
          const old = session.token; session.replaceSource(fixture.updated.source, { expectedRevision: old.revision });
          equal(geometry(snapshot(session)), geometry(fixture.updated.snapshot));
          await rejects(() => session.snapshot({ token: old }), "STALE_REVISION");
          session.replaceSource("A👍\ufe0f©\ufe0f🚀Z", { expectedRevision: session.token.revision });
          const token = session.token;
          await rejects(() => session.edit(2, 3, "x", { expectedRevision: token.revision }), "INVALID_SELECTION");
          equal(session.token, token); equal(session.source, "A👍\ufe0f©\ufe0f🚀Z");
          session.edit(1, 4, "🎉", { expectedRevision: token.revision });
          equal(session.source, "A🎉©\ufe0f🚀Z"); fresh = await create(session.source);
          equal(geometry(snapshot(session)), geometry(snapshot(fresh)));
          equal(session.copySource(0, bytes(session.source), session.token.revision), session.source);
        } finally { fresh?.dispose(); session.dispose(); }
      });
      await test("narrow reflow keeps presentation pairs atomic and returns to the identical wide layout", async () => {
        const source = "©\ufe0f👍\ufe0f✅\ufe0f🚀".repeat(24), session = await create(source);
        try {
          const wide = snapshot(session), token = session.token;
          session.reflow({ viewportWidth: 42 }, token);
          await rejects(() => session.snapshot({ token }), "STALE_LAYOUT");
          const narrow = snapshot(session), text = narrow.items.filter(item => item.kind === "text");
          assert(narrow.totalBounds.height > wide.totalBounds.height);
          equal(text.map(item => item.text).join(""), source);
          for (const item of text) {
            assert(item.bounds.x + item.bounds.width <= 42.002);
            for (const cluster of item.fontRun.clusters)
              assert(["©\ufe0f", "👍\ufe0f", "✅\ufe0f", "🚀"].includes(item.text.slice(...cluster.utf16)));
          }
          session.reflow(fixture.original.snapshot.options, session.token);
          equal(geometry(snapshot(session)), geometry(wide));
        } finally { session.dispose(); }
      });
    } else {
      await test("feature-disabled WASM refuses emoji and presentation sequences while retaining ordinary text", async () => {
        for (const source of [fixture.original.source, "😀", "🚀", "✅", "©\ufe0f", "©\ufe0e"])
          await rejects(() => create(source), "LAYOUT_ERROR");
        for (const source of ["office AV café © ® ™", "e\u0301 and ∑", "# **Ordinary** text"]) {
          const session = await create(source), f = painter();
          try { await f.renderer.render(session, size); assert(ink(pixels(f.canvas)) > 20); equal(session.source, source); }
          finally { f.dispose(); session.dispose(); }
        }
        evidence.featureOffRefused = true;
      });
    }
    await test(`${mode} rejected complex sequences preserve the last source, revisions, geometry and painted frame`, async () => {
      const session = await create(mode === "enabled" ? fixture.updated.source : "Kept café and **text**."), f = painter();
      try {
        const before = snapshot(session), token = session.token, source = session.source;
        await f.renderer.render(session, size); const prior = pixels(f.canvas);
        for (const invalid of ["👩\u200d💻", "👍🏻", "🇺🇸", "1\ufe0f\u20e3", "a\ufe0f", "🚀\ufe0f", "😀\u0301", "אב"])
          await rejects(() => session.replaceSource(invalid, { expectedRevision: token.revision }), "LAYOUT_ERROR");
        equal(session.source, source); equal(session.token, token); equal(geometry(snapshot(session)), geometry(before));
        await f.renderer.render(session, size); assert(samePixels(prior, pixels(f.canvas)));
      } finally { f.dispose(); session.dispose(); }
    });
  } finally { for (const restore of restores) restore(); }
  return { passed: results.filter(result => result.pass).length, failed: results.filter(result => !result.pass).length, evidence, results };
}
