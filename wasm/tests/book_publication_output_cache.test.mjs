// The COMPLETE production retained renderer/cache executes through normal ESM.
// Native rendering and the preview codec are explicit deterministic doubles.
// These tests prove skipped calls, owned bytes and invalidation, NOT WASM speed.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createRetainedBook, createCachedBookRenderer, BOOK_OUTPUT_CACHE_LIMITS } from "../book_retained.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const code = expected => cause => cause?.code === expected;
const file = (path, source = `# ${path}`) => ({ path, source });
function capture() {
  return { files: [file("a.md", "\ufeff# A\r\n😀"), file("b.md", "# B\n")], options: {
    title: "Book", author: "Writer", lang: "en", customCss: "body{}", toc: true,
    pageNumbers: true, font: "sans", darkMode: "disabled", fontScale: 1, expandIncludes: true,
    typography: "optimal-pagination", baseFontSize: 12, headingScale: 1.2, tableFontSize: 10,
    tocDepth: 3, fitToPages: 2, codeLineNumbers: true, metadataEpochSeconds: 0, htmlFontFormat: "ttf",
    page: { size: { widthPt: 612, heightPt: 792 }, margins: { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 72 } },
    running: { header: { left: "Book" }, footer: { center: "{page}" }, skipFirstPage: false },
    includeSources: [file("part.txt", "Included\r")],
    images: [{ destination: "a.png", bytes: new Uint8Array([1, 2, 3]) }],
    fontAssets: [{ slot: "body-regular", weight: 500, bytes: new Uint8Array([4, 5]) }],
  } };
}
function fixture(limits = {}, hooks = {}) {
  const stats = { creates: 0, frees: 0, updates: 0, replacements: 0, exports: [], previews: 0 };
  const engine = { async createBook(files, options) {
    stats.creates++;
    let value = structuredClone({ files, options }), revision = 0, disposed = false;
    const output = (format, selected) => {
      assert.equal(disposed, false);
      stats.exports.push([format, selected]);
      if (hooks.fail) throw Object.assign(new Error("native failure"), { code: "NATIVE_FAILURE" });
      const result = hooks.output?.(format, selected) ?? {
        bytes: encode({ ...value, format, selected }),
        sourceLength: [...value.files, ...value.options.includeSources].reduce((sum, f) => sum + new TextEncoder().encode(f.source).length, 0),
        ...(format === "chapter-preview" ? { previewMode: hooks.legacy ? "site-fallback" : "native-chapter" } : {}),
        // Real native outputs have helpers that close over their buffer. They
        // must not escape into a worker-cache hit with a different byte view.
        blob() { throw Error("not a wire-output method"); }, filename() { return "unused"; },
      };
      return hooks.wait ? hooks.wait.then(() => result) : result;
    };
    const session = {
      get sourceRevision() { return revision; },
      updateSources(changes, { expectedRevision }) {
        assert.equal(expectedRevision, revision); stats.updates++;
        if (hooks.rejectUpdate) throw Object.assign(new Error("update refused"), { code: "UPDATE_FAILED" });
        const replace = fs => fs.map(f => ({ ...f, source: changes.find(c => c.path === f.path)?.source ?? f.source }));
        value.files = replace(value.files); value.options.includeSources = replace(value.options.includeSources); revision++;
      },
      replaceSources(files, { includeSources, expectedRevision }) {
        assert.equal(expectedRevision, revision); stats.replacements++; revision++;
        value.files = structuredClone(files); value.options.includeSources = structuredClone(includeSources);
      },
      renderPdf: () => output("pdf"), renderEpub: () => output("epub"), renderSite: () => output("site"),
      validateLinks: () => output("links"), renderChapter: index => output("chapter-preview", index),
      dispose() { if (!disposed) stats.frees++; disposed = true; },
    };
    if (hooks.old) { delete session.updateSources; delete session.replaceSources; }
    return session;
  } };
  const preview = async api => {
    stats.previews++;
    const site = await api.renderBookSite();
    return { bytes: encode({ preview: [...site.bytes] }), sourceLength: site.sourceLength };
  };
  const retained = createRetainedBook(engine, preview, (session, selected) => session.renderChapter(selected));
  const cached = createCachedBookRenderer(retained, preview, limits);
  return { stats, hooks, retained, cached,
    render: (input = capture(), format = "pdf", selected) => cached.render(input.files, input.options, format, selected),
    fresh: async (input, format, selected) => {
      const other = fixture({}, { legacy: hooks.legacy });
      try { return await other.retained.render(input.files, input.options, format, selected); }
      finally { other.retained.clear(); }
    },
  };
}

test("all wire formats reuse exact captures while native output helpers remain native-only", async () => {
  const h = fixture();
  for (const format of ["pdf", "epub", "site", "preview", "links", "chapter-preview"]) {
    const first = await h.render(capture(), format), second = await h.render(capture(), format);
    assert.deepEqual(second, first); assert.notEqual(first.bytes, second.bytes);
    assert.deepEqual(Object.keys(second).sort(), format === "chapter-preview" ? ["bytes", "previewMode", "sourceLength"] : ["bytes", "sourceLength"]);
    const fresh = await h.fresh(capture(), format);
    assert.deepEqual(second.bytes, fresh.bytes); assert.equal(second.sourceLength, fresh.sourceLength);
  }
  assert.equal(h.stats.creates, 1); assert.equal(h.stats.exports.length, 5);
  assert.equal(h.stats.previews, 1); assert.equal(h.cached.cacheStats.entries, 6);
  h.cached.clear(); assert.equal(h.stats.frees, 1); assert.equal(h.cached.cacheStats.bytes, 0);
});

test("site publication and full-book preview share one native site export in either order", async () => {
  for (const formats of [["site", "preview", "site", "preview"], ["preview", "site", "preview", "site"]]) {
    const h = fixture(); for (const format of formats) await h.render(capture(), format);
    assert.deepEqual(h.stats.exports, [["site", undefined]]); assert.equal(h.stats.previews, 1);
    assert.equal(h.cached.cacheStats.hits, 3); h.cached.clear();
  }
});

test("selected chapters and native/fallback capability receipts stay distinct", async () => {
  for (const legacy of [false, true]) {
    const h = fixture({}, { legacy });
    for (const selected of [0, 1, 0, 1]) {
      const output = await h.render(capture(), "chapter-preview", selected);
      assert.equal(output.previewMode, legacy ? "site-fallback" : "native-chapter");
      assert.equal(JSON.parse(new TextDecoder().decode(output.bytes)).selected, selected);
    }
    assert.equal(h.stats.exports.length, 2); h.cached.clear();
  }
});

test("mutating or transferring cold and hot results cannot poison later hits", async () => {
  const h = fixture(); const first = await h.render(), expected = first.bytes.slice();
  first.bytes.fill(0); const second = await h.render(); assert.deepEqual(second.bytes, expected);
  const moved = structuredClone(second, { transfer: [second.bytes.buffer] });
  assert.equal(second.bytes.byteLength, 0); moved.bytes.fill(9);
  const third = await h.render(); assert.deepEqual(third.bytes, expected);
  const kept = third.bytes.slice(); h.cached.clear(); assert.deepEqual(third.bytes, kept);
});

test("only a subview's bytes are retained, never its oversized backing allocation", async () => {
  const backing = new Uint8Array(1024 * 1024); backing.set([1, 2, 3], 500);
  const h = fixture({}, { output: () => ({ bytes: backing.subarray(500, 503), sourceLength: 8 }) });
  await h.render(); backing.fill(0);
  const hit = await h.render(); assert.deepEqual([...hit.bytes], [1, 2, 3]);
  assert.equal(hit.bytes.buffer.byteLength, 3); assert.equal(h.cached.cacheStats.bytes, 3); h.cached.clear();
});

const sourceChanges = {
  chapter: input => { input.files[0].source += "!"; },
  include: input => { input.options.includeSources[0].source += "!"; },
  newline: input => { input.files[0].source = input.files[0].source.replace("\r\n", "\n"); },
  rename: input => { input.files[0].path = "renamed.md"; },
  reorder: input => { input.files.reverse(); },
  add: input => { input.files.push(file("new.md")); },
  remove: input => { input.files.pop(); },
  role: input => { input.options.includeSources.push(input.files.pop()); },
  includeRemoval: input => { input.options.includeSources = []; },
};
for (const [name, change] of Object.entries(sourceChanges)) test(`source dependency invalidation: ${name}`, async () => {
  const h = fixture(); await h.render(); const next = capture(); change(next);
  const result = await h.render(next); assert.deepEqual(result.bytes, (await h.fresh(next, "pdf")).bytes);
  assert.equal(h.stats.exports.length, 2); assert.equal(h.stats.creates, 1);
  await h.render(structuredClone(next)); assert.equal(h.stats.exports.length, 2); h.cached.clear();
});

const settingsChanges = Object.fromEntries([
  ...Object.entries({ title: "Other", author: "Editor", lang: "fr", customCss: "p{color:red}",
    font: "serif", darkMode: "auto", typography: "homogeneous", htmlFontFormat: "woff2" })
    .map(([key, value]) => [key, input => { input.options[key] = value; }]),
  ...["toc", "pageNumbers", "codeLineNumbers"].map(key => [key, input => { input.options[key] = !input.options[key]; }]),
  ...["fontScale", "baseFontSize", "tableFontSize", "tocDepth", "fitToPages", "metadataEpochSeconds"].map(key => [key, input => { input.options[key]++; }]),
  ["expandIncludes", input => { input.options.expandIncludes = false; input.options.includeSources = []; }],
  ["headingScale", input => { input.options.headingScale = 1.4; }],
  ["paper", input => { input.options.page.size.widthPt++; }],
  ["margin", input => { input.options.page.margins.leftPt++; }],
  ["running", input => { input.options.running.header.left += "!"; }],
  ["imageBytes", input => { input.options.images[0].bytes[1]++; }],
  ["imagePath", input => { input.options.images[0].destination = "b.png"; }],
  ["revokeImages", input => { input.options.images = []; }],
  ["fontBytes", input => { input.options.fontAssets[0].bytes[1]++; }],
  ["fontSlot", input => { input.options.fontAssets[0].slot = "body-bold"; }],
  ["fontWeight", input => { input.options.fontAssets[0].weight++; }],
  ["revokeFonts", input => { input.options.fontAssets = []; }],
]);
for (const [name, change] of Object.entries(settingsChanges)) test(`render dependency invalidation: ${name}`, async () => {
  const h = fixture(); await h.render(); const next = capture(); change(next);
  // These are already-normalized-input tests, not native option admission.
  const result = await h.render(next); assert.deepEqual(result.bytes, (await h.fresh(next, "pdf")).bytes);
  assert.equal(h.stats.exports.length, 2); assert.equal(h.stats.creates, 2); assert.equal(h.stats.frees, 1);
  h.cached.clear();
});

test("unknown future options disable reuse rather than hiding a new dependency", async () => {
  const h = fixture(); const input = capture(); input.options.futureOption = "A";
  await h.render(input); await h.render(structuredClone(input));
  assert.equal(h.stats.exports.length, 2); assert.equal(h.cached.cacheStats.entries, 0); h.cached.clear();
});

test("entry-count eviction is least-recently-used, not insertion-order FIFO", async () => {
  const h = fixture({ maxEntries: 2 });
  for (const format of ["pdf", "epub", "pdf", "site", "pdf", "epub"]) await h.render(capture(), format);
  assert.deepEqual(h.stats.exports.map(([kind]) => kind), ["pdf", "epub", "site", "epub"]);
  assert.equal(h.cached.cacheStats.entries, 2); assert.equal(h.cached.cacheStats.evictions, 2); h.cached.clear();
});

test("byte quota evicts enough entries; oversized results do not evict useful ones", async () => {
  const h = fixture({ maxBytes: 10, maxEntryBytes: 10 }, { output: format => ({ bytes: new Uint8Array(format === "links" ? 11 : 4), sourceLength: 0 }) });
  for (const format of ["pdf", "epub", "site", "links", "site", "epub"]) await h.render(capture(), format);
  assert.deepEqual(h.stats.exports.map(([kind]) => kind), ["pdf", "epub", "site", "links"]);
  assert.equal(h.cached.cacheStats.bytes, 8); assert.equal(h.cached.cacheStats.entries, 2); h.cached.clear();
});

test("lowered zero budgets disable retention without disabling rendering", async () => {
  for (const limits of [{ maxBytes: 0 }, { maxEntryBytes: 0 }, { maxEntries: 0 }]) {
    const h = fixture(limits); await h.render(); await h.render();
    assert.equal(h.stats.exports.length, 2); assert.equal(h.cached.cacheStats.bytes, 0); h.cached.clear();
  }
});

test("limits reject raised ceilings, malformed records and accessors without invoking them", () => {
  for (const bad of [null, [], { maxBytes: -1 }, { maxEntries: 1.5 }, { unknown: 1 },
    ...Object.entries(BOOK_OUTPUT_CACHE_LIMITS).map(([key, value]) => ({ [key]: value + 1 })),
    { get maxBytes() { throw Error("accessor invoked"); } }, { [Symbol("hidden")]: 1 }]) {
    assert.throws(() => fixture(bad), code("INVALID_OPTIONS"));
  }
});

test("empty, shared, malformed and invalid-capability outputs cannot enter the cache", async () => {
  for (const [result, format, expected] of [
    [{ bytes: new Uint8Array(), sourceLength: 0 }, "pdf", "OUTPUT_LIMIT"],
    [{ bytes: new Uint8Array(new SharedArrayBuffer(3)), sourceLength: 0 }, "pdf", "WORKER_PROTOCOL_ERROR"],
    [{ bytes: new Uint8Array([1]), sourceLength: NaN }, "pdf", "WORKER_PROTOCOL_ERROR"],
    [{ bytes: new Uint8Array([1]), sourceLength: -1 }, "pdf", "WORKER_PROTOCOL_ERROR"],
    [{ bytes: new Uint8Array([1]), sourceLength: 0, previewMode: "invented" }, "chapter-preview", "WORKER_PROTOCOL_ERROR"],
  ]) {
    const h = fixture({}, { output: () => result });
    await assert.rejects(h.render(capture(), format), code(expected));
    assert.equal(h.cached.cacheStats.entries, 0); assert.equal(h.stats.frees, 1);
    delete h.hooks.output; await h.render(); assert.equal(h.stats.creates, 2); h.cached.clear();
  }
});

test("native render and source-transaction failures retire every cached format", async () => {
  for (const transaction of [false, true]) {
    const h = fixture(); await h.render(); await h.render(capture(), "epub");
    const next = capture(); next.files[0].source += "edited";
    if (transaction) h.hooks.rejectUpdate = true; else h.hooks.fail = true;
    await assert.rejects(h.render(next), code(transaction ? "UPDATE_FAILED" : "NATIVE_FAILURE"));
    assert.equal(h.cached.cacheStats.entries, 0); assert.equal(h.cached.cacheStats.bytes, 0);
    h.hooks.rejectUpdate = h.hooks.fail = false;
    await h.render(); assert.equal(h.stats.creates, 2); h.cached.clear();
  }
});

test("explicit clear fences pending cache hits and preserves the busy contract", async () => {
  const h = fixture(); await h.render();
  const pending = h.render(); const rejected = assert.rejects(pending, code("EXPORT_CANCELLED"));
  const busy = assert.rejects(h.render(capture(), "epub"), code("BOOK_BUSY"));
  h.cached.clear(); await Promise.all([rejected, busy]); assert.equal(h.cached.cacheStats.entries, 0);
  await h.render(); assert.equal(h.stats.creates, 2); h.cached.clear();
});

test("clearing an in-flight native export cannot resurrect its captured output", async () => {
  let release; const wait = new Promise(resolve => { release = resolve; });
  const h = fixture({}, { wait }); const pending = h.render();
  await Promise.resolve(); await Promise.resolve();
  h.cached.clear(); release(); await assert.rejects(pending, code("EXPORT_CANCELLED"));
  assert.equal(h.cached.cacheStats.entries, 0); assert.equal(h.cached.cacheStats.bytes, 0);
  delete h.hooks.wait; await h.render(); h.cached.clear();
});

test("older native binaries still reconstruct changed source sets on a cache miss", async () => {
  const h = fixture({}, { old: true }); await h.render(); await h.render();
  const next = capture(); next.files.push(file("new.md")); await h.render(next); await h.render(structuredClone(next));
  assert.equal(h.stats.creates, 2); assert.equal(h.stats.exports.length, 2); h.cached.clear();
});

test("actual worker bootstrap installs the cache for both book and preview retention", () => {
  const entry = new URL("../book_worker_entry.js", import.meta.url), retained = new URL("../book_retained.mjs", import.meta.url);
  const program = `
    import vm from 'node:vm'; import fs from 'node:fs'; import assert from 'node:assert/strict';
    let installed, renders=0;
    const context=vm.createContext({ Uint8Array, ArrayBuffer, self:{}, install:(scope, engine)=>installed=engine,
      createBook:async()=>({renderPdf:()=>{renders++;return {bytes:new Uint8Array([1,2]),sourceLength:0}},dispose(){}}),
      preview:async()=>({bytes:new Uint8Array([3]),sourceLength:0}) });
    const module=(code,id)=>new vm.SourceTextModule(code,{context,identifier:id});
    const cache=module(fs.readFileSync(${JSON.stringify(retained.pathname)},'utf8'),'cache');
    const entry=module(fs.readFileSync(${JSON.stringify(entry.pathname)},'utf8'),'entry');
    const modules={ './book_retained.mjs':cache,
      './book_worker.mjs':module('export const installBookWorker=globalThis.install','worker'),
      './book.js':module('export const createBook=globalThis.createBook','book'),
      './book_inspection.mjs':module('export const inspectBook=()=>{}','inspect'),
      './franken_markdown.js':module('export const unused=0','documents'),
      './book_site_preview.mjs':module('export const renderBookPreview=globalThis.preview;export const renderBookSelectedPreview=globalThis.preview;export const renderBookSessionChapterPreview=globalThis.preview','preview') };
    await entry.link(name=>{assert(modules[name],name);return modules[name]}); await entry.evaluate();
    const files=[{path:'a.md',source:'A'}],options={includeSources:[],images:[],fontAssets:[]};
    for(let i=0;i<3;i++)await installed.renderRetainedBook(files,options,'pdf');
    assert.equal(renders,1);assert.equal(installed.clearRetainedBook,installed.clearRetainedBookPreview);
    installed.clearRetainedBookPreview();await installed.renderRetainedBook(files,options,'pdf');assert.equal(renders,2);
    await assert.rejects(installed.renderRetainedBookPreview(files,options,0),e=>e.code==='WORKER_PROTOCOL_ERROR');
  `;
  // Exercise a real entry import, with explicit engine/codec/install adapters.
  const run = spawnSync(process.execPath, ["--experimental-vm-modules", "--input-type=module", "-e", program], { encoding: "utf8", timeout: 10000 });
  assert.equal(run.status, 0, run.stderr || run.stdout);
  assert(readFileSync(entry, "utf8").includes("createCachedBookRenderer"));
});
