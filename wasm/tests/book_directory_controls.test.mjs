import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookDirectoryControls } from "../demo/book_directory_controls.mjs";

// Normal imports execute the complete production controller AND directory
// session. The DOM, source host and browser handles below are explicit doubles.
class Element {
  constructor(tag = "div") { this.tagName = tag; this.children = []; this.listeners = new Map(); this.attributes = {}; }
  id = ""; value = ""; textContent = ""; disabled = false; hidden = false; parentElement = null;
  appendChild(node) { node.parentElement = this; this.children.push(node); return node; }
  setAttribute(key, value) { this.attributes[key] = value; }
  querySelector(selector) {
    if (selector === `#${this.id}`) return this;
    for (const child of this.children) { const found = child.querySelector(selector); if (found) return found; }
    return null;
  }
  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(listener);
  }
  removeEventListener(type, listener) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter(fn => fn !== listener)); }
  emit(type, values = {}) {
    const event = { target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...values };
    for (let node = this; node; node = node.parentElement) for (const fn of [...(node.listeners.get(type) ?? [])]) fn(event);
    return event;
  }
  focus() { let root = this; while (root.parentElement) root = root.parentElement; root.activeElement = this; }
}
const encode = text => new TextEncoder().encode(text);
const view = source => source.replace(/\r\n?/g, "\n");
function fixture(t, { available = true, captureError = false } = {}) {
  const root = new Element("document"), window = new Element("window"), parent = root.appendChild(new Element());
  root.createElement = tag => new Element(tag);
  for (const id of ["publish-title", "chapter-source", "chapter-path", "source-role", "chapters", "title"])
    parent.appendChild(new Element()).id = id;
  const el = id => root.querySelector(`#${id}`);
  let files = [{ path: "a.md", source: "\ufeff# A\r\n" }, { path: "parts/shared.txt", source: "shared", role: "include" }];
  let revision = 1, epoch = 1, busy = false, active = 0, captures = 0, permissions = 0, writes = 0;
  const listeners = new Set(), busyListeners = new Set(), calls = [], confirmations = [];
  const disk = new Map(files.map(file => [file.path, { bytes: encode(file.source) }]));
  let grant = "granted", hook = async () => {};
  const directory = path => ({ kind: "directory", name: path.split("/").at(-1) || "manuscript",
    async requestPermission() { permissions++; return grant; },
    async getDirectoryHandle(name) { return directory(path ? `${path}/${name}` : name); },
    async getFileHandle(name, { create = false } = {}) {
      const target = path ? `${path}/${name}` : name;
      if (!disk.has(target)) {
        if (!create) throw Object.assign(Error("missing"), { name: "NotFoundError" });
        disk.set(target, { bytes: encode("") });
      }
      const entry = disk.get(target);
      return { kind: "file", name, entry,
        async isSameEntry(other) { return other.entry === entry; },
        async getFile() { return new File([entry.bytes], name); },
        async createWritable() { let bytes, aborted = false; return {
          async write(blob) { bytes = new Uint8Array(await blob.arrayBuffer()); await hook("write", target); },
          async close() { if (aborted) throw Object.assign(Error("aborted"), { name: "AbortError" }); entry.bytes = bytes; writes++; await hook("closed", target); },
          async abort() { aborted = true; },
        }; },
      };
    },
  });
  const folder = directory("");
  if (available) window.showDirectoryPicker = options => { calls.push("picker"); assert.deepEqual(options, { mode: "read" }); return folder; };
  window.confirm = () => { confirmations.push("forbidden native prompt"); throw Error("Reviews use explicit in-panel approval"); };
  const notify = () => { for (const listener of listeners) listener(); };
  const show = () => {
    const file = files[active]; el("chapter-source").value = view(file?.source ?? "");
    el("chapter-path").value = file?.path ?? ""; el("source-role").value = file?.role ?? "chapter";
  };
  const collection = {
    get files() { return files.map(file => ({ ...file })); }, get sourceProjectRevision() { return epoch; },
    get images() { throw Error("Folder controls cannot read image grants"); },
    get fonts() { throw Error("Folder controls cannot read font grants"); },
    get options() { throw Error("Folder controls cannot serialize publishing settings"); },
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
  };
  const controls = {
    get currentChapter() { return active; },
    get sourceBusy() { return busy; },
    checkpoint: () => JSON.stringify([revision, active, el("chapter-source").value, el("chapter-path").value, el("title").value]),
    captureProject() {
      calls.push("capture"); captures++;
      if (captureError) throw Object.assign(Error("invalid draft"), { code: "INVALID_PROJECT" });
      const file = files[active];
      if (file && (view(file.source) !== el("chapter-source").value || file.path !== el("chapter-path").value)) {
        files = files.map((item, i) => i === active ? { ...item, path: el("chapter-path").value,
          source: view(item.source) === el("chapter-source").value ? item.source : el("chapter-source").value } : item);
        revision++; notify();
      }
      return { files: collection.files };
    },
    subscribeSourceState(listener) { busyListeners.add(listener); return () => busyListeners.delete(listener); },
    replaceProject() { throw Error("Directory saves cannot replace a project"); },
    prepare() { throw Error("Directory saves cannot start rendering"); },
  };
  show();
  const ui = createBookDirectoryControls({ root, controls, collection, window });
  t.after(() => ui.dispose());
  return { root, window, el, ui, disk, calls, confirmations, listeners, busyListeners,
    get captures() { return captures; }, get permissions() { return permissions; }, get writes() { return writes; }, get files() { return collection.files; },
    set grant(value) { grant = value; }, set hook(value) { hook = value; },
    edit(text, dispatch = true) { el("chapter-source").value = text; if (dispatch) { controls.captureProject(); el("chapter-source").emit("input"); } },
    select(index) { active = index; show(); el("chapters").emit("change"); },
    replace(next = files) { files = next.map(file => ({ ...file })); epoch++; revision++; show(); notify(); },
    changeFiles(next) { files = next; active = Math.min(active, Math.max(0, files.length - 1)); revision++; show(); notify(); },
    busy(value) { busy = value; for (const listener of busyListeners) listener(); },
    text(path) { return new TextDecoder("utf-8", { ignoreBOM: true }).decode(disk.get(path).bytes); },
  };
}
async function awaitingReview(h) {
  for (let i = 0; i < 30 && h.el("book-directory-approve").hidden; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.el("book-directory-approve").hidden, false, "an actionable review should appear");
}
async function approve(h, work) {
  const result = work(); result.catch(() => {}); await awaitingReview(h);
  h.el("book-directory-approve").emit("click"); return result;
}

test("an accessible source-folder panel is installed without I/O or permission requests", t => {
  const h = fixture(t);
  assert.equal(h.el("book-directory-status").attributes.role, "status");
  assert.equal(h.el("book-directory-status").attributes["aria-live"], "polite");
  assert(h.el("book-directory-approve").hidden); assert(h.el("book-directory-save").disabled);
  assert.match(h.el("book-directory-limits").textContent, /NOT an atomic save/);
  assert.equal(h.permissions, 0); assert.equal(h.captures, 0); assert.equal(h.writes, 0);
});

test("review snapshots raw editor input without applying it and requires a separate connect approval", async t => {
  const h = fixture(t); h.edit("uncaptured edit", false);
  const result = h.ui.bind(); await awaitingReview(h);
  assert.deepEqual(h.calls, ["picker"]);
  assert.equal(h.captures, 0); assert.equal(h.files[0].source, "\ufeff# A\r\n");
  assert.equal(h.ui.state.folder, null); assert.equal(h.writes, 0);
  assert.match(h.el("book-directory-review-list").textContent, /replace: a.md/);
  assert.match(h.el("book-directory-review-list").textContent, /unchanged: parts\/shared.txt/);
  h.el("book-directory-approve").emit("click"); assert.equal(await result, true);
  assert.equal(h.ui.state.folder, "manuscript"); assert.equal(h.permissions, 0); assert.equal(h.writes, 0);
});

test("saving chapters and includes uses the real session, exact bytes and visible receipts", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind());
  h.edit("\ufeff# New 日本 😀\r\n"); h.select(1); h.edit("shared revised\n");
  const result = h.ui.save(); await awaitingReview(h);
  assert.equal(h.permissions, 1); assert.equal(h.writes, 0);
  assert.equal(h.el("book-directory-approve").textContent, "Write reviewed source files");
  h.el("book-directory-approve").emit("click"); const saved = await result;
  assert.equal(saved.verified, 2); assert.equal(h.writes, 2);
  for (const file of h.files) assert.equal(h.text(file.path), file.source);
  assert.match(h.el("book-directory-receipt").textContent, /verified: parts\/shared.txt/);
  assert.equal(h.ui.state.phase, "saved"); assert.deepEqual(h.confirmations, []);
});

test("cancel review and Escape leave source and disk unchanged", async t => {
  const h = fixture(t);
  const first = h.ui.bind(); await awaitingReview(h); h.el("book-directory-decline").emit("click");
  assert.equal(await first, false); assert.equal(h.ui.state.folder, null);
  await approve(h, () => h.ui.bind()); h.edit("ours");
  const saving = h.ui.save(); await awaitingReview(h);
  assert(h.root.emit("keydown", { key: "Escape" }).defaultPrevented);
  assert.equal(await saving, null); assert.equal(h.writes, 0); assert.equal(h.files[0].source, "ours");
});

test("source changes automatically retire pending approval without swallowing later input", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("captured");
  const saving = h.ui.save(); saving.catch(() => {}); await awaitingReview(h); h.edit("newer");
  await assert.rejects(saving, { code: "STALE_SOURCE" });
  assert(h.el("book-directory-approve").hidden); assert.equal(h.writes, 0); assert.equal(h.files[0].source, "newer");
});

test("undispatched source changes during approval fail the host checkpoint", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("captured");
  const saving = h.ui.save(); saving.catch(() => {}); await awaitingReview(h); h.edit("raw newer", false);
  h.el("book-directory-approve").emit("click"); await assert.rejects(saving, { code: "STALE_SOURCE" });
  assert.equal(h.el("chapter-source").value, "raw newer"); assert.equal(h.writes, 0);
});

test("byte-identical project replacement discards the old folder authorization", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); const before = h.files;
  h.replace(before); assert.equal(h.ui.state.folder, null); assert.equal(h.ui.state.phase, "unlinked");
  await assert.rejects(h.ui.save(), { code: "NO_CONNECTED_DIRECTORY" }); assert.equal(h.writes, 0);
});

test("project replacement during a pending save retires its approval and cannot write either book", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("old book edit");
  const saving = h.ui.save(); saving.catch(() => {}); await awaitingReview(h);
  h.replace([{ path: "different.md", source: "new book" }]);
  await assert.rejects(saving, { code: "STALE_SOURCE" }); assert.equal(h.ui.state.folder, null);
  assert.equal(h.writes, 0); assert.equal(h.files[0].source, "new book");
});

test("source additions require fresh path review and removed disk paths are not deleted", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind());
  h.changeFiles([{ path: "new.md", source: "New" }]); assert.equal(h.ui.state.phase, "needs-review");
  assert(h.el("book-directory-save").disabled);
  await approve(h, () => h.ui.bind()); await approve(h, () => h.ui.save());
  assert.equal(h.text("new.md"), "New"); assert(h.disk.has("a.md")); assert(h.disk.has("parts/shared.txt"));
});

test("a disk conflict is visible, blocks save, and retains current source for explicit review", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("ours");
  h.disk.get("a.md").bytes = encode("external");
  await assert.rejects(approve(h, () => h.ui.save()), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.ui.state.phase, "conflict"); assert(h.el("book-directory-save").disabled);
  assert.match(h.el("book-directory-status").textContent, /DIRECTORY_CHANGED/);
  assert.equal(h.files[0].source, "ours"); assert.equal(h.text("a.md"), "external");
});

test("partial/uncertain receipts remain visible rather than announcing all files saved", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("first"); h.select(1); h.edit("second");
  h.hook = async (op, path) => { if (op === "closed" && path === "parts/shared.txt") throw Error("uncertain close"); };
  await assert.rejects(approve(h, () => h.ui.save()), { code: "DIRECTORY_SAVE_UNCERTAIN" });
  assert.match(h.el("book-directory-receipt").textContent, /verified: a.md/);
  assert.match(h.el("book-directory-receipt").textContent, /uncertain: parts\/shared.txt/);
  assert(h.el("book-directory-save").disabled); assert.equal(h.files[1].source, "second");
});

test("busy imports and path composition dismiss approval and block imperatively initiated saves", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("ours");
  const saving = h.ui.save(); saving.catch(() => {}); await awaitingReview(h); h.busy(true);
  await assert.rejects(saving, { code: "STALE_SOURCE" });
  assert.throws(() => h.ui.save(), { code: "DIRECTORY_BUSY" }); h.busy(false);
  h.el("chapter-path").emit("compositionstart"); assert.throws(() => h.ui.save(), { code: "DIRECTORY_BUSY" });
  h.el("chapter-path").emit("compositionend"); await approve(h, () => h.ui.save()); assert.equal(h.writes, 1);
});

test("suspension disconnects and resumes without remembering authority or asking for permission", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind()); h.edit("ours");
  h.ui.suspend(); assert.equal(h.ui.state.folder, null); assert.throws(() => h.ui.bind(), { code: "SESSION_DISPOSED" });
  h.ui.resume(); assert.equal(h.ui.state.folder, null); assert.equal(h.permissions, 0); assert.equal(h.files[0].source, "ours");
});

test("disposal releases subscriptions and cancels a visible review without late writes", async t => {
  const h = fixture(t); const binding = h.ui.bind(); binding.catch(() => {}); await awaitingReview(h);
  h.ui.dispose(); h.ui.dispose(); await assert.rejects(binding, { code: "SESSION_DISPOSED" });
  assert.equal(h.listeners.size, 0); assert.equal(h.busyListeners.size, 0);
  h.el("book-directory-approve").emit("click"); assert.equal(h.writes, 0);
});

test("unsupported browsers retain an explicit fallback without enabling folder operations", async t => {
  const h = fixture(t, { available: false });
  assert(h.el("book-directory-review").disabled); assert.match(h.el("book-directory-status").textContent, /ordinary imports/);
  await assert.rejects(h.ui.bind(), { code: "DIRECTORY_ACCESS_UNAVAILABLE" }); assert.equal(h.permissions, 0);
});

test("invalid project drafts do not block saving valid raw source or trigger project capture", async t => {
  const h = fixture(t, { captureError: true }); h.edit("recover this raw source", false);
  const before = h.files; await approve(h, () => h.ui.bind()); await approve(h, () => h.ui.save());
  assert.equal(h.text("a.md"), "recover this raw source"); assert.equal(h.captures, 0);
  assert.deepEqual(h.files, before); assert.deepEqual(h.calls, ["picker"]);
});

test("beforeunload warns for dirty folder sources but does not steal project-save keyboard shortcuts", async t => {
  const h = fixture(t); await approve(h, () => h.ui.bind());
  assert.equal(h.window.emit("beforeunload").defaultPrevented, false); h.edit("ours");
  assert.equal(h.window.emit("beforeunload").defaultPrevented, true);
  assert.equal(h.root.emit("keydown", { key: "s", ctrlKey: true }).defaultPrevented, false);
  await approve(h, () => h.ui.save()); assert.equal(h.window.emit("beforeunload").defaultPrevented, false);
});
