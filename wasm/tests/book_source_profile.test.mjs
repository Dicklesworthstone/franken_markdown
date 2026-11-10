// Production adapters, worker protocol and publication coordinator with an explicit
// native ABI double. The profile-sensitive Markdown fixture is also exercised by
// tests/book_authoring_profiles.rs; this file does not implement a Markdown parser.
import assert from "node:assert/strict";
import test from "node:test";
import {
  bookLinkOptions, bookTextBytes, createBookBindings, parseBookLinkReport, prepareBookInput,
} from "../book_session.mjs";
import { createRetainedBook, createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import {
  createBookPublicationClient, createBookPublicationLinkCheck, createBookPublicationRetainedLinkCheck,
} from "../book_publication.mjs";

const files = [
  { path: "a.md", source: "# A\n\n[Term\n: definition](b.md#b)\n" },
  { path: "b.md", source: "# B\n" },
];
const includes = [{ path: "part.md", source: "# Part\r\n" }];
const profiles = ["commonmark-gfm", "gfm-plus"];
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const decode = value => JSON.parse(new TextDecoder().decode(value.bytes));

function nativeFixture({ ready = Promise.resolve(), supported = true } = {}) {
  const state = { constructions: [], checks: [], images: [], fonts: [], frees: 0, loads: 0 };
  class Book {
    constructor(paths, sources, profile, includePaths = [], includeText = [], expanded = false) {
      this.files = paths.map((path, i) => ({ path, source: sources[i] }));
      this.includes = includePaths.map((path, i) => ({ path, source: includeText[i] }));
      this.profile = profile;
      this.chapterCount = paths.length;
      this.sourceRevision = 0;
      this.sourceLength = [...sources, ...includeText].reduce((sum, source) => sum + bookTextBytes(source), 0);
      state.constructions.push({ profile, expanded, files: this.files, includes: this.includes });
    }
    static newWithProfile(paths, sources, profile) { return new Book(paths, sources, profile); }
    static fromSources(paths, sources, includePaths, includeText) {
      return new Book(paths, sources, undefined, includePaths, includeText, true);
    }
    static fromSourcesProfiled(paths, sources, includePaths, includeText, profile) {
      return new Book(paths, sources, profile, includePaths, includeText, true);
    }
    setAuthoringProfile(profile) { assert.equal(profile, this.profile); }
    setMetadata() {}
    setCustomCss() {}
    setTheme() {}
    setNavigation() {}
    setImage(destination, bytes) { state.images.push({ destination, bytes: [...bytes] }); }
    setFont(slot, bytes) { state.fonts.push({ slot, bytes: [...bytes] }); }
    updateSources() { throw new Error("These checks must not mutate retained source."); }
    validateLinks() {
      state.checks.push({ kind: "links", profile: this.profile });
      // Rust's profile-sensitive cross-line link fixture has different navigation
      // counts in each dialect. Only the native boundary selection is doubled here.
      return JSON.stringify({
        schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
        chapters: this.files.map((file, i) => ({
          path: file.path, checked: i === 0 && this.profile === "commonmark-gfm" ? 1 : 0,
          external: 0, unchecked: 0, findings: [],
        })),
      });
    }
    sourceDiagnostics() {
      state.checks.push({ kind: "diagnostics", profile: this.profile });
      return JSON.stringify({
        schema: "fmd-book-source-diagnostics-v1", coordinateSpace: "expanded_chapter",
        chapters: this.files.map(file => ({
          path: file.path, sourceBytes: bookTextBytes(file.source), diagnostics: [],
        })),
        summary: { chapters: this.chapterCount, diagnostics: 0 },
      });
    }
    renderSite() { return encode({ profile: this.profile, files: this.files, images: state.images }); }
    renderPdf() { return this.renderSite(); }
    free() { state.frees++; }
  }
  if (!supported) {
    delete Book.newWithProfile;
    delete Book.fromSourcesProfiled;
    delete Book.prototype.setAuthoringProfile;
  }
  const api = createBookBindings(async () => { state.loads++; await ready; return Book; });
  const retained = createRetainedBook(api, session => session.renderBookSite());
  const engine = { ...api, renderRetainedBook: retained.render, clearRetainedBook: retained.clear };
  return { api, engine, retained, state };
}

function transport() {
  const instances = [], sent = [];
  function client(retainBook = false) {
    return createBookWorkerClient({ retainBook, timeoutMs: 10000, workerFactory() {
      const native = nativeFixture();
      instances.push(native);
      const host = new EventTarget(), worker = new EventTarget();
      let terminated = false;
      host.postMessage = value => {
        const owned = structuredClone(value);
        sent.push(owned);
        queueMicrotask(() => {
          if (!terminated) worker.dispatchEvent(new MessageEvent("message", { data: owned }));
        });
      };
      worker.postMessage = value => {
        const owned = structuredClone(value);
        queueMicrotask(() => {
          if (!terminated) host.dispatchEvent(new MessageEvent("message", { data: owned }));
        });
      };
      host.terminate = () => { terminated = true; native.retained.clear(); };
      installBookWorker(worker, native.engine);
      return host;
    } });
  }
  return { client, instances, sent };
}

function sourceOptions(profile) {
  const options = { profile, includeSources: structuredClone(includes), expandIncludes: true };
  for (const key of ["images", "fontAssets", "customCss", "title", "page", "pdfA", "typography"])
    Object.defineProperty(options, key, { get() { throw new Error("Unexpected resource access: " + key); } });
  return options;
}

test("source check options retain one canonical dialect without reading publication resources", () => {
  for (const profile of profiles) {
    const options = sourceOptions(profile);
    let reads = 0;
    Object.defineProperty(options, "profile", { get() { reads++; return profile; } });
    assert.deepEqual(bookLinkOptions(options), { profile, includeSources: includes, expandIncludes: true });
    assert.equal(reads, 1);
  }
  assert.deepEqual(bookLinkOptions({}), { includeSources: undefined, expandIncludes: undefined });
});

test("invalid source-check dialects refuse before include getters or native initialization", async () => {
  const f = nativeFixture();
  for (const profile of [null, "", "gfm", " commonmark-gfm ", true, {}]) {
    for (const method of ["checkBookLinks", "checkBookSourceDiagnostics"]) {
      const options = { profile, get includeSources() { throw new Error("No include access allowed."); } };
      await assert.rejects(f.api[method](files, options), { code: "INVALID_OPTIONS" });
    }
  }
  assert.equal(f.state.loads, 0);
  assert.equal(f.state.constructions.length, 0);
});

for (const method of ["checkBookLinks", "checkBookSourceDiagnostics"]) {
  test(method + " owns the selected dialect and exact sources before engine initialization", async () => {
    let release;
    const f = nativeFixture({ ready: new Promise(resolve => { release = resolve; }) });
    const source = structuredClone(files), options = sourceOptions("commonmark-gfm");
    const pending = f.api[method](source, options);
    source[0].source = "changed";
    options.profile = "gfm-plus";
    options.includeSources[0].source = "changed include";
    release();
    await pending;
    assert.deepEqual(f.state.constructions, [
      { profile: "commonmark-gfm", expanded: true, files, includes },
    ]);
    assert.deepEqual(f.state.images, []);
    assert.deepEqual(f.state.fonts, []);
    assert.equal(f.state.frees, 1);
  });

  test(method + " rejects old native profile support instead of checking another grammar", async () => {
    const f = nativeFixture({ supported: false });
    for (const profile of profiles)
      await assert.rejects(f.api[method](files, { profile }), { code: "UNSUPPORTED_BOOK_PROFILE" });
    assert.equal(f.state.constructions.length, 0);
    await f.api[method](files);
    assert.equal(f.state.constructions.length, 1);
    assert.equal(f.state.frees, 1);
  });
}

test("one-shot worker links and diagnostics preserve dialects without resource transfer", async () => {
  const t = transport(), client = t.client();
  try {
    for (const profile of profiles) for (const format of ["links", "diagnostics"]) {
      await client.render(files, format, sourceOptions(profile));
      const state = t.instances.at(-1).state;
      assert.equal(state.constructions.at(-1).profile, profile);
      assert.deepEqual(state.images, []);
      assert.deepEqual(state.fonts, []);
      assert.equal(client.retainedInputRevision, null);
      const request = t.sent.at(-1);
      assert.equal(request.options.profile, profile);
      assert.deepEqual(request.options.images, []);
      assert.deepEqual(request.options.fontAssets, []);
      assert.equal(request.options.customCss, undefined);
    }
  } finally { client.dispose(); }
});

test("one-shot and retained worker navigation inspect the same chosen grammar", async () => {
  for (const profile of profiles) {
    const t = transport(), direct = t.client(), retained = t.client(true);
    try {
      const first = await direct.render(files, "links", { profile });
      await retained.render(files, "site", { profile });
      const second = await retained.renderSourceUpdate([], "links", {
        expectedRevision: retained.retainedInputRevision,
      });
      assert.deepEqual(parseBookLinkReport(first.bytes, ["a.md", "b.md"]),
        parseBookLinkReport(second.bytes, ["a.md", "b.md"]));
    } finally { direct.dispose(); retained.dispose(); }
  }
});

for (const profile of profiles) test("strict " + profile + " publication verifies the same capture before and after exports", async () => {
  const t = transport();
  const publication = createBookPublicationClient({
    captureInput: prepareBookInput, createClient: () => t.client(true),
    checkLinks: createBookPublicationLinkCheck({
      createClient: () => t.client(), parseReport: parseBookLinkReport, textBytes: bookTextBytes,
    }),
    checkRetainedLinks: createBookPublicationRetainedLinkCheck({ parseReport: parseBookLinkReport }),
  });
  const bytes = new Uint8Array([1, 2, 3]);
  const options = { profile, includeSources: structuredClone(includes),
    images: [{ destination: "one.png", bytes }] };
  try {
    const result = await publication.render(files, ["pdf", "site"], options, {
      strictLinks: true, onProgress(progress) {
        if (progress.phase === "checking-links") {
          options.profile = profile === "commonmark-gfm" ? "gfm-plus" : "commonmark-gfm";
          bytes.fill(9);
        }
      },
    });
    assert.equal(result.outputs.length, 2);
    assert.equal(t.instances.length, 2);
    assert(t.instances.every(native => native.state.constructions[0].profile === profile));
    assert.deepEqual(t.instances[0].state.images, []);
    assert.deepEqual(t.instances[1].state.images, [{ destination: "one.png", bytes: [1, 2, 3] }]);
    for (const output of result.outputs) {
      assert.equal(output.profile, profile);
      assert.equal(decode(output).profile, profile);
      assert.deepEqual(decode(output).images, [{ destination: "one.png", bytes: [1, 2, 3] }]);
    }
    assert(t.instances.every(native => native.state.frees === 1));
  } finally { publication.dispose(); }
});
