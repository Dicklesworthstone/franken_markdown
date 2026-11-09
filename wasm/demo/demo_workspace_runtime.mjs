// Only the trusted runtime shipped beside this demo can become executable
// workspace code. Document contents and worker requests never supply a URL.
export const DEMO_WORKSPACE_RUNTIME_LIMITS = Object.freeze({
  wasmBytes: 64 * 1024 * 1024,
  bindingsBytes: 4 * 1024 * 1024,
});
const fail = (code, message) => { throw Object.assign(new Error(message), { code }); };

async function readRuntime(fetchRuntime, name, maximum) {
  const url = new URL(`../pkg/${name}`, import.meta.url);
  let response;
  try {
    response = await fetchRuntime(url, { redirect: "error", mode: "same-origin", credentials: "same-origin", cache: "no-cache" });
  } catch {
    fail("WORKSPACE_RUNTIME_LOAD_FAILED", `Could not load the packaged ${name}. Serve the complete matching WASM package and try again.`);
  }
  if (!response?.ok || response.redirected || (response.url && response.url !== url.href))
    fail("WORKSPACE_RUNTIME_LOAD_FAILED", `The packaged ${name} is unavailable; redirects and replacement runtime locations are not allowed.`);
  const length = response.headers?.get("content-length");
  if (length !== null && length !== undefined && /^\d+$/.test(length) && (!Number.isSafeInteger(Number(length)) || Number(length) > maximum)) {
    try { await response.body?.cancel(); } catch { /* Refusal does not depend on server cleanup. */ }
    fail("WORKSPACE_RUNTIME_LIMIT", `The packaged ${name} exceeds its byte limit.`);
  }
  // arrayBuffer()/text() would allocate the entire response before admission.
  // Require streaming, count actual decoded bytes, then retain bounded copies.
  if (typeof response.body?.getReader !== "function")
    fail("WORKSPACE_RUNTIME_UNAVAILABLE", "This browser cannot stream the packaged workspace runtime within its byte limits.");
  const reader = response.body.getReader();
  let retained = new Uint8Array(0);
  let total = 0, complete = false;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      let bytes;
      try {
        if (!(value instanceof Uint8Array)) throw new TypeError();
        Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(value.buffer);
        bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
      } catch { fail("WORKSPACE_RUNTIME_INVALID", `The packaged ${name} returned invalid byte chunks.`); }
      if (bytes.byteLength > maximum - total)
        fail("WORKSPACE_RUNTIME_LIMIT", `The packaged ${name} exceeds its byte limit.`);
      // A byte cap alone would not bound an array retaining millions of tiny
      // chunks. Keep one growing byte buffer, with capacity itself capped.
      const next = total + bytes.byteLength;
      if (next > retained.length) {
        const grown = new Uint8Array(Math.min(maximum, Math.max(next, 65536, retained.length * 2)));
        grown.set(retained.subarray(0, total));
        retained = grown;
      }
      retained.set(bytes, total);
      total = next;
    }
    if (!total) fail("WORKSPACE_RUNTIME_INVALID", `The packaged ${name} is empty.`);
    complete = true;
    return retained.length === total ? retained : retained.slice(0, total);
  } finally {
    if (!complete) { try { await reader.cancel(); } catch { /* Preserve the admission error. */ } }
    reader.releaseLock();
  }
}

/** Host/test fetch seam only; there is deliberately no caller-selected URL. */
export async function loadPackagedWorkspaceRuntime(fetchRuntime = globalThis.fetch) {
  if (typeof fetchRuntime !== "function")
    fail("WORKSPACE_RUNTIME_UNAVAILABLE", "Fetching the packaged workspace runtime is unavailable in this browser.");
  const wasm = await readRuntime(fetchRuntime, "franken_markdown_bg.wasm", DEMO_WORKSPACE_RUNTIME_LIMITS.wasmBytes);
  if (wasm.length < 8 || ![0, 97, 115, 109, 1, 0, 0, 0].every((byte, index) => wasm[index] === byte))
    fail("WORKSPACE_RUNTIME_INVALID", "The packaged workspace engine is not a WebAssembly version-1 binary.");
  const bytes = await readRuntime(fetchRuntime, "franken_markdown.js", DEMO_WORKSPACE_RUNTIME_LIMITS.bindingsBytes);
  let bindings;
  try { bindings = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
  catch { fail("WORKSPACE_RUNTIME_INVALID", "The packaged workspace bindings are not valid UTF-8."); }
  if (!bindings.trim()) fail("WORKSPACE_RUNTIME_INVALID", "The packaged workspace bindings are empty.");
  return { wasm, bindings };
}
