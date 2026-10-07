// Production adapters with an explicit stateful binding double. This suite
// proves admission, lifecycle and wire behavior, not Rust parsing or WASM PDF.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings } from "../book_session.mjs";

const file = (path, source = "# Text") => ({ path, source });
const bytes = value => Buffer.byteLength(value, "utf8");
const failure = code => error => error?.code === code;
const initial = [file("one.md", "# One"), file("two.md", "# Two")];

function harness(overrides = {}) {
  const state = { constructions: 0, frees: 0, replacements: 0, updates: 0, calls: [] };
  class EngineBook {
    constructor(paths, sources) {
      state.constructions++;
      state.raw = this;
      this.paths = [...paths];
      this.sources = [...sources];
      this.includes = [];
      this.includeSources = [];
      this.sourceRevision = 0;
      this.expanding = false;
      this.images = [];
      this.fonts = [];
      this.dead = false;
    }
    static fromSources(paths, sources, includes, includeSources) {
      const raw = new this(paths, sources);
      raw.includes = [...includes];
      raw.includeSources = [...includeSources];
      raw.expanding = true;
      return raw;
    }
    get chapterCount() { return this.paths.length; }
    get sourceLength() { return [...this.sources, ...this.includeSources].reduce((n, s) => n + bytes(s), 0); }
    setMetadata(...values) { this.metadata = values; }
    setCustomCss(value) { this.css = value; }
    setTheme(...values) { this.theme = values; }
    setNavigation(...values) { this.navigation = values; }
    setFontScale(value) { this.scale = value; }
    setPdfOptions(...values) { this.pdf = values; }
    setHtmlFontFormat(value) { this.format = value; }
    setImage(key, value) { this.images.push([key, value]); }
    setFont(key, value) { this.fonts.push([key, value]); }
    setFontWeight() {}
    renderPdf() { assert.equal(this.dead, false); return new Uint8Array([1, this.chapterCount]); }
    renderPdfWithPage(geometry) { this.lastPage = [...geometry]; return this.renderPdf(); }
    renderEpub() { return this.renderPdf(); }
    renderSite() { return this.renderPdf(); }
    free() { assert.equal(this.dead, false); this.dead = true; state.frees++; }
    replaceSources(paths, sources, includes, includeSources, revision) {
      assert.equal(this.dead, false);
      state.replacements++;
      state.calls.push([paths, sources, includes, includeSources, revision]);
      if (revision !== this.sourceRevision) throw new Error("stale native revision");
      if (!this.expanding && includes.length) throw "include-only sources require an expanding book";
      const changed = JSON.stringify([paths, sources, includes, includeSources])
        !== JSON.stringify([this.paths, this.sources, this.includes, this.includeSources]);
      if (changed && this.sourceRevision === 0xffffffff) throw "source revision exhausted";
      this.paths = paths; this.sources = sources; this.includes = includes; this.includeSources = includeSources;
      if (changed) this.sourceRevision++;
      return JSON.stringify({ schema: "fmd-book-source-set-v1", revision: this.sourceRevision,
        source_length: this.sourceLength, chapter_count: this.chapterCount, resource_count: includes.length,
        changed, reparsed_chapter_count: changed ? paths.length : 0 });
    }
    updateSources(paths, sources, revision) {
      state.updates++;
      assert.equal(revision, this.sourceRevision);
      const index = this.paths.indexOf(paths[0]);
      if (index < 0) throw "unknown source";
      const changed = this.sources[index] !== sources[0];
      if (changed) { this.sources[index] = sources[0]; this.sourceRevision++; }
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        chapter_count: this.chapterCount, source_length: this.sourceLength,
        changed_sources: changed ? 1 : 0, reparsed_chapters: changed ? [index] : [] });
    }
  }
  Object.defineProperties(EngineBook.prototype, Object.getOwnPropertyDescriptors(overrides));
  return { state, EngineBook, api: createBookBindings(async () => EngineBook) };
}

async function setup(overrides = {}, options = {}) {
  const h = harness(overrides);
  h.session = await h.api.createBook(initial, options);
  return h;
}

test("complete replacement changes reading order and membership on the retained handle", async () => {
  const { session, state } = await setup({}, { title: "Manual", author: "Writer", customCss: "", toc: true,
    baseFontSize: 12, optimalPagination: true, metadataEpochSeconds: 0,
    page: { size: "a4", margins: 24 }, images: [{ destination: "x.svg", bytes: new Uint8Array([7]) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([8]) }] });
  const raw = state.raw, image = raw.images[0][1], font = raw.fonts[0][1], profile = raw.pdf;
  const result = session.replaceSources([file("new.md", "中"), initial[1]], {
    includeSources: [file("part.txt", "😀")], expectedRevision: 0,
  });
  assert.deepEqual(result, { revision: 1, sourceLength: 12, chapterCount: 2, resourceCount: 1,
    changed: true, reparsedChapterCount: 2 });
  assert.equal(Object.isFrozen(result), true);
  assert.equal(session.sourceRevision, 1);
  assert.equal(session.sourceLength, 12);
  assert.deepEqual(raw.paths, ["new.md", "two.md"]);
  assert.equal(raw.pdf, profile);
  assert.equal(raw.images[0][1], image);
  assert.equal(raw.fonts[0][1], font);
  assert.deepEqual(raw.metadata, ["Manual", "Writer", undefined]);
  assert.equal(raw.css, "");
  assert.deepEqual([...session.renderPdf().bytes], [1, 2]);
  assert.equal(raw.lastPage[2], 24);
  assert.equal(state.constructions, 1);
  session.dispose();
  assert.equal(state.frees, 1);
});

test("omitted includeSources means the complete next resource set is empty", async () => {
  const { session, state } = await setup({}, { includeSources: [file("part.md")] });
  const result = session.replaceSources([initial[1]]);
  assert.equal(result.resourceCount, 0);
  assert.equal(result.chapterCount, 1);
  assert.deepEqual(state.raw.includes, []);
  session.dispose();
});

test("exact no-op retains revision and reports no reparsing", async () => {
  const { session } = await setup();
  assert.deepEqual(session.replaceSources(initial), { revision: 0, sourceLength: 10, chapterCount: 2,
    resourceCount: 0, changed: false, reparsedChapterCount: 0 });
  session.dispose();
});

test("old text edit tickets cannot cross a source-set revision", async () => {
  const { session, state } = await setup();
  session.replaceSources([initial[1], initial[0]]);
  assert.throws(() => session.updateSources([initial[0]], { expectedRevision: 0 }), failure("STALE_BOOK_REVISION"));
  assert.equal(state.updates, 0);
  const update = session.updateSources([file("two.md", "# Edited")], { expectedRevision: 1 });
  assert.equal(update.revision, 2);
  assert.throws(() => session.replaceSources(initial, { expectedRevision: 1 }), failure("STALE_BOOK_REVISION"));
  assert.equal(state.replacements, 1);
  session.dispose();
});

test("stale admission precedes chapter and resource getters", async () => {
  const { session, state } = await setup();
  let reads = 0;
  const source = { get path() { reads++; throw new Error("must not read"); } };
  assert.throws(() => session.replaceSources([source], { includeSources: [source], expectedRevision: 5 }), failure("STALE_BOOK_REVISION"));
  assert.equal(reads, 0);
  assert.equal(state.replacements, 0);
  session.dispose();
});

test("each source getter is captured once and later mutations cannot retarget submitted arrays", async () => {
  const { session, state } = await setup();
  let paths = 0, sources = 0;
  const item = { get path() { return ++paths === 1 ? "captured.md" : "wrong.md"; },
    get source() { return ++sources === 1 ? "Captured" : "Wrong"; } };
  const resources = [file("part.txt", "Resource")];
  const requested = [item];
  session.replaceSources(requested, { includeSources: resources });
  requested[0] = file("changed.md"); resources[0].source = "Changed";
  assert.equal(paths, 1); assert.equal(sources, 1);
  assert.deepEqual(state.calls[0], [["captured.md"], ["Captured"], ["part.txt"], ["Resource"], 0]);
  session.dispose();
});

test("array custom iterators do not escape validated traversal", async () => {
  const { session, state } = await setup();
  const roots = [file("only.md")], includes = [file("part.txt")];
  roots[Symbol.iterator] = includes[Symbol.iterator] = () => { throw new Error("not used"); };
  session.replaceSources(roots, { includeSources: includes });
  assert.deepEqual(state.raw.paths, ["only.md"]);
  session.dispose();
});

test("UTF-8 source counts exclude logical path bytes and preserve BOM and CRLF", async () => {
  const { session } = await setup();
  const source = "\uFEFF# 中\r\n😀\r\n", resource = "β";
  const result = session.replaceSources([file("中文.md", source)], { includeSources: [file("é.txt", resource)] });
  assert.equal(result.sourceLength, bytes(source) + bytes(resource));
  session.dispose();
});

test("invalid source sets fail before invoking native replacement", async () => {
  const { session, state } = await setup();
  for (const roots of [[], {}, null, [null], [file("a.md", 9)], [file("\ud800", "x")],
    [file("a.md", "\udfff")], new Array(4097).fill(initial[0])]) {
    assert.throws(() => session.replaceSources(roots));
  }
  for (const includeSources of [null, {}, [null], [file("bad.txt", "\ud800")], new Array(4096).fill(file("x.txt"))]) {
    assert.throws(() => session.replaceSources(initial, { includeSources }));
  }
  assert.equal(state.replacements, 0);
  assert.equal(session.sourceRevision, 0);
  session.replaceSources([initial[0]]);
  session.dispose();
});

test("combined source/path budget is enforced before native copying", async () => {
  const { session, state } = await setup();
  const huge = "x".repeat(64 * 1024 * 1024);
  assert.throws(() => session.replaceSources([file("a.md", huge)]), /budget|64 MiB/);
  assert.throws(() => session.replaceSources([file(huge, "x")]), /budget|64 MiB/);
  assert.throws(() => session.replaceSources(initial, { includeSources: [file("part.txt", huge)] }), /budget|64 MiB/);
  assert.equal(state.replacements, 0);
  session.dispose();
});

test("replacement options reject unknown fields, accessors and lossy revision values", async () => {
  const { session, state } = await setup();
  for (const expectedRevision of [-1, 0.5, 2 ** 32, NaN, Infinity, "0", null]) {
    assert.throws(() => session.replaceSources(initial, { expectedRevision }), failure("INVALID_OPTIONS"));
  }
  for (const options of [null, [], new Date(), { title: "not an option" }, { expandIncludes: false },
    { [Symbol("unknown")]: 1 }, { get includeSources() { throw new Error("do not invoke"); } },
    { get expectedRevision() { throw new Error("do not invoke"); } }]) {
    assert.throws(() => session.replaceSources(initial, options), failure("INVALID_OPTIONS"));
  }
  session.replaceSources(initial, Object.assign(Object.create(null), { expectedRevision: 0 }));
  assert.equal(state.replacements, 1);
  session.dispose();
});

test("parse-only resource rejection leaves the previous book available", async () => {
  const { session, state } = await setup({}, { expandIncludes: false });
  assert.throws(() => session.replaceSources([file("new.md")], { includeSources: [file("part.txt")] }), /expanding book/);
  assert.equal(session.sourceRevision, 0);
  assert.deepEqual(state.raw.paths, ["one.md", "two.md"]);
  assert.deepEqual([...session.renderPdf().bytes], [1, 2]);
  session.replaceSources([file("new.md", "{{#include missing.md}}")]);
  session.dispose();
});

test("old WASM builds refuse collection edits without inspecting submitted sources", async () => {
  const { session, state } = await setup({ replaceSources: undefined });
  const roots = [{ get path() { throw new Error("must not read"); } }];
  assert.throws(() => session.replaceSources(roots), failure("UNSUPPORTED_BOOK_SOURCE_SET"));
  assert.equal(state.replacements, 0);
  assert.deepEqual([...session.renderPdf().bytes], [1, 2]);
  session.dispose();
});

test("a rejected native transaction retains the handle and bounded error reason", async () => {
  const { session, state, EngineBook } = await setup();
  const normal = EngineBook.prototype.replaceSources;
  EngineBook.prototype.replaceSources = () => { throw "include cycle: " + "x".repeat(5000); };
  assert.throws(() => session.replaceSources(initial), error => error instanceof Error && error.message.length === 2048);
  assert.equal(state.frees, 0);
  assert.equal(session.sourceRevision, 0);
  EngineBook.prototype.replaceSources = normal;
  session.replaceSources([initial[1]]);
  session.dispose();
});

test("reentrant selective and whole-set edits share one admission guard", async () => {
  const { session, state } = await setup();
  const root = { get path() {
    assert.throws(() => session.updateSources([initial[0]]), failure("BOOK_BUSY"));
    assert.throws(() => session.replaceSources(initial), failure("BOOK_BUSY"));
    return "new.md";
  }, source: "New" };
  session.replaceSources([root]);
  const update = { path: "new.md", get source() {
    assert.throws(() => session.replaceSources(initial), failure("BOOK_BUSY"));
    return "Edited";
  } };
  session.updateSources([update]);
  assert.equal(state.replacements, 1);
  assert.equal(state.updates, 1);
  session.dispose();
});

test("disposal from an input getter never calls a freed native handle", async () => {
  const { session, state } = await setup();
  assert.throws(() => session.replaceSources([{ path: "new.md", get source() { session.dispose(); return "New"; } }]), /disposed/);
  assert.equal(state.replacements, 0);
  assert.equal(state.frees, 1);
  session.dispose();
});

test("revision changes during admission are detected before native mutation", async () => {
  const { session, state } = await setup();
  assert.throws(() => session.replaceSources([{ path: "new.md", get source() { state.raw.sourceRevision++; return "New"; } }]), failure("STALE_BOOK_REVISION"));
  assert.equal(state.replacements, 0);
  session.dispose();
});

test("option Proxy traps also run inside the shared admission guard", async () => {
  const { session, state } = await setup();
  const options = new Proxy({}, { ownKeys() {
    assert.throws(() => session.updateSources([initial[0]]), failure("BOOK_BUSY"));
    session.dispose();
    return [];
  } });
  assert.throws(() => session.replaceSources(initial, options), /disposed/);
  assert.equal(state.replacements, 0);
  assert.equal(state.frees, 1);
});

test("malformed successful reports quarantine the handle instead of claiming rollback", async () => {
  for (const alter of [
    r => ({ ...r, schema: "wrong" }), r => ({ ...r, revision: r.revision + 1 }),
    r => ({ ...r, revision: 2 ** 32 }), r => ({ ...r, changed: "true" }),
    r => ({ ...r, chapter_count: 99 }), r => ({ ...r, resource_count: 1 }),
    r => ({ ...r, source_length: r.source_length + 1 }), r => ({ ...r, reparsed_chapter_count: 0 }),
    () => null, () => [], () => "x".repeat(5000),
  ]) {
    const { session, state, EngineBook } = await setup();
    const normal = EngineBook.prototype.replaceSources;
    EngineBook.prototype.replaceSources = function(...args) {
      return JSON.stringify(alter(JSON.parse(normal.apply(this, args))));
    };
    assert.throws(() => session.replaceSources([file("new.md")]), failure("INVALID_BOOK_SOURCE_SET_REPORT"));
    assert.equal(state.raw.sourceRevision, 1, "native success was not rolled back");
    assert.equal(state.frees, 1);
    assert.throws(() => session.renderPdf(), /disposed/);
    session.dispose();
  }
});

test("native post-state must independently agree with the report and admitted source bytes", async () => {
  const { session, state, EngineBook } = await setup();
  const normal = EngineBook.prototype.replaceSources;
  EngineBook.prototype.replaceSources = function(...args) {
    const result = normal.apply(this, args);
    this.sources[0] = "Corrupted content";
    return result;
  };
  assert.throws(() => session.replaceSources([file("new.md")]), failure("INVALID_BOOK_SOURCE_SET_REPORT"));
  assert.equal(state.frees, 1);
});

test("maximum revision accepts exact no-ops but never silently wraps", async () => {
  const { session, state } = await setup();
  state.raw.sourceRevision = 0xffffffff;
  const result = session.replaceSources(initial);
  assert.equal(result.revision, 0xffffffff);
  assert.equal(result.changed, false);
  assert.throws(() => session.replaceSources([initial[1]]), /exhausted/);
  assert.equal(session.sourceRevision, 0xffffffff);
  assert.equal(state.frees, 0);
  session.dispose();
});

test("quarantine remains effective when native cleanup itself throws", async () => {
  const { session, state, EngineBook } = await setup();
  EngineBook.prototype.replaceSources = () => "not json";
  EngineBook.prototype.free = () => { state.frees++; throw new Error("cleanup failed"); };
  assert.throws(() => session.replaceSources(initial), failure("INVALID_BOOK_SOURCE_SET_REPORT"));
  assert.equal(state.frees, 1);
  assert.throws(() => session.sourceRevision, /disposed/);
  session.dispose();
});
