import assert from "node:assert/strict";
import test from "node:test";
import { webcrypto } from "node:crypto";
import { createBookPublicationClient } from "../book_publication.mjs";
import { createBookEditionSession } from "../demo/book_edition.mjs";
import { createBookEditionControls } from "../demo/book_edition_controls.mjs";

// Production coordinator/session/controller/ZIP logic, with explicit native
// report/rendering, source-host, DOM and URL adapters. No WASM or browser claim.
const code = expected => cause => cause?.code === expected;
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function linkReport(findings = [], path = "guide.md") {
  return Object.freeze({ schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
    summary: Object.freeze({ chapters: 1, checked: findings.length || 1, findings: findings.length, external: 2, unchecked: 3 }),
    chapters: Object.freeze([Object.freeze({ path, checked: findings.length || 1, external: 2, unchecked: 3,
      findings: Object.freeze(findings.map(value => Object.freeze({ code: "missing_anchor", destination: "#lost", message: "Missing heading.", ...value }))) })]) });
}
const linkFailure = report => Object.assign(Error("Internal navigation findings."), { code: "BOOK_LINKS_FAILED", linkReport: report });
class Node {
  constructor(tag = "div") {
    this.tagName = tag; this.children = []; this.parentElement = null; this.attributes = new Map();
    this.listeners = new Map(); this.checked = false; this.disabled = false; this.hidden = false; this.textContent = "";
  }
  append(...nodes) { for (const node of nodes) { node.parentElement = this; this.children.push(node); } }
  replaceChildren(...nodes) { for (const node of this.children) node.parentElement = null; this.children = []; this.append(...nodes); }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  removeAttribute(name) { this.attributes.delete(name); }
  get href() { return this.getAttribute("href"); } set href(value) { this.setAttribute("href", value); }
  get download() { return this.getAttribute("download"); } set download(value) { this.setAttribute("download", value); }
  querySelector(selector) {
    if (selector === `#${this.id}`) return this;
    for (const child of this.children) { const found = child.querySelector(selector); if (found) return found; }
    return null;
  }
  contains(target) { for (let node = target; node; node = node.parentElement) if (node === this) return true; return false; }
  closest(tag) { for (let node = this; node; node = node.parentElement) if (node.tagName === tag) return node; return null; }
  addEventListener(kind, handler, capture = false) {
    const all = this.listeners.get(kind) ?? []; all.push({ handler, capture }); this.listeners.set(kind, all);
  }
  removeEventListener(kind, handler) { this.listeners.set(kind, (this.listeners.get(kind) ?? []).filter(value => value.handler !== handler)); }
  fire(kind) {
    const event = { target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    const path = []; for (let node = this; node; node = node.parentElement) path.push(node);
    for (const capture of [true, false]) for (const node of capture ? path.slice().reverse() : path) {
      for (const listener of [...(node.listeners.get(kind) ?? [])]) if (listener.capture === capture) listener.handler(event);
    }
    return event;
  }
}
function fixture(t, { dom = false, strictLinks = true, rawPublisher, onState = () => {} } = {}) {
  const h = { checks: [], renders: [], requests: [], states: [], live: new Map(), revoked: [], captures: 0,
    checkReleases: 0, renderReleases: 0, revision: 0, source: "# Guide\r\n\n🙂", blocked: false, nativeReport: linkReport() };
  let serial = 0;
  const changed = new Set(), busy = new Set();
  const input = () => ({ files: [{ path: "guide.md", source: h.source }], options: { includeSources: [{ path: "parts/note.md", source: "α" }],
    images: [{ destination: "figure.svg", bytes: new Uint8Array([1, 2]) }], fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]) }] } });
  const size = (files, options) => [...files, ...(options.includeSources ?? [])].reduce((sum, file) => sum + new TextEncoder().encode(file.source).length, 0);
  h.urls = {
    createObjectURL(blob) { const href = `blob:edition-test/${++serial}`; h.live.set(href, blob); h.onUrl?.(blob, href); return href; },
    revokeObjectURL(href) { h.revoked.push(href); h.live.delete(href); },
  };
  h.publisher = rawPublisher ?? createBookPublicationClient({
    captureInput: (files, options) => structuredClone({ files, options }),
    checkLinks(files, options, request) {
      h.checks.push({ files, options, request });
      const reply = { report: h.nativeReport, sourceLength: size(files, options) };
      const release = () => { h.checkReleases++; };
      request.signal.addEventListener("abort", release, { once: true });
      return Promise.resolve(h.onCheck?.(reply, request) ?? reply).finally(() => request.signal.removeEventListener("abort", release));
    },
    createClient() {
      let revision = 0, captured;
      const render = format => {
        h.renders.push(format);
        return { format: `book-${format}`, retainedInputRevision: ++revision, sourceLength: size(captured.files, captured.options),
          bytes: new TextEncoder().encode(`${format}:${captured.files[0].source}`) };
      };
      return {
        render(files, format, options) { captured = { files, options }; return render(format); },
        renderSourceUpdate(changes, format, options) { assert.deepEqual(changes, []); assert.equal(options.expectedRevision, revision); return render(format); },
        dispose() { h.renderReleases++; },
      };
    },
  });
  h.collection = { get revision() { return h.revision; }, get files() { return input().files; },
    snapshot: input, subscribe(fn) { changed.add(fn); return () => changed.delete(fn); } };
  h.controls = { captureProject() { h.captures++; }, checkpoint: () => JSON.stringify([h.revision, h.source]),
    get sourceBusy() { return h.blocked; }, subscribeSourceState(fn) { busy.add(fn); return () => busy.delete(fn); } };
  if (dom) {
    h.root = new Node("document"); h.root.createElement = tag => new Node(tag);
    h.root.createTextNode = text => Object.assign(new Node("text"), { textContent: text });
    const parent = new Node(), title = new Node("h2"), editor = new Node("textarea");
    title.id = "publish-title"; editor.id = "chapter-source"; parent.append(title); h.root.append(parent, editor);
    h.edition = createBookEditionControls({ root: h.root, publisher: h.publisher, collection: h.collection,
      controls: h.controls, urls: h.urls, crypto: webcrypto });
    h.el = key => h.root.querySelector(`#book-edition-${key}`);
  } else {
    h.edition = createBookEditionSession({ publisher: h.publisher, urls: h.urls, crypto: webcrypto, strictLinks,
      checkpoint: h.controls.checkpoint, isBlocked: () => h.blocked,
      capture() { h.captures++; return input(); }, onState(state) { h.states.push(state); onState(state, h); } });
    changed.add(() => h.edition.changed()); busy.add(() => h.edition.invalidate());
  }
  h.change = () => { h.revision++; for (const fn of changed) fn(); };
  h.setBusy = value => { h.blocked = value; for (const fn of busy) fn(); };
  h.text = () => h.el("verification").children.map(node => node.textContent).join("\n");
  t.after(() => h.edition.dispose()); return h;
}
function oldPublisher(extra = {}) {
  return { render(files, formats) {
    const outputs = formats.map(format => {
      const mimeType = { pdf: "application/pdf", epub: "application/epub+zip", site: "application/zip" }[format];
      return { format: `book-${format}`, sourceLength: 1, mimeType, blob: () => new Blob(["x"], { type: mimeType }) };
    });
    return { format: "book-publication", sourceLength: 1, byteLength: formats.length, outputs, ...extra };
  }, cancel() {}, dispose() {} };
}

test("workbench defaults to strict checking and publishes its current complete report", async t => {
  const h = fixture(t, { dom: true }); assert.equal(h.el("strict").checked, true);
  const gate = deferred(); h.onCheck = reply => gate.promise.then(() => reply);
  const pending = h.edition.prepare(); assert.equal(h.checks.length, 1); assert.equal(h.renders.length, 0); assert.equal(h.live.size, 0);
  assert.equal(h.edition.state.strictLinks, true); gate.resolve(); const state = await pending;
  assert.equal(state.phase, "ready"); assert.equal(state.linkReport, h.nativeReport); assert.equal(h.live.size, 3);
  assert.equal(h.captures, 1); assert.deepEqual(h.renders, ["pdf", "epub", "site"]);
  assert.match(h.text(), /0 findings/); assert.match(h.text(), /2 external and 3 other/); assert.match(h.text(), /not PDF\/EPUB conformance/);
});

test("a failing check exposes actionable findings but no publications; repair clears and reruns it", async t => {
  const h = fixture(t, { dom: true }); h.nativeReport = linkReport([{}]);
  await assert.rejects(h.edition.prepare(), code("BOOK_LINKS_FAILED"));
  assert.equal(h.live.size, 0); assert.equal(h.renders.length, 0); assert.equal(h.edition.state.linkReport, h.nativeReport);
  assert.match(h.text(), /guide.md: missing_anchor — #lost — Missing heading/);
  h.source += "\n# Lost"; h.change(); assert.equal(h.edition.state.linkReport, null); assert.equal(h.el("verification").hidden, true);
  h.nativeReport = linkReport(); await h.edition.prepare(); assert.equal(h.checks.length, 2); assert.equal(h.live.size, 3);
});

test("explicit opt-out publishes without a check and is labeled unchecked", async t => {
  const h = fixture(t, { dom: true }); h.nativeReport = linkReport([{}]);
  h.el("strict").checked = false; h.el("strict").fire("change");
  const state = await h.edition.prepare(); assert.equal(state.strictLinks, false); assert.equal(state.linkReport, null);
  assert.equal(h.checks.length, 0); assert.equal(h.live.size, 3); assert.match(h.text(), /not checked.*explicitly turned off/s);
});

test("default standalone session retains the old publisher contract", async t => {
  const h = fixture(t, { strictLinks: false, rawPublisher: oldPublisher() });
  const state = await h.edition.prepare(); assert.equal(state.phase, "ready"); assert.equal(state.linkReport, null); assert.equal(h.live.size, 3);
});

test("an old publisher ignoring strict requests cannot publish even one URL", async t => {
  const h = fixture(t, { rawPublisher: oldPublisher() });
  await assert.rejects(h.edition.prepare(), code("UNSUPPORTED_BOOK_VERIFICATION"));
  assert.equal(h.live.size, 0); assert.equal(h.revoked.length, 0); assert.equal(h.edition.state.linkReport, null);
});

for (const bad of [linkReport([], "other.md"), linkReport([{}]), { ...linkReport() },
  Object.freeze({ ...linkReport(), chapters: Object.freeze([]) })]) {
  test(`strict edition rejects mismatched or unowned report receipts: ${JSON.stringify(bad)}`, async t => {
    const h = fixture(t, { rawPublisher: oldPublisher({ linkReport: bad }) });
    await assert.rejects(h.edition.prepare(), code("INVALID_LINK_REPORT")); assert.equal(h.live.size, 0);
  });
}

test("failed-report provenance is checked too, rather than retaining another book's findings", async t => {
  const h = fixture(t, { rawPublisher: { ...oldPublisher(), render() { throw linkFailure(linkReport([{}], "elsewhere.md")); } } });
  await assert.rejects(h.edition.prepare(), code("INVALID_LINK_REPORT")); assert.equal(h.edition.state.linkReport, null);
});

test("turning policy off during verification cancels the checker and never starts rendering", async t => {
  const h = fixture(t, { dom: true }), gate = deferred(); h.onCheck = reply => gate.promise.then(() => reply);
  const pending = h.edition.prepare(); const rejected = assert.rejects(pending, code("EXPORT_CANCELLED"));
  h.el("strict").checked = false; h.el("strict").fire("input"); await rejected;
  assert.equal(h.checkReleases, 1); gate.resolve(); await tick(); assert.equal(h.renders.length, 0); assert.equal(h.live.size, 0);
});

test("policy changes revoke both native and derived bundle downloads without rerendering", async t => {
  const h = fixture(t, { dom: true }); await h.edition.prepare(); await h.edition.prepareBundle();
  assert.equal(h.checks.length, 1); assert.equal(h.renders.length, 3); assert.equal(h.live.size, 4);
  assert.equal(h.edition.state.linkReport, h.nativeReport);
  const links = h.el("downloads").children.map(row => row.children[0]);
  h.el("strict").checked = false; h.el("strict").fire("change");
  assert.equal(h.live.size, 0); assert.equal(h.edition.state.linkReport, null);
  for (const link of links) assert.equal(link.getAttribute("href"), null);
});

test("silent programmatic policy changes are fenced before completion", async t => {
  const h = fixture(t, { dom: true }), gate = deferred(); h.onCheck = reply => gate.promise.then(() => reply);
  const pending = h.edition.prepare(); h.el("strict").checked = false; gate.resolve();
  await assert.rejects(pending, code("STALE_SOURCE")); assert.equal(h.live.size, 0); assert.equal(h.edition.state.linkReport, null);
});

for (const event of ["click", "auxclick"]) test(`silent policy changes refuse ${event} activation and remove detached URLs`, async t => {
  const h = fixture(t, { dom: true }); await h.edition.prepare();
  const link = h.el("downloads").children[0].children[0]; h.el("strict").checked = false;
  assert.equal(link.fire(event).defaultPrevented, true); assert.equal(h.live.size, 0); assert.equal(link.href, null);
});

test("a silent raw source edit hides the verdict immediately and prevents bundling", async t => {
  const h = fixture(t); await h.edition.prepare(); h.source += "raw edit";
  assert.equal(h.edition.state.linkReport, null);
  await assert.rejects(h.edition.prepareBundle(), code("STALE_SOURCE")); assert.equal(h.live.size, 0);
});

for (const action of ["change", "busy", "suspend", "dispose"]) test(`${action} retires pending verification without stale output`, async t => {
  const h = fixture(t, { dom: true }), gate = deferred(); h.onCheck = reply => gate.promise.then(() => reply);
  const pending = h.edition.prepare(), rejected = assert.rejects(pending);
  if (action === "change") h.change(); else if (action === "busy") h.setBusy(true); else h.edition[action]();
  await rejected; assert.equal(h.checkReleases, 1); gate.resolve(); await tick();
  assert.equal(h.renders.length, 0); assert.equal(h.live.size, 0); assert.equal(h.edition.state.linkReport, null);
});

test("late old findings cannot replace a newer successful edition", async t => {
  const h = fixture(t), gate = deferred(); let first = true;
  h.onCheck = reply => { if (first) { first = false; return gate.promise; } return reply; };
  const old = h.edition.prepare(), rejected = assert.rejects(old); h.change(); await rejected;
  const ready = await h.edition.prepare(); gate.resolve({ report: linkReport([{}]), sourceLength: 1 }); await tick();
  assert.equal(h.edition.state.linkReport, ready.linkReport); assert.deepEqual(h.edition.state.downloads, ready.downloads);
});

test("findings are inert bounded text and the complete report is not silently truncated", async t => {
  const h = fixture(t, { dom: true });
  h.nativeReport = linkReport(Array.from({ length: 150 }, (_, i) => ({ destination: `<img src=x onerror=alert(${i})>`, message: "<script>text only</script>" })));
  await assert.rejects(h.edition.prepare(), code("BOOK_LINKS_FAILED"));
  assert.equal(h.el("verification").children.length, 102); assert(h.el("verification").children.every(node => node.tagName === "p" && node.children.length === 0));
  assert.match(h.text(), /first 100 of 150/); assert.match(h.text(), /<script>text only<\/script>/);
  assert.equal(h.edition.state.linkReport.summary.findings, 150); assert.equal(h.edition.state.linkReport.chapters[0].findings.length, 150);
});

test("malformed failure reports cannot be displayed as valid navigation findings", async t => {
  const h = fixture(t, { rawPublisher: { ...oldPublisher(), render() { throw linkFailure(linkReport()); } } });
  await assert.rejects(h.edition.prepare(), code("INVALID_LINK_REPORT")); assert.equal(h.edition.state.linkReport, null);
});

test("invalid policy mutations preserve a ready checked edition and unchanged policy is a no-op", async t => {
  const h = fixture(t); await h.edition.prepare(); const previous = h.edition.state;
  for (const value of [null, "true", 1, {}]) assert.throws(() => h.edition.setStrictLinks(value), code("INVALID_OPTIONS"));
  h.edition.setStrictLinks(true); assert.deepEqual(h.edition.state.downloads, previous.downloads); assert.equal(h.live.size, 3);
});

test("blocking and source notifications clear failure reports as well as successful ones", async t => {
  const h = fixture(t, { dom: true }); h.nativeReport = linkReport([{}]);
  await assert.rejects(h.edition.prepare(), code("BOOK_LINKS_FAILED")); h.setBusy(true);
  assert.equal(h.edition.state.linkReport, null); assert.equal(h.el("verification").hidden, true); assert.equal(h.edition.state.canPrepare, false);
  h.setBusy(false); assert.equal(h.edition.state.canPrepare, true);
});

test("suspension does not recheck automatically on resume and clears displayed verification", async t => {
  const h = fixture(t, { dom: true }); await h.edition.prepare(); h.edition.suspend();
  assert.equal(h.live.size, 0); assert.equal(h.el("verification").hidden, true); assert.equal(h.el("strict").disabled, true);
  h.edition.resume(); assert.equal(h.checks.length, 1); assert.equal(h.edition.state.linkReport, null); assert.equal(h.el("strict").disabled, false);
});
