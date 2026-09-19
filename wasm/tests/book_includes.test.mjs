// Production facade + worker transport; the WASM class and Worker endpoint
// are explicit doubles. Rust source_bundle tests own expansion semantics.
import test from "node:test";
import assert from "node:assert/strict";
import { createBookBindings, prepareBookInput, bookTextBytes } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";

const chapter = () => ({ path: "guide/start.md", source: "# Manual\n\n{{#include ../parts/shared.md:example}}\n" });
const resource = () => ({ path: "parts/shared.md", source: "<!-- ANCHOR: example -->\nShared 中🚀\n<!-- ANCHOR_END: example -->\n" });
function harness(load) {
  const state = { loads: 0, bundles: [], instances: [], calls: [], frees: 0, failure: null };
  class EngineBook {
    constructor(paths, sources) {
      this.chapterCount = paths.length;
      this.sourceLength = sources.reduce((sum, source) => sum + bookTextBytes(source), 0);
      this.dead = false; state.instances.push(this);
      state.calls.push(["new", paths, sources]);
    }
    static fromSources(paths, sources, includePaths, includeSources) {
      state.bundles.push({ paths, sources, includePaths, includeSources });
      if (state.failure !== null) throw state.failure;
      const raw = new EngineBook(paths, sources);
      raw.sourceLength += includeSources.reduce((sum, source) => sum + bookTextBytes(source), 0);
      return raw;
    }
    setMetadata(...values) { state.calls.push(["metadata", ...values]); }
    setCustomCss(...values) { state.calls.push(["css", ...values]); }
    setTheme(...values) { state.calls.push(["theme", ...values]); }
    setNavigation(...values) { state.calls.push(["navigation", ...values]); }
    setFontScale(...values) { state.calls.push(["scale", ...values]); }
    setImage(destination, bytes) { state.calls.push(["image", destination, [...bytes]]); }
    setFont(slot, bytes) { state.calls.push(["font", slot, [...bytes]]); }
    setFontWeight(...values) { state.calls.push(["weight", ...values]); }
    renderPdf() { return new Uint8Array([37, 80, 68, 70]); }
    renderEpub() { return new Uint8Array([80, 75, 3, 4]); }
    renderSite() { return new Uint8Array([80, 75, 1, 2]); }
    free() { assert.equal(this.dead, false); this.dead = true; state.frees++; }
  }
  const api = createBookBindings(async () => { state.loads++; if (load) await load(); return EngineBook; });
  return { api, state, EngineBook };
}

test("default expansion delegates ordered chapters and include-only sources to Rust once", async () => {
  const { api, state } = harness();
  const files = [chapter(), { path: "end.md", source: "# End" }], shared = resource();
  const session = await api.createBook(files, { includeSources: [shared] });
  assert.deepEqual(state.bundles[0], { paths: files.map(file => file.path), sources: files.map(file => file.source),
    includePaths: [shared.path], includeSources: [shared.source] });
  assert.equal(session.chapterCount, 2, "resources must not be appended to the chapter array");
  const bytes = [...files, shared].reduce((sum, file) => sum + bookTextBytes(file.source), 0);
  assert.equal(session.sourceLength, bytes);
  for (const output of [session.renderPdf(), session.renderEpub(), session.renderSite()]) assert.equal(output.sourceLength, bytes);
  assert.equal(state.bundles.length, 1);
  assert.equal(state.instances.length, 1);
  session.dispose(); session.dispose(); assert.equal(state.frees, 1);
});

test("chapters alone are an include source set and selectors are never parsed by JavaScript", async () => {
  const { api, state } = harness();
  const files = [chapter(), resource()];
  const session = await api.createBook(files);
  assert.deepEqual(state.bundles[0].sources, files.map(file => file.source));
  assert.deepEqual(state.bundles[0].includePaths, []);
  assert.deepEqual(state.bundles[0].includeSources, []);
  session.dispose();
});

test("code examples and unused resources still reach Rust for semantic validation", async () => {
  const { api, state } = harness();
  const source = "```md\n{{#include missing.md}}\n```\n";
  const literal = await api.createBook([{ path: "manual.md", source }]);
  assert.equal(state.bundles[0].sources[0], source); literal.dispose();
  const unused = await api.createBook([{ path: "plain.md", source: "# Plain" }], { includeSources: [resource()] });
  assert.equal(state.bundles[1].includePaths[0], resource().path); unused.dispose();
});

test("explicit parse-only mode preserves directives and rejects silently ignored resources", async () => {
  const { api, state } = harness();
  const session = await api.createBook([chapter()], { expandIncludes: false });
  assert.equal(state.bundles.length, 0);
  assert.equal(state.calls[0][2][0], chapter().source); session.dispose();
  const loads = state.loads;
  await assert.rejects(api.createBook([chapter()], { expandIncludes: false, includeSources: [resource()] }), /includeSources requires/);
  assert.equal(state.loads, loads);
});

test("outdated engines never silently render an unexpanded book", async () => {
  const { api, state, EngineBook } = harness(); delete EngineBook.fromSources;
  await assert.rejects(api.createBook([chapter()]), /FmdBook.fromSources.*rebuild/);
  assert.equal(state.instances.length, 0);
  const plain = await api.createBook([{ path: "a.md", source: "# Plain" }]); plain.dispose();
  const literal = await api.createBook([chapter()], { expandIncludes: false }); literal.dispose();
  assert.equal(state.frees, 2);
});

test("chapter/resource/options snapshots survive changes during asynchronous engine loading", async () => {
  let release; const gate = new Promise(resolve => { release = resolve; });
  const { api, state } = harness(() => gate);
  const files = [chapter()], shared = [resource()], image = new Uint8Array([1, 2, 3]);
  const expected = structuredClone({ files, shared });
  const options = { includeSources: shared, title: "Original", images: [{ destination: "x.png", bytes: image }] };
  const pending = api.createBook(files, options);
  files[0].source = "Different"; files.push(resource()); shared[0].path = "wrong.md"; shared[0].source = "Changed";
  shared.push(chapter()); options.includeSources = []; options.expandIncludes = false; options.title = "Changed"; image.fill(9);
  release(); const session = await pending;
  assert.deepEqual(state.bundles[0], { paths: expected.files.map(file => file.path), sources: expected.files.map(file => file.source),
    includePaths: expected.shared.map(file => file.path), includeSources: expected.shared.map(file => file.source) });
  assert.deepEqual(state.calls.find(call => call[0] === "image"), ["image", "x.png", [1, 2, 3]]);
  assert.equal(state.calls.find(call => call[0] === "metadata")[1], "Original"); session.dispose();
});

test("source field getters are captured once for validation and dispatch", () => {
  let paths = 0, sources = 0;
  const file = { get path() { paths++; return "a.md"; }, get source() { sources++; return "# A"; } };
  const input = prepareBookInput([file], { includeSources: [file] });
  assert.equal(paths, 2); assert.equal(sources, 2);
  assert.deepEqual(input.files, [{ path: "a.md", source: "# A" }]);
  assert.deepEqual(input.options.includeSources, input.files);
  assert.notEqual(input.options.includeSources[0], input.files[0]);
});

test("resource shape, malformed Unicode, and expansion-option types fail before loading", async () => {
  const { api, state } = harness();
  for (const includeSources of [{}, "x", [null], [{ path: "x", source: 1 }], [{ path: "bad\ud800", source: "text" }],
    [{ path: "x", source: "bad\udc00" }]]) await assert.rejects(api.createBook([chapter()], { includeSources }));
  for (const expandIncludes of [null, 0, 1, "true", {}]) await assert.rejects(api.createBook([chapter()], { expandIncludes }));
  assert.equal(state.loads, 0);
});

test("4096-source budget is combined across chapters and resources", () => {
  const resources = Array.from({ length: 4095 }, (_, i) => ({ path: `part-${i}.txt`, source: "" }));
  assert.equal(prepareBookInput([chapter()], { includeSources: resources }).options.includeSources.length, 4095);
  assert.throws(() => prepareBookInput([chapter(), resource()], { includeSources: resources }), /4096/);
  assert.throws(() => prepareBookInput([chapter()], { includeSources: new Array(4096).fill(resource()) }), /4096/);
});

test("64 MiB UTF-8 budget includes resource paths and chapter source together", async () => {
  const { api, state } = harness();
  const half = "x".repeat(32 * 1024 * 1024);
  await assert.rejects(api.createBook([{ path: "a.md", source: half }],
    { includeSources: [{ path: "b.txt", source: half }] }), /64 MiB|budget/);
  assert.equal(state.loads, 0);
});

test("one-shot helpers retain source bundles across every format and dispose each book", async () => {
  const { api, state } = harness();
  for (const method of ["renderBookPdf", "renderBookEpub", "renderBookSite"]) {
    const output = await api[method]([chapter()], { includeSources: [resource()], fontScale: 1.125, toc: true });
    assert.ok(output.bytes.length);
  }
  assert.equal(state.bundles.length, 3); assert.equal(state.frees, 3);
  assert.equal(state.calls.filter(call => call[0] === "scale" && call[1] === 1.125).length, 3);
});

test("raw Rust expansion errors become bounded Errors without allocating a book", async () => {
  const { api, state } = harness(); state.failure = "include_cycle: " + "x".repeat(10000);
  await assert.rejects(api.createBook([chapter()]), error => error instanceof Error
    && error.message.startsWith("include_cycle:") && error.message.length === 2048);
  assert.equal(state.instances.length, 0); assert.equal(state.frees, 0);
});

test("post-expansion configuration failure still releases the new WASM book", async () => {
  const { api, state, EngineBook } = harness(); const failure = new Error("bad font");
  EngineBook.prototype.setFont = () => { throw failure; };
  await assert.rejects(api.createBook([chapter()], { includeSources: [resource()],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([1]) }] }), error => error === failure);
  assert.equal(state.bundles.length, 1); assert.equal(state.frees, 1);
});

// A structured-clone transport double runs both production endpoints. It does
// not claim native browser threading, cancellation of WASM, or real rendering.
function transport(engine) {
  const state = { created: 0, terminated: 0, messages: [] };
  const workerFactory = () => {
    state.created++; let handler, dead = false; const listeners = new Map();
    const scope = {
      addEventListener(kind, callback) { assert.equal(kind, "message"); handler = callback; },
      postMessage(value, transfer) {
        const data = structuredClone(value, { transfer });
        queueMicrotask(() => { if (!dead) listeners.get("message")?.({ data }); });
      }
    };
    installBookWorker(scope, engine);
    return {
      addEventListener(kind, callback) { listeners.set(kind, callback); },
      removeEventListener(kind, callback) { if (listeners.get(kind) === callback) listeners.delete(kind); },
      terminate() { dead = true; state.terminated++; },
      postMessage(value, transfer) {
        const data = structuredClone(value, { transfer }); state.messages.push(data);
        queueMicrotask(() => { if (!dead) void handler({ data }); });
      }
    };
  };
  return { state, client: createBookWorkerClient({ workerFactory, timeoutMs: 1000 }) };
}

test("production worker endpoints forward resources and immutable snapshots to the real facade", async () => {
  const { api, state } = harness(), channel = transport(api);
  const files = [chapter()], shared = [resource()], expected = structuredClone(shared);
  const image = new Uint8Array([1, 2, 3]);
  const pending = channel.client.render(files, "site", { includeSources: shared, images: [{ destination: "x.png", bytes: image }] });
  shared[0].source = "Different"; shared.length = 0; files[0].source = "No include"; image.fill(9);
  const output = await pending;
  assert.deepEqual(state.bundles[0].includeSources, expected.map(file => file.source));
  assert.equal(state.bundles[0].sources[0], chapter().source);
  assert.equal(output.sourceLength, bookTextBytes(chapter().source) + bookTextBytes(expected[0].source));
  assert.deepEqual(state.calls.find(call => call[0] === "image"), ["image", "x.png", [1, 2, 3]]);
  assert.equal(image.byteLength, 3, "the host's asset buffer was not detached");
  assert.equal(channel.state.terminated, 1); assert.equal(state.frees, 1); assert.equal(channel.client.busy, false);
  channel.client.dispose();
});

test("include failures keep their reason through worker diagnostics and leave the client reusable", async () => {
  const { api, state } = harness(), channel = transport(api);
  state.failure = "include_missing: cannot read parts/shared.md";
  await assert.rejects(channel.client.render([chapter()], "pdf", { includeSources: [resource()] }),
    error => error.code === "BOOK_ERROR" && error.message === state.failure);
  assert.equal(channel.state.terminated, 1); assert.equal(channel.client.busy, false);
  state.failure = null;
  assert.equal((await channel.client.render([chapter()], "epub", { includeSources: [resource()] })).extension, "epub");
  assert.equal(channel.state.terminated, 2); assert.equal(state.frees, 1); channel.client.dispose();
});
