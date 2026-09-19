import test from "node:test";
import assert from "node:assert/strict";
import { createFileControls } from "../demo/flow_file_controls.mjs";
import { createSourceControls, createSourceEditingControls } from "../demo/flow_document.mjs";
import { MemoryFileHandle, deferred } from "./flow_file_fixtures.mjs";

// Deliberate DOM double: native EventTarget, File, Blob and object URLs; no
// browser focus, transient activation, beforeunload UI or OS acceptance claim.
class Element extends EventTarget {
  constructor(tag, document) {
    super(); this.tagName = tag; this.ownerDocument = document; this.children = []; this.style = {};
    this.attributes = {}; this._value = ""; this._text = ""; this.disabled = this.readOnly = false;
    this.selectionStart = this.selectionEnd = 0; this.selectionDirection = "none";
  }
  set value(value) { this._value = this.tagName === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
  get value() { return this._value; }
  set textContent(value) { this._text = String(value); this.children = []; }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(""); }
  append(...children) { for (const child of children) { child.parentNode = this; this.children.push(child); } }
  insertBefore(node, reference) {
    node.parentNode = this; const index = this.children.indexOf(reference);
    if (index < 0) this.children.push(node); else this.children.splice(index, 0, node);
  }
  get nextSibling() { return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] ?? null; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  removeAttribute(name) { delete this.attributes[name]; delete this[name]; }
  querySelector(selector) {
    if (selector === `#${this.id}`) return this;
    for (const child of this.children) { const found = child.querySelector(selector); if (found) return found; }
    return null;
  }
  focus() { this.ownerDocument.activeElement = this; }
  setSelectionRange(start, end, direction = "none") { this.selectionStart = start; this.selectionEnd = end; this.selectionDirection = direction; }
  closest() { return null; }
}
class Document extends Element {
  constructor() { super("document"); this.ownerDocument = this; }
  createElement(tag) { return new Element(tag, this); }
}
function setup({ supported = true } = {}) {
  const root = new Document(), window = new EventTarget(), urls = new Set();
  const add = (id, tag = "button") => { const el = root.createElement(tag); el.id = id; root.append(el); return el; };
  const source = add("source", "textarea"), filename = add("source-filename", "input");
  source.value = "# Initial\n"; filename.value = "draft.md";
  const open = add("open-markdown", "input"), prepare = add("prepare-markdown"), download = add("source-download", "a"), status = add("source-status", "p");
  let images = true, replacements = 0, confirmation = true, picked = new MemoryFileHandle(), saved = new MemoryFileHandle("new.md", "");
  let pickerCalls = 0, savePickerCalls = 0;
  Object.assign(window, { isSecureContext: supported, confirm: () => confirmation });
  if (supported) Object.assign(window, {
    showOpenFilePicker(options) { pickerCalls++; assert.equal(options.multiple, false); return Promise.resolve([picked]); },
    showSaveFilePicker(options) { savePickerCalls++; assert.ok(options.suggestedName); return Promise.resolve(saved); }
  });
  const controls = createSourceControls({ sourceEditor: source, filename, open, prepare, download, status,
    urls: { createObjectURL(blob) { const url = URL.createObjectURL(blob); urls.add(url); return url; },
      revokeObjectURL(url) { urls.delete(url); URL.revokeObjectURL(url); } },
    onReplace() { images = false; replacements++; source.dispatchEvent(new Event("fmd-document-replaced")); }
  });
  const editing = createSourceEditingControls({ sourceEditor: source,
    undo: add("source-undo"), redo: add("source-redo"), query: add("source-find", "input"),
    ignoreCase: add("source-find-insensitive", "input"), previous: add("source-find-previous"), next: add("source-find-next"),
    replacement: add("source-replacement", "input"), replace: add("source-replace"), replaceAll: add("source-replace-all"), status: add("source-edit-status", "p") });
  const make = () => createFileControls({ root, window, controls, sourceEditor: source, filename });
  let files = make();
  return { root, window, source, filename, controls, editing, urls, download,
    get files() { return files; }, get picked() { return picked; }, get saved() { return saved; },
    get images() { return images; }, get replacements() { return replacements; }, get pickerCalls() { return pickerCalls; }, get savePickerCalls() { return savePickerCalls; },
    pick(value) { picked = value; }, target(value) { saved = value; }, confirm(value) { confirmation = value; },
    edit(value) { source.value = value; source.setSelectionRange(value.length, value.length); source.dispatchEvent(new Event("input")); },
    suspend() { files.dispose(); }, resume() { files = make(); },
    el(id) { return root.querySelector(`#${id}`); },
    dispose() { files.dispose(); editing.dispose(); controls.dispose(); for (const url of urls) URL.revokeObjectURL(url); }
  };
}
async function settled(h) { for (let i = 0; i < 100 && h.files.state.busy; i++) await new Promise(resolve => setImmediate(resolve)); assert.equal(h.files.state.busy, null); }
function key(window, values = {}) {
  const event = new Event("keydown", { cancelable: true });
  Object.assign(event, { key: "s", ctrlKey: true, ...values }); window.dispatchEvent(event); return event;
}

test("native panel is labeled, mounted beside source downloads and reused after suspension", () => {
  const h = setup();
  assert.equal(h.el("file-status").attributes.role, "status");
  assert.equal(h.el("file-save").attributes["aria-describedby"], "file-help file-status");
  assert.equal(h.el("source-status").nextSibling.id, "file-controls");
  const panel = h.el("file-controls"); h.suspend(); h.resume();
  assert.equal(h.el("file-controls"), panel); assert.equal(h.files.state.filename, null);
  h.dispose();
});

test("unsupported browser retains ordinary source I/O and shortcut prepares explicit download", async () => {
  const h = setup({ supported: false });
  assert.equal(h.el("file-open").disabled, true); assert.equal(h.el("file-save-as").disabled, true);
  assert.match(h.el("file-status").textContent, /ordinary file input/);
  assert.equal(h.el("prepare-markdown").disabled, false);
  h.edit("# Offline without renderer");
  assert.equal(key(h.window).defaultPrevented, true);
  assert.equal(await (await fetch(h.download.href)).text(), "# Offline without renderer");
  assert.equal(h.download.hidden, false); assert.equal(h.files.state.phase, "unlinked");
  assert.match(h.el("file-status").textContent, /Click the source Download/);
  h.dispose(); assert.equal(h.urls.size, 0);
});

test("actual source controls preserve imported BOM/CRLF through direct file save", async () => {
  const h = setup(), file = new MemoryFileHandle("unicode.md", "\uFEFF# A\r\n日本語\r"); h.pick(file);
  await h.files.open();
  assert.equal(h.source.value, "\uFEFF# A\n日本語\n");
  const bytes = file.entry.bytes.slice(); await h.files.save();
  assert.deepEqual(file.entry.bytes, bytes); assert.equal(h.replacements, 1);
  h.dispose();
});

test("saving leaves undo/redo and document identity intact", async () => {
  const h = setup(); await h.files.open(); h.edit("# Edited");
  const replacements = h.replacements;
  await h.files.save();
  assert.equal(h.replacements, replacements); assert.equal(h.files.state.phase, "saved");
  assert.equal(h.editing.undo(), true); assert.equal(h.source.value, "# Disk\n");
  assert.equal(h.files.state.phase, "edited");
  assert.equal(h.editing.redo(), true); assert.equal(h.files.state.phase, "saved");
  h.dispose();
});

test("Save as renames source without revoking image authority or resetting history", async () => {
  const h = setup(); h.edit("# New document"); await h.files.saveAs();
  assert.equal(h.filename.value, "new.md"); assert.equal(h.replacements, 0); assert.equal(h.images, true);
  assert.equal(h.editing.undo(), true); assert.equal(h.source.value, "# Initial\n");
  assert.equal(h.files.state.phase, "edited"); h.dispose();
});

test("conflict workflow retains edits, offers native Blob recovery download and explicit reload", async () => {
  const h = setup(); await h.files.open(); h.edit("# My recovery"); h.picked.source = "# External";
  await assert.rejects(h.files.save(), { code: "FILE_CHANGED" });
  assert.equal(h.el("file-save").disabled, true); assert.match(h.el("file-status").textContent, /Disk conflict/);
  h.files.backup(); assert.equal(await (await fetch(h.download.href)).text(), "# My recovery");
  h.el("file-keep").dispatchEvent(new Event("click")); assert.equal(h.root.activeElement, h.source);
  assert.equal(h.files.state.phase, "conflict");
  h.confirm(false); assert.equal((await h.files.reload()).opened, false); assert.equal(h.source.value, "# My recovery");
  h.confirm(true); await h.files.reload(); assert.equal(h.source.value, "# External");
  assert.equal(h.files.state.phase, "saved"); assert.equal(h.urls.size, 0);
  h.dispose();
});

test("Ctrl/Cmd+S invokes picker synchronously and Shift selects Save as", async () => {
  const h = setup(); h.edit("# New");
  assert.equal(key(h.window, { ctrlKey: false, metaKey: true }).defaultPrevented, true);
  assert.equal(h.savePickerCalls, 1); await settled(h);
  h.target(new MemoryFileHandle("another.md", ""));
  assert.equal(key(h.window, { shiftKey: true }).defaultPrevented, true);
  assert.equal(h.savePickerCalls, 2); await settled(h);
  assert.equal(h.files.state.filename, "another.md");
  assert.equal(key(h.window, { altKey: true }).defaultPrevented, false);
  assert.equal(key(h.window, { repeat: true }).defaultPrevented, true); assert.equal(h.savePickerCalls, 2);
  h.dispose();
});

test("composition disables file commands and does not steal IME shortcuts", async () => {
  const h = setup(); await h.files.open(); h.source.dispatchEvent(new Event("compositionstart"));
  assert.equal(h.el("file-open").disabled, true);
  assert.equal(key(h.window).defaultPrevented, false);
  await assert.rejects(h.files.save(), { code: "FILE_BUSY" });
  h.source.dispatchEvent(new Event("compositionend")); assert.equal(h.el("file-open").disabled, false);
  h.source.readOnly = true; await assert.rejects(h.files.save(), { code: "FILE_BUSY" });
  h.dispose();
});

test("draft restoration and ordinary import detach file ownership without losing source", async () => {
  const h = setup(); await h.files.open();
  h.controls.replace({ filename: "recovered.md", source: "recovery" });
  assert.equal(h.files.state.filename, null); assert.equal(h.source.value, "recovery");
  await h.files.open(); await h.controls.openFile(new File(["imported"], "imported.md"));
  assert.equal(h.files.state.filename, null); assert.equal(h.source.value, "imported");
  h.dispose();
});

test("pending ordinary import blocks native access without disabling it permanently", async () => {
  const h = setup(), held = deferred();
  const file = new File(["imported"], "imported.md");
  Object.defineProperty(file, "arrayBuffer", { value: () => held.promise });
  const opening = h.controls.openFile(file);
  await assert.rejects(h.files.open(), { code: "FILE_BUSY" }); assert.equal(h.pickerCalls, 0);
  held.resolve(new TextEncoder().encode("imported").buffer); await opening;
  await h.files.open(); assert.equal(h.pickerCalls, 1); h.dispose();
});

test("suspension removes capabilities and late picker cannot reconnect after resume", async () => {
  const h = setup(), pending = deferred();
  h.window.showOpenFilePicker = () => pending.promise;
  const opening = h.files.open(); h.suspend(); h.resume();
  pending.resolve([new MemoryFileHandle()]); await assert.rejects(opening, { code: "SESSION_DISPOSED" });
  assert.equal(h.files.state.filename, null); assert.equal(h.source.value, "# Initial\n"); h.dispose();
});

test("beforeunload warning follows actual unsaved source and is removed on save/dispose", async () => {
  const h = setup();
  const unload = () => { const event = new Event("beforeunload", { cancelable: true }); Object.defineProperty(event, "returnValue", { value: "", writable: true }); h.window.dispatchEvent(event); return event.defaultPrevented; };
  assert.equal(unload(), false); h.edit("unsaved"); assert.equal(unload(), true);
  await h.files.saveAs(); assert.equal(unload(), false);
  h.edit("newer"); assert.equal(unload(), true); h.dispose(); assert.equal(unload(), false);
});

test("button failure shows bounded diagnostic, not native exception text", async () => {
  const h = setup(); h.window.showOpenFilePicker = () => { throw new Error("/private/user/path"); };
  h.el("file-open").dispatchEvent(new Event("click")); await settled(h); await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.el("file-status").textContent.includes("/private/user/path"), false);
  assert.match(h.el("file-status").textContent, /FILE_OPERATION_FAILED/); h.dispose();
});

test("file-name text is inserted as text, not interpreted as HTML", async () => {
  const h = setup(); h.pick(new MemoryFileHandle("&amp;.md", "# Good")); await h.files.open();
  assert.match(h.el("file-status").textContent, /&amp;\.md/); h.dispose();
});
