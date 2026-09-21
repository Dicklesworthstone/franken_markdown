import test from "node:test";
import assert from "node:assert/strict";
import { normalizeAssetBatch, packAssetBatch, FLOW_ASSET_BATCH_BYTES, FLOW_ASSET_BATCH_COUNT } from "./flow_asset_batch.mjs";
import { normalizeFlowRequest, snapshotArguments, requestWeight } from "./flow_worker_protocol.mjs";
import { facade, NativeDouble, decodeBatch, result } from "./tests/flow_asset_batch_fixture.mjs";
const code = expected => error => error.code === expected;

test("packing retains exact binary views and distinguishes absent and empty payloads", () => {
  const backing = new Uint8Array([99, 0, 255, 128, 10, 88]);
  const packed = packAssetBatch([result("3"), result("1", backing.subarray(1, 5)), result("2", new Uint8Array())]);
  assert.equal(packed.metadata, "fmd-assets-v1\n3,1,20,10,-\n1,1,20,10,4\n2,1,20,10,0");
  assert.deepEqual(packed.payload, new Uint8Array([0, 255, 128, 10]));
  backing.fill(7); assert.deepEqual(packed.payload, new Uint8Array([0, 255, 128, 10]));
  const values = decodeBatch(packed.metadata, packed.payload);
  assert(!Object.hasOwn(values[0], "bytes")); assert.equal(values[2].bytes.length, 0);
  assert.notEqual(packed.payload.buffer, backing.buffer);
});

test("pooled Node buffers are copied by view, without their unrelated backing bytes", () => {
  const input = Buffer.from([90, 1, 2, 3, 91]).subarray(1, 4);
  const { payload } = packAssetBatch([result("1", input)]);
  assert.deepEqual([...payload], [1, 2, 3]); assert.equal(payload.buffer.byteLength, 3);
  assert.equal(input.byteLength, 3); assert.notEqual(payload.buffer, input.buffer);
});

test("lossless u64 identities round-trip without Number coercion", () => {
  const values = [{ ...result(9007199254740993n), generation: 18446744073709551615n },
    { ...result("18446744073709551615"), generation: "18446744073709551615" }];
  const packed = packAssetBatch(values);
  assert.equal(packed.metadata, "fmd-assets-v1\n9007199254740993,18446744073709551615,20,10,-\n18446744073709551615,18446744073709551615,20,10,-");
  for (const id of [1, "01", "+1", "1.0", "18446744073709551616", -1n]) {
    assert.throws(() => normalizeAssetBatch([result(id)]), code("INVALID_IDENTITY"));
  }
});

test("count, per-payload and aggregate admission apply before packed allocation", () => {
  assert.throws(() => normalizeAssetBatch(new Array(FLOW_ASSET_BATCH_COUNT + 1)), code("BUDGET_EXCEEDED"));
  const exact = Array.from({ length: FLOW_ASSET_BATCH_COUNT }, (_, i) => result(String(i + 1)));
  assert.equal(normalizeAssetBatch(exact).length, FLOW_ASSET_BATCH_COUNT);
  const big = new Uint8Array(8 * 1024 * 1024);
  const four = Array.from({ length: 4 }, (_, i) => result(String(i + 1), big));
  assert.equal(normalizeAssetBatch(four).reduce((n, r) => n + r.bytes.length, 0), FLOW_ASSET_BATCH_BYTES);
  assert.throws(() => normalizeAssetBatch([...four, result("5", new Uint8Array(1))]), code("BUDGET_EXCEEDED"));
  assert.throws(() => normalizeAssetBatch([result("1", new Uint8Array(big.length + 1))]), code("BUDGET_EXCEEDED"));
});

test("the complete batch is validated before any native write", () => {
  const { raw, session } = facade();
  const bad = [
    [[result(), result()], "UNKNOWN_ASSET_REQUEST"],
    [[result(), { ...result("2"), generation: "2" }], "STALE_ASSET_GENERATION"],
    [[result(), { ...result("2"), width: 0 }], "INVALID_ASSET_DIMENSIONS"],
    [[result(), { ...result("2"), height: 32769 }], "INVALID_ASSET_DIMENSIONS"],
    [[result(), { ...result("2"), width: 1.5 }], "INVALID_ASSET_DIMENSIONS"],
    [[result(), { ...result("2"), surprise: true }], "INVALID_OPTIONS"],
    [new Array(1), "INVALID_OPTIONS"], [null, "INVALID_ARGUMENT"]
  ];
  for (const [input, expected] of bad) assert.throws(() => session.provideAssets(input), code(expected));
  assert.equal(raw.calls.length, 0); assert.equal(raw.singleCalls, 0);
  assert.deepEqual(session.token, { revision: "1", layoutRevision: "1" });
});

test("shared, detached, wrong-kind and null byte containers are refused", () => {
  const detached = new Uint8Array(0); structuredClone(detached, { transfer: [detached.buffer] });
  for (const data of [detached, new Uint8Array(new SharedArrayBuffer(1)), new Uint16Array(2), new DataView(new ArrayBuffer(1)), null, [1]]) {
    assert.throws(() => normalizeAssetBatch([result("1", data)]), code("INVALID_ARGUMENT"));
  }
});

test("batch facade publishes one token, retains getters and never invokes single writes", () => {
  const { raw, session } = facade();
  assert(session.supportsAssetBatches); assert(Object.isFrozen(session));
  const input = new Uint8Array([1, 2]);
  const ack = session.provideAssets([result("2"), result("1", input)]);
  assert.deepEqual(ack, { revision: "1", layoutRevision: "2" });
  assert.equal(raw.calls.length, 1); assert.equal(raw.singleCalls, 0);
  assert.equal(session.source, "authoritative source"); assert.equal(input.byteLength, 2);
  input.fill(8); assert.deepEqual(session.assetBytes("1", "1"), new Uint8Array([1, 2]));
  assert.deepEqual(session.pendingAssets().requests.map(r => r.id), ["3"]);
  session.dispose(); assert(raw.freed); assert(session.disposed);
  assert.throws(() => session.provideAssets([]), code("SESSION_DISPOSED"));
});

test("empty batches are no-ops and legacy binaries refuse rather than emulate atomicity", () => {
  const { raw, session } = facade();
  assert.deepEqual(session.provideAssets([]), session.token); assert.equal(raw.calls.length, 0);
  const legacy = new NativeDouble(); legacy.provideAssetsPacked = undefined;
  const old = facade(legacy).session; assert.equal(old.supportsAssetBatches, false);
  for (const input of [[], [result()]]) assert.throws(() => old.provideAssets(input), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(legacy.singleCalls, 0); assert.equal(old.source, "authoritative source");
});

test("late native rejection preserves pending state and permits a retry", () => {
  const { raw, session } = facade(); raw.rejectBatch = true;
  assert.throws(() => session.provideAssets([result(), result("2")]), code("LAYOUT_ERROR"));
  assert.deepEqual(session.token, { revision: "1", layoutRevision: "1" });
  assert.equal(session.pendingAssets().total, 3);
  raw.rejectBatch = false; session.provideAssets([result(), result("2")]);
  assert.equal(session.layoutRevision, "2"); assert.equal(session.pendingAssets().total, 1);
});

test("reentrant source edits during normalization fence every result before ingress", () => {
  const { raw, session } = facade();
  const item = result();
  Object.defineProperty(item, "width", { enumerable: true, get() {
    session.replaceSource("new source", { expectedRevision: "1" }); return 20;
  } });
  assert.throws(() => session.provideAssets([item]), code("STALE_ASSET_GENERATION"));
  assert.equal(raw.calls.length, 0); assert.equal(session.source, "new source");
});

test("user iterators and arrays growing through getters cannot defeat the count bound", () => {
  const values = [result()]; let reads = 0;
  Object.defineProperty(values[0], "width", { enumerable: true, get() {
    reads++; values.push(result("2")); return 20;
  } });
  values[Symbol.iterator] = () => { throw new Error("must not call a user iterator"); };
  const normalized = normalizeAssetBatch(values);
  assert.equal(reads, 1); assert.equal(normalized.length, 1);
});

test("worker snapshots isolate every view only after normalization and preserve absent payloads", () => {
  const input = new Uint8Array([9, 1, 2, 8]);
  const args = normalizeFlowRequest("provideAssets", [[result("1", input.subarray(1, 3)), result("2"), result("3", new Uint8Array())]]);
  assert(requestWeight(args) >= 2);
  const saved = snapshotArguments("provideAssets", args);
  assert.equal(saved.transfer.length, 2);
  assert.notEqual(saved.args[0][0].bytes.buffer, input.buffer);
  input.fill(7); assert.deepEqual([...saved.args[0][0].bytes], [1, 2]);
  assert(!Object.hasOwn(saved.args[0][1], "bytes"));
  const delivered = structuredClone(saved.args, { transfer: saved.transfer });
  assert.deepEqual([...delivered[0][0].bytes], [1, 2]); assert.equal(input.byteLength, 4);
});
