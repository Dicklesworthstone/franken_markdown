import test from "node:test";
import assert from "node:assert/strict";
import { applyBookSourceSet, createBookWorkerClient, installBookWorker } from "./book_worker.mjs";
import { createRetainedBook } from "./book_retained.mjs";
import { prepareBookInput } from "./book_session.mjs";

const chapter = (path, source = `# ${path}\n`) => ({ path, source });
const oldFiles = [chapter("a.md"), chapter("b.md")];
const code = expected => error => error?.code === expected;
const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));

// Both sides of the real worker protocol and the retained-session synchronizer
// run here. Only native parsing/rendering and the browser endpoint are doubles.
function harness(t, settings = {}) {
  const calls = { created: 0, replaced: 0, updated: 0, disposed: 0, terminated: 0,
    requests: [], inputs: [] };
  const native = {
    async createBook(files, options) {
      calls.created++;
      let current = structuredClone(files), includes = structuredClone(options.includeSources);
      let revision = 0, disposed = false;
      const render = () => {
        assert.equal(disposed, false);
        return { bytes: new TextEncoder().encode(JSON.stringify({ files: current, includes,
          title: options.title, images: options.images.length, fonts: options.fontAssets.length })),
          sourceLength: [...current, ...includes].reduce((n, file) => n + new TextEncoder().encode(file.source).length, 0) };
      };
      return {
        get sourceRevision() { return revision; },
        replaceSources(next, { includeSources, expectedRevision }) {
          assert.equal(expectedRevision, revision);
          if (settings.replaceError) throw Object.assign(new Error("native replacement failed"), { code: settings.replaceError });
          calls.replaced++;
          current = structuredClone(next); includes = structuredClone(includeSources); revision++;
        },
        updateSources(changes, { expectedRevision }) {
          assert.equal(expectedRevision, revision);
          calls.updated++;
          const values = new Map(changes.map(file => [file.path, file.source]));
          for (const file of [...current, ...includes]) if (values.has(file.path)) file.source = values.get(file.path);
          revision++;
        },
        renderPdf: render, renderEpub: render, renderSite: render,
        dispose() { if (!disposed) { calls.disposed++; disposed = true; } },
      };
    },
  };
  const retained = createRetainedBook(native, engine => engine.renderBookSite());
  const engine = {
    async renderRetainedBook(files, options, format) {
      calls.inputs.push({ files, options });
      return retained.render(files, options, format);
    },
    clearRetainedBook: retained.clear,
  };
  let receive;
  const listeners = new Map();
  const scope = {
    addEventListener(kind, fn) { assert.equal(kind, "message"); receive = fn; },
    postMessage(data, transfer = []) {
      const copy = structuredClone(data, { transfer });
      settings.reply?.(copy);
      queueMicrotask(() => { for (const fn of [...(listeners.get("message") ?? [])]) fn({ data: copy }); });
    },
  };
  installBookWorker(scope, engine);
  const endpoint = {
    addEventListener(kind, fn) { if (!listeners.has(kind)) listeners.set(kind, new Set()); listeners.get(kind).add(fn); },
    removeEventListener(kind, fn) { listeners.get(kind)?.delete(fn); },
    postMessage(data, transfer = []) {
      calls.requests.push({ data: structuredClone(data), transferCount: transfer.length });
      const copy = structuredClone(data, { transfer });
      settings.sent?.(data);
      void receive({ data: copy });
    },
    terminate() { calls.terminated++; retained.clear(); },
  };
  const client = createBookWorkerClient({ workerFactory: () => endpoint, retainBook: true,
    timeoutMs: 2000, ...settings.client });
  t.after(() => client.dispose());
  return { client, calls, receive };
}

async function seed(h, options = {}) {
  return h.client.render(oldFiles, "pdf", { title: "Captured edition", ...options });
}

test("replace chapter membership/order across formats without rebuilding or retransferring assets", async t => {
  const h = harness(t), image = new Uint8Array([1, 2, 3]), font = new Uint8Array([4, 5]);
  await seed(h, { images: [{ destination: "cover.png", bytes: image }],
    fontAssets: [{ slot: "body-regular", bytes: font }] });
  const next = [chapter("b.md"), chapter("c.md", "# Café 🌍\n"), chapter("a.md")];
  const result = await h.client.renderSourceSet(next, "epub", { expectedRevision: h.client.retainedInputRevision });
  assert.deepEqual(decode(result).files, next);
  assert.equal(decode(result).title, "Captured edition");
  assert.equal(decode(result).images, 1);
  assert.equal(decode(result).fonts, 1);
  assert.equal(h.calls.created, 1);
  assert.equal(h.calls.replaced, 1);
  assert.equal(h.calls.requests.at(-1).transferCount, 0);
  assert.deepEqual(Object.keys(h.calls.requests.at(-1).data.sourceSet).sort(), ["expectedRevision", "files", "includeSources"]);
  assert.strictEqual(h.calls.inputs[0].options.images, h.calls.inputs[1].options.images);
  assert.strictEqual(h.calls.inputs[0].options.fontAssets, h.calls.inputs[1].options.fontAssets);
  assert.deepEqual([...image], [1, 2, 3]);
  assert.deepEqual([...font], [4, 5]);
  const reduced = await h.client.renderSourceSet([next[1]], "site", { expectedRevision: result.retainedInputRevision });
  assert.deepEqual(decode(reduced).files, [next[1]]);
  assert.equal(h.calls.replaced, 2);
  assert.equal(h.calls.created, 1);
});

test("replacement can promote/demote sources and omission revokes includes", async t => {
  const h = harness(t);
  await seed(h, { includeSources: [chapter("shared.md")] });
  const next = [chapter("shared.md")], includes = [chapter("a.md")];
  const moved = await h.client.renderSourceSet(next, "pdf", { expectedRevision: h.client.retainedInputRevision, includeSources: includes });
  assert.deepEqual(decode(moved).includes, includes);
  const removed = await h.client.renderSourceSet(next, "epub", { expectedRevision: moved.retainedInputRevision });
  assert.deepEqual(decode(removed).includes, []);
  assert.equal(h.calls.created, 1);
  assert.equal(h.calls.replaced, 2);
});

test("future source diffs use the replaced baseline and new keys", async t => {
  const h = harness(t);
  await seed(h);
  await h.client.renderSourceSet([chapter("new.md")], "pdf", { expectedRevision: h.client.retainedInputRevision });
  const changed = [chapter("new.md", "# Edited\n")];
  const result = await h.client.renderSources(changed, "site", { expectedRevision: h.client.retainedInputRevision });
  assert.deepEqual(decode(result).files, changed);
  assert.deepEqual(h.calls.requests.at(-1).data.sourceUpdate.files, changed);
  assert.equal(h.calls.updated, 1);
  assert.equal(h.calls.replaced, 1);
});

test("stale or invalid local admission preserves the newer idle capture", async t => {
  const h = harness(t);
  await seed(h);
  const revision = h.client.retainedInputRevision;
  await assert.rejects(h.client.renderSourceSet([chapter("new.md")], "pdf", { expectedRevision: revision + 1 }), code("STALE_BOOK_CAPTURE"));
  await assert.rejects(h.client.renderSourceSet([], "pdf", { expectedRevision: revision }));
  await assert.rejects(h.client.renderSourceSet([chapter("same.md")], "pdf", {
    expectedRevision: revision, includeSources: [chapter("same.md")],
  }), code("DUPLICATE_BOOK_SOURCE"));
  await assert.rejects(h.client.renderSourceSet([chapter("new.md", "\ud800")], "pdf", { expectedRevision: revision }));
  let read = false;
  const options = { expectedRevision: revision, get images() { read = true; throw new Error("must not read assets"); } };
  await assert.rejects(h.client.renderSourceSet(oldFiles, "pdf", options), code("INVALID_BOOK_SOURCE_DELTA"));
  assert.equal(read, false);
  assert.equal(h.client.retainedInputRevision, revision);
  assert.equal(h.calls.requests.length, 1);
  assert.equal(h.calls.terminated, 0);
  assert.equal(h.client.busy, false);
});

test("selected chapter is checked against replacement, not previous book", async t => {
  const h = harness(t);
  await seed(h);
  const revision = h.client.retainedInputRevision;
  await assert.rejects(h.client.renderSourceSet([chapter("only.md")], "chapter-preview", {
    expectedRevision: revision,
  }, { selectedChapter: 1 }), code("INVALID_CHAPTER_INDEX"));
  assert.equal(h.client.retainedInputRevision, revision);
  assert.equal(h.calls.requests.length, 1);
});

test("wire replacement rejects mixed mutation/full snapshot envelopes", async t => {
  for (const extra of [{ files: oldFiles }, { options: {} }, { sourceUpdate: { expectedRevision: 1, files: [] } }]) {
    const h = harness(t);
    await seed(h);
    await h.receive({ data: { schemaVersion: 1, id: 2, format: "pdf", retainBook: true,
      maxOutputBytes: 1024, sourceSet: { expectedRevision: 1, files: [chapter("new.md")], includeSources: [] }, ...extra } });
    assert.equal(h.calls.inputs.length, 1);
    assert.equal(h.calls.replaced, 0);
  }
});

test("replacement preserves retained include-expansion policy", async t => {
  const h = harness(t);
  await seed(h, { expandIncludes: false });
  await assert.rejects(h.client.renderSourceSet([chapter("new.md")], "pdf", {
    expectedRevision: h.client.retainedInputRevision, includeSources: [chapter("include.md")],
  }));
  assert.equal(h.calls.inputs.length, 1);
  assert.equal(h.client.retainedInputRevision, null);
});

test("missing explicit replacement acknowledgment fails closed", async t => {
  const h = harness(t, { reply(data) { delete data.sourceSetApplied; } });
  await seed(h);
  await assert.rejects(h.client.renderSourceSet([chapter("new.md")], "pdf", {
    expectedRevision: h.client.retainedInputRevision,
  }), code("WORKER_PROTOCOL_ERROR"));
  assert.equal(h.client.retainedInputRevision, null);
  assert.equal(h.calls.terminated, 1);
});

test("native transaction failure never silently rebuilds or publishes", async t => {
  const h = harness(t, { replaceError: "BROKEN_INCLUDE" });
  await seed(h);
  await assert.rejects(h.client.renderSourceSet([chapter("new.md")], "pdf", {
    expectedRevision: h.client.retainedInputRevision,
  }), code("BROKEN_INCLUDE"));
  assert.equal(h.calls.created, 1);
  assert.equal(h.calls.terminated, 1);
  assert.equal(h.client.retainedInputRevision, null);
});

test("legacy native capability failure reconstructs the exact new collection", async t => {
  const h = harness(t, { replaceError: "UNSUPPORTED_BOOK_SOURCE_SET" });
  await seed(h);
  const next = [chapter("new.md")];
  const result = await h.client.renderSourceSet(next, "pdf", { expectedRevision: h.client.retainedInputRevision });
  assert.deepEqual(decode(result).files, next);
  assert.equal(h.calls.created, 2);
  assert.equal(h.calls.disposed, 1);
});

test("expired and nonretained clients never start a replacement worker", async t => {
  const h = harness(t);
  await seed(h);
  const expectedRevision = h.client.retainedInputRevision;
  h.client.cancel();
  await assert.rejects(h.client.renderSourceSet(oldFiles, "pdf", { expectedRevision }), code("BOOK_CAPTURE_EXPIRED"));
  assert.equal(h.calls.requests.length, 1);
  const fresh = harness(t, { client: { retainBook: false } });
  await assert.rejects(fresh.client.renderSourceSet(oldFiles, "pdf", { expectedRevision: 1 }), code("UNSUPPORTED_BOOK_SOURCE_DELTA"));
  assert.equal(fresh.calls.requests.length, 0);
});

test("replacement containers cannot mutate the idle source baseline through an embedding", async t => {
  const h = harness(t, { sent(data) { if (data.sourceSet) data.sourceSet.files[0].source = "tampered"; } });
  await seed(h);
  const next = [chapter("new.md")];
  await h.client.renderSourceSet(next, "pdf", { expectedRevision: h.client.retainedInputRevision });
  await h.client.renderSources(next, "epub", { expectedRevision: h.client.retainedInputRevision });
  assert.deepEqual(h.calls.requests.at(-1).data.sourceUpdate.files, []);
  assert.equal(h.calls.updated, 0);
});

test("pure reconstruction retains assets and does not mutate the previous capture", () => {
  const input = prepareBookInput(oldFiles, { includeSources: [chapter("old-include.md")], title: "Title",
    images: [{ destination: "cover.png", bytes: new Uint8Array([1]) }] });
  const before = structuredClone(input);
  const next = applyBookSourceSet(input, { expectedRevision: 1,
    files: [chapter("new.md")], includeSources: [] });
  assert.deepEqual(input, before);
  assert.strictEqual(next.options.images, input.options.images);
  assert.strictEqual(next.options.fontAssets, input.options.fontAssets);
  assert.equal(next.options.title, "Title");
  assert.deepEqual(next.options.includeSources, []);
  assert.throws(() => applyBookSourceSet(input, { expectedRevision: 1, files: oldFiles,
    includeSources: [], images: [] }), code("INVALID_BOOK_SOURCE_DELTA"));
});

test("replacement checks the complete chapter-plus-include source budget", () => {
  const input = prepareBookInput(oldFiles);
  const large = "x".repeat(33 * 1024 * 1024);
  assert.throws(() => applyBookSourceSet(input, { expectedRevision: 1,
    files: [chapter("new.md", large)], includeSources: [chapter("include.md", large)] }));
  assert.deepEqual(input.files, oldFiles);
});
