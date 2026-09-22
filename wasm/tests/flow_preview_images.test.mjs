// Real controller + image manager with explicitly synthetic native sessions and
// bitmaps. Browser codecs/pixels and generated WASM have separate proof gates.

import assert from "node:assert/strict";
import test from "node:test";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { FlowImageAssets } from "../flow-assets.js";
import { deferred, image, png, Session, tick } from "./flow_image_fixtures.mjs";

const input = (source, width = 360) => ({ source, width, height: 240 });
async function until(predicate) {
  for (let i = 0; i < 100 && !predicate(); i++) await tick();
  assert(predicate(), "expected asynchronous state did not settle");
}
function fixture({ load = () => png(), decode = () => image() } = {}) {
  const f = {
    sessions: [],
    owners: [],
    paints: [],
    calls: [],
    clears: 0,
    renderHook: null,
    deliveryHook: null,
  };
  f.controller = createPreviewController({
    async createSession(source, layout) {
      const session = new Session();
      session.source = source;
      session.layoutOptions = layout;
      session.dispose = () => {
        session.disposed = true;
      };
      session.replaceSource = async (text, opts) => {
        assert.equal(opts.expectedRevision, session.revision);
        assert(!session.disposed);
        session.source = text;
        session.edit();
      };
      const reflow = session.reflow.bind(session);
      session.reflow = (opts) => {
        if (opts) session.layoutOptions = { ...session.layoutOptions, ...opts };
        reflow();
      };
      session.readingOrder = async () => ({ nodes: [{ text: session.source }], nextOffset: null });
      const provide = session.provideAsset.bind(session);
      session.provideAsset = async (result) => {
        if (f.deliveryHook) await f.deliveryHook();
        if (session.disposed)
          throw Object.assign(new Error("closed"), { code: "SESSION_DISPOSED" });
        return provide(result);
      };
      f.sessions.push(session);
      return session;
    },
    createAssets(session) {
      const owner = new FlowImageAssets(session, {
        timeoutMs: 0,
        load: (request, context) => {
          f.calls.push({ session, request });
          return load(request, context);
        },
        decode,
      });
      f.owners.push(owner);
      return owner;
    },
    painter: {
      async render(session, options) {
        if (f.renderHook) await f.renderHook(session, options);
        if (options.signal.aborted)
          throw Object.assign(new Error("cancelled"), { code: "ABORTED" });
        if (options.token.layoutRevision !== session.token.layoutRevision)
          throw Object.assign(new Error("stale paint"), { code: "STALE_LAYOUT" });
        const bitmap = options.resolveImage(
          { requestId: "1", destination: "1.png", isResolved: session.requests.length === 0 },
          session.token,
        );
        const frame = {
          ...session.token,
          width: options.width,
          height: options.height,
          scrollY: options.scrollY,
        };
        f.paints.push({ frame, bitmap, source: session.source });
        return frame;
      },
      clear() {
        f.clears++;
      },
      dispose() {
        f.clears++;
      },
    },
  });
  return f;
}

test("text is painted before image I/O, then one automatic image refresh occurs", async () => {
  const gate = deferred(),
    f = fixture({ load: () => gate.promise }),
    c = f.controller;
  c.update(input("first"));
  await c.whenIdle();
  assert.equal(c.state.status, "ready");
  assert.equal(f.paints.length, 1);
  assert.equal(f.paints[0].bitmap, null);
  gate.resolve(png());
  await until(() => f.paints.some((p) => p.bitmap));
  assert.equal(f.paints.length, 2);
  assert.equal(c.state.images.loaded, 1);
  assert.equal(f.calls.length, 1);
  c.update({ ...input("first"), scrollY: 80 });
  await c.whenIdle();
  assert.equal(f.calls.length, 1);
  assert.equal(f.sessions[0].revision, "1");
  c.dispose();
});

test("editing proceeds while a loader ignores cancellation and late old bytes never reach layout", async () => {
  const gate = deferred();
  let calls = 0;
  const f = fixture({ load: () => (calls++ === 0 ? gate.promise : png()) }),
    c = f.controller;
  c.update(input("old"));
  await c.whenIdle();
  await until(() => f.calls.length === 1);
  c.update(input("new"));
  await c.whenIdle();
  assert.equal(c.state.reading, "new");
  assert.equal(f.calls.length, 1);
  assert.equal(f.paints.at(-1).bitmap, null);
  gate.resolve(png());
  await until(() => c.state.images?.loaded === 1 && f.paints.at(-1).bitmap !== null);
  assert.equal(f.sessions[0].writes.length, 1);
  assert.equal(f.sessions[0].writes[0].generation, "2");
  c.dispose();
});

test("repeated explicit restarts cannot multiply outstanding decoder batches", async () => {
  const gate = deferred(),
    late = image();
  let decodes = 0;
  const f = fixture({ decode: () => (decodes++ === 0 ? gate.promise : image()) }),
    c = f.controller;
  c.update(input("source"));
  await c.whenIdle();
  await until(() => decodes === 1);
  c.restart();
  await c.whenIdle();
  c.restart();
  await c.whenIdle();
  assert.equal(f.sessions.length, 3);
  assert.equal(f.calls.length, 1);
  assert(f.owners[0].disposed);
  gate.resolve(late);
  await until(() => f.calls.length === 2 && f.paints.at(-1).bitmap !== null);
  assert.equal(late.closed, 1);
  assert.equal(f.sessions[0].writes.length, 0);
  assert.equal(f.sessions[1].writes.length, 0);
  assert.equal(f.sessions[2].writes.length, 1);
  c.dispose();
});

test("refused/invalid images remain placeholders without turning text rendering into an error", async () => {
  for (const load of [() => null, () => new TextEncoder().encode("<svg/>")]) {
    const f = fixture({ load }),
      c = f.controller;
    c.update(input("safe text"));
    await until(() => c.state.images?.status === "ready");
    assert.equal(c.state.status, "ready");
    assert.equal(c.state.reading, "safe text");
    assert.equal(f.sessions[0].writes.length, 0);
    c.dispose();
  }
});

test("a paint racing an image layout update is retried without a reparse or error loop", async () => {
  const deliver = deferred(),
    paint = deferred();
  let readyToDeliver = false,
    readyToPaint = false;
  const f = fixture(),
    c = f.controller;
  f.deliveryHook = async () => {
    readyToDeliver = true;
    await deliver.promise;
  };
  c.update(input("unchanged"));
  await c.whenIdle();
  await until(() => readyToDeliver);
  f.renderHook = async () => {
    readyToPaint = true;
    await paint.promise;
  };
  c.update(input("unchanged", 720));
  await until(() => readyToPaint);
  deliver.resolve();
  await until(() => f.sessions[0].writes.length === 1);
  f.renderHook = null;
  paint.resolve();
  await until(() => c.state.frame?.width === 720 && c.state.status === "ready");
  assert.equal(f.sessions[0].revision, "1");
  assert.equal(f.calls.length, 1);
  assert.equal(f.paints.at(-1).bitmap?.width, 2);
  c.dispose();
});

test("disposing while images are pending closes late results and never refreshes a dead view", async () => {
  const gate = deferred(),
    bitmap = image(),
    f = fixture({ decode: () => gate.promise }),
    c = f.controller;
  c.update(input("source"));
  await c.whenIdle();
  await until(() => f.owners[0].stats.reservedPixels > 0);
  const paints = f.paints.length;
  c.dispose();
  gate.resolve(bitmap);
  await f.owners[0].whenIdle();
  await tick();
  assert.equal(bitmap.closed, 1);
  assert.equal(f.paints.length, paints);
  assert.equal(c.state.status, "disposed");
});
