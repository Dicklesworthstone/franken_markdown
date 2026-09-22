// Production session/admission code with an explicit stateful engine double.
// The double is NOT a Markdown parser and does NOT prove Rust or WASM behavior.
// Real-engine parity belongs to book_source_updates_smoke.mjs after a rebuild.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings } from "../book_session.mjs";

const file = (path, source) => ({ path, source });
const chapters = () => [file("one.md", "# One"), file("two.md", "# Two")];
const textBytes = s => new TextEncoder().encode(s).length;
const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));
function fixture({ editable = true, fromSources = true, load = async () => {} } = {}) {
  const state = { created: 0, freed: 0, modes: [], calls: [], raws: [], fail: null, corrupt: null };
  class EngineBook {
    constructor(paths, sources) {
      state.created++;
      this.paths = [...paths];
      this.files = new Map(paths.map((path, i) => [path, sources[i]]));
      this.sourceRevision = 0;
      this.chapterCount = paths.length;
      this.images = new Map();
      this.fonts = new Map();
      this.mode = "literal";
      state.modes.push(this.mode);
      state.raws.push(this);
    }
    static fromSources(paths, sources, includePaths, includeSources) {
      const raw = new EngineBook(paths, sources);
      includePaths.forEach((path, i) => raw.files.set(path, includeSources[i]));
      raw.mode = "expanded";
      state.modes[state.modes.length - 1] = raw.mode;
      return raw;
    }
    get sourceLength() { return [...this.files.values()].reduce((sum, s) => sum + textBytes(s), 0); }
    setMetadata(...values) { this.metadata = values; }
    setCustomCss(value) { this.css = value; }
    setTheme(...values) { this.theme = values; }
    setNavigation(...values) { this.navigation = values; }
    setFontScale(value) { this.scale = value; }
    setImage(key, bytes) { this.images.set(key, Array.from(bytes)); }
    setFont(key, bytes) { this.fonts.set(key, Array.from(bytes)); }
    setFontWeight(slot, weight) { this.weight = [slot, weight]; }
    updateSources(paths, sources, expected) {
      state.calls.push({ paths: [...paths], sources: [...sources], expected });
      if (state.fail) throw state.fail;
      assert.equal(expected, this.sourceRevision, "raw CAS must receive current revision");
      const next = new Map(this.files), seen = new Set(), changed = new Set();
      for (let i = 0; i < paths.length; i++) {
        // Minimal fixture identity only: real canonical path/include behavior is
        // tested by the Rust workspace suite, not reimplemented in JavaScript.
        if (!next.has(paths[i]) || seen.has(paths[i])) throw "book_update: unknown or duplicate source";
        seen.add(paths[i]);
        if (sources[i] !== next.get(paths[i])) changed.add(paths[i]);
        next.set(paths[i], sources[i]);
      }
      if (changed.size) {
        if (this.sourceRevision === 0xffffffff) throw "book_update: source revision exhausted";
        this.files = next;
        this.sourceRevision++;
      }
      const report = {
        schema: "fmd-book-source-update-v1", revision: this.sourceRevision,
        source_length: this.sourceLength, chapter_count: this.chapterCount,
        changed_sources: changed.size,
        reparsed_chapters: this.paths.flatMap((path, i) => changed.has(path) ? [i] : []),
      };
      return state.corrupt ? state.corrupt(report, this) : JSON.stringify(report);
    }
    render(kind, geometry) {
      return new TextEncoder().encode(JSON.stringify({
        kind, sources: [...this.files], metadata: this.metadata, css: this.css,
        images: [...this.images], fonts: [...this.fonts], geometry,
      }));
    }
    renderPdf() { return this.render("pdf"); }
    renderPdfWithPage(geometry) { return this.render("pdf", [...geometry]); }
    renderEpub() { return this.render("epub"); }
    renderSite() { return this.render("site"); }
    validateLinks() { return JSON.stringify({ revision: this.sourceRevision }); }
    free() { state.freed++; }
  }
  if (!editable) EngineBook.prototype.updateSources = undefined;
  if (!fromSources) EngineBook.fromSources = undefined;
  return {
    state,
    api: createBookBindings(async () => { await load(); return EngineBook; }),
  };
}

test("source edits update one retained session and every export without reinitializing it", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters(), {
    title: "Saved title", customCss: "kept", page: { size: "letter", orientation: "landscape" },
    images: [{ destination: "image.svg", bytes: new Uint8Array([1, 2]) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]), weight: 550 }],
  });
  assert.equal(book.sourceRevision, 0);
  const old = book.renderSite();
  const report = book.updateSources([file("one.md", "# Revised")], { expectedRevision: 0 });
  assert.deepEqual(report, { revision: 1, sourceLength: 14, chapterCount: 2, changedSources: 1, reparsedChapters: [0] });
  assert.deepEqual(state.calls[0], { paths: ["one.md"], sources: ["# Revised"], expected: 0 });
  assert.equal(Object.isFrozen(report), true);
  assert.equal(Object.isFrozen(report.reparsedChapters), true);
  assert.equal(book.sourceRevision, 1);
  assert.equal(book.sourceLength, 14);
  assert.equal(book.chapterCount, 2);
  for (const result of [book.renderPdf(), book.renderEpub(), book.renderSite()]) {
    const decoded = decode(result);
    assert.equal(decoded.sources[0][1], "# Revised");
    assert.deepEqual(decoded.images, [["image.svg", [1, 2]]]);
    assert.deepEqual(decoded.fonts, [["body-regular", [3, 4]]]);
    assert.equal(decoded.metadata[0], "Saved title");
    assert.equal(decoded.css, "kept");
    assert.equal(result.sourceLength, 14);
  }
  assert.deepEqual(decode(book.renderPdf()).geometry, [792, 612, 72, 72, 72, 72]);
  assert.equal(decode(old).sources[0][1], "# One", "old owned output stays unchanged");
  assert.equal(state.created, 1);
  book.dispose();
  assert.equal(state.freed, 1);
});

test("a no-op batch leaves revision and reparse counts unchanged", async () => {
  const { api } = fixture();
  const book = await api.createBook(chapters());
  assert.deepEqual(book.updateSources([file("one.md", "# One")]), {
    revision: 0, sourceLength: 10, chapterCount: 2, changedSources: 0, reparsedChapters: [],
  });
  book.updateSources([file("one.md", "Different")]);
  assert.equal(book.updateSources([file("one.md", "Different")]).revision, 1);
  book.dispose();
});

test("include-only source changes can commit without claiming chapter parser invocations", async () => {
  const { api } = fixture();
  const book = await api.createBook(chapters(), { includeSources: [file("unused.md", "Old")] });
  const report = book.updateSources([file("unused.md", "Newer")]);
  assert.deepEqual(report, { revision: 1, sourceLength: 15, chapterCount: 2, changedSources: 1, reparsedChapters: [] });
  assert.equal(book.chapterCount, 2);
  book.dispose();
});

test("stale and future revisions are rejected before inspecting source getters", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  book.updateSources([file("one.md", "Current")]);
  const poisoned = [{ get path() { throw new Error("read stale source"); } }];
  for (const expectedRevision of [0, 2, 0xffffffff]) {
    assert.throws(() => book.updateSources(poisoned, { expectedRevision }), { code: "STALE_BOOK_REVISION" });
  }
  assert.equal(state.calls.length, 1);
  assert.equal(book.sourceRevision, 1);
  assert.equal(book.updateSources([file("one.md", "Latest")], { expectedRevision: 1 }).revision, 2);
  book.dispose();
});

test("source-update options reject lossy revisions, unknown fields and accessors", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  let accessed = false;
  for (const options of [null, [], 0, { rev: 0 }, { get expectedRevision() { accessed = true; return 0; } },
    { [Symbol("revision")]: 0 }, { expectedRevision: -1 }, { expectedRevision: 0.1 },
    { expectedRevision: Infinity }, { expectedRevision: NaN }, { expectedRevision: 2 ** 32 },
    { expectedRevision: "0" }, { expectedRevision: 0n }, { expectedRevision: null },
    Object.create({ expectedRevision: 0 })]) {
    assert.throws(() => book.updateSources([file("one.md", "Changed")], options), { code: "INVALID_OPTIONS" });
  }
  assert.equal(accessed, false);
  assert.equal(state.calls.length, 0);
  const options = Object.create(null); options.expectedRevision = 0;
  assert.equal(book.updateSources([file("one.md", "Changed")], options).revision, 1);
  book.dispose();
});

test("raw update failures retain the last successful source and release the admission slot", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  const original = book.renderPdf().bytes;
  state.fail = "book_sources: missing include " + "x".repeat(5000);
  assert.throws(() => book.updateSources([file("one.md", "Changed")]), error => {
    assert.ok(error instanceof Error); assert.equal(error.message.length, 2048); return true;
  });
  assert.deepEqual(book.renderPdf().bytes, original);
  assert.equal(book.sourceRevision, 0);
  assert.equal(state.freed, 0);
  state.fail = null;
  assert.equal(book.updateSources([file("one.md", "Recovered")]).revision, 1);
  book.dispose();
});

test("membership and duplicate failures reach native admission as one batch, never partial calls", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  for (const changes of [[file("one.md", "New"), file("missing.md", "X")],
    [file("one.md", "New"), file("one.md", "Other")]]) {
    assert.throws(() => book.updateSources(changes), /unknown or duplicate/);
    assert.equal(book.sourceRevision, 0);
    assert.equal(decode(book.renderSite()).sources[0][1], "# One");
  }
  assert.equal(state.calls.length, 2);
  book.dispose();
});

test("host source getters are captured once and cannot retarget admitted strings", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  let pathReads = 0, sourceReads = 0, path = "one.md", source = "Changed";
  const changes = [{ get path() { pathReads++; return path; }, get source() { sourceReads++; path = "two.md"; return source; } }];
  book.updateSources(changes);
  source = "After";
  assert.equal(pathReads, 1); assert.equal(sourceReads, 1);
  assert.deepEqual(state.calls[0].paths, ["one.md"]);
  assert.deepEqual(state.calls[0].sources, ["Changed"]);
  book.dispose();
});

test("invalid and oversized update batches fail before invoking raw mutation", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  for (const changes of [[], null, {}, new Array(4097), [null], [file("one.md", 12)],
    [file("one.md", "\ud800")], [file("\udfff", "x")],
    [file("one.md", "x".repeat(64 * 1024 * 1024))]]) {
    assert.throws(() => book.updateSources(changes));
  }
  assert.equal(state.calls.length, 0);
  assert.equal(book.sourceRevision, 0);
  book.dispose();
});

test("Unicode and original newline bytes reach the engine without normalization", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  const source = "\ufeff# α\r\n\r\n😀\r\n";
  const report = book.updateSources([file("one.md", source)]);
  assert.equal(report.sourceLength, textBytes(source) + 5);
  assert.equal(state.calls[0].sources[0], source);
  book.dispose();
});

test("nested updates from getters are rejected before entering the renderer twice", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  const nested = [{ get path() {
    assert.throws(() => book.updateSources([file("two.md", "Nested")]), { code: "BOOK_BUSY" });
    return "one.md";
  }, source: "Outer" }];
  assert.equal(book.updateSources(nested).revision, 1);
  assert.equal(state.calls.length, 1);
  assert.equal(decode(book.renderSite()).sources[1][1], "# Two");
  book.dispose();
});

test("reentrant disposal during source or option admission never uses a freed handle", async () => {
  for (const duringOptions of [false, true]) {
    const { api, state } = fixture();
    const book = await api.createBook(chapters());
    const changes = duringOptions ? [file("one.md", "New")] : [{
      get path() { book.dispose(); return "one.md"; }, source: "New",
    }];
    const options = duringOptions ? new Proxy({}, { ownKeys() { book.dispose(); return []; } }) : {};
    assert.throws(() => book.updateSources(changes, options), /disposed/);
    assert.equal(state.calls.length, 0);
    assert.equal(state.freed, 1);
    assert.throws(() => book.updateSources([file("one.md", "Again")]), /disposed/);
    assert.throws(() => book.sourceRevision, /disposed/);
    book.dispose();
    assert.equal(state.freed, 1);
  }
});

test("a revision changed during admission is caught before raw mutation", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  const changes = [{ get path() { state.raws[0].sourceRevision++; return "one.md"; }, source: "New" }];
  assert.throws(() => book.updateSources(changes), { code: "STALE_BOOK_REVISION" });
  assert.equal(state.calls.length, 0);
  book.dispose();
});

test("render-only old packages remain usable but cannot pretend to support source editing", async () => {
  const { api, state } = fixture({ editable: false, fromSources: false });
  const book = await api.createBook(chapters());
  assert.deepEqual(state.modes, ["literal"]);
  assert.throws(() => book.sourceRevision, { code: "UNSUPPORTED_BOOK_UPDATE" });
  assert.throws(() => book.updateSources([file("one.md", "New")]), { code: "UNSUPPORTED_BOOK_UPDATE" });
  assert.equal(decode(book.renderPdf()).sources[0][1], "# One");
  book.dispose();
});

test("editable include-free books retain expansion mode before their first future directive", async () => {
  const { api, state } = fixture();
  const defaultBook = await api.createBook(chapters());
  const literalBook = await api.createBook(chapters(), { expandIncludes: false });
  assert.deepEqual(state.modes, ["expanded", "literal"]);
  defaultBook.updateSources([file("one.md", "{{#include two.md}}")]);
  literalBook.updateSources([file("one.md", "{{#include two.md}}")]);
  assert.equal(state.raws[0].mode, "expanded");
  assert.equal(state.raws[1].mode, "literal");
  defaultBook.dispose(); literalBook.dispose();
});

test("inconsistent editable binaries without fromSources fail rather than lose future expansion", async () => {
  const { api, state } = fixture({ fromSources: false });
  await assert.rejects(api.createBook(chapters()), /fromSources/);
  assert.equal(state.created, 0);
  const literal = await api.createBook(chapters(), { expandIncludes: false });
  literal.dispose();
});

test("malformed successful reports quarantine the session instead of claiming rollback", async () => {
  const corruptions = [
    () => "", () => "not JSON", () => "x".repeat(65537), () => "null",
    r => JSON.stringify({ ...r, schema: "other" }),
    r => JSON.stringify({ ...r, revision: r.revision + 1 }),
    r => JSON.stringify({ ...r, source_length: r.source_length + 1 }),
    r => JSON.stringify({ ...r, chapter_count: 3 }),
    r => JSON.stringify({ ...r, changed_sources: 2 }),
    r => JSON.stringify({ ...r, changed_sources: 0 }),
    r => JSON.stringify({ ...r, reparsed_chapters: [-1] }),
    r => JSON.stringify({ ...r, reparsed_chapters: [2] }),
    r => JSON.stringify({ ...r, reparsed_chapters: [0, 0] }),
    r => JSON.stringify({ ...r, reparsed_chapters: [1, 0] }),
    r => JSON.stringify({ ...r, reparsed_chapters: [0.5] }),
    r => JSON.stringify({ ...r, reparsed_chapters: null }),
    (r, raw) => { raw.sourceRevision++; return JSON.stringify(r); },
    (r, raw) => { raw.chapterCount++; return JSON.stringify(r); },
  ];
  for (const corrupt of corruptions) {
    const { api, state } = fixture();
    const book = await api.createBook(chapters());
    state.corrupt = corrupt;
    assert.throws(() => book.updateSources([file("one.md", "New")]), { code: "INVALID_BOOK_UPDATE_REPORT" });
    assert.equal(state.freed, 1);
    assert.throws(() => book.renderPdf(), /disposed/);
    assert.equal(state.raws[0].files.get("one.md"), "New", "failure must not claim native rollback");
    book.dispose(); assert.equal(state.freed, 1);
  }
});

test("malformed no-op report cannot sneak a different size or reparse list through", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  state.corrupt = r => JSON.stringify({ ...r, reparsed_chapters: [0] });
  assert.throws(() => book.updateSources([file("one.md", "# One")]), { code: "INVALID_BOOK_UPDATE_REPORT" });
  assert.equal(state.freed, 1);
});

test("maximum revision is exact and exhaustion cannot be confused with zero", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  state.raws[0].sourceRevision = 0xfffffffe;
  assert.equal(book.updateSources([file("one.md", "Changed")], { expectedRevision: 0xfffffffe }).revision, 0xffffffff);
  assert.equal(book.updateSources([file("one.md", "Changed")]).revision, 0xffffffff);
  assert.throws(() => book.updateSources([file("one.md", "Overflow")]), /exhausted/);
  assert.equal(book.sourceRevision, 0xffffffff);
  assert.equal(state.freed, 0);
  book.dispose();
});

test("many seeded edits preserve revision/order/byte contracts without recreating raw state", async () => {
  const { api, state } = fixture();
  const sources = chapters();
  const book = await api.createBook(sources);
  let seed = 1729;
  for (let round = 0; round < 1000; round++) {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    const index = seed % sources.length;
    sources[index].source = `# Round ${round}\n\nα😀 ${seed}\r\n`;
    const report = book.updateSources([sources[index]], { expectedRevision: round });
    assert.equal(report.revision, round + 1);
    assert.equal(report.sourceLength, sources.reduce((sum, f) => sum + textBytes(f.source), 0));
    assert.deepEqual(report.reparsedChapters, [index]);
  }
  assert.equal(state.created, 1);
  assert.equal(state.calls.length, 1000);
  assert.equal(book.sourceRevision, 1000);
  book.dispose();
});


test("source arrays cannot replace bounded index traversal with custom iterators", async () => {
  const { api, state } = fixture();
  const input = chapters();
  input[Symbol.iterator] = function* () { throw new Error("custom iterator must not run"); };
  const book = await api.createBook(input);
  const updates = [file("one.md", "New")];
  updates[Symbol.iterator] = function* () { throw new Error("custom iterator must not run"); };
  assert.equal(book.updateSources(updates).revision, 1);
  assert.deepEqual(state.calls[0].paths, ["one.md"]);
  book.dispose();
});

test("source-array length is captured once before indexing", async () => {
  const { api, state } = fixture();
  const book = await api.createBook(chapters());
  let reads = 0;
  const updates = new Proxy([file("one.md", "Changed")], {
    get(target, key, receiver) {
      if (key === "length") { reads++; assert.equal(reads, 1); }
      return Reflect.get(target, key, receiver);
    },
  });
  book.updateSources(updates);
  assert.equal(reads, 1);
  assert.deepEqual(state.calls[0].sources, ["Changed"]);
  book.dispose();
});
