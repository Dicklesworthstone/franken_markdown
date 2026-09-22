import { FlowImageAssets } from "../flow-assets.js";
import { deferred, png, Session } from "./flow_image_fixtures.mjs";

const assert = (condition, label) => {
  if (!condition) throw new Error(label);
};
const delay = () => new Promise((resolve) => setTimeout(resolve, 0));
const descriptor = { requestId: "1", destination: "1.png", isResolved: true };

export async function run() {
  const results = [];
  const test = async (name, fn) => {
    await fn();
    results.push(name);
  };
  async function encoded(mime = "image/png") {
    const canvas = new OffscreenCanvas(8, 4),
      context = canvas.getContext("2d");
    context.fillStyle = "#e02010";
    context.fillRect(0, 0, 8, 4);
    return new Uint8Array(
      await (await canvas.convertToBlob({ type: mime, quality: 1 })).arrayBuffer(),
    );
  }
  const originalFetch = globalThis.fetch;
  globalThis.fetch = () => {
    throw new Error("image loading must not fetch");
  };
  try {
    for (const mime of ["image/png", "image/jpeg"])
      await test(`${mime}: real decoder, bitmap, native dimensions, Canvas pixels and close`, async () => {
        const bytes = await encoded(mime),
          session = new Session();
        const assets = new FlowImageAssets(session, { load: () => bytes });
        try {
          const report = await assets.loadPending();
          assert(report.loaded === 1, JSON.stringify(report));
          const bitmap = assets.resolveImage(descriptor, session.token);
          assert(
            bitmap instanceof ImageBitmap && bitmap.width === 8 && bitmap.height === 4,
            "real bitmap dimensions",
          );
          const canvas = new OffscreenCanvas(8, 4),
            context = canvas.getContext("2d");
          context.drawImage(bitmap, 0, 0);
          const pixel = context.getImageData(3, 2, 1, 1).data;
          assert(
            pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 40 && pixel[3] === 255,
            `decoded color ${pixel}`,
          );
          assert(
            session.writes[0].width === 8 && session.writes[0].bytes === undefined,
            "dimension-only native delivery",
          );
          assets.dispose();
          assert(bitmap.width === 0 && bitmap.height === 0, "owned ImageBitmap was closed");
        } finally {
          assets.dispose();
        }
      });
    await test("JPEG EXIF orientation reports actual transposed dimensions to layout", async () => {
      const bytes = await encoded("image/jpeg");
      const exif = new Uint8Array([
        255, 225, 0, 34, 69, 120, 105, 102, 0, 0, 73, 73, 42, 0, 8, 0, 0, 0, 1, 0, 18, 1, 3, 0, 1,
        0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
      ]);
      const oriented = new Uint8Array(bytes.length + exif.length);
      oriented.set(bytes.subarray(0, 2));
      oriented.set(exif, 2);
      oriented.set(bytes.subarray(2), 2 + exif.length);
      const session = new Session(),
        assets = new FlowImageAssets(session, { load: () => oriented });
      try {
        const report = await assets.loadPending();
        assert(report.loaded === 1, JSON.stringify(report));
        assert(
          session.writes[0].width === 4 && session.writes[0].height === 8,
          "actual oriented layout dimensions",
        );
      } finally {
        assets.dispose();
      }
    });
    await test("invalid compressed pixels fail without delivery or leaked reservations", async () => {
      const session = new Session(),
        assets = new FlowImageAssets(session, { load: () => png() });
      try {
        const report = await assets.loadPending();
        assert(report.failed === 1 && session.writes.length === 0, "decoder rejection");
        assert(
          assets.stats.reservedPixels === 0 && assets.stats.inFlightBytes === 0,
          "reservations released",
        );
      } finally {
        assets.dispose();
      }
    });
    await test("SVG and oversized dimensions never reach browser decode", async () => {
      for (const bytes of [new TextEncoder().encode("<svg onload='throw 1'/>"), png(8192, 8192)]) {
        let called = false;
        const session = new Session(),
          assets = new FlowImageAssets(session, {
            load: () => bytes,
            decode: (blob) => {
              called = true;
              return createImageBitmap(blob);
            },
          });
        try {
          assert((await assets.loadPending()).failed === 1 && !called, "admission precedes codec");
        } finally {
          assets.dispose();
        }
      }
    });
    await test("revocation closes a real bitmap that arrives late", async () => {
      const bytes = await encoded(),
        gate = deferred(),
        began = deferred();
      let decoded;
      const session = new Session(),
        assets = new FlowImageAssets(session, {
          load: () => bytes,
          decode: async (blob) => {
            decoded = await createImageBitmap(blob);
            began.resolve();
            await gate.promise;
            return decoded;
          },
        });
      const work = assets.loadPending().then(
        () => "unexpected success",
        (error) => error.code,
      );
      await began.promise;
      assets.dispose();
      assert((await work) === "ASSET_REVOKED", "public revocation");
      assert(assets.busy && decoded.width === 8, "in-flight ownership remains accounted");
      gate.resolve();
      await assets.whenIdle();
      assert(decoded.width === 0 && assets.stats.reservedPixels === 0, "late real bitmap closed");
      assert(session.writes.length === 0, "late delivery suppressed");
    });
    return results;
  } finally {
    globalThis.fetch = originalFetch;
  }
}
