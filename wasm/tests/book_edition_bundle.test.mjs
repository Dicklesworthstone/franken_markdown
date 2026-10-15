import assert from "node:assert/strict";
import { createHash, webcrypto } from "node:crypto";
import test from "node:test";
import { createBookEditionBundle, BOOK_EDITION_BUNDLE_LIMIT } from "../demo/book_edition.mjs";

// No native renderer is substituted by this test: inputs are opaque publication
// bytes. Verify the production packager using an independent central-directory
// reader, bitwise CRC, and Node's SHA-256 rather than Web Crypto's receipt.
const types = { pdf: "application/pdf", epub: "application/epub+zip", site: "application/zip" };
const payloads = { pdf: new Uint8Array([37, 80, 68, 70, 45, 0, 255, 128]),
  epub: new Uint8Array([80, 75, 3, 4, 254, 0, 128]), site: new Uint8Array([80, 75, 3, 4, 42, 255]) };
const output = format => ({ format, blob: new Blob([payloads[format]], { type: types[format] }) });
const inputs = () => ["pdf", "epub", "site"].map(output);
const code = value => cause => cause?.code === value;
const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
const tick = () => new Promise(resolve => setImmediate(resolve));
function crc32(bytes) {
  let crc = -1;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
  }
  return (crc ^ -1) >>> 0;
}
export async function readBundle(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const view = new DataView(bytes.buffer), end = bytes.length - 22;
  const u16 = offset => view.getUint16(offset, true), u32 = offset => view.getUint32(offset, true);
  assert.equal(u32(end), 0x06054b50); assert.equal(u16(end + 4), 0); assert.equal(u16(end + 6), 0);
  assert.equal(u16(end + 8), u16(end + 10)); assert.equal(u16(end + 20), 0);
  const directoryStart = u32(end + 16), entries = new Map();
  assert.equal(directoryStart + u32(end + 12), end);
  let at = directoryStart, expectedLocal = 0;
  for (let i = 0; i < u16(end + 10); i++) {
    assert.equal(u32(at), 0x02014b50); assert.equal(u16(at + 4), 20); assert.equal(u16(at + 6), 20);
    assert.equal(u16(at + 8), 0x800); assert.equal(u16(at + 10), 0);
    assert.equal(u16(at + 12), 0); assert.equal(u16(at + 14), 33);
    const size = u32(at + 20), length = u16(at + 28), local = u32(at + 42);
    assert.equal(size, u32(at + 24)); assert.equal(u16(at + 30), 0); assert.equal(u16(at + 32), 0);
    assert.equal(u16(at + 34), 0); assert.equal(u32(at + 38), 0); assert.equal(local, expectedLocal);
    const name = new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + length));
    assert(!entries.has(name)); assert.equal(u32(local), 0x04034b50);
    assert.equal(u16(local + 4), 20); assert.equal(u16(local + 6), 0x800); assert.equal(u16(local + 8), 0);
    assert.equal(u16(local + 10), 0); assert.equal(u16(local + 12), 33);
    assert.equal(u32(local + 14), u32(at + 16)); assert.equal(u32(local + 18), size); assert.equal(u32(local + 22), size);
    assert.equal(u16(local + 26), length); assert.equal(u16(local + 28), 0);
    assert.equal(new TextDecoder().decode(bytes.subarray(local + 30, local + 30 + length)), name);
    const start = local + 30 + length;
    assert(start + size <= directoryStart);
    const body = bytes.slice(start, start + size);
    assert.equal(crc32(body), u32(at + 16)); entries.set(name, body);
    expectedLocal = start + size; at += 46 + length;
  }
  assert.equal(expectedLocal, directoryStart); assert.equal(at, end);
  return entries;
}

test("packages exact native bytes and a independently verifiable SHA-256 manifest", async () => {
  const result = await createBookEditionBundle(inputs(), { sourceLength: 27 });
  assert.equal(result.format, "book-edition-bundle"); assert.equal(result.filename, "book-edition.zip");
  assert.equal(result.mimeType, "application/zip"); assert.equal(result.blob.size, result.byteLength);
  assert(result.byteLength < BOOK_EDITION_BUNDLE_LIMIT); assert(Object.isFrozen(result));
  const entries = await readBundle(result.blob);
  assert.deepEqual([...entries.keys()], ["book.pdf", "book.epub", "book-site.zip", "manifest.json"]);
  const manifest = JSON.parse(new TextDecoder().decode(entries.get("manifest.json")));
  assert.deepEqual(manifest, result.manifest); assert.equal(manifest.sourceLength, 27);
  assert.equal(manifest.schema, "fmd-book-edition-v1");
  assert(Object.isFrozen(result.manifest.files)); assert(Object.isFrozen(result.manifest.files[0]));
  for (const file of manifest.files) {
    const bytes = entries.get(file.filename);
    assert.deepEqual(bytes, payloads[file.format]); assert.equal(file.byteLength, bytes.length);
    assert.equal(file.sha256, sha256(bytes)); assert.equal(file.mimeType, types[file.format]);
  }
});

test("deterministic bytes are independent of input ordering and previous invocations", async () => {
  const first = await createBookEditionBundle(inputs()); await tick();
  const second = await createBookEditionBundle(inputs().reverse());
  assert.deepEqual(await second.blob.arrayBuffer(), await first.blob.arrayBuffer());
  const other = await createBookEditionBundle(inputs(), { sourceLength: 1 });
  assert.notDeepEqual(await other.blob.arrayBuffer(), await first.blob.arrayBuffer());
});
for (const format of Object.keys(types)) test(`single-format ${format} bundles have no unrequested outputs`, async () => {
  const result = await createBookEditionBundle([output(format)]);
  const entries = await readBundle(result.blob);
  assert.equal(entries.size, 2); assert.equal(result.manifest.files.length, 1);
  assert.equal(result.manifest.files[0].format, format);
});

test("changing the input containers during hashing cannot retarget a captured bundle", async () => {
  const files = inputs(), original = files[0].blob;
  const pending = createBookEditionBundle(files);
  files[0].format = "site"; files[0].blob = output("site").blob; files.length = 0;
  const result = await pending, entries = await readBundle(result.blob);
  assert.deepEqual(entries.get("book.pdf"), new Uint8Array(await original.arrayBuffer()));
  assert.equal(entries.size, 4);
});

test("Blob subclass accessors and read overrides are never invoked", async () => {
  let calls = 0;
  class HostileBlob extends Blob {
    get size() { calls++; throw Error("size override"); }
    get type() { calls++; throw Error("type override"); }
    arrayBuffer() { calls++; throw Error("read override"); }
    slice() { calls++; throw Error("slice override"); }
  }
  const result = await createBookEditionBundle([{ format: "pdf", blob: new HostileBlob([payloads.pdf], { type: types.pdf }) }]);
  assert.equal(calls, 0); assert.deepEqual((await readBundle(result.blob)).get("book.pdf"), payloads.pdf);
});

test("malformed records, duplicate formats, accessors and unsafe names fail before hashing", async () => {
  let hashes = 0, getters = 0;
  const crypto = { subtle: { digest() { hashes++; throw Error("must not hash"); } } };
  const bad = [null, [], {}, new Array(1), [null], [output("pdf"), output("pdf")],
    [...inputs(), output("pdf")], [{ ...output("pdf"), filename: "../bad" }],
    [{ format: "../../file", blob: output("pdf").blob }], [{ format: "pdf", blob: {} }],
    [{ format: "pdf", blob: new Blob([], { type: types.pdf }) }],
    [{ format: "pdf", blob: output("epub").blob }],
    [{ format: "pdf", get blob() { getters++; return output("pdf").blob; } }]];
  for (const files of bad) await assert.rejects(createBookEditionBundle(files, { crypto }), cause => !!cause.code);
  assert.equal(hashes, 0); assert.equal(getters, 0);
});

test("array iteration stays bounded and ignores custom iterators", async () => {
  const files = inputs(); files[Symbol.iterator] = () => { throw Error("must not iterate"); };
  assert.equal((await createBookEditionBundle(files)).manifest.files.length, 3);
});

test("aggregate byte admission rejects oversized Blobs without a payload read or hash", async () => {
  const chunk = new Blob([new Uint8Array(1024 * 1024)]);
  const huge = new Blob([...Array(128).fill(chunk), "x"], { type: types.pdf });
  await assert.rejects(createBookEditionBundle([{ format: "pdf", blob: huge }], { crypto: null }), code("OUTPUT_LIMIT"));
  const half = new Blob(Array(64).fill(chunk), { type: types.pdf });
  const rest = new Blob([...Array(64).fill(chunk), "x"], { type: types.epub });
  await assert.rejects(createBookEditionBundle([{ format: "pdf", blob: half }, { format: "epub", blob: rest }]), code("OUTPUT_LIMIT"));
});

test("source metrics and crypto availability are checked explicitly", async () => {
  for (const sourceLength of [-1, NaN, 1.1, 64 * 1024 * 1024 + 1, "1"])
    await assert.rejects(createBookEditionBundle(inputs(), { sourceLength }), code("INVALID_OPTIONS"));
  await assert.rejects(createBookEditionBundle(inputs(), { crypto: null }), code("CRYPTO_UNAVAILABLE"));
  await assert.rejects(createBookEditionBundle(inputs(), { signal: {} }), code("INVALID_OPTIONS"));
  await assert.rejects(createBookEditionBundle(inputs(), { crypto: { subtle: { digest: async () => new ArrayBuffer(31) } } }), code("BUNDLE_DIGEST_FAILED"));
});

test("pre-aborted packaging does not read or hash publications", async () => {
  const abort = new AbortController(); abort.abort();
  await assert.rejects(createBookEditionBundle(inputs(), { signal: abort.signal, crypto: null }), code("EXPORT_CANCELLED"));
});

test("abort settles during an uncooperative digest and consumes its late rejection", async () => {
  const abort = new AbortController(); let reject, started;
  const entered = new Promise(resolve => { started = resolve; });
  const crypto = { subtle: { digest() { started(); return new Promise((_, no) => { reject = no; }); } } };
  const pending = createBookEditionBundle(inputs(), { crypto, signal: abort.signal });
  await entered; const rejected = assert.rejects(pending, code("EXPORT_CANCELLED")); abort.abort(); await rejected;
  reject(Error("late digest failure")); await tick();
});

test("CRC processing yields to cancellation for large publications", async () => {
  const abort = new AbortController(); let timer;
  const crypto = { subtle: { async digest(...args) {
    const hash = await webcrypto.subtle.digest(...args);
    timer = setTimeout(() => abort.abort(), 0); return hash;
  } } };
  const files = [{ format: "pdf", blob: new Blob([new Uint8Array(2 * 1024 * 1024)], { type: types.pdf }) }];
  await assert.rejects(createBookEditionBundle(files, { crypto, signal: abort.signal }), code("EXPORT_CANCELLED"));
  clearTimeout(timer);
});
