// Production planner, formatting/history controllers, collection and book host.
// DOM events/focus and the export worker are explicit adapters; no WASM rendering.
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookSourceHistoryControls } from "../demo/book_source_history.mjs";
import { createBookSourceFormattingControls } from "../demo/book_source_formatting.mjs";

class InputEvent extends Event {
  constructor(type, options) { super(type, options); this.inputType = options.inputType; this.data = options.data; }
}
class Element {
  constructor(tag, doc) { this.tagName = tag; this.ownerDocument = doc; }
  children = []; parentNode = null; style = {}; attributes = {}; listeners = new Map();
  value = ""; id = ""; textContent = ""; disabled = false; readOnly = false; checked = false;
  selectionStart = 0; selectionEnd = 0; selectionDirection = "none"; scrollTop = 0;
  get parentElement() { return this.parentNode; }
  get nextSibling() { return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1]; }
  append(...nodes) { for (const node of nodes) { node.parentNode = this; this.children.push(node); } }
  appendChild(node) { this.append(node); return node; }
  insertBefore(node, next) {
    node.parentNode = this; const index = this.children.indexOf(next);
    this.children.splice(index < 0 ? this.children.length : index, 0, node);
  }
  replaceChildren(...nodes) { this.children = []; this.append(...nodes); }
  remove() { this.parentNode?.children.splice(this.parentNode.children.indexOf(this), 1); this.parentNode = null; }
  setAttribute(name, value) { this.attributes[name] = value; }
  removeAttribute(name) { delete this[name]; delete this.attributes[name]; }
  querySelector(selector) {
    if (selector === `#${this.id}`) return this;
    for (const node of this.children) { const found = node.querySelector(selector); if (found) return found; }
    return null;
  }
  addEventListener(type, fn, options) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push({ fn, capture: options === true, once: options?.once }); this.listeners.set(type, listeners);
  }
  removeEventListener(type, fn) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter(item => item.fn !== fn)); }
  dispatchEvent(event) {
    Object.defineProperty(event, "target", { configurable: true, value: this });
    const listeners = [...(this.listeners.get(event.type) ?? [])];
    for (const capture of [true, false]) for (const item of listeners) if (item.capture === capture) {
      if (item.once) this.removeEventListener(event.type, item.fn); item.fn(event);
    }
    return !event.defaultPrevented;
  }
  emit(type, values = {}) { const event = new Event(type, { cancelable: true }); Object.assign(event, values); this.dispatchEvent(event); return event; }
  focus() { this.emit("focus"); }
  setSelectionRange(start, end, direction = "none") { this.selectionStart = start; this.selectionEnd = end; this.selectionDirection = direction; }
  setRangeText(text, start, end) { this.value = this.value.slice(0, start) + text + this.value.slice(end); }
}

function setup(t, files = [{ path: "one.md", source: "café😀" }, { path: "shared.txt", source: "one\ntwo", role: "include" }]) {
  const root = new Element("document"); root.defaultView = { Event, InputEvent };
  root.createElement = tag => new Element(tag, root);
  const parent = root.appendChild(root.createElement("section"));
  const html = readFileSync(new URL("../demo/book.html", import.meta.url), "utf8");
  for (const [, tag, id] of html.matchAll(/<(\w+)[^>]*\bid="([^"]+)"/g)) {
    const node = parent.appendChild(root.createElement(tag)); node.id = id;
  }
  const el = id => root.querySelector(`#${id}`), collection = createBookCollection();
  collection.append({ chapters: files.filter(file => file.role !== "include"), includeSources: files.filter(file => file.role === "include"),
    images: [{ destination: "figure.png", bytes: new Uint8Array([1, 2, 3]) }] });
  collection.setFonts([{ slot: "body-regular", name: "Explicit byte-storage fixture", bytes: new Uint8Array([4, 5]) }], collection.revision);
  let renders = 0;
  const worker = { cancel() {}, dispose() {}, render: async () => {
    renders++; return { extension: "pdf", blob: () => new Blob(["explicit export-worker fixture"]) };
  } };
  const controls = createBookControls({ root, collection, worker });
  const history = createBookSourceHistoryControls({ root, collection, controls });
  const formatting = createBookSourceFormattingControls({ root, collection, controls });
  t.after(() => { formatting.dispose(); history.dispose(); controls.dispose(); });
  const editor = el("chapter-source");
  editor.setSelectionRange(0, editor.value.length, "backward");
  return { root, el, collection, controls, history, formatting, editor,
    get renders() { return renders; },
    select(index) { el("chapters").value = String(index); el("chapters").emit("change"); },
    buttons() { return parent.children.find(node => Object.hasOwn(node.attributes, "data-source-formatting"))?.children.filter(node => node.tagName === "button") ?? []; },
  };
}

test("formatting is one ordinary undoable source edit and revokes a prepared download", async t => {
  const h = setup(t), before = h.collection.snapshot(), settings = h.collection.options, revision = h.collection.revision;
  await h.controls.prepare("pdf"); const href = h.el("download").href;
  let inputs = 0, beforeinputs = 0;
  h.editor.addEventListener("input", event => { inputs++; assert.equal(event.inputType, "insertReplacementText"); });
  h.editor.addEventListener("beforeinput", () => beforeinputs++);
  assert.equal(h.formatting.execute("bold"), true);
  assert.equal(h.editor.value, "**café😀**"); assert.equal(h.collection.files[0].source, h.editor.value);
  assert.equal(h.collection.revision, revision + 1); assert.equal(inputs, 1); assert.equal(beforeinputs, 1);
  assert.equal(h.history.state.undo, 1); assert(h.el("download").hidden); assert.notEqual(h.el("download").href, href);
  const after = h.collection.snapshot();
  assert.deepEqual(after.options.images, before.options.images); assert.deepEqual(after.options.fontAssets, before.options.fontAssets);
  assert.deepEqual(h.collection.options, settings);
  assert.equal(h.renders, 1);
  h.history.undo(); assert.equal(h.editor.value, "café😀");
  h.history.redo(); assert.equal(h.editor.value, "**café😀**"); assert.equal(h.renders, 1);
});

test("include-only source receives a complete multiline command and one undo", t => {
  const h = setup(t); h.select(1); h.editor.setSelectionRange(0, h.editor.value.length);
  h.buttons().find(button => button.attributes["data-source-command"] === "task-list").emit("click");
  assert.equal(h.collection.files[1].source, "- [ ] one\n- [ ] two"); assert.equal(h.collection.files[1].role, "include");
  assert.equal(h.collection.files[0].source, "café😀"); assert.equal(h.history.state.undo, 1);
  h.history.undo(); assert.equal(h.editor.value, "one\ntwo");
});

test("source shortcuts use the same history and ignore composition", t => {
  const h = setup(t), event = h.editor.emit("keydown", { key: "b", ctrlKey: true });
  assert(event.defaultPrevented); assert.equal(h.editor.value, "**café😀**"); assert.equal(h.history.state.undo, 1);
  h.editor.emit("compositionstart"); assert(h.buttons().every(button => button.disabled));
  assert.throws(() => h.formatting.execute("italic"), { code: "EDITOR_BUSY" });
  assert.equal(h.editor.emit("keydown", { key: "i", metaKey: true, isComposing: true }).defaultPrevented, false);
  h.editor.emit("compositionend"); assert(h.buttons().every(button => !button.disabled));
});

test("cancelled beforeinput creates no source revision or phantom undo", t => {
  const h = setup(t), revision = h.collection.revision;
  h.editor.addEventListener("beforeinput", event => event.preventDefault(), { once: true });
  assert.equal(h.formatting.execute("bold"), false); assert.equal(h.collection.revision, revision);
  assert.equal(h.history.state.undo, 0); assert.equal(h.editor.value, "café😀");
});

for (const phase of ["focus", "beforeinput"]) {
  test(`${phase} cannot apply a plan to a different byte-identical chapter`, t => {
    const h = setup(t, [{ path: "a.md", source: "same" }, { path: "b.md", source: "same" }]);
    h.editor.addEventListener(phase, () => { h.select(1); h.editor.setSelectionRange(0, 4, "backward"); }, { once: true });
    assert.throws(() => h.formatting.execute("bold"), { code: "STALE_SOURCE" });
    assert.deepEqual(h.collection.files.map(file => file.source), ["same", "same"]); assert.equal(h.history.state.undo, 0);
  });
  test(`${phase} cannot cross a byte-identical project replacement`, t => {
    const h = setup(t), project = h.collection.project();
    h.editor.addEventListener(phase, () => {
      h.controls.replaceProject(project, h.controls.checkpoint());
      h.editor.setSelectionRange(0, h.editor.value.length, "backward");
    }, { once: true });
    assert.throws(() => h.formatting.execute("bold"), { code: "STALE_SOURCE" });
    assert.equal(h.editor.value, "café😀"); assert.equal(h.history.state.undo, 0);
  });
}

test("pending import blocks formatting and re-enables it after admission", async t => {
  const h = setup(t), file = new File(["new source"], "next.md"), bytes = await file.arrayBuffer();
  let release; file.arrayBuffer = () => new Promise(resolve => { release = () => resolve(bytes); });
  const pending = h.controls.importFiles([file]); assert(h.controls.sourceBusy);
  assert(h.buttons().every(button => button.disabled));
  assert.throws(() => h.formatting.execute("bold"), { code: "EDITOR_BUSY" });
  release(); await pending; assert(h.buttons().every(button => !button.disabled));
});

test("empty book, readonly and path composition reject formatting", t => {
  const h = setup(t); h.editor.readOnly = true;
  assert.throws(() => h.formatting.execute("bold"), { code: "EDITOR_BUSY" }); h.editor.readOnly = false;
  h.el("chapter-path").emit("compositionstart"); assert(h.buttons().every(button => button.disabled));
  assert.throws(() => h.formatting.execute("bold"), { code: "EDITOR_BUSY" }); h.el("chapter-path").emit("compositionend");
  h.controls.applySourceTree([], h.controls.checkpoint()); assert(h.buttons().every(button => button.disabled));
  assert.throws(() => h.formatting.execute("bold"), { code: "EDITOR_BUSY" });
});

test("suspension and repeated resume leave exactly one active toolbar", t => {
  const h = setup(t), originalButtons = h.buttons(); assert.equal(originalButtons.length, 12);
  h.formatting.suspend(); assert.equal(h.buttons().length, 0);
  assert.throws(() => h.formatting.execute("bold"), { code: "SESSION_DISPOSED" });
  h.formatting.resume(); h.formatting.resume(); assert.equal(h.buttons().length, 12);
  originalButtons[0].emit("click"); assert.equal(h.editor.value, "café😀");
  h.buttons()[0].emit("click"); assert.equal(h.editor.value, "**café😀**"); assert.equal(h.history.state.undo, 1);
  h.formatting.dispose(); h.formatting.resume(); assert.equal(h.buttons().length, 0);
  const revision = h.collection.revision; h.editor.emit("keydown", { key: "i", ctrlKey: true }); assert.equal(h.collection.revision, revision);
});
