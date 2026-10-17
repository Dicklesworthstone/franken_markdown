import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings, parseBookLinkReport } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createRetainedBook } from "../book_retained.mjs";

// Only the native class and Worker endpoint are doubles. The facade, retained
// session, client and installed transport handler are the production modules.
// These fixtures do NOT parse Markdown or prove native navigation correctness.
const code = value => cause => cause?.code === value;
const encoder = new TextEncoder();
const files = [{ path: "guide/one.md", source: "# One\r\n" }];
const options = { includeSources: [{ path: "parts/shared.md", source: "Shared ✓" }],
  title: "Publication", images: [{ destination: "x.png", bytes: new Uint8Array([1, 2]) }],
  fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]) }] };
function deferred() {
  let resolve;
  const promise = new Promise(yes => { resolve = yes; });
  return { promise, resolve };
}
function harness(t, settings = {}) {
  const h = { created: 0, freed: 0, images: 0, fonts: 0, validations: 0, updates: 0,
    endpoints: 0, terminated: 0, posts: [], methods: [], sessions: [] };
  class NativeFixture {
    constructor(paths, sources) {
      h.created++; h.sessions.push(this); this.paths = [...paths]; this.sources = [...sources];
      this.resourcePaths = []; this.resources = []; this.sourceRevision = 0;
    }
    static fromSources(paths, sources, resourcePaths, resources) {
      const instance = new NativeFixture(paths, sources);
      instance.resourcePaths = [...resourcePaths]; instance.resources = [...resources]; return instance;
    }
    get chapterCount() { return this.paths.length; }
    get sourceLength() { return [...this.sources, ...this.resources].reduce((n, s) => n + encoder.encode(s).length, 0); }
    setMetadata(title) { this.title = title; }
    setCustomCss() {} setTheme() {} setNavigation() {}
    setImage() { h.images++; } setFont() { h.fonts++; }
    updateSources(paths, sources, expected) {
      assert.equal(expected, this.sourceRevision); h.updates++;
      for (let i = 0; i < paths.length; i++) {
        const chapter = this.paths.indexOf(paths[i]);
        if (chapter >= 0) this.sources[chapter] = sources[i];
        else this.resources[this.resourcePaths.indexOf(paths[i])] = sources[i];
      }
      return JSON.stringify({ schema: "fmd-book-source-update-v1", revision: ++this.sourceRevision,
        changed_sources: paths.length, chapter_count: this.chapterCount, source_length: this.sourceLength,
        reparsed_chapters: this.paths.map((_, i) => i) });
    }
    renderPdf() { h.methods.push("pdf"); return encoder.encode("opaque-pdf"); }
    renderEpub() { h.methods.push("epub"); return encoder.encode("opaque-epub"); }
    renderSite() { h.methods.push("site"); return encoder.encode("opaque-site"); }
    validateLinks() {
      h.methods.push("links"); h.validations++;
      if (settings.failure) throw Object.assign(Error("Native analysis unavailable"), { code: "CHECK_LIMIT" });
      if (settings.oversized) return "x".repeat(4 * 1024 * 1024 + 1);
      return JSON.stringify({ schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
        chapters: this.paths.map(path => ({ path, checked: 0, external: 0, unchecked: 0, findings: [] })) });
    }
    free() { h.freed++; }
  }
  if (settings.unsupported) NativeFixture.prototype.validateLinks = undefined;
  const bindings = createBookBindings(async () => NativeFixture);
  h.factory = () => {
    h.endpoints++;
    const retained = createRetainedBook(bindings, () => { throw Error("unexpected preview"); });
    const listeners = new Map(); let receive, stopped = false;
    const engine = { ...bindings, clearRetainedBook: retained.clear,
      async renderRetainedBook(...args) {
        if (args[2] === "links" && settings.hold) {
          settings.entered.resolve(); await settings.hold.promise;
          if (stopped) return { bytes: encoder.encode("late"), sourceLength: 0 };
        }
        return retained.render(...args);
      } };
    installBookWorker({ addEventListener(_, fn) { receive = fn; }, postMessage(value) {
      if (!stopped) for (const fn of [...(listeners.get("message") ?? [])]) fn({ data: structuredClone(value) });
    } }, engine);
    return {
      addEventListener(kind, fn) { const set = listeners.get(kind) ?? new Set(); set.add(fn); listeners.set(kind, set); },
      removeEventListener(kind, fn) { listeners.get(kind)?.delete(fn); },
      postMessage(value, transfer) {
        h.posts.push({ value: structuredClone(value), transfers: transfer.length });
        queueMicrotask(() => { void receive({ data: structuredClone(value) }); });
      },
      terminate() { stopped = true; h.terminated++; retained.clear(); },
    };
  };
  h.client = createBookWorkerClient({ workerFactory: h.factory, retainBook: true, ...settings.client });
  t.after(() => h.client.dispose());
  h.check = () => h.client.renderSourceUpdate([], "links", { expectedRevision: h.client.retainedInputRevision });
  return h;
}

test("PDF, EPUB, site and navigation check share the identical native book and resource capture", async t => {
  const h = harness(t);
  let output = await h.client.render(files, "pdf", options);
  for (const format of ["epub", "site"]) output = await h.client.renderSourceUpdate([], format,
    { expectedRevision: output.retainedInputRevision });
  const checked = await h.check();
  assert.deepEqual(h.methods, ["pdf", "epub", "site", "links"]);
  assert.equal(h.created, 1); assert.equal(h.endpoints, 1); assert.equal(h.images, 1); assert.equal(h.fonts, 1);
  assert.equal(h.updates, 0); assert.equal(checked.sourceLength, output.sourceLength);
  assert.equal(checked.format, "book-links"); assert.equal(checked.retainedInputRevision, 4);
  assert.equal(parseBookLinkReport(checked.bytes, [files[0].path]).summary.findings, 0);
  assert.equal(h.posts[0].transfers, 2);
  for (const post of h.posts.slice(1)) {
    assert.equal(post.transfers, 0); assert.equal("files" in post.value, false); assert.equal("options" in post.value, false);
    assert.deepEqual(post.value.sourceUpdate.files, []);
  }
  const again = await h.client.renderSourceUpdate([], "epub", { expectedRevision: 4 });
  assert.equal(again.retainedInputRevision, 5); assert.equal(h.created, 1);
  h.client.dispose(); assert.equal(h.freed, 1); assert.equal(h.terminated, 1);
});

test("link analysis can follow a native source transaction without rebuilding or retransferring assets", async t => {
  const h = harness(t); await h.client.render(files, "pdf", options);
  const output = await h.client.renderSourceUpdate([{ path: "parts/shared.md", source: "Updated shared" }], "links",
    { expectedRevision: h.client.retainedInputRevision });
  assert.equal(h.created, 1); assert.equal(h.updates, 1); assert.equal(h.images, 1);
  assert.equal(output.sourceLength, encoder.encode(files[0].source + "Updated shared").length);
  assert.equal(h.sessions[0].resources[0], "Updated shared");
});

test("source-set checks use the new chapter membership, not the previous report", async t => {
  const h = harness(t); await h.client.render(files, "pdf", options);
  const output = await h.client.renderSourceSet([{ path: "new.md", source: "New" }], "links",
    { expectedRevision: h.client.retainedInputRevision, includeSources: [] });
  // The fixture deliberately has no native replaceSources: the production
  // retained adapter reconstructs from the exact newly admitted source set.
  assert.equal(h.created, 2); assert.equal(h.freed, 1);
  assert.equal(parseBookLinkReport(output.bytes, ["new.md"]).summary.chapters, 1);
});

test("a stale link-check revision cannot discard a newer retained publication", async t => {
  const h = harness(t); await h.client.render(files, "pdf", options); await h.check();
  await assert.rejects(h.client.renderSourceUpdate([], "links", { expectedRevision: 1 }), code("STALE_BOOK_CAPTURE"));
  assert.equal(h.client.retainedInputRevision, 2); assert.equal(h.terminated, 0); await h.check();
});

test("full links requests stay one-shot, source-only and do not read resource getters", async t => {
  const h = harness(t); await h.client.render(files, "pdf", options);
  const input = { includeSources: options.includeSources,
    get images() { throw Error("must not read images"); },
    get fontAssets() { throw Error("must not read fonts"); },
    get title() { throw Error("must not read title"); } };
  const output = await h.client.render(files, "links", input);
  assert.equal(output.retainedInputRevision, undefined); assert.equal(h.client.hasRetainedBook, false);
  assert.equal(h.posts[1].transfers, 0); assert.equal(h.posts[1].value.retainBook, undefined);
  assert.equal(h.images, 1); assert.equal(h.fonts, 1); assert.equal(h.created, 2); assert.equal(h.freed, 2);
});

for (const mode of ["failure", "unsupported", "oversized"]) test(`${mode} analysis retires the capture with no rendering fallback`, async t => {
  const h = harness(t, { [mode]: true }); await h.client.render(files, "pdf", options);
  await assert.rejects(h.check());
  assert.equal(h.created, 1); assert.equal(h.freed, 1); assert.equal(h.terminated, 1);
  assert.equal(h.client.retainedInputRevision, null); assert.deepEqual(h.methods.filter(x => x !== "links"), ["pdf"]);
});

test("link response byte ceiling is enforced by the client even against a custom worker", async t => {
  let send, serial = 0, terminated = 0;
  const client = createBookWorkerClient({ retainBook: true, workerFactory: () => ({
    addEventListener(kind, fn) { if (kind === "message") send = fn; }, removeEventListener() {},
    terminate() { terminated++; }, postMessage(input) {
      const bytes = input.format === "links" ? new Uint8Array(4 * 1024 * 1024 + 1) : new Uint8Array([1]);
      queueMicrotask(() => send({ data: { ...input, bytes, sourceLength: 0, retainedBook: true, retainedInputRevision: ++serial } }));
    },
  }) }); t.after(() => client.dispose());
  await client.render(files, "pdf");
  await assert.rejects(client.renderSourceUpdate([], "links", { expectedRevision: 1 }), code("OUTPUT_LIMIT"));
  assert.equal(terminated, 1);
});

test("cancellation retires a held check promptly and late completion cannot restore its capture", async t => {
  const hold = deferred(), entered = deferred(), h = harness(t, { hold, entered });
  await h.client.render(files, "pdf", options);
  const pending = h.check(); await entered.promise;
  const rejected = assert.rejects(pending, code("EXPORT_CANCELLED")); h.client.cancel(); await rejected;
  assert.equal(h.freed, 1); assert.equal(h.client.hasRetainedBook, false);
  await h.client.render(files, "epub", options); const revision = h.client.retainedInputRevision;
  hold.resolve(); await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.client.retainedInputRevision, revision); assert.equal(h.created, 2);
});

test("preview-only retention cannot silently acquire publication analysis authority", async t => {
  const h = harness(t, { client: { retainBook: false, retainPreview: true } });
  await assert.rejects(h.client.renderSourceUpdate([], "links", { expectedRevision: 1 }), code("UNSUPPORTED_BOOK_SOURCE_DELTA"));
  assert.equal(h.endpoints, 0);
});
