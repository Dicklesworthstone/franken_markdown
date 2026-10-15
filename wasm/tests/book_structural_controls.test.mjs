import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookWorkerClient, installBookWorker, createRetainedBook } from "../book_worker.mjs";
import { createBookPdfProof } from "../book_pdf_proof.mjs";

// Execute the production controllers in the same realm as the real collection,
// capture, transport and retained-book coordinator. Only DOM, native rendering
// and unrelated frame/search adapters are doubles; this is not a WASM or browser
// rendering test. Unexpected use of a doubled peripheral fails explicitly.
const dataModule = source => `data:text/javascript,${encodeURIComponent(source)}`;
async function controller(path, doubles) {
  const url = new URL(path, import.meta.url);
  const source = readFileSync(url, "utf8").replace(/from\s+(["'])([^"']+)\1/g,
    (_, quote, specifier) => `from ${JSON.stringify(doubles[specifier] ?? new URL(specifier, url).href)}`);
  return import(dataModule(source));
}
const { createBookControls } = await controller("../demo/book_controls.mjs", {
  "./book_source_search.mjs": dataModule('export function bookEditorOffset() { throw Error("Source search is not exercised"); }'),
});
const { createBookPreviewControls } = await controller("../demo/book_preview_controls.mjs", {
  "../book_preview_frame.mjs": dataModule('export function bookPreviewFrame(html) { return "TEST FRAME: " + html; }'),
  "../book_site_preview.mjs": dataModule('export function parseBookPreview() { throw Error("Legacy site preview is not exercised"); } export function resolveBookPreviewLink() { throw Error("Frame navigation is not exercised"); }'),
});
const tick = () => new Promise(resolve => setImmediate(resolve));
class Element {
  constructor(tag = "div") {
    this.tag = tag; this.children = []; this.listeners = new Map(); this.attributes = {};
    this.value = ""; this.checked = false; this.disabled = false; this.hidden = false;
    this.textContent = ""; this.srcdoc = ""; this.contentWindow = {};
    this.selectionStart = this.selectionEnd = this.scrollTop = 0;
  }
  addEventListener(kind, listener) {
    if (!this.listeners.has(kind)) this.listeners.set(kind, new Set());
    this.listeners.get(kind).add(listener);
  }
  removeEventListener(kind, listener) { this.listeners.get(kind)?.delete(listener); }
  fire(kind, extra = {}) {
    const event = { target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...extra };
    for (const listener of [...(this.listeners.get(kind) ?? [])]) listener(event);
    return event;
  }
  replaceChildren(...nodes) { this.children = nodes; }
  setAttribute(key, value) { this.attributes[key] = value; }
  removeAttribute(key) { delete this.attributes[key]; delete this[key]; }
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
  focus() {}
}
const file = (path, source = path) => ({ path, source });
const observed = (object, properties) => Object.create(object, Object.fromEntries(
  Object.entries(properties).map(([key, value]) => [key, { value, configurable: true }]),
));

function rig(t, { legacy = false, automatic = false, idleTimeoutMs = 30000 } = {}) {
  const nodes = new Map(), el = id => {
    if (!nodes.has(id)) nodes.set(id, new Element(id === "chapter-source" ? "textarea" : "div"));
    return nodes.get(id);
  };
  const root = { querySelector: selector => selector === "#publish-title" ? null : el(selector.slice(1)),
    createElement: tag => new Element(tag) };
  const collection = createBookCollection();
  collection.append({ chapters: [file("one.md"), file("two.md")],
    includeSources: [file("shared.txt", "Shared\r\n")],
    images: [{ destination: "cover.png", bytes: new Uint8Array([1, 2]) }] });
  collection.setFonts([{ slot: "body-regular", name: "fixture.ttf", bytes: new Uint8Array([3, 4]) }], collection.revision);
  let snapshots = 0;
  const host = observed(collection, { snapshot() { snapshots++; return collection.snapshot(); } });
  const lanes = {};
  function lane(name) {
    const endpoints = [], messages = [], state = { hold: null, fail: false, badAck: false };
    let created = 0;
    const client = createBookWorkerClient({ retainBook: true, idleTimeoutMs, workerFactory() {
      let handler, terminated = false;
      const native = { async createBook(files, options) {
        created++;
        let current = files, includes = options.includeSources, revision = 0;
        const receipt = () => ({ files: current, includes,
          images: options.images.map(asset => [...asset.bytes]),
          fonts: options.fontAssets.map(asset => [...asset.bytes]), title: options.title });
        const output = async format => {
          if (state.hold) { const wait = state.hold; state.hold = null; await wait; }
          if (state.fail) throw Object.assign(Error("Native fixture failure"), { code: "NATIVE_FAILURE" });
          const bytes = new TextEncoder().encode((format === "pdf" ? "%PDF-1.7\n" : "") + JSON.stringify(receipt()));
          return { bytes, sourceLength: [...current, ...includes].reduce((sum, entry) => sum + new TextEncoder().encode(entry.source).length, 0) };
        };
        return { receipt, get sourceRevision() { return revision; },
          replaceSources(files, request) { assert.equal(request.expectedRevision, revision++); current = files; includes = request.includeSources; },
          updateSources(changes, request) {
            assert.equal(request.expectedRevision, revision++);
            const next = new Map(changes.map(entry => [entry.path, entry.source]));
            const apply = entries => entries.map(entry => ({ ...entry, source: next.get(entry.path) ?? entry.source }));
            current = apply(current); includes = apply(includes);
          },
          renderPdf: () => output("pdf"), renderEpub: () => output("epub"), renderSite: () => output("site"),
          dispose() {},
        };
      } };
      const retained = createRetainedBook(native, () => { throw Error("Legacy codec is not exercised"); }, async (session, selected) => {
        const result = await session.renderSite();
        const { files } = session.receipt();
        return { sourceLength: result.sourceLength, previewMode: "native-chapter",
          bytes: new TextEncoder().encode(JSON.stringify({ schema: "fmd-book-chapter-preview-v1", selected,
            pages: files.map((entry, index) => ({ path: `chapter-${index}.html`, source: entry.path, title: entry.path })),
            html: `<p>${files[selected].path}: ${files[selected].source}</p>` })) };
      });
      const endpoint = new EventTarget();
      endpoint.postMessage = (data, transfer) => {
        messages.push({ data: structuredClone(data), transfers: transfer.length });
        const owned = structuredClone(data, { transfer });
        queueMicrotask(() => { if (!terminated) void handler({ data: owned }); });
      };
      endpoint.terminate = () => { if (!terminated) { terminated = true; retained.clear(); } };
      Object.defineProperty(endpoint, "terminated", { get: () => terminated });
      installBookWorker({ addEventListener(_, listener) { handler = listener; }, postMessage(data, transfer) {
        if (terminated) return;
        const owned = structuredClone(data, { transfer });
        if (state.badAck) delete owned.sourceSetApplied;
        endpoint.dispatchEvent(new MessageEvent("message", { data: owned }));
      } }, { renderRetainedBook: retained.render, clearRetainedBook: retained.clear });
      endpoints.push(endpoint); return endpoint;
    } });
    const worker = legacy ? observed(client, { supportsSourceSet: undefined }) : client;
    return lanes[name] = { worker, endpoints, messages, state, get created() { return created; } };
  }
  const urls = new Map(), revoked = []; let serial = 0;
  const controls = createBookControls({ root, collection: host, worker: lane("export").worker, confirm: () => true,
    urls: { createObjectURL(blob) { const url = `blob:test-${++serial}`; urls.set(url, blob); return url; },
      revokeObjectURL(url) { revoked.push(url); urls.delete(url); } } });
  const timers = new Map(); let timerSerial = 0;
  const preview = createBookPreviewControls({ root, controls, collection: host, worker: lane("preview").worker,
    window: new Element(), crypto: { getRandomValues(bytes) { return bytes.fill(4); } },
    timers: { setTimeout(callback, delay) { const id = ++timerSerial; timers.set(id, { callback, delay }); return id; },
      clearTimeout(id) { timers.delete(id); } } });
  const proof = createBookPdfProof({ collection: host, controls, worker: lane("proof").worker });
  el("preview-auto").checked = automatic;
  t.after(() => { proof.dispose(); preview.dispose(); controls.dispose(); });
  const decode = text => JSON.parse(text.replace(/^%PDF-1.7\n/, ""));
  return { collection, controls, preview, proof, lanes, el, urls, revoked, get snapshots() { return snapshots; },
    async output() { return decode(await urls.get(el("download").href).text()); },
    async warm() { await controls.prepare("site"); await preview.build(); await proof.render(); },
    click(id) { return el(id).fire("click"); },
    input(id, value, event = "input") { el(id).value = value; el(id).fire(event); },
    replay(files) { controls.applySourceTree(files, controls.checkpoint()); },
    flushAuto() { for (const [id, timer] of [...timers]) if (timer.delay === 600) { timers.delete(id); timer.callback(); } },
    decode,
  };
}

test("editor reorder clears output but reuses all three independent workers and their assets", async t => {
  const f = rig(t); await f.warm(); const oldUrl = f.el("download").href;
  f.click("move-down");
  assert.equal(f.el("download").hidden, true); assert.ok(f.revoked.includes(oldUrl));
  assert.equal(f.proof.current, null); assert.match(f.el("preview-frame").srcdoc, /Build a preview/);
  for (const lane of Object.values(f.lanes)) assert.equal(lane.worker.hasRetainedBook, true);
  await f.controls.prepare("epub"); const preview = await f.preview.build(); const proof = await f.proof.render();
  assert.deepEqual((await f.output()).files.map(entry => entry.path), ["two.md", "one.md"]);
  assert.equal(preview.pages[preview.selected].source, "one.md"); assert.equal(preview.selected, 1);
  assert.equal(proof.chapters, 2); assert.equal(f.snapshots, 3);
  for (const lane of Object.values(f.lanes)) {
    assert.equal(lane.created, 1); assert.equal(lane.endpoints.length, 1);
    assert.equal(lane.messages[1].transfers, 0); assert.ok(lane.messages[1].data.sourceSet);
  }
  f.input("chapter-source", "Revised"); await f.controls.prepare("pdf");
  assert.deepEqual(f.lanes.export.messages[2].data.sourceUpdate.files, [file("one.md", "Revised")]);
});

for (const action of ["add-chapter", "add-include", "remove-chapter", "rename", "role", "replay"])
  test(`${action} reaches the source-set path through the actual editor controllers`, async t => {
    const f = rig(t); await f.warm(); const original = f.collection.files;
    if (action === "rename") f.input("chapter-path", "renamed.md");
    else if (action === "role") f.input("source-role", "include", "change");
    else if (action === "replay") f.replay([...original].reverse());
    else { f.click(action); await tick(); }
    await f.controls.prepare("site"); const view = await f.preview.build(); const proof = await f.proof.render();
    const expected = f.collection.project().files.filter(entry => entry.role !== "include");
    assert.deepEqual((await f.output()).files, expected);
    assert.deepEqual(view.pages.map(page => page.source), expected.map(entry => entry.path));
    assert.equal(proof.chapters, expected.length); assert.equal(f.snapshots, 3);
    for (const lane of Object.values(f.lanes)) {
      assert.equal(lane.created, 1); assert.equal(lane.messages.at(-1).transfers, 0);
      assert.ok(lane.messages.at(-1).data.sourceSet);
    }
    f.replay(original); await f.warm(); // Structural undo uses the same path.
    assert.equal(f.snapshots, 3);
  });

for (const action of ["images", "fonts", "title", "project"])
  test(`${action} change immediately retires every idle worker and all displayed output`, async t => {
    const f = rig(t); await f.warm();
    if (action === "images") f.collection.revokeImages();
    if (action === "fonts") f.collection.revokeFonts();
    if (action === "title") f.input("title", "New title");
    if (action === "project") f.controls.replaceProject(f.collection.project(), f.controls.checkpoint());
    for (const lane of Object.values(f.lanes)) assert.equal(lane.endpoints[0].terminated, true);
    assert.equal(f.proof.current, null); assert.equal(f.el("download").hidden, true);
    await f.warm(); assert.equal(f.snapshots, 6);
    for (const lane of Object.values(f.lanes)) assert.ok(lane.messages.at(-1).data.options);
  });

test("legacy source-set capability absence keeps full structural capture behavior", async t => {
  const f = rig(t, { legacy: true }); await f.warm(); f.click("move-down");
  for (const lane of Object.values(f.lanes)) assert.equal(lane.worker.hasRetainedBook, false);
  await f.warm(); assert.equal(f.snapshots, 6);
});

for (const name of ["export", "preview", "proof"])
  test(`${name}: a structural edit cancels active work; late completion cannot publish`, async t => {
    const f = rig(t); await f.warm(); const lane = f.lanes[name];
    f.input("chapter-source", "Pending source");
    let release; lane.state.hold = new Promise(resolve => { release = resolve; });
    const pending = name === "export" ? f.controls.prepare("pdf") : name === "preview" ? f.preview.build() : f.proof.render();
    const rejected = assert.rejects(pending, error => ["STALE_SOURCE", "EXPORT_CANCELLED"].includes(error.code));
    await tick(); f.click("move-down"); await rejected;
    assert.equal(lane.endpoints[0].terminated, true);
    release(); await tick();
    assert.equal(f.el("download").hidden, true); assert.equal(f.proof.current, null);
    assert.match(f.el("preview-frame").srcdoc, /Build a preview/);
    await f.warm(); assert.equal(lane.endpoints.length, 2);
  });

test("automatic preview re-renders reordered source without resending resources", async t => {
  const f = rig(t, { automatic: true }); await f.warm();
  f.click("move-down"); f.flushAuto(); await tick();
  assert.equal(f.lanes.preview.messages.length, 2);
  assert.ok(f.lanes.preview.messages[1].data.sourceSet);
  assert.equal(f.lanes.preview.messages[1].data.selectedChapter, 1);
  assert.equal(f.lanes.preview.messages[1].transfers, 0);
  assert.equal(f.el("preview-auto").checked, true);
});

test("empty-tree undo and redo never reuse old displayed content", async t => {
  const f = rig(t); await f.warm(); const original = f.collection.files;
  f.replay([]);
  assert.equal(f.el("preview-build").disabled, true); assert.equal(f.el("export-site").disabled, true);
  assert.equal(f.proof.current, null); assert.equal(f.el("download").hidden, true);
  f.replay(original); await f.warm(); assert.equal(f.snapshots, 3);
});

test("explicit cancel and suspension release retained resources rather than just hiding output", async t => {
  const f = rig(t); await f.warm();
  f.click("cancel-export"); f.click("preview-cancel"); f.proof.cancel();
  for (const lane of Object.values(f.lanes)) assert.equal(lane.worker.hasRetainedBook, false);
  await f.warm(); f.preview.suspend(); f.proof.suspend(); f.controls.suspend();
  for (const lane of Object.values(f.lanes)) assert.equal(lane.worker.hasRetainedBook, false);
  assert.deepEqual(f.collection.images, []); assert.deepEqual(f.collection.fonts, []);
});

for (const cause of ["badAck", "fail"])
  test(`failed structural preview (${cause}) clears output, stops auto-preview, and does not retry`, async t => {
    const f = rig(t); await f.warm(); f.click("move-down"); f.lanes.preview.state[cause] = true;
    f.el("preview-auto").checked = true;
    await assert.rejects(f.preview.build(), { code: cause === "badAck" ? "WORKER_PROTOCOL_ERROR" : "NATIVE_FAILURE" });
    assert.equal(f.el("preview-auto").checked, false);
    assert.equal(f.lanes.preview.worker.hasRetainedBook, false);
    assert.equal(f.lanes.preview.messages.length, 2);
    assert.match(f.el("preview-frame").srcdoc, /Build a preview/);
  });
