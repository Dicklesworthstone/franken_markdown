// Real public facade and real Node book-worker transport, with explicitly
// substituted generated-binding/FmdBook doubles. These are argument capture,
// admission and lifecycle tests, not native include expansion or PDF proof.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { Worker } from "node:worker_threads";
import { createBookWorkerClient } from "../book_worker.mjs";

const wrapper = await readFile(new URL("../franken_markdown.js", import.meta.url), "utf8");
const dataModule = source => "data:text/javascript;base64," + Buffer.from(source).toString("base64");
const utf8Length = value => new TextEncoder().encode(value).length;
let serial = 0;

async function fixture({ sources = true, arity = 42, legacy = true, profiled = true,
  advanced = true, grid = true } = {}) {
  const lines = [
    "// Explicit generated-binding double " + ++serial,
    "export const state = { calls: [], loads: 0, freed: 0, wait: null, failure: null };",
    "export default async function init() { state.loads++; await state.wait; }",
    "function output(kind, args) {",
    "  state.calls.push({ kind, args });",
    "  if (state.failure) throw state.failure;",
    "  const sourceLength = [...args[1], ...(kind === 'sources' ? args[40] : [])]",
    "    .reduce((sum, source) => sum + new TextEncoder().encode(source).length, 0);",
    "  return { bytes: new TextEncoder().encode('binding-double:' + kind),",
    "    format: 'book-pdf', mimeType: 'application/pdf', extension: 'pdf', sourceLength,",
    "    diagnosticsJson() { return '[]'; }, free() { state.freed++; } };",
    "}",
  ];
  for (const [name, kind, count, present] of [
    ["renderBookPdf", "legacy", 8, legacy],
    ["renderBookPdfConfiguredPage", "page", 29, advanced],
    ["renderBookPdfConfiguredAdvanced", "advanced", 37, advanced],
    ["renderBookPdfConfiguredGrid", "grid", 38, grid],
    ["renderBookPdfConfiguredProfiled", "profiled", 39, profiled],
    ["renderBookPdfConfiguredSources", "sources", arity, sources],
  ]) {
    lines.push("export const " + name + " = " + (present === false ? "undefined"
      : present === "malformed" ? "42"
        : "(...args) => output(" + JSON.stringify(kind) + ", args)") + ";");
    if (present !== false && present !== "malformed")
      lines.push("Object.defineProperty(" + name + ", 'length', { value: " + count + " });");
  }
  for (const name of ["renderEpubConfigured", "renderHtmlConfiguredAdvanced",
    "renderPdfConfiguredMulti", "renderSvgConfigured", "accessibilityAudit",
    "capabilities", "documentStats", "renderSemanticDiffHtml", "searchIndex", "semanticDiff"])
    lines.push("export function " + name + "() { throw new Error('unrelated binding invoked'); }");
  const bindingUrl = dataModule(lines.join("\n"));
  const { state } = await import(bindingUrl);
  let source = wrapper.replaceAll('"./pkg/franken_markdown.js"', JSON.stringify(bindingUrl));
  for (const name of ["pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    source = source.replaceAll('"./' + name + '"', JSON.stringify(new URL("../" + name, import.meta.url).href));
  return { api: await import(dataModule(source)), state };
}

const chapters = () => [
  { path: "guide/start.md", source: "# Manual\r\n\r\n{{#include ../parts/shared.md:excerpt}}\r\n" },
  { path: "end.md", source: "# End\n" },
];
const resources = () => [
  { path: "parts/shared.md", source: "<!-- ANCHOR: excerpt -->\r\nShared 中🚀\r\n<!-- ANCHOR_END: excerpt -->\r\n" },
  { path: "parts/source.rs", source: "\ufefffn included() {}\n" },
];
const capture = call => ({
  paths: call.args[0], sources: call.args[1], includePaths: call.args[39],
  includeSources: call.args[40], expandIncludes: call.args[41],
});

test("root book PDF delegates ordered chapters and include-only sources to the 42-argument binding", async () => {
  const f = await fixture(), files = chapters(), shared = resources();
  const output = await f.api.renderBookPdf(files, { includeSources: shared });
  assert.equal(f.state.calls.length, 1);
  assert.equal(f.state.calls[0].kind, "sources");
  assert.equal(f.state.calls[0].args.length, 42);
  assert.deepEqual(capture(f.state.calls[0]), {
    paths: ["guide/start.md", "end.md"], sources: files.map(file => file.source),
    includePaths: ["parts/shared.md", "parts/source.rs"], includeSources: shared.map(file => file.source),
    expandIncludes: true,
  });
  assert.equal(output.sourceLength, [...files, ...shared].reduce((sum, file) => sum + utf8Length(file.source), 0));
  assert.equal(output.format, "book-pdf");
  assert.equal(output.mimeType, "application/pdf");
  assert.equal(output.filename("manual"), "manual.pdf");
  assert.equal(f.state.freed, 1);
});

test("explicit parse-only mode preserves chapter directives and sends no include-only sources", async () => {
  const f = await fixture(), files = chapters();
  await f.api.renderBookPdf(files, { expandIncludes: false });
  assert.equal(f.state.calls[0].kind, "sources");
  assert.deepEqual(capture(f.state.calls[0]), {
    paths: files.map(file => file.path), sources: files.map(file => file.source),
    includePaths: [], includeSources: [], expandIncludes: false,
  });
});

for (const warm of [false, true]) test("book PDF owns sources, metadata and exact asset views before " + (warm ? "warm" : "cold") + " initialization yields", async () => {
  const f = await fixture();
  let release = () => {};
  if (warm) await f.api.init();
  else f.state.wait = new Promise(resolve => { release = resolve; });
  const files = chapters(), shared = resources();
  const expected = structuredClone({ files, shared });
  const image = new Uint8Array([99, 1, 2, 88]), font = new Uint8Array([77, 3, 4, 66]);
  const page = { size: { widthPt: 720, heightPt: 540 }, margins: 24 };
  const options = { includeSources: shared, title: "Before", author: "Writer", lang: "de", page,
    pdfImages: [{ destination: "parts/figure.png", bytes: image.subarray(1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: new DataView(font.buffer, 1, 2), weight: 550 }] };
  const pending = f.api.renderBookPdf(files, options);
  files[0].path = "changed.md"; files[0].source = "# Later"; files.reverse();
  shared[0].path = "wrong.md"; shared[0].source = "Later include"; shared.length = 0;
  options.includeSources = []; options.expandIncludes = false; options.title = "After";
  page.size.widthPt = 900; image.fill(9); font.fill(9);
  release();
  await pending;
  const call = f.state.calls[0], args = call.args;
  assert.deepEqual(capture(call), {
    paths: expected.files.map(file => file.path), sources: expected.files.map(file => file.source),
    includePaths: expected.shared.map(file => file.path), includeSources: expected.shared.map(file => file.source),
    expandIncludes: true,
  });
  assert.equal(args[4], "Before"); assert.equal(args[5], "Writer"); assert.equal(args[23], "de");
  assert.deepEqual(args[9], ["parts/figure.png"]);
  assert.deepEqual([...args[10]], [1, 2]); assert.deepEqual([...args[11]], [2]);
  assert.deepEqual([...args[12]], [3, 4]); assert.deepEqual([...args[17]], [550, 0, 0, 0, 0]);
  assert.deepEqual([...args[28]], [720, 540, 24, 24, 24, 24]);
  assert.equal(image.byteLength, 4); assert.equal(font.byteLength, 4);
});

test("empty and explicit default source options preserve legacy eight-argument and profiled 39-argument calls", async () => {
  const f = await fixture({ sources: false }), files = chapters();
  for (const profile of [undefined, "commonmark-gfm"]) {
    const base = { title: "Manual", author: "Writer", ...(profile === undefined ? {} : { profile }) };
    await f.api.renderBookPdf(files, base);
    const original = f.state.calls.at(-1);
    assert.equal(original.kind, profile === undefined ? "legacy" : "profiled");
    assert.equal(original.args.length, profile === undefined ? 8 : 39);
    for (const extra of [{ includeSources: [] }, { expandIncludes: true },
      { includeSources: [], expandIncludes: true }, { includeSources: undefined, expandIncludes: undefined }]) {
      await f.api.renderBookPdf(files, { ...base, ...extra });
      assert.deepEqual(f.state.calls.at(-1), original);
    }
  }
});

test("a sources-only generated export carries profile, grid, spacing and supported pagination modes", async () => {
  for (const profile of [undefined, "commonmark-gfm", "gfm-plus"]) {
    // Strict pagination and split-footnote continuation are separate native
    // policies. Do not treat a dispatch double as proof of an invalid mixture.
    for (const policy of [{ strictPagination: true }, { splitFootnotes: true }]) {
      const f = await fixture({ legacy: false, profiled: false, advanced: false, grid: false });
      const output = await f.api.renderBookPdf(chapters(), {
        includeSources: resources(), profile, baselineGridPt: 12,
        typography: { lineHeight: 2, tableCellPaddingXEm: 0, tableCellPaddingYEm: 0.5 },
        footnoteBacklinks: true, ...policy,
        running: { header: { left: "{title}" }, footer: { center: "{page}" } },
      });
      const call = f.state.calls[0], args = call.args;
      assert.equal(call.kind, "sources"); assert.equal(args.length, 42);
      for (const token of ["theme-spacing", "line-height=2", "table-padding-x=0", "table-padding-y=0.5",
        "footnote-backlinks", policy.strictPagination ? "strict-pagination" : "split-footnotes"])
        assert.ok(args[36].split(",").includes(token), "missing typography token: " + token);
      assert.equal(args[37], 12); assert.equal(args[38], profile);
      assert.deepEqual(args[29], ["{title}", "", "", "", "{page}", ""]);
      assert.equal(output.profile, profile);
      assert.equal(f.state.freed, 1);
    }
  }
});

test("missing, malformed and short source bindings fail before reading sources, assets or initializing", async () => {
  for (const setup of [{ sources: false }, { sources: "malformed" }, { arity: 41 }]) {
    for (const parseOnly of [false, true]) {
      const f = await fixture(setup);
      let read = 0;
      const file = { get path() { read++; throw new Error("source path read"); },
        get source() { read++; throw new Error("source text read"); } };
      const options = { ...(parseOnly ? { expandIncludes: false } : { includeSources: [file] }),
        get pdfImages() { read++; throw new Error("image payload read"); },
        get fontAssets() { read++; throw new Error("font payload read"); } };
      await assert.rejects(f.api.renderBookPdf([file], options), { code: "UNSUPPORTED_WASM_PACKAGE" });
      assert.equal(read, 0); assert.equal(f.state.loads, 0); assert.equal(f.state.calls.length, 0);
    }
  }
});

test("invalid expansion flags and disabled expansion with resources fail before source or asset reads", async () => {
  const f = await fixture();
  let read = 0;
  const file = { get path() { read++; throw new Error("source path read"); },
    get source() { read++; throw new Error("source text read"); } };
  for (const expandIncludes of [null, 0, 1, "", "true", {}, []])
    await assert.rejects(f.api.renderBookPdf([file], { expandIncludes,
      get pdfImages() { read++; throw new Error("image read"); } }), /expandIncludes/);
  await assert.rejects(f.api.renderBookPdf([file], {
    includeSources: [file], expandIncludes: false,
    get pdfImages() { read++; throw new Error("image read"); },
  }), /includeSources.*expandIncludes|expandIncludes.*includeSources/);
  assert.equal(read, 0); assert.equal(f.state.loads, 0); assert.equal(f.state.calls.length, 0);
});

test("malformed include source containers are rejected before initialization", async () => {
  const f = await fixture();
  for (const includeSources of [null, {}, "part.md", [null], [undefined], new Array(1),
    [{ path: "", source: "content" }], [{ source: "content" }]])
    await assert.rejects(f.api.renderBookPdf(chapters(), { includeSources }));
  assert.equal(f.state.loads, 0); assert.equal(f.state.calls.length, 0);
});

test("include-source getters retain root coercions and are captured exactly once", async () => {
  const f = await fixture();
  let optionReads = 0, policyReads = 0, pathReads = 0, sourceReads = 0;
  let path = "  parts/number.md  ", source = 42, shared;
  shared = [{ get path() { pathReads++; return path; }, get source() { sourceReads++; return source; } }];
  const options = { get includeSources() { optionReads++; return shared; },
    get expandIncludes() { policyReads++; return true; } };
  const pending = f.api.renderBookPdf(chapters(), options);
  path = "later.md"; source = "Later"; shared = [];
  await pending;
  assert.equal(optionReads, 1); assert.equal(policyReads, 1);
  assert.equal(pathReads, 1); assert.equal(sourceReads, 1);
  assert.deepEqual(f.state.calls[0].args[39], ["parts/number.md"]);
  assert.deepEqual(f.state.calls[0].args[40], ["42"]);
});

test("an initially empty include capture cannot acquire sources from a later option getter", async () => {
  for (const profile of [undefined, "commonmark-gfm"]) {
    const f = await fixture(), shared = [];
    let reads = 0;
    const unrequested = {
      get path() { reads++; throw new Error("late resource read"); },
      get source() { reads++; throw new Error("late resource read"); },
    };
    const output = await f.api.renderBookPdf(chapters(), {
      includeSources: shared,
      get profile() { shared.push(unrequested); return profile; },
    });
    assert.equal(reads, 0);
    assert.equal(shared.length, 1, "the option getter actually mutated the caller's collection");
    const call = f.state.calls[0];
    assert.equal(call.kind, profile === undefined ? "legacy" : "profiled");
    assert.equal(call.args.length, profile === undefined ? 8 : 39);
    assert.equal(output.sourceLength, chapters().reduce((total, file) => total + utf8Length(file.source), 0));
  }
});

test("later option getters cannot remove an initially captured include source", async () => {
  for (const profile of [undefined, "gfm-plus"]) {
    const f = await fixture(), shared = resources(), expected = structuredClone(shared);
    await f.api.renderBookPdf(chapters(), {
      includeSources: shared,
      get profile() { shared.length = 0; return profile; },
    });
    assert.equal(shared.length, 0);
    const call = f.state.calls[0];
    assert.equal(call.kind, "sources"); assert.equal(call.args.length, 42);
    assert.deepEqual(call.args[39], expected.map(file => file.path));
    assert.deepEqual(call.args[40], expected.map(file => file.source));
    assert.equal(call.args[38], profile);
  }
});

test("include array length and entries are captured once before later option callbacks", async () => {
  const f = await fixture();
  const first = resources()[0], later = { path: "later.md", source: "Later content" };
  let selected = first, countReads = 0, entryReads = 0, optionReads = 0;
  const entries = [];
  Object.defineProperty(entries, "0", { get() { entryReads++; return selected; } });
  const shared = new Proxy(entries, {
    get(target, key, receiver) {
      if (key === "length") countReads++;
      return Reflect.get(target, key, receiver);
    },
  });
  await f.api.renderBookPdf(chapters(), {
    get includeSources() { optionReads++; return shared; },
    get profile() { selected = later; return undefined; },
  });
  assert.equal(optionReads, 1); assert.equal(countReads, 1); assert.equal(entryReads, 1);
  assert.deepEqual(f.state.calls[0].args[39], [first.path]);
  assert.deepEqual(f.state.calls[0].args[40], [first.source]);
});

test("chapter and include-only counts share the 4096-file admission limit", async () => {
  const f = await fixture();
  let read = 0;
  const file = { get path() { read++; throw new Error("excess source read"); },
    get source() { read++; throw new Error("excess source read"); } };
  await assert.rejects(f.api.renderBookPdf([{ path: "a.md", source: "" }], {
    includeSources: new Array(4096).fill(file),
  }), /4096|source.*limit|source.*count/);
  assert.equal(read, 0); assert.equal(f.state.loads, 0);
  const files = [{ path: "a.md", source: "" }];
  const shared = Array.from({ length: 4095 }, (_, i) => ({ path: "part-" + i + ".md", source: "" }));
  await f.api.renderBookPdf(files, { includeSources: shared });
  assert.equal(f.state.calls[0].args[0].length, 1);
  assert.equal(f.state.calls[0].args[39].length, 4095);
});

test("malformed Unicode in include paths and text is rejected before payload copying", async () => {
  const f = await fixture();
  let read = 0;
  for (const shared of [
    [{ path: "parts/\ud800.md", source: "content" }],
    [{ path: "part.md", source: "\udc00" }],
    [{ path: "part.md", source: "before\ud800after" }],
  ]) await assert.rejects(f.api.renderBookPdf(chapters(), { includeSources: shared,
    pdfImages: [{ destination: "image", get bytes() { read++; throw new Error("asset bytes read"); } }],
  }), /surrogate|Unicode|UTF-8/);
  assert.equal(read, 0); assert.equal(f.state.loads, 0); assert.equal(f.state.calls.length, 0);
});

test("UTF-8 bytes from both source roles and their paths share one 64 MiB budget", async () => {
  const f = await fixture();
  // Reuse one immutable string. Each role alone is 32 MiB, while their paths
  // make the combined UTF-8 input exceed 64 MiB despite only 32 Mi UTF-16 units.
  const text = "\u00e9".repeat(16 * 1024 * 1024);
  let read = 0;
  await assert.rejects(f.api.renderBookPdf([{ path: "a.md", source: text }], {
    includeSources: [{ path: "b.md", source: text }],
    pdfImages: [{ destination: "image", get bytes() { read++; throw new Error("payload read"); } }],
  }), /UTF-8|byte limit|budget/);
  assert.equal(read, 0); assert.equal(f.state.loads, 0); assert.equal(f.state.calls.length, 0);
});

test("native expansion failures do not retry a legacy route", async () => {
  const f = await fixture();
  f.state.failure = "include_cycle: parts/shared.md -> parts/shared.md";
  await assert.rejects(f.api.renderBookPdf(chapters(), { includeSources: resources() }),
    error => String(error).includes("include_cycle"));
  assert.deepEqual(f.state.calls.map(call => call.kind), ["sources"]);
  assert.equal(f.state.freed, 0);
});

function worker(t) {
  const threads = [], listeners = new Map();
  const client = createBookWorkerClient({ timeoutMs: 5000, workerFactory() {
    const thread = new Worker(new URL("./book_pdf_sources_worker_fixture.mjs", import.meta.url));
    threads.push(thread);
    return {
      addEventListener(type, callback) {
        const listener = type === "message" ? value => callback({ data: value }) : callback;
        listeners.set(callback, listener); thread.on(type, listener);
      },
      removeEventListener(type, callback) { thread.off(type, listeners.get(callback)); },
      postMessage(value, transfer) { thread.postMessage(value, transfer); },
      terminate() { return thread.terminate(); },
    };
  } });
  t.after(async () => {
    client.dispose();
    await Promise.all(threads.map(thread => thread.terminate()));
  });
  return client;
}

test("one-shot PDF and the existing real book worker retain identical chapter/include roles and policy", async t => {
  for (const profile of [undefined, "commonmark-gfm", "gfm-plus"]) {
    for (const expandIncludes of [true, false]) {
      const f = await fixture(), client = worker(t), files = chapters();
      const shared = expandIncludes ? resources() : [];
      const options = { includeSources: shared, expandIncludes, ...(profile === undefined ? {} : { profile }) };
      const [direct, threaded] = await Promise.all([
        f.api.renderBookPdf(files, options), client.render(files, "pdf", options),
      ]);
      const remote = JSON.parse(new TextDecoder().decode(threaded.bytes));
      assert.deepEqual(capture(f.state.calls[0]), {
        paths: remote.paths, sources: remote.sources, includePaths: remote.includePaths,
        includeSources: remote.includeSources, expandIncludes: remote.expandIncludes,
      });
      assert.equal(threaded.sourceLength, direct.sourceLength);
      assert.equal(threaded.profile, profile);
      assert.equal(direct.profile, profile);
      assert.equal(remote.paths.length, 2, "include-only files must not become chapters");
    }
  }
});

test("a real book worker preserves the same captured sources and resource bytes during host edits", async t => {
  const f = await fixture(), client = worker(t);
  const files = chapters(), shared = resources();
  const expected = structuredClone({ files, shared });
  const image = new Uint8Array([9, 1, 2, 8]);
  const options = { includeSources: shared, images: [{ destination: "parts/plot.png", bytes: image.subarray(1, 3) }] };
  const pending = client.render(files, "pdf", options);
  files[0].source = "Edited chapter"; shared[0].source = "Edited include"; shared.length = 0; image.fill(5);
  const output = await pending, remote = JSON.parse(new TextDecoder().decode(output.bytes));
  await f.api.renderBookPdf(expected.files, { includeSources: expected.shared,
    pdfImages: [{ destination: "parts/plot.png", bytes: new Uint8Array([1, 2]) }] });
  assert.deepEqual(capture(f.state.calls[0]), {
    paths: remote.paths, sources: remote.sources, includePaths: remote.includePaths,
    includeSources: remote.includeSources, expandIncludes: remote.expandIncludes,
  });
  assert.deepEqual(remote.images, [{ destination: "parts/plot.png", bytes: [1, 2] }]);
  assert.deepEqual([...f.state.calls[0].args[10]], [1, 2]);
  assert.equal(output.sourceLength, [...expected.files, ...expected.shared]
    .reduce((sum, file) => sum + utf8Length(file.source), 0));
  assert.equal(image.byteLength, 4);
});
