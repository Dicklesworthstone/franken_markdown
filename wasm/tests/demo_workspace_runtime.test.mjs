import assert from "node:assert/strict";
import test from "node:test";
import { loadPackagedWorkspaceRuntime, DEMO_WORKSPACE_RUNTIME_LIMITS as limits } from "../demo/demo_workspace_runtime.mjs";

const magic = Uint8Array.from([0, 97, 115, 109, 1, 0, 0, 0]);
const bindings = new TextEncoder().encode("export default async function init() {} // café😀");
function response(chunks, options = {}) {
  let index = 0;
  const state = { reads: 0, cancelled: 0, released: 0 };
  return { state, ok: true, redirected: false, url: "", headers: { get: () => null },
    body: { cancel() { state.cancelled++; }, getReader() { return {
      async read() { state.reads++; return index < chunks.length ? { value: chunks[index++], done: false } : { done: true }; },
      cancel() { state.cancelled++; }, releaseLock() { state.released++; },
    }; } }, ...options };
}
function fetcher(wasm = response([magic]), js = response([bindings])) {
  const calls = [];
  const fetch = async (url, options) => { calls.push({ url: url.href, options }); return calls.length === 1 ? wasm : js; };
  return { fetch, calls, wasm, js };
}

test("runtime loading fetches only the fixed packaged pair and preserves exact streamed bytes", async () => {
  const wasm = response([magic.subarray(0, 3), magic.subarray(3)]), f = fetcher(wasm, response([bindings]));
  const result = await loadPackagedWorkspaceRuntime(f.fetch);
  assert.deepEqual(result.wasm, magic);
  assert.equal(result.bindings, new TextDecoder().decode(bindings));
  assert.deepEqual(f.calls.map(call => new URL(call.url).pathname.split("/").slice(-2).join("/")),
    ["pkg/franken_markdown_bg.wasm", "pkg/franken_markdown.js"]);
  for (const call of f.calls) assert.deepEqual(call.options, { redirect: "error", mode: "same-origin", credentials: "same-origin", cache: "no-cache" });
  assert.equal(wasm.state.released, 1);
  result.wasm.fill(0);
  assert.equal(magic[1], 97, "returned runtime owns its bytes");
});

for (const [label, options] of [
  ["HTTP failure", { ok: false }], ["redirect", { redirected: true }],
  ["changed runtime URL", { url: "https://elsewhere.invalid/runtime.wasm" }],
]) test(`${label} refuses before consuming the runtime body or fetching bindings`, async () => {
  const f = fetcher(response([magic], options));
  await assert.rejects(loadPackagedWorkspaceRuntime(f.fetch), { code: "WORKSPACE_RUNTIME_LOAD_FAILED" });
  assert.equal(f.calls.length, 1); assert.equal(f.wasm.state.reads, 0);
});

test("declared oversized runtime is rejected before any stream read", async () => {
  const f = fetcher(response([magic], { headers: { get: () => String(limits.wasmBytes + 1) } }));
  await assert.rejects(loadPackagedWorkspaceRuntime(f.fetch), { code: "WORKSPACE_RUNTIME_LIMIT" });
  assert.equal(f.wasm.state.reads, 0); assert.equal(f.wasm.state.cancelled, 1); assert.equal(f.calls.length, 1);
});

for (const declared of [null, "1"]) test(`actual stream cap wins over ${declared === null ? "missing" : "underreported"} content length`, async () => {
  const js = response([new Uint8Array(limits.bindingsBytes + 1)], { headers: { get: () => declared } });
  await assert.rejects(loadPackagedWorkspaceRuntime(fetcher(undefined, js).fetch), { code: "WORKSPACE_RUNTIME_LIMIT" });
  assert.equal(js.state.reads, 1); assert.equal(js.state.cancelled, 1); assert.equal(js.state.released, 1);
});

test("stream chunks are copied before the next read can mutate their storage", async () => {
  const first = magic.slice(), second = Uint8Array.of(0);
  let reads = 0;
  const wasm = response([]);
  wasm.body.getReader = () => ({ async read() {
    if (reads++ === 0) return { done: false, value: first };
    if (reads === 2) { first.fill(9); return { done: false, value: second }; }
    return { done: true };
  }, cancel() {}, releaseLock() {} });
  const result = await loadPackagedWorkspaceRuntime(fetcher(wasm).fetch);
  assert.deepEqual(result.wasm, Uint8Array.from([...magic, 0]));
});

test("many tiny chunks use the same bounded streaming path", async () => {
  const binding = "export default function init() {} // " + "x".repeat(10000);
  const js = response([...new TextEncoder().encode(binding)].map(value => Uint8Array.of(value)));
  const result = await loadPackagedWorkspaceRuntime(fetcher(undefined, js).fetch);
  assert.equal(result.bindings, binding); assert.equal(js.state.cancelled, 0);
});

for (const [label, wasm, js, code] of [
  ["empty runtime", response([]), undefined, "WORKSPACE_RUNTIME_INVALID"],
  ["invalid WASM magic", response([Uint8Array.of(1, 2, 3)]), undefined, "WORKSPACE_RUNTIME_INVALID"],
  ["nonstreaming response", response([], { body: null }), undefined, "WORKSPACE_RUNTIME_UNAVAILABLE"],
  ["nonbyte stream", response(["not bytes"]), undefined, "WORKSPACE_RUNTIME_INVALID"],
  ["invalid UTF-8 bindings", undefined, response([Uint8Array.of(0xc0, 0xaf)]), "WORKSPACE_RUNTIME_INVALID"],
  ["blank bindings", undefined, response([Uint8Array.of(32, 10)]), "WORKSPACE_RUNTIME_INVALID"],
]) test(`${label} is a visible refusal without an unbounded fallback`, async () => {
  await assert.rejects(loadPackagedWorkspaceRuntime(fetcher(wasm, js).fetch), { code });
});

test("missing fetch and network errors produce actionable package errors", async () => {
  await assert.rejects(loadPackagedWorkspaceRuntime(null), { code: "WORKSPACE_RUNTIME_UNAVAILABLE" });
  await assert.rejects(loadPackagedWorkspaceRuntime(async () => { throw new TypeError("offline"); }),
    error => error.code === "WORKSPACE_RUNTIME_LOAD_FAILED" && /complete matching WASM package/.test(error.message));
});
