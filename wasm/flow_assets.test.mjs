import test from "node:test";
import assert from "node:assert/strict";
import { FlowImageAssets } from "./flow-assets.js";
import { png, deferred, tick, image, Session } from "./tests/flow_image_fixtures.mjs";
const code = expected => error => error.code === expected;
const options = more => ({ load: () => png(), decode: (_, info) => image(info.width, info.height), timeoutMs: 0, ...more });
const descriptor = (id = "1", destination = `${id}.png`) => ({ requestId: id, destination, isResolved: true });

async function idle(assets) {
  for (let i = 0; i < 50 && assets.busy; i++) await tick();
  assert.equal(assets.busy, false);
}

test("gathers all pending pages before serial, dimension-only deliveries; sync and async sessions", async () => {
  for (const asyncMode of [false, true]) {
    const session = new Session(5), calls = [];
    const original = session.provideAsset.bind(session);
    let writing = 0;
    session.provideAsset = asyncMode ? async result => {
      assert.equal(writing++, 0); await tick(); const value = original(result); writing--; return value;
    } : original;
    const assets = new FlowImageAssets(session, options({ load: r => { calls.push(r.id); assert.deepEqual(session.pages, [0, 2, 4]); return png(); } }));
    const report = await assets.loadPending();
    assert.equal(report.loaded, 5); assert.equal(report.failed, 0); assert.equal(session.writes.length, 5);
    assert(session.writes.every(r => r.bytes === undefined && r.width === 2 && r.height === 3));
    assert.equal(assets.stats.retainedPixels, 30); assert.equal(assets.stats.inFlightBytes, 0);
    assert.equal(assets.resolveImage(descriptor(), session.token).width, 2);
    assert.equal(assets.resolveImage(descriptor("1", "other.png"), session.token), null);
    assert.equal(assets.resolveImage({ ...descriptor(), isResolved: false }, session.token), null);
    session.reflow(); assert.equal(assets.resolveImage(descriptor(), session.token).width, 2);
    assert.equal((await assets.loadPending()).loaded, 0); assert.equal(calls.length, 5);
    assets.dispose(); assert.equal(assets.stats.reservedPixels, 0);
  }
});

test("out-of-order loaders are limited, immutable input views are snapshotted", async () => {
  const session = new Session(3), gates = [deferred(), deferred(), deferred()], decoded = [];
  let started = 0;
  const bytes = png(), pool = new Uint8Array(bytes.length + 30); pool.set(bytes, 10);
  const view = pool.subarray(10, 10 + bytes.length);
  const decodeGate = deferred();
  const assets = new FlowImageAssets(session, options({ limits: { maxConcurrentLoads: 2 },
    load: () => gates[started++].promise, decode: async (blob, info) => {
      await decodeGate.promise; decoded.push(new Uint8Array(await blob.arrayBuffer())); return image(info.width, info.height);
    } }));
  const work = assets.loadPending(); await tick(); assert.equal(started, 2);
  gates[1].resolve(view); await tick(); view.fill(9); decodeGate.resolve(); await tick(); await tick();
  assert.equal(started, 3); gates[0].resolve(png()); gates[2].resolve(png());
  await work; assert.deepEqual(decoded[0], bytes); assert.equal(session.writes[0].requestId, "2"); assets.dispose();
});

test("a source change while decoding closes the late bitmap without a native write", async () => {
  const session = new Session(), gate = deferred(), late = image(); let began = false;
  const assets = new FlowImageAssets(session, options({ decode: () => { began = true; return gate.promise; } }));
  const work = assets.loadPending(); const refused = assert.rejects(work, code("STALE_REVISION"));
  await tick(); assert(began); session.edit(); assets.synchronize();
  await refused; assert.equal(assets.busy, true); gate.resolve(late); await idle(assets);
  assert.equal(late.closed, 1); assert.equal(session.writes.length, 0); assert.equal(assets.stats.reservedPixels, 0);
  assets.dispose();
});

test("reflow preserves bitmaps, but stale geometry and source generations never borrow them", async () => {
  const session = new Session(), bitmap = image();
  const assets = new FlowImageAssets(session, options({ decode: () => bitmap }));
  await assets.loadPending(); const old = session.token; session.reflow();
  assert.throws(() => assets.resolveImage(descriptor(), old), code("STALE_LAYOUT"));
  assert.equal(assets.resolveImage(descriptor(), session.token), bitmap);
  session.edit(); assert.equal(assets.resolveImage(descriptor(), session.token), null); assert.equal(bitmap.closed, 1);
  assert.equal(assets.stats.attempted, 0); assets.dispose(); assert.equal(bitmap.closed, 1);
});

test("timeout settles the public wait, but ignoring abort cannot free physical decode slots", async () => {
  const session = new Session(5), gate = deferred(); let loads = 0;
  const assets = new FlowImageAssets(session, options({ timeoutMs: 10, limits: { maxConcurrentLoads: 1 }, load: () => { loads++; return gate.promise; } }));
  await assert.rejects(assets.loadPending(), code("ASSET_TIMEOUT"));
  assert.equal(assets.busy, true); await assert.rejects(assets.loadPending(), code("ASSET_BUSY"));
  assert.equal(loads, 1); gate.resolve(png()); await idle(assets);
  assert.equal(session.writes.length, 0); assert.equal(assets.stats.inFlightBytes, 0); assets.dispose();
});

test("aborting a dispatched native mutation is not rollback and never signals the worker", async () => {
  const session = new Session(), gate = deferred(), bitmap = image(); let dispatched = false;
  const original = session.provideAsset.bind(session);
  session.provideAsset = async (...args) => { assert.equal(args.length, 1); dispatched = true; await gate.promise; return original(args[0]); };
  const assets = new FlowImageAssets(session, options({ decode: () => bitmap })), controller = new AbortController();
  const work = assets.loadPending({ signal: controller.signal }); const refused = assert.rejects(work, code("ABORTED"));
  await tick(); assert(dispatched); controller.abort(); await refused; assert(assets.busy);
  gate.resolve(); await idle(assets);
  assert.equal(session.writes.length, 1); assert.equal(assets.resolveImage(descriptor(), session.token), bitmap);
  assets.dispose(); assert.equal(bitmap.closed, 1);
});

test("revocation during native delivery discards its eventual bitmap and does not replay", async () => {
  const session = new Session(), gate = deferred(), bitmap = image();
  const original = session.provideAsset.bind(session);
  session.provideAsset = async result => { await gate.promise; return original(result); };
  const assets = new FlowImageAssets(session, options({ decode: () => bitmap }));
  const work = assets.loadPending(), refused = assert.rejects(work, code("ASSET_REVOKED"));
  await tick(); assets.clear(); await refused; gate.resolve(); await idle(assets);
  assert.equal(assets.resolveImage(descriptor(), session.token), null); assert.equal(bitmap.closed, 1);
  assert.equal(session.writes.length, 1); assets.dispose();
});

test("aggregate pixel and in-flight byte admission happen before decoder allocation", async () => {
  for (const limits of [{ maxRetainedPixels: 6 }, { maxInFlightBytes: png().length }]) {
    const session = new Session(2), gate = deferred(); let decodes = 0;
    const assets = new FlowImageAssets(session, options({ limits, decode: async () => { decodes++; await gate.promise; return image(); } }));
    const work = assets.loadPending(); await tick(); assert.equal(decodes, 1); gate.resolve();
    const report = await work; assert.equal(report.loaded, 1); assert.equal(report.failed, 1);
    assert.match(report.errors[0].code, /^ASSET_(PIXEL|BYTE)_LIMIT$/); assets.dispose(); assert.equal(assets.stats.reservedPixels, 0);
  }
});

test("refused authorization and individual failures do not block other images or retry implicitly", async () => {
  const session = new Session(3); let loads = 0;
  const assets = new FlowImageAssets(session, options({ load: r => { loads++; if (r.id === "1") return null;
    if (r.id === "2") throw new Error("private credentials must not leak"); return png(); } }));
  const report = await assets.loadPending(); assert.equal(report.loaded, 1); assert.equal(report.failed, 1); assert.equal(report.skipped, 1);
  assert.deepEqual(report.errors, [{ requestId: "2", code: "ASSET_LOAD_FAILED" }]);
  await assets.loadPending(); assert.equal(loads, 3); assets.dispose();
});

test("malformed inventories are rejected before granting any loader authority", async () => {
  for (const mutate of [p => { p.requests.push(p.requests[0]); p.total++; p.nextOffset = null; },
    p => { p.requests[0].generation = "2"; }, p => { p.nextOffset = 0; },
    p => { p.total = 257; }, p => { p.revision = 1; }]) {
    const session = new Session(), original = session.pendingAssets.bind(session); let loads = 0;
    session.pendingAssets = args => { const page = original(args); mutate(page); return page; };
    const assets = new FlowImageAssets(session, options({ load: () => { loads++; return png(); } }));
    await assert.rejects(assets.loadPending(), code("ASSET_PROTOCOL_ERROR")); assert.equal(loads, 0); assets.dispose();
  }
});

test("bad decoder results and failed layout transactions close owned bitmaps", async () => {
  for (const badDimensions of [true, false]) {
    const session = new Session(), bitmap = image(badDimensions ? 20 : 2, 3);
    if (!badDimensions) session.provideAsset = () => { throw new Error("layout refused"); };
    const assets = new FlowImageAssets(session, options({ decode: () => bitmap }));
    const report = await assets.loadPending(); assert.equal(report.failed, 1); assert.equal(bitmap.closed, 1);
    assert.equal(assets.stats.reservedPixels, 0); assert.equal(assets.stats.inFlightBytes, 0); assets.dispose();
  }
});

test("dispose closes retained and late decoded images exactly once without owning the session", async () => {
  const session = new Session(2), gate = deferred(), first = image(), late = image(); let calls = 0;
  const assets = new FlowImageAssets(session, options({ decode: () => calls++ ? gate.promise : first }));
  const work = assets.loadPending(), refused = assert.rejects(work, code("ASSET_REVOKED"));
  await tick(); assert.equal(assets.stats.images, 1); assets.dispose(); assets.dispose(); await refused;
  assert.equal(first.closed, 1); assert.equal(session.disposed, false); gate.resolve(late); await idle(assets);
  assert.equal(late.closed, 1); assert.equal(assets.stats.reservedPixels, 0);
  await assert.rejects(assets.loadPending(), code("SESSION_DISPOSED"));
});

test("one completion notification; observer failures cannot undo committed images", async () => {
  const session = new Session(4); let calls = 0;
  const assets = new FlowImageAssets(session, options({ onChange: () => { calls++; throw new Error("observer"); } }));
  assert.equal((await assets.loadPending()).loaded, 4); assert.equal(calls, 1);
  await assets.loadPending(); assert.equal(calls, 1); assets.dispose();
});

test("limits, callback types, and already aborted signals reject before loading", async () => {
  const session = new Session();
  for (const extra of [{ limits: { maxAssets: 0 } }, { limits: { maxConcurrentLoads: 5 } },
    { limits: { surprise: 1 } }, { timeoutMs: NaN }, { decode: 1 }, { onChange: 1 }]) {
    assert.throws(() => new FlowImageAssets(session, options(extra)), code("INVALID_OPTIONS"));
  }
  assert.throws(() => new FlowImageAssets(session, {}), code("INVALID_OPTIONS"));
  let loads = 0; const assets = new FlowImageAssets(session, options({ load: () => { loads++; return png(); } }));
  await assert.rejects(assets.loadPending({ signal: AbortSignal.abort() }), code("ABORTED")); assert.equal(loads, 0); assets.dispose();
});
