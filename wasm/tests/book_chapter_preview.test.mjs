// Production JS facade/codec tests with an explicit native ABI double.
// These fixtures do not claim Rust rendering, ZIP parity or WASM execution.
import test from "node:test";
import assert from "node:assert/strict";
import { createBookBindings, parseBookChapterPreview } from "../book_session.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const files = [{ path: "intro.md", source: "# Intro" }, { path: "next.md", source: "# Next" }];
function payload(selected = 0, count = 2) {
  return { schema: "fmd-book-chapter-preview-v1", selected,
    pages: Array.from({ length: count }, (_, i) => ({ path: `ch${i}.html`, source: `ch${i}.md`, title: `Chapter ${i}` })),
    html: `<!doctype html><p>Selected ${selected}: 日本語 😀 &amp; <script>untrusted()</script></p>` };
}
function fixture({ legacy = false } = {}) {
  const calls = { load: 0, created: 0, freed: 0, selected: [], site: 0, pdf: 0, epub: 0 };
  let reply, failure;
  class Raw {
    constructor(paths, sources) {
      calls.created++;
      this.chapterCount = paths.length;
      this.sourceLength = sources.reduce((sum, text) => sum + new TextEncoder().encode(text).length, 0);
    }
    setMetadata() {}
    setCustomCss() {}
    setTheme() {}
    setNavigation() {}
    renderChapterPreview(selected) {
      calls.selected.push(selected);
      if (failure !== undefined) throw failure;
      return reply === undefined ? encode(payload(selected, this.chapterCount)) : reply;
    }
    renderSite() { calls.site++; throw new Error("Must not render the site for a native chapter preview"); }
    renderPdf() { calls.pdf++; return new Uint8Array([1, 2]); }
    renderEpub() { calls.epub++; return new Uint8Array([3, 4]); }
    free() { calls.freed++; }
  }
  if (legacy) delete Raw.prototype.renderChapterPreview;
  return { calls, bindings: createBookBindings(async () => { calls.load++; return Raw; }),
    setReply: value => { reply = value; }, setFailure: value => { failure = value; } };
}
const bad = action => assert.throws(action, { code: "INVALID_BOOK_PREVIEW" });
const over = action => assert.throws(action, { code: "PREVIEW_LIMIT" });

test("session renders exactly the requested native chapter without site work", async () => {
  const f = fixture(), session = await f.bindings.createBook(files);
  assert.equal(session.supportsChapterPreview, true);
  const result = session.renderChapterPreview(1);
  assert.equal(result.format, "book-chapter-preview");
  assert.equal(result.mimeType, "application/json");
  assert.equal(result.filename("selected"), "selected.json");
  assert.equal(result.sourceLength, 13);
  assert.deepEqual(parseBookChapterPreview(result.bytes, 1, 2), payload(1));
  assert.deepEqual(f.calls.selected, [1]);
  assert.equal(f.calls.site, 0);
  session.dispose();
  assert.equal(f.calls.freed, 1);
  assert.deepEqual(new Uint8Array(await result.blob().arrayBuffer()), result.bytes);
});

test("one-shot API disposes after success and native failure", async () => {
  const f = fixture();
  const result = await f.bindings.renderBookChapterPreview(files, 0);
  assert.equal(parseBookChapterPreview(result.bytes).selected, 0);
  assert.equal(f.calls.freed, 1);
  f.setFailure("native failure " + "x".repeat(4096));
  await assert.rejects(f.bindings.renderBookChapterPreview(files, 1), error => {
    assert.equal(error.message.length, 2048);
    return error.message.startsWith("native failure");
  });
  assert.equal(f.calls.freed, 2);
  assert.equal(f.calls.site, 0);
});

test("invalid indexes never reach native rendering or trigger numeric coercion", async () => {
  const f = fixture(), session = await f.bindings.createBook(files);
  const dangerous = { valueOf() { throw new Error("coercion"); } };
  for (const selected of [-1, 2, 0.5, NaN, Infinity, 4294967296, "1", null, true, 1n, dangerous]) {
    assert.throws(() => session.renderChapterPreview(selected), { code: "INVALID_CHAPTER_INDEX" });
  }
  session.renderChapterPreview(-0);
  assert.deepEqual(f.calls.selected, [0]);
  session.dispose();
});

test("global index errors are rejected before source admission or initialization", async () => {
  const f = fixture();
  const source = [{ get path() { throw new Error("must not read input"); } }];
  for (const index of [-1, 4096, Infinity, "0"]) {
    await assert.rejects(f.bindings.renderBookChapterPreview(source, index), { code: "INVALID_CHAPTER_INDEX" });
  }
  assert.equal(f.calls.load, 0);
  await assert.rejects(f.bindings.renderBookChapterPreview(files, 2), { code: "INVALID_CHAPTER_INDEX" });
  assert.equal(f.calls.freed, 1);
});

test("old packages are explicit and existing exports keep working", async () => {
  const f = fixture({ legacy: true }), session = await f.bindings.createBook(files);
  assert.equal(session.supportsChapterPreview, false);
  assert.throws(() => session.renderChapterPreview(0), { code: "UNSUPPORTED_BOOK_CHAPTER_PREVIEW" });
  assert.deepEqual([...session.renderPdf().bytes], [1, 2]);
  assert.deepEqual([...session.renderEpub().bytes], [3, 4]);
  assert.equal(f.calls.site, 0);
  session.dispose();
});

test("disposed sessions cannot render or report capabilities", async () => {
  const f = fixture(), session = await f.bindings.createBook(files);
  session.dispose(); session.dispose();
  assert.throws(() => session.renderChapterPreview(), /disposed/);
  assert.throws(() => session.supportsChapterPreview, /disposed/);
  assert.equal(f.calls.freed, 1);
  assert.deepEqual(f.calls.selected, []);
});

test("native success must match requested index and current chapter count", async () => {
  const f = fixture(), session = await f.bindings.createBook(files);
  for (const reply of [payload(0), payload(1, 3), { ...payload(1), schema: "wrong" }]) {
    f.setReply(encode(reply));
    bad(() => session.renderChapterPreview(1));
  }
  f.setReply(undefined);
  assert.equal(parseBookChapterPreview(session.renderChapterPreview(1).bytes).selected, 1);
  session.dispose();
  assert.equal(f.calls.site, 0);
});

test("one-shot invalid native output still releases its handle", async () => {
  const f = fixture(); f.setReply(encode(payload(1)));
  await assert.rejects(f.bindings.renderBookChapterPreview(files, 0), { code: "INVALID_BOOK_PREVIEW" });
  assert.equal(f.calls.freed, 1);
});

test("chapter maps admit 129 and 4096 entries without unselected HTML", () => {
  for (const count of [129, 4096]) {
    const value = parseBookChapterPreview(encode(payload(count - 1, count)), count - 1, count);
    assert.equal(value.pages.length, count);
    assert.equal(value.pages[count - 1].path, `ch${count - 1}.html`);
    assert.equal(Object.hasOwn(value.pages[0], "html"), false);
  }
  over(() => parseBookChapterPreview(encode(payload(0, 4097))));
});

test("decoded content is immutable and HTML remains verbatim, not sanitized", () => {
  const input = payload(1), bytes = encode(input), value = parseBookChapterPreview(bytes);
  bytes.fill(0);
  assert.equal(value.html, input.html);
  assert.equal(Object.isFrozen(value), true);
  assert.equal(Object.isFrozen(value.pages), true);
  assert.equal(Object.isFrozen(value.pages[0]), true);
  assert.throws(() => { value.pages[0].path = "other.html"; }, TypeError);
});

test("strict byte admission rejects malformed UTF-8, JSON, shared and oversized data", () => {
  for (const bytes of [null, "{}", [], new Uint8Array(), new Uint8Array([0xc0, 0xaf]),
    new Uint8Array([123]), new Uint8Array(new SharedArrayBuffer(8))]) {
    bad(() => parseBookChapterPreview(bytes));
  }
  over(() => parseBookChapterPreview(new Uint8Array(64 * 1024 * 1024 + 1)));
  const encoded = encode(payload()), padded = new Uint8Array(encoded.length + 8);
  padded.set(encoded, 4);
  assert.deepEqual(parseBookChapterPreview(padded.subarray(4, -4)), payload());
});

test("wrong schemas, empty maps and nonintegral selected indexes fail shut", () => {
  for (const change of [{ schema: "fmd-book-preview-v1" }, { pages: [] }, { pages: null },
    { selected: -1 }, { selected: 2 }, { selected: 0.25 }, { selected: "0" }, { html: 123 }]) {
    bad(() => parseBookChapterPreview(encode({ ...payload(), ...change })));
  }
  bad(() => parseBookChapterPreview(encode(payload()), 1));
  bad(() => parseBookChapterPreview(encode(payload()), 0, 3));
});

test("reader targets reject unsafe paths and ambiguous navigation identities", () => {
  for (const path of ["index.html", "~fmd-search.html", "../x.html", ".x.html", "a/x.html", "a\\x.html",
    "https:x.html", "x.html#anchor", "x%20.html", "x?.html", "x .html", "x\u0000.html", "x.pdf"]) {
    const p = payload(); p.pages[0].path = path;
    bad(() => parseBookChapterPreview(encode(p)));
  }
  for (const field of ["path", "source"]) {
    const p = payload(); p.pages[1][field] = p.pages[0][field];
    bad(() => parseBookChapterPreview(encode(p)));
  }
  const folded = payload(); folded.pages[1].path = "CH0.html";
  bad(() => parseBookChapterPreview(encode(folded)));
});

test("UTF-8 metadata ceilings are exact and malformed Unicode is refused", () => {
  const p = payload(); p.pages[0].title = "😀".repeat(1024);
  assert.equal(parseBookChapterPreview(encode(p)).pages[0].title, p.pages[0].title);
  p.pages[0].title += "a";
  over(() => parseBookChapterPreview(encode(p)));
  for (const field of ["path", "source", "title"]) {
    const p = payload(); p.pages[0][field] = "\ud800";
    bad(() => parseBookChapterPreview(encode(p)));
  }
  const name = payload(); name.pages[0].path = "a".repeat(250) + ".html";
  assert.equal(parseBookChapterPreview(encode(name)).pages[0].path.length, 255);
  name.pages[0].path = "a" + name.pages[0].path;
  over(() => parseBookChapterPreview(encode(name)));
});

test("HTML budget counts UTF-8 bytes rather than JS code units", () => {
  const p = payload(); p.html = "😀".repeat(2 * 1024 * 1024);
  assert.equal(parseBookChapterPreview(encode(p)).html.length, 4 * 1024 * 1024);
  p.html += "a";
  over(() => parseBookChapterPreview(encode(p)));
  bad(() => parseBookChapterPreview(encode({ ...payload(), html: "\udc00" })));
});
