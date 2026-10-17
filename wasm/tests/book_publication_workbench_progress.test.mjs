// Complete production coordinator, edition session and unchanged controller run
// through normal imports. Browser DOM/source host/URL ownership and native
// rendering/checking are explicit doubles; this is not browser/WASM acceptance.
import assert from "node:assert/strict";
import test from "node:test";
import { webcrypto } from "node:crypto";
import { createBookPublicationClient } from "../book_publication.mjs";
import { createBookEditionSession } from "../demo/book_edition.mjs";
import { createBookEditionControls } from "../demo/book_edition_controls.mjs";

const code = expected => cause => cause?.code === expected;
const tick = () => new Promise(resolve => setImmediate(resolve));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
async function until(predicate) {
  const end = performance.now() + 3000;
  while (!predicate()) { assert(performance.now() < end, "expected publication phase did not arrive"); await delay(2); }
}
const report = Object.freeze({ schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
  summary: Object.freeze({ chapters: 1, checked: 0, external: 0, unchecked: 0, findings: 0 }),
  chapters: Object.freeze([Object.freeze({ path: "guide.md", checked: 0, external: 0, unchecked: 0, findings: Object.freeze([]) })]),
});
const progress = (phase, more = {}) => ({ phase, attempt: 1, maxAttempts: 2, completedFormats: 0, totalFormats: 3, ...more });
const initial = () => progress("checking-links");
const rendering = (index = 0, more = {}) => progress("rendering", { completedFormats: index, format: ["pdf", "epub", "site"][index], ...more });
const retrying = () => progress("retrying", { attempt: 2, retryCode: "WORKER_FAILED" });
const verifying = () => progress("verifying-publication", { completedFormats: 3 });
const mime = { pdf: "application/pdf", epub: "application/epub+zip", site: "application/zip" };
function complete(formats = ["pdf", "epub", "site"], links = report) {
  return { format: "book-publication", sourceLength: 1, byteLength: formats.length, linkReport: links,
    outputs: formats.map(format => ({ format: `book-${format}`, sourceLength: 1, mimeType: mime[format],
      blob: () => new Blob(["x"], { type: mime[format] }) })) };
}

// Minimal DOM/event boundary; content is never parsed or evaluated as HTML.
class Element {
  constructor(tag = "div") {
    this.tagName = tag; this.children = []; this.parentElement = null; this.attrs = new Map();
    this.listeners = new Map(); this.checked = false; this.disabled = false; this.hidden = false; this.textContent = "";
  }
  append(...nodes) { for (const node of nodes) { node.parentElement = this; this.children.push(node); } }
  replaceChildren(...nodes) { for (const node of this.children) node.parentElement = null; this.children = []; this.append(...nodes); }
  setAttribute(key, value) { this.attrs.set(key, String(value)); }
  getAttribute(key) { return this.attrs.get(key) ?? null; }
  removeAttribute(key) { this.attrs.delete(key); }
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
  removeEventListener(kind, handler) { this.listeners.set(kind, (this.listeners.get(kind) ?? []).filter(item => item.handler !== handler)); }
  fire(kind) {
    const event = { target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    const path = []; for (let node = this; node; node = node.parentElement) path.push(node);
    for (const capture of [true, false]) for (const node of capture ? path.slice().reverse() : path)
      for (const listener of [...(node.listeners.get(kind) ?? [])]) if (listener.capture === capture) listener.handler(event);
    return event;
  }
}
function fixture(t, { dom = false, real = false, strict = true, failPhase, onState = () => {} } = {}) {
  const h = { source: "A", revision: 0, blocked: false, states: [], requests: [], urls: new Map(), revoked: [],
    captures: 0, clients: 0, disposed: 0, cancelled: 0, checks: 0, finals: 0, calls: [], gates: [] };
  let serial = 0;
  const urls = { createObjectURL(blob) { const url = `blob:test/${++serial}`; h.urls.set(url, blob); return url; },
    revokeObjectURL(url) { h.urls.delete(url); h.revoked.push(url); } };
  const capture = () => ({ files: [{ path: "guide.md", source: h.source }], options: { includeSources: [], images: [], fontAssets: [] } });
  const checkpoint = () => JSON.stringify([h.source, h.revision]);
  const listeners = new Set(), sourceListeners = new Set();
  h.change = () => { h.revision++; for (const fn of listeners) fn(); };
  h.sourceChange = () => { for (const fn of sourceListeners) fn(); };
  h.publisher = real ? createBookPublicationClient({
    captureInput: (files, options) => structuredClone({ files, options }),
    checkLinks() { h.checks++; return { report, sourceLength: 1 }; },
    async checkRetainedLinks() { h.finals++; await h.onFinal?.(); return report; },
    createClient() {
      const id = ++h.clients;
      h.onCreate?.(id);
      let revision = 0;
      async function render(format) {
        h.calls.push({ id, format });
        if (id === 1 && format === failPhase) throw Object.assign(Error("lost endpoint"), { code: "WORKER_FAILED" });
        await h.onRender?.(id, format);
        return { format: `book-${format}`, bytes: Uint8Array.of(id), sourceLength: 1, retainedInputRevision: ++revision };
      }
      return { render: (_files, format) => render(format),
        renderSourceUpdate(changes, format, opts) { assert.deepEqual(changes, []); assert.equal(opts.expectedRevision, revision); return render(format); },
        dispose() { h.disposed++; } };
    },
  }) : {
    render(_files, formats, _options, request) {
      const gate = deferred(); h.gates.push(gate); h.requests.push(request);
      h.onRequest?.(request);
      return gate.promise.then(value => value ?? complete(formats));
    }, cancel() { h.cancelled++; }, dispose() { h.disposed++; },
  };
  if (dom) {
    const root = new Element("document"), parent = new Element(), title = new Element("h2"), editor = new Element("textarea");
    title.id = "publish-title"; editor.id = "chapter-source"; parent.append(title); root.append(parent, editor);
    root.createElement = tag => new Element(tag);
    root.createTextNode = text => Object.assign(new Element("text"), { textContent: text });
    h.root = root; h.editor = editor; h.el = key => root.querySelector(`#book-edition-${key}`);
    h.edition = createBookEditionControls({ root, publisher: h.publisher, urls, crypto: webcrypto,
      controls: { captureProject() { h.captures++; }, checkpoint, get sourceBusy() { return h.blocked; },
        subscribeSourceState(fn) { sourceListeners.add(fn); return () => sourceListeners.delete(fn); } },
      collection: { get revision() { return h.revision; }, get files() { return capture().files; }, snapshot: capture,
        subscribe(fn) { listeners.add(fn); return () => listeners.delete(fn); } },
    });
    if (!strict) { h.el("strict").checked = false; h.el("strict").fire("change"); }
  } else {
    h.edition = createBookEditionSession({ publisher: h.publisher, urls, crypto: webcrypto, strictLinks: strict,
      checkpoint, isBlocked: () => h.blocked, capture() { h.captures++; return capture(); },
      onState(state) { h.states.push(state); onState(state, h); },
    });
    listeners.add(() => h.edition.changed()); sourceListeners.add(() => h.edition.invalidate());
  }
  h.emit = value => h.requests.at(-1).onProgress(value);
  t.after(() => { h.edition.dispose(); for (const gate of h.gates) gate.resolve(); });
  return h;
}

test("real recovery, session and live controller show each stage without premature downloads", async t => {
  const h = fixture(t, { dom: true, real: true, failPhase: "epub" }), finish = deferred();
  h.onFinal = () => finish.promise;
  const pending = h.edition.prepare();
  await until(() => h.edition.state.publicationProgress?.phase === "retrying");
  assert.match(h.el("status").textContent, /Restarting the complete edition.*attempt 2 of 2.*same captured book/);
  assert.equal(h.el("status").getAttribute("aria-live"), "polite");
  assert.equal(h.el("status").getAttribute("role"), "status");
  assert.equal(h.el("panel").getAttribute("aria-busy"), "true");
  assert.equal(h.el("prepare").disabled, true); assert.equal(h.el("cancel").disabled, false);
  assert.equal(h.el("bundle").disabled, true); assert.equal(h.urls.size, 0);
  assert.equal(h.el("downloads").hidden, true); assert.equal(h.edition.state.linkReport, null);
  await until(() => h.finals === 1);
  assert.match(h.el("status").textContent, /Attempt 2 of 2: Verifying the rendered edition/);
  assert.equal(h.urls.size, 0); assert.equal(h.edition.state.phase, "busy");
  finish.resolve(); const state = await pending;
  assert.equal(state.phase, "ready"); assert.equal(state.publicationProgress, null);
  assert.equal(state.linkReport, report); assert.equal(h.urls.size, 3); assert.equal(h.captures, 1);
  assert.deepEqual(h.calls.filter(call => call.id === 2).map(call => call.format), ["pdf", "epub", "site"]);
  assert.equal(h.checks, 2); assert.equal(h.clients, 2); assert.equal(h.disposed, 2);
  assert.equal(h.el("panel").getAttribute("aria-busy"), "false");
  assert.equal(h.el("downloads").children.length, 3);
  for (const blob of h.urls.values()) assert.deepEqual([...new Uint8Array(await blob.arrayBuffer())], [2]);
});

test("real publication phase messages follow the selected order and never claim percentage completion", async t => {
  const h = fixture(t, { real: true }); h.edition.setFormats(["site", "pdf"]);
  await h.edition.prepare();
  const phases = h.states.filter(state => state.publicationProgress);
  assert.deepEqual(phases.map(state => state.publicationProgress.phase), ["checking-links", "rendering", "rendering", "verifying-publication"]);
  assert.match(phases[1].message, /Rendering HTML-site ZIP \(1 of 2\)/);
  assert.match(phases[2].message, /Rendering PDF \(2 of 2\)/);
  assert(phases.every(state => state.phase === "busy" && !state.canBundle && state.downloads.length === 0 && state.linkReport === null));
});

test("non-strict factory loss before any rendering phase still reports recovery", async t => {
  const h = fixture(t, { real: true, strict: false });
  h.onCreate = id => { if (id === 1) throw Object.assign(Error("startup failed"), { code: "WORKER_FAILED" }); };
  await h.edition.prepare();
  assert(h.states.some(state => state.publicationProgress?.phase === "retrying"));
  assert.equal(h.checks, 0); assert.equal(h.finals, 0); assert.equal(h.edition.state.linkReport, null);
});

for (const action of ["cancel", "raw-input", "composition", "policy", "formats", "suspend", "dispose"]) {
  test(`workbench ${action} during real backoff stops replacement-worker startup`, async t => {
    const h = fixture(t, { dom: true, real: true, failPhase: "epub" });
    const pending = h.edition.prepare(), rejected = assert.rejects(pending);
    await until(() => h.edition.state.publicationProgress?.phase === "retrying");
    if (action === "cancel") h.el("cancel").fire("click");
    else if (action === "raw-input") h.editor.fire("input");
    else if (action === "composition") h.editor.fire("compositionstart");
    else if (action === "policy") { h.el("strict").checked = false; h.el("strict").fire("change"); }
    else if (action === "formats") { h.el("epub").checked = false; h.el("epub").fire("change"); }
    else h.edition[action]();
    await rejected; await delay(125);
    assert.equal(h.clients, 1); assert.equal(h.urls.size, 0); assert.equal(h.edition.state.publicationProgress, null);
    assert(!/Restarting/.test(h.el("status").textContent));
    if (action === "suspend") { h.edition.resume(); assert.equal(h.clients, 1); }
  });
}

for (const action of ["cancel", "invalidate", "changed", "suspend", "dispose"]) {
  test(`uncooperative progress after ${action} cannot repaint or resurrect the edition`, async t => {
    const h = fixture(t); const pending = h.edition.prepare(), rejected = assert.rejects(pending);
    h.emit(initial()); h.edition[action](); await rejected;
    const before = h.edition.state, count = h.states.length;
    h.emit(rendering()); h.emit(retrying()); h.emit(verifying());
    assert.equal(h.states.length, count); assert.equal(h.edition.state.message, before.message);
    assert.equal(h.edition.state.publicationProgress, null); assert.equal(h.urls.size, 0);
    h.gates[0].resolve(); await tick(); assert.equal(h.urls.size, 0);
  });
}

test("old-job progress cannot overwrite a new job or a completed edition", async t => {
  const h = fixture(t), old = h.edition.prepare(), rejected = assert.rejects(old);
  const oldObserver = h.requests[0].onProgress; h.edition.cancel(); await rejected;
  const next = h.edition.prepare(); h.emit(rendering());
  const message = h.edition.state.message;
  oldObserver(retrying()); assert.equal(h.edition.state.message, message);
  h.gates[1].resolve(); await next; const ready = h.edition.state;
  h.emit(verifying()); oldObserver(rendering());
  assert.equal(h.edition.state.message, ready.message); assert.equal(h.edition.state.publicationProgress, null);
  assert.equal(h.urls.size, 3); h.gates[0].reject(Error("late failure")); await tick();
});

test("silent source changes abort on progress, not merely when output eventually returns", async t => {
  const h = fixture(t), pending = h.edition.prepare(), rejected = assert.rejects(pending, code("STALE_SOURCE"));
  h.emit(initial()); h.source = "new editor text";
  assert.equal(h.edition.state.publicationProgress, null);
  h.emit(rendering()); await rejected;
  assert.equal(h.requests[0].signal.aborted, true); assert.equal(h.cancelled, 1); assert.equal(h.urls.size, 0);
});

test("source mutation by the view observer cancels before the next real native operation", async t => {
  const h = fixture(t, { real: true, onState(state, host) {
    if (state.publicationProgress?.phase === "rendering") host.source = "changed inside view";
  } });
  await assert.rejects(h.edition.prepare(), code("STALE_SOURCE"));
  assert.equal(h.calls.length, 0); assert.equal(h.clients, 1); assert.equal(h.disposed, 1); assert.equal(h.urls.size, 0);
});

test("busy source host and source changes while inspecting progress explicitly abort", async t => {
  for (const mutate of [h => { h.blocked = true; }, h => { h.revision++; }]) {
    const h = fixture(t), pending = h.edition.prepare(), rejected = assert.rejects(pending, code("STALE_SOURCE"));
    h.emit(initial());
    const candidate = new Proxy(rendering(), { ownKeys(target) { mutate(h); return Reflect.ownKeys(target); } });
    h.emit(candidate); await rejected;
    assert.equal(h.requests[0].signal.aborted, true); assert.equal(h.urls.size, 0);
  }
});

test("progress state is an owned frozen primitive record, not the callback object", async t => {
  const h = fixture(t), pending = h.edition.prepare(), value = rendering(); h.emit(value);
  const stored = h.edition.state.publicationProgress;
  assert(Object.isFrozen(stored)); assert.notEqual(stored, value); value.format = "<script>"; value.attempt = 999;
  assert.equal(stored.format, "pdf"); assert.equal(stored.attempt, 1);
  assert.deepEqual(Object.keys(stored).sort(), ["attempt", "completedFormats", "format", "maxAttempts", "phase", "totalFormats"]);
  h.gates[0].resolve(); await pending;
});

test("malformed metadata, accessors and host exceptions are ignored without changing outcome", async t => {
  const h = fixture(t), pending = h.edition.prepare(); h.emit(initial());
  let reads = 0;
  const bad = [null, [], "rendering", progress("ready"), rendering(0, { attempt: 0 }), rendering(0, { attempt: Infinity }),
    rendering(0, { maxAttempts: 4 }), rendering(0, { completedFormats: 0.5 }), rendering(0, { totalFormats: 2 }),
    rendering(0, { format: "<img src=x>" }), rendering(0, { retryCode: "WORKER_FAILED" }),
    progress("retrying", { attempt: 2, retryCode: "INVALID_LINK_REPORT" }), verifyingWithWrongCount(),
    { ...rendering(), output: Uint8Array.of(1) }, { ...rendering(), [Symbol("hidden")]: 1 },
    { ...rendering(), get attempt() { reads++; throw Error("getter invoked"); } },
    new Proxy({}, { getPrototypeOf() { throw Error("host inspection failed"); } }),
  ];
  const count = h.states.length, text = h.edition.state.message;
  for (const value of bad) h.emit(value);
  assert.equal(reads, 0); assert.equal(h.states.length, count); assert.equal(h.edition.state.message, text);
  h.gates[0].resolve(); const state = await pending; assert.equal(state.phase, "ready");
  function verifyingWithWrongCount() { return progress("verifying-publication", { completedFormats: 2 }); }
});

test("duplicate, regressive and skipped-attempt notifications cannot undo progress", async t => {
  const h = fixture(t), pending = h.edition.prepare(); h.emit(initial()); h.emit(rendering(1));
  const count = h.states.length;
  for (const value of [initial(), rendering(), rendering(1), rendering(2, { attempt: 2 }),
    rendering(2, { maxAttempts: 3 }), progress("retrying", { attempt: 3, maxAttempts: 3, retryCode: "WORKER_FAILED" })]) h.emit(value);
  assert.equal(h.states.length, count);
  h.emit(retrying()); h.emit(progress("checking-links", { attempt: 2 })); h.emit(rendering(0, { attempt: 2 }));
  const resetCount = h.states.length; h.emit(rendering(2)); h.emit(retrying());
  assert.equal(h.states.length, resetCount); assert.equal(h.edition.state.publicationProgress.attempt, 2);
  assert.match(h.edition.state.message, /Attempt 2 of 2: Rendering PDF \(1 of 3\)/);
  h.gates[0].resolve(); await pending;
});

test("non-strict jobs cannot display invented verification phases", async t => {
  const h = fixture(t, { strict: false }), pending = h.edition.prepare(), count = h.states.length;
  h.emit(initial()); h.emit(verifying()); assert.equal(h.states.length, count);
  h.emit(rendering()); assert.match(h.edition.state.message, /Rendering PDF/);
  h.gates[0].resolve(complete(undefined, undefined)); await pending;
  assert.equal(h.edition.state.linkReport, null);
});

test("a verification progress event cannot substitute for the required final report", async t => {
  const h = fixture(t), pending = h.edition.prepare(), rejected = assert.rejects(pending, code("UNSUPPORTED_BOOK_VERIFICATION"));
  h.emit(initial()); h.emit(verifying()); assert.equal(h.urls.size, 0); assert.equal(h.edition.state.linkReport, null);
  h.gates[0].resolve(complete(undefined, null)); await rejected;
  assert.equal(h.urls.size, 0); assert.equal(h.edition.state.publicationProgress, null);
});

test("failure after all progress phases still publishes no partial edition", async t => {
  const h = fixture(t), pending = h.edition.prepare(), rejected = assert.rejects(pending, code("INVALID_LINK_REPORT"));
  h.emit(initial()); h.emit(rendering()); h.emit(rendering(1)); h.emit(rendering(2)); h.emit(verifying());
  h.gates[0].reject(Object.assign(Error("bad final report"), { code: "INVALID_LINK_REPORT" })); await rejected;
  assert.equal(h.urls.size, 0); assert.equal(h.edition.state.phase, "error"); assert.equal(h.edition.state.publicationProgress, null);
});

test("publishers without progress remain compatible and successful", async t => {
  const h = fixture(t), pending = h.edition.prepare();
  assert.equal(h.edition.state.publicationProgress, null); h.gates[0].resolve(); await pending;
  assert.equal(h.urls.size, 3); assert.equal(h.edition.state.phase, "ready");
});

test("throwing view callbacks do not change publication or recovery behavior", async t => {
  const h = fixture(t, { real: true, failPhase: "epub", onState(state) {
    if (state.publicationProgress) throw Error("view failed");
  } });
  await h.edition.prepare(); assert.equal(h.urls.size, 3); assert.equal(h.clients, 2);
});

test("cancel from the initial synchronous progress callback settles safely", async t => {
  const h = fixture(t); h.onRequest = request => request.onProgress(initial());
  h.source = "first";
  // Mutate the source during data admission, before publisher.render returns.
  h.onRequest = request => { h.source = "different"; request.onProgress(initial()); };
  await assert.rejects(h.edition.prepare(), code("STALE_SOURCE"));
  assert.equal(h.requests[0].signal.aborted, true); assert.equal(h.urls.size, 0);
});

test("late rendering progress cannot repaint or revoke a completed delivery bundle", async t => {
  const h = fixture(t), pending = h.edition.prepare(); h.emit(initial()); h.gates[0].resolve(); await pending;
  await h.edition.prepareBundle(); const ready = h.edition.state;
  h.emit(rendering()); h.emit(retrying());
  assert.equal(h.edition.state.message, ready.message); assert.equal(h.edition.state.publicationProgress, null);
  assert.equal(h.urls.size, 4); assert(ready.bundleByteLength > 0);
});
