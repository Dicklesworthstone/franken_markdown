// Production adapters with an explicit native ABI double. JSON describes calls;
// real rendering is checked independently by the native and generated-WASM tests.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { bookAuthoringProfileReceipt, prepareBookInput } from "../book_session.mjs";
import { createCachedBookRenderer } from "../book_retained.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createBookPublicationClient } from "../book_publication.mjs";
import { renderBookSelectedPreview } from "../book_site_preview.mjs";
import { nativeFixture } from "./book_worker_delta_fixture.mjs";

const files = [{ path: "one.md", source: "# One\n\nTerm\n: Definition" }];
const decode = result => JSON.parse(new TextDecoder().decode(result.bytes));
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const unsupported = { code: "UNSUPPORTED_BOOK_PROFILE" };
function transport(t, alter = value => value, native = {}) {
  const endpoints = [];
  const factory = () => {
    const thread = new Worker(new URL("./book_worker_delta_fixture.mjs", import.meta.url), {
      workerData: { counters: new SharedArrayBuffer(24), ...native },
    });
    const endpoint = new EventTarget(); endpoint.messages = [];
    thread.on("message", value => endpoint.dispatchEvent(new MessageEvent("message", { data: alter(value) })));
    for (const kind of ["error", "messageerror"])
      thread.on(kind, () => endpoint.dispatchEvent(new Event(kind)));
    endpoint.postMessage = (value, transfer) => {
      endpoint.messages.push(structuredClone(value)); thread.postMessage(value, transfer);
    };
    endpoint.terminate = () => thread.terminate(); endpoints.push(endpoint); return endpoint;
  };
  const make = () => createBookWorkerClient({ retainBook: true, workerFactory: factory, timeoutMs: 10000 });
  const client = make(); t.after(() => client.dispose());
  return { client, make, endpoints };
}

test("book authoring profiles are canonical, captured once and owned before initialization", async () => {
  let assetReads = 0;
  for (const profile of [null, "", "gfm", "GFM-PLUS", " commonmark-gfm ", true, 1, {}]) {
    assert.throws(() => prepareBookInput(files, { profile,
      get images() { assetReads++; throw Error("profile must be admitted first"); } }), { code: "INVALID_OPTIONS" });
  }
  assert.equal(assetReads, 0);
  const f = nativeFixture(); let reads = 0;
  const image = new Uint8Array([1, 2, 3]);
  const options = { get profile() { reads++; return reads === 1 ? "commonmark-gfm" : "gfm-plus"; },
    images: [{ destination: "one.png", bytes: image }] };
  const pending = f.engine.createBook(files, options);
  image.fill(9);
  const session = await pending;
  try {
    for (const output of [session.renderPdf(), session.renderEpub(), session.renderSite()]) {
      assert.equal(output.profile, "commonmark-gfm");
      assert.equal(decode(output).settings.profile, "commonmark-gfm");
      assert.equal(decode(output).images[0].sha256,
        "039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81");
    }
    assert.equal(reads, 1); assert.equal(f.stats.creates, 1);
    assert.equal(session.renderPdf({ page: { size: "letter" } }).profile, "commonmark-gfm");
  } finally { session.dispose(); }
});

test("explicit profiles refuse old native books before construction while defaults remain compatible", async () => {
  const f = nativeFixture({ oldProfile: true });
  for (const profile of ["commonmark-gfm", "gfm-plus"])
    await assert.rejects(f.engine.createBook(files, { profile }), unsupported);
  assert.equal(f.stats.creates, 0);
  const output = await f.engine.renderBookSite(files);
  assert(!Object.hasOwn(output, "profile")); assert.equal(decode(output).settings.profile, undefined);
  assert.equal(f.stats.creates, 1); assert.equal(f.stats.frees, 1);
});

test("source dialect is selected before either native constructor parses or expands", async () => {
  for (const expandIncludes of [false, true]) {
    const f = nativeFixture();
    const session = await f.engine.createBook(files, { profile: "commonmark-gfm", expandIncludes });
    try {
      assert.deepEqual(f.constructions, [{ profile: "commonmark-gfm", expanded: expandIncludes }]);
      assert.equal(decode(session.renderSite()).settings.profile, "commonmark-gfm");
      assert.equal(f.stats.creates, 1);
    } finally { session.dispose(); }
  }
});

test("legacy profile setters cannot substitute for source-aware construction", async () => {
  for (const expandIncludes of [false, true]) {
    const f = nativeFixture({ oldProfileConstruction: true });
    await assert.rejects(f.engine.createBook(files, { profile: "commonmark-gfm", expandIncludes }), unsupported);
    assert.equal(f.stats.creates, 0);
    const ordinary = await f.engine.createBook(files, { expandIncludes });
    ordinary.dispose();
    assert.equal(f.stats.creates, 1);
  }
});

test("retained native books and output caches cannot reuse a different authoring dialect", async () => {
  const f = nativeFixture();
  const cached = createCachedBookRenderer(f.retained, api => api.renderBookSite());
  const render = async (source, profile, format = "site") => {
    const input = prepareBookInput([{ ...files[0], source }], { profile });
    return cached.render(input.files, input.options, format);
  };
  try {
    const first = await render(files[0].source, "commonmark-gfm");
    const hit = await render(files[0].source, "commonmark-gfm");
    assert.deepEqual(hit, first); assert.notEqual(hit.bytes, first.bytes);
    assert.equal(cached.cacheStats.hits, 1); assert.equal(f.stats.exports, 1);
    const edited = await render("Changed term\n: Changed definition", "commonmark-gfm", "pdf");
    assert.equal(edited.profile, "commonmark-gfm"); assert.equal(f.stats.updates, 1);
    assert.equal(f.stats.creates, 1);
    const changed = await render("Changed term\n: Changed definition", "gfm-plus", "epub");
    assert.equal(changed.profile, "gfm-plus"); assert.equal(decode(changed).settings.profile, "gfm-plus");
    assert.equal(f.stats.creates, 2);
    const ordinary = await render("Changed term\n: Changed definition", undefined);
    assert(!Object.hasOwn(ordinary, "profile")); assert.equal(f.stats.creates, 3);
  } finally { cached.clear(); }
});

test("real worker source deltas preserve the dialect and resources across every rendered format", async t => {
  const f = transport(t), bytes = new Uint8Array([4, 5, 6]);
  const options = { profile: "commonmark-gfm", images: [{ destination: "one.png", bytes }] };
  const pending = f.client.render(files, "pdf", options);
  options.profile = "gfm-plus"; bytes.fill(9);
  const first = await pending;
  assert.equal(first.profile, "commonmark-gfm");
  const images = decode(first).images;
  for (const format of ["epub", "site", "preview", "pdf"]) {
    const result = await f.client.renderSourceUpdate([{ path: "one.md", source: `${format}\n: Definition` }],
      format, { expectedRevision: f.client.retainedInputRevision });
    assert.equal(result.profile, "commonmark-gfm");
    assert.equal(decode(result).settings.profile, "commonmark-gfm");
    assert.deepEqual(decode(result).images, images);
  }
  assert.equal(f.endpoints.length, 1);
  assert(f.endpoints[0].messages.slice(1).every(message => !Object.hasOwn(message, "options")));
});

for (const field of ["bookAuthoringProfile", "profile"]) {
  for (const delta of [false, true]) test(`worker ${field} acknowledgements cannot disappear on ${delta ? "source deltas" : "initial exports"}`, async t => {
    const f = transport(t, value => {
      if (!delta || value.id > 1) delete value[field];
      return value;
    });
    if (delta) {
      await f.client.render(files, "site", { profile: "gfm-plus" });
      await assert.rejects(f.client.renderSourceUpdate([], "epub", { expectedRevision: f.client.retainedInputRevision }), unsupported);
    } else await assert.rejects(f.client.render(files, "site", { profile: "gfm-plus" }), unsupported);
    assert.equal(f.client.retainedInputRevision, null); assert.equal(f.client.busy, false);
  });
}

test("old native profile capability errors survive real worker transport", async t => {
  const f = transport(t, value => value, { oldProfile: true });
  await assert.rejects(f.client.render(files, "pdf", { profile: "commonmark-gfm" }), unsupported);
  assert(!Object.hasOwn(await f.client.render(files, "site"), "profile"));
});

test("source-only retained checks acknowledge private capture without claiming a rendered dialect", async () => {
  let listener; const replies = [];
  installBookWorker({ addEventListener(_, handle) { listener = handle; }, postMessage(value) { replies.push(value); } }, {
    async renderRetainedBook(_files, options, format) {
      assert.equal(options.profile, "commonmark-gfm");
      return { bytes: encode({ kind: format }), sourceLength: 10,
        ...(format === "links" ? {} : { profile: options.profile }) };
    }, clearRetainedBook() {},
  });
  const base = { schemaVersion: 1, retainBook: true, maxOutputBytes: 1024 };
  await listener({ data: { ...base, id: 1, format: "site", files, options: { profile: "commonmark-gfm" } } });
  await listener({ data: { ...base, id: 2, format: "links", sourceUpdate: { expectedRevision: 1, files: [] } } });
  assert.equal(replies[1].error, undefined);
  assert.equal(replies[1].bookAuthoringProfile, "commonmark-gfm");
  assert(!Object.hasOwn(replies[1], "profile"));
});

test("atomic publication retains one profile through PDF, EPUB and site outputs", async t => {
  const f = transport(t);
  const publication = createBookPublicationClient({ createClient: f.make, captureInput: prepareBookInput });
  t.after(() => publication.dispose());
  const result = await publication.render(files, ["pdf", "epub", "site"], { profile: "commonmark-gfm" });
  assert.equal(result.outputs.length, 3);
  for (const output of result.outputs) {
    assert.equal(output.profile, "commonmark-gfm");
    assert.equal(decode(output).settings.profile, "commonmark-gfm");
  }
});

test("publication rejects injected clients that strip an actual exporter receipt", async t => {
  const f = transport(t);
  const publication = createBookPublicationClient({ captureInput: prepareBookInput, createClient() {
    const client = f.make();
    return { ...client, async renderSourceUpdate(...args) {
      const { profile, ...unacknowledged } = await client.renderSourceUpdate(...args);
      assert.equal(profile, "gfm-plus"); return unacknowledged;
    } };
  } });
  t.after(() => publication.dispose());
  await assert.rejects(publication.render(files, ["pdf", "epub"], { profile: "gfm-plus" }), unsupported);
});

test("selected preview captures and checks the native chapter dialect before async initialization", async () => {
  let release, disposed = false;
  const wait = new Promise(resolve => { release = resolve; });
  const bytes = encode({ schema: "fmd-book-chapter-preview-v1", selected: 0,
    pages: [{ path: "one.html", source: "one.md", title: "One" }], html: "<p>Native fixture output</p>" });
  const engine = { async createBook() {
    await wait;
    return { chapterCount: 1, supportsChapterPreview: true,
      renderChapterPreview: () => ({ bytes, sourceLength: 10, profile: "commonmark-gfm" }),
      dispose() { disposed = true; } };
  } };
  const options = { profile: "commonmark-gfm" }, pending = renderBookSelectedPreview(engine, files, options);
  options.profile = "gfm-plus"; release();
  assert.equal((await pending).profile, "commonmark-gfm"); assert(disposed);
  await assert.rejects(renderBookSelectedPreview(engine, files, { profile: "gfm-plus" }), unsupported);
});

test("profile receipts validate omission, canonical shape and exact capture identity", () => {
  assert.deepEqual(bookAuthoringProfileReceipt({}), {});
  assert.deepEqual(bookAuthoringProfileReceipt({ profile: "gfm-plus" }), { profile: "gfm-plus" });
  for (const [output, expected] of [[{}, "gfm-plus"], [{ profile: "gfm-plus" }, undefined],
    [{ profile: "commonmark-gfm" }, "gfm-plus"], [{ profile: null }, undefined], [{ profile: "" }, undefined]])
    assert.throws(() => bookAuthoringProfileReceipt(output, expected), unsupported);
});
