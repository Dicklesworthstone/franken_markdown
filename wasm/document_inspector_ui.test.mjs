// Controlled DOM/inspector adapters test the production UI controller. These
// fixtures are not Rust analysis, generated-WASM, or browser-layout proof.
import assert from "node:assert/strict";
import test from "node:test";
import { createDocumentLabController } from "./demo/document_lab.mjs";
import { statsFixture, auditFixture } from "./tests/document_inspector_fixture.mjs";

class Element extends EventTarget {
  constructor(tag = "div") {
    super(); this.tagName = tag; this.children = []; this.value = "";
    this.disabled = false; this.hidden = false; this.open = false; this.attributes = {};
    this.textContent = "";
  }
  appendChild(child) { this.children.push(child); child.parent = this; this.onAppend?.(child); return child; }
  replaceChildren(...children) { this.children = children; }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(child => child !== this); }
  setAttribute(name, value) { this.attributes[name] = value; }
  set innerHTML(_) { assert.fail("native report text must never be HTML"); }
}
function harness(t, factory) {
  const ids = ["markdown", "document-lab", "open-document-lab", "inspect-document", "download-inspection",
    "inspection-status", "inspection-results", "inspection-metrics", "inspection-structure",
    "inspection-outline", "inspection-quality", "inspection-accessibility", "preview", "allow-html", "status"];
  const els = Object.fromEntries(ids.map(id => [id, new Element()]));
  els.markdown.value = "aaaa"; els.preview.srcdoc = "untouched preview";
  els["allow-html"].checked = true; els.status.textContent = "unrelated HTML render";
  const links = [], downloads = [], urls = [], revoked = [], operations = [], clients = [];
  const doc = { body: new Element("body"), getElementById: id => els[id],
    createElement(tag) {
      const element = new Element(tag);
      if (tag === "a") {
        links.push(element);
        element.click = () => downloads.push({ url: element.href, filename: element.download });
      }
      return element;
    },
  };
  const win = new EventTarget();
  win.URL = { createObjectURL(blob) {
    const entry = { url: `blob:fixture-${urls.length}`, blob }; urls.push(entry);
    win.onUrl?.(); return entry.url;
  }, revokeObjectURL(url) { revoked.push(url); } };
  const create = options => {
    assert.deepEqual(options, { maxPendingOperations: 1 });
    const client = { disposed: false,
      inspect(source, controls) {
        assert.equal(this.disposed, false);
        assert.deepEqual(Object.keys(controls), ["signal"]);
        return new Promise((resolve, reject) => operations.push({ source, controls, resolve, reject, client }));
      },
      dispose() { this.disposed = true; },
    };
    clients.push(client); return client;
  };
  const api = createDocumentLabController(doc, win, factory ?? create);
  t.after(() => api.dispose());
  function finish(index, override = {}) {
    const op = operations[index];
    const stats = statsFixture(op.source), accessibility = auditFixture();
    const report = { schema: "fmd-document-inspection-v1", sourceLength: stats.bytes, stats, accessibility, ...override };
    const text = JSON.stringify(report);
    op.resolve({ ...report, bytes: new TextEncoder().encode(text), blob: () => new Blob([text], { type: "application/json" }) });
  }
  return { api, els, doc, win, links, downloads, urls, revoked, operations, clients, finish };
}
const fire = (target, name, extra = {}) => target.dispatchEvent(Object.assign(new Event(name), extra));
const text = element => [element.textContent, ...element.children.map(text)].join(" ");

test("Document Lab starts lazily and opening it preserves source, preview and trust", t => {
  const h = harness(t);
  assert.equal(h.clients.length, 0);
  assert.equal(h.els["download-inspection"].disabled, true);
  fire(h.els["open-document-lab"], "click");
  assert.equal(h.els["document-lab"].open, true);
  assert.equal(h.clients.length, 0);
  assert.equal(h.els.markdown.value, "aaaa");
  assert.equal(h.els.preview.srcdoc, "untouched preview");
  assert.equal(h.els["allow-html"].checked, true);
  assert.equal(h.els.status.textContent, "unrelated HTML render");
});

test("complete reports show native metrics and findings as text and download explicit JSON", async t => {
  const h = harness(t), running = h.api.inspect();
  assert.equal(h.els["inspect-document"].textContent, "Cancel inspection");
  assert.equal(h.els["document-lab"].attributes["aria-busy"], "true");
  h.finish(0); assert.equal(await running, true);
  assert.match(text(h.els["inspection-metrics"]), /Words 7/);
  assert.match(text(h.els["inspection-quality"]), /warning — broken_internal_anchor/);
  assert.match(text(h.els["inspection-accessibility"]), /image_missing_alt — Fixture <image>/);
  assert.doesNotMatch(text(h.els["inspection-accessibility"]), /warning|error/);
  assert.match(text(h.els["inspection-outline"]), /Fixture <heading>/);
  assert.equal(h.els["inspection-results"].hidden, false);
  assert.equal(h.downloads.length, 0, "inspection never downloads automatically");
  assert.equal(h.api.downloadReport(), true);
  assert.equal(h.downloads[0].filename, "document.inspection.json");
  assert.deepEqual(JSON.parse(await h.urls[0].blob.text()).stats, statsFixture("aaaa"));
  assert.equal(h.doc.body.children.length, 0);
  assert.equal(h.els["document-lab"].attributes["aria-busy"], "false");
});

test("same-length edits abort old work and an older completion cannot replace the newer draft", async t => {
  const h = harness(t), old = h.api.inspect();
  h.els.markdown.value = "bbbb"; fire(h.els.markdown, "input");
  assert.equal(h.operations[0].controls.signal.aborted, true);
  assert.equal(h.clients[0].disposed, true);
  const latest = h.api.inspect();
  const stats = { ...statsFixture("bbbb"), words: 42 };
  h.finish(1, { stats }); assert.equal(await latest, true);
  h.finish(0); assert.equal(await old, false);
  assert.match(text(h.els["inspection-metrics"]), /Words 42/);
  assert.equal(h.api.downloadReport(), true);
  assert.equal(JSON.parse(await h.urls[0].blob.text()).stats.words, 42);
});

test("late failure from cancelled work cannot clear a newer successful report", async t => {
  const h = harness(t), old = h.api.inspect();
  const latest = h.api.inspect();
  h.finish(1); await latest;
  h.operations[0].reject(new Error("old failure")); await old;
  assert.equal(h.els["inspection-results"].hidden, false);
  assert.doesNotMatch(h.els["inspection-status"].textContent, /old failure/);
  assert.equal(h.els["download-inspection"].disabled, false);
});

test("programmatic source edits without events cannot publish an old result", async t => {
  const h = harness(t), run = h.api.inspect();
  h.els.markdown.value = "bbbb";
  h.finish(0); assert.equal(await run, false);
  assert.equal(h.els["inspection-results"].hidden, true);
  assert.equal(h.els["download-inspection"].disabled, true);
  assert.match(h.els["inspection-status"].textContent, /Source changed/);
});

test("download rechecks exact source spelling before creating any URL", async t => {
  const h = harness(t), run = h.api.inspect(); h.finish(0); await run;
  h.els.markdown.value = "bbbb";
  assert.equal(h.api.downloadReport(), false);
  assert.equal(h.urls.length, 0);
  assert.equal(h.downloads.length, 0);
  assert.equal(h.els["inspection-results"].hidden, true);
});

test("reentrant URL and DOM callbacks cannot download an outdated report", async t => {
  for (const phase of ["url", "append"]) {
    const h = harness(t), run = h.api.inspect(); h.finish(0); await run;
    const change = () => { h.els.markdown.value = "bbbb"; };
    if (phase === "url") h.win.onUrl = change;
    else h.doc.body.onAppend = change;
    assert.equal(h.api.downloadReport(), false, phase);
    assert.equal(h.downloads.length, 0);
    assert.deepEqual(h.revoked, [h.urls[0].url]);
    assert.equal(h.doc.body.children.length, 0);
  }
});

test("cancel control settles UI immediately and later inspection owns a new client", async t => {
  const h = harness(t), old = h.api.inspect();
  fire(h.els["inspect-document"], "click");
  assert.equal(h.operations[0].controls.signal.aborted, true);
  assert.equal(h.clients[0].disposed, true);
  assert.match(h.els["inspection-status"].textContent, /cancelled/);
  h.finish(0); assert.equal(await old, false);
  const run = h.api.inspect(); assert.equal(h.clients.length, 2);
  h.finish(1); assert.equal(await run, true);
  assert.equal(h.els.markdown.value, "aaaa");
});

test("composition cancels pending inspection and blocks new work until composition ends", async t => {
  const h = harness(t), run = h.api.inspect();
  fire(h.els.markdown, "compositionstart");
  assert.equal(h.els["inspect-document"].disabled, true);
  assert.equal(await h.api.inspect(), false);
  assert.equal(h.clients.length, 1);
  h.finish(0); await run;
  fire(h.els.markdown, "compositionend");
  assert.equal(h.els["inspect-document"].disabled, false);
  assert.equal(h.clients.length, 1, "composition end does not start native analysis");
});

test("page lifecycle releases workers and URLs without restoring obsolete reports", async t => {
  const h = harness(t), run = h.api.inspect(); h.finish(0); await run;
  h.api.downloadReport();
  fire(h.win, "pagehide");
  assert.equal(h.clients[0].disposed, true);
  assert.deepEqual(h.revoked, [h.urls[0].url]);
  assert.equal(await h.api.inspect(), false);
  assert.equal(h.els["inspection-results"].hidden, true);
  fire(h.win, "pageshow", { persisted: true });
  assert.equal(h.els["inspect-document"].disabled, false);
  assert.equal(h.els["download-inspection"].disabled, true);
  assert.equal(h.clients.length, 1);
  assert.equal(h.els.markdown.value, "aaaa");
});

test("page hide and disposal reject late publication and detach controls", async t => {
  const h = harness(t), run = h.api.inspect();
  fire(h.win, "pagehide"); h.finish(0); assert.equal(await run, false);
  fire(h.win, "pageshow", { persisted: true });
  h.api.dispose(); h.api.dispose();
  const message = h.els["inspection-status"].textContent;
  fire(h.els.markdown, "input"); fire(h.els["inspect-document"], "click");
  assert.equal(h.els["inspection-status"].textContent, message);
  assert.equal(h.operations.length, 1);
  assert.equal(await h.api.inspect(), false);
});

test("native errors show actionable codes, not a partial or falsely successful audit", async t => {
  const h = harness(t), run = h.api.inspect();
  h.operations[0].reject(Object.assign(new Error("Rebuild the package"), { code: "UNSUPPORTED_WASM_PACKAGE" }));
  assert.equal(await run, false);
  assert.match(h.els["inspection-status"].textContent, /UNSUPPORTED_WASM_PACKAGE Rebuild/);
  assert.equal(h.els["inspection-results"].hidden, true);
  assert.equal(h.els["download-inspection"].disabled, true);
  assert.equal(h.els["inspect-document"].textContent, "Inspect current draft");
});

test("visible lists and long text are bounded without truncating the downloadable reports", async t => {
  const h = harness(t), run = h.api.inspect();
  const long = "x".repeat(2047) + "😀".repeat(2048) + "<script>bad()</script>";
  const stats = statsFixture("aaaa");
  stats.findings = Array.from({ length: 150 }, () => ({ code: "fixture", message: long }));
  stats.outline = Array.from({ length: 150 }, () => ({ level: 2, text: long, slug: "fixture" }));
  const accessibility = { schema_version: "1", target: "pdf", findings: stats.findings };
  h.finish(0, { stats, accessibility }); await run;
  for (const name of ["inspection-quality", "inspection-accessibility", "inspection-outline"]) {
    assert.equal(h.els[name].children.length, 101);
    assert.match(text(h.els[name]), /Showing 100 of 150/);
    assert.ok(h.els[name].children[0].textContent.length < 2100);
    assert.ok(Array.from(h.els[name].children[0].textContent).every(char =>
      char.codePointAt(0) < 0xd800 || char.codePointAt(0) > 0xdfff));
  }
  h.api.downloadReport();
  const report = JSON.parse(await h.urls[0].blob.text());
  assert.equal(report.stats.findings.length, 150);
  assert.equal(report.stats.findings[0].message, long);
});

test("empty sources remain inspectable and no findings is not labelled certification", async t => {
  const h = harness(t); h.els.markdown.value = "";
  const run = h.api.inspect(), stats = statsFixture("");
  stats.findings = []; stats.outline = [];
  h.finish(0, { stats, accessibility: { schema_version: "1", target: "pdf", findings: [] } });
  assert.equal(await run, true);
  assert.match(h.els["inspection-status"].textContent, /0 findings reported/);
  assert.match(h.els["inspection-status"].textContent, /not PDF certification/);
  assert.match(text(h.els["inspection-outline"]), /No headings/);
  assert.equal(h.els["download-inspection"].disabled, false);
});

test("worker creation cannot publish after a reentrant source change", async t => {
  let h, disposed = false;
  h = harness(t, () => {
    h.els.markdown.value = "bbbb";
    return { inspect() { assert.fail("stale source dispatched"); }, dispose() { disposed = true; } };
  });
  assert.equal(await h.api.inspect(), false);
  assert.equal(disposed, true);
  assert.match(h.els["inspection-status"].textContent, /Source changed/);
});

test("focus invalidates an old displayed result and repeated downloads retain only one URL", async t => {
  const h = harness(t), run = h.api.inspect(); h.finish(0); await run;
  h.api.downloadReport(); h.api.downloadReport();
  assert.deepEqual(h.revoked, [h.urls[0].url]);
  h.els.markdown.value = "bbbb"; fire(h.win, "focus");
  assert.deepEqual(h.revoked, [h.urls[0].url, h.urls[1].url]);
  assert.equal(h.els["inspection-results"].hidden, true);
});
