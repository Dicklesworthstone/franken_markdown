import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookFileControls } from "../demo/book_file_controls.mjs";
import { createBookSourceFolderControls } from "../demo/book_source_folder_controls.mjs";
import { Element } from "./book_source_archive_helpers.mjs";
import { codec, sample, exportedFolder, deferred } from "./book_source_folder_helpers.mjs";

const tick = () => new Promise(resolve => setImmediate(resolve));
function fixture(t, { confirm = () => true, onOpened = () => {}, timeoutMs } = {}) {
  const root = new Element("document"), window = new Element("window");
  root.createElement = tag => new Element(tag);
  root.createTextNode = text => Object.assign(new Element("text"), { textContent: text });
  const section = root.appendChild(new Element("section")); section.setAttribute("aria-labelledby", "publish-title");
  const html = readFileSync(new URL("../demo/book.html", import.meta.url), "utf8");
  for (const [, id] of html.matchAll(/id="([^"]+)"/g)) section.appendChild(new Element()).id = id;
  const el = id => root.querySelector(`#${id}`), collection = createBookCollection();
  collection.replaceProject({ schemaVersion: 1, files: [{ path: "current.md", source: "\ufeffCurrent unsaved work\r\n" }], options: { title: "Current book" } });
  let chosenHandle, renders = 0, cancelCalls = 0, replacements = 0;
  const confirmations = [];
  window.confirm = () => true; window.showOpenFilePicker = () => Promise.resolve([chosenHandle]);
  const original = createBookControls({ root, collection, confirm: () => true,
    worker: { render() { renders++; throw Error("Source folders must not render"); }, cancel() { cancelCalls++; }, dispose() {} } });
  const connected = createBookFileControls({ root, controls: original, collection, window, projectCodec: codec });
  const controls = connected.controls;
  const ui = createBookSourceFolderControls({ root, controls, collection, projectCodec: codec,
    confirm: text => { confirmations.push(text); return confirm(text); }, timeoutMs,
    onProjectOpened: () => { replacements++; onOpened(); } });
  const input = el("book-source-folder-input"); input.files = [];
  // Explicit DOM adapter: clearing a real file input also clears its FileList.
  Object.defineProperty(input, "value", { get: () => "", set(value) { if (value === "") input.files = []; } });
  t.after(() => { ui.dispose(); connected.dispose(); original.dispose(); });
  return { root, window, el, collection, controls, ui, connected, input, confirmations,
    get renders() { return renders; }, get cancelCalls() { return cancelCalls; },
    get replacements() { return replacements; },
    choose(files) { input.files = files; input.emit("change"); },
    edit(source, dispatch = true) { el("chapter-source").value = source; if (dispatch) el("chapter-source").emit("input"); },
    async connect() {
      const bytes = codec.serializeBookProject(collection.project());
      chosenHandle = { kind: "file", name: "connected.json", getFile: async () => new File([bytes], "connected.json") };
      await connected.open(); assert.equal(connected.state.filename, "connected.json");
    },
    resources() {
      collection.append({ chapters: [], images: [{ destination: "old.svg", bytes: new Uint8Array([60]) }] });
      collection.setFonts([{ slot: "body-regular", name: "old.ttf", bytes: new Uint8Array([1, 2, 3]) }], collection.revision);
    },
  };
}

test("review and explicit reopen use actual project/file owners and clear old resource and file authority", async t => {
  const h = fixture(t); await h.connect(); h.resources();
  const before = h.collection.project(), revision = h.collection.revision, cancels = h.cancelCalls;
  const edited = "\ufeff# Updated 日本\r\nExact CRLF\r\n";
  h.choose(await exportedFolder(sample(), { "sources/guide/start.md": edited }));
  assert.equal(h.el("book-source-folder-review").disabled, false);
  assert.equal(h.el("book-source-folder-restore").disabled, true);
  const summary = await h.ui.review();
  assert.equal(summary.changedSources, 1); assert.equal(summary.sourceCount, 3);
  assert.deepEqual(h.collection.project(), before); assert.equal(h.collection.revision, revision);
  assert.equal(h.collection.images.length, 1); assert.equal(h.collection.fonts.length, 1);
  assert.equal(h.cancelCalls, cancels); assert.equal(h.renders, 0); assert.equal(h.confirmations.length, 0);
  assert.equal(h.el("book-source-folder-status").attributes.role, "status");
  assert.equal(h.el("book-source-folder-files").children.length, 3);
  assert.match(h.el("book-source-folder-files").children[0].textContent, /guide\/start.md.*edited since export/);
  assert.equal(await h.ui.restore(), true);
  const expected = codec.normalizeBookProject(sample()); expected.files[0].source = edited;
  assert.deepEqual(h.collection.project(), expected); assert.equal(h.el("chapter-source").value, edited.replace(/\r\n?/g, "\n"));
  assert.deepEqual(h.collection.images, []); assert.deepEqual(h.collection.fonts, []);
  assert.equal(h.connected.state.filename, null); assert.equal(h.replacements, 1); assert.equal(h.renders, 0);
  assert.match(h.confirmations[0], /Save current work first/); assert.match(h.confirmations[0], /images and fonts will be cleared/);
  assert.equal(h.input.files.length, 0); assert.equal(h.el("book-source-folder-restore").disabled, true);
});

test("declining replacement preserves the source, settings, resources and reviewed folder", async t => {
  const h = fixture(t, { confirm: () => false }); h.resources();
  const before = h.collection.project(), images = h.collection.images, fonts = h.collection.fonts;
  h.choose(await exportedFolder()); await h.ui.review();
  assert.equal(await h.ui.restore(), false);
  assert.deepEqual(h.collection.project(), before); assert.deepEqual(h.collection.images, images); assert.deepEqual(h.collection.fonts, fonts);
  assert.equal(h.replacements, 0); assert.equal(h.el("book-source-folder-restore").disabled, false);
});

test("incomplete or invalid selected trees never partially replace a live book", async t => {
  const h = fixture(t); h.resources(); const before = h.collection.project(), revision = h.collection.revision;
  const files = await exportedFolder(); h.choose(files.filter(file => file.name !== "shared.txt"));
  await assert.rejects(h.ui.review(), { code: "SOURCE_FILE_MISSING" });
  assert.deepEqual(h.collection.project(), before); assert.equal(h.collection.revision, revision);
  assert.equal(h.collection.images.length, 1); assert.equal(h.collection.fonts.length, 1);
  assert.equal(h.el("book-source-folder-restore").disabled, true); assert.equal(h.confirmations.length, 0);
  h.choose(await exportedFolder(sample(), { "sources/last.md": new Uint8Array([0xff]) }));
  await assert.rejects(h.ui.review(), { code: "INVALID_UNICODE" }); assert.equal(h.collection.revision, revision);
});

test("editing during a pending file read retires it and permits a new review without late clobbering", async t => {
  const h = fixture(t), entered = deferred(), held = deferred(), files = await exportedFolder();
  const source = files.find(file => file.name === "start.md"), originalRead = source.arrayBuffer.bind(source);
  Object.defineProperty(source, "arrayBuffer", { value() { entered.resolve(); return held.promise; } });
  h.choose(files); const pending = h.ui.review(), rejected = assert.rejects(pending, { code: "IMPORT_CANCELLED" });
  await entered.promise; h.edit("New unsaved edit"); await rejected;
  h.choose(await exportedFolder()); await h.ui.review();
  const status = h.el("book-source-folder-status").textContent;
  held.resolve(await originalRead()); await tick();
  assert.equal(h.el("book-source-folder-status").textContent, status);
  assert.equal(h.el("book-source-folder-restore").disabled, false);
  assert.equal(h.collection.files[0].source, "New unsaved edit"); assert.equal(h.replacements, 0);
});

test("silent editor or picker changes are fenced before review publication and final replacement", async t => {
  const h = fixture(t); h.choose(await exportedFolder());
  const pending = h.ui.review(); h.edit("Silent raw draft", false);
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(h.el("chapter-source").value, "Silent raw draft");
  await h.ui.review(); h.el("title").value = "Unannounced setting draft";
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" }); assert.equal(h.confirmations.length, 0);
  await h.ui.review(); h.input.files = await exportedFolder();
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" }); assert.equal(h.replacements, 0);
});

test("edits and explicit cancellation during asynchronous confirmation cannot install the reviewed project", async t => {
  let approval = deferred(); const h = fixture(t, { confirm: () => approval.promise });
  h.choose(await exportedFolder()); await h.ui.review();
  const pending = h.ui.restore(), rejected = assert.rejects(pending, { code: "IMPORT_CANCELLED" });
  h.edit("Keep this edit"); await rejected; approval.resolve(true); await tick();
  assert.equal(h.collection.files[0].source, "Keep this edit"); assert.equal(h.replacements, 0);
  approval = deferred(); h.choose(await exportedFolder()); await h.ui.review();
  const retry = h.ui.restore(), cancelled = assert.rejects(retry, { code: "IMPORT_CANCELLED" });
  h.ui.cancel(); await cancelled; approval.resolve(true); await tick();
  assert.equal(h.collection.files[0].source, "Keep this edit"); assert.equal(h.replacements, 0);
});

test("identical project replacement, composition and lifecycle changes invalidate reviewed imports", async t => {
  const h = fixture(t); h.choose(await exportedFolder()); await h.ui.review();
  h.collection.replaceProject(h.collection.project());
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" }); assert.equal(h.replacements, 0);
  await h.ui.review(); h.el("chapter-source").emit("compositionstart");
  assert.equal(h.el("book-source-folder-review").disabled, true);
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" });
  h.el("chapter-source").emit("compositionend"); await h.ui.review();
  h.ui.suspend(); assert.equal(h.input.files.length, 0); assert.equal(h.el("book-source-folder-review").disabled, true);
  h.ui.resume(); h.choose(await exportedFolder()); await h.ui.review(); h.ui.dispose();
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" }); assert.equal(h.replacements, 0);
});

test("ordinary project import supersedes a reviewed folder through the existing busy/identity signals", async t => {
  const h = fixture(t); h.choose(await exportedFolder()); await h.ui.review();
  const alternate = { schemaVersion: 1, files: [{ path: "other.md", source: "Other project" }], options: {} };
  await h.controls.importFiles([new File([codec.serializeBookProject(alternate)], "other.json")], "project");
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" });
  assert.deepEqual(h.collection.project(), codec.normalizeBookProject(alternate)); assert.equal(h.replacements, 0);
});

test("a read deadline clears a stalled folder operation without losing the book", async t => {
  const h = fixture(t, { timeoutMs: 1 }), files = await exportedFolder(), held = deferred();
  const manifest = files.find(file => file.name === "book.fmdbook.json"), original = manifest.arrayBuffer.bind(manifest);
  Object.defineProperty(manifest, "arrayBuffer", { value: () => held.promise });
  h.choose(files); const before = h.collection.project();
  await assert.rejects(h.ui.review(), { code: "IMPORT_CANCELLED" });
  assert.match(h.el("book-source-folder-status").textContent, /exceeded its deadline/);
  assert.deepEqual(h.collection.project(), before); assert.equal(h.el("book-source-folder-review").disabled, false);
  held.resolve(await original()); await tick(); assert.equal(h.replacements, 0);
});

test("reentrant project replacement during collection notification keeps newer state without stale success", async t => {
  const h = fixture(t); h.choose(await exportedFolder()); await h.ui.review();
  const newer = { schemaVersion: 1, files: [{ path: "newer.md", source: "Newer project wins" }], options: { title: "Newer" } };
  let once = true;
  const unsubscribe = h.collection.subscribe(() => {
    if (once) { once = false; h.collection.replaceProject(newer); }
  });
  t.after(unsubscribe);
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" });
  assert.deepEqual(h.collection.project(), codec.normalizeBookProject(newer));
  assert.equal(h.replacements, 1); assert.match(h.el("book-source-folder-status").textContent, /Newer work was kept/);
  assert.doesNotMatch(h.el("book-source-folder-status").textContent, /^Reopened /);
});

test("identity callback edits are retained and prevent stale successful reopen reporting", async t => {
  let h;
  h = fixture(t, { onOpened: () => h.edit("Newer edit during detachment") });
  h.choose(await exportedFolder()); await h.ui.review();
  await assert.rejects(h.ui.restore(), { code: "STALE_SOURCE" });
  assert.equal(h.collection.files[0].source, "Newer edit during detachment");
  assert.equal(h.replacements, 1); assert.doesNotMatch(h.el("book-source-folder-status").textContent, /^Reopened /);
});
