// Production controller, capture selector, worker transport, session facade and
// preview codecs; explicit DOM/collection/native-ABI doubles, not native WASM.
import test from "node:test";
import assert from "node:assert/strict";
import { createBookPreviewControls } from "../demo/book_preview_controls.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { parseBookChapterPreview, prepareBookInput } from "../book_session.mjs";
import { fixtureEngine } from "./book_chapter_worker_fixture.mjs";

const tick = () => new Promise(resolve => setImmediate(resolve));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const gate = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
class Element extends EventTarget {
  value = ""; checked = false; disabled = false; children = []; attributes = new Map();
  contentWindow = {}; srcdoc = ""; textContent = "";
  setAttribute(name, value) { this.attributes.set(name, value); }
  removeAttribute(name) { this.attributes.delete(name); }
  replaceChildren(...children) { this.children = children; }
  click() { this.dispatchEvent(new Event("click")); }
}
const ids = ["preview-build", "preview-auto", "preview-cancel", "preview-pages", "preview-previous",
  "preview-next", "preview-frame", "preview-status", "chapter-source", "chapter-path", "chapters",
  "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"];

function setup(t, { legacyNative = false, legacyAdapter = false, idleTimeoutMs = 30000 } = {}) {
  const el = Object.fromEntries(ids.map(id => [id, new Element()])), host = new EventTarget();
  const root = { querySelector: selector => el[selector.slice(1)], createElement: () => new Element() };
  const sources = [{ path: "intro.md", source: "Intro", role: "chapter" },
    { path: "parts/note.md", source: "Included", role: "include" },
    { path: "end.md", source: "End", role: "chapter" }];
  const state = { snapshots: 0, projects: 0, revision: 0, configuration: 0, sourceBusy: false,
    options: {}, images: [], fonts: [], onCapture: null, invalid: false, forbidSnapshot: false };
  const subscriptions = new Set(), sourceSubscriptions = new Set(), jobs = [], endpoints = [], results = [];
  const collection = {
    get files() { return sources; },
    get revision() { return state.revision; },
    get renderConfigurationRevision() { return state.configuration; },
    subscribe(fn) { subscriptions.add(fn); return () => subscriptions.delete(fn); },
    project() {
      state.projects++;
      return { files: structuredClone(sources), options: structuredClone(state.options) };
    },
    snapshot() {
      state.snapshots++;
      if (state.forbidSnapshot) throw new Error("Unexpected full resource snapshot");
      return prepareBookInput(sources.filter(file => file.role === "chapter"), { ...state.options,
        includeSources: sources.filter(file => file.role === "include"), images: state.images, fontAssets: state.fonts });
    },
  };
  const controls = {
    get sourceBusy() { return state.sourceBusy; },
    captureProject() {
      if (state.invalid) throw new Error("Invalid current settings");
      state.onCapture?.();
    },
    checkpoint: () => JSON.stringify([state.revision, el["chapter-source"].value, el["font-scale"].value]),
    subscribeSourceState(fn) { sourceSubscriptions.add(fn); return () => sourceSubscriptions.delete(fn); },
  };
  const client = createBookWorkerClient({ retainPreview: true, timeoutMs: 3000, idleTimeoutMs,
    workerFactory() {
      const native = fixtureEngine({ legacy: legacyNative }), endpoint = new EventTarget(), scope = new EventTarget();
      endpoint.terminated = false; endpoint.native = native;
      endpoint.terminate = () => { endpoint.terminated = true; native.retained.clear(); };
      endpoint.postMessage = (data, transfer) => {
        const bytes = transfer.reduce((sum, buffer) => sum + buffer.byteLength, 0);
        const owned = structuredClone(data, { transfer }); jobs.push({ data: owned, assetBytes: bytes });
        queueMicrotask(() => { if (!endpoint.terminated) scope.dispatchEvent(new MessageEvent("message", { data: owned })); });
      };
      scope.postMessage = (data, transfer = []) => {
        const owned = structuredClone(data, { transfer });
        queueMicrotask(() => { if (!endpoint.terminated) endpoint.dispatchEvent(new MessageEvent("message", { data: owned })); });
      };
      installBookWorker(scope, native.engine); endpoints.push(endpoint); return endpoint;
    },
  });
  let transform = output => output, delay = null;
  const wrapped = name => async (...args) => {
    const hold = delay; delay = null;
    const result = await client[name](...args);
    results.push(result);
    if (hold) await hold.promise;
    return transform(result);
  };
  const worker = {
    supportsChapterPreview: !legacyAdapter,
    render: wrapped("render"), renderSources: wrapped("renderSources"),
    cancel: client.cancel, cancelPending: client.cancelPending, dispose: client.dispose,
    get retainedInputRevision() { return client.retainedInputRevision; },
    get hasRetainedPreview() { return client.hasRetainedPreview; },
  };
  const timers = new Map(); let serial = 0, nonce = 0;
  const view = createBookPreviewControls({ root, controls, collection, worker, window: host,
    crypto: { getRandomValues(bytes) { bytes.fill(++nonce); return bytes; } },
    timers: { setTimeout(fn, ms) { timers.set(++serial, { fn, ms }); return serial; },
      clearTimeout(id) { timers.delete(id); } },
  });
  t.after(() => view.dispose());
  const message = (detail, { channel, origin = "null", source = el["preview-frame"].contentWindow } = {}) => {
    channel ??= el["preview-frame"].srcdoc.match(/nonce="([a-f0-9]+)"/)?.[1];
    host.dispatchEvent(Object.assign(new Event("message"), { origin, source,
      data: { schemaVersion: 1, channel, ...detail } }));
  };
  const until = async predicate => {
    const deadline = Date.now() + 3000;
    while (!predicate() && Date.now() < deadline) await sleep(2);
    assert(predicate(), `Reader did not settle: ${el["preview-status"].textContent}`);
  };
  const changed = (configuration = false) => {
    state.revision++; if (configuration) state.configuration++;
    for (const listener of subscriptions) listener();
  };
  return { view, client, worker, el, state, sources, collection, controls, jobs, endpoints, results, timers,
    changed, message, until,
    async opened(index) {
      await until(() => el["preview-pages"].value === String(index) && el["preview-frame"].srcdoc.includes("script nonce"));
      message({ kind: "ready" });
    },
    select(index) { el["preview-pages"].value = String(index); el["preview-pages"].dispatchEvent(new Event("change")); },
    async fire(ms) {
      for (const [id, job] of [...timers]) if (job.ms === ms) { timers.delete(id); job.fn(); }
      await tick();
    },
    holdNext() { delay = gate(); return delay; },
    transform(fn) { transform = fn; },
    importBusy(value) { state.sourceBusy = value; for (const listener of sourceSubscriptions) listener(); },
    preview() { return parseBookChapterPreview(results.at(-1).bytes); },
  };
}

async function built(h) { const value = await h.view.build(); h.message({ kind: "ready" }); return value; }
const blank = h => !h.el["preview-frame"].srcdoc.includes("script nonce");

test("actual controller uses selected native preview and complete navigation without site work", async t => {
  const h = setup(t), result = await built(h);
  assert.equal(h.jobs[0].data.format, "chapter-preview");
  assert.equal(h.jobs[0].data.selectedChapter, 0);
  assert.equal(result.pages.length, 2); assert.equal(result.pages[1].source, "end.md");
  assert.equal(Object.hasOwn(result.pages[1], "html"), false);
  assert.equal(h.endpoints[0].native.calls.sites, 0);
  assert.equal(h.el["preview-pages"].children.length, 2);
  assert.equal(h.el["preview-frame"].attributes.get("sandbox"), "allow-scripts");
  assert.match(h.el["preview-status"].textContent, /not PDF pagination/);
});

test("chapter navigation and ordinary source edits never resnapshot or transfer retained resources", async t => {
  const h = setup(t);
  h.state.images = [0, 1].map(i => ({ destination: `figure${i}.png`, bytes: new Uint8Array(6 * 1024 * 1024) }));
  h.state.fonts = [{ slot: "body-regular", bytes: new Uint8Array([1, 2, 3]) }];
  await built(h); h.state.forbidSnapshot = true;
  h.el["preview-next"].click(); assert(blank(h)); await h.opened(1);
  assert.equal(h.state.snapshots, 1); assert.equal(h.state.projects, 1);
  assert.equal(h.jobs[0].assetBytes, 12 * 1024 * 1024 + 3);
  assert.equal(h.jobs[1].assetBytes, 0);
  assert.deepEqual(h.jobs[1].data.sourceUpdate.files, []);
  assert.equal(Object.hasOwn(h.jobs[1].data, "options"), false);
  assert.equal(h.endpoints[0].native.calls.created, 1);
  h.sources[2].source = "Changed 😀"; h.sources[1].source = "Changed include"; h.changed();
  assert(blank(h)); const result = await built(h);
  assert.equal(result.selected, 1);
  const detail = JSON.parse(result.html);
  assert.equal(detail.source, "Changed 😀"); assert.equal(detail.includes[0].source, "Changed include");
  assert.equal(detail.created, 1); assert.equal(detail.updates, 1); assert.equal(detail.sites, 0);
  assert.equal(h.jobs[2].assetBytes, 0); assert.equal(h.state.snapshots, 1);
});

test("same-page fragments reuse current HTML while cross-chapter fragments render their target", async t => {
  const h = setup(t); await built(h);
  const source = structuredClone(h.sources);
  h.message({ kind: "link", href: "#same-anchor" });
  assert.equal(h.jobs.length, 1); assert.match(h.el["preview-frame"].srcdoc, /same-anchor/);
  h.message({ kind: "link", href: "ch1.html#other-anchor" }); await h.opened(1);
  assert.equal(h.jobs.length, 2); assert.match(h.el["preview-frame"].srcdoc, /other-anchor/);
  assert.equal(h.jobs[1].data.selectedChapter, 1); assert.deepEqual(h.sources, source);
  h.el["preview-previous"].click(); await h.opened(0);
  assert.equal(h.endpoints[0].native.calls.created, 1);
});

test("raw editor changes and unrelated iframe messages cannot authorize navigation", async t => {
  const h = setup(t); await built(h);
  for (const meta of [{ origin: "https://example.test" }, { source: {} }, { channel: "f".repeat(32) }])
    h.message({ kind: "link", href: "ch1.html" }, meta);
  h.message({ kind: "link", href: "https://example.test/x" });
  assert.equal(h.jobs.length, 1);
  h.el["chapter-source"].value = "unreported source";
  h.message({ kind: "link", href: "ch1.html" });
  assert(blank(h)); assert.equal(h.jobs.length, 1);
  assert.match(h.el["preview-status"].textContent, /STALE_SOURCE/);
});

test("old reader messages and late navigation failures do not replace a newer preview", async t => {
  const h = setup(t); await built(h);
  const oldChannel = h.el["preview-frame"].srcdoc.match(/nonce="([a-f0-9]+)"/)[1];
  const hold = h.holdNext(); h.select(1);
  await h.until(() => h.results.length === 2);
  assert(blank(h)); assert(h.el["preview-next"].disabled);
  h.message({ kind: "link", href: "ch1.html" }, { channel: oldChannel });
  assert.equal(h.jobs.length, 2);
  h.sources[0].source = "New current"; h.changed(); await built(h);
  const currentFrame = h.el["preview-frame"].srcdoc, currentStatus = h.el["preview-status"].textContent;
  hold.reject(new Error("late navigation failure")); await tick(); await tick();
  assert.equal(h.el["preview-frame"].srcdoc, currentFrame);
  assert.equal(h.el["preview-status"].textContent, currentStatus);
});

test("late navigation success cannot paint after cancellation or overwrite a newer build", async t => {
  const h = setup(t); await built(h);
  const hold = h.holdNext(); h.select(1); await h.until(() => h.results.length === 2);
  h.el["preview-cancel"].click(); assert(blank(h)); assert(!h.client.hasRetainedBook);
  await built(h); const frame = h.el["preview-frame"].srcdoc;
  hold.resolve(); await tick(); await tick();
  assert.equal(h.el["preview-frame"].srcdoc, frame);
});

test("reordering preserves the selected source identity and removal chooses a remaining chapter", async t => {
  const h = setup(t); await built(h); h.select(1); await h.opened(1);
  h.sources.reverse(); h.changed(true);
  const reordered = await built(h);
  assert.equal(reordered.selected, 0); assert.equal(reordered.pages[0].source, "end.md");
  h.sources.splice(0, 1); h.changed(true);
  const removed = await built(h);
  assert.equal(removed.pages[removed.selected].source, "intro.md");
  assert.equal(h.state.snapshots, 3);
});

test("navigation refuses a source removed during capture rather than opening its old numeric slot", async t => {
  const h = setup(t); await built(h);
  h.state.onCapture = () => { h.sources.splice(2, 1); h.changed(true); };
  h.select(1); await tick();
  assert(blank(h)); assert.equal(h.jobs.length, 1);
  assert.match(h.el["preview-status"].textContent, /requested chapter was removed/);
});

test("resource revocation immediately releases the worker and demands a fresh asset-free capture", async t => {
  const h = setup(t); h.state.images = [{ destination: "x.png", bytes: new Uint8Array([1]) }];
  await built(h); const old = h.endpoints[0];
  h.state.images = []; h.changed(true);
  assert(old.terminated); assert(blank(h)); assert.equal(h.client.retainedInputRevision, null);
  await built(h); assert.equal(h.state.snapshots, 2);
  assert.equal(h.jobs[1].assetBytes, 0); assert.deepEqual(h.jobs[1].data.options.images, []);
  assert.equal(h.endpoints.length, 2);
});

test("settings changes and expired idle captures rebuild from current source before navigation", async t => {
  const h = setup(t, { idleTimeoutMs: 20 }); await built(h);
  await h.until(() => !h.client.hasRetainedBook);
  h.select(1); await h.opened(1);
  assert.equal(h.state.snapshots, 2); assert.equal(h.endpoints.length, 2);
  h.state.options.darkMode = "disabled"; h.changed(true);
  await built(h); assert.equal(h.state.snapshots, 3);
  assert.equal(h.jobs.at(-1).data.options.darkMode, "disabled");
});

test("older native binaries use a labelled bounded site fallback; older adapters keep legacy navigation", async t => {
  const h = setup(t, { legacyNative: true }); await built(h);
  assert.match(h.el["preview-status"].textContent, /whole-site fallback/);
  h.select(1); await h.opened(1);
  assert.equal(h.results.at(-1).previewMode, "site-fallback");
  assert.equal(h.preview().html, "<p>Legacy 1: End</p>");
  const old = setup(t, { legacyAdapter: true }); await built(old);
  assert.equal(old.jobs[0].data.format, "preview");
  old.el["preview-next"].click(); assert.equal(old.el["preview-pages"].value, "1");
  assert.equal(old.jobs.length, 1);
});

test("invalid mode, selection, count and source identity never become reader HTML", async t => {
  const changes = [
    value => ({ ...value, format: "book-preview" }),
    value => ({ ...value, previewMode: undefined }),
    value => ({ ...value, selectedChapter: 1 }),
    value => { const body = JSON.parse(new TextDecoder().decode(value.bytes)); body.pages[0].source = "foreign.md";
      return { ...value, bytes: new TextEncoder().encode(JSON.stringify(body)) }; },
    value => { const body = JSON.parse(new TextDecoder().decode(value.bytes)); body.selected = 1;
      return { ...value, bytes: new TextEncoder().encode(JSON.stringify(body)) }; },
    value => { const body = JSON.parse(new TextDecoder().decode(value.bytes)); body.pages.pop();
      return { ...value, bytes: new TextEncoder().encode(JSON.stringify(body)) }; },
  ];
  for (const change of changes) {
    const h = setup(t); h.transform(change);
    await assert.rejects(h.view.build(), { code: "INVALID_BOOK_PREVIEW" });
    assert(blank(h)); assert(!h.client.hasRetainedBook); assert(h.el["preview-pages"].disabled);
  }
});

test("native failure turns automatic preview off without retrying whole-site work", async t => {
  const h = setup(t); await built(h);
  h.el["preview-auto"].checked = true;
  h.sources[0].source = "FAIL_NATIVE"; h.changed();
  await assert.rejects(h.view.build(), { code: "PREVIEW_LIMIT" });
  assert(blank(h)); assert(!h.el["preview-auto"].checked);
  assert.equal(h.jobs.length, 2); assert.equal(h.endpoints[0].native.calls.sites, 0);
  assert(!h.client.hasRetainedBook);
});

test("invalid settings, composition and import block rendering instead of exporting old state", async t => {
  const h = setup(t); await built(h);
  h.state.invalid = true;
  await assert.rejects(h.view.build(), /Invalid current settings/);
  assert.equal(h.jobs.length, 1); h.state.invalid = false;
  h.el["chapter-source"].dispatchEvent(new Event("compositionstart"));
  await assert.rejects(h.view.build(), { code: "BOOK_BUSY" });
  h.el["chapter-source"].dispatchEvent(new Event("compositionend"));
  h.importBusy(true); await assert.rejects(h.view.build(), { code: "BOOK_BUSY" });
  h.importBusy(false); await built(h);
  assert.equal(h.jobs.length, 2);
});

test("automatic source edits coalesce and rebuild only the selected chapter", async t => {
  const h = setup(t); await built(h); h.select(1); await h.opened(1);
  h.el["preview-auto"].checked = true;
  h.sources[2].source = "New end"; h.changed(); h.changed();
  assert.equal([...h.timers.values()].filter(job => job.ms === 600).length, 1);
  await h.fire(600); await h.opened(1);
  assert.equal(h.jobs.length, 3); assert.equal(h.jobs[2].data.selectedChapter, 1);
  assert.equal(h.state.snapshots, 1); assert.equal(h.endpoints[0].native.calls.sites, 0);
});

test("suspension and disposal clear ownership, timers and pending reader results", async t => {
  const h = setup(t); await built(h);
  const hold = h.holdNext(); h.select(1); await h.until(() => h.results.length === 2);
  h.view.suspend(); assert(blank(h)); assert(!h.client.hasRetainedBook);
  hold.resolve(); await tick(); await tick(); assert(blank(h));
  await assert.rejects(h.view.build(), { code: "PREVIEW_CLOSED" });
  h.view.resume(); await built(h);
  h.view.dispose(); assert.equal(h.timers.size, 0); assert(blank(h));
  const count = h.jobs.length; h.el["preview-build"].click(); await tick();
  assert.equal(h.jobs.length, count);
});

test("a missing frame acknowledgement retires the new-format retained worker", async t => {
  const h = setup(t); await h.view.build();
  h.el["preview-frame"].dispatchEvent(new Event("load"));
  await h.fire(10000);
  assert(blank(h)); assert(!h.client.hasRetainedPreview);
  assert.match(h.el["preview-status"].textContent, /PREVIEW_FRAME_FAILED/);
});
