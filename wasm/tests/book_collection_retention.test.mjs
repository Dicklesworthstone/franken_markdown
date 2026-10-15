import assert from "node:assert/strict";
import test from "node:test";
import { createBookCollection, readPortableBookProject } from "../demo/book_collection.mjs";
import {
  bookCollectionResourceRevision, createBookCollectionCapture,
  createBookWorkerClient, installBookWorker, createRetainedBook,
} from "../book_worker.mjs";

// Real collection, admission, capture, transfer and retained-session coordinator.
// Only the native exporter is doubled: these are ownership/routing tests, not
// evidence about PDF/EPUB/HTML fidelity. Fixture font/image bytes are not fonts.
const file = (path, source = `# ${path}\n`) => ({ path, source });
const image = (value = 7) => ({ destination: "cover.png", bytes: new Uint8Array([value, 8]) });
const font = (value = 1) => ({ slot: "body-regular", name: "fixture.ttf", bytes: new Uint8Array([value, 2]) });
const readOutput = output => JSON.parse(new TextDecoder().decode(output.bytes));
const counters = collection => [collection.revision, collection.renderConfigurationRevision, collection.renderResourceRevision];
const observed = (object, overrides) => Object.create(object, Object.fromEntries(
  Object.entries(overrides).map(([key, value]) => [key, { value, configurable: true }]),
));

function fixture(t, { legacyWorker = false, legacyCollection = false } = {}) {
  const collection = createBookCollection();
  collection.append({ chapters: [file("one.md"), file("two.md")],
    includeSources: [file("shared.md", "Shared\r\n")], images: [image()] });
  collection.setFonts([font()], collection.revision);
  const calls = [], messages = [], workers = [];
  const stats = { snapshots: 0, projects: 0, created: 0, disposed: 0 };
  const state = { fail: null, acknowledge: true, gate: null };
  const host = observed(collection, {
    snapshot() { stats.snapshots++; return collection.snapshot(); },
    project() { stats.projects++; return collection.project(); },
    ...(legacyCollection ? { renderResourceRevision: undefined } : {}),
  });
  function workerFactory() {
    let handler, terminated = false;
    const engine = { async createBook(files, options) {
      stats.created++;
      let current = files, includes = options.includeSources, revision = 0;
      const render = async () => {
        if (state.gate) { const gate = state.gate; state.gate = null; await gate; }
        const bytes = new TextEncoder().encode(JSON.stringify({ files: current, includes,
          title: options.title, page: options.page,
          images: options.images.map(asset => Array.from(asset.bytes)),
          fonts: options.fontAssets.map(asset => Array.from(asset.bytes)),
        }));
        return { bytes, sourceLength: [...current, ...includes]
          .reduce((sum, entry) => sum + new TextEncoder().encode(entry.source).length, 0) };
      };
      return {
        get sourceRevision() { return revision; },
        updateSources(changes, { expectedRevision }) {
          assert.equal(expectedRevision, revision);
          calls.push({ kind: "update", changes: structuredClone(changes) });
          const values = new Map(changes.map(entry => [entry.path, entry.source]));
          const patch = entries => entries.map(entry => ({ ...entry,
            source: values.has(entry.path) ? values.get(entry.path) : entry.source }));
          current = patch(current); includes = patch(includes); revision++;
        },
        replaceSources(files, { includeSources, expectedRevision }) {
          assert.equal(expectedRevision, revision);
          calls.push({ kind: "replace", files: structuredClone(files), includes: structuredClone(includeSources) });
          if (state.fail) throw Object.assign(new Error("native replacement failed"), { code: state.fail });
          current = files; includes = includeSources; revision++;
        },
        renderPdf: render, renderEpub: render, renderSite: render,
        dispose() { stats.disposed++; },
      };
    } };
    const retained = createRetainedBook(engine, () => { throw new Error("No legacy preview codec in this fixture"); });
    const endpoint = new EventTarget();
    endpoint.postMessage = (data, transfer) => {
      messages.push({ data: structuredClone(data), transfers: transfer.length });
      const owned = structuredClone(data, { transfer });
      queueMicrotask(() => { if (!terminated) void handler({ data: owned }); });
    };
    endpoint.terminate = () => { if (!terminated) { terminated = true; retained.clear(); } };
    Object.defineProperty(endpoint, "terminated", { get: () => terminated });
    installBookWorker({
      addEventListener(kind, listener) { assert.equal(kind, "message"); handler = listener; },
      postMessage(data, transfer) {
        if (terminated) return;
        const output = structuredClone(data, { transfer });
        if (!state.acknowledge) delete output.sourceSetApplied;
        endpoint.dispatchEvent(new MessageEvent("message", { data: output }));
      },
    }, { renderRetainedBook: retained.render, clearRetainedBook: retained.clear });
    workers.push(endpoint);
    return endpoint;
  }
  const client = createBookWorkerClient({ workerFactory, retainBook: true });
  const worker = legacyWorker ? observed(client, { supportsSourceSet: undefined }) : client;
  const capture = createBookCollectionCapture(host, worker);
  t.after(() => { client.dispose(); collection.dispose(); });
  return { collection, host, client, worker, capture, calls, messages, stats, state, workers,
    async run(format = "site") { return readOutput(await capture().render(format)); } };
}

test("source text and every structural edit preserve the resource fence", () => {
  const c = createBookCollection();
  assert.deepEqual(counters(c), [0, 0, 0]);
  c.append({ chapters: [file("one.md"), file("two.md")], images: [] });
  assert.deepEqual(counters(c), [1, 1, 0]);
  c.edit(0, "one.md", "Changed");
  assert.deepEqual(counters(c), [2, 1, 0]);
  c.edit(0, "renamed.md", "Changed");
  c.move(0, 1);
  c.setRole(0, "include");
  c.remove(0);
  c.replaceSources([file("renamed.md", "New")], c.revision);
  c.replaceSourceTree([file("added.md"), file("renamed.md", "New")], c.revision);
  assert.deepEqual(counters(c), [8, 6, 0]);
  assert.equal(c.sourceProjectRevision, 0);
  c.dispose();
});

test("resource changes advance before observers, and failed/no-op operations do not", () => {
  const c = createBookCollection(), seen = [];
  c.subscribe(() => seen.push(counters(c)));
  c.append({ chapters: [file("one.md")], images: [image()] });
  c.setFonts([font()], c.revision);
  c.setPage({ size: "a4" }, c.revision);
  c.setPublication({ baseFontSize: 12 }, c.revision);
  c.configure({ title: "Another title" });
  c.changeImages([{ destination: "cover.png", bytes: image(9).bytes }], c.revision);
  c.revokeFont("body-regular");
  c.revokeImages();
  assert.deepEqual(seen.map(row => row[2]), [1, 2, 3, 4, 5, 6, 7, 8]);
  const before = counters(c);
  c.move(0, -1); c.setRole(0, "chapter"); c.revokeFonts();
  c.replaceSourceTree(c.files, c.revision);
  c.setPage(undefined, c.revision); c.setPublication({}, c.revision);
  assert.throws(() => c.setFonts([font()], c.revision - 1), { code: "STALE_SOURCE" });
  assert.throws(() => c.append({ chapters: [file("one.md")], images: [] }), { code: "DUPLICATE_PATH" });
  assert.deepEqual(counters(c), before);
  c.dispose();
});

test("reordering uses native source-set replacement, then ordinary edits send only a delta", async t => {
  const f = fixture(t);
  const initial = await f.run("pdf");
  f.collection.move(0, 1);
  const reordered = await f.run("epub");
  assert.deepEqual(reordered.files.map(entry => entry.path), ["two.md", "one.md"]);
  assert.deepEqual(reordered.images, initial.images);
  assert.deepEqual(reordered.fonts, initial.fonts);
  assert.equal(f.stats.created, 1);
  assert.equal(f.stats.snapshots, 1);
  assert.equal(f.messages[1].transfers, 0);
  assert.ok(f.messages[1].data.sourceSet);
  assert.equal(f.messages[1].data.options, undefined);
  f.collection.edit(0, "two.md", "Revised chapter");
  const edited = await f.run();
  assert.equal(edited.files[0].source, "Revised chapter");
  assert.deepEqual(f.messages[2].data.sourceUpdate.files, [file("two.md", "Revised chapter")]);
  assert.deepEqual(f.calls.map(call => call.kind), ["replace", "update"]);
});

test("additions, removal, renaming and chapter/include role changes share one native book", async t => {
  const f = fixture(t);
  await f.run();
  f.collection.append({ chapters: [file("three.md")], images: [] });
  await f.run();
  f.collection.setRole(1, "include");
  let output = await f.run();
  assert.deepEqual(output.files.map(entry => entry.path), ["one.md", "three.md"]);
  assert.deepEqual(output.includes.map(entry => entry.path), ["two.md", "shared.md"]);
  f.collection.remove(2); // Revoke the include source, not any image/font grant.
  f.collection.edit(0, "renamed.md", "Renamed chapter");
  output = await f.run();
  assert.deepEqual(output.files.map(entry => entry.path), ["renamed.md", "three.md"]);
  assert.deepEqual(output.includes.map(entry => entry.path), ["two.md"]);
  f.collection.remove(1);
  assert.deepEqual((await f.run()).includes, []);
  assert.equal(f.stats.created, 1);
  assert.equal(f.stats.snapshots, 1);
  assert.ok(f.messages.slice(1).every(message => message.transfers === 0 && message.data.sourceSet));
});

test("source-tree undo/redo preserves resources and exact imported line endings", async t => {
  const f = fixture(t), original = f.collection.files;
  await f.run();
  f.collection.replaceSourceTree([...original].reverse(), f.collection.revision);
  await f.run();
  f.collection.replaceSourceTree(original, f.collection.revision);
  assert.equal((await f.run()).includes[0].source, "Shared\r\n");
  assert.equal(f.stats.snapshots, 1);
  assert.equal(f.stats.created, 1);
});

for (const [name, mutate] of [
  ["settings", c => c.configure({ title: "New edition" })],
  ["page geometry", c => c.setPage({ size: "a4" }, c.revision)],
  ["publishing profile", c => c.setPublication({ codeLineNumbers: true }, c.revision)],
  ["same-size image replacement", c => c.changeImages([{ destination: "cover.png", bytes: image(9).bytes }], c.revision)],
  ["font replacement", c => c.setFonts([font(5)], c.revision)],
  ["font revocation", c => c.revokeFonts()],
  ["image revocation", c => c.revokeImages()],
  ["mixed source/image import", c => c.append({ chapters: [file("three.md")], images: [{ destination: "new.png", bytes: new Uint8Array([5]) }] })],
]) test(`${name} never takes the source-only path after a structural edit`, async t => {
  const f = fixture(t);
  await f.run(); f.collection.move(0, 1); mutate(f.collection);
  const expected = f.collection.snapshot();
  const output = await f.run();
  assert.equal(f.stats.snapshots, 2);
  assert.ok(f.messages[1].data.files);
  assert.equal(f.messages[1].data.sourceSet, undefined);
  assert.deepEqual(output.images, expected.options.images.map(asset => Array.from(asset.bytes)));
  assert.deepEqual(output.fonts, expected.options.fontAssets.map(asset => Array.from(asset.bytes)));
  assert.equal(output.title, expected.options.title);
});

test("source-project replacement is a resource identity boundary even for identical text", async t => {
  const f = fixture(t);
  await f.run();
  const before = f.collection.renderResourceRevision;
  f.collection.replaceProject(f.collection.project());
  assert.ok(f.collection.renderResourceRevision > before);
  const output = await f.run();
  assert.deepEqual(output.images, []); assert.deepEqual(output.fonts, []);
  assert.equal(f.stats.snapshots, 2);
});

test("authorized portable restore advances resource identity and cannot reuse an older capture", async t => {
  const f = fixture(t);
  await f.run();
  const download = await f.collection.portableDownload();
  const review = await readPortableBookProject(new File([download.blob], download.filename));
  const before = f.collection.renderResourceRevision;
  f.collection.replacePortableProject(review, f.collection.revision, { authorizeResources: true });
  assert.ok(f.collection.renderResourceRevision > before);
  await f.run(); assert.equal(f.stats.snapshots, 2);
});

for (const kind of ["legacyWorker", "legacyCollection"]) test(`${kind} keeps legacy structural/full-snapshot behavior`, async t => {
  const f = fixture(t, { [kind]: true });
  await f.run();
  f.collection.move(0, 1); await f.run();
  assert.equal(f.stats.snapshots, 2);
  assert.ok(f.messages[1].data.files);
  f.collection.edit(0, "two.md", "text delta"); await f.run();
  assert.ok(f.messages[2].data.sourceUpdate);
  assert.equal(f.stats.snapshots, 2);
});

test("resource/configuration changes invalidate a prepared one-use source-set task", async t => {
  const f = fixture(t);
  await f.run(); f.collection.move(0, 1);
  const task = f.capture();
  f.collection.revokeImages();
  await assert.rejects(task.render("pdf"), { code: "STALE_SOURCE" });
  await assert.rejects(task.render("pdf"), { code: "BOOK_CAPTURE_USED" });
  assert.equal(f.messages.length, 1);
  assert.deepEqual((await f.run()).images, []);
});

test("expired or externally replaced worker baselines force a new complete capture", async t => {
  const f = fixture(t);
  await f.run(); f.collection.move(0, 1); f.worker.cancel();
  await f.run(); assert.equal(f.stats.snapshots, 2);
  const other = f.collection.snapshot();
  await f.worker.render(other.files, "pdf", other.options);
  f.collection.move(0, 1); await f.run();
  assert.equal(f.stats.snapshots, 3);
  assert.ok(f.messages.at(-1).data.files);
});

test("native source-set failure propagates without implicit replay and retires its worker", async t => {
  const f = fixture(t);
  await f.run(); f.collection.move(0, 1);
  f.state.fail = "INVALID_INCLUDE";
  await assert.rejects(f.run(), { code: "INVALID_INCLUDE" });
  assert.equal(f.stats.created, 1);
  assert.equal(f.messages.length, 2);
  assert.equal(f.worker.retainedInputRevision, null);
  assert.equal(f.workers[0].terminated, true);
  f.state.fail = null;
  await f.run(); assert.equal(f.stats.snapshots, 2);
});

test("a missing source-set acknowledgement cannot become a successful publication", async t => {
  const f = fixture(t);
  await f.run(); f.collection.move(0, 1); f.state.acknowledge = false;
  await assert.rejects(f.run(), { code: "WORKER_PROTOCOL_ERROR" });
  assert.equal(f.worker.retainedInputRevision, null);
  assert.equal(f.stats.snapshots, 1);
});

test("invalid draft source trees remain editable but never become publications", async t => {
  const f = fixture(t);
  await f.run();
  f.collection.replaceSourceTree([file("one.md"), file("one.md")], f.collection.revision);
  assert.throws(() => f.capture(), { code: "DUPLICATE_PATH" });
  f.collection.replaceSourceTree([], f.collection.revision);
  assert.throws(() => f.capture(), /at least one chapter/);
  assert.equal(f.messages.length, 1);
  assert.equal(f.stats.snapshots, 1);
});

test("observer reentry cannot disguise an asset change as a structural edit", async t => {
  const f = fixture(t);
  await f.run();
  let entered = false;
  const unsubscribe = f.collection.subscribe(() => {
    if (!entered) { entered = true; f.collection.setFonts([font(9)], f.collection.revision); }
  });
  f.collection.move(0, 1); unsubscribe();
  assert.deepEqual((await f.run()).fonts, [[9, 2]]);
  assert.equal(f.stats.snapshots, 2);
});

test("the shared controller fence is capability-aware and fails closed for invalid identities", () => {
  const capable = { supportsSourceSet: true, renderSourceSet() {} };
  assert.equal(bookCollectionResourceRevision({ renderConfigurationRevision: 8 }, capable), 8);
  assert.equal(bookCollectionResourceRevision({ renderConfigurationRevision: 8, renderResourceRevision: 2 }, {}), 8);
  assert.equal(bookCollectionResourceRevision({ renderConfigurationRevision: 8, renderResourceRevision: 2 }, capable), 2);
  for (const value of [null, -1, NaN, Infinity, "2", Number.MAX_SAFE_INTEGER + 1])
    assert.ok(Number.isNaN(bookCollectionResourceRevision({ renderConfigurationRevision: 8, renderResourceRevision: value }, capable)));
});
