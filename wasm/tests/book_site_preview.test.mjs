// Independent Node/zlib classic-ZIP fixtures exercise the production adapter.
// These tests do not claim that a generated Rust/WASM artifact was executed.
import test from "node:test";
import assert from "node:assert/strict";
import { deflateRawSync } from "node:zlib";
import { decodeBookSiteArchive, parseBookPreview, renderBookPreview, resolveBookPreviewLink } from "../book_site_preview.mjs";
const encode = value => new TextEncoder().encode(value);
function checksum(bytes) { let c = 0xffffffff; for (const b of bytes) { c ^= b; for (let i = 0; i < 8; i++) c = (c >>> 1) ^ ((c & 1) ? 0xedb88320 : 0); } return (c ^ 0xffffffff) >>> 0; }
function archive(entries, method = 8) {
  const local = [], central = []; let offset = 0;
  for (const [name, source] of entries) {
    const nameBytes = Buffer.from(name), body = typeof source === "string" ? Buffer.from(source) : Buffer.from(source);
    const payload = method === 8 ? deflateRawSync(body) : body, crc = checksum(body);
    const a = Buffer.alloc(30), b = Buffer.alloc(46);
    a.writeUInt32LE(0x04034b50); a.writeUInt16LE(20, 4); a.writeUInt16LE(0x800, 6); a.writeUInt16LE(method, 8);
    a.writeUInt32LE(crc, 14); a.writeUInt32LE(payload.length, 18); a.writeUInt32LE(body.length, 22); a.writeUInt16LE(nameBytes.length, 26);
    b.writeUInt32LE(0x02014b50); b.writeUInt16LE(20, 4); b.writeUInt16LE(20, 6); b.writeUInt16LE(0x800, 8); b.writeUInt16LE(method, 10);
    b.writeUInt32LE(crc, 16); b.writeUInt32LE(payload.length, 20); b.writeUInt32LE(body.length, 24); b.writeUInt16LE(nameBytes.length, 28); b.writeUInt32LE(offset, 42);
    local.push(a, nameBytes, payload); central.push(b, nameBytes); offset += a.length + nameBytes.length + payload.length;
  }
  const c = Buffer.concat(central), end = Buffer.alloc(22); end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10); end.writeUInt32LE(c.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, c, end]);
}
const chapter = { source: "guide/start.md", page: "guide-start.html", title: "Start 🚀", index: {} };
const other = { source: "end.md", page: "end.html", title: "End", index: {} };
function entries(chapters = [chapter, other]) {
  return [[chapter.page, '<!doctype html><html lang="en"><head><style>p{color:red}</style></head><body><h1 id="start">Start 🚀</h1><a href="end.html#end">End</a></body></html>'],
    [other.page, '<html><body><h1 id="end">End</h1></body></html>'],
    ["search-index.json", JSON.stringify({ schema: "fmd-book-search-index-v1", chapters })],
    ["index.html", '<meta http-equiv="refresh" content="0;url=guide-start.html">']];
}
const rejects = (bytes, code = "INVALID_BOOK_PREVIEW") => assert.rejects(decodeBookSiteArchive(bytes), { code });
const central = bytes => bytes.readUInt32LE(bytes.length - 6);
for (const method of [0, 8]) test(`method ${method}: exact engine HTML and generated chapter order survive decoding`, async () => {
  const source = entries([other, chapter]), parsed = await decodeBookSiteArchive(archive(source, method));
  assert.deepEqual(parsed.pages.map(page => [page.path, page.source, page.title]), [[other.page, other.source, other.title], [chapter.page, chapter.source, chapter.title]]);
  assert.equal(parsed.pages[1].html, source[0][1]); assert.equal(parsed.pages.length, 2); assert(Object.isFrozen(parsed.pages[0]));
  assert.equal(parsed.pages.some(page => page.path === "index.html"), false);
});
test("archive input is captured before asynchronous decompression", async () => {
  const bytes = archive(entries()), pending = decodeBookSiteArchive(bytes); bytes.fill(0);
  assert.equal((await pending).pages[0].source, chapter.source);
});
test("renderer adapter delegates the complete ordered collection and asset options", async () => {
  const files = [{ path: "source.md", source: "untouched" }], options = { images: [{ destination: "x.png", bytes: new Uint8Array([1]) }] };
  let calls = 0;
  const result = await renderBookPreview({ async renderBookSite(f, o) { calls++; assert.equal(f, files); assert.equal(o, options); return { bytes: archive(entries()), sourceLength: 77 }; } }, files, options);
  assert.equal(calls, 1); assert.equal(result.sourceLength, 77); assert.equal(parseBookPreview(result.bytes).pages.length, 2);
});
for (const [name, mutate] of [
  ["missing end record", b => b.subarray(0, b.length - 1)],
  ["split archive", b => { b.writeUInt16LE(1, b.length - 18); return b; }],
  ["encrypted flags", b => { b.writeUInt16LE(0x801, central(b) + 8); return b; }],
  ["data descriptor", b => { b.writeUInt16LE(0x808, central(b) + 8); return b; }],
  ["ZIP64 version", b => { b.writeUInt16LE(45, central(b) + 6); return b; }],
  ["extra fields", b => { b.writeUInt16LE(1, central(b) + 30); return b; }],
  ["different local name", b => { b[30] ^= 1; return b; }],
  ["different local length", b => { b.writeUInt32LE(3, 22); return b; }],
  ["overlapping local offset", b => { b.writeUInt32LE(1, central(b) + 42); return b; }],
  ["CRC mismatch", b => { b.writeUInt32LE(0, 14); b.writeUInt32LE(0, central(b) + 16); return b; }],
  ["central directory gap", b => { b.writeUInt32LE(central(b) + 1, b.length - 6); return b; }],
  ["trailing bytes", b => Buffer.concat([b, Buffer.from([0])])]
]) test(`rejects ${name}`, async () => rejects(mutate(archive(entries()))));
test("all declared sizes are admitted before any decompression starts", async () => {
  const bytes = archive(entries()); bytes.writeUInt32LE(9 * 1024 * 1024, 22); bytes.writeUInt32LE(9 * 1024 * 1024, central(bytes) + 24);
  await rejects(bytes, "PREVIEW_LIMIT");
});
test("expansion beyond admitted size and malformed deflate streams fail closed", async () => {
  const bytes = archive(entries()); bytes.writeUInt32LE(1, 22); bytes.writeUInt32LE(1, central(bytes) + 24); await rejects(bytes);
  const broken = archive(entries()); broken.fill(0xff, 30 + chapter.page.length, 35 + chapter.page.length); await rejects(broken);
});
for (const name of ["../escape.html", "/absolute.html", "a\\b.html", "javascript:x.html", "a%2fb.html", "a#b.html", ".hidden.html"]) test(`rejects output path ${name}`, async () => {
  const e = entries(); e[0][0] = name; await rejects(archive(e));
});
test("duplicate names and source paths cannot create ambiguous chapter navigation", async () => {
  const e = entries(); e[1][0] = chapter.page; await rejects(archive(e));
  await rejects(archive(entries([chapter, { ...other, source: chapter.source }])));
  await rejects(archive(entries([chapter, chapter])));
});
test("missing chapter bytes, index, or chapter mapping fail closed", async () => {
  await rejects(archive(entries([chapter, { ...other, page: "missing.html" }])));
  await rejects(archive(entries().filter(([name]) => name !== "search-index.json")));
  await rejects(archive(entries([chapter])));
});
test("malformed UTF-8 is rejected even with valid sizes and CRC", async () => {
  const e = entries(); e[0][1] = new Uint8Array([0xc0, 0x80]); await rejects(archive(e));
});
test("malformed JSON, schema drift and malformed Unicode metadata are rejected", async () => {
  const e = entries(); e[2][1] = "not json"; await rejects(archive(e));
  e[2][1] = JSON.stringify({ schema: "future", chapters: [chapter, other] }); await rejects(archive(e));
  await rejects(archive(entries([{ ...chapter, title: "bad\ud800" }, other])));
});
test("preview wire messages are bounded, validated, and immutable", async () => {
  const value = await decodeBookSiteArchive(archive(entries())), bytes = encode(JSON.stringify(value));
  assert.deepEqual(parseBookPreview(bytes), value);
  for (const bad of [encode("[]"), encode('{"schema":"fmd-book-preview-v1","pages":[null]}'), encode(JSON.stringify({ ...value, pages: [value.pages[0], value.pages[0]] }))]) {
    assert.throws(() => parseBookPreview(bad), { code: "INVALID_BOOK_PREVIEW" });
  }
  assert.throws(() => parseBookPreview(new Uint8Array(new SharedArrayBuffer(22))), { code: "INVALID_BOOK_PREVIEW" });
});
test("navigation uses only engine-attested flat pages and decoded fragments", async () => {
  const preview = await decodeBookSiteArchive(archive(entries()));
  for (const href of ["end.html#end", "./end.html#end"]) assert.deepEqual(resolveBookPreviewLink(preview, chapter.page, href), { path: "end.html", fragment: "end" });
  assert.deepEqual(resolveBookPreviewLink(preview, chapter.page, "#%E4%B8%AD"), { path: chapter.page, fragment: "中" });
  assert.equal(resolveBookPreviewLink(preview, other.page, "index.html").path, chapter.page);
  for (const href of ["https://example.org", "//example.org", "javascript:alert(1)", "data:text/html,x", "../end.html", "././end.html", "%65nd.html", "end.html?x", "end.html#%00", "end.html#%zz", "missing.html", "end.html\n"]) {
    assert.equal(resolveBookPreviewLink(preview, chapter.page, href), null, href);
  }
});

for (const method of [0, 8]) test(`method ${method}: current search-enabled sites retain only chapter pages`, async () => {
  const source = entries([other, chapter]);
  source.splice(3, 0, ["~fmd-search.html", '<!doctype html><script>throw new Error("never execute search here")</script>']);
  const bytes = archive(source, method);
  const preview = await decodeBookSiteArchive(bytes);
  assert.deepEqual(preview.pages.map(page => page.path), [other.page, chapter.page]);
  assert.equal(preview.pages[1].html, source[0][1]);
  assert.equal(resolveBookPreviewLink(preview, chapter.page, "./~fmd-search.html"), null);
  const output = await renderBookPreview({ async renderBookSite() { return { bytes, sourceLength: 19 }; } }, [], {});
  assert.deepEqual(parseBookPreview(output.bytes), preview);
  assert.equal(output.sourceLength, 19);
});
test("all 128 chapters plus the three generated auxiliary files are admitted", async () => {
  const chapters = Array.from({ length: 128 }, (_, i) => ({ source: `${i}.md`, page: `${i}.html`, title: `Chapter ${i}` }));
  const source = chapters.map(chapter => [chapter.page, `<h1>${chapter.title}</h1>`]);
  source.push(["index.html", "Landing"], ["search-index.json", JSON.stringify({ schema: "fmd-book-search-index-v1", chapters })], ["~fmd-search.html", "Search"]);
  assert.equal((await decodeBookSiteArchive(archive(source, 0))).pages.length, 128);
});
test("auxiliary pages cannot masquerade as indexed chapters or bypass integrity checks", async () => {
  const source = entries(); source.push(["~fmd-search.html", "Search"]);
  source[2][1] = JSON.stringify({ schema: "fmd-book-search-index-v1", chapters: [chapter, { ...other, page: "~fmd-search.html" }] });
  await rejects(archive(source));
  const extra = entries(); extra.push(["unindexed.html", "Not attested"]); await rejects(archive(extra));
  const duplicate = entries(); duplicate.push(["~fmd-search.html", "One"], ["~fmd-search.html", "Two"]); await rejects(archive(duplicate));
  const malformed = entries(); malformed.push(["~fmd-search.html", new Uint8Array([0xc0, 0x80])]); await rejects(archive(malformed));
  const preview = await decodeBookSiteArchive(archive(entries()));
  assert.throws(() => parseBookPreview(encode(JSON.stringify({ ...preview, pages: [{ ...preview.pages[0], path: "~fmd-search.html" }] }))), { code: "INVALID_BOOK_PREVIEW" });
});
