import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookImageControls, createBookImagePanel } from "../demo/book_image_controls.mjs";

class Element extends EventTarget {
  constructor(root, tag) { super(); this.root = root; this.tagName = tag; this.attrs = {}; this.children = []; this._value = ""; this.files = []; this.disabled = false; }
  set id(value) { this._id = value; this.root.elements[value] = this; }
  get id() { return this._id; }
  set value(value) { this._value = value; if (this.type === "file" && value === "") this.files = []; }
  get value() { return this._value; }
  set innerHTML(_) { assert.fail("image management must never parse HTML"); }
  setAttribute(key, value) { this.attrs[key] = value; }
  appendChild(child) { this.children.push(child); child.parentElement = this; return child; }
  replaceChildren(...children) { this.children = []; children.forEach(child => this.appendChild(child)); }
  emit(type) { this.dispatchEvent(new Event(type)); }
}
function fixture({ confirm = () => true, timeoutMs = 10000 } = {}) {
  const root = { elements: {}, querySelector(selector) { return this.elements[selector.slice(1)] ?? null; }, createElement(tag) { return new Element(this, tag); } };
  const title = root.createElement("h2"); title.id = "arrange-title";
  root.createElement("section").appendChild(title);
  const source = root.createElement("textarea"); source.id = "chapter-source"; source.value = "# Book\n![a](images/a.png)";
  const book = createBookCollection();
  book.append({ chapters: [{ path: "book.md", source: source.value }], images: [
    { destination: "images/a.png", bytes: new Uint8Array([1, 2]) },
    { destination: "b.jpeg", bytes: new Uint8Array([3, 4]) },
    { destination: "map.svg", bytes: new Uint8Array([5, 6]) },
  ] });
  let busy = false; const listeners = new Set();
  const controls = {
    get sourceBusy() { return busy; },
    captureProject() { book.edit(0, "book.md", source.value); return book.project(); },
    checkpoint() { return JSON.stringify([book.revision, source.value]); },
    subscribeSourceState(fn) { listeners.add(fn); return () => listeners.delete(fn); },
  };
  createBookImagePanel(root);
  const manager = createBookImageControls({ root, collection: book, controls, confirm, timeoutMs });
  const el = root.elements;
  return { book, controls, manager, el, root, listeners,
    select(key) { el["book-image-selection"].value = key; el["book-image-selection"].emit("change"); },
    choose(name = "new.png", data = [9, 8, 7]) {
      const file = new File([Uint8Array.from(data)], name);
      el["book-image-file"].files = [file]; el["book-image-file"].emit("change"); return file;
    },
    busy(value) { busy = value; for (const fn of listeners) fn(); },
  };
}
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const next = () => new Promise(resolve => setImmediate(resolve));

test("panel has explicit controls, bounded file selection and safe text-only inventory", () => {
  const f = fixture();
  createBookImagePanel(f.root);
  assert.equal(f.el["arrange-title"].parentElement.children.length, 2);
  assert.equal(f.el["book-image-selection"].children.length, 3);
  assert.equal(f.el["book-image-replace"].disabled, true);
  assert.equal(f.el["book-image-remove"].disabled, false);
  assert.equal(f.el["book-image-cancel"].disabled, true);
  assert.equal(f.el["book-image-status"].attrs.role, "status");
  assert.match(f.el["book-image-help"].textContent, /without editing Markdown/);
  f.manager.dispose();
});

test("replacement keeps the bound path, other images, exact source and page/font settings", async () => {
  const f = fixture();
  f.book.setFonts([{ slot: "body-regular", name: "font.ttf", bytes: new Uint8Array([1]) }], f.book.revision);
  f.book.setPage({ size: "a4" }, f.book.revision);
  const before = f.book.snapshot();
  f.choose();
  const revision = f.book.revision;
  assert.equal(await f.manager.replace(), revision + 1);
  const after = f.book.snapshot();
  assert.deepEqual(after.files, before.files);
  assert.deepEqual({ ...after.options, images: [] }, { ...before.options, images: [] });
  assert.equal(after.options.images[0].destination, "images/a.png");
  assert.deepEqual([...after.options.images[0].bytes], [9, 8, 7]);
  assert.deepEqual(after.options.images.slice(1), before.options.images.slice(1));
  assert.equal(f.el["book-image-file"].files.length, 0);
  assert.match(f.el["book-image-status"].textContent, /replaced at the same path/);
  f.manager.dispose();
});

test("identical replacements retain configuration revision and do not notify subscribers", async () => {
  const f = fixture(); f.choose("a.png", [1, 2]);
  const revision = f.book.revision, config = f.book.renderConfigurationRevision;
  const off = f.book.subscribe(() => assert.fail("no-op emitted change"));
  assert.equal(await f.manager.replace(), revision);
  assert.equal(f.book.renderConfigurationRevision, config);
  assert.match(f.el["book-image-status"].textContent, /identical/);
  off(); f.manager.dispose();
});

test("source capture preserves silently typed editor content before image mutation", async () => {
  const f = fixture(); f.choose();
  f.el["chapter-source"].value = "new editor text";
  await f.manager.replace();
  assert.equal(f.book.files[0].source, "new editor text");
  assert.deepEqual([...f.book.snapshot().options.images[0].bytes], [9, 8, 7]);
  f.manager.dispose();
});

test("confirmed removal leaves source references and other resource bindings intact", async () => {
  let message;
  const f = fixture({ confirm: text => { message = text; return true; } });
  const before = f.book.files;
  await f.manager.remove();
  assert.match(message, /images\/a.png/);
  assert.deepEqual(f.book.files, before);
  assert.deepEqual(f.book.images.map(image => image.destination), ["b.jpeg", "map.svg"]);
  assert.equal(f.el["book-image-selection"].value, "b.jpeg");
  f.manager.dispose();
});

test("declined or absent confirmation never removes resources", async () => {
  for (const confirm of [() => false, null, () => "true"]) {
    const f = fixture({ confirm }), before = f.book.snapshot();
    if (confirm === null) await assert.rejects(f.manager.remove(), { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
    else assert.equal(await f.manager.remove(), false);
    assert.deepEqual(f.book.snapshot(), before); f.manager.dispose();
  }
});

test("file reading failures, empty files and cross-format replacements preserve the old image", async () => {
  const f = fixture(), before = f.book.snapshot();
  for (const [name, data] of [["wrong.svg", [1]], ["bad.md", [1]], ["empty.png", []]]) {
    f.choose(name, data); await assert.rejects(f.manager.replace());
    assert.deepEqual(f.book.snapshot(), before);
  }
  const bad = f.choose(); bad.arrayBuffer = async () => { throw Error("I/O failure"); };
  await assert.rejects(f.manager.replace(), /I\/O failure/);
  assert.deepEqual(f.book.snapshot(), before);
  const changedSize = f.choose(); changedSize.arrayBuffer = async () => new ArrayBuffer(1);
  await assert.rejects(f.manager.replace(), { code: "FILE_READ_FAILED" });
  assert.deepEqual(f.book.snapshot(), before);
  f.select("b.jpeg"); f.choose("ok.jpg"); await f.manager.replace();
  assert.deepEqual([...f.book.snapshot().options.images[1].bytes], [9, 8, 7]);
  f.manager.dispose();
});

test("per-file and final-set size admission occur before reading", async () => {
  const f = fixture();
  f.book.revokeImages();
  f.book.append({ chapters: [], images: [0, 1, 2, 3].map(i => ({ destination: `${i}.png`, bytes: new Uint8Array(8 * 1024 * 1024 - 1) })) });
  f.book.append({ chapters: [], images: [{ destination: "small.png", bytes: new Uint8Array([1]) }] });
  f.select("small.png");
  let reads = 0;
  const file = f.choose();
  file.arrayBuffer = () => { reads++; throw Error("must not read"); };
  Object.defineProperty(file, "size", { value: 100 });
  await assert.rejects(f.manager.replace(), { code: "BOOK_LIMIT" });
  assert.equal(reads, 0);
  f.select("0.png"); const huge = f.choose();
  Object.defineProperty(huge, "size", { value: 8 * 1024 * 1024 + 1 });
  huge.arrayBuffer = () => { reads++; throw Error("must not read"); };
  await assert.rejects(f.manager.replace(), { code: "FILE_LIMIT" });
  assert.equal(reads, 0); f.manager.dispose();
});

test("cancel settles an uncooperative read and observes its late failure", async () => {
  const f = fixture(), read = deferred();
  f.choose().arrayBuffer = () => read.promise;
  const pending = f.manager.replace();
  assert.equal(f.manager.pending, true);
  f.manager.cancel();
  await assert.rejects(pending, { code: "IMAGE_CANCELLED" });
  read.reject(Error("late failure")); await next();
  assert.deepEqual([...f.book.snapshot().options.images[0].bytes], [1, 2]);
  f.manager.dispose();
});

test("late cancelled reads cannot replace a newer successful selection", async () => {
  const f = fixture(), read = deferred();
  f.choose().arrayBuffer = () => read.promise;
  const old = f.manager.replace(); f.manager.cancel();
  await assert.rejects(old, { code: "IMAGE_CANCELLED" });
  f.choose("second.png", [10]); await f.manager.replace();
  read.resolve(new Uint8Array([9, 8, 7]).buffer); await next();
  assert.deepEqual([...f.book.snapshot().options.images[0].bytes], [10]); f.manager.dispose();
});

test("raw source, selected binding and selected file are rechecked after reading", async () => {
  for (const what of ["source", "destination", "file"]) {
    const f = fixture(), read = deferred(), original = f.book.snapshot();
    f.choose().arrayBuffer = () => read.promise;
    const pending = f.manager.replace();
    if (what === "source") f.el["chapter-source"].value = "silent change";
    if (what === "destination") f.el["book-image-selection"].value = "b.jpeg";
    if (what === "file") f.el["book-image-file"].files = [new File(["x"], "new.png")];
    read.resolve(new Uint8Array([9, 8, 7]).buffer);
    await assert.rejects(pending, { code: "STALE_SOURCE" });
    assert.deepEqual(f.book.snapshot(), original); f.manager.dispose();
  }
});

test("configuration changes invalidate asynchronous removal confirmation", async () => {
  const approval = deferred(), f = fixture({ confirm: () => approval.promise });
  const pending = f.manager.remove();
  f.book.setPage({ size: "a4" }, f.book.revision);
  await assert.rejects(pending, { code: "IMAGE_CANCELLED" });
  approval.resolve(true); await next();
  assert.equal(f.book.images.length, 3); f.manager.dispose();
});

test("composition/import, suspension and disposal cancel only the local operation", async () => {
  for (const action of ["busy", "suspend", "dispose", "input"]) {
    const f = fixture(), read = deferred(); f.choose().arrayBuffer = () => read.promise;
    const pending = f.manager.replace();
    if (action === "busy") f.busy(true);
    else if (action === "input") f.el["chapter-source"].emit("input");
    else f.manager[action]();
    await assert.rejects(pending, { code: "IMAGE_CANCELLED" });
    assert.equal(f.book.images.length, 3);
    if (action === "suspend") { assert.equal(f.el["book-image-file"].files.length, 0); f.manager.resume(); }
    if (action === "busy") { await assert.rejects(f.manager.replace(), { code: "BOOK_BUSY" }); f.busy(false); }
    read.reject(Error("late")); await next(); f.manager.dispose(); assert.equal(f.listeners.size, 0);
  }
});

test("timeout retires a hanging confirmation and permits explicit retry", async () => {
  const approval = deferred(), f = fixture({ confirm: () => approval.promise, timeoutMs: 5 });
  await assert.rejects(f.manager.remove(), { code: "IMAGE_CANCELLED" });
  assert.equal(f.manager.pending, false);
  assert.match(f.el["book-image-status"].textContent, /timed out/);
  approval.resolve(true); await next();
  assert.equal(f.book.images.length, 3); f.manager.dispose();
});

test("reentrant cancellation during committed notification does not report rollback", async () => {
  const f = fixture(); f.choose();
  const off = f.book.subscribe(() => f.manager.cancel());
  const before = f.book.revision;
  assert.equal(await f.manager.replace(), before + 1);
  assert.match(f.el["book-image-status"].textContent, /replaced at the same path/);
  off(); f.manager.dispose();
});

test("empty resources disable actions, while restored resources refresh the inventory", () => {
  const f = fixture(); f.book.revokeImages();
  assert.equal(f.el["book-image-replace"].disabled, true);
  assert.equal(f.el["book-image-remove"].disabled, true);
  f.book.append({ chapters: [], images: [{ destination: "restored.svg", bytes: new Uint8Array([5]) }] });
  assert.equal(f.el["book-image-selection"].value, "restored.svg");
  assert.equal(f.el["book-image-remove"].disabled, false); f.manager.dispose();
});
