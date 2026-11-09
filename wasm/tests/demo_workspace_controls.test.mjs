// Production demo controller with explicit DOM/window/worker adapters.
// Physical-worker/native-publisher coverage lives in demo_workspace_worker.test.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createDemoController } from "../demo/demo.js";
import { createDemoWorkspaceWorker } from "../demo/demo_workspace_worker.mjs";

const html = await readFile(new URL("../demo/index.html", import.meta.url), "utf8");
const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
const gate = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const encode = value => new TextEncoder().encode(value);
function fixture(t, { workspaceFactory } = {}) {
  const downloads = [], urls = [], revoked = [], calls = [], renders = [], clients = [], timers = new Map();
  class Element extends EventTarget {
    _value = ""; checked = false; disabled = false; textContent = ""; srcdoc = ""; children = [];
    constructor(tag = "div") { super(); this.tag = tag; }
    get value() { return this._value; }
    set value(value) { this._value = this.tag === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
    focus() {}
    appendChild(child) { this.children.push(child); child.parent = this; return child; }
    replaceChildren(...children) { this.children = children; }
    removeAttribute(name) { delete this[name]; }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(child => child !== this); }
    click() {
      if (this.disabled) return;
      const allowed = this.dispatchEvent(new Event("click", { cancelable: true }));
      if (allowed && this.tag === "a" && this.href) downloads.push({ url: this.href, filename: this.download });
    }
  }
  const els = Object.fromEntries(ids.map(id => [id, new Element(
    id === "markdown" || id === "custom-css" ? "textarea" : ["download-html", "download-workspace"].includes(id) ? "a" : "div",
  )]));
  els.markdown.value = "# Original"; els.title.value = "Original title"; els.author.value = "Author";
  els.font.value = "sans"; els["dark-mode"].value = "auto"; els["line-numbers"].checked = true;
  els["image-folder"].webkitdirectory = true;
  const doc = { querySelector: selector => els[selector.slice(1)], createElement: tag => new Element(tag), body: new Element("body") };
  const win = new EventTarget(); let nextTimer = 0;
  win.confirm = () => true;
  win.setTimeout = callback => { const id = ++nextTimer; timers.set(id, callback); return id; };
  win.clearTimeout = id => timers.delete(id);
  win.URL = {
    createObjectURL(blob) { const url = `blob:workspace-${urls.length}`; urls.push({ url, blob }); return url; },
    revokeObjectURL(url) { revoked.push(url); },
  };
  const ordinaryFactory = () => ({ disposed: false, dispose() { this.disposed = true; }, render(kind, source, options, controls) {
    const pending = gate(); renders.push({ kind, source, options, controls, ...pending }); return pending.promise;
  } });
  const factory = workspaceFactory ?? (() => {
    const client = { disposed: false, disposeCount: 0, dispose() { this.disposed = true; this.disposeCount++; }, prepare(source, options, controls) {
      const pending = gate(); calls.push({ source, options, controls, client, ...pending }); return pending.promise;
    } }; clients.push(client); return client;
  });
  const controller = createDemoController(doc, win, ordinaryFactory, {
    workspaceFactory: factory, openStore: async () => ({ async read() { return { schemaVersion: 1, version: 0, document: null }; }, dispose() {} }),
  });
  t.after(() => controller.dispose());
  const output = (call, text = "<!doctype html><html>workspace café😀</html>") => ({
    sourceLength: encode(call.source).length, bytes: encode(text), diagnostics: [],
    text: () => text, blob: () => new Blob([encode(text)], { type: "text/html;charset=utf-8" }), filename: base => `${base}.${call.kind ?? "html"}`,
  });
  const input = (id, value) => { if (["allow-html", "line-numbers"].includes(id)) els[id].checked = value; else els[id].value = value;
    els[id].dispatchEvent(new Event("input")); };
  return { controller, els, doc, win, calls, renders, clients, urls, revoked, downloads, output, input };
}

test("prepare preserves exact imported source, current settings and authorized images; download reuses the completed artifact", async t => {
  const f = fixture(t), source = "\uFEFF# Café😀\r\n\r\n![A](chart.svg)\r\n";
  assert.equal(await f.controller.openMarkdown(new File([source], "Offline Notes.md")), true);
  f.input("font", "serif"); f.input("dark-mode", "disabled");
  assert.equal(await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]), true);
  const pending = f.controller.prepareWorkspace(), call = f.calls[0];
  assert.equal(call.source, source); assert.equal(f.els.markdown.value, source.replace(/\r\n/g, "\n"));
  assert.equal(call.options.font, "serif"); assert.equal(call.options.darkMode, "disabled");
  assert.equal(call.options.title, "Offline Notes"); assert.equal(call.options.author, "Author");
  assert.equal(call.options.codeLineNumbers, true); assert.equal(call.options.metadataEpochSeconds, 1700000000);
  assert.equal(call.options.allowRawHtml, false);
  assert.deepEqual(call.options.pdfImages.map(image => image.destination), ["chart.svg", "./chart.svg"]);
  for (const image of call.options.pdfImages) assert.deepEqual(image.bytes, encode("<svg/>"));
  assert.equal(f.els["prepare-workspace"].disabled, true); assert.equal(f.els["cancel-workspace"].disabled, false);
  call.resolve(f.output(call)); assert.equal(await pending, true);
  assert.equal(call.client.disposed, true); assert.equal(f.downloads.length, 0);
  const link = f.els["download-workspace"];
  assert.equal(link.hidden, false); assert.equal(link.download, "offline-notes.workspace.html");
  link.click(); link.click();
  assert.equal(f.downloads.length, 2); assert.equal(f.calls.length, 1); assert.equal(f.urls.length, 1);
  assert.equal(await f.urls[0].blob.text(), f.output(call).text());
  assert.match(f.els["workspace-status"].textContent, /ready.*open the HTML file/);
});

test("preparing or cancelling a workspace preserves current HTML/PDF/Markdown downloads", async t => {
  const f = fixture(t), preview = f.controller.renderPreview(), htmlCall = f.renders[0];
  htmlCall.resolve(f.output(htmlCall, "Ordinary HTML")); await preview;
  const pdf = f.controller.downloadPdf(), pdfCall = f.renders[1]; pdfCall.resolve(f.output(pdfCall, "PDF")); await pdf;
  assert.equal(f.controller.downloadMarkdown(), true);
  const htmlUrl = f.els["download-html"].href, ordinaryUrls = f.urls.map(item => item.url);
  const pending = f.controller.prepareWorkspace(), call = f.calls[0];
  assert.equal(f.controller.cancelWorkspace(), true);
  assert.equal(call.controls.signal.aborted, true); assert.equal(call.client.disposed, true);
  call.resolve(f.output(call)); assert.equal(await pending, false);
  assert.equal(f.els["download-html"].href, htmlUrl); assert.equal(f.els["download-html"].hidden, false);
  assert(ordinaryUrls.every(url => !f.revoked.includes(url)));
  assert.equal(f.urls.length, ordinaryUrls.length);
});

for (const id of ["markdown", "font", "dark-mode", "title", "author", "custom-css", "allow-html", "line-numbers"]) {
  test(`${id} edits cancel preparation and discard a late result`, async t => {
    const f = fixture(t), pending = f.controller.prepareWorkspace(), call = f.calls[0];
    f.input(id, ["allow-html", "line-numbers"].includes(id) ? !f.els[id].checked : f.els[id].value + " changed");
    assert.equal(call.controls.signal.aborted, true); assert.equal(call.client.disposed, true);
    call.resolve(f.output(call)); assert.equal(await pending, false); assert.equal(f.urls.length, 0);
  });
  test(`silent ${id} edits prevent completed workspace activation`, async t => {
    const f = fixture(t), pending = f.controller.prepareWorkspace(), call = f.calls[0];
    call.resolve(f.output(call)); await pending;
    if (["allow-html", "line-numbers"].includes(id)) f.els[id].checked = !f.els[id].checked; else f.els[id].value += " changed";
    f.els["download-workspace"].click();
    assert.equal(f.downloads.length, 0); assert.equal(f.els["download-workspace"].hidden, true);
    assert.deepEqual(f.revoked, f.urls.map(item => item.url));
  });
}

for (const boundary of ["composition", "pagehide", "dispose", "clear-images", "replace-images", "identical source replacement", "file-open intent"]) {
  test(`${boundary} retires completed and pending workspace authority`, async t => {
    const f = fixture(t);
    await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]);
    let pending = f.controller.prepareWorkspace(), call = f.calls[0]; call.resolve(f.output(call)); assert.equal(await pending, true);
    pending = f.controller.prepareWorkspace(); call = f.calls[1];
    let read = null, opened = null;
    if (boundary === "composition") f.els.markdown.dispatchEvent(new Event("compositionstart"));
    else if (boundary === "pagehide") f.win.dispatchEvent(new Event("pagehide"));
    else if (boundary === "dispose") f.controller.dispose();
    else if (boundary === "clear-images") f.controller.clearImages();
    else if (boundary === "replace-images") await f.controller.importImages([new File(["new image"], "chart.svg")]);
    else if (boundary === "identical source replacement") await f.controller.openMarkdown(new File([f.els.markdown.value], "Original.md"));
    else {
      read = gate(); const file = new File(["new"], "new.md"); file.arrayBuffer = () => read.promise;
      opened = f.controller.openMarkdown(file); assert.equal(f.els["prepare-workspace"].disabled, true);
    }
    assert.equal(call.controls.signal.aborted, true);
    call.resolve(f.output(call)); assert.equal(await pending, false);
    f.els["download-workspace"].click(); assert.equal(f.downloads.length, 0);
    assert.deepEqual(f.revoked, f.urls.map(item => item.url));
    if (opened) { read.resolve(encode("new").buffer); await opened; }
  });
}

test("silent source change during asynchronous preparation refuses publication and reports the stale result", async t => {
  const f = fixture(t), pending = f.controller.prepareWorkspace(), call = f.calls[0];
  f.els.markdown.value = "Silent newer text";
  call.resolve(f.output(call)); assert.equal(await pending, false); assert.equal(f.urls.length, 0);
  assert.match(f.els["workspace-status"].textContent, /changed during preparation/);
});

test("a superseded operation cannot replace a newer prepared artifact or its status", async t => {
  const f = fixture(t), first = f.controller.prepareWorkspace(), old = f.calls[0];
  const second = f.controller.prepareWorkspace(), next = f.calls[1];
  next.resolve(f.output(next, "New workspace")); assert.equal(await second, true);
  const url = f.els["download-workspace"].href, status = f.els["workspace-status"].textContent;
  old.reject(Object.assign(new Error("Late failure"), { code: "LATE_ERROR" })); assert.equal(await first, false);
  assert.equal(f.els["download-workspace"].href, url); assert.equal(f.els["workspace-status"].textContent, status);
  assert.equal(f.revoked.length, 0);
});

for (const boundary of ["result conversion", "URL creation", "worker release"]) test(`${boundary} reentry cannot claim a stale prepared workspace`, async t => {
  const f = fixture(t), pending = f.controller.prepareWorkspace(), call = f.calls[0], result = f.output(call);
  if (boundary === "result conversion") result.blob = () => { f.els.markdown.value = "Newer"; return new Blob(["stale"]); };
  else if (boundary === "URL creation") {
    const create = f.win.URL.createObjectURL;
    f.win.URL.createObjectURL = blob => { const url = create(blob); f.input("markdown", "Newer"); return url; };
  } else {
    const dispose = call.client.dispose;
    call.client.dispose = function () { dispose.call(this); f.els.markdown.value = "Newer"; };
  }
  call.resolve(result); assert.equal(await pending, false);
  assert.equal(f.els["download-workspace"].hidden, true); assert.deepEqual(f.revoked, f.urls.map(item => item.url));
  assert.doesNotMatch(f.els["workspace-status"].textContent, /workspace ready/);
});

for (const [id, value, expected] of [["custom-css", "p{color:red}", /Clear Custom CSS/], ["allow-html", true, /Turn off Allow raw HTML/]])
  test(`unsupported ${id} settings remain visible and cannot be silently dropped`, async t => {
    let starts = 0;
    const f = fixture(t, { workspaceFactory: () => createDemoWorkspaceWorker({ workerFactory: () => { starts++; throw new Error("Must not start"); } }) });
    f.input(id, value);
    assert.equal(await f.controller.prepareWorkspace(), false); assert.equal(starts, 0);
    assert.match(f.els["workspace-status"].textContent, /UNSUPPORTED_WORKSPACE_SETTINGS/);
    assert.match(f.els["workspace-status"].textContent, expected);
    assert.equal(id === "allow-html" ? f.els[id].checked : f.els[id].value, value);
    assert.equal(f.urls.length, 0);
  });

test("native warnings appear beside the workspace without changing ordinary preview diagnostics", async t => {
  const f = fixture(t), pending = f.controller.prepareWorkspace(), call = f.calls[0], result = f.output(call);
  f.els.diagnostics.textContent = "Existing preview findings";
  result.diagnostics = [{ severity: "warning", start: 0, end: 4, message: "Image was unavailable" }];
  call.resolve(result); assert.equal(await pending, true);
  assert.match(f.els["workspace-status"].textContent, /warning 0-4: Image was unavailable/);
  assert.equal(f.els.diagnostics.textContent, "Existing preview findings");
});
