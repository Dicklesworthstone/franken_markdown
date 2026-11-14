// Real source collection/history and production rename controls, with explicit
// DOM, source-host and native-worker adapters. No JavaScript Markdown rewriter.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookSourceHistory } from "../demo/book_source_history.mjs";
import { createBookSourceRenameControls, createBookSourceRenamePanel } from "../demo/book_source_rename.mjs";

class Element extends EventTarget {
  constructor(root, tag) { super(); this.root = root; this.tagName = tag; this.children = []; this.value = ""; this.attrs = {}; }
  set id(value) { this._id = value; this.root.elements[value] = this; }
  get id() { return this._id; }
  appendChild(child) { this.children.push(child); child.parentElement = this; return child; }
  replaceChildren(...children) { this.children = []; for (const child of children) this.appendChild(child); }
  setAttribute(key, value) { this.attrs[key] = value; }
  set innerHTML(_) { assert.fail("Source paths must be text, never HTML"); }
  emit(type) { this.dispatchEvent(new Event(type)); }
}
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
const error = code => value => value?.code === code;
const initial = [{ path: "one.md", source: "\ufeff# One\r\n\r\n[Two](two.md#target)\r\n" },
  { path: "note.txt", source: "[One](one.md)\r\n", role: "include" },
  { path: "two.md", source: "# Two\n[One](one.md)\n" }];
const renamed = [{ path: "guide/one.md", source: "\ufeff# One\r\n\r\n[Two](../two.md#target)\r\n" },
  { path: "note.txt", source: "[One](guide/one.md)\r\n", role: "include" },
  { path: "two.md", source: "# Two\n[One](guide/one.md)\n" }];
function output(files = renamed, oldPath = "one.md", newPath = "guide/one.md", count = 3, edits = 3) {
  const report = { schema: "fmd-book-source-rename-v1", oldPath, newPath,
    files: files.filter(file => file.role !== "include").map(({ path, source }) => ({ path, source })),
    includeSources: files.filter(file => file.role === "include").map(({ path, source }) => ({ path, source })),
    changedSourceCount: count, editCount: edits };
  return { bytes: new TextEncoder().encode(JSON.stringify(report)), report };
}

function fixture(t, nativeResult = () => Promise.resolve(output())) {
  const root = { elements: {}, querySelector(selector) { return this.elements[selector.slice(1)] ?? null; },
    createElement(tag) { return new Element(this, tag); } };
  const section = root.createElement("section"), title = root.createElement("h2"); title.id = "arrange-title"; section.appendChild(title);
  for (const id of ["chapter-source", "chapter-path", "chapters", "source-role"]) {
    const el = root.createElement("input"); el.id = id; section.appendChild(el);
  }
  const collection = createBookCollection();
  collection.append({ chapters: initial.map(file => ({ ...file })), images: [{ destination: "plot.svg", bytes: Uint8Array.of(1, 2) }] });
  collection.setFonts([{ slot: "body-regular", name: "owned.ttf", bytes: Uint8Array.of(3, 4) }], collection.revision);
  collection.configure({ ...collection.options, title: "Kept", profile: "commonmark-gfm" });
  const history = createBookSourceHistory(collection.files, undefined, { trackStructure: true });
  let active = 0, busy = false, writes = 0, failApply = false, afterApply = null;
  const busyListeners = new Set(), calls = []; let cancels = 0, disposals = 0;
  const node = id => root.elements[id];
  const show = () => {
    const file = collection.files[active];
    node("chapter-path").value = file.path; node("chapter-source").value = file.source.replace(/\r\n?/g, "\n");
    node("source-role").value = file.role ?? "chapter";
  }; show();
  const checkpoint = () => JSON.stringify([collection.revision, active, node("chapter-path").value,
    node("chapter-source").value, node("source-role").value]);
  const controls = {
    get currentChapter() { return active; }, get sourceBusy() { return busy; }, checkpoint,
    subscribeSourceState(fn) { busyListeners.add(fn); return () => busyListeners.delete(fn); },
    captureProject() {
      if (busy) throw Object.assign(Error("busy"), { code: "BOOK_BUSY" });
      const file = collection.files[active];
      if (node("chapter-path").value !== file.path || node("chapter-source").value !== file.source.replace(/\r\n?/g, "\n"))
        throw Object.assign(Error("uncaptured edit"), { code: "STALE_SOURCE" });
      return collection.project();
    },
    applySourceTree(files, stamp) {
      assert.equal(stamp, checkpoint());
      if (failApply) throw Object.assign(Error("host refused"), { code: "HOST_REFUSAL" });
      const revision = collection.replaceSourceTree(files, collection.revision); writes++;
      history.record(collection.files, { label: "Rename source" }); show(); afterApply?.(); return revision;
    },
  };
  const worker = { renameSource(...args) { calls.push(args); return nativeResult(...args); },
    cancel() { cancels++; }, dispose() { disposals++; } };
  let imageKeyOverride;
  const sourceCollection = new Proxy(collection, { get(target, key, receiver) {
    if (["images", "fonts", "snapshot"].includes(key)) assert.fail("rename accessed resource bytes or size metadata: " + key);
    if (key === "imageDestinations" && imageKeyOverride !== undefined) return imageKeyOverride;
    return Reflect.get(target, key, receiver);
  } });
  const manager = createBookSourceRenameControls({ root, controls, collection: sourceCollection, worker });
  node("book-rename-path").value = "guide/one.md"; node("book-rename-path").emit("input");
  t.after(() => { manager.dispose(); history.dispose(); collection.dispose(); });
  return { root, node, manager, controls, collection, history, calls,
    get writes() { return writes; }, get cancels() { return cancels; }, get disposals() { return disposals; },
    busy(value) { busy = value; for (const fn of busyListeners) fn(); },
    select(index) { active = index; show(); node("chapters").emit("change"); },
    rejectApply(value) { failApply = value; }, afterApply(fn) { afterApply = fn; }, show,
    imageKeys(value) { imageKeyOverride = value; },
  };
}

test("rename panel is installed once, uses text paths and requires a prepared plan", async t => {
  const f = fixture(t), parent = f.node("arrange-title").parentElement;
  const count = parent.children.length; createBookSourceRenamePanel(f.root);
  assert.equal(parent.children.length, count); assert.equal(f.node("book-rename-status").attrs.role, "status");
  assert.equal(f.node("book-rename-apply").disabled, true);
  assert.throws(() => f.manager.apply(), error("STALE_SOURCE"));
  const result = await f.manager.prepare();
  assert.equal(result.editCount, 3); assert.equal(f.writes, 0);
  assert.equal(f.node("book-rename-changes").children.length, 3);
  assert.match(f.node("book-rename-changes").children[0].textContent, /one.md → guide\/one.md/);
  assert.equal(f.calls[0][3].profile, "commonmark-gfm");
  assert.deepEqual(f.calls[0][3].imageDestinations, ["plot.svg"]);
  assert(Object.isFrozen(f.calls[0][3].imageDestinations));
  assert.deepEqual(f.calls[0][3].images, []); assert.deepEqual(f.calls[0][3].fontAssets, []);
});

test("collection exposes only immutable image keys to source rename", t => {
  const f = fixture(t), keys = f.collection.imageDestinations;
  assert.deepEqual(keys, ["plot.svg"]); assert(Object.isFrozen(keys));
  assert.throws(() => keys.push("forged.svg"), TypeError);
  f.collection.append({ chapters: [], images: [{ destination: "other.svg", bytes: Uint8Array.of(5, 6) }] });
  assert.deepEqual(keys, ["plot.svg"]);
  assert.deepEqual(f.collection.imageDestinations, ["plot.svg", "other.svg"]);
});

test("adding or revoking image keys invalidates pending and prepared source renames", async t => {
  const gate = deferred(), f = fixture(t, () => gate.promise), pending = f.manager.prepare();
  f.collection.append({ chapters: [], images: [{ destination: "guide/plot.svg", bytes: Uint8Array.of(5) }] });
  gate.resolve(output()); assert.equal(await pending, null); assert.equal(f.writes, 0);
  assert.deepEqual(f.collection.files, initial); assert.equal(f.node("book-rename-apply").disabled, true);
  await f.manager.prepare(); assert.deepEqual(f.calls[1][3].imageDestinations, ["plot.svg", "guide/plot.svg"]);
  f.collection.revokeImages();
  assert.throws(() => f.manager.apply(), error("STALE_SOURCE")); assert.equal(f.writes, 0);
  assert.deepEqual(f.collection.files, initial);
});

test("a silently changed key inventory is fenced independently of source checkpoints", async t => {
  for (const stage of ["pending", "prepared"]) {
    const gate = deferred(), f = fixture(t, () => gate.promise), pending = f.manager.prepare();
    const checkpoint = f.controls.checkpoint();
    if (stage === "prepared") { gate.resolve(output()); await pending; }
    f.imageKeys(["plot.svg", "guide/plot.svg"]);
    assert.equal(f.controls.checkpoint(), checkpoint);
    if (stage === "pending") { gate.resolve(output()); assert.equal(await pending, null); }
    else assert.throws(() => f.manager.apply(), error("STALE_SOURCE"));
    assert.equal(f.writes, 0); assert.deepEqual(f.collection.files, initial);
  }
});

test("apply changes all source references in one revision and history restores source structure without resource loss", async t => {
  const f = fixture(t), before = f.collection.snapshot(), revision = f.collection.revision;
  const presentation = value => ({ ...value.options, includeSources: [] });
  await f.manager.prepare();
  assert.deepEqual(f.collection.files, initial); assert.equal(f.collection.revision, revision);
  assert.equal(f.manager.apply(), revision + 1); assert.equal(f.writes, 1);
  assert.deepEqual(f.collection.files, renamed); assert.equal(f.history.state.undo, 1);
  assert.deepEqual(presentation(f.collection.snapshot()), presentation(before));
  const undo = f.history.plan("undo");
  f.collection.replaceSourceTree(undo.files, f.collection.revision); f.history.commit(undo, f.collection.files); f.show();
  assert.deepEqual(f.collection.files, initial); assert.deepEqual(presentation(f.collection.snapshot()), presentation(before));
  const redo = f.history.plan("redo");
  f.collection.replaceSourceTree(redo.files, f.collection.revision); f.history.commit(redo, f.collection.files); f.show();
  assert.deepEqual(f.collection.files, renamed); assert.deepEqual(presentation(f.collection.snapshot()), presentation(before));
});

test("include-only source moves retain their role and position in the mixed editor tree", async t => {
  const next = initial.map(file => ({ ...file, ...(file.path === "note.txt" ? { path: "parts/note.txt",
    source: "[One](../one.md)\r\n" } : {}) }));
  const f = fixture(t, () => Promise.resolve(output(next, "note.txt", "parts/note.txt", 1, 1)));
  f.select(1); f.node("book-rename-path").value = "parts/note.txt";
  await f.manager.prepare(); f.manager.apply();
  assert.deepEqual(f.collection.files, next); assert.equal(f.collection.files[1].role, "include");
  assert.deepEqual(f.calls[0].slice(1, 3), ["note.txt", "parts/note.txt"]);
});

test("source edits cancel pending refactors and stale responses cannot overwrite a fresh plan", async t => {
  const first = deferred(), second = deferred(); let request = 0;
  const f = fixture(t, () => ++request === 1 ? first.promise : second.promise);
  const pending = f.manager.prepare();
  f.collection.edit(1, "note.txt", "changed"); f.show();
  assert.equal(f.node("book-rename-apply").disabled, true); assert(f.cancels >= 2);
  // Restore exact prior bytes; revision ownership still invalidates the first plan.
  f.collection.edit(1, "note.txt", initial[1].source); f.show();
  const retry = f.manager.prepare(); const current = f.node("book-rename-status").textContent;
  first.resolve(output()); assert.equal(await pending, null);
  assert.equal(f.node("book-rename-status").textContent, current); assert.equal(f.writes, 0);
  second.resolve(output()); await retry; f.manager.apply(); assert.deepEqual(f.collection.files, renamed);
});

for (const kind of ["source", "selected path", "destination"]) {
  test(`silent ${kind} changes cannot apply an older completed plan`, async t => {
    const f = fixture(t); await f.manager.prepare();
    const id = kind === "source" ? "chapter-source" : kind === "selected path" ? "chapter-path" : "book-rename-path";
    f.node(id).value += "new"; // deliberately no DOM event
    assert.throws(() => f.manager.apply(), error("STALE_SOURCE"));
    assert.equal(f.writes, 0); assert.deepEqual(f.collection.files, initial);
  });
}

test("silent source changes during native execution and identical project recovery discard the result", async t => {
  for (const kind of ["source", "project"]) {
    const gate = deferred(), f = fixture(t, () => gate.promise), pending = f.manager.prepare();
    if (kind === "source") f.node("chapter-source").value += "silent";
    else f.collection.replaceProject(f.collection.project());
    gate.resolve(output()); assert.equal(await pending, null);
    assert.equal(f.writes, 0); assert.equal(f.node("book-rename-apply").disabled, true);
  }
});

test("source and destination composition remain fenced through completion and permit a fresh retry", async t => {
  const f = fixture(t); await f.manager.prepare();
  f.busy(true); assert.equal(f.node("book-rename-apply").disabled, true);
  await assert.rejects(f.manager.prepare(), error("BOOK_BUSY"));
  // Source host still owns its queued composition completion.
  await tick(); await assert.rejects(f.manager.prepare(), error("BOOK_BUSY"));
  f.busy(false);
  f.node("book-rename-path").emit("compositionstart");
  await assert.rejects(f.manager.prepare(), error("BOOK_BUSY"));
  f.node("book-rename-path").emit("compositionend");
  await assert.rejects(f.manager.prepare(), error("BOOK_BUSY"));
  await tick(); await f.manager.prepare(); f.manager.apply(); assert.equal(f.writes, 1);
});

test("Cancel, suspension and disposal retire work without touching source or unrelated operations", async t => {
  for (const kind of ["cancel", "suspend", "dispose"]) {
    const gate = deferred(), f = fixture(t, () => gate.promise), before = f.collection.snapshot();
    const pending = f.manager.prepare(); f.manager[kind]();
    gate.resolve(output()); assert.equal(await pending, null);
    assert.equal(f.writes, 0); assert.deepEqual(f.collection.snapshot(), before);
    if (kind === "dispose") assert.equal(f.disposals, 1);
    if (kind === "suspend") { f.manager.resume(); assert.equal(f.node("book-rename-prepare").disabled, false); }
  }
});

test("native failures, invalid reports and refused host writes retain the complete original collection", async t => {
  for (const nativeResult of [() => Promise.reject(Object.assign(Error("old engine"), { code: "UNSUPPORTED_BOOK_SOURCE_RENAME" })),
    () => Promise.resolve(output(renamed.slice(1))), () => Promise.resolve(output(renamed, "two.md"))]) {
    const f = fixture(t, nativeResult), before = f.collection.snapshot();
    await assert.rejects(f.manager.prepare()); assert.deepEqual(f.collection.snapshot(), before); assert.equal(f.writes, 0);
  }
  const f = fixture(t), before = f.collection.snapshot(); await f.manager.prepare(); f.rejectApply(true);
  assert.throws(() => f.manager.apply(), error("HOST_REFUSAL")); assert.deepEqual(f.collection.snapshot(), before);
  f.rejectApply(false); await f.manager.prepare(); f.manager.apply(); assert.equal(f.writes, 1);
});

test("a Worker-compatible host cannot rewrite private source identities while capturing the request", async t => {
  const f = fixture(t, files => {
    files[0].path = "forged.md";
    const forged = renamed.map(file => ({ ...file })); forged[0].path = "forged.md";
    return Promise.resolve(output(forged));
  });
  await assert.rejects(f.manager.prepare(), error("INVALID_BOOK_SOURCE_RENAME"));
  assert.deepEqual(f.collection.files, initial); assert.equal(f.writes, 0);
});

test("a peer edit after atomic application is preserved and reported instead of being rolled back", async t => {
  const f = fixture(t); await f.manager.prepare();
  f.afterApply(() => f.collection.edit(2, "two.md", "peer source"));
  f.manager.apply(); assert.equal(f.collection.files[2].source, "peer source");
  assert.equal(f.collection.files[0].path, "guide/one.md");
  assert.match(f.node("book-rename-status").textContent, /applied, then the book changed again/);
});
