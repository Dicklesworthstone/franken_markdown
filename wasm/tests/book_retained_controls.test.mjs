import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { createBookCollection, readPortableBookProject } from "../demo/book_collection.mjs";
import { createBookPreviewControls } from "../demo/book_preview_controls.mjs";
import { createBookWorkerClient, createRetainedBookPreview, installBookWorker } from "../book_worker.mjs";
import { renderBookPreview } from "../book_site_preview.mjs";
import { nativeFixture } from "./book_retained_fixture.mjs";

// Complete production collection, client, protocol, native facade, ZIP codec,
// frame serializer and preview controller. Only DOM/editor and native ABI are
// adapters here. Transport uses actual structuredClone transfer ownership.
class Element extends EventTarget {
  value = ""; checked = false; disabled = false; textContent = ""; srcdoc = "";
  children = []; attrs = {}; contentWindow = {};
  replaceChildren(...nodes) { this.children = nodes; }
  setAttribute(key, value) { this.attrs[key] = value; }
  removeAttribute(key) { delete this.attrs[key]; }
  emit(kind) { this.dispatchEvent(new Event(kind)); }
}
const ids = "preview-build preview-auto preview-cancel preview-pages preview-previous preview-next preview-frame preview-status chapter-source chapter-path chapters title author lang font dark-mode font-scale toc page-numbers".split(" ");
const sleep = () => new Promise(resolve => setTimeout(resolve, 0));
function fixture(t, { legacy = false } = {}) {
  const el = Object.fromEntries(ids.map(id => [id, new Element()]));
  const root = { querySelector: selector => el[selector.slice(1)], createElement: () => new Element() };
  const window = new EventTarget(), sourceListeners = new Set(), endpoints = [];
  const collection = createBookCollection();
  collection.append({ chapters: [{ path: "a.md", source: "\ufeff# A\r\n" }, { path: "b.md", source: "# B\n" }],
    includeSources: [{ path: "shared.md", source: "shared\n" }], images: [{ destination: "x.png", bytes: new Uint8Array([1]) }] });
  collection.setFonts([{ slot: "body-regular", name: "Test.ttf", bytes: new Uint8Array([2]) }], collection.revision);
  el["chapter-source"].value = collection.files[0].source;
  let sourceBusy = false;
  const controls = {
    get sourceBusy() { return sourceBusy; },
    checkpoint: () => JSON.stringify([collection.revision, el["chapter-source"].value]),
    captureProject() { collection.edit(0, collection.files[0].path, el["chapter-source"].value); return collection.project(); },
    subscribeSourceState(listener) { sourceListeners.add(listener); return () => sourceListeners.delete(listener); },
  };
  let timerId = 0;
  const tasks = new Map();
  const timers = { setTimeout(fn, delay) { const id = ++timerId; tasks.set(id, { fn, delay }); return id; },
    clearTimeout(id) { tasks.delete(id); } };
  const client = createBookWorkerClient({ retainPreview: true, workerFactory() {
    const native = nativeFixture(), retained = createRetainedBookPreview(native.engine, renderBookPreview);
    let handler;
    const endpoint = new Element(); endpoint.stats = native.stats; endpoint.terminated = 0;
    endpoint.postMessage = (message, transfer) => {
      const data = structuredClone(message, { transfer });
      queueMicrotask(() => { if (!endpoint.terminated) handler({ data }); });
    };
    endpoint.terminate = () => { endpoint.terminated++; retained.clear(); };
    installBookWorker({ addEventListener(_kind, fn) { handler = fn; }, postMessage(value, transfer) {
      const data = structuredClone(value, { transfer });
      queueMicrotask(() => { if (!endpoint.terminated) endpoint.dispatchEvent(new MessageEvent("message", { data })); });
    } }, { ...native.engine,
      renderBookPreview: (files, options) => renderBookPreview(native.engine, files, options),
      renderRetainedBookPreview: retained.render, clearRetainedBookPreview: retained.clear });
    endpoints.push(endpoint); return endpoint;
  } });
  const worker = legacy ? { render: client.render, cancel: client.cancel, dispose: client.dispose } : client;
  const model = legacy ? new Proxy(collection, { get(target, key) { return key === "renderConfigurationRevision" ? undefined : target[key]; } }) : collection;
  const preview = createBookPreviewControls({ root, controls, collection: model, worker, window, timers });
  t.after(() => { preview.dispose(); collection.dispose(); });
  return { el, root, collection, controls, preview, client, endpoints, tasks, window,
    type(source) { el["chapter-source"].value = source; controls.captureProject(); el["chapter-source"].emit("input"); },
    setBusy(value) { sourceBusy = value; for (const listener of sourceListeners) listener(); },
    message(kind, overrides = {}) {
      const channel = el["preview-frame"].srcdoc.match(/nonce="([a-f0-9]{32})"/)?.[1];
      const event = new Event("message"); Object.assign(event, { origin: "null", source: el["preview-frame"].contentWindow,
        data: { schemaVersion: 1, channel, kind, ...overrides } }); window.dispatchEvent(event);
    },
  };
}

test("source edits and source-only batches advance source revision but retain configuration identity", () => {
  const book = createBookCollection(); assert.equal(book.renderConfigurationRevision, 0);
  book.append({ chapters: [{ path: "a.md", source: "# A" }], images: [], includeSources: [{ path: "i.md", source: "I" }] });
  const config = book.renderConfigurationRevision, revision = book.revision;
  const calls = []; book.subscribe((...args) => calls.push(args));
  book.edit(0, "a.md", "# Changed");
  assert.equal(book.renderConfigurationRevision, config); assert.equal(book.revision, revision + 1);
  const files = book.files; files[1].source = "new include"; book.replaceSources(files, book.revision);
  assert.equal(book.renderConfigurationRevision, config); assert.deepEqual(calls, [[], []], "observer API remains unchanged");
  assert(!JSON.stringify(book.project()).includes("renderConfigurationRevision"));
  book.edit(0, "renamed.md", "# Changed"); assert.equal(book.renderConfigurationRevision, config + 1);
  book.dispose(); assert.throws(() => book.renderConfigurationRevision, { code: "SESSION_DISPOSED" });
});

test("configuration changes and restore never keep the source-only identity", async () => {
  const book = createBookCollection();
  book.append({ chapters: [{ path: "a.md", source: "A" }, { path: "b.md", source: "B" }], images: [] });
  const operations = [
    () => book.configure({ title: "Next" }), () => book.setPage({ size: "a4" }, book.revision),
    () => book.setFonts([{ slot: "body-regular", name: "x.ttf", bytes: new Uint8Array([1]) }], book.revision),
    () => book.revokeFonts(), () => book.revokeImages(), () => book.move(0, 1),
    () => book.setRole(0, "include"), () => book.remove(0), () => book.replaceProject(book.project()),
  ];
  for (const operation of operations) { const previous = book.renderConfigurationRevision; operation(); assert.equal(book.renderConfigurationRevision, previous + 1); }
  const review = await readPortableBookProject(new File([(await book.portableDownload()).blob], "book.json"));
  const previous = book.renderConfigurationRevision;
  book.replacePortableProject(review, book.revision, { authorizeResources: true });
  assert.equal(book.renderConfigurationRevision, previous + 1); book.dispose();
});

test("publisher builds, source input and include edits reuse the existing native preview session", async t => {
  const f = fixture(t); const first = await f.preview.build(); f.message("ready");
  assert.equal(f.client.hasRetainedPreview, true);
  f.type("# Edited\n"); assert.match(f.el["preview-frame"].srcdoc, /Build a preview/);
  assert.equal(f.endpoints[0].terminated, 0); assert.equal(f.el["preview-cancel"].disabled, false);
  f.collection.edit(2, "shared.md", "changed snippet");
  const second = await f.preview.build();
  assert.equal(f.endpoints.length, 1); assert.equal(f.endpoints[0].stats.creates, 1);
  assert.equal(f.endpoints[0].stats.updates.length, 1);
  assert.deepEqual(f.endpoints[0].stats.updates[0].changes, [f.collection.files[0], f.collection.files[2]].map(({ path, source }) => ({ path, source })));
  assert.notDeepEqual(first, second); assert.match(second.pages[0].html, /# Edited/);
  await f.preview.build(); assert.equal(f.endpoints[0].stats.updates.length, 1);
});

test("resource and presentation changes immediately release idle native state", async t => {
  const mutations = [f => f.collection.revokeImages(), f => f.collection.revokeFonts(),
    f => f.collection.setFonts([{ slot: "body-regular", name: "Test.ttf", bytes: new Uint8Array([3]) }], f.collection.revision),
    f => f.collection.setPage({ size: "a4" }, f.collection.revision),
    f => f.collection.configure({ ...f.collection.options, title: "New" }),
    f => f.collection.move(0, 1), f => f.collection.setRole(1, "include"),
    f => f.collection.replaceProject(f.collection.project())];
  for (const mutate of mutations) {
    const f = fixture(t); await f.preview.build(); mutate(f);
    assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.client.hasRetainedPreview, false);
    assert.match(f.el["preview-frame"].srcdoc, /Build a preview/); f.preview.dispose();
  }
});

test("an edit cancels active work, while a subsequent retry builds the current snapshot", async t => {
  const f = fixture(t), pending = f.preview.build();
  f.type("newest"); await assert.rejects(pending, { code: "EXPORT_CANCELLED" });
  assert.equal(f.endpoints[0].terminated, 1);
  assert.match((await f.preview.build()).pages[0].html, /newest/);
});

test("silent editor changes cannot publish a completed cached preview", async t => {
  const f = fixture(t); await f.preview.build();
  const pending = f.preview.build(); f.el["chapter-source"].value = "silent newer text";
  await assert.rejects(pending, { code: "STALE_SOURCE" });
  assert.equal(f.client.hasRetainedPreview, false); assert.equal(f.endpoints[0].terminated, 1);
  assert.match(f.el["preview-frame"].srcdoc, /Build a preview/);
});

test("capturing pending text does not schedule a duplicate automatic build", async t => {
  const f = fixture(t); await f.preview.build(); f.el["preview-auto"].checked = true;
  f.el["chapter-source"].value = "not yet captured";
  await f.preview.build();
  assert.equal([...f.tasks.values()].filter(task => task.delay === 600).length, 0);
  assert.equal(f.endpoints.length, 1); assert.equal(f.endpoints[0].stats.updates.length, 1);
});

test("opted-in automatic preview coalesces edits and reuses an already-idle book", async t => {
  const f = fixture(t); await f.preview.build(); f.el["preview-auto"].checked = true;
  f.type("one"); f.type("two"); f.type("three");
  const tasks = [...f.tasks.entries()].filter(([, task]) => task.delay === 600);
  assert.equal(tasks.length, 1); f.tasks.delete(tasks[0][0]); tasks[0][1].fn();
  for (let i = 0; i < 10 && f.endpoints[0].stats.sites < 2; i++) await sleep();
  assert.equal(f.endpoints[0].stats.sites, 2); assert.equal(f.endpoints.length, 1);
  assert.equal(f.endpoints[0].stats.updates[0].changes[0].source, "three");
});

test("imports and composition release idle state and block rebuild admission", async t => {
  const f = fixture(t); await f.preview.build(); f.setBusy(true);
  assert.equal(f.endpoints[0].terminated, 1); assert.equal(f.el["preview-build"].disabled, true);
  await assert.rejects(f.preview.build(), { code: "BOOK_BUSY" });
  f.setBusy(false); await f.preview.build();
  f.el["chapter-source"].emit("compositionstart"); assert.equal(f.endpoints[1].terminated, 1);
  await assert.rejects(f.preview.build(), { code: "BOOK_BUSY" });
  f.el["chapter-source"].emit("compositionend"); assert.equal(f.el["preview-build"].disabled, false);
});

test("explicit clear, suspension and disposal release retained workers without changing source", async t => {
  const f = fixture(t), before = f.collection.project();
  await f.preview.build(); f.el["preview-cancel"].emit("click"); assert.equal(f.endpoints[0].terminated, 1);
  await f.preview.build(); f.preview.suspend(); assert.equal(f.endpoints[1].terminated, 1);
  f.preview.resume(); assert.equal(f.endpoints.length, 2, "resume must not render automatically");
  await f.preview.build(); f.preview.dispose(); assert.equal(f.endpoints[2].terminated, 1);
  assert.deepEqual(f.collection.project(), before);
});

test("a failed iframe acknowledgment releases the idle cache rather than retaining unusable output", async t => {
  const f = fixture(t); await f.preview.build();
  const timeout = [...f.tasks.values()].find(task => task.delay === 10000); timeout.fn();
  assert.equal(f.endpoints[0].terminated, 1); assert.match(f.el["preview-status"].textContent, /PREVIEW_FRAME_FAILED/);
});

test("legacy embedding adapters remain compatible without retention lifecycle support", async t => {
  const f = fixture(t, { legacy: true }); await f.preview.build(); f.type("old host");
  assert.equal(f.endpoints[0].terminated, 1); await f.preview.build(); assert.equal(f.endpoints.length, 2);
});

test("no-op, invalid and stale model operations do not advance the configuration counter", () => {
  const book = createBookCollection(); book.append({ chapters: [{ path: "a.md", source: "A" }], images: [] });
  const previous = book.renderConfigurationRevision;
  book.edit(0, "a.md", "A"); book.setPage(undefined, book.revision); book.revokeFonts();
  assert.throws(() => book.setPage({ size: "bad" }, book.revision));
  assert.throws(() => book.setPage({ size: "a4" }, book.revision - 1), { code: "STALE_SOURCE" });
  assert.equal(book.renderConfigurationRevision, previous); book.dispose();
});
