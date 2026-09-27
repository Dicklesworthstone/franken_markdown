// Execute the real source entrypoint without any renderer/WASM artifact and
// with storage unavailable. Elements/window are explicit EventTarget doubles;
// File, Blob and object-URL lifetime are real Node implementations.

import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";

const tick = () => new Promise((resolve) => setImmediate(resolve));
let sequence = 0;
class Element extends EventTarget {
  #value = "";
  style = {};
  children = [];
  parentNode = null;
  setAttribute(name, value) { this[name] = value; }
  append(...children) { for (const child of children) { child.parentNode = this; this.children.push(child); } }
  insertBefore(child, next) {
    child.parentNode = this;
    const index = this.children.indexOf(next);
    if (index < 0) this.children.push(child); else this.children.splice(index, 0, child);
  }
  get nextSibling() { return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] ?? null; }
  remove() {
    if (this.parentNode) this.parentNode.children.splice(this.parentNode.children.indexOf(this), 1);
    this.parentNode = null;
  }
  setRangeText(text, start, end) { this.value = this.value.slice(0, start) + text + this.value.slice(end); }
  disabled = false;
  checked = false;
  hidden = false;
  textContent = "";
  files = [];
  selectionStart = 0;
  selectionEnd = 0;
  selectionDirection = "none";
  get value() {
    return this.#value;
  }
  set value(next) {
    this.#value = next.replace(/\r\n?/g, "\n");
    this.setSelectionRange(this.#value.length, this.#value.length);
  }
  setSelectionRange(start, end, direction = "none") {
    this.selectionStart = start;
    this.selectionEnd = end;
    this.selectionDirection = direction;
  }
  focus() {}
  select() {
    this.setSelectionRange(0, this.value.length);
  }
  removeAttribute(name) {
    delete this[name];
  }
  click() {
    const event = new Event("click", { cancelable: true });
    this.dispatchEvent(event);
    return event;
  }
}
async function setup(t) {
  const names = [
    "source",
    "source-filename",
    "source-status",
    "open-markdown",
    "prepare-markdown",
    "source-download",
    "remember-draft",
    "refresh-draft",
    "restore-draft",
    "forget-draft",
    "save-draft",
    "draft-status",
    "source-undo",
    "source-redo",
    "source-find",
    "source-find-insensitive",
    "source-find-previous",
    "source-find-next",
    "source-replacement",
    "source-replace",
    "source-replace-all",
    "source-edit-status",
  ];
  const nodes = Object.fromEntries(names.map((name) => [name, new Element()]));
  nodes.source.value = "# Existing source";
  nodes["source-filename"].value = "document.md";
  const host = new EventTarget();
  host.confirm = () => true;
  const previous = Object.fromEntries(
    ["window", "document", "indexedDB"].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  const doc = {
    querySelector: (query) => nodes[query.slice(1)],
    createElement() { const element = new Element(); element.ownerDocument = doc; return element; },
  };
  for (const [id, node] of Object.entries(nodes)) { node.ownerDocument = doc; node.id = id; }
  const parent = doc.createElement();
  parent.append(nodes.source);
  Object.defineProperties(globalThis, {
    window: { configurable: true, value: host },
    document: { configurable: true, value: doc },
    indexedDB: { configurable: true, value: null },
  });
  t.after(() => {
    host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: false }));
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  await import(`../demo/flow-source.js?test=${++sequence}`);
  await tick();
  return { nodes, host };
}
test("source entry opens and downloads Markdown with no WASM and no IndexedDB", async (t) => {
  const { nodes } = await setup(t);
  assert(nodes["draft-status"].textContent.includes("STORAGE_UNAVAILABLE"));
  assert.equal(nodes["prepare-markdown"].disabled, false);
  nodes["prepare-markdown"].click();
  assert.equal(await (await fetch(nodes["source-download"].href)).text(), "# Existing source");
  const previous = nodes["source-download"].href,
    sequence = [];
  nodes.source.addEventListener("fmd-document-replaced", () => sequence.push("revoke"));
  nodes.source.addEventListener("input", () => sequence.push("input"));
  const input = new Promise((resolve) =>
    nodes.source.addEventListener("input", resolve, { once: true }),
  );
  nodes["open-markdown"].files = [new File(["# New source"], "new.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change"));
  await input;
  await tick();
  assert.deepEqual(sequence, ["revoke", "input"]);
  assert.equal(nodes.source.value, "# New source");
  assert.equal(nodes["source-filename"].value, "new.md");
  await assert.rejects(fetch(previous));
});
test("pagehide revokes downloads and bfcache reentry preserves untouched original file bytes", async (t) => {
  const { nodes, host } = await setup(t);
  const original = "\ufeff# Restored\r\nline\rend\n";
  const input = new Promise((resolve) =>
    nodes.source.addEventListener("input", resolve, { once: true }),
  );
  nodes["open-markdown"].files = [new File([original], "mixed.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change"));
  await input;
  await tick();
  nodes["prepare-markdown"].click();
  const old = nodes["source-download"].href;
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  assert.equal(nodes["source-download"].hidden, true);
  await assert.rejects(fetch(old));
  host.dispatchEvent(Object.assign(new Event("pageshow"), { persisted: true }));
  await tick();
  nodes["prepare-markdown"].click();
  const bytes = new Uint8Array(await (await fetch(nodes["source-download"].href)).arrayBuffer());
  assert.deepEqual(bytes, new TextEncoder().encode(original));
  assert.equal(nodes["remember-draft"].checked, false);
});
test("programmatic edit after prepare prevents stale source download activation", async (t) => {
  const { nodes } = await setup(t);
  nodes["prepare-markdown"].click();
  const url = nodes["source-download"].href;
  nodes.source.value = "changed without input";
  assert.equal(nodes["source-download"].click().defaultPrevented, true);
  assert.equal(nodes["source-download"].hidden, true);
  await assert.rejects(fetch(url));
});
test("live entrypoint replacements and keyboard undo invalidate downloads without a renderer", async (t) => {
  const { nodes } = await setup(t);
  nodes["prepare-markdown"].click();
  const url = nodes["source-download"].href;
  nodes["source-find"].value = "source";
  nodes["source-find"].dispatchEvent(new Event("input"));
  nodes["source-replacement"].value = "Markdown";
  nodes["source-replace-all"].click();
  assert.equal(nodes.source.value, "# Existing Markdown");
  assert.equal(nodes["source-download"].hidden, true);
  await assert.rejects(fetch(url));
  assert.equal(nodes["source-undo"].disabled, false);
  const key = Object.assign(new Event("keydown", { cancelable: true }), {
    key: "z",
    ctrlKey: true,
  });
  nodes.source.dispatchEvent(key);
  assert.equal(key.defaultPrevented, true);
  assert.equal(nodes.source.value, "# Existing source");
  nodes["source-redo"].click();
  assert.equal(nodes.source.value, "# Existing Markdown");
  const input = new Promise((resolve) =>
    nodes.source.addEventListener("input", resolve, { once: true }),
  );
  nodes["open-markdown"].files = [new File(["# Different document"], "different.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change"));
  await input;
  await tick();
  nodes["source-undo"].click();
  assert.equal(nodes.source.value, "# Different document");
  assert.equal(nodes["source-undo"].disabled, true);
  assert.equal(nodes["source-redo"].disabled, true);
});
test("bfcache restarts editing once with a fresh history, not stale document transactions", async (t) => {
  const { nodes, host } = await setup(t);
  nodes.source.value = "first edit";
  nodes.source.dispatchEvent(new Event("input"));
  assert.equal(nodes["source-undo"].disabled, false);
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  const key = Object.assign(new Event("keydown", { cancelable: true }), {
    key: "z",
    ctrlKey: true,
  });
  nodes.source.dispatchEvent(key);
  assert.equal(key.defaultPrevented, false);
  host.dispatchEvent(Object.assign(new Event("pageshow"), { persisted: true }));
  await tick();
  assert.equal(nodes["source-undo"].disabled, true);
  nodes.source.value = "second edit";
  nodes.source.dispatchEvent(new Event("input"));
  nodes["source-undo"].click();
  assert.equal(nodes.source.value, "first edit");
  assert.equal(nodes["source-undo"].disabled, true);
  nodes["source-redo"].click();
  assert.equal(nodes.source.value, "second edit");
});

test("formatting enters existing history and revokes a prepared source download", async (t) => {
  const { nodes } = await setup(t);
  nodes["prepare-markdown"].click();
  const url = nodes["source-download"].href;
  nodes.source.setSelectionRange(11, 17, "backward");
  const key = Object.assign(new Event("keydown", { cancelable: true }), { key: "b", ctrlKey: true });
  nodes.source.dispatchEvent(key);
  assert(key.defaultPrevented);
  assert.equal(nodes.source.value, "# Existing **source**");
  assert(nodes["source-download"].hidden);
  await assert.rejects(fetch(url));
  nodes["source-undo"].click();
  assert.equal(nodes.source.value, "# Existing source");
  assert.equal(nodes.source.selectionStart, 11);
  assert.equal(nodes.source.selectionEnd, 17);
  assert.equal(nodes.source.selectionDirection, "backward");
  assert(nodes["source-undo"].disabled);
});
