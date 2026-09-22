import assert from "node:assert/strict";
import test from "node:test";
import { createFlowAdapter, FLOW_EDIT_LIMIT, FLOW_SOURCE_LIMIT, FlowError } from "./flow_session.mjs";

// Protocol double only. Rust edit_batch tests exercise real shaping and rollback;
// these tests do not pretend to compile or run the WASM renderer.
function transport() {
  return {
    revision: "9007199254740993",
    layoutRevision: "7",
    source: "original",
    calls: [],
    editManyUtf16Packed(...args) {
      this.calls.push(args);
      if (this.reject) throw this.reject;
      if (args[1].length !== 0) {
        this.revision = String(BigInt(this.revision) + 1n);
        this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
      }
    },
    free() {},
  };
}
const code = (expected) => (error) => error instanceof FlowError && error.code === expected;
const edit = (start, end, replacement) => ({ start, end, replacement });

test("one native transaction preserves original UTF-16 ranges and UTF-8 replacement lengths", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  assert.equal(api.supportsEditBatches, true);
  const edits = Object.freeze([Object.freeze(edit(8, 9, "é")), Object.freeze(edit(1, 3, "😀")), edit(5, 5, "\0")]);
  const result = api.editMany(edits, { expectedRevision: 9007199254740993n });
  assert.equal(raw.calls.length, 1);
  const [revision, ranges, lengths, text, reuse] = raw.calls[0];
  assert.equal(revision, "9007199254740993");
  assert.ok(ranges instanceof Uint32Array);
  assert.ok(lengths instanceof Uint32Array);
  assert.deepEqual([...ranges], [8, 9, 1, 3, 5, 5]);
  assert.deepEqual([...lengths], [2, 4, 1]);
  assert.equal(text, "é😀\0");
  assert.equal(reuse, false);
  assert.deepEqual(result, { revision: "9007199254740994", layoutRevision: "8" });
  assert.ok(Object.isFrozen(result));
});

test("simultaneous inserts retain order and explicit resource authorization", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  api.editMany([edit(1, 1, "second"), edit(1, 1, "first")], {
    expectedRevision: api.revision, reuseAssets: true,
  });
  assert.deepEqual([...raw.calls[0][1]], [1, 1, 1, 1]);
  assert.deepEqual([...raw.calls[0][2]], [6, 5]);
  assert.equal(raw.calls[0][3], "secondfirst");
  assert.equal(raw.calls[0][4], true);
});

test("a late invalid operation rejects the whole batch before native ingress", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  const options = { expectedRevision: api.revision };
  for (const invalid of [
    edit(-1, 2, "x"), edit(0.5, 2, "x"), edit("0", 1, "x"),
    edit(0, 2 ** 32, "x"), edit(3, 1, "x"), edit(0, 0, null),
    { ...edit(0, 0, "x"), unknown: true }, undefined,
  ]) {
    assert.throws(() => api.editMany([edit(0, 0, "valid"), invalid], options), FlowError);
  }
  for (const invalid of [null, {}, "edits", new Uint32Array(2)])
    assert.throws(() => api.editMany(invalid, options), code("INVALID_ARGUMENT"));
  assert.equal(raw.calls.length, 0);
  assert.equal(api.source, "original");
});

test("unpaired surrogates cannot be repaired by joining separate replacements", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  assert.throws(() => api.editMany([edit(0, 0, "\ud83d"), edit(0, 0, "\ude00")], {
    expectedRevision: api.revision,
  }), code("INVALID_UNICODE"));
  assert.equal(raw.calls.length, 0);
});

test("operation count and aggregate replacement bytes are admitted before native ingress", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  const options = { expectedRevision: api.revision };
  const tooMany = new Array(FLOW_EDIT_LIMIT + 1);
  Object.defineProperty(tooMany, 0, { get() { throw Error("count must precede element reads"); } });
  assert.throws(() => api.editMany(tooMany, options), code("BUDGET_EXCEEDED"));
  const half = "é".repeat(FLOW_SOURCE_LIMIT / 4);
  assert.throws(() => api.editMany([edit(0, 0, half), edit(0, 0, half + "x")], options), code("BUDGET_EXCEEDED"));
  assert.equal(raw.calls.length, 0);
  api.editMany(Array.from({ length: FLOW_EDIT_LIMIT }, () => edit(0, 0, "")), options);
  assert.equal(raw.calls.length, 1);
  assert.equal(raw.calls[0][2].length, FLOW_EDIT_LIMIT);
});

test("stale and invalid identities never inspect batch contents", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  const unreadable = [edit(0, 0, "")];
  Object.defineProperty(unreadable, 0, { get() { throw Error("stale input must not be read"); } });
  assert.throws(() => api.editMany(unreadable, { expectedRevision: "1" }), code("STALE_REVISION"));
  assert.throws(() => api.editMany(unreadable, { expectedRevision: Number(api.revision) }), code("INVALID_IDENTITY"));
  assert.throws(() => api.editMany([], { expectedRevision: api.revision, reuseAssets: "yes" }), code("INVALID_ARGUMENT"));
  assert.equal(raw.calls.length, 0);
});

test("native batch diagnostics keep their code and leave the acknowledged token intact", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  const before = api.token;
  raw.reject = '{"code":"OVERLAPPING_EDITS","message":"edits 0 and 1 overlap"}';
  assert.throws(() => api.editMany([edit(0, 4, "x"), edit(2, 5, "y")], {
    expectedRevision: before.revision,
  }), (error) => code("OVERLAPPING_EDITS")(error) && error.cause === raw.reject);
  assert.deepEqual(api.token, before);
  assert.equal(api.source, "original");
  assert.equal(raw.calls.length, 1);
});

test("empty batches use the atomic entry point without speculative revision increments", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  const before = api.token;
  assert.deepEqual(api.editMany([], { expectedRevision: before.revision }), before);
  assert.equal(raw.calls.length, 1);
  assert.deepEqual([...raw.calls[0][1]], []);
  assert.deepEqual([...raw.calls[0][2]], []);
  assert.equal(raw.calls[0][3], "");
});

test("legacy binaries fail explicitly rather than falling back to partial single edits", () => {
  const raw = transport();
  raw.editManyUtf16Packed = undefined;
  raw.editUtf16 = () => assert.fail("no sequential fallback");
  raw.replaceSource = () => assert.fail("no alternative write fallback");
  const api = createFlowAdapter(raw);
  assert.equal(api.supportsEditBatches, false);
  assert.throws(() => api.editMany([edit(0, 0, "x")], { expectedRevision: api.revision }),
    code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(raw.calls.length, 0);
});

test("disposed sessions reject batch writes before inspecting options", () => {
  const raw = transport();
  const api = createFlowAdapter(raw);
  api.dispose();
  assert.throws(() => api.editMany(null, null), code("SESSION_DISPOSED"));
  assert.equal(raw.calls.length, 0);
});
