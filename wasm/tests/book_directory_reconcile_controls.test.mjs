import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookDirectoryControls } from "../demo/book_directory_controls.mjs";

// Full production controller, merge planner and disk-session modules execute
// through normal imports. Only DOM, host transactions and browser handles are
// doubles. File/Blob encoding uses Node's native implementations.
class Node {
  constructor(tag = "div") { this.tag = tag; this.children = []; this.listeners = new Map(); this.attributes = {}; }
  id = ""; disabled = false; hidden = false; parentElement = null; _value = ""; _text = "";
  set value(value) { this._value = this.tag === "textarea" ? value.replace(/\r\n?/g, "\n") : value; }
  get value() { return this._value; }
  set textContent(text) { this._text = text; for (const child of this.children) child.parentElement = null; this.children = []; }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(""); }
  set innerHTML(_) { throw Error("Source must not become HTML"); }
  appendChild(node) { this.children.push(node); node.parentElement = this; return node; }
  setAttribute(name, value) { this.attributes[name] = value; }
  querySelector(selector) { if (selector === `#${this.id}`) return this; for (const child of this.children) { const found = child.querySelector(selector); if (found) return found; } return null; }
  addEventListener(type, listener) { if (!this.listeners.has(type)) this.listeners.set(type, new Set()); this.listeners.get(type).add(listener); }
  removeEventListener(type, listener) { this.listeners.get(type)?.delete(listener); }
  emit(type, values = {}) {
    const event = { target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...values };
    for (let node = this; node; node = node.parentElement) for (const listener of [...(node.listeners.get(type) ?? [])]) listener(event);
    return event;
  }
  focus() { let root = this; while (root.parentElement) root = root.parentElement; root.activeElement = this; }
}
function fixture(t, initial = [{ path: "a.md", source: "a\nb\nc\n" }, { path: "parts/inc.txt", source: "include\n", role: "include" }]) {
  const root = new Node("document"), window = new Node("window"), panel = root.appendChild(new Node());
  root.createElement = tag => new Node(tag);
  for (const [id, tag] of [["publish-title", "h2"], ["chapter-source", "textarea"], ["chapter-path", "input"], ["source-role", "select"], ["chapters", "select"]])
    panel.appendChild(new Node(tag)).id = id;
  const el = id => root.querySelector(`#${id}`);
  let files = initial.map(file => ({ ...file })), revision = 1, epoch = 1, selected = 0, busy = false, installs = 0, writes = 0, permissions = 0;
  const subscribers = new Set(), busySubscribers = new Set(), disk = new Map(files.map(file => [file.path, { source: file.source }]));
  const emit = () => { for (const listener of subscribers) listener(); };
  const show = () => { el("chapter-source").value = files[selected].source; el("chapter-path").value = files[selected].path; el("source-role").value = files[selected].role ?? "chapter"; };
  const folder = (path = "") => ({ kind: "directory", name: path.split("/").at(-1) || "sources",
    async requestPermission() { permissions++; return "granted"; },
    async getDirectoryHandle(name) { return folder(path ? `${path}/${name}` : name); },
    async getFileHandle(name, { create = false } = {}) {
      assert.equal(create, false, "reconciliation cannot create files");
      const key = path ? `${path}/${name}` : name, entry = disk.get(key);
      if (!entry) throw Object.assign(Error("absent"), { name: "NotFoundError" });
      return { kind: "file", name, entry,
        async isSameEntry(other) { return other.entry === entry; },
        async getFile() { return new File([new TextEncoder().encode(entry.source)], name); },
        async createWritable() { writes++; throw Error("reconciliation cannot write files"); },
      };
    },
  });
  window.showDirectoryPicker = async options => { assert.deepEqual(options, { mode: "read" }); return folder(); };
  const collection = { get files() { return files.map(file => ({ ...file })); }, get sourceProjectRevision() { return epoch; },
    get images() { throw Error("No image access"); }, get fonts() { throw Error("No font access"); }, get options() { throw Error("No setting access"); },
    subscribe(fn) { subscribers.add(fn); return () => subscribers.delete(fn); } };
  const controls = { get currentChapter() { return selected; }, get sourceBusy() { return busy; },
    checkpoint: () => JSON.stringify([revision, selected, el("chapter-source").value, el("chapter-path").value]),
    subscribeSourceState(fn) { busySubscribers.add(fn); return () => busySubscribers.delete(fn); },
    applySources(next, expected) {
      assert.equal(expected, controls.checkpoint()); installs++;
      assert.deepEqual(next.map(file => [file.path, file.role]), files.map(file => [file.path, file.role]));
      files = next.map(file => ({ ...file })); revision++; show(); emit();
    },
    captureProject() { throw Error("No project capture"); }, prepare() { throw Error("No renderer"); },
  };
  show(); const ui = createBookDirectoryControls({ root, controls, collection, window }); t.after(() => ui.dispose());
  return { root, window, el, ui, disk, controls, collection,
    get files() { return files; }, get installs() { return installs; }, get writes() { return writes; }, get permissions() { return permissions; },
    edit(index, source, dispatch = true) { if (dispatch) { files[index].source = source; revision++; if (index === selected) show(); emit(); }
      else { assert.equal(index, selected); el("chapter-source").value = source; } },
    external(path, source, replace = false) { if (replace) disk.set(path, { source }); else disk.get(path).source = source; },
    busy(value) { busy = value; for (const listener of busySubscribers) listener(); },
    replaceProject() { epoch++; revision++; emit(); },
    choose(id, value) { const node = el(`book-directory-merge-${id}`); assert(node, id); node.value = value; node.emit("change"); },
  };
}
async function pending(h, work) {
  const promise = work(); promise.catch(() => {});
  for (let i = 0; i < 50 && h.el("book-directory-approve").hidden; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.el("book-directory-approve").hidden, false, "review must appear");
  return { promise };
}
async function connect(h) { const { promise } = await pending(h, () => h.ui.bind()); h.el("book-directory-approve").emit("click"); await promise; }
const approve = h => h.el("book-directory-approve").emit("click");
async function conflict(t) { const h = fixture(t); await connect(h); h.edit(0, "a\nours\nc\n"); h.external("a.md", "a\ntheirs\nc\n"); return h; }

test("reconciliation is reachable but requires a folder and explicit approval", async t => {
  const h = fixture(t); assert(h.el("book-directory-reconcile").disabled); await connect(h);
  assert.equal(h.el("book-directory-reconcile").disabled, false);
  h.edit(0, "A\nb\nc\n"); h.external("a.md", "a\nb\nC\n");
  const { promise } = await pending(h, () => h.ui.reconcile());
  assert.equal(h.installs, 0); assert.match(h.el("book-directory-review-list").textContent, /merged: a.md/);
  approve(h); const receipt = await promise;
  assert.equal(receipt.changedSources, 1); assert.equal(h.files[0].source, "A\nb\nC\n");
  assert.equal(h.installs, 1); assert.equal(h.disk.get("a.md").source, "a\nb\nC\n");
  assert.equal(h.writes, 0); assert.equal(h.permissions, 0); assert(h.ui.state.canSave);
});
test("the original refresh still refuses divergence, while the new button starts reconciliation", async t => {
  const h = fixture(t); await connect(h); h.edit(0, "A\nb\nc\n"); h.external("a.md", "a\nb\nC\n");
  await assert.rejects(h.ui.refresh(), { code: "DIRECTORY_SOURCE_CONFLICT" });
  h.el("book-directory-reconcile").emit("click");
  for (let i = 0; i < 50 && h.el("book-directory-approve").hidden; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.el("book-directory-approve").textContent, "Apply reviewed reconciliation"); approve(h);
  for (let i = 0; i < 50 && h.ui.state.busy; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.files[0].source, "A\nb\nC\n"); assert.equal(h.installs, 1);
});
test("choosing a conflict region preserves unrelated local and disk edits", async t => {
  const h = fixture(t, [{ path: "a.md", source: "a\nsp1\nb\nsp2\nc\n" }]); await connect(h);
  h.edit(0, "A\nsp1\nours\nsp2\nc\n"); h.external("a.md", "a\nsp1\ntheirs\nsp2\nC\n");
  const { promise } = await pending(h, () => h.ui.reconcile());
  assert(h.el("book-directory-approve").disabled); approve(h); assert.equal(h.installs, 0);
  h.choose("0-choice", "regions"); assert(h.el("book-directory-approve").disabled);
  h.choose("0-hunk-0", "disk"); assert.equal(h.el("book-directory-approve").disabled, false);
  approve(h); await promise;
  assert.equal(h.files[0].source, "A\nsp1\ntheirs\nsp2\nC\n"); assert.equal(h.installs, 1);
  assert(h.el("book-directory-merge").hidden); assert.equal(h.el("book-directory-merge").children.length, 0);
});
test("every conflicting file must be chosen before the single source transaction", async t => {
  const h = await conflict(t); h.edit(1, "local include"); h.external("parts/inc.txt", "disk include");
  const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "disk");
  assert(h.el("book-directory-approve").disabled); h.choose("1-choice", "local"); approve(h); await promise;
  assert.equal(h.files[0].source, "a\ntheirs\nc\n"); assert.equal(h.files[1].source, "local include");
  assert.equal(h.files[1].role, "include"); assert.equal(h.installs, 1); assert.equal(h.writes, 0);
});
test("whole-file edited resolutions, including empty text, use existing atomic admission", async t => {
  const h = await conflict(t); const { promise } = await pending(h, () => h.ui.reconcile());
  h.choose("0-choice", "edited"); const area = h.el("book-directory-merge-0-source"); assert.equal(area.hidden, false);
  area.value = ""; area.emit("input"); approve(h); await promise;
  assert.equal(h.files[0].source, ""); assert.equal(h.disk.get("a.md").source, "a\ntheirs\nc\n");
});
test("untouched manual text preserves imported CRLF and BOM despite textarea normalization", async t => {
  const h = fixture(t, [{ path: "a.md", source: "\ufeffa\r\nb\r\n" }]); await connect(h);
  h.edit(0, "\ufeffours\r\nb\r\n"); h.external("a.md", "\ufefftheirs\r\nb\r\n");
  const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "edited");
  assert.equal(h.el("book-directory-merge-0-source").value, "\ufeffours\nb\n"); approve(h); await promise;
  assert.equal(h.files[0].source, "\ufeffours\r\nb\r\n");
});
test("text composition in a resolution blocks approval without invalidating legitimate input", async t => {
  const h = await conflict(t); const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "edited");
  const area = h.el("book-directory-merge-0-source"); area.emit("compositionstart"); area.value = "日本 😀"; approve(h);
  assert(h.el("book-directory-approve").disabled); assert.equal(h.installs, 0);
  area.emit("compositionend"); approve(h); await promise; assert.equal(h.files[0].source, "日本 😀");
});
test("undispatched editor input after review is never overwritten by a chosen result", async t => {
  const h = await conflict(t); const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "disk");
  h.edit(0, "new raw input", false); approve(h); await assert.rejects(promise, { code: "STALE_SOURCE" });
  assert.equal(h.installs, 0); assert.equal(h.el("chapter-source").value, "new raw input");
});
test("a disk edit during approval refuses the entire source set", async t => {
  const h = await conflict(t); h.external("parts/inc.txt", "new include");
  const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "disk");
  h.external("parts/inc.txt", "even newer include"); approve(h);
  await assert.rejects(promise, { code: "DIRECTORY_CHANGED" }); assert.equal(h.installs, 0);
  assert.equal(h.files[1].source, "include\n"); assert.equal(h.writes, 0);
});
test("identical bytes in a different disk file cannot satisfy a stale identity check", async t => {
  const h = await conflict(t); const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "local");
  h.external("a.md", "a\ntheirs\nc\n", true); approve(h);
  await assert.rejects(promise, { code: "DIRECTORY_CHANGED" }); assert.equal(h.installs, 0);
});
test("Escape discards region decisions and the next review requires new approval", async t => {
  const h = await conflict(t); const first = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "regions"); h.choose("0-hunk-0", "disk");
  assert(h.root.emit("keydown", { key: "Escape" }).defaultPrevented); assert.equal(await first.promise, null);
  const second = await pending(h, () => h.ui.reconcile()); assert.equal(h.el("book-directory-merge-0-choice").value, "");
  h.choose("0-choice", "disk"); approve(h); await second.promise; assert.equal(h.installs, 1);
});
test("an absent disk source can be retained or edited but not silently deleted", async t => {
  const h = fixture(t); await connect(h); h.disk.delete("a.md");
  const { promise } = await pending(h, () => h.ui.reconcile());
  assert(!h.el("book-directory-merge-0-choice").children.some(option => option.value === "disk"));
  h.choose("0-choice", "disk"); assert(h.el("book-directory-approve").disabled);
  h.choose("0-choice", "local"); approve(h); await promise; assert.equal(h.files[0].source, "a\nb\nc\n");
});
test("untrusted source markup stays inert text in the review", async t => {
  const h = await conflict(t); h.external("a.md", '<img src=x onerror="bad()">\n');
  const { promise } = await pending(h, () => h.ui.reconcile());
  assert.match(h.el("book-directory-merge").textContent, /<img src=x/);
  h.choose("0-choice", "disk"); approve(h); await promise; assert.equal(h.writes, 0);
});
test("project replacement, suspension and busy imports retire pending decisions", async t => {
  for (const stop of [h => h.replaceProject(), h => h.ui.suspend(), h => h.busy(true)]) {
    const h = await conflict(t); const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "disk");
    stop(h); await assert.rejects(promise, { code: "STALE_SOURCE" });
    assert(h.el("book-directory-merge").hidden); assert.equal(h.installs, 0); assert.equal(h.writes, 0);
  }
});
test("disposal cancels a reviewed reconciliation and old controls cannot apply it", async t => {
  const h = await conflict(t); const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "disk");
  const button = h.el("book-directory-approve"); h.ui.dispose(); button.emit("click");
  await assert.rejects(promise, { code: "SESSION_DISPOSED" }); assert.equal(h.installs, 0);
});
test("the workbench reconciles multiple distant edits in a ten-thousand-line source", async t => {
  const b = Array.from({ length: 10000 }, (_, i) => `line-${i}\n`), l = b.slice(), r = b.slice(), expected = b.slice();
  l[2] = expected[2] = "local top\n"; l[9997] = expected[9997] = "local bottom\n";
  r[20] = expected[20] = "disk top\n"; r[9900] = expected[9900] = "disk bottom\n";
  const h = fixture(t, [{ path: "a.md", source: b.join("") }]); await connect(h);
  h.edit(0, l.join("")); h.external("a.md", r.join(""));
  const { promise } = await pending(h, () => h.ui.reconcile()); approve(h); await promise;
  assert.equal(h.files[0].source, expected.join("")); assert.equal(h.installs, 1); assert.equal(h.writes, 0);
});
test("over-large region reviews fall back to explicit whole-file decisions without truncating", async t => {
  const b = Array.from({ length: 514 }, (_, i) => `line-${i}\n`);
  const l = b.map((line, i) => i % 2 ? line : `local-${i}\n`), r = b.map((line, i) => i % 2 ? line : `disk-${i}\n`);
  const h = fixture(t, [{ path: "a.md", source: b.join("") }]); await connect(h);
  h.edit(0, l.join("")); h.external("a.md", r.join(""));
  const { promise } = await pending(h, () => h.ui.reconcile());
  assert(!h.el("book-directory-merge-0-choice").children.some(option => option.value === "regions"));
  assert(h.el("book-directory-approve").disabled); h.choose("0-choice", "local"); approve(h); await promise;
  assert.equal(h.files[0].source, l.join("")); assert.equal(h.writes, 0);
});
test("invalid Unicode in a manual result cannot partly apply other disk changes", async t => {
  const h = await conflict(t); h.external("parts/inc.txt", "external include");
  const { promise } = await pending(h, () => h.ui.reconcile()); h.choose("0-choice", "edited");
  h.el("book-directory-merge-0-source").value = "\ud800"; approve(h);
  await assert.rejects(promise, { code: "INVALID_UNICODE" }); assert.equal(h.installs, 0);
  assert.equal(h.files[0].source, "a\nours\nc\n"); assert.equal(h.files[1].source, "include\n");
});
