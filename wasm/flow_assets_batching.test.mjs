// Production loader/ownership logic, synthetic native transactions and bitmaps.
// These tests do not claim Rust/WASM execution or browser codec validation.

import assert from "node:assert/strict";
import test from "node:test";
import { assetFailure } from "./flow_raster.mjs";
import { FlowImageAssets } from "./flow-assets.js";
import { deferred, image, jpeg, png, Session, tick } from "./tests/flow_image_fixtures.mjs";

class BatchSession extends Session {
  supportsAssetBatches = true;
  batches = [];
  singles = 0;
  provideAsset(result) {
    this.singles++;
    return super.provideAsset(result);
  }
  provideAssets(results) {
    assert(results.length > 0);
    const ids = new Set();
    for (const result of results) {
      assert.equal(result.generation, this.revision);
      assert(!ids.has(result.requestId));
      ids.add(result.requestId);
      assert(this.requests.some((request) => request.id === result.requestId));
    }
    // One publication point, never a loop of the single-image mutation.
    this.requests = this.requests.filter((request) => !ids.has(request.id));
    this.writes.push(...results);
    this.batches.push(results);
    this.reflow();
    return this.token;
  }
}
const options = (extra) => ({
  load: () => png(),
  decode: (_, info) => image(info.width, info.height),
  timeoutMs: 0,
  ...extra,
});
const descriptor = (id = "1", destination = `${id}.png`) => ({
  requestId: id,
  destination,
  isResolved: true,
});
const code = (expected) => (error) => error.code === expected;
function manager(t, session, extra) {
  const assets = new FlowImageAssets(session, options(extra));
  t.after(() => assets.dispose());
  return assets;
}
async function until(check) {
  for (let i = 0; i < 50 && !check(); i++) await tick();
  assert(check(), "expected physical progress");
}

test("ready images use one atomic reflow per bounded group in direct and asynchronous sessions", async (t) => {
  for (const asynchronous of [false, true]) {
    const session = new BatchSession(12);
    let notifications = 0,
      active = 0;
    const original = session.provideAssets.bind(session);
    if (asynchronous)
      session.provideAssets = async (results) => {
        assert.equal(active++, 0);
        await tick();
        const ack = original(results);
        active--;
        return ack;
      };
    const assets = manager(t, session, {
      onChange: () => notifications++,
      load: () => {
        assert.deepEqual(session.pages, [0, 2, 4, 6, 8, 10]);
        return png();
      },
    });
    assert.deepEqual(await assets.loadPending(), {
      revision: "1",
      loaded: 12,
      failed: 0,
      skipped: 0,
      errors: [],
    });
    assert.deepEqual(
      session.batches.map((batch) => batch.length),
      [4, 4, 4],
    );
    assert.equal(session.layoutRevision, "4");
    assert.equal(session.singles, 0);
    assert(session.writes.every((result) => result.bytes === undefined));
    assert.equal(assets.stats.images, 12);
    assert.equal(assets.stats.reservedPixels, 72);
    assert.equal(assets.stats.inFlightBytes, 0);
    assert.equal(notifications, 1);
    await assets.loadPending();
    assert.equal(session.batches.length, 3);
    assert.equal(notifications, 1);
  }
});

test("fast images publish without waiting for a slow loader to fill the group", async (t) => {
  const session = new BatchSession(4),
    slow = deferred();
  const assets = manager(t, session, {
    load: (request) => (request.id === "1" ? slow.promise : png()),
  });
  const pending = assets.loadPending();
  await until(() => session.batches.length === 1);
  assert.deepEqual(
    session.batches[0].map((result) => result.requestId),
    ["2", "3", "4"],
  );
  assert.equal(assets.stats.images, 3);
  assert(assets.busy);
  slow.resolve(png());
  assert.equal((await pending).loaded, 4);
  assert.deepEqual(
    session.batches[1].map((result) => result.requestId),
    ["1"],
  );
});

test("ready decoders coalesce behind a dispatched transaction with physical reservations held", async (t) => {
  const session = new BatchSession(4),
    gate = deferred(),
    decodeGates = [null, deferred(), deferred(), deferred()];
  const original = session.provideAssets.bind(session);
  let decodes = 0,
    active = 0,
    calls = 0;
  session.provideAssets = async (...args) => {
    assert.equal(args.length, 1);
    assert.equal(active++, 0);
    if (++calls === 1) await gate.promise;
    const ack = original(args[0]);
    active--;
    return ack;
  };
  const assets = manager(t, session, { decode: () => decodeGates[decodes++]?.promise ?? image() });
  const pending = assets.loadPending();
  await until(() => calls === 1);
  for (const item of decodeGates.slice(1)) item.resolve(image());
  await tick();
  assert.equal(calls, 1);
  assert.equal(assets.stats.inFlightBytes, 4 * png().length);
  assert.equal(assets.stats.reservedPixels, 24);
  assert.equal(assets.stats.images, 0);
  gate.resolve();
  assert.equal((await pending).loaded, 4);
  assert.deepEqual(
    session.batches.map((batch) => batch.length),
    [1, 3],
  );
  assert.equal(assets.stats.inFlightBytes, 0);
});

test("failed atomic groups close every bitmap without single-image replay or implicit retry", async (t) => {
  const session = new BatchSession(6),
    bitmaps = [],
    original = session.provideAssets.bind(session);
  let calls = 0,
    loads = 0;
  session.provideAssets = (results) => {
    if (++calls === 1) throw assetFailure("ASSET_PIXEL_LIMIT", "synthetic native refusal");
    return original(results);
  };
  const assets = manager(t, session, {
    load: () => {
      loads++;
      return png();
    },
    decode: () => {
      const bitmap = image();
      bitmaps.push(bitmap);
      return bitmap;
    },
  });
  const report = await assets.loadPending();
  assert.equal(report.loaded, 2);
  assert.equal(report.failed, 4);
  assert.deepEqual(
    report.errors,
    ["1", "2", "3", "4"].map((requestId) => ({ requestId, code: "ASSET_PIXEL_LIMIT" })),
  );
  assert.deepEqual(
    bitmaps.map((bitmap) => bitmap.closed),
    [1, 1, 1, 1, 0, 0],
  );
  assert.deepEqual(
    session.requests.map((request) => request.id),
    ["1", "2", "3", "4"],
  );
  assert.equal(session.singles, 0);
  assert.equal(assets.stats.reservedPixels, 12);
  assert.equal(assets.resolveImage(descriptor(), session.token), null);
  assert.equal((await assets.loadPending()).skipped, 4);
  assert.equal(loads, 6);
  assert.equal(calls, 2);
});

test("authorization refusals and invalid decodes stay per-image, outside valid ready groups", async (t) => {
  const session = new BatchSession(4),
    bad = image(100, 100);
  let decodes = 0;
  const assets = manager(t, session, {
    load: (request) => (request.id === "1" ? null : png()),
    decode: () => (++decodes === 1 ? bad : image()),
  });
  const report = await assets.loadPending();
  assert.equal(report.loaded, 2);
  assert.equal(report.failed, 1);
  assert.equal(report.skipped, 1);
  assert.deepEqual(report.errors, [{ requestId: "2", code: "INVALID_IMAGE" }]);
  assert.deepEqual(
    session.batches[0].map((result) => result.requestId),
    ["3", "4"],
  );
  assert.equal(bad.closed, 1);
  assert.equal(session.singles, 0);
});

test("retained payload groups are byte-bounded and use exact immutable decoder snapshots", async (t) => {
  const session = new BatchSession(4),
    expected = png(),
    decoded = [],
    pools = [];
  const assets = manager(t, session, {
    retainSourceBytes: true,
    limits: { maxAssetBytes: expected.length * 2 },
    load: () => {
      const pool = new Uint8Array(expected.length + 20);
      pool.set(expected, 7);
      pools.push(pool);
      return pool.subarray(7, 7 + expected.length);
    },
    decode: async (blob) => {
      const bytes = new Uint8Array(await blob.arrayBuffer());
      decoded.push(bytes);
      for (const pool of pools) pool.fill(255);
      return image();
    },
  });
  assert.equal((await assets.loadPending()).loaded, 4);
  assert.deepEqual(
    session.batches.map((batch) => batch.length),
    [2, 2],
  );
  assert(decoded.every((bytes) => bytes.length === expected.length));
  for (const result of session.writes) assert.deepEqual(result.bytes, expected);
  for (const group of session.batches)
    assert(group.reduce((n, result) => n + result.bytes.byteLength, 0) <= expected.length * 2);
  assert(pools.every((pool) => pool.byteLength === expected.length + 20));
  assert.equal(assets.stats.inFlightBytes, 0);
  assert.equal(session.singles, 0);
});

test("abort during batch delivery preserves authorized committed images without signalling the worker", async (t) => {
  const session = new BatchSession(4),
    gate = deferred(),
    bitmaps = [],
    controller = new AbortController();
  const original = session.provideAssets.bind(session);
  let dispatched = false;
  session.provideAssets = async (...args) => {
    assert.equal(args.length, 1);
    assert.equal(args[0].length, 4);
    dispatched = true;
    await gate.promise;
    return original(args[0]);
  };
  const assets = manager(t, session, {
    decode: () => {
      const bitmap = image();
      bitmaps.push(bitmap);
      return bitmap;
    },
  });
  const pending = assets.loadPending({ signal: controller.signal }),
    refused = assert.rejects(pending, code("ABORTED"));
  await until(() => dispatched);
  controller.abort();
  await refused;
  assert(assets.busy);
  assert.equal(assets.stats.inFlightBytes, 4 * png().length);
  await assert.rejects(assets.loadPending(), code("ASSET_BUSY"));
  gate.resolve();
  await assets.whenIdle();
  assert(!assets.busy);
  assert.equal(assets.stats.images, 4);
  assert.equal(assets.stats.inFlightBytes, 0);
  assert.equal(assets.resolveImage(descriptor(), session.token), bitmaps[0]);
  assert(bitmaps.every((bitmap) => bitmap.closed === 0));
});

test("abort stops queued groups while the already dispatched group settles", async (t) => {
  const session = new BatchSession(4),
    gate = deferred(),
    later = deferred(),
    bitmaps = [],
    controller = new AbortController();
  const original = session.provideAssets.bind(session);
  let decodes = 0,
    calls = 0;
  session.provideAssets = async (results) => {
    calls++;
    await gate.promise;
    return original(results);
  };
  const assets = manager(t, session, {
    decode: async () => {
      if (decodes++) await later.promise;
      const bitmap = image();
      bitmaps.push(bitmap);
      return bitmap;
    },
  });
  const pending = assets.loadPending({ signal: controller.signal }),
    refused = assert.rejects(pending, code("ABORTED"));
  await until(() => calls === 1);
  later.resolve();
  await until(() => bitmaps.length === 4);
  controller.abort();
  await refused;
  gate.resolve();
  await assets.whenIdle();
  assert.equal(calls, 1);
  assert.equal(session.writes.length, 1);
  assert.equal(assets.stats.images, 1);
  assert.deepEqual(
    bitmaps.map((bitmap) => bitmap.closed),
    [0, 1, 1, 1],
  );
  assert.equal(assets.stats.reservedPixels, 6);
  assert.equal(assets.stats.inFlightBytes, 0);
});

test("source changes, revocation and disposal close every late group bitmap exactly once", async (t) => {
  for (const action of ["edit", "clear", "dispose"]) {
    const session = new BatchSession(4),
      gate = deferred(),
      bitmaps = [],
      original = session.provideAssets.bind(session);
    let dispatched = false;
    session.provideAssets = async (results) => {
      dispatched = true;
      await gate.promise;
      if (action === "edit") throw assetFailure("STALE_REVISION", "old resource generation");
      return original(results);
    };
    const assets = manager(t, session, {
      decode: () => {
        const bitmap = image();
        bitmaps.push(bitmap);
        return bitmap;
      },
    });
    const pending = assets.loadPending(),
      refused = assert.rejects(
        pending,
        code(action === "edit" ? "STALE_REVISION" : "ASSET_REVOKED"),
      );
    await until(() => dispatched);
    if (action === "edit") {
      session.edit();
      assets.synchronize();
    } else assets[action]();
    await refused;
    assert(assets.busy);
    gate.resolve();
    await assets.whenIdle();
    assert(bitmaps.every((bitmap) => bitmap.closed === 1));
    assert.equal(assets.stats.images, 0);
    assert.equal(assets.stats.reservedPixels, 0);
    assert.equal(assets.stats.inFlightBytes, 0);
    assert.equal(session.disposed, false);
    assert.equal(session.singles, 0);
  }
});

test("a later resize may advance layout before a batch acknowledgment is observed", async (t) => {
  const session = new BatchSession(4),
    original = session.provideAssets.bind(session);
  session.provideAssets = async (results) => {
    const ack = original(results);
    session.reflow();
    return ack;
  };
  const assets = manager(t, session);
  assert.equal((await assets.loadPending()).loaded, 4);
  assert.equal(session.layoutRevision, "3");
  assert.equal(assets.stats.images, 4);
  assert(assets.resolveImage(descriptor(), session.token));
});

test("invalid acknowledgments never publish a prefix of a group's bitmaps", async (t) => {
  for (const change of [
    (ack) => ({ ...ack, revision: "2" }),
    (ack) => ({ ...ack, layoutRevision: "1" }),
    (ack) => ({ ...ack, layoutRevision: "999" }),
    (ack) => ({ ...ack, layoutRevision: 2 }),
  ]) {
    const session = new BatchSession(4),
      original = session.provideAssets.bind(session),
      bitmaps = [];
    session.provideAssets = (results) => change(original(results));
    const assets = manager(t, session, {
      decode: () => {
        const bitmap = image();
        bitmaps.push(bitmap);
        return bitmap;
      },
    });
    const report = await assets.loadPending();
    assert.equal(report.failed, 4);
    assert.equal(report.loaded, 0);
    assert(report.errors.every((error) => error.code === "ASSET_PROTOCOL_ERROR"));
    assert(bitmaps.every((bitmap) => bitmap.closed === 1));
    assert.equal(assets.stats.images, 0);
    assert.equal(assets.stats.reservedPixels, 0);
    assert.equal(session.singles, 0);
  }
});

test("legacy sessions keep single deliveries and never infer support from a method name", async (t) => {
  for (const advertised of [undefined, false]) {
    const session = new Session(4);
    session.supportsAssetBatches = advertised;
    session.provideAssets = () => {
      throw new Error("unnegotiated method called");
    };
    const assets = manager(t, session);
    assert.equal((await assets.loadPending()).loaded, 4);
    assert.equal(session.writes.length, 4);
    assert.equal(session.layoutRevision, "5");
  }
});

test("inconsistent capabilities are rejected before authorizing any request", async (t) => {
  const session = new Session(4);
  session.supportsAssetBatches = true;
  let loads = 0;
  const assets = manager(t, session, {
    load: () => {
      loads++;
      return png();
    },
  });
  await assert.rejects(assets.loadPending(), code("ASSET_PROTOCOL_ERROR"));
  assert.equal(loads, 0);
  assert.equal(assets.stats.attempted, 0);
  assert(!assets.busy);
});

test("byte and pixel admission still precede decoder allocation for batch-capable sessions", async (t) => {
  for (const limits of [{ maxRetainedPixels: 6 }, { maxInFlightBytes: png().length }]) {
    const session = new BatchSession(2),
      gate = deferred();
    let decodes = 0;
    const assets = manager(t, session, {
      limits,
      decode: async () => {
        decodes++;
        await gate.promise;
        return image();
      },
    });
    const pending = assets.loadPending();
    await until(() => decodes === 1);
    gate.resolve();
    const report = await pending;
    assert.equal(decodes, 1);
    assert.equal(report.loaded, 1);
    assert.equal(report.failed, 1);
    assert.match(report.errors[0].code, /^ASSET_(PIXEL|BYTE)_LIMIT$/);
    assert.equal(assets.stats.inFlightBytes, 0);
    assert.equal(session.singles, 0);
  }
});

test("timeout cannot recycle physical decoder slots even with an empty ready queue", async (t) => {
  const session = new BatchSession(8),
    gate = deferred(),
    bitmaps = [];
  let decodes = 0;
  const assets = manager(t, session, {
    timeoutMs: 10,
    decode: async () => {
      decodes++;
      await gate.promise;
      const bitmap = image();
      bitmaps.push(bitmap);
      return bitmap;
    },
  });
  await assert.rejects(assets.loadPending(), code("ASSET_TIMEOUT"));
  assert.equal(decodes, 4);
  assert(assets.busy);
  await assert.rejects(assets.loadPending(), code("ASSET_BUSY"));
  gate.resolve();
  await assets.whenIdle();
  assert.equal(decodes, 4);
  assert(bitmaps.every((bitmap) => bitmap.closed === 1));
  assert.equal(session.batches.length, 0);
  assert.equal(assets.stats.inFlightBytes, 0);
  assert.equal(assets.stats.reservedPixels, 0);
});

test("large u64 occurrence identities and oriented dimensions survive the grouped publication", async (t) => {
  const session = new BatchSession(2);
  session.requests[0].id = "9007199254740993";
  session.requests[1].id = "18446744073709551615";
  for (const request of session.requests) request.url = "same.jpg";
  const assets = manager(t, session, {
    retainSourceBytes: true,
    load: () => jpeg(),
    decode: () => image(3, 2),
  });
  assert.equal((await assets.loadPending()).loaded, 2);
  assert.deepEqual(
    session.batches[0].map((result) => result.requestId),
    ["9007199254740993", "18446744073709551615"],
  );
  assert(session.writes.every((result) => result.width === 3 && result.height === 2));
  for (const result of session.writes) {
    assert.deepEqual(result.bytes, jpeg());
    assert(assets.resolveImage(descriptor(result.requestId, "same.jpg"), session.token));
  }
});

test("revocation or copy failure in the tail payload cannot dispatch a valid prefix", async (t) => {
  for (const revoke of [false, true]) {
    const session = new BatchSession(4),
      entered = deferred(),
      hold = deferred(),
      bitmaps = [];
    const original = Blob.prototype.arrayBuffer;
    let copies = 0;
    const assets = manager(t, session, {
      retainSourceBytes: true,
      decode: () => {
        const bitmap = image();
        bitmaps.push(bitmap);
        return bitmap;
      },
    });
    Blob.prototype.arrayBuffer = async function () {
      if (++copies === 2) {
        entered.resolve();
        await hold.promise;
        if (!revoke) throw new Error("private loader state must not escape");
      }
      return original.call(this);
    };
    try {
      const pending = assets.loadPending();
      const refusal = revoke ? assert.rejects(pending, code("ASSET_REVOKED")) : null;
      await entered.promise;
      assert.equal(session.batches.length, 0);
      assert.equal(assets.stats.images, 0);
      if (revoke) assets.clear();
      hold.resolve();
      if (revoke) await refusal;
      else {
        const report = await pending;
        assert.equal(report.failed, 4);
        assert(report.errors.every((error) => error.code === "ASSET_LOAD_FAILED"));
      }
      await assets.whenIdle();
      assert.equal(session.writes.length, 0);
      assert.equal(session.singles, 0);
      assert(bitmaps.every((bitmap) => bitmap.closed === 1));
      assert.equal(assets.stats.inFlightBytes, 0);
      assert.equal(assets.stats.reservedPixels, 0);
    } finally {
      hold.resolve();
      Blob.prototype.arrayBuffer = original;
    }
  }
});
