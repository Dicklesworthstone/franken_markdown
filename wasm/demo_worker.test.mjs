import assert from "node:assert/strict";
import test from "node:test";
import { createDemoController } from "./demo/demo.js";

// Production controller, explicit DOM/window/renderer doubles. These tests do
// not claim native browser CSP, download, iframe or generated-WASM validation.
function fixture(t) {
  const downloads = [], urls = [], revoked = [], calls = [], clients = [];
  class Element extends EventTarget {
    value = ""; checked = false; disabled = false; textContent = ""; srcdoc = ""; children = [];
    constructor(tag = "div") { super(); this.tag = tag; }
    appendChild(item) { this.children.push(item); item.parent = this; return item; }
    replaceChildren(...items) { this.children = items; }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(x => x !== this); }
    click() { if (this.tag === "a") downloads.push({ href: this.href, download: this.download }); this.dispatchEvent(new Event("click")); }
  }
  const ids = ["markdown", "render", "download-pdf", "preview", "diagnostics", "status", "source-size", "preview-meta", "font", "dark-mode", "title", "author", "custom-css", "allow-html", "line-numbers"];
  const els = Object.fromEntries(ids.map(id => [id, new Element()]));
  els.markdown.value = "# Original";
  els.font.value = "sans"; els["dark-mode"].value = "auto"; els.title.value = "Original title";
  els.author.value = "Author"; els["line-numbers"].checked = true;
  const doc = { querySelector: selector => els[selector.slice(1)], createElement: tag => new Element(tag), body: new Element("body") };
  const win = new EventTarget(), timers = new Map();
  let sequence = 0;
  win.setTimeout = fn => { const id = ++sequence; timers.set(id, fn); return id; };
  win.clearTimeout = id => timers.delete(id);
  win.URL = {
    createObjectURL(blob) { const url = `blob:fixture-${urls.length}`; urls.push({ url, blob }); return url; },
    revokeObjectURL(url) { revoked.push(url); },
  };
  const factory = () => {
    const client = { disposed: false, dispose() { this.disposed = true; }, render(kind, source, options, controls) {
      let resolve, reject;
      const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
      calls.push({ kind, source, options, controls, resolve, reject, client });
      return promise; // Deliberately ignores cancellation to exercise late-result fencing.
    } };
    clients.push(client);
    return client;
  };
  const controller = createDemoController(doc, win, factory);
  t.after(() => controller.dispose());
  const output = (call, text = call.source) => {
    const bytes = new TextEncoder().encode(text);
    return { sourceLength: new TextEncoder().encode(call.source).length, bytes, diagnostics: [],
      text: () => text, blob: () => new Blob([bytes]), filename: base => `${base}.${call.kind}` };
  };
  const input = (id, value) => { els[id].value = value; els[id].dispatchEvent(new Event("input")); };
  const flush = () => { const tasks = [...timers.values()]; timers.clear(); for (const task of tasks) task(); };
  return { controller, els, win, doc, calls, clients, urls, revoked, downloads, timers, output, input, flush };
}
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };

test("typing coalesces and late cancelled previews never overwrite newer input", async t => {
  const f = fixture(t);
  f.flush();
  const old = f.calls[0];
  assert.equal(f.els.render.textContent, "Cancel render");
  f.input("markdown", "# Second");
  f.input("markdown", "# Latest");
  assert.equal(old.controls.signal.aborted, true);
  assert.equal(old.client.disposed, true);
  assert.equal(f.timers.size, 1);
  f.flush();
  const latest = f.calls[1];
  latest.resolve(f.output(latest, "<h1>Latest</h1>"));
  await settle();
  old.resolve(f.output(old, "<h1>OLD</h1>"));
  await settle();
  assert.equal(f.els.preview.srcdoc, "<h1>Latest</h1>");
  assert.equal(f.els.markdown.value, "# Latest");
  assert.equal(f.els.render.disabled, false);
});

test("silent programmatic source or setting changes fail the publication checkpoint", async t => {
  const f = fixture(t);
  const pending = f.controller.renderPreview(), old = f.calls[0];
  f.els["custom-css"].value = "p {color:red}"; // No input event.
  old.resolve(f.output(old, "stale"));
  assert.equal(await pending, false);
  assert.equal(f.els.preview.srcdoc, "");
  assert.match(f.els.status.textContent, /changed/);
});

test("preview and PDF use independent workers and receive only supported settings", async t => {
  const f = fixture(t);
  f.els["custom-css"].value = "body {color: green}";
  const preview = f.controller.renderPreview(), pdf = f.controller.downloadPdf();
  const a = f.calls[0], b = f.calls[1];
  assert.notEqual(a.client, b.client);
  assert.equal(a.options.customCss, "body {color: green}");
  assert.equal(Object.hasOwn(a.options, "author"), false);
  assert.equal(b.options.author, "Author");
  assert.equal(b.options.metadataEpochSeconds, 1700000000);
  assert.equal(Object.hasOwn(b.options, "customCss"), false);
  b.resolve(f.output(b));
  assert.equal(await pdf, true);
  assert.equal(f.downloads.length, 1);
  assert.equal(f.downloads[0].download, "original-title.pdf");
  a.resolve(f.output(a, "html"));
  assert.equal(await preview, true);
  assert.equal(f.els.preview.srcdoc, "html");
});

test("edits cancel PDF export and never publish or download its late output", async t => {
  const f = fixture(t);
  const pending = f.controller.downloadPdf(), call = f.calls[0];
  f.input("title", "New title");
  call.resolve(f.output(call));
  assert.equal(await pending, false);
  assert.equal(call.controls.signal.aborted, true);
  assert.equal(f.downloads.length, 0);
  assert.equal(f.urls.length, 0);
});

test("completed download URLs are revoked on edits and disposal", async t => {
  const f = fixture(t);
  let pending = f.controller.downloadPdf();
  f.calls[0].resolve(f.output(f.calls[0])); await pending;
  f.input("markdown", "Changed");
  assert.deepEqual(f.revoked, [f.urls[0].url]);
  pending = f.controller.downloadPdf();
  f.calls[1].resolve(f.output(f.calls[1])); await pending;
  f.controller.dispose(); f.controller.dispose();
  assert.deepEqual(f.revoked, f.urls.map(x => x.url));
});

test("composition pauses preview admission until committed text is available", async t => {
  const f = fixture(t);
  f.els.markdown.dispatchEvent(new Event("compositionstart"));
  f.input("markdown", "中"); f.flush();
  assert.equal(f.calls.length, 0);
  assert.equal(await f.controller.renderPreview(), false);
  f.els.markdown.dispatchEvent(new Event("compositionend")); f.flush();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].source, "中");
  f.calls[0].resolve(f.output(f.calls[0])); await settle();
});

test("page suspension terminates workers and resets raw-HTML trust without changing source", async t => {
  const f = fixture(t);
  f.els["allow-html"].checked = true;
  const preview = f.controller.renderPreview(), pdf = f.controller.downloadPdf();
  const old = [...f.calls];
  f.win.dispatchEvent(new Event("pagehide"));
  for (const call of old) call.resolve(f.output(call, "stale"));
  assert.equal(await preview, false); assert.equal(await pdf, false);
  assert.equal(f.els["allow-html"].checked, false);
  assert.equal(f.els.markdown.value, "# Original");
  assert.equal(f.els.preview.srcdoc, ""); assert.equal(f.downloads.length, 0);
  assert.ok(f.clients.every(c => c.disposed));
  const show = new Event("pageshow"); Object.defineProperty(show, "persisted", { value: true });
  f.win.dispatchEvent(show); f.flush();
  assert.equal(f.calls.length, 3);
  assert.equal(f.calls[2].options.allowRawHtml, false);
  f.calls[2].resolve(f.output(f.calls[2])); await settle();
});

test("manual cancel remains available during rendering and does not cancel the other worker", async t => {
  const f = fixture(t);
  const a = f.controller.renderPreview(), b = f.controller.downloadPdf();
  f.els.render.click();
  assert.equal(f.calls[0].controls.signal.aborted, true);
  assert.equal(f.calls[1].controls.signal.aborted, false);
  f.calls[0].resolve(f.output(f.calls[0])); f.calls[1].resolve(f.output(f.calls[1]));
  assert.equal(await a, false); assert.equal(await b, true);
  assert.equal(f.els.render.textContent, "Render");
});

test("worker failure is visible and never invokes a main-thread fallback", async t => {
  const f = fixture(t);
  const pending = f.controller.renderPreview(), call = f.calls[0];
  call.client.disposed = true;
  call.reject(Object.assign(new Error("worker unavailable"), { code: "WORKER_FAILED" }));
  assert.equal(await pending, false);
  assert.match(f.els.diagnostics.children[0].textContent, /WORKER_FAILED/);
  assert.equal(f.els.status.textContent, "render failed");
  assert.equal(f.calls.length, 1);
  const retry = f.controller.renderPreview();
  assert.notEqual(f.calls[1].client, call.client);
  f.calls[1].resolve(f.output(f.calls[1])); assert.equal(await retry, true);
});

test("empty source never starts a render or leaves controls busy", async t => {
  const f = fixture(t);
  f.input("markdown", "  \n");
  assert.equal(await f.controller.renderPreview(), false);
  assert.equal(await f.controller.downloadPdf(), false);
  assert.equal(f.calls.length, 0);
  assert.equal(f.els.render.disabled, false);
  assert.equal(f.els.status.textContent, "empty source");
});
