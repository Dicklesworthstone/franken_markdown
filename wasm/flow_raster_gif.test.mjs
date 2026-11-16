import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { FlowImageAssets } from "./flow-assets.js";
import { rasterInfo } from "./flow_raster.mjs";
import { deferred, image, Session } from "./tests/flow_image_fixtures.mjs";

const limits = { maxAssetBytes: 8 * 1024 * 1024, maxImagePixels: 16777216, maxDimension: 8192 };
const fixture = name => new Uint8Array(Buffer.from(readFileSync(
  new URL("../tests/fixtures/gif/" + name + ".base64", import.meta.url), "utf8",
).replace(/\s/g, ""), "base64"));
const code = expected => error => error.code === expected;
const join = (...parts) => {
  const out = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
  let offset = 0;
  for (const part of parts) { out.set(part, offset); offset += part.length; }
  return out;
};

test("real animated and interlaced GIFs admit complete containers and static preview prefixes", () => {
  for (const [name, width, height] of [["animated-colors", 2, 2], ["interlaced-palette", 96, 96]]) {
    const bytes = fixture(name), info = rasterInfo(bytes, limits);
    assert.equal(info.mime, "image/gif");
    assert.equal(info.width, width);
    assert.equal(info.height, height);
    assert.equal(info.pixels, width * height);
    assert(Object.isFrozen(info));
    const preview = join(bytes.subarray(0, info.firstFrameEnd), [0x3b]);
    assert.deepEqual(rasterInfo(preview, limits), info);
    if (name === "animated-colors") assert(preview.length < bytes.length);
  }
});

test("every truncated GIF prefix and bytes after its trailer are rejected", () => {
  for (const name of ["animated-colors", "interlaced-palette"]) {
    const bytes = fixture(name);
    for (let end = 0; end < bytes.length; end++) {
      assert.throws(() => rasterInfo(bytes.subarray(0, end), limits), name + " prefix " + end);
    }
    assert.throws(() => rasterInfo(join(bytes, [0]), limits), code("INVALID_IMAGE"));
  }
});

test("GIF validation includes later frames, palettes, graphic controls and sub-block boundaries", () => {
  const bytes = fixture("animated-colors");
  const firstImage = bytes.findIndex((byte, index) => byte === 0x2c && index > 25);
  const firstControl = bytes.findIndex((byte, index) => byte === 0x21 && bytes[index + 1] === 0xf9);
  const next = rasterInfo(bytes, limits).firstFrameEnd;
  const secondImage = bytes.findIndex((byte, index) => index > next && byte === 0x2c);
  for (const change of [
    b => { b[11] = 255; }, // Background must address the global palette.
    b => { b[firstControl + 2] = 3; },
    b => { b[firstControl + 3] |= 0xe0; },
    b => { b[firstControl + 3] = 0x1d; }, // Undefined disposal method.
    b => { b[firstControl + 7] = 1; },
    b => { new DataView(b.buffer).setUint16(firstImage + 5, 0, true); },
    b => { new DataView(b.buffer).setUint16(firstImage + 1, 2, true); },
    b => { b[firstImage + 9] |= 0x08; }, // Reserved descriptor bit.
    b => { b[firstImage + 10] = 1; }, // Invalid LZW code size.
    b => { b[firstImage + 11] = 255; }, // Sub-block extends past the source.
    b => { new DataView(b.buffer).setUint16(secondImage + 5, 3, true); },
    b => { b[27] = 10; }, // Application header is exactly 11 bytes.
  ]) {
    const bad = bytes.slice(); change(bad);
    assert.throws(() => rasterInfo(bad, limits), code("INVALID_IMAGE"));
  }
  for (const extension of [0x01, 0x02]) {
    const bad = bytes.slice(); bad[firstControl + 1] = extension;
    assert.throws(() => rasterInfo(bad, limits), code("UNSUPPORTED_IMAGE"));
  }
  const highTransparent = bytes.slice();
  highTransparent[firstControl + 6] = 255;
  assert.equal(rasterInfo(highTransparent, limits).mime, "image/gif",
    "an unused transparent marker needs no palette entry");
  const repeated = join(bytes.subarray(0, firstImage),
    bytes.subarray(firstControl, firstControl + 8), bytes.subarray(firstImage));
  assert.throws(() => rasterInfo(repeated, limits), code("INVALID_IMAGE"));
});

test("GIF local palettes, frame offsets and interlacing retain logical screen dimensions", () => {
  const bytes = fixture("animated-colors"), end = rasterInfo(bytes, limits).firstFrameEnd;
  const firstImage = bytes.findIndex((byte, index) => byte === 0x2c && index > 25);
  const header = bytes.slice(0, 13);
  header[10] &= 0x7f;
  const descriptor = bytes.slice(firstImage, firstImage + 10);
  descriptor[9] |= 0x81; // Local four-entry palette.
  const local = join(header, bytes.subarray(25, firstImage), descriptor,
    bytes.subarray(13, 25), bytes.subarray(firstImage + 10, end), [0x3b]);
  assert.equal(rasterInfo(local, limits).pixels, 4);
  const offset = bytes.slice();
  const view = new DataView(offset.buffer);
  view.setUint16(6, 4, true); view.setUint16(8, 3, true);
  view.setUint16(firstImage + 1, 1, true); view.setUint16(firstImage + 3, 1, true);
  const info = rasterInfo(offset, limits);
  assert.equal(info.width, 4);
  assert.equal(info.height, 3);
  assert.equal(info.pixels, 12, "budget the complete canvas, not just the first frame");
});

test("GIF byte, logical dimension and pixel limits apply before decoding", () => {
  const bytes = fixture("animated-colors");
  for (const changes of [{ maxAssetBytes: bytes.length - 1 },
    { maxDimension: 1 }, { maxImagePixels: 3 }]) {
    assert.throws(() => rasterInfo(bytes, { ...limits, ...changes }),
      code(Object.hasOwn(changes, "maxAssetBytes") ? "ASSET_BYTE_LIMIT" : "ASSET_PIXEL_LIMIT"));
  }
  const huge = bytes.slice();
  new DataView(huge.buffer).setUint16(6, 65535, true);
  new DataView(huge.buffer).setUint16(8, 65535, true);
  assert.throws(() => rasterInfo(huge, limits), code("ASSET_PIXEL_LIMIT"));
});

test("Flow decodes one immutable GIF frame while retaining the exact animated source for export", async () => {
  const original = fixture("animated-colors"), input = original.slice();
  const session = new Session(), bitmap = image(2, 2);
  session.requests[0].url = "animation.gif";
  const assets = new FlowImageAssets(session, {
    load: () => input,
    retainSourceBytes: true,
    decode: async (blob, context) => {
      assert.equal(blob.type, "image/gif");
      assert.equal(context.mime, "image/gif");
      assert.equal(context.width, 2); assert.equal(context.height, 2);
      assert.equal(context.pixels, 4);
      assert(!Object.hasOwn(context, "firstFrameEnd"));
      input.fill(0);
      const preview = new Uint8Array(await blob.arrayBuffer());
      const end = rasterInfo(original, limits).firstFrameEnd;
      assert.deepEqual(preview, join(original.subarray(0, end), [0x3b]));
      assert.equal(rasterInfo(preview, limits).firstFrameEnd, preview.length - 1);
      return bitmap;
    },
  });
  try {
    const report = await assets.loadPending();
    assert.equal(report.loaded, 1); assert.equal(report.failed, 0);
    assert.deepEqual(session.writes[0].bytes, original);
    assert.equal(assets.resolveImage({
      requestId: "1", destination: "animation.gif", isResolved: true,
    }, session.token), bitmap);
    assert.equal(assets.stats.reservedPixels, 4);
    assert.equal(assets.stats.inFlightBytes, 0);
  } finally { assets.dispose(); }
  assert.equal(bitmap.closed, 1);
});

test("invalid later GIF frames never reach the decoder or consume a native image request", async () => {
  const bytes = fixture("animated-colors");
  const end = rasterInfo(bytes, limits).firstFrameEnd;
  const secondImage = bytes.findIndex((byte, index) => index > end && byte === 0x2c);
  new DataView(bytes.buffer).setUint16(secondImage + 5, 3, true);
  const session = new Session();
  const assets = new FlowImageAssets(session, {
    load: () => bytes, decode: () => { assert.fail("invalid container reached decoder"); },
  });
  try {
    const report = await assets.loadPending();
    assert.equal(report.failed, 1);
    assert.equal(report.errors[0].code, "INVALID_IMAGE");
    assert.equal(session.writes.length, 0);
    assert.equal(session.requests.length, 1);
    assert.equal(assets.stats.reservedPixels, 0);
    assert.equal(assets.stats.inFlightBytes, 0);
  } finally { assets.dispose(); }
});

test("revocation during GIF decoding closes the late bitmap and publishes no source bytes", async () => {
  const began = deferred(), finish = deferred(), bitmap = image(2, 2);
  const session = new Session();
  const assets = new FlowImageAssets(session, {
    load: () => fixture("animated-colors"), retainSourceBytes: true,
    decode: () => { began.resolve(); return finish.promise; },
  });
  const result = assert.rejects(assets.loadPending(), code("ASSET_REVOKED"));
  await began.promise;
  assets.clear(); finish.resolve(bitmap);
  await result; await assets.whenIdle();
  assert.equal(bitmap.closed, 1);
  assert.equal(session.writes.length, 0);
  assert.equal(assets.stats.reservedPixels, 0);
  assert.equal(assets.stats.inFlightBytes, 0);
  assets.dispose();
});
