// The COMPLETE production retained renderer/cache executes through normal ESM.
// Native rendering and the preview codec are explicit deterministic doubles.
// These tests prove skipped calls, owned bytes and invalidation, NOT WASM speed.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { Worker } from "node:worker_threads";
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

const state = text => { const value = capture(); value.files[0].source = text; return value; };
const keyUnits = value => [...value.files, ...value.options.includeSources]
  .reduce((sum, file) => sum + file.path.length + file.source.length, 0);

test("undo and redo reuse complete prior source states without replaying native transactions", async () => {
  const h = fixture(), a = state("A"), b = state("B");
  for (const value of [a, b, a, b, a]) {
    const result = await h.render(structuredClone(value));
    assert.deepEqual(result.bytes, (await h.fresh(value, "pdf")).bytes);
  }
  assert.equal(h.stats.exports.length, 2); assert.equal(h.stats.updates, 1);
  assert.equal(h.cached.cacheStats.snapshots, 2); assert.equal(h.cached.cacheStats.historyUnits, keyUnits(b));
  h.cached.clear(); assert.equal(h.cached.cacheStats.snapshots, 0); assert.equal(h.cached.cacheStats.historyUnits, 0);
});

test("a new format after a historical hit synchronizes against actual native state", async () => {
  const h = fixture(), a = state("A"), b = state("B"), c = state("C");
  b.files[1].source = "B also changed this chapter";
  await h.render(a); await h.render(b); await h.render(structuredClone(a));
  assert.equal(h.stats.updates, 1); // Native state is B while the returned capture is A.
  const result = await h.render(c, "epub");
  assert.deepEqual(result.bytes, (await h.fresh(c, "epub")).bytes);
  assert.equal(h.stats.updates, 2); assert.equal(h.stats.exports.length, 3); h.cached.clear();
});

test("structural undo restores the whole navigation source set, not only selected text", async () => {
  const h = fixture(), a = capture(), b = capture();
  b.files.reverse(); b.files.push(file("new.md", "A new chapter"));
  b.options.includeSources.push(file("extra.txt", "New include"));
  for (const input of [a, b, a, b]) {
    const result = await h.render(structuredClone(input), "chapter-preview", 0);
    assert.deepEqual(result.bytes, (await h.fresh(input, "chapter-preview", 0)).bytes);
  }
  assert.equal(h.stats.replacements, 1); assert.equal(h.stats.exports.length, 2); h.cached.clear();
});

test("same-length and concatenation-colliding texts remain distinct exact source keys", async () => {
  const h = fixture(), variants = [state("ab"), state("a"), state("é"), state("e\u0301"), state("\ufeffx\r\n"), state("x\n")];
  variants[0].files[1].source = "c"; variants[1].files[1].source = "bc";
  const initial = [];
  for (const input of variants) initial.push((await h.render(input)).bytes.slice());
  for (let i = variants.length - 1; i >= 0; i--) {
    const result = await h.render(structuredClone(variants[i])); assert.deepEqual(result.bytes, initial[i]);
  }
  assert.equal(h.stats.exports.length, variants.length); h.cached.clear();
});

test("history snapshot eviction respects revisits rather than first insertion", async () => {
  const h = fixture({ maxSnapshots: 2 });
  for (const text of ["A", "B", "A", "C", "B"]) await h.render(state(text));
  assert.equal(h.stats.exports.length, 4); assert.equal(h.cached.cacheStats.snapshots, 2);
  assert.equal(h.cached.cacheStats.historyUnits, keyUnits(state("C"))); h.cached.clear();
});

test("historical source keys have an independent UTF-16-unit budget", async () => {
  const units = keyUnits(state("A")), h = fixture({ maxHistoryUnits: units });
  for (const text of ["A", "B", "C", "B", "A"]) {
    await h.render(state(text)); assert(h.cached.cacheStats.historyUnits <= units);
    assert(h.cached.cacheStats.snapshots <= 2);
  }
  assert.equal(h.stats.exports.length, 4); h.cached.clear();
});

test("large current books remain hot-cacheable without retaining them in history", async () => {
  const small = state("A"), large = state("large\r\n😀".repeat(10000));
  const h = fixture({ maxHistoryUnits: keyUnits(small) });
  for (const value of [large, large, small, large, large]) await h.render(structuredClone(value));
  assert.equal(h.stats.exports.length, 3);
  assert(h.cached.cacheStats.historyUnits <= keyUnits(small)); h.cached.clear();
});

test("zero history or one snapshot retains ordinary current-capture caching", async () => {
  for (const limits of [{ maxSnapshots: 1 }, { maxHistoryUnits: 0 }]) {
    const h = fixture(limits);
    for (const text of ["A", "A", "B", "B", "A"]) await h.render(state(text));
    assert.equal(h.stats.exports.length, 3); assert.equal(h.cached.cacheStats.snapshots, 1);
    assert.equal(h.cached.cacheStats.historyUnits, 0); h.cached.clear();
  }
  const disabled = fixture({ maxSnapshots: 0 }); await disabled.render(); await disabled.render();
  assert.equal(disabled.stats.exports.length, 2); assert.equal(disabled.cached.cacheStats.entries, 0); disabled.cached.clear();
});

test("global output eviction releases empty historical source keys and their charge", async () => {
  const h = fixture({ maxEntries: 1 });
  for (const text of ["A", "B", "C", "A"]) {
    await h.render(state(text)); assert.equal(h.cached.cacheStats.entries, 1);
    assert.equal(h.cached.cacheStats.snapshots, 1); assert.equal(h.cached.cacheStats.historyUnits, 0);
  }
  assert.equal(h.stats.exports.length, 4); h.cached.clear();
});

test("options and resource revocation erase every historical output, even after restoring old values", async () => {
  for (const change of [input => { input.options.images = []; }, input => { input.options.fontAssets = []; },
    input => { input.options.title = "New title"; }]) {
    const h = fixture(); await h.render(state("A")); await h.render(state("B"));
    const changed = state("A"); change(changed); await h.render(changed);
    assert.equal(h.cached.cacheStats.snapshots, 1); assert.equal(h.cached.cacheStats.historyUnits, 0);
    await h.render(state("A")); assert.equal(h.stats.exports.length, 4);
    assert.equal(h.stats.creates, 3); h.cached.clear();
  }
});

test("unknown options and failures cannot leave reusable history behind", async () => {
  const h = fixture(); await h.render(state("A")); await h.render(state("B"));
  const unknown = state("C"); unknown.options.future = true; await h.render(unknown);
  assert.equal(h.cached.cacheStats.snapshots, 0); assert.equal(h.cached.cacheStats.historyUnits, 0);
  await h.render(state("A")); await h.render(state("B")); h.hooks.fail = true;
  await assert.rejects(h.render(state("C")), code("NATIVE_FAILURE"));
  assert.equal(h.cached.cacheStats.snapshots, 0); assert.equal(h.cached.cacheStats.historyUnits, 0);
  h.hooks.fail = false; await h.render(state("A")); assert.equal(h.stats.creates, 2); h.cached.clear();
});

test("clear during a historical hit cancels output and destroys all old source keys", async () => {
  const h = fixture(); await h.render(state("A")); await h.render(state("B"));
  const pending = h.render(state("A")), rejected = assert.rejects(pending, code("EXPORT_CANCELLED"));
  h.cached.clear(); await rejected;
  assert.equal(h.cached.cacheStats.snapshots, 0); assert.equal(h.cached.cacheStats.historyUnits, 0);
  await h.render(state("A")); assert.equal(h.stats.exports.length, 3); h.cached.clear();
});

test("old binaries can reuse history and still reconstruct the next uncached source set", async () => {
  const h = fixture({}, { old: true }), a = capture(), b = capture(), c = capture();
  b.files.reverse(); c.files.push(file("new.md"));
  for (const input of [a, b, a, c]) {
    const result = await h.render(structuredClone(input));
    assert.deepEqual(result.bytes, (await h.fresh(input, "pdf")).bytes);
  }
  assert.equal(h.stats.creates, 3); assert.equal(h.stats.exports.length, 3); h.cached.clear();
});

test("500 seeded source/format/history transitions match fresh uncached output within all budgets", async () => {
  const h = fixture({ maxSnapshots: 3, maxEntries: 6, maxBytes: 12000, maxEntryBytes: 8000, maxHistoryUnits: 2000 });
  const variants = [capture(), ...Object.values(sourceChanges).map(change => { const input = capture(); change(input); return input; })];
  let seed = 0x5eadcafe;
  const next = max => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % max; };
  const formats = ["pdf", "epub", "site", "preview", "links", "chapter-preview"];
  await h.render(); await h.render();
  for (let i = 0; i < 500; i++) {
    const input = structuredClone(variants[next(variants.length)]), format = formats[next(formats.length)];
    const selected = format === "chapter-preview" ? next(input.files.length) : undefined;
    if (i % 31 === 0) input.options.images = [];
    if (i % 47 === 0) h.cached.clear();
    const actual = await h.render(input, format, selected), expected = await h.fresh(input, format, selected);
    assert.deepEqual(actual.bytes, expected.bytes, `transition ${i}`);
    assert.equal(actual.sourceLength, expected.sourceLength);
    const stats = h.cached.cacheStats;
    assert(stats.bytes <= 12000 && stats.entries <= 6 && stats.snapshots <= 3 && stats.historyUnits <= 2000);
  }
  assert(h.cached.cacheStats.hits > 0); h.cached.clear();
});

test("real worker-thread transfers preserve historical bytes and later native synchronization", async t => {
  const moduleUrl = new URL("../book_retained.mjs", import.meta.url).href;
  const source = `
    import {parentPort} from 'node:worker_threads';
    import {createRetainedBook,createCachedBookRenderer} from ${JSON.stringify(moduleUrl)};
    let exports=0,updates=0;
    const engine={async createBook(files,options){
      let value=structuredClone({files,options}),revision=0;
      const output=()=>{exports++;return {bytes:new TextEncoder().encode(JSON.stringify(value)),sourceLength:0}};
      return {get sourceRevision(){return revision},updateSources(changes,{expectedRevision}){
        if(expectedRevision!==revision)throw Error('wrong native baseline');
        const apply=list=>list.map(file=>({...file,source:changes.find(c=>c.path===file.path)?.source??file.source}));
        value.files=apply(value.files);value.options.includeSources=apply(value.options.includeSources);revision++;updates++;
      },renderPdf:output,renderEpub:output,dispose(){}};
    }};
    const preview=async()=>{throw Error('no preview in this transport fixture')};
    const cached=createCachedBookRenderer(createRetainedBook(engine,preview),preview);
    parentPort.on('message',async message=>{
      try{
        if(message.clear){cached.clear();parentPort.postMessage({cleared:true,stats:cached.cacheStats});return}
        const result=await cached.render(message.input.files,message.input.options,message.format);
        parentPort.postMessage({...result,exports,updates,stats:cached.cacheStats},[result.bytes.buffer]);
      }catch(error){parentPort.postMessage({error:error.message})}
    });
  `;
  const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(source)}`));
  t.after(() => worker.terminate());
  const send = message => new Promise((resolve, reject) => {
    const cleanup = () => { clearTimeout(timer); worker.off("message", got); worker.off("error", failed); };
    const got = value => { cleanup(); value.error ? reject(Error(value.error)) : resolve(value); };
    const failed = error => { cleanup(); reject(error); };
    const timer = setTimeout(() => failed(Error("worker fixture timed out")), 5000);
    worker.once("message", got); worker.once("error", failed); worker.postMessage(message);
  });
  const a = state("A"), b = state("B"); b.files[1].source = "Changed on B";
  const first = await send({ input: a, format: "pdf" }); first.bytes.fill(0);
  await send({ input: b, format: "pdf" });
  const undo = await send({ input: a, format: "pdf" });
  assert.equal(undo.exports, 2); assert.equal(undo.updates, 1);
  assert.deepEqual(JSON.parse(new TextDecoder().decode(undo.bytes)).files, a.files);
  const next = await send({ input: a, format: "epub" });
  assert.equal(next.exports, 3); assert.equal(next.updates, 2);
  assert.deepEqual(JSON.parse(new TextDecoder().decode(next.bytes)).files, a.files);
  const cleared = await send({ clear: true }); assert.equal(cleared.stats.snapshots, 0);
  const reopened = await send({ input: a, format: "pdf" }); assert.equal(reopened.exports, 4);
});
