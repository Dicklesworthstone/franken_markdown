// Production raster admission + asset/export adapters; explicit decoder/native
// session/render doubles. No browser decoding or generated-WASM claim.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowImageAssets } from "../flow-assets.js";
import { withFlowExports } from "../flow_export.mjs";
const png = () => new Uint8Array(Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j2ioAAAAASUVORK5CYII=", "base64"));
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const code = expected => error => error.code === expected;
function fixture(options = {}, count = 1) {
  const calls = [], images = [], payloads = new Map(); let resolved = new Set();
  const session = {
    disposed: false, source: "![x](local.png)", revision: "1", layoutRevision: "1",
    get token() { return { revision: this.revision, layoutRevision: this.layoutRevision }; },
    pendingAssets({ offset, limit }) {
      const requests = Array.from({ length: count }, (_, i) => ({ id: String(i + 1), generation: this.revision,
        kind: "image", url: "local.png", altText: "x", enclosingSourceByteOffset: 0 }))
        .filter(item => !resolved.has(item.id));
      const end = Math.min(offset + limit, requests.length);
      return { schemaVersion: 1, ...this.token, offset, total: requests.length, nextOffset: end < requests.length ? end : null,
        requests: requests.slice(offset, end) };
    },
    provideAsset(result) {
      calls.push(result); resolved.add(result.requestId);
      if (result.bytes) payloads.set(result.requestId, new Uint8Array(result.bytes));
      this.layoutRevision = String(BigInt(this.layoutRevision) + 1n); return this.token;
    },
    snapshot({ offset, limit }) {
      const items = Array.from({ length: count }, (_, i) => ({ kind: "image", requestId: String(i + 1),
        destination: "local.png", isResolved: resolved.has(String(i + 1)) }));
      const end = Math.min(offset + limit, items.length);
      return { schemaVersion: 1, ...this.token, offset, total: items.length, nextOffset: end < items.length ? end : null, items: items.slice(offset, end) };
    },
    assetBytes(id) { return payloads.get(id) ?? null; },
    edit() { this.revision = String(BigInt(this.revision) + 1n); this.layoutRevision = String(BigInt(this.layoutRevision) + 1n); payloads.clear(); resolved = new Set(); }
  };
  const decode = () => { const image = { width: 1, height: 1, closed: false, close() { this.closed = true; } }; images.push(image); return image; };
  const manager = new FlowImageAssets(session, { load: png, decode, ...options });
  return { manager, session, calls, images, payloads, decode };
}
test("retention is explicit: default still sends dimensions only and export rejects missing bytes", async t => {
  const f = fixture(); t.after(() => f.manager.dispose());
  assert.equal((await f.manager.loadPending()).loaded, 1); assert.equal(Object.hasOwn(f.calls[0], "bytes"), false);
  const api = withFlowExports(f.session, { html() { assert.fail("must not render"); }, pdf() { assert.fail("must not render"); } });
  await assert.rejects(api.exportDocument("pdf", {}, api.token), code("UNRESOLVED_EXPORT_ASSET"));
});
test("retains the immutable admitted bytes, not later loader mutations, and exports them", async t => {
  const original = png(), expected = png();
  const f = fixture({ retainSourceBytes: true, load: () => original,
    decode: async blob => { original.fill(0); assert.deepEqual(new Uint8Array(await blob.arrayBuffer()), expected); return { width: 1, height: 1, close() {} }; } });
  t.after(() => f.manager.dispose());
  assert.equal((await f.manager.loadPending()).loaded, 1);
  assert.deepEqual(f.calls[0].bytes, expected); assert.notEqual(f.calls[0].bytes.buffer, original.buffer);
  const render = async (_, settings) => { assert.deepEqual(settings.pdfImages[0].bytes, expected);
    return { format: "pdf", mimeType: "application/pdf", bytes: new Uint8Array([1]), diagnostics: [] }; };
  const result = await withFlowExports(f.session, { html: render, pdf: render }).exportDocument("pdf", {}, f.session.token);
  assert.equal(result.assetCount, 1); assert.equal(result.assetBytes, expected.length);
  assert.equal(f.manager.stats.inFlightBytes, 0);
});
test("source changes close old bitmaps and require fresh payload authorization", async t => {
  let loads = 0;
  const f = fixture({ retainSourceBytes: true, load: () => { loads++; return png(); } }); t.after(() => f.manager.dispose());
  await f.manager.loadPending(); f.session.edit(); f.manager.synchronize();
  assert.equal(f.images[0].closed, true); assert.equal(f.payloads.size, 0);
  await f.manager.loadPending(); assert.equal(loads, 2); assert.equal(f.calls[1].generation, "2");
});
test("invalid retention options do not get coerced", () => {
  for (const value of [null, 1, "true", {}, []]) assert.throws(() => fixture({ retainSourceBytes: value }), code("INVALID_OPTIONS"));
});
test("revocation during the extra Blob copy prevents native payload delivery", async t => {
  const hold = gate(), entered = gate(), original = Blob.prototype.arrayBuffer;
  const f = fixture({ retainSourceBytes: true }); t.after(() => f.manager.dispose());
  Blob.prototype.arrayBuffer = async function () { entered.resolve(); await hold.promise; return original.call(this); };
  t.after(() => { Blob.prototype.arrayBuffer = original; });
  const pending = f.manager.loadPending(), rejected = assert.rejects(pending, code("ASSET_REVOKED"));
  await entered.promise; f.manager.clear(); hold.resolve(); await rejected; await f.manager.whenIdle();
  assert.equal(f.calls.length, 0); assert.equal(f.images[0].closed, true); assert.equal(f.manager.stats.inFlightBytes, 0);
});
test("serialized payload publication does not overlap native delivery", async t => {
  const f = fixture({ retainSourceBytes: true }, 4); t.after(() => f.manager.dispose());
  const original = f.session.provideAsset.bind(f.session); let active = 0, maximum = 0;
  f.session.provideAsset = async result => { maximum = Math.max(maximum, ++active);
    await new Promise(resolve => setTimeout(resolve, 5)); const token = original(result); active--; return token; };
  const report = await f.manager.loadPending(); assert.equal(report.loaded, 4); assert.equal(maximum, 1);
  assert.equal(f.calls.length, 4); assert(f.calls.every(call => call.bytes.length === png().length));
});
test("native budget rejection closes the unpublished bitmap and leaves the image unresolved", async t => {
  const f = fixture({ retainSourceBytes: true }); t.after(() => f.manager.dispose());
  f.session.provideAsset = () => { throw new Error("native payload budget exceeded"); };
  const report = await f.manager.loadPending(); assert.equal(report.failed, 1);
  assert.equal(f.images[0].closed, true); assert.equal(f.manager.stats.reservedPixels, 0); assert.equal(f.manager.stats.inFlightBytes, 0);
  assert.equal(f.payloads.size, 0);
});
