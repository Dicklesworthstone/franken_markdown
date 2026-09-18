// Real controller and source I/O; explicit textarea/DOM event-contract doubles.
// These tests do not claim native browser input or Rust/WASM acceptance.
import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { FLOW_SOURCE_LIMIT } from "../flow_session.mjs";
import { createSourceEditingControls, createSourceControls } from "../demo/flow_document.mjs";
const code = expected => error => error.code === expected;
const event = (type, values = {}) => Object.assign(new Event(type, { cancelable: true, bubbles: true }), values);
class Element extends EventTarget {
  #value = "";
  disabled = false; readOnly = false; checked = false; hidden = false; textContent = "";
  selectionStart = 0; selectionEnd = 0; selectionDirection = "none"; focused = false;
  get value() { return this.#value; }
  set value(value) { this.#value = value.replace(/\r\n?/g, "\n"); this.setSelectionRange(this.#value.length, this.#value.length); }
  setSelectionRange(start, end, direction = "none") {
    this.selectionStart = start; this.selectionEnd = end; this.selectionDirection = direction;
  }
  focus() { this.focused = true; }
  select() { this.setSelectionRange(0, this.value.length); }
  removeAttribute(name) { delete this[name]; }
  click() { const e = event("click"); if (!this.disabled) this.dispatchEvent(e); return e; }
}
function setup(t, initial = "a cat and a cat") {
  const nodes = Object.fromEntries(["sourceEditor", "undo", "redo", "query", "ignoreCase", "previous", "next", "replacement", "replace", "replaceAll", "status"].map(name => [name, new Element()]));
  nodes.sourceEditor.value = initial; nodes.sourceEditor.setSelectionRange(0, 0);
  const panel = { open: false }; nodes.query.closest = () => panel;
  const api = createSourceEditingControls(nodes); t.after(() => api.dispose());
  const type = (source, start = source.length, end = start, inputType = "insertText") => {
    nodes.sourceEditor.dispatchEvent(event("beforeinput", { inputType }));
    nodes.sourceEditor.value = source; nodes.sourceEditor.setSelectionRange(start, end);
    nodes.sourceEditor.dispatchEvent(event("input", { inputType }));
  };
  const find = text => { nodes.query.value = text; nodes.query.dispatchEvent(event("input")); };
  const key = (key, more = {}) => { const e = event("keydown", { key, ctrlKey: true, ...more }); nodes.sourceEditor.dispatchEvent(e); return e; };
  return { ...nodes, api, type, find, key, panel };
}
test("typing, undo and redo restore source and selection and notify downstream listeners once", t => {
  const f = setup(t, "😀 cat"); let inputs = 0;
  f.sourceEditor.addEventListener("input", () => inputs++);
  f.sourceEditor.setSelectionRange(3, 6, "backward"); f.type("😀 dog", 6);
  assert.equal(f.undo.disabled, false); f.undo.click();
  assert.equal(f.sourceEditor.value, "😀 cat");
  assert.deepEqual([f.sourceEditor.selectionStart, f.sourceEditor.selectionEnd, f.sourceEditor.selectionDirection], [3, 6, "backward"]);
  f.redo.click(); assert.equal(f.sourceEditor.value, "😀 dog"); assert.equal(inputs, 3);
  assert.equal(f.redo.disabled, true); assert.equal(f.sourceEditor.focused, true);
});
test("keyboard history is scoped to the source editor and blocks native fallback at the boundary", t => {
  const f = setup(t, "a"); f.type("ab");
  assert.equal(f.key("z").defaultPrevented, true); assert.equal(f.sourceEditor.value, "a");
  assert.equal(f.key("z").defaultPrevented, true); assert.equal(f.sourceEditor.value, "a");
  f.key("Z", { ctrlKey: false, metaKey: true, shiftKey: true }); assert.equal(f.sourceEditor.value, "ab");
  f.key("z"); f.key("y"); assert.equal(f.sourceEditor.value, "ab");
  const e = event("keydown", { key: "z", ctrlKey: true }); f.query.dispatchEvent(e); assert.equal(e.defaultPrevented, false);
  assert.equal(f.key("z", { altKey: true }).defaultPrevented, false);
});
test("cancelable beforeinput history routes context-menu undo and redo through the same history", t => {
  const f = setup(t, "a"); f.type("ab");
  for (const [inputType, value] of [["historyUndo", "a"], ["historyRedo", "ab"]]) {
    const e = event("beforeinput", { inputType }); f.sourceEditor.dispatchEvent(e);
    assert.equal(e.defaultPrevented, true); assert.equal(f.sourceEditor.value, value);
  }
});
test("composition inputs form one undo transaction and editing commands never interrupt composition", t => {
  const f = setup(t, "x"); f.sourceEditor.setSelectionRange(1, 1);
  f.sourceEditor.dispatchEvent(event("compositionstart"));
  for (const source of ["xk", "xka", "xか"]) {
    f.sourceEditor.value = source; f.sourceEditor.dispatchEvent(event("input", { isComposing: true }));
  }
  assert.equal(f.undo.disabled, true); assert.equal(f.key("z", { isComposing: true }).defaultPrevented, false);
  assert.throws(() => f.api.replaceAll(), code("EDITOR_BUSY"));
  f.sourceEditor.dispatchEvent(event("compositionend")); f.sourceEditor.dispatchEvent(event("input"));
  f.api.undo(); assert.equal(f.sourceEditor.value, "x"); assert.equal(f.api.undo(), false);
  f.api.redo(); assert.equal(f.sourceEditor.value, "xか");
});
test("literal source navigation wraps with UTF-16 ranges and fresh ASCII-fold options", t => {
  const f = setup(t, "😀 Cat cat CAT"); f.find("cat"); f.api.findNext();
  assert.deepEqual([f.sourceEditor.selectionStart, f.sourceEditor.selectionEnd], [7, 10]);
  f.ignoreCase.checked = true; // Even without a change event, action reads fresh options.
  f.api.findNext(); assert.equal(f.sourceEditor.selectionStart, 11);
  f.api.findNext(); assert.equal(f.sourceEditor.selectionStart, 3); assert.match(f.status.textContent, /wrapped/);
  f.api.findPrevious(); assert.equal(f.sourceEditor.selectionStart, 11);
});
test("Ctrl/Cmd+F expands the source find panel and Enter/Shift+Enter navigate", t => {
  const f = setup(t); f.sourceEditor.setSelectionRange(2, 5);
  assert.equal(f.key("f").defaultPrevented, true); assert.equal(f.query.value, "cat");
  assert.equal(f.panel.open, true); assert.equal(f.query.focused, true);
  const forward = event("keydown", { key: "Enter" }); f.query.dispatchEvent(forward);
  assert.equal(forward.defaultPrevented, true); assert.equal(f.sourceEditor.selectionStart, 12);
  f.query.dispatchEvent(event("keydown", { key: "Enter", shiftKey: true })); assert.equal(f.sourceEditor.selectionStart, 2);
});
test("replace-selected uses literal replacement and is one undoable mutation", t => {
  const f = setup(t); f.find("cat"); f.api.findNext(); f.replacement.value = "$& <script>";
  let inputs = 0; f.sourceEditor.addEventListener("input", () => inputs++);
  assert.equal(f.api.replaceSelected(), true); assert.equal(f.sourceEditor.value, "a $& <script> and a cat");
  assert.equal(inputs, 1); f.api.undo(); assert.equal(f.sourceEditor.value, "a cat and a cat");
  assert.deepEqual([f.sourceEditor.selectionStart, f.sourceEditor.selectionEnd], [2, 5]);
});
test("replace-all and deletion are atomic single history entries", t => {
  const f = setup(t); f.find("cat"); f.replacement.value = "dog";
  assert.equal(f.api.replaceAll(), true); assert.equal(f.sourceEditor.value, "a dog and a dog");
  f.api.undo(); assert.equal(f.sourceEditor.value, "a cat and a cat"); assert.equal(f.api.undo(), false);
  f.replacement.value = ""; f.api.replaceAll(); assert.equal(f.sourceEditor.value, "a  and a ");
  assert.equal(f.api.redo(), false); f.api.undo(); assert.equal(f.sourceEditor.value, "a cat and a cat");
});
test("selection and query changes are revalidated rather than editing stale offsets", t => {
  const f = setup(t); f.find("cat"); f.api.findNext(); f.replacement.value = "dog";
  f.sourceEditor.setSelectionRange(0, 1);
  assert.throws(() => f.api.replaceSelected(), code("NO_SELECTED_MATCH")); assert.equal(f.sourceEditor.value, "a cat and a cat");
  f.sourceEditor.value = "other source"; f.sourceEditor.setSelectionRange(2, 5);
  assert.throws(() => f.api.replaceSelected(), code("NO_SELECTED_MATCH")); assert.equal(f.sourceEditor.value, "other source");
  f.query.value = "source"; f.api.findNext(); assert.equal(f.sourceEditor.selectionStart, 6);
});
test("programmatic image-reference input joins history without a beforeinput event", t => {
  const f = setup(t, "document"); f.sourceEditor.value += "\n![local](image.png)";
  f.sourceEditor.dispatchEvent(event("input"));
  assert.equal(f.undo.disabled, false); f.api.undo(); assert.equal(f.sourceEditor.value, "document");
  f.api.redo(); assert.equal(f.sourceEditor.value, "document\n![local](image.png)");
});
test("document replacement clears old history even when the new document has identical text", t => {
  const f = setup(t, "old"); f.type("new");
  f.sourceEditor.dispatchEvent(event("fmd-document-replaced")); f.sourceEditor.dispatchEvent(event("input"));
  assert.equal(f.api.undo(), false); assert.equal(f.sourceEditor.value, "new");
  f.type("newer"); f.api.undo(); assert.equal(f.sourceEditor.value, "new"); assert.equal(f.api.undo(), false);
});
test("no-op replacements preserve redo and emit no false input notification", t => {
  const f = setup(t, "cat"); f.type("dog"); f.api.undo(); f.find("cat"); f.replacement.value = "cat";
  let inputs = 0; f.sourceEditor.addEventListener("input", () => inputs++);
  assert.equal(f.api.replaceAll(), false); assert.equal(inputs, 0); assert.equal(f.redo.disabled, false);
  f.api.redo(); assert.equal(f.sourceEditor.value, "dog");
});
test("too many matches disables replace-all and direct invocation refuses partial replacement", t => {
  const f = setup(t, "a".repeat(10001)); f.find("a"); f.replacement.value = "b";
  assert.equal(f.replaceAll.disabled, true); assert.equal(f.next.disabled, false); assert.match(f.status.textContent, /More than 10000/);
  assert.throws(() => f.api.replaceAll(), code("TOO_MANY_MATCHES")); assert.equal(f.sourceEditor.value, "a".repeat(10001));
  assert.equal(f.api.undo(), false);
});
test("over-budget replacement leaves both editor and redo untouched", t => {
  const original = "a".repeat(FLOW_SOURCE_LIMIT - 1) + "X", f = setup(t, original);
  f.type(original.slice(0, -1) + "Y"); f.api.undo(); f.find("X"); f.replacement.value = "😀";
  assert.throws(() => f.api.replaceAll(), code("BUDGET_EXCEEDED")); assert.equal(f.sourceEditor.value, original);
  f.api.findNext(); assert.throws(() => f.api.replaceSelected(), code("BUDGET_EXCEEDED"));
  assert.equal(f.redo.disabled, false); f.api.redo(); assert.ok(f.sourceEditor.value.endsWith("Y"));
});
test("invalid typed source remains visible and commands pause until repaired", t => {
  const f = setup(t, "safe"); f.type("safe\ud800");
  assert.equal(f.sourceEditor.value, "safe\ud800"); assert.equal(f.undo.disabled, true); assert.match(f.status.textContent, /INVALID_UNICODE/);
  assert.throws(() => f.api.undo(), code("INVALID_UNICODE")); assert.equal(f.sourceEditor.value, "safe\ud800");
  f.type("safe😀"); f.api.undo(); assert.equal(f.sourceEditor.value, "safe");
});
test("an invalid initial editor does not crash source-control startup", t => {
  const f = setup(t, "\ud800"); assert.equal(f.undo.disabled, true);
  f.type("repaired"); f.type("edited"); f.api.undo(); assert.equal(f.sourceEditor.value, "repaired");
});
test("readonly state blocks mutations and composition-style keyCode 229 is untouched", t => {
  const f = setup(t, "a"); f.type("ab"); f.sourceEditor.readOnly = true;
  assert.throws(() => f.api.undo(), code("EDITOR_BUSY")); assert.equal(f.key("z").defaultPrevented, false);
  f.sourceEditor.readOnly = false; assert.equal(f.key("z", { keyCode: 229 }).defaultPrevented, false);
  assert.equal(f.sourceEditor.value, "ab");
});
test("newlines in programmatic replacement normalize before recording undo", t => {
  const f = setup(t, "X"); f.find("X");
  Object.defineProperty(f.replacement, "value", { value: "a\r\nb\rc", configurable: true });
  f.api.replaceAll(); assert.equal(f.sourceEditor.value, "a\nb\nc");
  f.api.undo(); assert.equal(f.sourceEditor.value, "X"); f.api.redo(); assert.equal(f.sourceEditor.value, "a\nb\nc");
});
test("disposal removes listeners and prevents old history from being reused", t => {
  const f = setup(t); f.type("edited"); f.api.dispose(); f.api.dispose();
  assert.equal(f.undo.disabled, true); assert.throws(() => f.api.undo(), code("SESSION_DISPOSED"));
  assert.equal(f.key("z").defaultPrevented, false); f.type("still editable"); assert.equal(f.sourceEditor.value, "still editable");
});
test("source downloads invalidate on replace and undo recovers exact imported BOM/CRLF bytes", async t => {
  const f = setup(t), filename = new Element(), open = new Element(), prepare = new Element(), download = new Element(), status = new Element();
  filename.value = "old.md"; const blobs = [], revoked = [];
  const docs = createSourceControls({ sourceEditor: f.sourceEditor, filename, open, prepare, download, status,
    onReplace: () => f.sourceEditor.dispatchEvent(event("fmd-document-replaced")),
    urls: { createObjectURL(blob) { blobs.push(blob); return `blob:${blobs.length}`; }, revokeObjectURL(url) { revoked.push(url); } } });
  t.after(() => docs.dispose()); const original = "\ufeff# cat\r\ncat\rend\n";
  await docs.openFile(new File([original], "import.md")); docs.prepareDownload();
  assert.equal(f.api.undo(), false); f.find("cat"); f.replacement.value = "dog"; f.api.replaceAll();
  assert.equal(download.hidden, true); assert.deepEqual(revoked, ["blob:1"]);
  f.api.undo(); assert.equal(docs.snapshot().source, original); docs.prepareDownload();
  assert.deepEqual(new Uint8Array(await blobs.at(-1).arrayBuffer()), new TextEncoder().encode(original));
});

test("undo during an acknowledged draft write saves the latest source, not the replaced text", async t => {
  const { createDraftControls } = await import("../demo/flow_draft_controls.mjs");
  const f = setup(t, "old"), nodes = Object.fromEntries(["filename", "remember", "refresh", "restore", "forget", "save", "status"].map(name => [name, new Element()]));
  nodes.filename.value = "doc.md"; const writes = []; let release, started, version = 0;
  const held = new Promise(resolve => { release = resolve; }), entered = new Promise(resolve => { started = resolve; });
  const store = { async read() { return { schemaVersion: 1, version, document: null }; },
    async save(document, expected) {
      assert.equal(expected, version); writes.push(document.source);
      if (writes.length === 1) { started(); await held; }
      return { schemaVersion: 1, version: ++version, document };
    }, dispose() {} };
  const drafts = createDraftControls({ ...nodes, sourceEditor: f.sourceEditor, openStore: async () => store,
    readDocument: () => ({ filename: nodes.filename.value, source: f.sourceEditor.value }), restoreDocument() {}, delayMs: 60000 });
  t.after(() => drafts.dispose()); await drafts.whenIdle();
  nodes.remember.checked = true; nodes.remember.dispatchEvent(event("change"));
  f.find("old"); f.replacement.value = "new"; f.api.replaceAll();
  const saving = drafts.flush(); await entered; f.api.undo(); release(); await saving;
  assert.deepEqual(writes, ["new", "old"]); assert.equal(f.sourceEditor.value, "old");
  assert.equal(drafts.state.dirty, false); assert.equal(drafts.state.version, 2);
});
test("draft conflict never takes away source undo or overwrites the editor", async t => {
  const { createDraftControls } = await import("../demo/flow_draft_controls.mjs");
  const f = setup(t, "old"), nodes = Object.fromEntries(["filename", "remember", "refresh", "restore", "forget", "save", "status"].map(name => [name, new Element()]));
  nodes.filename.value = "doc.md";
  const store = { async read() { return { schemaVersion: 1, version: 0, document: null }; },
    async save() { throw Object.assign(new Error("Another tab changed the draft."), { code: "DRAFT_CONFLICT" }); }, dispose() {} };
  const drafts = createDraftControls({ ...nodes, sourceEditor: f.sourceEditor, openStore: async () => store,
    readDocument: () => ({ filename: nodes.filename.value, source: f.sourceEditor.value }), restoreDocument() {}, delayMs: 60000 });
  t.after(() => drafts.dispose()); await drafts.whenIdle();
  nodes.remember.checked = true; nodes.remember.dispatchEvent(event("change")); f.type("new");
  await assert.rejects(drafts.flush(), code("DRAFT_CONFLICT"));
  assert.equal(f.sourceEditor.value, "new"); assert.equal(drafts.state.enabled, false);
  f.api.undo(); assert.equal(f.sourceEditor.value, "old"); assert.equal(drafts.state.enabled, false);
});
