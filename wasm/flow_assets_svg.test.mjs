import assert from "node:assert/strict";
import test from "node:test";
import { FlowImageAssets } from "./flow-assets.js";
import { decodeRaster, rasterInfo } from "./flow_raster.mjs";
import { deferred, image, Session } from "./tests/flow_image_fixtures.mjs";

const limits = { maxAssetBytes: 8 * 1024 * 1024, maxDimension: 8192, maxImagePixels: 16777216 };
const svg = '<svg viewBox="0 0 8 4"><rect width="8" height="4" fill="red"/></svg>';
const bytes = text => new TextEncoder().encode(text);
const descriptor = { requestId: "1", destination: "1.png", isResolved: true };

test("SVG previews normalize dimensions while native retention receives the exact authorized source", async () => {
  const source = bytes('\uFEFF<?xml version="1.0"?>\n<!-- original -->' + svg);
  const original = source.slice(), info = rasterInfo(source, limits), session = new Session();
  const bitmap = image(8, 4);
  let decoded;
  const assets = new FlowImageAssets(session, {
    load: () => source, retainSourceBytes: true,
    decode: async (blob, context) => {
      assert.equal(blob.type, "image/svg+xml");
      assert.equal(context.mime, "image/svg+xml");
      assert.equal(context.width, 8);
      assert.equal(context.height, 4);
      assert.equal(context.pixels, 32);
      for (const internal of ["preview", "previewByteLength", "firstFrameEnd"])
        assert.equal(Object.hasOwn(context, internal), false, internal);
      assert(Object.isFrozen(context));
      assert.equal(assets.stats.inFlightBytes, original.length + info.previewByteLength);
      decoded = await blob.text();
      source.fill(0); // Neither queued preview nor retained source can change.
      return bitmap;
    },
  });
  try {
    const report = await assets.loadPending();
    assert.equal(report.loaded, 1);
    assert.equal(report.failed, 0);
    assert.equal(decoded, info.preview);
    assert(decoded.includes('width="8" height="4"'));
    assert(!decoded.includes("original"));
    assert.deepEqual(session.writes[0].bytes, original);
    assert.equal(assets.resolveImage(descriptor, session.token), bitmap);
    assert.equal(assets.stats.inFlightBytes, 0);
  } finally { assets.dispose(); }
  assert.equal(bitmap.closed, 1);
  assert.equal(assets.stats.reservedPixels, 0);
  assets.dispose();
  assert.equal(bitmap.closed, 1);
});

test("SVG encoded-preview and pixel admission happen before invoking the decoder", async () => {
  const input = bytes(svg), info = rasterInfo(input, limits);
  for (const [settings, expected] of [
    [{ maxInFlightBytes: input.length + info.previewByteLength - 1 }, "ASSET_BYTE_LIMIT"],
    [{ maxAssetBytes: input.length }, "ASSET_BYTE_LIMIT"],
    [{ maxImagePixels: 1 }, "ASSET_PIXEL_LIMIT"],
    [{ maxDimension: 4 }, "ASSET_PIXEL_LIMIT"],
  ]) {
    const session = new Session();
    const assets = new FlowImageAssets(session, {
      load: () => input, limits: settings,
      decode: () => { assert.fail("inadmissible SVG reached the decoder"); },
    });
    try {
      const report = await assets.loadPending();
      assert.equal(report.loaded, 0);
      assert.equal(report.failed, 1);
      assert.equal(report.errors[0].code, expected);
      assert.equal(session.writes.length, 0);
      assert.equal(session.requests.length, 1);
      assert.equal(assets.stats.inFlightBytes, 0);
      assert.equal(assets.stats.reservedPixels, 0);
    } finally { assets.dispose(); }
  }
});

test("invalid SVG UTF-8 and unsupported source constructs never consume a native image request", async () => {
  const invalidUtf8 = bytes(svg);
  invalidUtf8[15] = 0xff;
  for (const input of [
    invalidUtf8,
    bytes('<svg width="8" height="4"><image href="https://example.test/a.png"/></svg>'),
    bytes('<svg width="8" height="4"><style>rect{fill:red}</style></svg>'),
    bytes('<svg width="8" height="4"><rect fill="url(https://example.test/a.svg#x)"/></svg>'),
  ]) {
    const session = new Session(), assets = new FlowImageAssets(session, {
      load: () => input,
      decode: () => { assert.fail("rejected SVG reached the decoder"); },
    });
    try {
      const report = await assets.loadPending();
      assert.equal(report.failed, 1);
      assert.equal(session.requests.length, 1);
      assert.equal(session.writes.length, 0);
    } finally { assets.dispose(); }
  }
});

test("late SVG decoder results remain owned until settlement and close after disposal", async () => {
  const began = deferred(), finish = deferred(), bitmap = image(8, 4);
  const session = new Session(), input = bytes(svg);
  const assets = new FlowImageAssets(session, {
    load: () => input,
    decode: () => { began.resolve(); return finish.promise; },
  });
  const pending = assets.loadPending();
  const rejected = assert.rejects(pending);
  await began.promise;
  assets.dispose();
  assert.equal(assets.busy, true);
  assert(assets.stats.inFlightBytes > input.length);
  finish.resolve(bitmap);
  await rejected;
  await assets.whenIdle();
  assert.equal(bitmap.closed, 1);
  assert.equal(session.writes.length, 0);
  assert.equal(assets.stats.inFlightBytes, 0);
  assert.equal(assets.stats.reservedPixels, 0);
});

// These explicit decoder/DOM doubles exercise lifecycle control, not browser
// pixel fidelity. The browser runner has separate real image/Canvas fixtures.
for (const mode of ["success", "load-error", "abort-load", "abort-bitmap", "constructor-error"]) {
  test("SVG browser fallback ownership: " + mode, async () => {
    const saved = new Map(["Image", "URL", "createImageBitmap"].map(key => [
      key, Object.getOwnPropertyDescriptor(globalThis, key),
    ]));
    const controller = new AbortController(), began = deferred(), extracting = deferred(), finish = deferred();
    const bitmap = image(8, 4), urls = [], revoked = [], elements = [];
    class ImageDouble {
      constructor() {
        if (mode === "constructor-error") throw new Error("image construction failed");
        elements.push(this);
      }
      set src(value) {
        this.source = value;
        if (!value) return;
        began.resolve();
        if (mode !== "abort-load")
          queueMicrotask(() => mode === "load-error" ? this.onerror?.() : this.onload?.());
      }
      get src() { return this.source; }
    }
    const set = (key, value) => Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
    set("Image", ImageDouble);
    set("URL", {
      createObjectURL(blob) { assert.equal(blob.type, "image/svg+xml"); urls.push(blob); return "blob:flow-svg-test"; },
      revokeObjectURL(url) { revoked.push(url); },
    });
    set("createImageBitmap", async value => {
      if (value instanceof Blob) throw new Error("SVG Blob decoding unavailable");
      assert(value instanceof ImageDouble);
      extracting.resolve();
      return mode === "abort-bitmap" ? finish.promise : bitmap;
    });
    try {
      const pending = decodeRaster(new Blob([svg], { type: "image/svg+xml" }), { signal: controller.signal });
      if (mode === "success") {
        assert.equal(await pending, bitmap);
        assert.equal(bitmap.closed, 0);
        bitmap.close();
      } else {
        const rejected = assert.rejects(pending);
        if (mode === "abort-load") { await began.promise; controller.abort(); }
        if (mode === "abort-bitmap") {
          await extracting.promise;
          controller.abort();
          finish.resolve(bitmap);
        }
        await rejected;
        assert.equal(bitmap.closed, mode === "abort-bitmap" ? 1 : 0);
      }
      assert.equal(urls.length, 1);
      assert.deepEqual(revoked, ["blob:flow-svg-test"]);
      for (const element of elements) {
        assert.equal(element.src, "");
        assert.equal(element.onload, null);
        assert.equal(element.onerror, null);
      }
    } finally {
      for (const [key, descriptor] of saved) {
        if (descriptor) Object.defineProperty(globalThis, key, descriptor);
        else delete globalThis[key];
      }
    }
  });
}
