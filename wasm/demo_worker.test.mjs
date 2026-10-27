import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createHash } from "node:crypto";
import { createDemoController } from "./demo/demo.js";
import { createWorkerRenderer } from "./document_worker.mjs";

// Production controller, explicit DOM/window/renderer doubles. These tests do
// not claim native browser CSP, download, iframe or generated-WASM validation.
function fixture(t, { directory = true, rendererFactory } = {}) {
  const downloads = [], urls = [], revoked = [], calls = [], clients = [], workerSettings = [];
  class Element extends EventTarget {
    value = ""; checked = false; disabled = false; textContent = ""; srcdoc = ""; children = [];
    constructor(tag = "div") { super(); this.tag = tag; }
    focus() {}
    appendChild(item) { this.children.push(item); item.parent = this; return item; }
    replaceChildren(...items) { this.children = items; }
    removeAttribute(name) { delete this[name]; }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(x => x !== this); }
    click() {
      const accepted = this.dispatchEvent(new Event("click", { cancelable: true }));
      if (accepted && this.tag === "a" && this.href) downloads.push({ href: this.href, download: this.download });
    }
  }
  const ids = ["download-markdown", "open-markdown", "markdown-file", "file-status", "markdown", "render", "download-pdf", "download-html", "preview", "diagnostics", "status", "source-size", "preview-meta", "font", "dark-mode", "title", "author", "custom-css", "allow-html", "line-numbers",
    "image-files", "image-folder", "image-document-path", "apply-image-folder", "clear-images", "cancel-images", "image-status", "image-references", "image-folder-help",
    "remember-draft", "refresh-draft", "restore-draft", "forget-draft", "save-draft", "draft-status"];
  const els = Object.fromEntries(ids.map(id => [id, new Element(id === "download-html" ? "a" : "div")]));
  els.markdown.value = "# Original";
  els.font.value = "sans"; els["dark-mode"].value = "auto"; els.title.value = "Original title";
  els.author.value = "Author"; els["line-numbers"].checked = true;
  if (directory) els["image-folder"].webkitdirectory = true;
  const doc = { querySelector: selector => els[selector.slice(1)], createElement: tag => new Element(tag), body: new Element("body") };
  const win = new EventTarget(), timers = new Map();
  let sequence = 0;
  win.confirm = () => true;
  win.setTimeout = fn => { const id = ++sequence; timers.set(id, fn); return id; };
  win.clearTimeout = id => timers.delete(id);
  win.URL = {
    createObjectURL(blob) { const url = `blob:fixture-${urls.length}`; urls.push({ url, blob }); return url; },
    revokeObjectURL(url) { revoked.push(url); },
  };
  const factory = options => {
    workerSettings.push(options);
    if (rendererFactory) return rendererFactory(options);
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
  return { controller, els, win, doc, calls, clients, urls, revoked, downloads, timers, output, input, flush, workerSettings };
}
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };

test("HTML download reuses exact completed bytes and filename without another render", async t => {
  const f = fixture(t), link = f.els["download-html"];
  assert.equal(link.hidden, true);
  const pending = f.controller.renderPreview(), call = f.calls[0];
  const html = '<!doctype html><html><head><style>body{color:green}</style></head><body>café😀</body></html>';
  call.resolve(f.output(call, html));
  assert.equal(await pending, true);
  assert.equal(f.els.preview.srcdoc, html);
  assert.equal(link.hidden, false);
  assert.equal(link.download, "original-title.html");
  assert.equal(await f.urls[0].blob.text(), html);
  assert.deepEqual(new Uint8Array(await f.urls[0].blob.arrayBuffer()), new TextEncoder().encode(html));
  assert.equal(f.downloads.length, 0, "preview completion does not download automatically");
  link.click(); link.click();
  assert.equal(f.downloads.length, 2);
  assert.equal(f.calls.length, 1);
  assert.equal(f.clients.length, 1);
  assert.deepEqual(f.revoked, [], "repeat activation keeps the current artifact usable");
});

for (const field of ["markdown", "font", "dark-mode", "title", "author", "custom-css", "allow-html", "line-numbers"]) {
  test(`silent ${field} changes block stale HTML activation`, async t => {
    const f = fixture(t), pending = f.controller.renderPreview();
    f.calls[0].resolve(f.output(f.calls[0], "completed HTML")); await pending;
    const element = f.els[field];
    if (["allow-html", "line-numbers"].includes(field)) element.checked = !element.checked;
    else element.value += " changed";
    f.els["download-html"].click();
    assert.equal(f.downloads.length, 0);
    assert.equal(f.els["download-html"].hidden, true);
    assert.equal(f.els["download-html"].href, undefined);
    assert.deepEqual(f.revoked, [f.urls[0].url]);
    assert.equal(f.calls.length, 1);
  });
}

test("edits and replacement rendering retire HTML URLs before accepting a fresh result", async t => {
  const f = fixture(t), link = f.els["download-html"];
  let pending = f.controller.renderPreview();
  f.calls[0].resolve(f.output(f.calls[0], "first")); await pending;
  pending = f.controller.renderPreview();
  assert.equal(link.hidden, true);
  assert.deepEqual(f.revoked, [f.urls[0].url]);
  f.calls[1].resolve(f.output(f.calls[1], "second")); await pending;
  assert.equal(link.hidden, false);
  f.input("markdown", "Changed");
  assert.equal(link.hidden, true);
  assert.deepEqual(f.revoked, f.urls.map(x => x.url));
  f.controller.dispose();
  assert.deepEqual(f.revoked, f.urls.map(x => x.url), "no duplicate revocation");
});

test("HTML and PDF prepared URLs have independent completion and cancellation", async t => {
  const f = fixture(t), preview = f.controller.renderPreview();
  f.calls[0].resolve(f.output(f.calls[0], "html")); await preview;
  const htmlUrl = f.els["download-html"].href;
  const pdf = f.controller.downloadPdf();
  f.calls[1].resolve(f.output(f.calls[1])); await pdf;
  const cancelled = f.controller.downloadPdf();
  f.els["download-pdf"].click();
  f.calls[2].resolve(f.output(f.calls[2])); assert.equal(await cancelled, false);
  assert.equal(f.els["download-html"].href, htmlUrl);
  assert.equal(f.els["download-html"].hidden, false);
  assert.deepEqual(f.revoked, []);
  f.els["download-html"].click();
  assert.equal(f.downloads.at(-1).href, htmlUrl);
  f.controller.dispose();
  assert.deepEqual(new Set(f.revoked), new Set(f.urls.map(x => x.url)));
  assert.equal(f.revoked.length, f.urls.length);
});

for (const boundary of ["composition", "suspension", "disposal"]) {
  test(`${boundary} retires completed HTML download authority`, async t => {
    const f = fixture(t), pending = f.controller.renderPreview();
    f.calls[0].resolve(f.output(f.calls[0])); await pending;
    if (boundary === "composition") f.els.markdown.dispatchEvent(new Event("compositionstart"));
    else if (boundary === "suspension") f.win.dispatchEvent(new Event("pagehide"));
    else f.controller.dispose();
    f.els["download-html"].click();
    assert.equal(f.downloads.length, 0);
    assert.equal(f.els["download-html"].hidden, true);
    assert.equal(f.els["download-html"].href, undefined);
    assert.deepEqual(f.revoked, f.urls.map(x => x.url));
  });
}

test("HTML publication rechecks source after converting the renderer result", async t => {
  const f = fixture(t), pending = f.controller.renderPreview(), call = f.calls[0];
  const result = f.output(call);
  result.blob = () => { f.els.markdown.value = "silently changed"; return new Blob(["stale"]); };
  call.resolve(result);
  assert.equal(await pending, false);
  assert.equal(f.els.preview.srcdoc, "");
  assert.equal(f.els["download-html"].hidden, true);
  assert.deepEqual(f.revoked, f.urls.map(x => x.url));
});

test("blur change after a completed input preview keeps the first download click usable", async t => {
  const f = fixture(t);
  f.input("custom-css", "body {color: green}");
  const pending = f.controller.renderPreview();
  f.calls[0].resolve(f.output(f.calls[0], "completed")); await pending;
  const url = f.els["download-html"].href;
  f.els["custom-css"].dispatchEvent(new Event("change"));
  f.els["download-html"].click();
  assert.deepEqual(f.downloads, [{ href: url, download: "original-title.html" }]);
  assert.deepEqual(f.revoked, []);
  assert.equal(f.timers.size, 0);
  assert.equal(f.calls.length, 1);
});

for (const boundary of ["composition", "suspension"]) {
  test(`unchanged source gets a fresh preview after ${boundary} ends`, async t => {
    const f = fixture(t), pending = f.controller.renderPreview();
    f.calls[0].resolve(f.output(f.calls[0])); await pending;
    if (boundary === "composition") {
      f.els.markdown.dispatchEvent(new Event("compositionstart"));
      f.els.markdown.dispatchEvent(new Event("compositionend"));
    } else {
      f.win.dispatchEvent(new Event("pagehide"));
      const show = new Event("pageshow"); Object.defineProperty(show, "persisted", { value: true });
      f.win.dispatchEvent(show);
    }
    f.flush();
    assert.equal(f.calls.length, 2);
    assert.equal(f.calls[1].source, "# Original");
    f.calls[1].resolve(f.output(f.calls[1])); await settle();
    assert.equal(f.els["download-html"].hidden, false);
    assert.notEqual(f.els["download-html"].href, f.urls[0].url);
  });
}

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

function delayedFile(name = "local.md", source = "# Imported") {
  const file = new File([source], name);
  let resolve, reject;
  const pending = new Promise((yes, no) => { resolve = yes; reject = no; });
  Object.defineProperty(file, "arrayBuffer", { value: () => pending });
  return { file, resolve: () => resolve(new TextEncoder().encode(source).buffer), reject };
}

test("local Markdown reuses the worker path, notifies source consumers and resets raw HTML", async t => {
  const f = fixture(t), prior = f.controller.renderPreview();
  f.calls[0].resolve(f.output(f.calls[0])); await prior;
  const oldUrl = f.els["download-html"].href;
  f.els["allow-html"].checked = true;
  let inputs = 0, confirmations = 0;
  f.els.markdown.addEventListener("input", () => inputs++);
  f.win.confirm = () => { confirmations++; return false; };
  assert.equal(await f.controller.openMarkdown(new File(["# Local café😀\r\n"], "Local Notes.MD")), true);
  assert.equal(confirmations, 0, "unchanged initial source does not require replacement approval");
  assert.equal(inputs, 1);
  assert.equal(f.els.markdown.value, "# Local café😀\r\n"); // The DOM double does not normalize textarea newlines.
  assert.equal(f.els.title.value, "Local Notes");
  assert.equal(f.els["allow-html"].checked, false);
  assert.equal(f.els["download-html"].hidden, true);
  assert.ok(f.revoked.includes(oldUrl));
  f.flush();
  assert.equal(f.calls[1].source, f.els.markdown.value);
  assert.equal(f.calls[1].options.title, "Local Notes");
  assert.equal(f.calls[1].options.allowRawHtml, false);
  assert.match(f.els["file-status"].textContent, /Opened Local Notes.MD/);
});

test("replacement rejection preserves the edited draft and its completed export", async t => {
  const f = fixture(t);
  f.input("markdown", "# Keep edits"); f.flush();
  f.calls[0].resolve(f.output(f.calls[0])); await settle();
  const url = f.els["download-html"].href;
  f.win.confirm = message => { assert.match(message, /Replace the current draft with local.md/); return false; };
  assert.equal(await f.controller.openMarkdown(new File(["# Discarded"], "local.md")), false);
  assert.equal(f.els.markdown.value, "# Keep edits");
  assert.equal(f.els["download-html"].href, url);
  assert.equal(f.els["download-html"].hidden, false);
  assert.deepEqual(f.revoked, []);
  assert.match(f.els["file-status"].textContent, /Open cancelled/);
});

test("accepted replacement becomes the next clean baseline and fences both pending workers", async t => {
  const f = fixture(t);
  f.input("markdown", "# Edited");
  const html = f.controller.renderPreview(), pdf = f.controller.downloadPdf();
  let approvals = 0;
  f.win.confirm = () => { approvals++; return true; };
  assert.equal(await f.controller.openMarkdown(new File(["# New"], "new.md")), true);
  assert.equal(approvals, 1);
  for (const call of f.calls) { assert.equal(call.controls.signal.aborted, true); call.resolve(f.output(call)); }
  assert.equal(await html, false); assert.equal(await pdf, false);
  assert.equal(f.downloads.length, 0);
  assert.equal(await f.controller.openMarkdown(new File(["# Next"], "next.md")), true);
  assert.equal(approvals, 1, "unmodified imported source is the clean baseline");
  assert.equal(f.els.markdown.value, "# Next");
});

for (const [name, mutate] of [
  ["an edit away and back", f => { f.input("markdown", "changed"); f.input("markdown", "# Original"); }],
  ["a silent source edit", f => { f.els.markdown.value = "silent"; }],
  ["a settings change", f => f.input("title", "New title")],
  ["composition that ends before the read", f => { f.els.markdown.dispatchEvent(new Event("compositionstart")); f.els.markdown.dispatchEvent(new Event("compositionend")); }],
  ["suspension and resume", f => { f.win.dispatchEvent(new Event("pagehide")); const e = new Event("pageshow"); Object.defineProperty(e, "persisted", { value: true }); f.win.dispatchEvent(e); }],
  ["disposal", f => f.controller.dispose()],
]) {
  test(`delayed file cannot replace source after ${name}`, async t => {
    const f = fixture(t), delayed = delayedFile(), pending = f.controller.openMarkdown(delayed.file);
    mutate(f); const before = f.els.markdown.value;
    delayed.resolve(); assert.equal(await pending, false);
    assert.equal(f.els.markdown.value, before);
    assert.equal(f.els.title.value, name === "a settings change" ? "New title" : "Original title");
    assert.doesNotMatch(f.els["file-status"].textContent, /Opened local.md/);
  });
}

test("a later file owns publication and status when the first read fails last", async t => {
  const f = fixture(t), first = delayedFile("first.md"), pending = f.controller.openMarkdown(first.file);
  assert.equal(await f.controller.openMarkdown(new File(["# Second"], "second.md")), true);
  first.reject(new Error("late disk error")); assert.equal(await pending, false);
  assert.equal(f.els.markdown.value, "# Second");
  assert.match(f.els["file-status"].textContent, /Opened second.md/);
});

test("a successful older read cannot replace a more recent selected file", async t => {
  const f = fixture(t), first = delayedFile("first.md"), second = delayedFile("second.md", "# Second");
  const a = f.controller.openMarkdown(first.file), b = f.controller.openMarkdown(second.file);
  second.resolve(); assert.equal(await b, true);
  first.resolve(); assert.equal(await a, false);
  assert.equal(f.els.markdown.value, "# Second");
});

for (const [name, file, code] of [
  ["invalid UTF-8", new File([new Uint8Array([0xc3, 0x28])], "bad.md"), "INVALID_UNICODE"],
  ["over-budget source", new File([new Uint8Array(4 * 1024 * 1024 + 1)], "large.md"), "BUDGET_EXCEEDED"],
  ["path-like name", new File(["# No"], "../outside.md"), "INVALID_FILENAME"],
]) {
  test(`rejected ${name} retains source and usable existing HTML`, async t => {
    const f = fixture(t), preview = f.controller.renderPreview();
    f.calls[0].resolve(f.output(f.calls[0])); await preview;
    const url = f.els["download-html"].href;
    assert.equal(await f.controller.openMarkdown(file), false);
    assert.equal(f.els.markdown.value, "# Original");
    assert.equal(f.els["download-html"].href, url);
    assert.equal(f.els["download-html"].hidden, false);
    assert.deepEqual(f.revoked, []);
    assert.match(f.els["file-status"].textContent, new RegExp(code));
  });
}

test("replacement confirmation cannot install after a nested edit", async t => {
  const f = fixture(t); f.input("markdown", "# Edited");
  f.win.confirm = () => { f.input("markdown", "# Newer edits"); return true; };
  assert.equal(await f.controller.openMarkdown(new File(["# Late"], "late.md")), false);
  assert.equal(f.els.markdown.value, "# Newer edits");
});

test("an identical import retires exports even when input values have not changed", async t => {
  const f = fixture(t); f.els.title.value = "Original";
  const preview = f.controller.renderPreview(); f.calls[0].resolve(f.output(f.calls[0])); await preview;
  const url = f.els["download-html"].href;
  assert.equal(await f.controller.openMarkdown(new File(["# Original"], "Original.md")), true);
  assert.equal(f.els["download-html"].hidden, true);
  assert.ok(f.revoked.includes(url));
});

test("file input retries the same file and cancellation emits no replacement", async t => {
  const f = fixture(t), picker = f.els["markdown-file"];
  let selections = 0;
  picker.addEventListener("click", () => selections++);
  f.els["open-markdown"].click(); assert.equal(selections, 1);
  picker.files = []; picker.dispatchEvent(new Event("change"));
  assert.equal(f.els.markdown.value, "# Original");
  picker.files = [new File(["# Selected"], "selected.md")]; picker.value = "selected.md";
  picker.dispatchEvent(new Event("change")); await new Promise(resolve => setImmediate(resolve));
  assert.equal(picker.value, ""); assert.equal(f.els.markdown.value, "# Selected");
  f.els.markdown.dispatchEvent(new Event("compositionstart"));
  assert.equal(f.els["open-markdown"].disabled, true);
  assert.equal(await f.controller.openMarkdown(new File(["# Blocked"], "blocked.md")), false);
  f.els["open-markdown"].click(); assert.equal(selections, 1);
});

test("Markdown download captures the draft without starting a renderer or leaving an anchor", async t => {
  const f = fixture(t);
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(f.calls.length, 0);
  assert.equal(f.clients.length, 0);
  assert.equal(f.downloads.length, 1);
  assert.equal(f.downloads[0].download, "franken-markdown.md");
  assert.equal(await f.urls[0].blob.text(), "# Original");
  assert.equal(f.urls[0].blob.type, "text/markdown; charset=utf-8");
  assert.equal(f.doc.body.children.length, 0);
  assert.match(f.els["file-status"].textContent, /download requested/);
  assert.doesNotMatch(f.els["file-status"].textContent, /saved|overwritten/i);
});

test("Markdown backup retains imported BOM/CRLF bytes, exports edits, and restores original bytes on exact revert", async t => {
  const f = fixture(t);
  // Match the real textarea newline behavior; the decoder itself keeps bytes.
  let displayed = f.els.markdown.value;
  Object.defineProperty(f.els.markdown, "value", {
    get: () => displayed, set: value => { displayed = String(value).replace(/\r\n?/g, "\n"); },
  });
  const original = "\ufeff# Résumé\r\n\r\nCafe\u0301 😀\r\n";
  assert.equal(await f.controller.openMarkdown(new File([original], "Résumé.markdown")), true);
  assert.equal(f.els.markdown.value, original.replace(/\r\n/g, "\n"));
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(f.downloads[0].download, "Résumé.markdown");
  assert.deepEqual(new Uint8Array(await f.urls[0].blob.arrayBuffer()), new TextEncoder().encode(original));
  f.input("markdown", "# Edited\n\nKeep these changes.\n");
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(await f.urls[1].blob.text(), "# Edited\n\nKeep these changes.\n");
  f.input("markdown", original.replace(/\r\n/g, "\n"));
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.deepEqual(new Uint8Array(await f.urls[2].blob.arrayBuffer()), new TextEncoder().encode(original));
  assert.equal(f.calls.length, 0);
});

test("Markdown download never emits an executable source extension", async t => {
  const f = fixture(t);
  await f.controller.openMarkdown(new File(["<script>source text</script>"], "source.html"));
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(f.downloads[0].download, "source.html.md");
  assert.equal(await f.urls[0].blob.text(), "<script>source text</script>");
});

test("renderer failure does not prevent downloading the current Markdown draft", async t => {
  const f = fixture(t), pending = f.controller.renderPreview();
  f.calls[0].reject(Object.assign(new Error("worker unavailable"), { code: "WORKER_FAILED" }));
  assert.equal(await pending, false);
  f.input("markdown", "# Preserve despite failure");
  f.els["download-markdown"].click();
  assert.equal(f.downloads.length, 1);
  assert.equal(await f.urls[0].blob.text(), "# Preserve despite failure");
  assert.equal(f.calls.length, 1, "source download never retries or falls back to a renderer");
});

test("empty Markdown remains downloadable without restoring the sample", async t => {
  const f = fixture(t); f.input("markdown", "");
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(f.urls[0].blob.size, 0);
  assert.equal(f.els.markdown.value, "");
  assert.equal(f.calls.length, 0);
});

for (const [name, source, code] of [
  ["malformed Unicode", "\ud800", "INVALID_UNICODE"],
  ["over-budget source", "x".repeat(4 * 1024 * 1024 + 1), "BUDGET_EXCEEDED"],
]) {
  test(`source download reports ${name} without publishing or discarding the draft`, t => {
    const f = fixture(t); f.input("markdown", source);
    assert.equal(f.controller.downloadMarkdown(), false);
    assert.equal(f.downloads.length, 0); assert.equal(f.urls.length, 0);
    assert.equal(f.els.markdown.value, source);
    assert.match(f.els["file-status"].textContent, new RegExp(code));
  });
}

test("repeated Markdown downloads own their URL separately from completed HTML and PDF", async t => {
  const f = fixture(t), html = f.controller.renderPreview(), pdf = f.controller.downloadPdf();
  for (const call of f.calls) call.resolve(f.output(call));
  assert.equal(await html, true); assert.equal(await pdf, true);
  const htmlUrl = f.els["download-html"].href, pdfUrl = f.downloads[0].href;
  assert.equal(f.controller.downloadMarkdown(), true);
  const firstSourceUrl = f.downloads[1].href;
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.deepEqual(f.revoked, [firstSourceUrl]);
  assert.equal(f.els["download-html"].href, htmlUrl);
  assert.ok(!f.revoked.includes(pdfUrl));
  assert.equal(f.calls.length, 2);
  f.input("markdown", "# New draft");
  for (const url of [htmlUrl, pdfUrl, f.downloads[2].href]) assert.ok(f.revoked.includes(url));
});

for (const [name, mutate] of [
  ["composition", f => f.els.markdown.dispatchEvent(new Event("compositionstart"))],
  ["suspension", f => f.win.dispatchEvent(new Event("pagehide"))],
  ["disposal", f => f.controller.dispose()],
]) {
  test(`${name} retires Markdown URLs and prevents further source downloads`, t => {
    const f = fixture(t); assert.equal(f.controller.downloadMarkdown(), true);
    const url = f.downloads[0].href; mutate(f);
    assert.deepEqual(f.revoked, [url]);
    assert.equal(f.els["download-markdown"].disabled, true);
    assert.equal(f.controller.downloadMarkdown(), false);
    assert.equal(f.downloads.length, 1);
  });
}

for (const [name, mutate] of [
  ["source replacement", f => f.input("markdown", "# Newer source")],
  ["silent source change", f => { f.els.markdown.value = "# Silent newer source"; }],
  ["composition", f => f.els.markdown.dispatchEvent(new Event("compositionstart"))],
  ["suspension", f => f.win.dispatchEvent(new Event("pagehide"))],
  ["disposal", f => f.controller.dispose()],
]) {
  test(`source URL creation rechecks ownership after ${name}`, t => {
    const f = fixture(t), create = f.win.URL.createObjectURL;
    f.win.URL.createObjectURL = blob => { const url = create(blob); mutate(f); return url; };
    assert.equal(f.controller.downloadMarkdown(), false);
    assert.equal(f.downloads.length, 0);
    assert.deepEqual(f.revoked, [f.urls[0].url]);
    assert.equal(f.doc.body.children.length, 0);
  });
}

test("a newer reentrant Markdown download retains its own URL when the old request is retired", t => {
  const f = fixture(t), create = f.win.URL.createObjectURL;
  let nested = false;
  f.win.URL.createObjectURL = blob => {
    const url = create(blob);
    if (!nested) { nested = true; assert.equal(f.controller.downloadMarkdown(), true); }
    return url;
  };
  assert.equal(f.controller.downloadMarkdown(), false);
  assert.equal(f.downloads.length, 1);
  assert.equal(f.downloads[0].href, f.urls[1].url);
  assert.deepEqual(f.revoked, [f.urls[0].url]);
  f.controller.dispose();
  assert.deepEqual(f.revoked, [f.urls[0].url, f.urls[1].url]);
});

test("source replacement while mounting the download link prevents activation and revokes only once", t => {
  const f = fixture(t), append = f.doc.body.appendChild.bind(f.doc.body);
  f.doc.body.appendChild = link => { append(link); f.input("markdown", "# Changed before activation"); return link; };
  assert.equal(f.controller.downloadMarkdown(), false);
  assert.equal(f.downloads.length, 0);
  assert.deepEqual(f.revoked, [f.urls[0].url]);
  assert.equal(f.doc.body.children.length, 0);
});

test("source-download click failure releases the temporary anchor and exact URL", t => {
  const f = fixture(t), create = f.doc.createElement;
  f.doc.createElement = tag => {
    const element = create(tag);
    if (tag === "a") element.click = () => { throw new Error("download activation failed"); };
    return element;
  };
  assert.equal(f.controller.downloadMarkdown(), false);
  assert.equal(f.doc.body.children.length, 0);
  assert.deepEqual(f.revoked, [f.urls[0].url]);
  assert.match(f.els["file-status"].textContent, /SOURCE_DOWNLOAD_ERROR: download activation failed/);
});

function localImage(name, bytes = [1, 2, 3], path) {
  const file = new File([new Uint8Array(bytes)], name);
  if (path) Object.defineProperty(file, "webkitRelativePath", { value: path });
  return file;
}
async function imageRender(f, kind = "html") {
  const count = f.calls.length;
  const pending = kind === "html" ? f.controller.renderPreview() : f.controller.downloadPdf();
  assert.equal(f.calls.length, count + 1);
  const call = f.calls.at(-1);
  call.resolve(f.output(call));
  assert.equal(await pending, true);
  return call;
}

test("local image selection feeds the same exact assets to HTML and PDF without rewriting Markdown", async t => {
  const f = fixture(t), source = f.els.markdown.value;
  assert.equal(await f.controller.importImages([
    localImage("photo.jpg", [1, 2]), localImage("chart a.svg", [3, 4]),
  ]), true);
  assert.equal(f.els.markdown.value, source);
  assert.match(f.els["image-references"].textContent, /chart%20a\.svg/);
  const html = await imageRender(f), pdf = await imageRender(f, "pdf");
  assert.deepEqual(pdf.options.pdfImages, html.options.pdfImages);
  assert.deepEqual([...html.options.pdfImages.find(image => image.destination === "photo.jpg").bytes], [1, 2]);
  assert.deepEqual([...html.options.pdfImages.find(image => image.destination === "./chart%20a.svg").bytes], [3, 4]);
  assert.equal(f.workerSettings[0].maxPendingBytes, 64 * 1024 * 1024);
  assert.equal(f.workerSettings[1].maxPendingBytes, 64 * 1024 * 1024);
  assert.equal(f.els["clear-images"].disabled, false);
  assert.equal(f.els["cancel-images"].disabled, true);
});

test("image replacement retires old HTML/PDF work before publishing the new grant", async t => {
  const f = fixture(t);
  await f.controller.importImages([localImage("picture.png", [1])]);
  const oldHtml = f.controller.renderPreview(), oldPdf = f.controller.downloadPdf();
  const oldCalls = [...f.calls];
  assert.equal(await f.controller.importImages([localImage("picture.png", [2])]), true);
  for (const call of oldCalls) {
    assert.equal(call.controls.signal.aborted, true);
    assert.equal(call.client.disposed, true);
    call.resolve(f.output(call, "old image output"));
  }
  assert.equal(await oldHtml, false);
  assert.equal(await oldPdf, false);
  assert.equal(f.downloads.length, 0);
  assert.equal(f.els.preview.srcdoc, "");
  const current = await imageRender(f);
  assert.equal(current.options.pdfImages[0].bytes[0], 2);
});

test("typing retains the current explicit image grant but opening another document revokes it", async t => {
  const f = fixture(t);
  await f.controller.importImages([localImage("picture.png", [7])]);
  f.input("markdown", "# Edited document");
  assert.equal((await imageRender(f)).options.pdfImages[0].bytes[0], 7);
  assert.equal(await f.controller.openMarkdown(new File(["# Edited document"], "other.md")), true);
  assert.equal((await imageRender(f)).options.pdfImages, undefined);
  assert.equal(f.els["image-references"].textContent, "");
  assert.equal(f.els["clear-images"].disabled, true);
});

test("image loading blocks rendered downloads while original Markdown remains downloadable", async t => {
  const f = fixture(t), selected = delayedFile("picture.png", "bytes");
  await imageRender(f);
  const oldUrl = f.els["download-html"].href;
  const pending = f.controller.importImages([selected.file]);
  assert.ok(f.revoked.includes(oldUrl));
  assert.equal(f.els["download-html"].hidden, true);
  assert.equal(f.els.render.disabled, true);
  assert.equal(f.els["download-pdf"].disabled, true);
  assert.equal(await f.controller.renderPreview(), false);
  assert.equal(await f.controller.downloadPdf(), false);
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(await f.urls.at(-1).blob.text(), f.els.markdown.value);
  selected.resolve();
  assert.equal(await pending, true);
  assert.equal(f.els.render.disabled, false);
});

for (const [name, mutate] of [
  ["source input", f => f.input("markdown", "# Newer source")],
  ["silent source change", f => { f.els.markdown.value = "# Silent source"; }],
  ["settings input", f => f.input("title", "New title")],
  ["silent settings change", f => { f.els.title.value = "Silent title"; }],
  ["composition", f => f.els.markdown.dispatchEvent(new Event("compositionstart"))],
]) {
  test(`${name} during image loading cannot install an outdated selection`, async t => {
    const f = fixture(t), selected = delayedFile("new.png", "new bytes");
    await f.controller.importImages([localImage("previous.png", [9])]);
    const pending = f.controller.importImages([selected.file]);
    mutate(f);
    selected.resolve();
    assert.equal(await pending, false);
    if (name === "composition") f.els.markdown.dispatchEvent(new Event("compositionend"));
    const current = await imageRender(f);
    assert.equal(current.options.pdfImages[0].destination, "previous.png");
    assert.equal(current.options.pdfImages[0].bytes[0], 9);
  });
}

test("cancel stops waiting for file reads and keeps the previously committed images", async t => {
  const f = fixture(t), selected = delayedFile("new.png", "new bytes");
  await f.controller.importImages([localImage("previous.png", [9])]);
  const pending = f.controller.importImages([selected.file]);
  assert.equal(f.controller.cancelImages(), true);
  assert.equal(await pending, false, "cancellation settles before File.arrayBuffer finishes");
  assert.match(f.els["image-status"].textContent, /cancelled/);
  assert.equal((await imageRender(f)).options.pdfImages[0].bytes[0], 9);
  const url = f.els["download-html"].href;
  selected.resolve(); await settle();
  assert.equal(f.els["download-html"].href, url);
});

test("revoke removes images and retires in-flight reads without later resurrection", async t => {
  const f = fixture(t), selected = delayedFile("new.png", "new bytes");
  await f.controller.importImages([localImage("previous.png")]);
  const pending = f.controller.importImages([selected.file]);
  assert.equal(f.controller.clearImages(), true);
  assert.equal(await pending, false);
  selected.resolve(); await settle();
  assert.equal((await imageRender(f)).options.pdfImages, undefined);
  assert.equal(f.els["image-references"].textContent, "");
  assert.match(f.els["image-status"].textContent, /revoked/);
});

test("a newer image selection owns state and cleanup even when an old read settles last", async t => {
  const f = fixture(t), old = delayedFile("old.png", "old bytes");
  const first = f.controller.importImages([old.file]);
  const second = f.controller.importImages([localImage("current.svg", [8])]);
  assert.equal(await second, true);
  assert.equal(await first, false);
  const status = f.els["image-status"].textContent;
  old.resolve(); await settle();
  assert.equal(f.els["image-status"].textContent, status);
  assert.equal((await imageRender(f)).options.pdfImages[0].destination, "current.svg");
});

for (const imageFirst of [true, false]) {
  test(`a newer Markdown open owns replacement when the older image read settles ${imageFirst ? "first" : "last"}`, async t => {
    const f = fixture(t), image = delayedFile("older.png", "old image bytes");
    const markdown = delayedFile("newer.md", "# Newer document");
    await f.controller.importImages([localImage("previous.png", [9])]);
    const imageRead = f.controller.importImages([image.file]);
    const sourceRead = f.controller.openMarkdown(markdown.file);
    assert.equal(await imageRead, false, "opening source retires the older read immediately");
    if (imageFirst) { image.resolve(); await settle(); }
    markdown.resolve();
    assert.equal(await sourceRead, true);
    const status = f.els["file-status"].textContent;
    if (!imageFirst) { image.resolve(); await settle(); }
    assert.equal(f.els.markdown.value, "# Newer document");
    assert.equal(f.els["file-status"].textContent, status);
    assert.match(status, /Opened newer\.md/);
    assert.equal((await imageRender(f)).options.pdfImages, undefined);
  });
}

for (const failure of ["read failure", "declined replacement"]) {
  test(`a newer Markdown ${failure} retains committed images after retiring an older image read`, async t => {
    const f = fixture(t), image = delayedFile("older.png", "old image bytes");
    const markdown = delayedFile("newer.md", "# Rejected document");
    await f.controller.importImages([localImage("previous.png", [9])]);
    f.input("markdown", "# Keep current draft");
    f.win.confirm = () => false;
    const imageRead = f.controller.importImages([image.file]);
    const sourceRead = f.controller.openMarkdown(markdown.file);
    assert.equal(await imageRead, false);
    if (failure === "read failure") markdown.reject(new Error("source read failed"));
    else markdown.resolve();
    assert.equal(await sourceRead, false);
    image.resolve(); await settle();
    assert.equal(f.els.markdown.value, "# Keep current draft");
    const current = await imageRender(f);
    assert.equal(current.options.pdfImages[0].destination, "previous.png");
    assert.equal(current.options.pdfImages[0].bytes[0], 9);
  });
}

test("image loading deadline releases controls and does not discard the previous grant", async t => {
  const f = fixture(t), selected = delayedFile("new.png", "new bytes");
  await f.controller.importImages([localImage("previous.png", [9])]);
  const pending = f.controller.importImages([selected.file]);
  assert.equal(f.timers.size, 1, "only the image deadline is active");
  [...f.timers.values()][0]();
  assert.equal(await pending, false);
  assert.match(f.els["image-status"].textContent, /timed out/);
  assert.equal(f.els.render.disabled, false);
  assert.equal((await imageRender(f)).options.pdfImages[0].bytes[0], 9);
  selected.resolve(); await settle();
});

test("failed and partially read selections preserve all previously committed image bytes", async t => {
  const f = fixture(t);
  await f.controller.importImages([localImage("previous.png", [9])]);
  const broken = localImage("broken.svg");
  broken.arrayBuffer = async () => { throw new Error("file read failed"); };
  for (const files of [[localImage("document.md")], [localImage("new.jpg", [1]), broken]]) {
    assert.equal(await f.controller.importImages(files), false);
    assert.equal((await imageRender(f)).options.pdfImages[0].destination, "previous.png");
    assert.match(f.els["image-status"].textContent, /previous image selection is retained/);
  }
});

for (const retire of ["suspend", "dispose"]) {
  test(`${retire} releases selected image bytes and pending read ownership`, async t => {
    const f = fixture(t), selected = delayedFile("new.png", "new bytes");
    await f.controller.importImages([localImage("previous.png")]);
    const pending = f.controller.importImages([selected.file]);
    if (retire === "suspend") f.win.dispatchEvent(new Event("pagehide"));
    else f.controller.dispose();
    assert.equal(await pending, false);
    assert.equal(f.els["image-files"].disabled, true);
    assert.equal(f.els["image-references"].textContent, "");
    selected.resolve(); await settle();
    assert.equal(await f.controller.importImages([localImage("late.png")]), false);
    if (retire === "suspend") {
      const event = new Event("pageshow"); event.persisted = true; f.win.dispatchEvent(event);
      assert.equal((await imageRender(f)).options.pdfImages, undefined);
      assert.equal(await f.controller.importImages([localImage("fresh.png")]), true);
    }
  });
}

test("folder controls preserve nested image paths and ignore source-file contents", async t => {
  const f = fixture(t);
  const image = localImage("diagram.svg", [4], "project/shared/diagram.svg");
  const source = localImage("guide.md", [9], "project/docs/guide.md");
  source.arrayBuffer = () => { throw new Error("source contents must not be read"); };
  f.els["image-folder"].files = [image, source];
  f.els["image-folder"].dispatchEvent(new Event("change"));
  assert.equal(f.els["apply-image-folder"].disabled, false);
  f.els["image-document-path"].value = "docs/guide.md";
  f.els["apply-image-folder"].click();
  await new Promise(setImmediate);
  assert.match(f.els["image-status"].textContent, /1 local image authorized/);
  assert.equal(f.els.markdown.value, "# Original");
  const call = await imageRender(f);
  assert.deepEqual(call.options.pdfImages.map(asset => asset.destination), ["../shared/diagram.svg", "./../shared/diagram.svg"]);
});

test("individual-image picker captures files before resetting for same-file retries", async t => {
  const f = fixture(t), picker = f.els["image-files"];
  Object.defineProperty(picker, "value", {
    get: () => "",
    set(value) { if (value === "") this.files = []; },
  });
  const image = localImage("picture.png", [6]);
  for (let attempt = 0; attempt < 2; attempt++) {
    picker.files = [image];
    picker.dispatchEvent(new Event("change"));
    assert.deepEqual(picker.files, []);
    await new Promise(setImmediate);
    assert.match(f.els["image-status"].textContent, /1 local image authorized/);
    assert.equal((await imageRender(f)).options.pdfImages[0].bytes[0], 6);
  }
});

test("changing the folder-path draft cancels pending loading before a new mapping can apply", async t => {
  const f = fixture(t), selected = delayedFile("new.png", "new bytes");
  Object.defineProperty(selected.file, "webkitRelativePath", { value: "root/new.png" });
  f.els["image-document-path"].value = "docs/guide.md";
  const pending = f.controller.importImages([selected.file], { folder: true, documentPath: "docs/guide.md" });
  f.input("image-document-path", "elsewhere/guide.md");
  assert.equal(await pending, false);
  selected.resolve(); await settle();
  assert.equal((await imageRender(f)).options.pdfImages, undefined);
});

test("unsupported folder pickers retain working individual-file imports", async t => {
  const f = fixture(t, { directory: false });
  assert.equal(f.els["image-folder"].disabled, true);
  assert.equal(f.els["apply-image-folder"].disabled, true);
  assert.match(f.els["image-folder-help"].textContent, /unavailable/);
  assert.equal(await f.controller.importImages([localImage("picture.png")]), true);
  assert.equal((await imageRender(f)).options.pdfImages[0].destination, "picture.png");
});

function physicalImageRenderer(settings) {
  return createWorkerRenderer({ ...settings, workerFactory: () => {
    const worker = new Worker(new URL("./tests/demo_image_worker_fixture.mjs", import.meta.url));
    const listeners = new Map();
    worker.on("error", () => {});
    return {
      postMessage: (message, transfer) => worker.postMessage(message, transfer),
      terminate: () => worker.terminate(),
      addEventListener(kind, listener) {
        const wrapped = kind === "message" ? data => listener({ data }) : error => listener({ error });
        listeners.set(listener, wrapped); worker.on(kind, wrapped);
      },
      removeEventListener(kind, listener) {
        worker.off(kind, listeners.get(listener)); listeners.delete(listener);
      },
    };
  } });
}

test("physical HTML and PDF workers receive identical captured folder resources", async t => {
  const f = fixture(t, { rendererFactory: physicalImageRenderer });
  const bytes = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"><rect width="5" height="7"/></svg>');
  f.els.markdown.value = "# Figure\n\n![Diagram](../images/diagram.svg)";
  assert.equal(await f.controller.importImages([
    localImage("diagram.svg", bytes, "project/images/diagram.svg"),
  ], { folder: true, documentPath: "docs/guide.md" }), true);
  assert.equal(await f.controller.renderPreview(), true);
  const html = JSON.parse(f.els.preview.srcdoc);
  assert.equal(await f.controller.downloadPdf(), true);
  const pdf = JSON.parse(await f.urls.at(-1).blob.text());
  assert.deepEqual(pdf, html);
  assert.equal(html.source, f.els.markdown.value);
  assert.deepEqual(html.images.map(image => image.destination), ["../images/diagram.svg", "./../images/diagram.svg"]);
  assert(html.images.every(image => image.sha256 === createHash("sha256").update(bytes).digest("hex")));
  f.controller.clearImages();
  assert.equal(await f.controller.renderPreview(), true);
  assert.deepEqual(JSON.parse(f.els.preview.srcdoc).images, []);
});

test("the demo's explicit worker budget admits an eight-MiB image and its literal path alias", async t => {
  const f = fixture(t, { rendererFactory: physicalImageRenderer });
  const bytes = new Uint8Array(8 * 1024 * 1024).fill(7);
  assert.equal(await f.controller.importImages([localImage("large.png", bytes)]), true);
  assert.equal(await f.controller.renderPreview(), true);
  const receipt = JSON.parse(f.els.preview.srcdoc);
  assert.equal(receipt.images.length, 2);
  assert(receipt.images.every(image => image.size === bytes.length));
  assert.equal(f.workerSettings[0].maxPendingOperations, 1);
  assert.equal(f.workerSettings[0].maxPendingBytes, 64 * 1024 * 1024);
});
