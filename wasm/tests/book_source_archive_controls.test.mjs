import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { createBookCollection, normalizeBookProject, serializeBookProject } from "../demo/book_collection.mjs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookDirectoryControls } from "../demo/book_directory_controls.mjs";
import { Element, readSourceZip } from "./book_source_archive_helpers.mjs";

const text = bytes => new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
const tick = () => new Promise(resolve => setImmediate(resolve));
function fixture(t, { source = "\ufeff# One 😀\r\n\r\nOriginal\r\n", files } = {}) {
  const root = new Element("document"), window = new Element("window");
  root.createElement = tag => new Element(tag);
  root.createTextNode = value => Object.assign(new Element("text"), { textContent: value });
  const section = root.appendChild(new Element("section")); section.setAttribute("aria-labelledby", "publish-title");
  const html = readFileSync(new URL("../demo/book.html", import.meta.url), "utf8");
  for (const [, id] of html.matchAll(/id="([^"]+)"/g)) section.appendChild(new Element()).id = id;
  const el = id => root.querySelector(`#${id}`), collection = createBookCollection();
  collection.replaceProject({ schemaVersion: 7, files: files ?? [{ path: "guide/one.md", source },
    { path: "parts/shared.txt", source: "Shared\rsource\r\n", role: "include" }],
    options: { title: "Applied title", profile: "gfm-plus", page: { size: "a4" } } });
  const live = new Map(), revoked = [], hooks = { create: null, revoke: null };
  let created = 0, renders = 0, cancellations = 0, grants = 0, captures = 0;
  const urls = {
    createObjectURL(blob) { const url = URL.createObjectURL(blob); live.set(url, blob); created++; hooks.create?.(); return url; },
    revokeObjectURL(url) { revoked.push(url); live.delete(url); URL.revokeObjectURL(url); hooks.revoke?.(); },
  };
  window.URL = urls;
  const controls = createBookControls({ root, collection, urls, confirm: () => true,
    worker: { render() { renders++; throw Error("Source ZIP must not render"); }, cancel() { cancellations++; }, dispose() {} } });
  const archiveHost = new Proxy(controls, { get(target, name) {
    if (name === "captureProject") return () => { captures++; throw Error("Do not apply publishing drafts"); };
    return Reflect.get(target, name);
  } });
  const archiveCollection = new Proxy(collection, { get(target, name) {
    if (["images", "fonts", "snapshot", "portableDownload"].includes(name)) { grants++; throw Error("No resource access for source ZIP"); }
    return Reflect.get(target, name);
  } });
  // Execute the real folder component that the actual book entry installs.
  // No directory picker is available, and no renderer is substituted by ZIP work.
  const ui = createBookDirectoryControls({ root, controls: archiveHost, collection: archiveCollection, window,
    projectCodec: { normalizeBookProject, serializeBookProject } });
  t.after(() => { ui.dispose(); controls.dispose(); });
  return { root, window, el, collection, controls, ui, live, revoked, hooks,
    get created() { return created; }, get renders() { return renders; }, get cancellations() { return cancellations; },
    get grants() { return grants; }, get captures() { return captures; },
    edit(value, dispatched = true) { el("chapter-source").value = value; if (dispatched) el("chapter-source").emit("input"); },
    async zip() { return readSourceZip(await (await fetch(el("book-source-archive-download").href)).blob()); },
  };
}

test("source ZIP is available without folder access and uses the actual source host and project codec", async t => {
  const h = fixture(t), before = h.collection.project(), cancels = h.cancellations;
  assert.equal(h.el("book-directory-review").disabled, true);
  assert.equal(h.el("book-source-archive-prepare").disabled, false);
  assert.equal(h.el("book-source-archive-status").attributes.role, "status");
  assert.equal(h.el("book-source-archive-status").attributes["aria-live"], "polite");
  assert.match(h.el("book-source-archive-help").textContent, /Images and fonts are excluded/);
  await h.ui.prepareArchive();
  const entries = await h.zip();
  assert.equal(text(entries.get("sources/guide/one.md")), before.files[0].source);
  assert.equal(text(entries.get("sources/parts/shared.txt")), before.files[1].source);
  assert.deepEqual(JSON.parse(text(entries.get("book.fmdbook.json"))), before);
  assert.equal(h.el("book-source-archive-download").download, "book-sources.zip");
  assert.equal(h.el("book-source-archive-download").click().defaultPrevented, false);
  assert.equal(h.renders, 0); assert.equal(h.grants, 0); assert.equal(h.captures, 0); assert.equal(h.cancellations, cancels);
  assert.deepEqual(h.collection.project(), before);
  assert.match(h.el("book-source-archive-status").textContent, /preparation has not saved a file/);
});

test("raw editor source/path/role is captured without applying drafts or changing collection authority", async t => {
  const h = fixture(t), before = h.collection.project(), revision = h.collection.revision;
  h.edit("Unannounced exact source 日本\n", false);
  h.el("chapter-path").value = "shared/new.txt"; h.el("source-role").value = "include";
  h.el("title").value = "Unapplied title"; h.el("font-scale").value = "invalid draft";
  await h.ui.prepareArchive();
  const entries = await h.zip(), saved = JSON.parse(text(entries.get("book.fmdbook.json")));
  assert.equal(text(entries.get("sources/shared/new.txt")), "Unannounced exact source 日本\n");
  assert.equal(saved.files[0].role, "include"); assert.equal(saved.options.title, "Applied title");
  assert.equal(saved.options.fontScale, 1); assert.deepEqual(saved.options, before.options);
  assert.deepEqual(h.collection.project(), before); assert.equal(h.collection.revision, revision);
  assert.equal(h.el("title").value, "Unapplied title"); assert.equal(h.el("font-scale").value, "invalid draft");
  assert.equal(h.captures, 0); assert.equal(h.grants, 0);
});

test("source changes during preparation cancel the archive and cannot overwrite a fresh successful retry", async t => {
  const h = fixture(t, { source: "x".repeat(1024 * 1024) });
  const pending = h.ui.prepareArchive(), rejected = assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  h.edit("Current source\n");
  await h.ui.prepareArchive(); const link = h.el("book-source-archive-download").href, status = h.el("book-source-archive-status").textContent;
  await rejected; await tick();
  assert.equal(h.el("book-source-archive-download").href, link); assert.equal(h.el("book-source-archive-status").textContent, status);
  assert.equal(text((await h.zip()).get("sources/guide/one.md")), "Current source\n"); assert.equal(h.live.size, 1);
});

test("silent edits fence completion and download activation without trusting old visible links", async t => {
  const h = fixture(t);
  const pending = h.ui.prepareArchive(); h.edit("Silently changed", false);
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(h.created, 0);
  await h.ui.prepareArchive(); const old = h.el("book-source-archive-download").href;
  h.el("chapter-path").value = "other.md";
  assert.equal(h.el("book-source-archive-download").click().defaultPrevented, true);
  assert(h.el("book-source-archive-download").hidden); assert(h.revoked.includes(old));
  await h.ui.prepareArchive(); assert((await h.zip()).has("sources/other.md"));
});

test("composition, cancellation and page suspension retire only source ZIP state", async t => {
  const h = fixture(t);
  const pending = h.ui.prepareArchive(), rejected = assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  h.el("chapter-source").emit("compositionstart"); await rejected;
  assert(h.el("book-source-archive-prepare").disabled);
  await assert.rejects(h.ui.prepareArchive(), { code: "BOOK_BUSY" });
  h.el("chapter-source").emit("compositionend"); await h.ui.prepareArchive();
  h.el("book-source-archive-cancel").click(); assert.equal(h.live.size, 0);
  const next = h.ui.prepareArchive(), stopped = assert.rejects(next, { code: "EXPORT_CANCELLED" });
  h.ui.suspend(); await stopped; assert.equal(h.live.size, 0);
  h.ui.resume(); await h.ui.prepareArchive(); assert.equal(h.live.size, 1);
  h.ui.dispose(); h.ui.dispose(); assert.equal(h.live.size, 0); assert.equal(h.grants, 0);
});

test("pending local imports retire source ZIP ownership and require a fresh capture after completion", async t => {
  const h = fixture(t); await h.ui.prepareArchive();
  let release;
  class SlowFile extends File { arrayBuffer() { return new Promise(resolve => { release = resolve; }); } }
  const input = new SlowFile(["# New chapter\n"], "new.md");
  const importing = h.controls.importFiles([input]);
  assert(h.el("book-source-archive-download").hidden); assert.equal(h.live.size, 0);
  await assert.rejects(h.ui.prepareArchive(), { code: "BOOK_BUSY" });
  release(new TextEncoder().encode("# New chapter\n").buffer); await importing;
  await h.ui.prepareArchive();
  assert.equal(text((await h.zip()).get("sources/new.md")), "# New chapter\n");
});

test("byte-identical project replacement and applied settings changes retire captured source downloads", async t => {
  const h = fixture(t); await h.ui.prepareArchive(); const first = h.el("book-source-archive-download").href;
  const current = h.collection.project(); h.controls.replaceProject(current, h.controls.checkpoint());
  assert(h.el("book-source-archive-download").hidden); assert(h.revoked.includes(first));
  await h.ui.prepareArchive(); const second = h.el("book-source-archive-download").href;
  h.collection.configure({ ...h.collection.options, title: "New applied title" });
  assert(h.revoked.includes(second)); await h.ui.prepareArchive();
  assert.equal(JSON.parse(text((await h.zip()).get("book.fmdbook.json"))).options.title, "New applied title");
});

test("reentrant URL creation cannot publish an archive after suspension", async t => {
  const h = fixture(t);
  h.hooks.create = () => { h.hooks.create = null; h.ui.suspend(); };
  await assert.rejects(h.ui.prepareArchive(), { code: "STALE_SOURCE" });
  assert.equal(h.created, 1); assert.equal(h.revoked.length, 1); assert.equal(h.live.size, 0);
  assert(h.el("book-source-archive-download").hidden);
  h.ui.resume(); await h.ui.prepareArchive(); assert.equal(h.live.size, 1);
});

test("revocation reentry preserves the newer preparation and its URL", async t => {
  const h = fixture(t); await h.ui.prepareArchive(); let newer;
  h.hooks.revoke = () => { h.hooks.revoke = null; newer = h.ui.prepareArchive(); };
  await assert.rejects(h.ui.prepareArchive(), { code: "EXPORT_CANCELLED" });
  await newer; assert.equal(h.live.size, 1); assert.equal(h.el("book-source-archive-download").hidden, false);
  assert.equal(h.el("book-source-archive-download").click().defaultPrevented, false);
});

test("invalid portable paths fail visibly while source editing and a corrected retry remain available", async t => {
  const h = fixture(t); h.el("chapter-path").value = "CON.md";
  h.el("book-source-archive-prepare").click();
  for (let i = 0; i < 10 && !h.el("book-source-archive-status").textContent.includes("INVALID_PATH"); i++) await tick();
  assert.match(h.el("book-source-archive-status").textContent, /INVALID_PATH/);
  assert.equal(h.created, 0); assert.equal(h.collection.files[0].path, "guide/one.md");
  h.el("chapter-path").value = "fixed.md"; await h.ui.prepareArchive();
  assert((await h.zip()).has("sources/fixed.md"));
});
