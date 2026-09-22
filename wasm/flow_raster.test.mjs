import assert from "node:assert/strict";
import test from "node:test";
import { decodeRaster, rasterInfo } from "./flow_raster.mjs";
import { jpeg, png } from "./tests/flow_image_fixtures.mjs";

const limits = { maxAssetBytes: 1000000, maxImagePixels: 1000000, maxDimension: 8192 };
const code = (expected) => (error) => error.code === expected;

test("PNG and baseline/progressive JPEG admission reads real dimensions", () => {
  assert.deepEqual(rasterInfo(png(7, 11), limits), {
    mime: "image/png",
    width: 7,
    height: 11,
    pixels: 77,
  });
  for (const progressive of [false, true])
    assert.deepEqual(rasterInfo(jpeg(7, 11, progressive), limits), {
      mime: "image/jpeg",
      width: 7,
      height: 11,
      pixels: 77,
    });
});

test("every truncated container is rejected", () => {
  for (const bytes of [png(), jpeg(), jpeg(2, 3, true)]) {
    for (let end = 0; end < bytes.length; end++)
      assert.throws(() => rasterInfo(bytes.subarray(0, end), limits));
  }
});

test("byte/pixel bounds are checked before browser decode", () => {
  for (const bytes of [png(8193, 2), png(8192, 8192), png(0, 4), jpeg(0, 4), jpeg(5000, 5000)]) {
    assert.throws(() => rasterInfo(bytes, limits), code("ASSET_PIXEL_LIMIT"));
  }
  assert.throws(() => rasterInfo(png(), { ...limits, maxAssetBytes: 1 }), code("ASSET_BYTE_LIMIT"));
});

test("MIME names never admit SVG, GIF, APNG, compressed PNG metadata or unknown critical chunks", () => {
  for (const text of ["<svg xmlns='http://www.w3.org/2000/svg'/>", "GIF89a", "RIFF1234WEBP"]) {
    assert.throws(
      () => rasterInfo(new TextEncoder().encode(text), limits),
      code("UNSUPPORTED_IMAGE"),
    );
  }
  for (const type of ["acTL", "fcTL", "fdAT", "iCCP", "zTXt", "iTXt"]) {
    assert.throws(
      () => rasterInfo(png(2, 3, [[type, new Uint8Array(8)]]), limits),
      code("UNSUPPORTED_IMAGE"),
    );
  }
  assert.throws(
    () => rasterInfo(png(2, 3, [["AAAA", new Uint8Array()]]), limits),
    code("INVALID_IMAGE"),
  );
});

test("duplicate frame headers, trailing bytes, bad chunk sizes and delayed JPEG dimensions are refused", () => {
  const original = png();
  const repeated = new Uint8Array(original.length + 25);
  repeated.set(original.subarray(0, 33));
  repeated.set(original.subarray(8, 33), 33);
  repeated.set(original.subarray(33), 58);
  assert.throws(() => rasterInfo(repeated, limits));
  const huge = png();
  new DataView(huge.buffer).setUint32(33, 0xffffffff);
  assert.throws(() => rasterInfo(huge, limits));
  for (const bytes of [png(), jpeg()]) {
    const trailing = new Uint8Array(bytes.length + 1);
    trailing.set(bytes);
    assert.throws(() => rasterInfo(trailing, limits));
  }
  const dnl = jpeg();
  dnl[3] = 0xdc;
  assert.throws(() => rasterInfo(dnl, limits));
});

test("non-shared exact byte views are required, including detached empty buffers", () => {
  assert.throws(() => rasterInfo(new DataView(new ArrayBuffer(4)), limits));
  assert.throws(() => rasterInfo(new Uint8Array(new SharedArrayBuffer(4)), limits));
  const bytes = png();
  structuredClone(bytes, { transfer: [bytes.buffer] });
  assert.throws(() => rasterInfo(bytes, limits));
});

test("default decoder fails explicitly when createImageBitmap is unavailable", async () => {
  assert.equal(typeof globalThis.createImageBitmap, "undefined");
  await assert.rejects(
    decodeRaster(new Blob([png()], { type: "image/png" })),
    code("IMAGE_DECODER_UNAVAILABLE"),
  );
});
