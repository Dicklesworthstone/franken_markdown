// The actual controller and model, using explicit DOM/worker doubles. File,
// Blob, UTF-8 decoding and object-URL revocation use native Node implementations.
import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { readFileSync } from "node:fs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookCollection } from "../demo/book_collection.mjs";
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
const code = expected => error => error.code === expected;
class Element extends EventTarget {
  #value = ""; children = []; textContent = ""; disabled = false; checked = false; hidden = false;
  get value() { return this.#value; }
  set value(next) { this.#value = String(next).replace(/\r\n?/g, "\n"); }
  removeAttribute(name) { delete this[name]; }
  replaceChildren(...children) { this.children = children; }
  click() { const event = new Event("click", { cancelable: true }); this.dispatchEvent(event); return event; }
}
function setup(t) {
  // Use the production HTML inventory so a missing control is not hidden by a
  // hand-maintained parallel list. This is not a native browser DOM test.
  const html = readFileSync(new URL("../demo/book.html", import.meta.url), "utf8");
  const el = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], new Element()]));
  const root = { querySelector: query => el[query.slice(1)], createElement: () => new Element() };
  const collection = createBookCollection(), jobs = []; let cancels = 0, confirmation = () => true;
  const worker = {
    render(files, format, options) { const job = gate(); jobs.push({ ...job, files, format, options }); return job.promise; },
    cancel() { cancels++; }, dispose() {}
  };
  const controls = createBookControls({ root, worker, collection, confirm: message => confirmation(message) });
  t.after(() => controls.dispose());
  const output = format => ({ extension: format === "site" ? "zip" : format, blob: () => new Blob(["explicit engine double"], { type: "application/octet-stream" }) });
  return { root, el, collection, controls, jobs, output, get cancels() { return cancels; }, confirm(fn) { confirmation = fn; } };
}
async function initial(f) { await f.controls.importFiles([new File(["# One\r\n"], "one.md"), new File(["# Two"], "two.md")]); }
function type(el, value, event = "input") { el.value = value; el.dispatchEvent(new Event(event)); }
for (const format of ["pdf", "epub", "site"]) test(`publishing UI routes ${format} and exposes only a separate explicit download`, async t => {
  const f = setup(t); await initial(f); type(f.el.title, "My Manual");
  const promise = f.controls.prepare(format);
  assert(f.el.download.hidden); assert.equal(f.jobs[0].format, format); assert(f.el[`export-${format}`].disabled);
  assert.deepEqual(f.jobs[0].files.map(file => file.path), ["one.md", "two.md"]);
  f.jobs[0].resolve(f.output(format)); await promise;
  assert.equal(f.el.download.download, `My-Manual.${format === "site" ? "zip" : format}`);
  assert.equal(f.el.download.click().defaultPrevented, false);
  assert.equal(await (await fetch(f.el.download.href)).text(), "explicit engine double");
  assert.match(f.el.status.textContent, /has not saved/);
});
test("new chapter, editing, selection and reordering preserve each chapter's source", async t => {
  const f = setup(t); f.el["add-chapter"].click(); type(f.el["chapter-source"], "# First edit");
  f.el["add-chapter"].click(); type(f.el["chapter-source"], "# Second edit"); f.el["move-up"].click();
  assert.deepEqual(f.collection.files.map(file => file.source), ["# Second edit", "# First edit"]);
  type(f.el.chapters, "1", "change"); assert.equal(f.el["chapter-source"].value, "# First edit");
  f.el["remove-chapter"].click(); await tick(); assert.equal(f.collection.files.length, 1);
});
test("stale worker completion after typing cannot overwrite a newer prepared source project", async t => {
  const f = setup(t); await initial(f);
  const pending = f.controls.prepare("pdf"); const check = assert.rejects(pending, code("STALE_SOURCE"));
  type(f.el["chapter-source"], "# Newer"); f.controls.prepareSource(true); const current = f.el.download.href;
  f.jobs[0].resolve(f.output("pdf")); await check;
  assert.equal(f.el.download.href, current); assert.equal(f.el.download.download, "book.fmdbook.json");
  assert.match(f.el.status.textContent, /book.fmdbook.json/); assert(f.cancels > 0);
});
test("programmatic source or option edits invalidate activation even without input events", async t => {
  const f = setup(t); await initial(f);
  for (const id of ["chapter-source", "title", "font-scale"]) {
    f.controls.prepareSource(true); const href = f.el.download.href;
    f.el[id].value += id === "font-scale" ? "0" : "changed";
    assert(f.el.download.click().defaultPrevented); assert(f.el.download.hidden); await assert.rejects(fetch(href));
  }
});
test("bad settings and malformed typed Unicode report an error rather than silently rendering old source", async t => {
  const f = setup(t); await initial(f);
  type(f.el["chapter-source"], "keep\ud800"); await assert.rejects(f.controls.prepare("pdf"));
  assert.equal(f.el["chapter-source"].value, "keep\ud800"); assert.equal(f.jobs.length, 0); assert.match(f.el.status.textContent, /Unicode/);
  type(f.el["chapter-source"], "valid"); type(f.el["font-scale"], "0"); await assert.rejects(f.controls.prepare("pdf"));
  assert.equal(f.jobs.length, 0); assert.match(f.el.status.textContent, /Font scale/);
});
test("typing during file reading rejects the whole pending import", async t => {
  const f = setup(t); await initial(f); const hold = gate();
  class SlowFile extends File { async arrayBuffer() { await hold.promise; return super.arrayBuffer(); } }
  const pending = f.controls.importFiles([new SlowFile(["late"], "late.md")]);
  type(f.el["chapter-source"], "typed during read"); hold.resolve();
  await assert.rejects(pending, code("STALE_SOURCE")); assert.equal(f.collection.files.length, 2); assert.equal(f.collection.files[0].source, "typed during read");
});
test("project reopen confirms replacement and clears image authority", async t => {
  const f = setup(t); await initial(f); await f.controls.importFiles([new File(["image"], "figure.svg")]);
  f.controls.prepareSource(true); const original = await (await fetch(f.el.download.href)).blob();
  type(f.el["chapter-source"], "current edits");
  f.confirm(() => false); assert.equal(await f.controls.importFiles([new File([original], "book.json")], "project"), false);
  assert.equal(f.collection.files[0].source, "current edits"); assert.equal(f.collection.images.length, 1);
  f.confirm(() => true); await f.controls.importFiles([new File([original], "book.json")], "project");
  assert.equal(f.collection.files[0].source, "# One\r\n"); assert.equal(f.collection.images.length, 0); assert.match(f.el.status.textContent, /reauthorize/);
});
test("editing during asynchronous project confirmation refuses replacement", async t => {
  const f = setup(t); await initial(f); const hold = gate(), entered = gate();
  const project = f.collection.projectDownload();
  f.confirm(() => { entered.resolve(); return hold.promise; });
  const pending = f.controls.importFiles([new File([project.blob], "book.json")], "project");
  await entered.promise; type(f.el.title, "New title"); hold.resolve(true);
  await assert.rejects(pending, code("STALE_SOURCE")); assert.equal(f.collection.options.title, "New title");
});
test("removal confirmation cannot discard a subsequently edited chapter", async t => {
  const f = setup(t); await initial(f); const hold = gate(); f.confirm(() => hold.promise);
  f.el["remove-chapter"].click(); type(f.el["chapter-source"], "retain"); hold.resolve(true); await tick();
  assert.equal(f.collection.files.length, 2); assert.equal(f.collection.files[0].source, "retain"); assert.match(f.el.status.textContent, /STALE_SOURCE/);
});
test("revoking images and page suspension invalidate all downloads without losing chapters", async t => {
  const f = setup(t); await initial(f); await f.controls.importFiles([new File(["image"], "figure.svg")]);
  f.controls.prepareSource(true); const href = f.el.download.href;
  f.controls.suspend(); assert(f.el.download.hidden); await assert.rejects(fetch(href));
  assert.equal(f.collection.images.length, 0); assert.equal(f.collection.files.length, 2);
  f.controls.prepareSource(false); assert.equal(f.el.download.download, "one.md");
});
test("disposal fences a file read and removes controls", async t => {
  const f = setup(t), hold = gate();
  class SlowFile extends File { async arrayBuffer() { await hold.promise; return super.arrayBuffer(); } }
  const pending = f.controls.importFiles([new SlowFile(["late"], "late.md")]);
  f.controls.dispose(); hold.resolve(); await assert.rejects(pending, code("SESSION_DISPOSED"));
  f.el["add-chapter"].click(); await assert.rejects(f.controls.prepare("pdf"), code("SESSION_DISPOSED"));
});
test("source rescue remains available with invalid publishing settings and a half-edited path", async t => {
  const f = setup(t); await initial(f); type(f.el["chapter-path"], "../unfinished"); type(f.el["font-scale"], "0");
  f.controls.prepareSource(false); assert.equal(f.el.download.download, "chapter-1.md");
  const bytes = new Uint8Array(await (await fetch(f.el.download.href)).arrayBuffer());
  assert.deepEqual(bytes, new TextEncoder().encode("# One\r\n"));
});

test("real workbench entry saves source with no WASM or Worker and preserves it through page suspension", async t => {
  const f = setup(t); f.controls.dispose();
  const host = new EventTarget(); host.confirm = () => true;
  const previous = Object.fromEntries(["document", "window", "Worker"].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  Object.defineProperties(globalThis, {
    document: { configurable: true, value: f.root },
    window: { configurable: true, value: host },
    Worker: { configurable: true, value: undefined }
  });
  t.after(() => {
    host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: false }));
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key];
    }
  });
  await import("../demo/book.js?source-entry");
  f.el["add-chapter"].click(); type(f.el["chapter-source"], "# Source without WASM");
  f.el["save-project"].click();
  const href = f.el.download.href;
  assert.equal((await (await fetch(href)).json()).files[0].source, "# Source without WASM");
  f.el["export-pdf"].click(); await tick();
  assert.match(f.el.status.textContent, /WORKER_FAILED/); assert.equal(f.el["chapter-source"].value, "# Source without WASM");
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  await assert.rejects(fetch(href));
  f.el["save-chapter"].click();
  assert.equal(await (await fetch(f.el.download.href)).text(), "# Source without WASM");
});
