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
  const ids = ["open-markdown", "markdown-file", "file-status", "markdown", "render", "download-pdf", "download-html", "preview", "diagnostics", "status", "source-size", "preview-meta", "font", "dark-mode", "title", "author", "custom-css", "allow-html", "line-numbers"];
  const els = Object.fromEntries(ids.map(id => [id, new Element(id === "download-html" ? "a" : "div")]));
  els.markdown.value = "# Original";
  els.font.value = "sans"; els["dark-mode"].value = "auto"; els.title.value = "Original title";
  els.author.value = "Author"; els["line-numbers"].checked = true;
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
