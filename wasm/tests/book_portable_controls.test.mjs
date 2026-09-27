import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookControls } from "../demo/book_controls.mjs";

// Production controller/collection/codec; only DOM, URL ownership and the
// publication worker are doubles. No native rendering is claimed here.
const IDS = "source-role add-include import-includes import-include-folder chapters chapter-path chapter-source add-chapter move-up move-down remove-chapter import-files import-folder open-project save-project save-chapter revoke-images image-list title author lang font dark-mode font-scale toc page-numbers export-pdf export-epub export-site cancel-export download status publish-title".split(" ");
class Element extends EventTarget {
  constructor(root, tag = "input") { super(); this.root = root; this.tagName = tag; this.children = []; this.attrs = {}; this._value = ""; this.checked = false; this.disabled = false; this.hidden = false; this.files = []; }
  set id(value) { this._id = value; this.root.elements[value] = this; }
  get id() { return this._id; }
  set value(value) { this._value = this.tagName === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
  get value() { return this._value; }
  set innerHTML(_) { assert.fail("portable UI must not parse HTML"); }
  setAttribute(key, value) { this.attrs[key] = String(value); }
  getAttribute(key) { return this.attrs[key] ?? null; }
  removeAttribute(key) { delete this.attrs[key]; delete this[key]; }
  append(...nodes) { for (const node of nodes) { node.parentElement = this; this.children.push(node); } }
  replaceChildren(...nodes) { this.children = []; this.append(...nodes); }
  emit(type) { const event = new Event(type, { cancelable: true }); this.dispatchEvent(event); return event; }
  click() { return this.disabled ? null : this.emit("click"); }
  focus() {}
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
}
function book(prefix = "old") {
  const value = createBookCollection();
  value.append({
    chapters: [{ path: `${prefix}.md`, source: `\ufeff# ${prefix}\r\n\r\n![x](x.png)\n` }],
    includeSources: [{ path: `${prefix}.txt`, source: "shared\r\n" }],
    images: [{ destination: "x.png", bytes: new Uint8Array([1, 2, 255]) }],
  });
  value.setFonts([{ slot: "body-bold", name: `${prefix}.ttf`, weight: 650, bytes: new Uint8Array(70001).fill(37) }], value.revision);
  return value;
}
function fixture({ confirm = () => true, omitConfirm = false, panel = true } = {}) {
  const root = { elements: {}, querySelector(selector) { return this.elements[selector.slice(1)] ?? null; }, createElement(tag) { return new Element(this, tag); } };
  for (const id of IDS) { if (id === "publish-title" && !panel) continue; const node = root.createElement(id === "chapter-source" ? "textarea" : "input"); node.id = id; }
  if (panel) root.createElement("section").append(root.elements["publish-title"]);
  const blobs = new Map(), revoked = [], exports = [], collection = book();
  let serial = 0, replaced = 0;
  const controls = createBookControls({
    root, collection, ...(omitConfirm ? {} : { confirm }),
    urls: { createObjectURL(blob) { const key = `blob:test/${++serial}`; blobs.set(key, blob); return key; }, revokeObjectURL(key) { revoked.push(key); blobs.delete(key); } },
    worker: { cancel() {}, dispose() {}, async render(files, format, options) {
      exports.push({ files, format, options });
      return { extension: "pdf", blob: () => new Blob(["recording worker, not a rendered PDF"]) };
    } },
    onProjectReplaced() { replaced++; },
  });
  return { root, el: root.elements, controls, collection, blobs, revoked, exports, get replaced() { return replaced; } };
}
async function portableFile() {
  const source = book("new"), output = await source.portableDownload();
  return { source, file: new File([output.blob], output.filename) };
}
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
async function until(predicate) {
  for (let i = 0; i < 100 && !predicate(); i++) await tick();
  assert(predicate(), "operation did not reach its expected state");
}

test("publisher installs accessible opt-in controls without a new package entry", () => {
  const f = fixture();
  assert.match(f.el["save-portable"].textContent, /includes images and fonts/);
  assert.match(f.el["portable-help"].textContent, /unencrypted/);
  assert.equal(f.el["open-portable"].type, "file");
  assert.equal(f.el["open-portable"].getAttribute("aria-describedby"), "portable-help portable-review status");
  assert.equal(f.el["cancel-portable"].disabled, true);
  assert.equal(f.el.download.hidden, true);
  const host = fixture({ panel: false }); // Existing embedding surfaces stay compatible.
  assert.equal(host.el["save-portable"], undefined);
  assert.equal(typeof host.controls.preparePortable, "function");
});

test("portable button captures editor state and publishes an explicit download without rendering", async () => {
  const f = fixture();
  f.el["chapter-source"].value = "typed without an input event";
  f.el["save-portable"].click();
  await until(() => !f.el.download.hidden);
  assert.equal(f.exports.length, 0);
  assert.equal(f.el.download.download, "book.fmdbook.bundle.json");
  const saved = JSON.parse(await f.blobs.get(f.el.download.href).text());
  assert.equal(saved.project.files[0].source, "typed without an input event");
  assert.equal(saved.images.length, 1);
  assert.equal(saved.fontAssets[0].weight, 650);
  assert.match(f.el.status.textContent, /Preparation alone has not saved/);
  assert.equal(f.el.download.click().defaultPrevented, false);
});

test("raw editor changes during preparation or before download never publish stale bytes", async () => {
  const f = fixture(), pending = f.controls.preparePortable();
  f.el["chapter-source"].value = "new raw editor value";
  await assert.rejects(pending, { code: "STALE_SOURCE" });
  assert.equal(f.blobs.size, 0);
  assert.equal(f.el["chapter-source"].value, "new raw editor value");
  await f.controls.preparePortable();
  const previous = f.el.download.href;
  f.el.title.value = "changed silently";
  assert.equal(f.el.download.click().defaultPrevented, true);
  assert(f.revoked.includes(previous));
  assert.equal(f.blobs.size, 0);
});

test("confirmed restore updates the editor, detaches recovery and feeds resources to publication", async () => {
  const input = await portableFile();
  const messages = [], f = fixture({ confirm: message => { messages.push(message); return true; } });
  const before = f.collection.revision;
  assert.equal(await f.controls.importFiles([input.file], "portable"), true);
  assert.equal(f.collection.revision, before + 1);
  assert.equal(f.controls.currentChapter, 0);
  assert.equal(f.el["chapter-path"].value, "new.md");
  assert.equal(f.el["chapter-source"].value, input.source.files[0].source.replace(/\r\n?/g, "\n"));
  assert.deepEqual(f.collection.snapshot(), input.source.snapshot());
  assert.equal(f.replaced, 1);
  assert.match(messages[0], /authorize 1 embedded images.*1 embedded font roles/);
  assert.equal(f.controls.sourceBusy, false);
  await f.controls.prepare("pdf");
  const expected = input.source.snapshot();
  assert.deepEqual(f.exports[0], { files: expected.files, format: "pdf", options: expected.options });
});

test("decline and missing confirmation handler preserve sources, images and fonts", async () => {
  const input = await portableFile();
  for (const omitConfirm of [false, true]) {
    const f = fixture({ confirm: () => false, omitConfirm });
    const before = f.collection.snapshot(), revision = f.collection.revision;
    const pending = f.controls.importFiles([input.file], "portable");
    if (omitConfirm) await assert.rejects(pending, { code: "RESOURCE_AUTHORIZATION_REQUIRED" });
    else assert.equal(await pending, false);
    assert.deepEqual(f.collection.snapshot(), before);
    assert.equal(f.collection.revision, revision);
    assert.equal(f.replaced, 0);
  }
});

test("resource review stays text-only and edits during asynchronous approval fence installation", async () => {
  const input = await portableFile();
  for (const event of [false, true]) {
    let approve;
    const f = fixture({ confirm: () => new Promise(resolve => { approve = resolve; }) });
    const pending = f.controls.importFiles([input.file], "portable");
    await until(() => approve);
    assert.match(f.el["portable-review"].textContent, /new.md/);
    assert.match(f.el["portable-review"].textContent, /body-bold: new.ttf, weight 650/);
    f.el["chapter-source"].value = "newer editor text";
    if (event) f.el["chapter-source"].emit("input");
    approve(true);
    await assert.rejects(pending, error => ["ABORTED", "STALE_SOURCE"].includes(error.code));
    assert.equal(f.el["chapter-source"].value, "newer editor text");
    assert.equal(f.collection.files[0].path, "old.md");
    assert.equal(f.collection.fonts[0].name, "old.ttf");
    assert.equal(f.replaced, 0);
  }
});

test("cancel releases a pending file read and observes its late rejection", async () => {
  const f = fixture(), input = new File(["x"], "slow.json");
  let rejectRead;
  input.arrayBuffer = () => new Promise((_, reject) => { rejectRead = reject; });
  const pending = f.controls.importFiles([input], "portable");
  assert.equal(f.controls.sourceBusy, true);
  assert.equal(f.el["cancel-portable"].disabled, false);
  f.el["cancel-portable"].click();
  await assert.rejects(pending, { code: "ABORTED" });
  assert.equal(f.controls.sourceBusy, false);
  assert.equal(f.replaced, 0);
  rejectRead(new Error("late file failure"));
  await tick();
});

test("cancel, suspension and disposal retire asynchronous approval without late restore", async () => {
  const input = await portableFile();
  for (const action of ["cancel", "suspend", "dispose"]) {
    let settle;
    const f = fixture({ confirm: () => new Promise((_, reject) => { settle = reject; }) });
    const pending = f.controls.importFiles([input.file], "portable");
    await until(() => settle);
    if (action === "cancel") f.el["cancel-portable"].click();
    else f.controls[action]();
    await assert.rejects(pending, { code: "ABORTED" });
    settle(new Error("late rejected confirmation"));
    await tick();
    assert.equal(f.replaced, 0);
    assert.equal(f.blobs.size, 0);
    if (action === "suspend") {
      assert.equal(f.collection.files[0].path, "old.md");
      assert.equal(f.collection.images.length, 0);
      assert.equal(f.collection.fonts.length, 0);
    }
  }
});

test("composition blocks portable work and cancels an in-flight copy", async () => {
  const f = fixture(), input = await portableFile();
  f.el["chapter-source"].emit("compositionstart");
  await assert.rejects(f.controls.preparePortable(), { code: "BOOK_BUSY" });
  await assert.rejects(f.controls.importFiles([input.file], "portable"), { code: "BOOK_BUSY" });
  assert.equal(f.el["save-portable"].disabled, true);
  f.el["chapter-source"].emit("compositionend");
  const pending = f.controls.preparePortable();
  f.el["chapter-source"].emit("compositionstart");
  await assert.rejects(pending, { code: "ABORTED" });
  assert.equal(f.blobs.size, 0);
});

test("resource changes revoke downloads and default source saves still exclude resources", async () => {
  const f = fixture();
  await f.controls.preparePortable();
  const previous = f.el.download.href;
  f.collection.revokeFonts();
  assert(f.revoked.includes(previous));
  assert.equal(f.el.download.hidden, true);
  f.controls.prepareSource(true);
  const source = JSON.parse(await f.blobs.get(f.el.download.href).text());
  assert.equal(f.el.download.download, "book.fmdbook.json");
  assert.equal(source.images, undefined);
  assert.equal(source.fontAssets, undefined);
});

test("file-picker event uses the portable path and clears the selected file", async () => {
  const f = fixture(), input = await portableFile();
  f.el["open-portable"].files = [input.file];
  f.el["open-portable"].value = "fake-selected-path";
  f.el["open-portable"].emit("change");
  await until(() => f.replaced === 1);
  assert.equal(f.el["open-portable"].value, "");
  assert.deepEqual(f.collection.snapshot(), input.source.snapshot());
});
