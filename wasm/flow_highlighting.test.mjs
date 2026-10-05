import test from "node:test";
import assert from "node:assert/strict";
import { createFlowAdapter } from "./flow_session.mjs";

// Native ABI double only. Production admission, token fencing, mode reads and
// error normalization execute unchanged; these tests do not emulate Markdown.
function native() {
  return {
    revision: "9007199254740993",
    layoutRevision: "9007199254740995",
    source: "```rust\nlet value = 42;\n```",
    codeHighlighting: false,
    calls: [],
    failure: null,
    setCodeHighlighting(revision, layout, enabled) {
      this.calls.push([revision, layout, enabled]);
      if (this.failure) throw JSON.stringify({ code: this.failure, message: "native refusal" });
      assert.equal(revision, this.revision);
      assert.equal(layout, this.layoutRevision);
      if (enabled !== this.codeHighlighting) {
        const next = BigInt(this.layoutRevision) + 1n;
        if (next > 18446744073709551615n)
          throw JSON.stringify({ code: "REVISION_EXHAUSTED", message: "exhausted" });
        this.layoutRevision = next.toString();
        this.codeHighlighting = enabled;
      }
    },
    snapshotJson(revision, layoutRevision, offset) {
      return JSON.stringify({ schemaVersion: 1, revision, layoutRevision,
        offset, total: 0, items: [], nextOffset: null });
    },
    free() { this.freed = true; },
  };
}
const code = (expected) => (error) => error?.code === expected;

test("mode changes use precise native identities and only advance layout", () => {
  const raw = native(), api = createFlowAdapter(raw), source = api.source;
  assert.equal(api.supportsCodeHighlighting, true);
  assert.equal(api.codeHighlighting, false);
  const before = api.token;
  const next = api.setCodeHighlighting(true, {
    revision: BigInt(before.revision), layoutRevision: BigInt(before.layoutRevision),
  });
  assert.deepEqual(raw.calls, [[before.revision, before.layoutRevision, true]]);
  assert.deepEqual(next, { revision: before.revision, layoutRevision: "9007199254740996" });
  assert.equal(api.codeHighlighting, true);
  assert.equal(api.source, source);
  assert.deepEqual(api.setCodeHighlighting(true, next), next);
  api.setCodeHighlighting(false, api.token);
  assert.equal(api.codeHighlighting, false);
  assert.equal(api.layoutRevision, "9007199254740997");
});

test("old native packages remain readable but reject explicit mode requests", () => {
  const raw = native();
  delete raw.setCodeHighlighting;
  delete raw.codeHighlighting;
  const api = createFlowAdapter(raw);
  assert.equal(api.supportsCodeHighlighting, false);
  assert.equal(api.codeHighlighting, false);
  assert.equal(api.source, raw.source);
  for (const enabled of [false, true])
    assert.throws(() => api.setCodeHighlighting(enabled, api.token), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.deepEqual(raw.calls, []);
});

test("strict boolean and token admission precede every native mutation", () => {
  const raw = native(), api = createFlowAdapter(raw), token = api.token;
  for (const enabled of [0, 1, "true", null, undefined, {}, []])
    assert.throws(() => api.setCodeHighlighting(enabled, token), code("INVALID_ARGUMENT"));
  for (const revision of [1, "01", "-1", "18446744073709551616"])
    assert.throws(() => api.setCodeHighlighting(true, { ...token, revision }), code("INVALID_IDENTITY"));
  assert.throws(() => api.setCodeHighlighting(true, { ...token, revision: "1" }), code("STALE_REVISION"));
  assert.throws(() => api.setCodeHighlighting(false, { ...token, layoutRevision: "1" }), code("STALE_LAYOUT"));
  assert.equal(raw.calls.length, 0);
});

test("successful mode changes fence old snapshots and already-created iterators", () => {
  const raw = native(), api = createFlowAdapter(raw);
  const token = api.token, pages = api.pages();
  api.setCodeHighlighting(true, token);
  assert.throws(() => api.snapshot({ token }), code("STALE_LAYOUT"));
  assert.throws(() => pages.next(), code("STALE_LAYOUT"));
  assert.throws(() => api.setCodeHighlighting(true, token), code("STALE_LAYOUT"));
  assert.equal(api.snapshot({ token: api.token }).nextOffset, null);
});

test("native refusal publishes no speculative mode or token and allows retry", () => {
  const raw = native(), api = createFlowAdapter(raw), token = api.token;
  raw.failure = "BUDGET_EXCEEDED";
  assert.throws(() => api.setCodeHighlighting(true, token), code("BUDGET_EXCEEDED"));
  assert.equal(api.codeHighlighting, false);
  assert.deepEqual(api.token, token);
  raw.failure = null;
  api.setCodeHighlighting(true, token);
  raw.failure = "LAYOUT_ERROR";
  const enabledToken = api.token;
  assert.throws(() => api.setCodeHighlighting(false, enabledToken), code("LAYOUT_ERROR"));
  assert.equal(api.codeHighlighting, true);
  assert.deepEqual(api.token, enabledToken);
});

test("no-op at u64 maximum is distinct from a refused real transition", () => {
  const raw = native();
  raw.layoutRevision = "18446744073709551615";
  const api = createFlowAdapter(raw), token = api.token;
  assert.deepEqual(api.setCodeHighlighting(false, token), token);
  assert.throws(() => api.setCodeHighlighting(true, token), code("REVISION_EXHAUSTED"));
  assert.deepEqual(api.token, token);
  assert.equal(api.codeHighlighting, false);
});

test("inconsistent native acknowledgments are refused, never reported successful", () => {
  for (const corrupt of ["ignored", "source", "layout", "type"]) {
    const raw = native();
    raw.setCodeHighlighting = function (_r, _l, enabled) {
      if (corrupt !== "ignored") this.codeHighlighting = enabled;
      this.layoutRevision = (BigInt(this.layoutRevision) + (corrupt === "layout" ? 2n : 1n)).toString();
      if (corrupt === "source") this.revision = (BigInt(this.revision) + 1n).toString();
      if (corrupt === "type") this.codeHighlighting = "true";
    };
    const api = createFlowAdapter(raw);
    assert.throws(() => api.setCodeHighlighting(true, api.token), code("INVALID_WASM_RESPONSE"));
  }
});

test("disposal guards mode reads and mutations before touching freed native state", () => {
  const raw = native(), api = createFlowAdapter(raw), token = api.token;
  api.dispose();
  Object.defineProperty(raw, "codeHighlighting", { get() { assert.fail("freed native getter"); } });
  assert.equal(api.disposed, true);
  assert.throws(() => api.supportsCodeHighlighting, code("SESSION_DISPOSED"));
  assert.throws(() => api.codeHighlighting, code("SESSION_DISPOSED"));
  assert.throws(() => api.setCodeHighlighting(true, token), code("SESSION_DISPOSED"));
  api.dispose();
  assert.equal(raw.freed, true);
});
