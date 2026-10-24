import { FlowImageAssets } from "../flow-assets.js";
import { deferred, png, Session } from "./flow_image_fixtures.mjs";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { createExportControls } from "../demo/flow_export_controls.mjs";
import { withFlowExports } from "../flow_export.mjs";

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
  async function encoded(mime = "image/png", color = "#e02010") {
    const canvas = new OffscreenCanvas(8, 4),
      context = canvas.getContext("2d");
    context.fillStyle = color;
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
