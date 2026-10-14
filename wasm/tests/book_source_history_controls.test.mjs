import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceHistoryControls } from "../demo/book_source_history.mjs";

// DOM and source-transaction host doubles are explicit. The complete production
// history engine/controller are imported normally; no implementation is rewritten.
class Element {
  constructor(tag = "div") { this.tagName = tag; this.children = []; this.listeners = new Map(); this.attributes = {}; }
  id = ""; value = ""; textContent = ""; parentElement = null; disabled = false; selectionStart = 0; selectionEnd = 0;
  appendChild(node) { node.parentElement = this; this.children.push(node); return node; }
  insertBefore(node, sibling) {
    node.parentElement = this;
    const index = this.children.indexOf(sibling);
    if (index < 0) this.children.push(node); else this.children.splice(index, 0, node);
  }
  setAttribute(key, value) { this.attributes[key] = value; }
  querySelector(selector) {
    if (selector === `#${this.id}`) return this;
    for (const child of this.children) { const value = child.querySelector(selector); if (value) return value; }
    return null;
  }
  addEventListener(type, fn, capture = false) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push({ fn, capture: capture === true });
  }
  removeEventListener(type, fn) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter(item => item.fn !== fn)); }
  emit(type, values = {}) {
    const event = { target: this, cancelable: true, defaultPrevented: false,
      preventDefault() { if (this.cancelable) this.defaultPrevented = true; }, ...values };
    const listeners = [...(this.listeners.get(type) ?? [])];
    for (const capture of [true, false]) for (const item of listeners) if (item.capture === capture) item.fn(event);
    return event;
  }
}
const clone = files => files.map(file => ({ ...file }));
const view = text => text.replace(/\r\n?/g, "\n");
function fixture(t, limits) {
  const root = new Element("document"); root.createElement = tag => new Element(tag);
  const parent = root.appendChild(new Element());
  for (const id of ["chapter-source", "chapter-path", "source-role", "chapters", "title"]) parent.appendChild(new Element()).id = id;
  const node = id => root.querySelector(`#${id}`), editor = node("chapter-source");
  let files = [{ path: "a.md", source: "\ufeff# A\r\n" }, { path: "b.md", source: "# B\n" }, { path: "shared.txt", source: "shared", role: "include" }];
  let revision = 1, config = 1, active = 0, busy = false, time = 0;
  const listeners = new Set(), busyListeners = new Set();
  let fail = false, afterApply = null, navigations = [], writes = 0;
  const resources = Object.freeze({ image: new Uint8Array([1, 2]), font: new Uint8Array([3, 4]) });
  const settings = { title: "Original", running: { footer: { center: "{page}" } } };
  const notify = () => { for (const fn of listeners) fn(); };
  const show = () => {
    const file = files[active]; editor.value = view(file?.source ?? "");
    node("chapter-path").value = file?.path ?? ""; node("source-role").value = file?.role ?? "chapter";
  };
  const collection = {
    get files() { return clone(files); }, get revision() { return revision; }, get renderConfigurationRevision() { return config; },
    get images() { throw Error("History must not read image payloads or grants"); },
    get fonts() { throw Error("History must not read font payloads or grants"); },
    get options() { throw Error("History must not read or restore publishing options"); },
    subscribe(fn) { listeners.add(fn); return () => listeners.delete(fn); },
  };
  function change(next, configuration = false) { files = clone(next); revision++; if (configuration) config++; notify(); }
  const checkpoint = () => JSON.stringify([revision, active, editor.value, node("chapter-path").value, node("title").value]);
  const controls = {
    get currentChapter() { return active; }, get sourceBusy() { return busy; }, checkpoint,
    captureProject() { throw Error("History must not validate or serialize a whole source project"); },
    subscribeSourceState(fn) { busyListeners.add(fn); return () => busyListeners.delete(fn); },
    applySources(next, stamp) {
      assert.equal(stamp, checkpoint());
      if (fail) throw Object.assign(Error("Rejected source write"), { code: "TEST_REJECTION" });
      assert.deepEqual(next.map(({ path, role }) => [path, role]), files.map(({ path, role }) => [path, role]));
      const installed = revision + 1; writes++; change(next); afterApply?.(); show(); return installed;
    },
    selectSourceRange(index, start, end, stamp) {
      assert.equal(stamp, checkpoint()); active = index; navigations.push([index, start, end]); show();
      editor.selectionStart = view(files[index].source.slice(0, start)).length;
      editor.selectionEnd = view(files[index].source.slice(0, end)).length;
    },
  };
  // The existing controller installs these before the history component. Capture
  // phase ordering is deliberate: history must reserve a typing group first.
  editor.addEventListener("input", () => {
    if (editor.value !== view(files[active].source)) {
      const next = clone(files); next[active].source = editor.value; change(next);
    }
  });
  editor.addEventListener("compositionstart", () => { busy = true; for (const fn of busyListeners) fn(); });
  editor.addEventListener("compositionend", () => { busy = false; for (const fn of busyListeners) fn(); });
  show(); const ui = createBookSourceHistoryControls({ root, collection, controls, limits, now: () => time });
  t.after(() => ui.dispose());
  return { ui, editor, root, node, collection, settings, resources, navigations, listeners, busyListeners,
    get files() { return clone(files); }, get writes() { return writes; },
    set fail(value) { fail = value; }, set afterApply(value) { afterApply = value; },
    type(text, inputType = "insertText", { start = editor.selectionStart, end = start, advance = 20, isComposing = false } = {}) {
      time += advance; editor.selectionStart = start; editor.selectionEnd = end;
      editor.emit("beforeinput", { inputType }); editor.value = text;
      editor.selectionStart = editor.selectionEnd = text.length;
      editor.emit("input", { inputType, isComposing });
    },
    batch(next) { change(next); show(); },
    configure() { config++; revision++; settings.title = "New title"; notify(); },
    recover(next) { change(next, true); show(); },
    select(index) { active = index; show(); node("chapters").emit("change"); },
    busy(value) { busy = value; for (const fn of busyListeners) fn(); },
  };
}

test("installs an accessible source toolbar with no resource/settings reads", t => {
  const h = fixture(t);
  assert(h.node("book-history-undo").disabled); assert(h.node("book-history-redo").disabled);
  assert.equal(h.node("book-history-status").attributes.role, "status");
  assert.equal(h.node("book-history-undo").attributes["aria-keyshortcuts"], "Control+z Meta+z");
  assert.match(h.node("book-history-help").textContent, /session-only/);
});

test("consecutive input captures coalesce before the host listener runs", t => {
  const h = fixture(t), initial = h.files;
  h.type("abc"); h.type("abcd"); h.type("abcde");
  assert.equal(h.ui.state.undo, 1);
  h.ui.undo(); assert.deepEqual(h.files, initial);
  h.ui.redo(); assert.equal(h.editor.value, "abcde");
  assert.equal(h.writes, 2);
});

test("time, caret navigation, paste and different input kinds split typing groups", t => {
  const h = fixture(t);
  h.type("a"); h.type("ab", "insertText", { advance: 1500 });
  h.editor.emit("keydown", { key: "ArrowLeft" }); h.type("abc");
  h.type("pasted", "insertFromPaste"); h.type("pasted!", "insertText");
  h.type("pasted", "deleteContentBackward");
  assert.equal(h.ui.state.undo, 6);
});

test("edits in different chapters and includes are undone together and navigate to the affected source", t => {
  const h = fixture(t), initial = h.files;
  h.type("a edited"); h.select(2); h.type("include edited"); h.select(1);
  h.ui.undo(); assert.equal(h.files[2].source, "shared"); assert.equal(h.navigations.at(-1)[0], 2);
  h.ui.undo(); assert.deepEqual(h.files, initial); assert.equal(h.navigations.at(-1)[0], 0);
  h.ui.redo(); h.ui.redo(); assert.equal(h.files[2].source, "include edited");
});

test("whole-book source transaction remains undoable after later typing", t => {
  const h = fixture(t), initial = h.files;
  h.batch(initial.map(file => ({ ...file, source: file.source + " replaced" })));
  h.type("edited after replacement");
  h.ui.undo(); assert(h.files.every(file => file.source.endsWith(" replaced")));
  h.ui.undo(); assert.deepEqual(h.files, initial);
});

test("presentation/resource configuration changes preserve source history but split typing bursts", t => {
  const h = fixture(t), resources = h.resources;
  h.type("a"); h.configure(); h.type("ab");
  assert.equal(h.ui.state.undo, 2); h.ui.undo(); h.ui.undo();
  assert.equal(h.settings.title, "New title"); assert.equal(h.resources, resources);
  assert.equal(h.settings.running.footer.center, "{page}");
});

test("imports, renames, order changes and different-source recovery reset history", t => {
  const h = fixture(t); h.type("a"); h.recover(h.files.toReversed());
  assert.equal(h.ui.state.undo, 0);
  h.type("b"); const next = h.files; next[0].path = "renamed.md"; h.recover(next);
  assert.equal(h.ui.state.undo, 0);
  h.type("c"); h.recover(h.files.map(file => ({ ...file, source: "reopened" })));
  assert.equal(h.ui.state.undo, 0);
});

test("undispatched source or path edits refuse undo without dropping the draft", t => {
  const h = fixture(t); h.type("captured"); h.editor.value = "not yet captured";
  assert.throws(() => h.ui.undo(), { code: "STALE_SOURCE" });
  assert.equal(h.editor.value, "not yet captured"); assert.equal(h.files[0].source, "captured");
  assert.equal(h.ui.state.undo, 1);
});

test("raw unrelated settings do not prevent source recovery or get overwritten", t => {
  const h = fixture(t); h.type("a"); h.node("title").value = "unsubmitted title";
  h.ui.undo(); assert.equal(h.node("title").value, "unsubmitted title");
});

test("undo can repair an incomplete UTF-16 source without serializing the invalid current project", t => {
  const h = fixture(t), initial = h.files;
  h.type("\ud800"); h.ui.undo(); assert.deepEqual(h.files, initial);
});

test("keyboard and cancelable native undo are routed once, including empty history", t => {
  const h = fixture(t); h.type("changed");
  assert(h.editor.emit("keydown", { key: "z", ctrlKey: true }).defaultPrevented);
  assert.equal(h.writes, 1);
  assert(h.editor.emit("beforeinput", { inputType: "historyRedo" }).defaultPrevented);
  assert.equal(h.writes, 2); assert.equal(h.editor.value, "changed");
  h.editor.emit("keydown", { key: "z", metaKey: true });
  h.editor.emit("keydown", { key: "Z", metaKey: true, shiftKey: true });
  h.editor.emit("keydown", { key: "z", ctrlKey: true });
  assert(h.editor.emit("keydown", { key: "z", ctrlKey: true }).defaultPrevented);
  assert.match(h.node("book-history-status").textContent, /NO_HISTORY/);
  assert.equal(h.writes, 5);
});

test("other fields and modified key bindings keep their ordinary browser behavior", t => {
  const h = fixture(t); h.type("a");
  assert(!h.node("title").emit("keydown", { key: "z", ctrlKey: true }).defaultPrevented);
  assert(!h.editor.emit("keydown", { key: "z", ctrlKey: true, altKey: true }).defaultPrevented);
  assert(!h.editor.emit("keydown", { key: "y", metaKey: true }).defaultPrevented);
  assert.equal(h.writes, 0);
});

test("composition groups partial input and final committed text into one undo", t => {
  const h = fixture(t), initial = h.files;
  h.editor.emit("compositionstart"); h.type("日", "insertCompositionText", { isComposing: true });
  h.type("日本", "insertCompositionText", { isComposing: true });
  assert(h.node("book-history-undo").disabled); assert.throws(() => h.ui.undo(), { code: "BOOK_BUSY" });
  h.editor.emit("compositionend"); h.type("日本語", "insertFromComposition");
  assert.equal(h.ui.state.undo, 1); h.ui.undo(); assert.deepEqual(h.files, initial);
});

test("import busy state blocks buttons and imperative history operations", t => {
  const h = fixture(t); h.type("a"); h.busy(true);
  assert(h.node("book-history-undo").disabled); assert.throws(() => h.ui.undo(), { code: "BOOK_BUSY" });
  h.busy(false); h.ui.undo(); assert.equal(h.ui.state.redo, 1);
});

test("a rejected source transaction does not consume history and can be retried", t => {
  const h = fixture(t); h.type("a"); h.fail = true;
  assert.throws(() => h.ui.undo(), { code: "TEST_REJECTION" });
  assert.equal(h.ui.state.undo, 1); assert.equal(h.ui.state.redo, 0);
  h.fail = false; h.ui.undo(); assert.equal(h.ui.state.redo, 1);
});

test("a concurrent edit after installation retires history, never overwrites the new source", t => {
  const h = fixture(t); h.type("a");
  h.afterApply = () => { h.afterApply = null; h.batch(h.files.map(file => ({ ...file, source: "newer edit" }))); };
  assert.throws(() => h.ui.undo(), { code: "HISTORY_CONFLICT" });
  assert(h.files.every(file => file.source === "newer edit")); assert.equal(h.ui.state.undo, 0);
});

test("cursor navigation never addresses the middle of a surrogate pair", t => {
  const h = fixture(t); h.type("😀"); h.type("😁", "insertFromPaste"); h.ui.undo();
  assert.equal(h.editor.value, "😀"); assert.equal(h.navigations.at(-1)[1], 0);
});

test("clear, suspension and disposal release history without changing source or resources", t => {
  const h = fixture(t); h.type("a"); const files = h.files;
  h.node("book-history-clear").emit("click"); assert.equal(h.ui.state.undo, 0); assert.deepEqual(h.files, files);
  h.type("ab"); h.ui.suspend(); assert.equal(h.ui.state.undo, 0);
  assert.throws(() => h.ui.undo(), { code: "HISTORY_CLOSED" });
  h.ui.resume(); assert.equal(h.ui.state.undo, 0); h.type("abc"); assert.equal(h.ui.state.undo, 1);
  h.ui.dispose(); h.ui.dispose(); assert.equal(h.listeners.size, 0); assert.equal(h.busyListeners.size, 0);
  const writes = h.writes; h.node("book-history-undo").emit("click"); assert.equal(h.writes, writes);
});

test("history budget exhaustion is visible and does not disable ordinary editing", t => {
  const h = fixture(t, { entries: 2, bytes: 64 });
  h.type("a", "insertFromPaste"); h.type("b", "insertFromPaste"); h.type("c", "insertFromPaste");
  assert.equal(h.ui.state.undo, 2); assert.match(h.node("book-history-status").textContent, /exceeded the history budget/);
  assert.equal(h.editor.disabled, false);
});

test("successful import reset is allowed while the host still owns its busy slot", t => {
  const h = fixture(t); h.type("a"); h.busy(true);
  h.ui.reset(); assert.equal(h.ui.state.undo, 0); assert.equal(h.files[0].source, "a");
  h.busy(false); h.type("ab"); assert.equal(h.ui.state.undo, 1);
});

test("noncancelable native undo is observed as a source edit, not replayed twice", t => {
  const h = fixture(t), original = h.editor.value; h.type("a");
  const event = h.editor.emit("beforeinput", { inputType: "historyUndo", cancelable: false });
  assert.equal(event.defaultPrevented, false); h.editor.value = original;
  h.editor.emit("input", { inputType: "historyUndo" });
  assert.equal(h.writes, 0); assert.equal(h.ui.state.undo, 2);
  h.ui.undo(); assert.equal(h.editor.value, "a");
});
