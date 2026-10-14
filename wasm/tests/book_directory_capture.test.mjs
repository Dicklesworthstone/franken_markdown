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
function fixture(t, { diskFiles = { "one.md": "old", "snippets/part.rs": "fn main() {}" }, files, picker = true } = {}) {
  const root = new Root(), window = new Target(), disk = new Disk(diskFiles);
  const section = root.createElement("section");
  for (const id of ["publish-title", "chapter-source", "chapter-path", "source-role", "title"]) {
    const node = root.createElement("input"); node.id = id; section.appendChild(node);
  }
  let source = files ?? [{ path: "one.md", source: "new", role: "chapter" }, { path: "snippets/part.rs", source: "fn main() {}", role: "include" }];
  let revision = 0, epoch = 0, selected = 0, busy = false, picks = 0;
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
  const controller = createBookDirectoryControls({ root, window, controls, collection });
  // Test-side aliases only: run the existing public UI actions and real clicks.
  const ui = Object.freeze({
    connect: async () => controller.bind(), save: async () => controller.save(),
    detach: () => controller.detach(), suspend: () => controller.suspend(),
    resume: () => controller.resume(), dispose: () => controller.dispose(),
    approve: () => node("approve").emit("click"), cancelReview: () => node("cancel").emit("click"),
    get state() { return controller.state; },
  });
  t.after(() => ui.dispose());
  const change = () => { revision++; for (const fn of subscribers) fn(); };
  return { root, window, disk, ui, controls, collection, editor, path, role, node, subscribers, busySubscribers,
    get picks() { return picks; }, get files() { return source; },
    edit(text, dispatch = true) { editor.value = text; if (dispatch) root.emit("input", { target: editor }); },
    update(files, project = false) { source = files; if (project) epoch++; show(); change(); },
    select(index) { selected = index; show(); },
    setBusy(value) { busy = value; for (const fn of busySubscribers) fn(); },
    async connect() { const result = ui.connect(); await until(() => !node("approve").disabled); ui.approve(); assert.equal(await result, true); },
    async save() { const result = ui.save(); await until(() => !node("approve").disabled); ui.approve(); return result; },
  };
}

test("boot is read-only, creates a labelled panel, and leaves other save shortcuts alone", t => {
  const h = fixture(t); assert.equal(h.picks, 0); assert.deepEqual(h.disk.log, []);
  assert(h.node("save").disabled); assert(!h.node("connect").disabled);
  assert.equal(h.node("status").attrs.role, "status");
  assert.equal(h.root.querySelector("#book-directory").attrs["aria-labelledby"], "book-directory-title");
  assert(!h.root.emit("keydown", { ctrlKey: true, key: "s" }).defaultPrevented);
  assert.equal(h.root.querySelector("#title").value, "unapplied draft");
});

test("connection and save each require an explicit in-page approval with all source paths", async t => {
  const h = fixture(t); const connect = h.ui.connect(); assert.equal(h.picks, 1);
  await until(() => !h.node("approve").disabled);
  assert.match(h.node("review").textContent, /replace: one.md/);
  assert.match(h.node("review").textContent, /unchanged: snippets\/part.rs/);
  assert.match(h.node("review").textContent, /without writing any file/);
  assert.deepEqual(writes(h.disk), []); assert.equal(h.ui.state.folder, null);
  h.node("approve").emit("click"); await connect;
  const save = h.ui.save(); await until(() => !h.node("approve").disabled);
  assert.match(h.node("review").textContent, /not committed atomically/);
  assert.deepEqual(writes(h.disk), []); assert(h.disk.log.some(([op]) => op === "permission"));
  h.node("approve").emit("click"); const receipt = await save;
  assert.equal(receipt.verified, 1); assert.equal(h.disk.text("one.md"), "new");
  assert.equal(h.ui.state.phase, "saved"); assert(h.node("save").disabled);
  assert.match(h.node("receipt").textContent, /verified: one.md/);
});

test("unmodified textarea normalization preserves original BOM and mixed newline bytes", async t => {
  const text = "\ufeff# 日本 😀\r\nA\rB\n";
  const h = fixture(t, { files: [{ path: "one.md", source: text, role: "chapter" }] });
  assert.notEqual(h.editor.value, text); await h.connect(); await h.save();
  assert.equal(h.disk.text("one.md"), text); assert.equal(h.files[0].source, text);
});

test("undispatched current editor text is saved without applying settings or changing source snapshots", async t => {
  const h = fixture(t); h.edit("\ufeff# Raw edit 😀\n", false); await h.connect(); await h.save();
  assert.equal(h.disk.text("one.md"), h.editor.value); assert.equal(h.files[0].source, "new");
  assert.equal(h.root.querySelector("#title").value, "unapplied draft");
});

test("include-only sources, new nested paths and empty source files are saved", async t => {
  const h = fixture(t, { files: [{ path: "nested/empty.rs", source: "", role: "include" }, { path: "other.txt", source: "\ufeffdata\r\n", role: "include" }] });
  await h.connect(); assert(!h.disk.dirs.has("nested"));
  const receipt = await h.save(); assert.equal(receipt.verified, 2);
  assert.equal(h.disk.text("nested/empty.rs"), ""); assert.equal(h.disk.text("other.txt"), "\ufeffdata\r\n");
  assert.match(h.node("receipt").textContent, /Directory creation attempted \(may remain\): nested/);
});

test("raw path changes require re-review and never remove the previous disk path", async t => {
  const h = fixture(t); await h.connect(); h.path.value = "renamed.md";
  const rejected = h.ui.save(); await assert.rejects(rejected, { code: "DIRECTORY_REVIEW_REQUIRED" });
  assert.equal(h.disk.text("one.md"), "old"); await h.connect(); await h.save();
  assert.equal(h.disk.text("renamed.md"), "new"); assert.equal(h.disk.text("one.md"), "old");
});

test("cancelling either review performs no writes and releases the operation slot", async t => {
  const h = fixture(t); let job = h.ui.connect(); await until(() => !h.node("approve").disabled);
  h.ui.cancelReview(); assert.equal(await job, false); assert.equal(h.ui.state.folder, null);
  await h.connect(); job = h.ui.save(); await until(() => !h.node("approve").disabled);
  h.node("cancel").emit("click"); assert.equal(await job, null);
  assert.equal(h.ui.state.busy, null); assert.deepEqual(writes(h.disk), []);
  assert(h.ui.state.canSave);
});

test("an input event dismisses a stale review and settles its promise without manual approval", async t => {
  const h = fixture(t); const job = h.ui.connect(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.edit("later"); await rejection;
  assert(h.node("review-panel").hidden); assert.equal(h.ui.state.busy, null); assert.equal(h.ui.state.folder, null);
  assert(h.node("approve").hidden); assert.deepEqual(writes(h.disk), []);
});

test("undispatched editing during approval is caught by the production session fence", async t => {
  const h = fixture(t); await h.connect(); const job = h.ui.save(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.edit("late text", false); h.ui.approve(); await rejection;
  assert.equal(h.disk.text("one.md"), "old"); assert.deepEqual(writes(h.disk), []);
});

test("a byte-identical project switch detaches the folder identity", async t => {
  const h = fixture(t); await h.connect(); h.update(h.files, true);
  assert.equal(h.ui.state.folder, null); assert(h.node("save").disabled);
  await assert.rejects(h.ui.save(), { code: "NO_CONNECTED_DIRECTORY" });
});

test("source-tree changes require review while ordinary edits retain the folder", async t => {
  const h = fixture(t); await h.connect(); h.update(h.files.map(file => ({ ...file, source: "changed" })));
  assert.equal(h.ui.state.folder, "sources"); assert.equal(h.ui.state.phase, "edited");
  h.update([...h.files, { path: "added.md", source: "added", role: "chapter" }]);
  assert.equal(h.ui.state.phase, "needs-review"); assert(h.node("save").disabled);
});

test("imports and composition block pickers and invalidate pending approval", async t => {
  const h = fixture(t); h.setBusy(true);
  await assert.rejects(h.ui.connect(), { code: "DIRECTORY_BUSY" }); assert.equal(h.picks, 0);
  h.setBusy(false); h.root.emit("compositionstart", { target: h.path });
  await assert.rejects(h.ui.connect(), { code: "DIRECTORY_BUSY" }); assert.equal(h.picks, 0);
  h.root.emit("compositionend", { target: h.path });
  const job = h.ui.connect(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.setBusy(true); h.setBusy(false); await rejection;
  assert.equal(h.ui.state.busy, null);
});

test("disconnecting cancels an in-page review but does not delete any source", async t => {
  const h = fixture(t); await h.connect(); const job = h.ui.save(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.node("disconnect").emit("click"); await rejection;
  assert.equal(h.ui.state.folder, null); assert.equal(h.disk.text("one.md"), "old"); assert.deepEqual(writes(h.disk), []);
});

test("a save cannot be queued behind another operation", async t => {
  const h = fixture(t); const job = h.ui.connect(); await until(() => !h.node("approve").disabled);
  await assert.rejects(h.ui.save(), { code: "DIRECTORY_BUSY" }); h.ui.approve(); await job;
  assert.equal(h.picks, 1); assert.deepEqual(writes(h.disk), []);
});

test("external modifications in an unchanged include prevent all writes and expose a conflict", async t => {
  const h = fixture(t); await h.connect(); h.disk.put("snippets/part.rs", "external");
  const job = h.ui.save(); const rejection = assert.rejects(job, { code: "DIRECTORY_CHANGED" });
  await until(() => !h.node("approve").disabled); h.ui.approve(); await rejection;
  assert.deepEqual(writes(h.disk), []); assert.equal(h.ui.state.phase, "conflict");
  assert.match(h.node("status").textContent, /DIRECTORY_CHANGED/); assert(h.node("save").disabled);
});

test("staging failure aborts streams and displays the per-path receipt", async t => {
  const h = fixture(t); await h.connect();
  h.disk.hook = async op => { if (op === "write") throw browserError("QuotaExceededError"); };
  const job = h.ui.save(); const rejection = assert.rejects(job, { code: "DIRECTORY_WRITE_FAILED" });
  await until(() => !h.node("approve").disabled); h.ui.approve(); await rejection;
  assert(h.disk.log.some(([op]) => op === "abort")); assert.equal(h.disk.text("one.md"), "old");
  assert.match(h.node("receipt").textContent, /not-saved: one.md/);
});

test("a later failed close exposes verified and uncertain paths and blocks silent retry", async t => {
  const h = fixture(t); h.update(h.files.map(file => ({ ...file, source: "changed" }))); await h.connect();
  h.disk.hook = async (op, path) => { if (op === "closed" && path === "snippets/part.rs") throw browserError("QuotaExceededError"); };
  const job = h.ui.save(); const rejection = assert.rejects(job, { code: "DIRECTORY_SAVE_UNCERTAIN" });
  await until(() => !h.node("approve").disabled); h.ui.approve(); await rejection;
  assert.match(h.node("receipt").textContent, /verified: one.md/);
  assert.match(h.node("receipt").textContent, /uncertain: snippets\/part.rs/);
  assert.equal(h.ui.state.phase, "uncertain"); assert(h.node("save").disabled);
  await assert.rejects(h.ui.save(), { code: "DIRECTORY_REVIEW_REQUIRED" });
});

test("editing during a committed close retains a verified old receipt, not a false current-save claim", async t => {
  const h = fixture(t); await h.connect();
  h.disk.hook = async op => { if (op === "closed") h.edit("newer"); };
  const receipt = await h.save(); assert.equal(receipt.current, false); assert.equal(receipt.verified, 1);
  assert.equal(h.disk.text("one.md"), "new"); assert.equal(h.editor.value, "newer");
  assert.equal(h.ui.state.phase, "edited"); assert.match(h.node("receipt").textContent, /not proof that current editor changes are saved/);
});

test("project replacement during staging stops remaining writes and retires the old connection", async t => {
  const h = fixture(t); await h.connect();
  h.disk.hook = async op => { if (op === "write") h.update(h.files, true); };
  const job = h.ui.save(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.ui.approve(); await rejection;
  assert.equal(h.ui.state.folder, null); assert.equal(h.disk.text("one.md"), "old");
  assert(!h.disk.log.some(([op]) => op === "close"));
});

test("suspension cancels pending review and resume never reconnects or prompts", async t => {
  const h = fixture(t); await h.connect(); const job = h.ui.save(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  await until(() => !h.node("approve").disabled); h.ui.suspend(); await rejection;
  assert(h.node("connect").disabled); assert.equal(h.ui.state.folder, null); assert(h.node("review-panel").hidden);
  h.ui.resume(); assert.equal(h.picks, 1); assert.equal(h.ui.state.folder, null); assert(!h.node("connect").disabled);
});

test("a picker completing after suspension cannot revive ownership", async t => {
  const h = fixture(t); let resolve;
  h.window.showDirectoryPicker = () => new Promise(done => { resolve = done; });
  const job = h.ui.connect(); const rejection = assert.rejects(job, { code: "STALE_SOURCE" });
  h.ui.suspend(); h.ui.resume(); resolve(h.disk.root); await rejection;
  assert.equal(h.ui.state.folder, null); assert(h.node("review-panel").hidden);
});

test("dispose settles review, unsubscribes observers and prevents late DOM updates", async t => {
  const h = fixture(t); const job = h.ui.connect(); const rejection = assert.rejects(job, { code: "SESSION_DISPOSED" });
  await until(() => !h.node("approve").disabled); h.ui.dispose(); const last = h.node("status").textContent;
  await rejection; h.ui.dispose(); h.ui.resume();
  assert.equal(h.subscribers.size, 0); assert.equal(h.busySubscribers.size, 0);
  assert.equal(h.node("status").textContent, last); assert(h.node("connect").disabled);
  await assert.rejects(h.ui.connect(), { code: "SESSION_DISPOSED" });
});

test("beforeunload warns for connected unsaved source and active I/O, not a verified save", async t => {
  const h = fixture(t); assert(!h.window.emit("beforeunload").defaultPrevented);
  const job = h.ui.connect(); assert(h.window.emit("beforeunload").defaultPrevented);
  await until(() => !h.node("approve").disabled); h.ui.approve(); await job;
  assert(h.window.emit("beforeunload").defaultPrevented); await h.save();
  assert(!h.window.emit("beforeunload").defaultPrevented); h.edit("more", false);
  assert(h.window.emit("beforeunload").defaultPrevented); h.ui.detach();
  assert(!h.window.emit("beforeunload").defaultPrevented);
});

test("unsupported browsers retain an explanatory fallback without opening any resource", async t => {
  const h = fixture(t, { picker: false }); assert(h.node("connect").disabled);
  assert.match(h.node("status").textContent, /unavailable here/);
  await assert.rejects(h.ui.connect(), { code: "DIRECTORY_ACCESS_UNAVAILABLE" }); assert.deepEqual(h.disk.log, []);
});

test("malformed editor Unicode, colliding paths and empty books fail before opening a picker", async t => {
  const h = fixture(t); h.edit("\ud800", false);
  await assert.rejects(h.ui.connect(), { code: "INVALID_UNICODE" }); assert.equal(h.picks, 0);
  h.update([{ path: "A.md", source: "a" }, { path: "a.md", source: "b" }]);
  await assert.rejects(h.ui.connect(), { code: "PATH_COLLISION" }); assert.equal(h.picks, 0);
  h.update([]); assert(h.node("connect").disabled);
  await assert.rejects(h.ui.connect(), { code: "NO_SOURCE_FILES" });
});

test("denied write permission never starts review or opens a writer", async t => {
  const h = fixture(t); await h.connect(); h.disk.grant = "denied";
  await assert.rejects(h.ui.save(), { code: "DIRECTORY_PERMISSION_DENIED" });
  assert(h.node("review-panel").hidden); assert.deepEqual(writes(h.disk), []);
});

test("folder names are plain text, and a duplicate controller is refused", async t => {
  const h = fixture(t); h.disk.root.name = "<img onerror=alert(1)>";
  await h.connect(); assert.match(h.node("status").textContent, /<img onerror=alert\(1\)>/);
  assert.throws(() => createBookDirectoryControls(h), /duplicate source-folder panel/);
});
