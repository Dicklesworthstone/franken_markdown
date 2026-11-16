import { FlowImageAssets } from "../flow-assets.js";
import { decodeRaster } from "../flow_raster.mjs";
import { deferred, png, Session } from "./flow_image_fixtures.mjs";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { createExportControls } from "../demo/flow_export_controls.mjs";
import { withFlowExports } from "../flow_export.mjs";

const assert = (condition, label) => {
  if (!condition) throw new Error(label);
};
const delay = () => new Promise((resolve) => setTimeout(resolve, 0));
const descriptor = { requestId: "1", destination: "1.png", isResolved: true };

export async function run(colourJpegs, gifFixtures) {
  assert(Array.isArray(colourJpegs) && colourJpegs.length === 4,
    "all four real CMYK/YCCK fixtures are required");
  assert(Array.isArray(gifFixtures) && gifFixtures.length === 2, "both real GIF fixtures are required");
  const results = [];
  const test = async (name, fn) => {
    await fn();
    results.push(name);
  };
  async function encoded(mime = "image/png", color = "#e02010") {
    const canvas = new OffscreenCanvas(8, 4),
      context = canvas.getContext("2d");
    context.fillStyle = color;
    context.fillRect(0, 0, 8, 4);
    return new Uint8Array(
      await (await canvas.convertToBlob({ type: mime, quality: 1 })).arrayBuffer(),
    );
  }

  const gifs = new Map(gifFixtures.map(({ name, base64 }) => [
    name, Uint8Array.from(atob(base64.replace(/\s/g, "")), byte => byte.charCodeAt(0)),
  ]));
  assert(gifs.has("animated-colors") && gifs.has("interlaced-palette"), "both real GIF fixtures are required");
  async function gifPixels(name, bytes, width, height, expected, previewLength = bytes.length) {
    const session = new Session(), destination = `${name}.gif`;
    session.requests[0].url = destination;
    let decodedBytes;
    const assets = new FlowImageAssets(session, {
      load: () => bytes, retainSourceBytes: true,
      // Observe the host boundary, then use the production browser decoder.
      // Native layout/publication remains the explicit Session fixture.
      decode: async (blob, info) => {
        assert(blob.type === "image/gif" && info.mime === "image/gif", "GIF MIME reaches the codec");
        assert(!Object.hasOwn(info, "firstFrameEnd"), "container offset stays internal");
        decodedBytes = new Uint8Array(await blob.arrayBuffer());
        return decodeRaster(blob, info);
      },
    });
    try {
      const report = await assets.loadPending();
      assert(report.loaded === 1 && report.failed === 0, JSON.stringify(report));
      const bitmap = assets.resolveImage({ ...descriptor, destination }, session.token);
      assert(bitmap instanceof ImageBitmap && bitmap.width === width && bitmap.height === height,
        `${name}: logical-screen bitmap dimensions`);
      const canvas = new OffscreenCanvas(width, height), context = canvas.getContext("2d");
      context.drawImage(bitmap, 0, 0);
      const pixels = context.getImageData(0, 0, width, height).data;
      assert(pixels.length === expected.length, `${name}: full pixel coverage`);
      const difference = expected.findIndex((byte, index) => pixels[index] !== byte);
      assert(difference < 0,
        `${name}: pixel byte ${difference}, got ${pixels[difference]}, expected ${expected[difference]}`);
      assert(decodedBytes.length === previewLength && decodedBytes.at(-1) === 0x3b
        && decodedBytes.subarray(0, -1).every((byte, index) => byte === bytes[index]),
        `${name}: codec receives exactly one complete first frame and a trailer`);
      const retained = session.writes[0].bytes;
      assert(session.writes[0].width === width && session.writes[0].height === height,
        "actual logical dimensions reach layout");
      assert(retained.length === bytes.length && retained.every((byte, index) => byte === bytes[index]),
        `${name}: complete original GIF preserved for publication`);
      assets.dispose();
      assert(bitmap.width === 0 && bitmap.height === 0 && assets.stats.reservedPixels === 0,
        "GIF bitmap and pixel reservations released");
    } finally {
      assets.dispose();
    }
  }
  const originalFetch = globalThis.fetch;
  globalThis.fetch = () => {
    throw new Error("image loading must not fetch");
  };
  try {
    for (const { name, hex } of colourJpegs)
      await test(`${name}: real colour decoding, six Canvas patches and exact retained bytes`, async () => {
        const bytes = Uint8Array.from(hex.replace(/\s/g, "").match(/../g), byte => parseInt(byte, 16));
        const session = new Session();
        const assets = new FlowImageAssets(session, { load: () => bytes, retainSourceBytes: true });
        try {
          const report = await assets.loadPending();
          assert(report.loaded === 1 && report.failed === 0, JSON.stringify(report));
          const bitmap = assets.resolveImage(descriptor, session.token);
          assert(bitmap instanceof ImageBitmap && bitmap.width === 96 && bitmap.height === 32,
            "real colour bitmap dimensions");
          const canvas = new OffscreenCanvas(96, 32), context = canvas.getContext("2d");
          context.drawImage(bitmap, 0, 0);
          const swatches = [[255, 0, 0], [0, 255, 0], [0, 0, 255],
            [255, 255, 255], [0, 0, 0], [127, 127, 127]];
          for (const [index, expected] of swatches.entries()) {
            const pixel = context.getImageData(index * 16 + 8, 16, 1, 1).data;
            assert(pixel[3] === 255 && expected.every((value, channel) => Math.abs(pixel[channel] - value) <= 6),
              `${name} swatch ${index}: ${pixel}`);
          }
          const retained = session.writes[0].bytes;
          assert(retained.length === bytes.length && retained.every((byte, index) => byte === bytes[index]),
            "original JPEG preserved for publication");
          assets.dispose();
          assert(bitmap.width === 0 && bitmap.height === 0, "colour bitmap released");
        } finally {
          assets.dispose();
        }
      });

    await test("animated GIF: first-frame palette, transparency, static codec input and complete retained source", async () => {
      // Frame one is red/green/blue/transparent; frame two is deliberately different.
      await gifPixels("animated-colors", gifs.get("animated-colors"), 2, 2,
        new Uint8Array([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]), 73);
    });
    await test("interlaced GIF: every decoded palette pixel across LZW code-width changes", async () => {
      // Independent Pillow fixture; the source pattern is shared with the native
      // GIF codec regression, not generated by the JavaScript admission parser.
      const expected = new Uint8Array(96 * 96 * 4);
      let state = 0x12345678;
      for (let offset = 0; offset < expected.length; offset += 4) {
        state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
        const index = state >>> 24;
        expected.set([index, 255 - index, (index * 73) & 255, 255], offset);
      }
      await gifPixels("interlaced-palette", gifs.get("interlaced-palette"), 96, 96, expected);
    });
    await test("GIF unused high transparency marker does not require a palette entry", async () => {
      const bytes = gifs.get("animated-colors").slice();
      assert(bytes[44] === 0x21 && bytes[45] === 0xf9 && bytes[50] === 0,
        "real fixture's first graphics control extension");
      bytes[50] = 255; // None of this four-color frame's pixels use the marker.
      await gifPixels("unused-transparency", bytes, 2, 2,
        new Uint8Array([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255]), 73);
    });
    await test("GIF offset frames use local palettes with and without a global palette", async () => {
      const source = gifs.get("animated-colors");
      assert(source[52] === 0x2c && source[61] === 0 && source[71] === 0,
        "real fixture's first image descriptor and complete compressed data");
      // Keep the real compressed first frame, move it to (1, 1), and attach
      // its original palette locally. A conflicting global palette proves
      // precedence; the transparent frame leaves the uncovered canvas clear.
      const local = new Uint8Array(85);
      local.set(source.subarray(0, 62));
      local.set(source.subarray(13, 25), 62);
      local.set(source.subarray(62, 72), 74);
      local[84] = 0x3b;
      local[6] = local[8] = 3;
      local[53] = local[55] = 1;
      local[61] = 0x81;
      local.fill(255, 13, 25);
      const expected = new Uint8Array(3 * 3 * 4);
      expected.set([255, 0, 0, 255], 4 * 4);
      expected.set([0, 255, 0, 255], 5 * 4);
      expected.set([0, 0, 255, 255], 7 * 4);
      await gifPixels("local-palette-offset", local, 3, 3, expected);
      const localOnly = new Uint8Array(local.length - 12);
      localOnly.set(local.subarray(0, 13));
      localOnly.set(local.subarray(25), 13);
      localOnly[10] &= 0x7f;
      await gifPixels("local-palette-only-offset", localOnly, 3, 3, expected);
    });
    await test("GIF malformed suffix and oversized logical canvas never reach the browser codec", async () => {
      const source = gifs.get("animated-colors");
      const trailing = new Uint8Array(source.length + 1);
      trailing.set(source);
      const oversized = source.slice(), view = new DataView(oversized.buffer);
      view.setUint16(6, 8192, true);
      view.setUint16(8, 8192, true);
      for (const bytes of [source.subarray(0, -1), trailing, oversized]) {
        let decoded = false;
        const session = new Session(), assets = new FlowImageAssets(session, {
          load: () => bytes,
          decode: blob => { decoded = true; return decodeRaster(blob); },
        });
        try {
          const report = await assets.loadPending();
          assert(report.failed === 1 && report.loaded === 0 && !decoded, "GIF admission precedes codec");
          assert(session.writes.length === 0 && assets.stats.reservedPixels === 0
            && assets.stats.inFlightBytes === 0, "rejected GIF has no publication or reservation");
        } finally {
          assets.dispose();
        }
      }
    });
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
    await test("restarted preview waits for current images then hands exact native-decoded bytes to export", async () => {
      // Native session and document renderer are explicit boundary fixtures.
      // Controller, asset manager, export adapter/controls, DOM, PNG codec,
      // Canvas pixels and Blob URL behavior execute their production paths.
      const red = await encoded(), blue = await encoded("image/png", "#1030e0"), hold = deferred();
      const sessions = [], owners = [], renders = [], paints = [];
      let loads = 0, controller, controls;
      const panel = document.createElement("section"), source = document.createElement("textarea");
      const html = document.createElement("button"), pdf = document.createElement("button");
      const download = document.createElement("a"), status = document.createElement("p");
      panel.append(source, html, pdf, download, status); document.body.append(panel); source.value = "old";
      let admittedDownload = false;
      try {
        controls = createExportControls({ html, pdf, download, status, sourceEditor: source,
          exportDocument: (...args) => controller.exportDocument(...args) });
        download.addEventListener("click", event => {
          admittedDownload = !event.defaultPrevented; event.preventDefault(); // No browser file save/navigation.
        });
        controller = createPreviewController({
          async createSession(text, layout) {
            const s = new Session(); s.source = text; s.layoutOptions = layout;
            s.dispose = () => { s.disposed = true; };
            s.replaceSource = async next => { s.source = next; s.edit(); };
            s.readingOrder = async () => ({ nodes: [{ text: s.source }], nextOffset: null });
            s.snapshot = ({ offset }) => ({ schemaVersion: 1, ...s.token, offset, total: 1, nextOffset: null,
              items: [{ kind: "image", requestId: "1", destination: "1.png", isResolved: s.requests.length === 0 }] });
            s.assetBytes = () => s.writes.at(-1)?.bytes ?? null;
            s.exportDocument = withFlowExports(s, {
              async html(text, options) {
                renders.push({ text, images: options.pdfImages });
                return { format: "html", mimeType: "text/html; charset=utf-8", diagnostics: [],
                  bytes: new TextEncoder().encode("<!doctype html><title>Explicit renderer fixture</title>") };
              },
              pdf() { throw new Error("PDF rendering is outside this browser boundary test"); },
            }).exportDocument;
            sessions.push(s); return s;
          },
          createAssets(session) {
            const owner = new FlowImageAssets(session, { timeoutMs: 0, retainSourceBytes: true,
              load: () => loads++ === 0 ? hold.promise : blue });
            owners.push(owner); return owner;
          },
          painter: {
            async render(session, options) {
              const bitmap = options.resolveImage({ ...descriptor, isResolved: session.requests.length === 0 }, session.token);
              if (bitmap) {
                const canvas = new OffscreenCanvas(8, 4), context = canvas.getContext("2d");
                context.drawImage(bitmap, 0, 0); paints.push(context.getImageData(3, 2, 1, 1).data);
              }
              return { ...session.token, width: options.width, height: options.height, scrollY: options.scrollY };
            }, clear() {}, dispose() {},
          }, onState: state => controls.update(state),
        });
        const input = text => ({ source: text, width: 360, height: 240 });
        controller.update(input("old")); await controller.whenIdle();
        source.value = "current"; controller.update(input("current")); controller.restart(); await controller.whenIdle();
        assert(controller.state.status === "ready" && controller.state.reading === "current", "text remains ready");
        assert(controller.state.images?.status === "loading" && html.disabled && loads === 1, "current inventory waits behind retired read");
        let refusal;
        try { await controller.exportDocument("html", {}, "current"); } catch (error) { refusal = error.code; }
        assert(refusal === "ASSET_BUSY" && renders.length === 0, "export waits before native inventory/render work");
        hold.resolve(red);
        for (let i = 0; i < 100 && (controller.state.images?.loaded !== 1
          || controller.state.frame.layoutRevision !== sessions.at(-1).token.layoutRevision); i++) await delay();
        assert(controller.state.images?.loaded === 1 && !html.disabled, "current image batch makes export usable");
        const pixel = paints.at(-1);
        assert(pixel?.[2] > 200 && pixel[0] < 60 && pixel[3] === 255, `current blue bitmap ${pixel}`);
        html.click(); for (let i = 0; i < 100 && download.hidden; i++) await delay();
        assert(!download.hidden && renders.length === 1 && renders[0].text === "current", "current source exported once");
        const bytes = renders[0].images[0]?.bytes;
        assert(bytes?.length === blue.length && bytes.every((byte, index) => byte === blue[index]), "exact current encoded PNG retained");
        assert(sessions[0].writes.length === 0 && sessions.at(-1).writes.length === 1, "retired bytes never reach new native session");
        const response = await originalFetch(download.href), blob = await response.blob();
        assert((await blob.text()).includes("Explicit renderer fixture"), "actual export Blob URL resolves");
        const href = download.href; controller.update({ ...input("current"), scrollY: 40 }); await controller.whenIdle();
        download.click(); assert(admittedDownload && download.href === href, "scroll preserves a usable current download");
        controls.update({ ...controller.state, images: { status: "loading" } });
        download.click(); assert(!admittedDownload && download.hidden, "new loading intent revokes old download");
        let revoked = false; try { await originalFetch(href); } catch { revoked = true; }
        assert(revoked, "old Blob URL actually revoked");
        controls.update(controller.state); html.click();
        for (let i = 0; i < 100 && download.hidden; i++) await delay();
        download.click(); assert(admittedDownload && renders.length === 2, "fresh publication works after loading settles");
      } finally {
        hold.resolve(red); controls?.dispose(); controller?.dispose();
        await Promise.all(owners.map(owner => owner.whenIdle())); panel.remove();
      }
    });
    return results;
  } finally {
    globalThis.fetch = originalFetch;
  }
}
