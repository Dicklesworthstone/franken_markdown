import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { FlowImageAssets } from "./flow-assets.js";
import { decodeRaster, rasterInfo } from "./flow_raster.mjs";
import { Session, deferred, image, jpeg } from "./tests/flow_image_fixtures.mjs";

// Actual encoded six-colour fixtures shared with the PDF path. Their origin
// and expected swatches are documented in tests/pdf_cmyk_jpeg_test.rs.
const fixtures = [
  "jpeg-cmyk-swatches", "jpeg-ycck-swatches",
  "jpeg-cmyk-progressive-swatches", "jpeg-ycck-progressive-swatches",
].map(name => ({
  name,
  bytes: Uint8Array.from(Buffer.from(readFileSync(
    new URL(`../tests/fixtures/${name}.hex`, import.meta.url), "utf8",
  ).replace(/\s/g, ""), "hex")),
}));
const limits = { maxAssetBytes: 8 * 1024 * 1024, maxImagePixels: 16777216, maxDimension: 8192 };
const code = expected => error => error.code === expected;
const marker = (bytes, kind, start = 0) => {
  for (let i = start; i + 1 < bytes.length; i++)
    if (bytes[i] === 255 && bytes[i + 1] === kind) return i;
  throw new Error(`Missing fixture marker ${kind}`);
};
const joined = (...parts) => Uint8Array.from(parts.flatMap(part => [...part]));
const segment = (bytes, kind) => {
  const start = marker(bytes, kind), length = bytes[start + 2] * 256 + bytes[start + 3];
  return { start, end: start + 2 + length, bytes: bytes.slice(start, start + 2 + length) };
};
const edited = (bytes, offset, value) => {
  const copy = bytes.slice(); copy[offset] = value; return copy;
};
const decoder = (t, implementation) => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "createImageBitmap");
  Object.defineProperty(globalThis, "createImageBitmap", { configurable: true, value: implementation });
  t.after(() => {
    if (previous) Object.defineProperty(globalThis, "createImageBitmap", previous);
    else delete globalThis.createImageBitmap;
  });
};

test("real baseline and progressive Adobe CMYK/YCCK are admitted without rewriting bytes", () => {
  for (const { name, bytes } of fixtures) {
    const before = bytes.slice();
    const allocation = joined([1, 2, 3], bytes, [4]);
    assert.deepEqual(rasterInfo(allocation.subarray(3, -1), limits), {
      mime: "image/jpeg", width: 96, height: 32, pixels: 3072,
    }, name);
    assert.deepEqual(bytes, before);
  }
});

test("all truncated colour JPEG containers and trailing bytes remain rejected", () => {
  for (const { name, bytes } of fixtures) {
    for (let end = 0; end < bytes.length; end++)
      assert.throws(() => rasterInfo(bytes.subarray(0, end), limits), undefined, `${name} prefix ${end}`);
    assert.throws(() => rasterInfo(joined(bytes, [0]), limits), code("INVALID_IMAGE"));
  }
});

test("four-channel colour requires an unambiguous Adobe transform before scan data", () => {
  for (const { name, bytes } of fixtures) {
    const app14 = segment(bytes, 0xee);
    const without = joined(bytes.subarray(0, app14.start), bytes.subarray(app14.end));
    const variants = [
      without,
      joined(without.subarray(0, -2), app14.bytes, without.subarray(-2)),
      joined(bytes.subarray(0, 2), [255, 238, 0, 7, 65, 100, 111, 98, 101], bytes.subarray(2)),
      ...[1, 3, 255].map(value => edited(bytes, app14.start + 15, value)),
    ];
    const conflict = edited(app14.bytes, 15, app14.bytes[15] === 0 ? 2 : 0);
    variants.push(joined(bytes.subarray(0, 2), conflict, bytes.subarray(2)));
    variants.push(joined(bytes.subarray(0, -2), conflict, bytes.subarray(-2)));
    for (const value of variants)
      assert.throws(() => rasterInfo(value, limits), code("INVALID_IMAGE"), name);
  }
});

test("identical Adobe duplicates, marker ordering, and existing JFIF metadata remain valid", () => {
  for (const { bytes } of fixtures) {
    const app14 = segment(bytes, 0xee);
    const without = joined(bytes.subarray(0, app14.start), bytes.subarray(app14.end));
    const sos = marker(without, 0xda);
    for (const value of [
      joined(bytes.subarray(0, 2), app14.bytes, bytes.subarray(2)),
      joined(without.subarray(0, sos), app14.bytes, without.subarray(sos)),
      joined(bytes.subarray(0, -2), app14.bytes, bytes.subarray(-2)),
    ]) assert.equal(rasterInfo(value, limits).pixels, 3072);
  }
});

test("four-component frames and every scan validate identities, sampling and coding tables", () => {
  for (const { name, bytes } of fixtures) {
    const progressive = name.includes("progressive");
    const sof = marker(bytes, progressive ? 0xc2 : 0xc0), sos = marker(bytes, 0xda);
    for (const [offset, value] of [
      [sof + 4, 12], [sof + 13, bytes[sof + 10]],
      [sof + 11, 0], [sof + 11, 0x51], [sof + 11, 0x15], [sof + 12, 4],
      [sos + 4, 0], [sos + 5, 250], [sos + 7, bytes[sos + 5]],
      [sos + 6, 0x40], [sos + 6, 0x04],
      [sos + 5 + 2 * bytes[sos + 4], progressive ? 1 : 2],
      [sos + 6 + 2 * bytes[sos + 4], progressive ? 1 : 62],
      [sos + 7 + 2 * bytes[sos + 4], 0xe0],
    ]) assert.throws(() => rasterInfo(edited(bytes, offset, value), limits), code("INVALID_IMAGE"));
    if (progressive) {
      const second = marker(bytes, 0xda, sos + 2);
      assert.throws(() => rasterInfo(edited(bytes, second + 5, 250), limits), code("INVALID_IMAGE"));
    }
  }
});

test("colour photos retain the existing encoded-byte and decoded-pixel ceilings", () => {
  for (const { bytes } of fixtures) {
    assert.throws(() => rasterInfo(bytes, { ...limits, maxAssetBytes: bytes.length - 1 }), code("ASSET_BYTE_LIMIT"));
    assert.throws(() => rasterInfo(bytes, { ...limits, maxImagePixels: 3071 }), code("ASSET_PIXEL_LIMIT"));
    assert.throws(() => rasterInfo(bytes, { ...limits, maxDimension: 95 }), code("ASSET_PIXEL_LIMIT"));
  }
});

test("four-channel Adobe rules do not change the existing grayscale and RGB admission", () => {
  const rgb = Uint8Array.from(Buffer.from(readFileSync(
    new URL("../tests/fixtures/jpeg-quadrants.hex", import.meta.url), "utf8",
  ).replace(/\s/g, ""), "hex"));
  for (const bytes of [jpeg(), jpeg(2, 3, true), rgb]) {
    const expected = rasterInfo(bytes, limits);
    const app14 = segment(fixtures[0].bytes, 0xee).bytes;
    for (const extra of [
      [255, 238, 0, 7, 65, 100, 111, 98, 101],
      edited(app14, 15, 255),
      joined(app14, edited(app14, 15, 2)),
    ]) assert.deepEqual(rasterInfo(joined(bytes.subarray(0, 2), extra, bytes.subarray(2)), limits), expected);
  }
});

test("admitted colour JPEGs retain their exact encoded payloads through asset delivery", async t => {
  for (const { bytes } of fixtures) {
    const session = new Session(), decoded = [], bitmap = image(32, 96);
    const assets = new FlowImageAssets(session, {
      load: () => bytes,
      retainSourceBytes: true,
      decode: async (blob, info) => {
        assert.equal(info.mime, "image/jpeg");
        assert.equal(info.pixels, 3072);
        decoded.push(new Uint8Array(await blob.arrayBuffer()));
        return bitmap; // Existing EXIF transposition admission remains available.
      },
    });
    t.after(() => assets.dispose());
    assert.deepEqual(await assets.loadPending(), { revision: "1", loaded: 1, failed: 0, skipped: 0, errors: [] });
    assert.deepEqual(decoded, [bytes]);
    assert.deepEqual(session.writes[0].bytes, bytes);
    assert.equal(session.writes[0].width, 32);
    assert.equal(session.writes[0].height, 96);
    assert.equal(session.revision, "1");
  }
});

test("browser decoding failures stay typed and cannot consume source asset requests", async t => {
  decoder(t, () => Promise.reject(new Error("decoder failed")));
  await assert.rejects(decodeRaster(new Blob([fixtures[0].bytes], { type: "image/jpeg" })), code("INVALID_IMAGE"));
  const session = new Session(), assets = new FlowImageAssets(session, { load: () => fixtures[0].bytes });
  t.after(() => assets.dispose());
  const before = session.token, report = await assets.loadPending();
  assert.deepEqual(report.errors, [{ requestId: "1", code: "INVALID_IMAGE" }]);
  assert.equal(report.loaded, 0);
  assert.equal(report.failed, 1);
  assert.deepEqual(session.token, before);
  assert.equal(session.requests.length, 1);
  assert.deepEqual(session.writes, []);
  assert.equal(assets.stats.reservedPixels, 0);
  assert.equal(assets.stats.inFlightBytes, 0);
});

test("aborting an admitted colour decode preserves cancellation over a late codec failure", async t => {
  const started = deferred(), pending = deferred(), controller = new AbortController();
  decoder(t, () => { started.resolve(); return pending.promise; });
  const session = new Session(), assets = new FlowImageAssets(session, { load: () => fixtures[0].bytes });
  t.after(() => assets.dispose());
  const before = session.token, operation = assets.loadPending({ signal: controller.signal });
  await started.promise;
  const rejected = assert.rejects(operation, code("ABORTED"));
  controller.abort();
  await rejected;
  assert.equal(assets.busy, true, "physical decode still owns its reservations");
  pending.reject(new Error("late decoder failure"));
  await assets.whenIdle();
  assert.deepEqual(session.token, before);
  assert.equal(session.requests.length, 1);
  assert.deepEqual(session.writes, []);
  assert.equal(assets.busy, false);
  assert.equal(assets.stats.reservedPixels, 0);
  assert.equal(assets.stats.inFlightBytes, 0);
});
