import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookDirectoryControls } from "../demo/book_directory_controls.mjs";

// The production controller AND disk session execute unchanged. DOM, source
// host notifications and browser handles are adapters; File/Blob/UTF-8 are real.
class Target {
  listeners = new Map();
  addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type, new Set()); this.listeners.get(type).add(fn); }
  removeEventListener(type, fn) { this.listeners.get(type)?.delete(fn); }
  emit(type, detail = {}) {
    const event = { target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...detail };
    for (const fn of this.listeners.get(type) ?? []) fn(event);
    return event;
  }
}
class Element extends Target {
  id = ""; textContent = ""; value = ""; hidden = false; disabled = false; style = {}; attrs = {}; children = [];
  setAttribute(name, value) { this.attrs[name] = value; }
  appendChild(node) { node.parentElement = this; this.children.push(node); return node; }
  set innerHTML(_) { throw Error("No HTML insertion is authorized"); }
}
class Root extends Target {
  nodes = [];
  createElement(tag) { const node = new Element(); node.tagName = tag; this.nodes.push(node); return node; }
  querySelector(selector) { return this.nodes.find(node => `#${node.id}` === selector) ?? null; }
}
const encoder = new TextEncoder();
const decode = bytes => new TextDecoder("utf-8", { ignoreBOM: true }).decode(bytes);
const browserError = name => Object.assign(new Error(name), { name });
class Disk {
  files = new Map(); dirs = new Set([""]); log = []; hook = async () => {}; grant = "granted";
  constructor(files = {}) { for (const [path, source] of Object.entries(files)) this.put(path, source); this.root = this.directory(""); }
  put(path, source) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i++) this.dirs.add(parts.slice(0, i).join("/"));
    const entry = { bytes: encoder.encode(source), locked: false }; this.files.set(path, entry); return entry;
  }
  text(path) { return decode(this.files.get(path).bytes); }
  async call(op, path) { this.log.push([op, path]); await this.hook(op, path); }
  directory(path) {
    const disk = this, child = name => path ? `${path}/${name}` : name;
    return { kind: "directory", name: path.split("/").at(-1) || "sources",
      async requestPermission(options) { assert.deepEqual(options, { mode: "readwrite" }); await disk.call("permission", path); return disk.grant; },
      async getDirectoryHandle(name, { create = false } = {}) {
        const next = child(name); await disk.call(create ? "mkdir" : "directory", next);
        if (!disk.dirs.has(next)) { if (!create) throw browserError("NotFoundError"); disk.dirs.add(next); }
        return disk.directory(next);
      },
      async getFileHandle(name, { create = false } = {}) {
        const next = child(name); await disk.call(create ? "create" : "locate", next);
        if (!disk.files.has(next)) { if (!create) throw browserError("NotFoundError"); disk.put(next, ""); }
        return disk.handle(next, disk.files.get(next));
      },
      entries() { throw Error("No enumeration"); }, removeEntry() { throw Error("No deletion"); },
    };
  }
  handle(path, entry) {
    const disk = this;
    return { kind: "file", name: path.split("/").at(-1), entry,
      async isSameEntry(other) { await disk.call("identity", path); return entry === other.entry; },
      async getFile() { await disk.call("read", path); return new File([entry.bytes], path.split("/").at(-1)); },
      async createWritable(options) {
        assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
        await disk.call("writer", path); if (entry.locked) throw browserError("NoModificationAllowedError");
        entry.locked = true; let bytes, aborted = false;
        return {
          async write(blob) { await disk.call("write", path); assert(blob instanceof Blob); bytes = new Uint8Array(await blob.arrayBuffer()); },
          async close() { try { await disk.call("close", path); if (aborted) throw browserError("AbortError"); entry.bytes = bytes; await disk.call("closed", path); } finally { entry.locked = false; } },
          async abort() { await disk.call("abort", path); aborted = true; entry.locked = false; },
        };
      },
    };
  }
}
async function until(predicate) {
  for (let i = 0; i < 1000; i++) { if (predicate()) return; await new Promise(setImmediate); }
  assert.fail("operation did not reach the expected state");
}
const writes = disk => disk.log.filter(([op]) => ["writer", "create", "mkdir", "close"].includes(op));
function fixture(t, { diskFiles = { "one.md": "old", "snippets/part.rs": "fn main() {}" }, files, picker = true, refresh = true } = {}) {
  const root = new Root(), window = new Target(), disk = new Disk(diskFiles);
  const section = root.createElement("section");
  for (const id of ["publish-title", "chapter-source", "chapter-path", "source-role", "title"]) {
    const node = root.createElement("input"); node.id = id; section.appendChild(node);
  }
  let source = files ?? [{ path: "one.md", source: "new", role: "chapter" }, { path: "snippets/part.rs", source: "fn main() {}", role: "include" }];
  let revision = 0, epoch = 0, selected = 0, busy = false, picks = 0, applied = 0;
  const subscribers = new Set(), busySubscribers = new Set();
  const node = name => root.querySelector(`#book-directory-${{
    connect: "review", review: "review-list", "review-panel": "approve", cancel: "decline",
  }[name] ?? name}`);
  const editor = root.querySelector("#chapter-source"), path = root.querySelector("#chapter-path"), role = root.querySelector("#source-role");
  function show() {
    editor.value = source[selected]?.source.replace(/\r\n?/g, "\n") ?? "";
    path.value = source[selected]?.path ?? ""; role.value = source[selected]?.role ?? "chapter";
  }
  show(); root.querySelector("#title").value = "unapplied draft";
  const forbidden = () => { throw Error("Source saving must not parse, render, mutate or serialize the book"); };
  const controls = { get currentChapter() { return selected; }, get sourceBusy() { return busy; },
    checkpoint: () => JSON.stringify([revision, selected, editor.value, path.value, role.value]),
    subscribeSourceState(fn) { busySubscribers.add(fn); return () => busySubscribers.delete(fn); },
    captureProject: forbidden, replaceProject: forbidden, prepareSource: forbidden, applySources: forbidden,
  };
  const collection = { get files() { return source.map(file => ({ ...file })); },
    get sourceProjectRevision() { return epoch; }, get revision() { return revision; },
    get options() { return forbidden(); }, get images() { return forbidden(); }, get fonts() { return forbidden(); },
    project: forbidden, snapshot: forbidden, configure: forbidden,
    subscribe(fn) { subscribers.add(fn); return () => subscribers.delete(fn); },
  };
  if (picker) window.showDirectoryPicker = options => {
    assert.deepEqual(options, { mode: "read" }); picks++; return Promise.resolve(disk.root);
  };
  if (refresh) controls.applySources = (next, expected) => {
    assert.equal(expected, controls.checkpoint());
    assert.deepEqual(next.map(file => [file.path, file.role]), source.map(file => [file.path, file.role]));
    applied++; source = next.map(file => ({ ...file })); revision++; show();
    for (const fn of subscribers) fn();
    return revision;
  };
  else delete controls.applySources;
  const controller = createBookDirectoryControls({ root, window, controls, collection });
  // Test-side aliases only: run the existing public UI actions and real clicks.
  const ui = Object.freeze({
    connect: async () => controller.bind(), save: async () => controller.save(), refresh: async () => controller.refresh(),
    detach: () => controller.detach(), suspend: () => controller.suspend(),
    resume: () => controller.resume(), dispose: () => controller.dispose(),
    approve: () => node("approve").emit("click"), cancelReview: () => node("cancel").emit("click"),
    get state() { return controller.state; },
  });
  t.after(() => ui.dispose());
  const change = () => { revision++; for (const fn of subscribers) fn(); };
  return { root, window, disk, ui, controls, collection, editor, path, role, node, subscribers, busySubscribers,
    get applied() { return applied; }, get picks() { return picks; }, get files() { return source; },
    edit(text, dispatch = true) { editor.value = text; if (dispatch) root.emit("input", { target: editor }); },
    update(files, project = false) { source = files; if (project) epoch++; show(); change(); },
    select(index) { selected = index; show(); },
    setBusy(value) { busy = value; for (const fn of busySubscribers) fn(); },
    async connect() { const result = ui.connect(); await until(() => !node("approve").disabled); ui.approve(); assert.equal(await result, true); },
    async save() { const result = ui.save(); await until(() => !node("approve").disabled); ui.approve(); return result; },
  };
}

const initial = [
  { path: "one.md", source: "old", role: "chapter" },
  { path: "snippets/part.rs", source: "fn main() {}", role: "include" },
];
async function refresh(h) {
  const pending = h.ui.refresh(); await until(() => !h.node("approve").disabled); h.ui.approve(); return pending;
}

test("the workbench refreshes disk-only include edits and retains raw local source and roles in one transaction", async t => {
  const h = fixture(t, { files: initial }); await h.connect();
  h.edit("local raw source", false); h.disk.put("snippets/part.rs", "fn main() {\r\n    // 日本\r\n}\r\n");
  const before = h.files, pending = h.ui.refresh(); await until(() => !h.node("approve").disabled);
  assert.deepEqual(h.files, before); assert.equal(h.applied, 0);
  assert.match(h.node("review").textContent, /keep-editor: one.md/);
  assert.match(h.node("review").textContent, /reload: snippets\/part.rs/);
  assert.match(h.node("review").textContent, /not a text diff/);
  assert.equal(h.node("approve").textContent, "Apply reviewed disk-only changes");
  h.ui.approve(); const result = await pending; assert.equal(result.changedSources, 1); assert.equal(h.applied, 1);
  assert.deepEqual(h.files.map(file => file.role), ["chapter", "include"]);
  assert.equal(h.editor.value, "local raw source"); assert.equal(h.files[0].source, "local raw source");
  assert.equal(h.files[1].source, h.disk.text("snippets/part.rs")); assert.equal(h.disk.text("one.md"), "old");
  assert.equal(h.root.querySelector("#title").value, "unapplied draft");
  assert.equal(h.ui.state.dirty, 1); assert.deepEqual(writes(h.disk), []);
  await h.save(); assert.equal(h.disk.text("one.md"), "local raw source");
});

test("refresh approval is separate, cancellable with Escape, and performs no source installation before acceptance", async t => {
  const h = fixture(t, { files: initial }); await h.connect(); h.disk.put("one.md", "external");
  const pending = h.ui.refresh(); await until(() => !h.node("approve").disabled);
  assert.equal(h.applied, 0); assert.match(h.node("status").textContent, /editor and disk unchanged/);
  assert(h.root.emit("keydown", { key: "Escape" }).defaultPrevented); assert.equal(await pending, null);
  assert.equal(h.files[0].source, "old"); assert.equal(h.applied, 0); assert.deepEqual(writes(h.disk), []);
});

test("refresh stays available after a disk conflict, but does not hide divergent source conflicts", async t => {
  const h = fixture(t, { files: initial }); await h.connect(); h.edit("local");
  h.disk.put("one.md", "external"); h.disk.put("snippets/part.rs", "safe disk edit");
  // Exercise the real click error handler, not only the public Promise API.
  h.node("refresh").emit("click"); await until(() => h.node("status").textContent.includes("DIRECTORY_SOURCE_CONFLICT"));
  assert.equal(h.applied, 0); assert(!h.node("refresh").disabled); assert(h.node("approve").hidden);
  assert.match(h.node("review").textContent, /conflict: one.md/);
  assert.match(h.node("review").textContent, /reload: snippets\/part.rs/);
  assert.equal(h.editor.value, "local"); assert.equal(h.files[1].source, "fn main() {}");
  // Explicitly returning the editor to the common baseline permits refresh.
  h.edit("old"); await refresh(h); assert.equal(h.files[0].source, "external"); assert.equal(h.applied, 1);
});

test("uncaptured role/path drafts are not discarded by the source installer", async t => {
  const h = fixture(t, { files: initial }); await h.connect(); h.disk.put("snippets/part.rs", "external"); h.disk.log = [];
  h.role.value = "include";
  await assert.rejects(h.ui.refresh(), { code: "DIRECTORY_SOURCE_DRAFT" }); assert.deepEqual(h.disk.log, []);
  assert.equal(h.role.value, "include"); h.role.value = "chapter"; h.path.value = "renamed.md";
  await assert.rejects(h.ui.refresh(), { code: "DIRECTORY_SOURCE_DRAFT" }); assert.equal(h.path.value, "renamed.md");
  assert.equal(h.applied, 0); assert.deepEqual(h.disk.log, []);
});

test("source changes during refresh review keep their text and cancel the old approval", async t => {
  const h = fixture(t, { files: initial }); await h.connect(); h.disk.put("snippets/part.rs", "external");
  const pending = h.ui.refresh(), rejection = assert.rejects(pending, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.edit("newer", false); h.ui.approve(); await rejection;
  assert.equal(h.editor.value, "newer"); assert.equal(h.applied, 0);
});

test("identical project replacement, composition and page suspension retire refresh approvals", async t => {
  for (const change of [h => h.update(h.files, true), h => h.root.emit("compositionstart", { target: h.path }), h => h.ui.suspend()]) {
    const h = fixture(t, { files: initial }); await h.connect(); h.disk.put("one.md", "external");
    const pending = h.ui.refresh(), rejection = assert.rejects(pending, { code: "STALE_SOURCE" });
    await until(() => !h.node("approve").disabled); change(h); await rejection; assert.equal(h.applied, 0);
  }
});

test("source application failures after mutation explicitly retire the old folder authority", async t => {
  const h = fixture(t, { files: initial }); await h.connect(); h.disk.put("one.md", "external");
  const apply = h.controls.applySources;
  h.controls.applySources = (...args) => { apply(...args); throw Error("failed after install"); };
  const pending = h.ui.refresh(), rejection = assert.rejects(pending, { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  await until(() => !h.node("approve").disabled); h.ui.approve(); await rejection;
  assert.equal(h.ui.state.folder, null); assert.equal(h.files[0].source, "external"); assert.deepEqual(writes(h.disk), []);
});

test("read refresh does not ask for write permission", async t => {
  const h = fixture(t, { files: initial }); await h.connect(); h.disk.grant = "denied"; h.disk.put("one.md", "external");
  await refresh(h); assert.equal(h.files[0].source, "external");
  assert(!h.disk.log.some(([op]) => op === "permission")); assert.deepEqual(writes(h.disk), []);
});

test("a host without atomic source transactions keeps saving but disables refresh", async t => {
  const h = fixture(t, { files: initial, refresh: false }); await h.connect(); assert(h.node("refresh").disabled);
  h.disk.log = []; await assert.rejects(h.ui.refresh(), { code: "DIRECTORY_REFRESH_UNAVAILABLE" }); assert.deepEqual(h.disk.log, []);
  h.edit("still saveable"); await h.save(); assert.equal(h.disk.text("one.md"), "still saveable");
});
