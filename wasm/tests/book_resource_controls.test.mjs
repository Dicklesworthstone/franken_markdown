// Real workbench/model/inspection adapters. DOM and renderer endpoints below
// are explicit doubles; File, Blob, object URLs and SHA-256 use Node APIs.
import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { readFileSync } from "node:fs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookInspectionControls } from "../demo/book_inspection_controls.mjs";
import { inspectBook } from "../book_inspection.mjs";
import { findBookSource, planBookReplacement } from "../demo/book_source_search.mjs";
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
class Element extends EventTarget {
  #value = ""; children = []; textContent = ""; disabled = false; checked = false; hidden = false;
  selectionStart = 0; selectionEnd = 0; scrollTop = 0;
  get value() { return this.#value; }
  set value(value) { this.#value = String(value).replace(/\r\n?/g, "\n"); }
  removeAttribute(name) { delete this[name]; }
  setAttribute(name, value) { this[name] = value; }
  replaceChildren(...children) { this.children = children; }
  append(...children) { this.children.push(...children); }
  focus() { this.focused = true; }
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
  click() { const event = new Event("click", { cancelable: true }); this.dispatchEvent(event); return event; }
}
const type = (element, value, kind = "input") => { element.value = value; element.dispatchEvent(new Event(kind)); };
function setup(t) {
  const html = readFileSync(new URL("../demo/book.html", import.meta.url), "utf8");
  const el = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], new Element()]));
  // The inspection panel is created dynamically. Its inventory is sourced from
  // its real adapter, not a second hand-maintained view of required controls.
  const inspectionSource = readFileSync(new URL("../demo/book_inspection_controls.mjs", import.meta.url), "utf8");
  for (const match of inspectionSource.matchAll(/'(inspection-[a-z-]+)'/g)) el[match[1]] ??= new Element();
  const root = { querySelector: query => el[query.slice(1)], createElement: () => new Element() };
  const collection = createBookCollection(), jobs = [], cleanup = [];
  let cancels = 0, confirmation = () => true;
  const worker = { render(files, format, options) { const job = gate(); jobs.push({ ...job, files, format, options }); return job.promise; },
    cancel() { cancels++; }, dispose() {} };
  const controls = createBookControls({ root, collection, worker, confirm: message => confirmation(message) });
  t.after(() => { for (const fn of cleanup) fn(); controls.dispose(); });
  return { root, el, collection, controls, jobs, cleanup, get cancels() { return cancels; }, confirm(fn) { confirmation = fn; } };
}
async function initial(f) {
  await f.controls.importFiles([new File(["# One\n\n{{#include shared.md}}\n"], "one.md"), new File(["# Two\n"], "two.md")]);
  await f.controls.importFiles([new File(["\ufeffShared 😀\r\ntext\r"], "shared.md")], "includes");
}
const choose = (f, index) => type(f.el.chapters, index, "change");
const output = format => ({ extension: format === "site" ? "zip" : format,
  blob: () => new Blob(["explicit renderer double"], { type: "application/octet-stream" }) });
for (const format of ["pdf", "epub", "site"]) test(`${format}: authoring UI dispatches chapters and includes separately`, async t => {
  const f = setup(t); await initial(f); choose(f, 2); f.el["move-up"].click();
  assert.deepEqual(f.el.chapters.children.map(item => item.textContent), ["1. one.md", "[include] shared.md", "2. two.md"]);
  const pending = f.controls.prepare(format), job = f.jobs[0];
  assert.deepEqual(job.files.map(file => file.path), ["one.md", "two.md"]);
  assert.deepEqual(job.options.includeSources, [{ path: "shared.md", source: "\ufeffShared 😀\r\ntext\r" }]);
  job.resolve(output(format)); await pending;
  assert.equal(f.el.download.hidden, false); assert.equal(f.el.download.click().defaultPrevented, false);
});
test("include-only drafts remain editable and saveable without a publishable chapter", async t => {
  const f = setup(t); f.el["add-include"].click(); type(f.el["chapter-source"], "Shared text");
  assert.equal(f.el["source-role"].value, "include"); assert.equal(f.collection.files[0].role, "include");
  for (const format of ["pdf", "epub", "site"]) assert.equal(f.el[`export-${format}`].disabled, true);
  f.controls.prepareSource(true);
  const project = await (await fetch(f.el.download.href)).json();
  assert.equal(project.schemaVersion, 2); assert.equal(project.files[0].source, "Shared text");
  await assert.rejects(f.controls.prepare("pdf"), { code: "EMPTY_BOOK" }); assert.equal(f.jobs.length, 0);
  type(f.el["source-role"], "chapter", "change");
  assert.equal(f.el["export-pdf"].disabled, false); assert.equal(f.collection.project().schemaVersion, 1);
});
test("explicit resource picker imports non-Markdown UTF-8 without granting images", async t => {
  const f = setup(t); f.el["import-includes"].files = [new File(["fn main() {}\r\n"], "code.rs"), new File(["<svg/>"], "shape.svg")];
  f.el["import-includes"].dispatchEvent(new Event("change")); await tick();
  assert.deepEqual(f.collection.files.map(file => file.role), ["include", "include"]); assert.equal(f.collection.images.length, 0);
  assert.equal(f.el["source-role"].value, "include"); f.controls.prepareSource(false);
  assert.equal(f.el.download.download, "code.rs"); assert.equal(await (await fetch(f.el.download.href)).text(), "fn main() {}\r\n");
});
test("resource folder picker preserves one-root paths", async t => {
  const f = setup(t), file = new File(["snippet"], "x.txt");
  Object.defineProperty(file, "webkitRelativePath", { value: "selected/parts/x.txt" });
  f.el["import-include-folder"].files = [file]; f.el["import-include-folder"].dispatchEvent(new Event("change")); await tick();
  assert.deepEqual(f.collection.files, [{ path: "parts/x.txt", source: "snippet", role: "include" }]);
});
test("role changes retire running exports without overwriting newer source downloads", async t => {
  const f = setup(t); await initial(f); choose(f, 1);
  const pending = f.controls.prepare("pdf"), rejected = assert.rejects(pending, { code: "STALE_SOURCE" });
  type(f.el["source-role"], "include", "change"); f.controls.prepareSource(true); const href = f.el.download.href;
  f.jobs[0].resolve(output("pdf")); await rejected;
  assert.equal(f.el.download.href, href); assert.equal(f.el.download.download, "book.fmdbook.json"); assert(f.cancels > 0);
  assert.equal((await (await fetch(href)).json()).files[1].role, "include");
});
test("programmatic role edits cannot activate an output prepared for another role", async t => {
  const f = setup(t); await initial(f); f.controls.prepareSource(true); const href = f.el.download.href;
  f.el["source-role"].value = "include";
  assert.equal(f.el.download.click().defaultPrevented, true); await assert.rejects(fetch(href));
  assert.equal(f.collection.files[0].role, undefined, "no unrequested source mutation");
});
test("invalid role promotion leaves source bytes and role intact", async t => {
  const f = setup(t); await f.controls.importFiles([new File(["\ufeffcode\r\n"], "x.rs")], "includes");
  const revision = f.collection.revision; type(f.el["source-role"], "chapter", "change");
  assert.equal(f.collection.revision, revision); assert.equal(f.el["source-role"].value, "include");
  assert.match(f.el.status.textContent, /INVALID_PATH/); f.controls.prepareSource(false);
  assert.equal(await (await fetch(f.el.download.href)).text(), "\ufeffcode\r\n".replace(/^\ufeff/, ""));
  // Response.text removes a leading BOM; raw source download must still retain it.
  assert.deepEqual(new Uint8Array(await (await fetch(f.el.download.href)).arrayBuffer()), new TextEncoder().encode("\ufeffcode\r\n"));
});
test("editing during resource reads rejects the entire pending import", async t => {
  const f = setup(t); await initial(f); const hold = gate();
  class SlowFile extends File { async arrayBuffer() { await hold.promise; return super.arrayBuffer(); } }
  const pending = f.controls.importFiles([new SlowFile(["late"], "late.txt")], "includes");
  assert.equal(f.el["source-role"].disabled, true); type(f.el["chapter-source"], "newer edit"); hold.resolve();
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(f.collection.files.length, 3);
  assert.equal(f.collection.files[0].source, "newer edit"); assert.equal(f.el["source-role"].disabled, false);
});
test("source recovery restores mixed roles and order while revoking image authority", async t => {
  const f = setup(t); await initial(f); choose(f, 2); f.el["move-up"].click();
  const saved = f.controls.captureProject();
  await f.controls.importFiles([new File(["image"], "figure.svg")]);
  type(f.el["chapter-source"], "changed snippet"); f.controls.prepareSource(true); const href = f.el.download.href;
  f.controls.replaceProject(saved, f.controls.checkpoint());
  assert.deepEqual(f.collection.files, saved.files); assert.equal(f.collection.images.length, 0);
  assert.equal(f.el["source-role"].value, "chapter"); await assert.rejects(fetch(href));
  choose(f, 1); assert.equal(f.el["source-role"].value, "include");
  assert.deepEqual(f.collection.snapshot().options.includeSources, [{ path: "shared.md", source: saved.files[1].source }]);
});
test("version-2 project reopen cannot discard edits made during confirmation", async t => {
  const f = setup(t); await initial(f); const project = f.collection.projectDownload(), hold = gate(), entered = gate();
  f.confirm(() => { entered.resolve(); return hold.promise; });
  const pending = f.controls.importFiles([new File([project.blob], "book.json")], "project");
  await entered.promise; type(f.el["chapter-source"], "retain"); hold.resolve(true);
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(f.collection.files[0].source, "retain");
  assert.equal(f.collection.files[2].role, "include");
});
test("source-search navigation, replacement and undo preserve resource identity", async t => {
  const f = setup(t); await initial(f);
  const search = findBookSource(f.controls.captureProject(), "text"), match = search.matches[0];
  f.controls.selectSourceRange(match.chapter, match.start, match.end, f.controls.checkpoint());
  assert.equal(f.controls.currentChapter, 2); assert.equal(f.el["source-role"].value, "include");
  assert.equal(f.el["chapter-source"].value.slice(f.el["chapter-source"].selectionStart, f.el["chapter-source"].selectionEnd), "text");
  const plan = planBookReplacement(search, "replacement"); f.controls.applySources(plan.after, f.controls.checkpoint());
  assert.equal(f.collection.files[2].role, "include"); assert.match(f.collection.files[2].source, /replacement/);
  f.controls.applySources(plan.before, f.controls.checkpoint()); assert.deepEqual(f.collection.files, search.files);
});
test("suspension revokes image and output authority without losing include-only source", async t => {
  const f = setup(t); await initial(f); await f.controls.importFiles([new File(["image"], "image.svg")]);
  const before = f.controls.captureProject(); f.controls.prepareSource(true); const href = f.el.download.href;
  f.controls.suspend(); await assert.rejects(fetch(href)); assert.equal(f.collection.images.length, 0);
  assert.deepEqual(f.controls.captureProject(), before);
});
function inspection(f) {
  const calls = [];
  // A deliberately incomplete engine exercises the real report producer,
  // identity verification and UI without pretending to execute Rust/WASM.
  const engine = { documentStats() { throw new Error("explicit statistics double"); }, accessibilityAudit() { throw new Error("explicit audit double"); } };
  const worker = { async render(files, format, options) {
    calls.push({ files, format, options }); return { ...await inspectBook(engine, files), format: "book-inspection" };
  }, cancel() {}, dispose() {} };
  const controls = createBookInspectionControls({ root: f.root, controls: f.controls, collection: f.collection, worker });
  f.cleanup.push(() => controls.dispose()); return { controls, calls };
}
test("chapter inspection excludes include-only files and binds source navigation by path", async t => {
  const f = setup(t); await initial(f); choose(f, 2); f.el["move-up"].click();
  const audit = inspection(f), report = await audit.controls.run();
  assert.deepEqual(audit.calls[0].files.map(file => file.path), ["one.md", "two.md"]); assert.equal(audit.calls[0].options, undefined);
  assert.equal(report.summary.totalChapters, 2); assert.equal(report.summary.verdict, "incomplete");
  type(f.el["inspection-chapters"], 1, "change"); f.el["inspection-source"].click();
  assert.equal(f.controls.currentChapter, 2); assert.equal(f.el["chapter-path"].value, "two.md");
  assert.equal(f.el["source-role"].value, "chapter"); assert.match(f.el["inspection-status"].textContent, /Opened two.md/);
});
test("resource edits and role changes retire chapter inspection downloads", async t => {
  const f = setup(t); await initial(f); choose(f, 2); const audit = inspection(f); await audit.controls.run();
  f.el["inspection-save"].click(); const href = f.el["inspection-download"].href;
  type(f.el["chapter-source"], "new snippet"); await assert.rejects(fetch(href));
  assert.equal(f.el["inspection-chapters"].disabled, true);
  await audit.controls.run(); type(f.el["source-role"], "chapter", "change");
  assert.equal(f.el["inspection-source"].disabled, true);
});
test("include-only drafts do not masquerade as an inspected published book", async t => {
  const f = setup(t); f.el["add-include"].click(); const audit = inspection(f);
  assert.equal(f.el["inspection-run"].disabled, true);
  await assert.rejects(audit.controls.run(), { code: "EMPTY_BOOK" }); assert.equal(audit.calls.length, 0);
});
