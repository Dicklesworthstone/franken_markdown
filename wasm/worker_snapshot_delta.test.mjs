import assert from "node:assert/strict";
import test from "node:test";
import {
  SNAPSHOT_DELTA_CACHE_BYTES,
  SnapshotDeltaEncoder,
  SnapshotDeltaDecoder,
  OwnedWorkerRpc,
  serveOwnedWorker,
} from "./worker_transport.mjs";

const item = (index) => ({
  kind: "text", text: `Paragraph ${index}: ${"中 α𝑥 ".repeat(20)}`,
  source: { start: index * 100, end: index * 100 + 99 },
  glyphs: [{ id: index, x: index / 3, advance: 3.5 }],
});
const page = (count = 32, revision = "1") => ({
  schemaVersion: 1, revision, layoutRevision: revision,
  offset: 0, total: count, nextOffset: null,
  items: Array.from({ length: count }, (_, index) => item(index)),
  width: 360, height: count * 24,
});
const roundtrip = (encoder, decoder, value, baseId = decoder.baseId) => {
  const packet = structuredClone(encoder.encode(value, baseId));
  const actual = decoder.decode(packet);
  assert.deepEqual(actual, value);
  return packet;
};
const invalid = (fn) => assert.throws(fn, { code: "WORKER_PROTOCOL_ERROR" });

test("first page is full; localized edits transfer only changed items and all fresh metadata", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  const before = page();
  assert.equal(roundtrip(encoder, decoder, before).kind, "full");
  const after = structuredClone(before);
  after.revision = "18446744073709551615";
  after.layoutRevision = "18446744073709551615";
  after.height += 100;
  after.items[12].text = "Updated paragraph";
  const packet = roundtrip(encoder, decoder, after);
  assert.equal(packet.kind, "delta");
  assert.deepEqual(packet.changes.map(([index]) => index), [12]);
  assert.equal(Object.hasOwn(packet.page, "items"), false);
  assert.ok(JSON.stringify(packet).length < JSON.stringify(after).length / 10);
});

test("caller and backend mutations cannot poison either baseline", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  const value = page();
  const original = structuredClone(value);
  const first = decoder.decode(structuredClone(encoder.encode(value, null)));
  first.items[0].glyphs[0].x = 9999;
  first.items.pop();
  value.items[2].text = "backend reused its mutable buffer";
  const packet = roundtrip(encoder, decoder, original);
  assert.equal(packet.kind, "delta");
  assert.deepEqual(packet.changes, []);
});

test("growth, truncation, reordering, glyph changes, and empty pages round trip", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  roundtrip(encoder, decoder, page());
  const grown = page(35, "2");
  const packet = roundtrip(encoder, decoder, grown);
  assert.equal(packet.kind, "delta");
  assert.deepEqual(packet.changes.map(([index]) => index), [32, 33, 34]);
  assert.equal(roundtrip(encoder, decoder, page(30, "3")).kind, "delta");
  const reorder = page(30, "4");
  reorder.items.reverse();
  roundtrip(encoder, decoder, reorder);
  reorder.items[3].glyphs[0].advance += 1;
  roundtrip(encoder, decoder, reorder);
  roundtrip(encoder, decoder, page(0, "5"));
});

test("obsolete queued baselines and explicit resets receive self-contained full pages", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  roundtrip(encoder, decoder, page());
  const obsolete = decoder.baseId;
  roundtrip(encoder, decoder, page(32, "2"));
  assert.equal(roundtrip(encoder, decoder, page(32, "3"), obsolete).kind, "full");
  decoder.clear();
  assert.equal(roundtrip(encoder, decoder, page(32, "4")).kind, "full");
  encoder.clear();
  assert.equal(roundtrip(encoder, decoder, page(32, "5")).kind, "full");
});

test("oversized baselines are not retained; later small pages resume delta transfer", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  roundtrip(encoder, decoder, page());
  const large = page(1, "2");
  large.items[0].text = "x".repeat(SNAPSHOT_DELTA_CACHE_BYTES / 2);
  const packet = roundtrip(encoder, decoder, large);
  assert.equal(packet.kind, "full");
  assert.equal(packet.id, null);
  assert.equal(decoder.baseId, null);
  assert.equal(roundtrip(encoder, decoder, page(32, "3")).kind, "full");
  assert.equal(roundtrip(encoder, decoder, page(32, "4")).kind, "delta");
});

test("malformed, duplicate, out-of-order, sparse, and oversized patches fail transactionally", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  roundtrip(encoder, decoder, page());
  const baseId = decoder.baseId;
  const next = page(33, "2");
  next.items[4].text = "edit";
  const valid = encoder.encode(next, baseId);
  assert.equal(valid.kind, "delta");
  const variations = [
    { baseId: baseId + 100 }, { id: baseId }, { id: null },
    { length: -1 }, { length: 2049 }, { length: 1.5 }, { changes: [] },
    { changes: [[-1, item(0)]] }, { changes: [[33, item(0)]] },
    { changes: [[4, item(0)], [4, item(1)]] },
    { changes: [[32, item(0)], [4, item(1)]] },
    { changes: [[32]] }, { changes: [[32, undefined]] },
    { changes: [[32, { bad: NaN }]] },
    { changes: [[32, { text: "x".repeat(SNAPSHOT_DELTA_CACHE_BYTES) }]] },
    { page: { ...valid.page, items: [] } },
  ];
  for (const variation of variations) {
    invalid(() => decoder.decode({ ...valid, ...variation }));
    assert.equal(decoder.baseId, baseId);
  }
  assert.deepEqual(decoder.decode(structuredClone(valid)), next);
  invalid(() => decoder.decode(structuredClone(valid)));
});

test("invalid full snapshots and non-JSON items do not create baselines", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  for (const items of [null, new Array(2049).fill({}), [undefined], [NaN], [1n], [new Date(0)]]) {
    invalid(() => encoder.encode({ ...page(), items }, null));
    invalid(() => decoder.decode({ kind: "full", id: 1, page: { ...page(), items } }));
    assert.equal(decoder.baseId, null);
  }
  invalid(() => encoder.encode(page(), 0));
  invalid(() => decoder.decode({ kind: "mystery", id: 1, page: page() }));
  roundtrip(encoder, decoder, page());
});

test("deterministic mixed edit trace matches fresh pages for 250 revisions", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  let seed = 0x5eed;
  const random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0);
  const current = page();
  for (let revision = 1; revision <= 250; revision += 1) {
    const index = random() % Math.max(current.items.length, 1);
    switch (random() % 4) {
      case 0: current.items.splice(index, 0, item(random() % 500)); break;
      case 1: current.items.splice(index, 1); break;
      case 2: if (current.items[index]) current.items[index].text += ` ${revision}`; break;
      case 3: current.items.reverse(); break;
    }
    current.revision = String(revision);
    current.layoutRevision = String(revision * 2);
    current.total = current.items.length;
    roundtrip(encoder, decoder, current, revision % 11 === 0 ? null : decoder.baseId);
  }
});

// Real structured clone + production RPC, with an explicitly named in-memory
// endpoint/renderer double. This is transport proof, not generated-WASM proof.
function endpointPair() {
  const endpoints = [0, 1].map(() => ({ listeners: new Map(), closed: false }));
  endpoints.forEach((endpoint, side) => {
    endpoint.addEventListener = (kind, listener) => endpoint.listeners.set(kind, listener);
    endpoint.removeEventListener = (kind) => endpoint.listeners.delete(kind);
    endpoint.postMessage = (value) => {
      const copied = structuredClone(value);
      queueMicrotask(() => {
        if (!endpoint.closed) endpoints[1 - side].listeners.get("message")?.({ data: copied });
      });
    };
    endpoint.terminate = () => { endpoints.forEach((entry) => { entry.closed = true; }); };
  });
  return endpoints;
}

test("production RPC reconstructs deltas before resolution; queued stale bases fall back", async () => {
  const [client, worker] = endpointPair();
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  const kinds = [];
  let revision = 0;
  const stop = serveOwnedWorker(worker, (_method, [base]) => {
    const value = encoder.encode(page(32, String(++revision)), base);
    kinds.push(value.kind);
    return { value, state: { revision } };
  });
  const rpc = new OwnedWorkerRpc(client, {}, (_state, _method, result) => {
    const decoded = decoder.decode(result);
    for (const key of Object.keys(result)) delete result[key];
    Object.defineProperties(result, Object.getOwnPropertyDescriptors(decoded));
  });
  const request = () => {
    const base = decoder.baseId;
    return rpc.request("snapshotDelta", 16, () => ({ args: [base] }));
  };
  try {
    assert.deepEqual(await request(), page(32, "1"));
    const one = request();
    const two = request();
    assert.deepEqual(await one, page(32, "2"));
    assert.deepEqual(await two, page(32, "3"));
    assert.deepEqual(await request(), page(32, "4"));
    assert.deepEqual(kinds, ["full", "delta", "full", "delta"]);
  } finally { rpc.dispose(); stop(); }
});

test("a corrupted baseline reply terminates RPC instead of publishing a partial page", async () => {
  const [client, worker] = endpointPair();
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  let calls = 0;
  const stop = serveOwnedWorker(worker, (_method, [base]) => {
    const value = encoder.encode(page(), base);
    if (++calls === 2) value.baseId = 999;
    return { value, state: {} };
  });
  const rpc = new OwnedWorkerRpc(client, {}, (_state, _method, value) => decoder.decode(value));
  try {
    await rpc.request("snapshotDelta", 16, () => ({ args: [null] }));
    await assert.rejects(
      rpc.request("snapshotDelta", 16, () => ({ args: [decoder.baseId] })),
      { code: "WORKER_PROTOCOL_ERROR" },
    );
    assert.equal(rpc.closed, true);
  } finally { rpc.dispose(); stop(); }
});

async function flowFixture({ legacy = false, legacyViewport = false, corrupt = false, corruptViewport = false } = {}) {
  const { createWorkerFlowSessionWith, installFlowWorker } = await import("./flow_worker_session.mjs");
  const [client, worker] = endpointPair();
  const replies = [];
  const requests = [];
  let revision = 1;
  let gate = null;
  const state = {
    supportsViewport: true,
    layoutOptions: { viewportWidth: 360, bodySize: 14, codeSize: 12, lineHeight: 20 },
    get token() { return { revision: String(revision), layoutRevision: String(revision) }; },
    source: "# Guide",
    async snapshot(options) {
      if (gate) await gate;
      if (options.token && options.token.revision !== String(revision)) {
        const error = new Error("stale source revision");
        error.code = "REVISION_CONFLICT";
        throw error;
      }
      const current = page(32, String(revision));
      const start = options.offset;
      current.items = current.items.slice(start, start + options.limit);
      current.offset = start;
      current.nextOffset = start + current.items.length < 32 ? start + current.items.length : null;
      return current;
    },
    async viewport(options) {
      const current = await this.snapshot({ offset: 0, limit: 32, token: options.token });
      const visible = current.items.map((entry, index) => ({ ...entry, index }))
        .filter((entry) => entry.index >= options.afterIndex && entry.index % 3 === 0)
        .slice(0, options.limit);
      const last = visible.at(-1)?.index ?? 31;
      return {
        schemaVersion: 1, ...this.token,
        queryKind: "viewport-v1", shapingProfile: "bundled-simple-ltr",
        afterIndex: options.afterIndex, total: 32,
        items: visible.map((entry, offset) => ({
          ...entry,
          bounds: { x: 0, y: offset * 20, width: 100, height: 20 },
          effectiveClip: { ...options.viewport },
        })),
        viewport: { ...options.viewport },
        totalBounds: { x: 0, y: 0, width: 360, height: 1000 },
        visitedEntries: 32 - options.afterIndex,
        nextIndex: visible.length === options.limit && last + 1 < 32 ? last + 1 : null,
      };
    },
    reflow(options) {
      this.layoutOptions = { ...this.layoutOptions, ...options };
      revision += 1;
      return this.token;
    },
    dispose() {},
  };
  const sendRequest = client.postMessage;
  client.postMessage = (request) => { requests.push(structuredClone(request)); sendRequest(request); };
  const sendReply = worker.postMessage;
  worker.postMessage = (reply) => {
    if (reply.ok && legacy && reply.value?.supportsSnapshotDeltas === true) {
      delete reply.value.supportsSnapshotDeltas;
      delete reply.value.supportsViewportDeltas;
    }
    if (reply.ok && legacyViewport && reply.value?.supportsViewportDeltas === true)
      delete reply.value.supportsViewportDeltas;
    if (reply.ok && corrupt && reply.value?.kind === "delta")
      reply.value.page.revision = "999";
    if (reply.ok && corruptViewport && reply.value?.kind === "delta" && reply.value.page.viewport)
      reply.value.page.viewport.x += 1;
    replies.push(structuredClone(reply));
    sendReply(reply);
  };
  const stop = installFlowWorker(worker, () => state);
  const session = await createWorkerFlowSessionWith(() => client, "# Guide", {}, { timeoutMs: 1000 });
  return {
    session, replies, requests,
    block() {
      let resume;
      gate = new Promise((resolve) => { resume = resolve; });
      return () => { gate = null; resume(); };
    },
    close() { session.dispose(); stop(); },
  };
}

test("flow sessions negotiate deltas without exposing envelopes or changing public tokens", async () => {
  const fixture = await flowFixture();
  try {
    const first = await fixture.session.snapshot();
    first.items[0].text = "caller mutated its old snapshot";
    await fixture.session.reflow({ viewportWidth: 720 }, fixture.session.token);
    const second = await fixture.session.snapshot();
    assert.deepEqual(second, page(32, "2"));
    assert.equal(Object.hasOwn(second, "kind"), false);
    assert.equal(fixture.replies.at(-1).value.kind, "delta");
    assert.deepEqual(fixture.replies.at(-1).value.changes, []);
    assert.deepEqual(fixture.session.token, { revision: "2", layoutRevision: "2" });
  } finally { fixture.close(); }
});

test("legacy capability acknowledgments retain full snapshot wire calls", async () => {
  const fixture = await flowFixture({ legacy: true });
  try {
    assert.deepEqual(await fixture.session.snapshot(), page());
    assert.deepEqual(await fixture.session.snapshot(), page());
    assert.deepEqual(fixture.requests.slice(1).map((request) => request.method), ["snapshot", "snapshot"]);
    assert.equal(Object.hasOwn(fixture.replies.at(-1).value, "kind"), false);
  } finally { fixture.close(); }
});

test("the flow page iterator preserves offsets and complete contents through delta negotiation", async () => {
  const fixture = await flowFixture();
  try {
    const collected = [];
    const offsets = [];
    for await (const result of fixture.session.pages({ limit: 8, glyphs: true })) {
      collected.push(...result.items);
      offsets.push(result.offset);
    }
    assert.deepEqual(offsets, [0, 8, 16, 24]);
    assert.deepEqual(collected, page().items);
  } finally { fixture.close(); }
});

test("flow-session revision checks still reject a syntactically valid stale delta", async () => {
  const fixture = await flowFixture({ corrupt: true });
  try {
    await fixture.session.snapshot();
    await assert.rejects(fixture.session.snapshot(), { code: "WORKER_PROTOCOL_ERROR" });
    assert.equal(fixture.session.disposed, true);
  } finally { fixture.close(); }
});

test("recoverable snapshot conflicts do not consume or corrupt the acknowledged baseline", async () => {
  const fixture = await flowFixture();
  try {
    await fixture.session.snapshot();
    await assert.rejects(
      fixture.session.snapshot({ token: { revision: "0", layoutRevision: "0" } }),
      { code: "REVISION_CONFLICT" },
    );
    assert.equal(fixture.session.disposed, false);
    assert.deepEqual(await fixture.session.snapshot(), page());
    assert.equal(fixture.replies.at(-1).value.kind, "delta");
  } finally { fixture.close(); }
});

test("queued cancellation never dispatches a delta request or advances a baseline", async () => {
  const fixture = await flowFixture();
  try {
    await fixture.session.snapshot();
    const resume = fixture.block();
    const active = fixture.session.snapshot();
    const abort = new AbortController();
    const cancelled = fixture.session.snapshot({}, { signal: abort.signal });
    abort.abort();
    await assert.rejects(cancelled, { code: "ABORTED" });
    resume();
    await active;
    assert.deepEqual(await fixture.session.snapshot(), page());
    assert.equal(fixture.replies.at(-1).value.kind, "delta");
    assert.equal(fixture.requests.filter((request) => request.method === "snapshotDelta").length, 3);
  } finally { fixture.close(); }
});

test("native negative-zero geometry remains lossless via uncached full pages", () => {
  const encoder = new SnapshotDeltaEncoder();
  const decoder = new SnapshotDeltaDecoder();
  roundtrip(encoder, decoder, page());
  const value = page();
  value.items[0].glyphs[0].x = -0;
  const packet = roundtrip(encoder, decoder, value);
  assert.equal(packet.kind, "full");
  assert.equal(packet.id, null);
  assert.equal(decoder.baseId, null);
});

test("indexed viewport deltas retain sparse cursor metadata and coexist with snapshot pages", async () => {
  const fixture = await flowFixture();
  const options = { viewport: { x: 0, y: 0, width: 360, height: 800 }, limit: 32 };
  try {
    const first = await fixture.session.viewport(options);
    const second = await fixture.session.viewport(options);
    assert.deepEqual(second, first);
    assert.equal(fixture.replies.at(-1).value.kind, "delta");
    assert.equal(fixture.requests.at(-1).method, "viewportDelta");
    assert.deepEqual(fixture.replies.at(-1).value.changes, []);
    assert.equal(second.nextIndex, null);
    assert.deepEqual(await fixture.session.snapshot(), page());
    assert.deepEqual(await fixture.session.viewport(options), first);
    assert.deepEqual(await fixture.session.viewport(options), first);
    assert.equal(fixture.replies.at(-1).value.kind, "delta");
  } finally { fixture.close(); }
});

test("viewport deltas cannot bypass echoed-rectangle request validation", async () => {
  const fixture = await flowFixture({ corruptViewport: true });
  const options = { viewport: { x: 0, y: 0, width: 360, height: 800 }, limit: 32 };
  try {
    await fixture.session.viewport(options);
    await assert.rejects(fixture.session.viewport(options), { code: "WORKER_PROTOCOL_ERROR" });
    assert.equal(fixture.session.disposed, true);
  } finally { fixture.close(); }
});

test("viewport delta capability is negotiated independently for old workers", async () => {
  const fixture = await flowFixture({ legacyViewport: true });
  const options = { viewport: { x: 0, y: 0, width: 360, height: 800 }, limit: 32 };
  try {
    const first = await fixture.session.viewport(options);
    assert.deepEqual(await fixture.session.viewport(options), first);
    assert.equal(fixture.requests.at(-1).method, "viewport");
    await fixture.session.snapshot();
    assert.equal(fixture.requests.at(-1).method, "snapshotDelta");
  } finally { fixture.close(); }
});

test("sparse viewport pagination keeps original inventory indices after delta decoding", async () => {
  const fixture = await flowFixture();
  const options = { viewport: { x: 0, y: 0, width: 360, height: 800 }, afterIndex: 1, limit: 3 };
  try {
    const first = await fixture.session.viewport(options);
    assert.deepEqual(first.items.map((entry) => entry.index), [3, 6, 9]);
    assert.equal(first.nextIndex, 10);
    assert.deepEqual(await fixture.session.viewport(options), first);
    assert.equal(fixture.replies.at(-1).value.kind, "delta");
    const next = await fixture.session.viewport({ ...options, afterIndex: first.nextIndex });
    assert.deepEqual(next.items.map((entry) => entry.index), [12, 15, 18]);
  } finally { fixture.close(); }
});
