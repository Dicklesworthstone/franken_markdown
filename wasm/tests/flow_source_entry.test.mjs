// Execute the real source entrypoint without any renderer/WASM artifact and
// with storage unavailable. Elements/window are explicit EventTarget doubles;
// File, Blob and object-URL lifetime are real Node implementations.
import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
const tick = () => new Promise(resolve => setImmediate(resolve));
let sequence = 0;
class Element extends EventTarget {
  #value = "";
  disabled = false; checked = false; hidden = false; textContent = ""; files = [];
  get value() { return this.#value; }
  set value(next) { this.#value = next.replace(/\r\n?/g, "\n"); }
  removeAttribute(name) { delete this[name]; }
  click() { const event = new Event("click", { cancelable: true }); this.dispatchEvent(event); return event; }
}
async function setup(t) {
  const names = ["source", "source-filename", "source-status", "open-markdown", "prepare-markdown", "source-download",
    "remember-draft", "refresh-draft", "restore-draft", "forget-draft", "save-draft", "draft-status"];
  const nodes = Object.fromEntries(names.map(name => [name, new Element()]));
  nodes.source.value = "# Existing source"; nodes["source-filename"].value = "document.md";
  const host = new EventTarget(); host.confirm = () => true;
  const previous = Object.fromEntries(["window", "document", "indexedDB"].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  Object.defineProperties(globalThis, {
    window: { configurable: true, value: host },
    document: { configurable: true, value: { querySelector: query => nodes[query.slice(1)] } },
    indexedDB: { configurable: true, value: null }
  });
  t.after(() => {
    host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: false }));
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key];
    }
  });
  await import(`../demo/flow-source.js?test=${++sequence}`); await tick();
  return { nodes, host };
}
test("source entry opens and downloads Markdown with no WASM and no IndexedDB", async t => {
  const { nodes } = await setup(t);
  assert(nodes["draft-status"].textContent.includes("STORAGE_UNAVAILABLE"));
  assert.equal(nodes["prepare-markdown"].disabled, false);
  nodes["prepare-markdown"].click();
  assert.equal(await (await fetch(nodes["source-download"].href)).text(), "# Existing source");
  const previous = nodes["source-download"].href, sequence = [];
  nodes.source.addEventListener("fmd-document-replaced", () => sequence.push("revoke"));
  nodes.source.addEventListener("input", () => sequence.push("input"));
  const input = new Promise(resolve => nodes.source.addEventListener("input", resolve, { once: true }));
  nodes["open-markdown"].files = [new File(["# New source"], "new.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change")); await input; await tick();
  assert.deepEqual(sequence, ["revoke", "input"]); assert.equal(nodes.source.value, "# New source");
  assert.equal(nodes["source-filename"].value, "new.md"); await assert.rejects(fetch(previous));
});
test("pagehide revokes downloads and bfcache reentry preserves untouched original file bytes", async t => {
  const { nodes, host } = await setup(t);
  const original = "\ufeff# Restored\r\nline\rend\n";
  const input = new Promise(resolve => nodes.source.addEventListener("input", resolve, { once: true }));
  nodes["open-markdown"].files = [new File([original], "mixed.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change")); await input; await tick();
  nodes["prepare-markdown"].click(); const old = nodes["source-download"].href;
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  assert.equal(nodes["source-download"].hidden, true); await assert.rejects(fetch(old));
  host.dispatchEvent(Object.assign(new Event("pageshow"), { persisted: true })); await tick();
  nodes["prepare-markdown"].click();
  const bytes = new Uint8Array(await (await fetch(nodes["source-download"].href)).arrayBuffer());
  assert.deepEqual(bytes, new TextEncoder().encode(original));
  assert.equal(nodes["remember-draft"].checked, false);
});
test("programmatic edit after prepare prevents stale source download activation", async t => {
  const { nodes } = await setup(t); nodes["prepare-markdown"].click(); const url = nodes["source-download"].href;
  nodes.source.value = "changed without input";
  assert.equal(nodes["source-download"].click().defaultPrevented, true);
  assert.equal(nodes["source-download"].hidden, true); await assert.rejects(fetch(url));
});
