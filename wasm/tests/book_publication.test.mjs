import assert from "node:assert/strict";
import test from "node:test";
import { createBookPublicationClient } from "../book_publication.mjs";
import { createRetainedBook } from "../book_retained.mjs";

const files = () => [{ path: "guide.md", source: "# Guide" }];
const capture = (files, options) => structuredClone({ files, options });
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const tick = () => new Promise(resolve => setImmediate(resolve));
const receipt = (format, revision, bytes = [1, 2, 3], sourceLength = 7) => ({
  format: `book-${format}`, retainedInputRevision: revision, sourceLength,
  bytes: Uint8Array.from(bytes),
});
const code = expected => cause => cause?.code === expected;
function fixture({ handler, captureInput = capture, create, dispose, ...settings } = {}) {
  const calls = [], clients = [];
  let captures = 0;
  const publisher = createBookPublicationClient({
    ...settings,
    captureInput(...args) { captures++; return captureInput(...args); },
    createClient() {
      const index = clients.length;
      const client = {
        disposed: 0,
        render(...args) { calls.push([index, "render", ...args]); return handler?.("render", args, index) ?? receipt(args[1], 1); },
        renderSourceUpdate(...args) {
          calls.push([index, "delta", ...args]);
          return handler?.("delta", args, index) ?? receipt(args[1], args[2].expectedRevision + 1);
        },
        dispose() { this.disposed++; dispose?.(client); },
      };
      clients.push(client);
      create?.(client);
      return client;
    },
  });
  return { publisher, calls, clients, get captures() { return captures; } };
}

test("one publication captures once and requests every later format through an empty source delta", async () => {
  const f = fixture();
  const result = await f.publisher.render(files());
  assert.equal(result.format, "book-publication");
  assert.deepEqual(result.outputs.map(output => output.format), ["book-pdf", "book-epub", "book-site"]);
  assert.equal(f.captures, 1);
  assert.equal(f.clients.length, 1);
  assert.deepEqual(f.calls.map(call => [call[1], call[3]]), [["render", "pdf"], ["delta", "epub"], ["delta", "site"]]);
  assert.deepEqual(f.calls.slice(1).map(call => [call[2], call[4]]), [
    [[], { expectedRevision: 1 }], [[], { expectedRevision: 2 }],
  ]);
  assert.equal(f.clients[0].disposed, 1);
  assert.equal(f.publisher.busy, false);
  assert.equal(result.byteLength, 9);
  assert.equal(result.sourceLength, 7);
});

test("selection order is exact and a single selected format is supported", async () => {
  for (const formats of [["site", "pdf"], ["epub"]]) {
    const f = fixture();
    const result = await f.publisher.render(files(), formats);
    assert.deepEqual(result.outputs.map(output => output.extension), formats.map(format => format === "site" ? "zip" : format));
    assert.equal(f.calls.length, formats.length);
  }
});

test("empty, duplicate, unsupported, sparse and accessor format lists fail before capture or worker creation", async () => {
  let reads = 0;
  const accessor = ["pdf"];
  Object.defineProperty(accessor, "0", { get() { reads++; return "pdf"; } });
  for (const formats of [[], ["pdf", "pdf"], ["preview"], ["PDF"], ["constructor"], ["pdf", "epub", "site", "pdf"], "pdf", null, new Array(1), accessor]) {
    const f = fixture();
    await assert.rejects(f.publisher.render(files(), formats), code("INVALID_FORMAT"));
    assert.equal(f.captures, 0);
    assert.equal(f.clients.length, 0);
    assert.equal(f.publisher.busy, false);
  }
  assert.equal(reads, 0);
});

test("format iteration is bounded even when a proxy lies about dynamic length", async () => {
  const formats = new Proxy(["pdf"], { get(target, key) { if (key === "length") return 1e9; return target[key]; } });
  const f = fixture();
  assert.equal((await f.publisher.render(files(), formats)).outputs.length, 1);
});

test("constructor bounds reject invalid adapters, timeouts and combined output budgets", () => {
  for (const settings of [{ createClient: null }, { captureInput: null }, { timeoutMs: 0 }, { timeoutMs: 600001 },
    { timeoutMs: NaN }, { timeoutMs: 1.1 }, { maxOutputBytes: 0 }, { maxOutputBytes: 128 * 1024 * 1024 + 1 }, { maxOutputBytes: Infinity }])
    assert.throws(() => createBookPublicationClient({ createClient() {}, captureInput: capture, ...settings }), code("INVALID_OPTIONS"));
});

test("no result or partial output escapes while any requested format is unfinished", async () => {
  const gate = deferred();
  const f = fixture({ handler: (kind, args) => args[1] === "epub" ? gate.promise : undefined });
  let published = false;
  const pending = f.publisher.render(files()).then(value => { published = true; return value; });
  await tick();
  assert.equal(f.calls.length, 2);
  assert.equal(published, false);
  assert.equal(f.publisher.busy, true);
  assert.equal(Object.hasOwn(f.publisher, "outputs"), false);
  gate.resolve(receipt("epub", 2));
  assert.equal((await pending).outputs.length, 3);
});

test("a later native failure rejects the entire edition and never starts remaining formats", async () => {
  const cause = Object.assign(new Error("EPUB validation failed"), { code: "EPUB_ERROR" });
  const f = fixture({ handler: (kind, args) => args[1] === "epub" ? Promise.reject(cause) : undefined });
  await assert.rejects(f.publisher.render(files()), error => error === cause);
  assert.equal(f.calls.length, 2);
  assert.equal(f.clients[0].disposed, 1);
  assert.equal(f.publisher.busy, false);
});

test("all JavaScript thrown values remain failures, including null, undefined, zero and empty strings", async () => {
  for (const cause of [null, undefined, 0, "", false]) {
    const f = fixture({ handler() { throw cause; } });
    let rejected = false;
    await f.publisher.render(files()).then(() => assert.fail("must reject"), actual => { rejected = true; assert.equal(actual, cause); });
    assert.equal(rejected, true);
    assert.equal(f.clients[0].disposed, 1);
    assert.equal(f.publisher.busy, false);
  }
});

test("the output budget covers the sum, not each format independently", async () => {
  const f = fixture({ maxOutputBytes: 5 });
  await assert.rejects(f.publisher.render(files()), code("OUTPUT_LIMIT"));
  assert.equal(f.calls.length, 2);
  assert.equal(f.clients[0].disposed, 1);
  assert.equal((await fixture({ maxOutputBytes: 9 }).publisher.render(files())).byteLength, 9);
});

test("retained capture support is mandatory and cannot silently fall back to separate publications", async () => {
  const f = fixture({ handler: (kind, args) => ({ ...receipt(args[1], 1), retainedInputRevision: undefined }) });
  await assert.rejects(f.publisher.render(files()), code("UNSUPPORTED_BOOK_PUBLICATION"));
  assert.equal(f.calls.length, 1);
  assert.equal(f.clients.length, 1);
  assert.equal(f.clients[0].disposed, 1);
});

test("stale receipts, wrong formats, changed source lengths and malformed output envelopes reject atomically", async () => {
  const cases = [
    { retainedInputRevision: 1 }, { retainedInputRevision: 0 }, { retainedInputRevision: 1.5 },
    { format: "book-pdf" }, { sourceLength: 8 }, { sourceLength: NaN }, { sourceLength: -1 },
    { sourceLength: 64 * 1024 * 1024 + 1 }, { bytes: [1] }, { bytes: new Uint8Array() },
    { bytes: new Uint8Array(new SharedArrayBuffer(3)) },
  ];
  for (const changes of cases) {
    const f = fixture({ handler: (kind, args) => kind === "delta" ? { ...receipt(args[1], 2), ...changes } : undefined });
    await assert.rejects(f.publisher.render(files()));
    assert.equal(f.calls.length, 2);
    assert.equal(f.clients[0].disposed, 1);
  }
});

test("outputs own only returned views and survive buffer reuse and worker cleanup", async () => {
  const storage = Uint8Array.from([99, 1, 2, 3, 99]);
  const f = fixture({ handler(kind, args) {
    const revision = kind === "render" ? 1 : args[2].expectedRevision + 1;
    storage.set([revision, revision, revision], 1);
    return { ...receipt(args[1], revision), bytes: storage.subarray(1, 4) };
  }, dispose() { storage.fill(0); } });
  const result = await f.publisher.render(files());
  for (let i = 0; i < result.outputs.length; i++) {
    const output = result.outputs[i];
    assert.deepEqual([...output.bytes], [i + 1, i + 1, i + 1]);
    assert.equal(output.bytes.buffer.byteLength, 3);
    assert.equal(output.blob().size, 3);
    assert.equal(output.blob().type, ["application/pdf", "application/epub+zip", "application/zip"][i]);
    assert.ok(Object.isFrozen(output));
  }
  assert.ok(Object.isFrozen(result));
  assert.ok(Object.isFrozen(result.outputs));
});

test("one owned snapshot protects source, include text, settings and byte views throughout the edition", async () => {
  const gate = deferred();
  const sources = files(), formats = ["pdf", "epub", "site"];
  const options = { title: "Original", includeSources: [{ path: "part.md", source: "shared" }],
    images: [{ destination: "logo.png", bytes: new Uint8Array([7]) }], running: { header: { left: "Original" } } };
  const f = fixture({ handler: (kind) => kind === "render" ? gate.promise : undefined });
  const pending = f.publisher.render(sources, formats, options);
  sources[0].source = "changed"; formats.reverse(); options.title = "Changed";
  options.includeSources[0].source = "changed"; options.images[0].bytes[0] = 9;
  options.running.header.left = "Changed";
  gate.resolve(receipt("pdf", 1));
  await pending;
  assert.equal(f.captures, 1);
  assert.equal(f.calls[0][2][0].source, "# Guide");
  const captured = f.calls[0][4];
  assert.equal(captured.title, "Original");
  assert.equal(captured.includeSources[0].source, "shared");
  assert.equal(captured.images[0].bytes[0], 7);
  assert.equal(captured.running.header.left, "Original");
  assert.deepEqual(f.calls.slice(1).map(call => call[3]), ["epub", "site"]);
  assert.ok(f.calls.slice(1).every(call => Object.keys(call[4]).join() === "expectedRevision"));
});

test("busy spans the entire publication, including gaps between outputs", async () => {
  const gate = deferred();
  const f = fixture({ handler: (kind) => kind === "delta" ? gate.promise : undefined });
  const pending = f.publisher.render(files(), ["pdf", "epub"]);
  await tick();
  await assert.rejects(f.publisher.render(files()), code("BOOK_BUSY"));
  assert.equal(f.captures, 1);
  assert.equal(f.clients.length, 1);
  gate.resolve(receipt("epub", 2));
  await pending;
});

test("pre-aborted and invalid signals fail before input capture or worker creation", async () => {
  const controller = new AbortController(); controller.abort();
  for (const signal of [controller.signal, null, {}, { aborted: false }]) {
    const f = fixture();
    await assert.rejects(f.publisher.render(files(), ["pdf"], {}, { signal }));
    assert.equal(f.captures, 0);
    assert.equal(f.clients.length, 0);
  }
});

test("AbortSignal cancellation terminates the private client and ignores late success", async () => {
  const gate = deferred(), abort = new AbortController();
  const f = fixture({ handler: () => gate.promise });
  const pending = f.publisher.render(files(), undefined, {}, { signal: abort.signal });
  abort.abort();
  await assert.rejects(pending, code("EXPORT_CANCELLED"));
  assert.equal(f.clients[0].disposed, 1);
  assert.equal(f.publisher.busy, false);
  gate.resolve(receipt("pdf", 1)); await tick();
  assert.equal(f.calls.length, 1);
});

test("cancelled old work cannot retire a newer publication or expose late failures", async () => {
  const gates = [deferred(), deferred()];
  const f = fixture({ handler: (kind, args, index) => kind === "render" ? gates[index].promise : undefined });
  const old = f.publisher.render(files()); f.publisher.cancel();
  await assert.rejects(old, code("EXPORT_CANCELLED"));
  const next = f.publisher.render(files());
  gates[0].reject(new Error("late old failure")); await tick();
  assert.equal(f.publisher.busy, true);
  assert.equal(f.clients[1].disposed, 0);
  gates[1].resolve(receipt("pdf", 1));
  assert.equal((await next).outputs.length, 3);
  assert.deepEqual(f.clients.map(client => client.disposed), [1, 1]);
});

test("dispose is permanent and idempotent while ordinary cancel leaves the publisher reusable", async () => {
  const gate = deferred();
  const f = fixture({ handler: () => gate.promise });
  const pending = f.publisher.render(files());
  f.publisher.dispose(); f.publisher.dispose();
  await assert.rejects(pending, code("EXPORT_CANCELLED"));
  await assert.rejects(f.publisher.render(files()), code("SESSION_DISPOSED"));
  assert.equal(f.clients[0].disposed, 1);
  gate.resolve(receipt("pdf", 1)); await tick();
});

test("one shared deadline remains armed across all formats and is removed after cancellation", async (t) => {
  const jobs = new Map(), scheduled = [];
  t.mock.method(globalThis, "setTimeout", (fn, ms) => { const id = {}; jobs.set(id, fn); scheduled.push(ms); return id; });
  t.mock.method(globalThis, "clearTimeout", id => jobs.delete(id));
  const gate = deferred();
  const f = fixture({ timeoutMs: 25, handler: kind => kind === "delta" ? gate.promise : undefined });
  const pending = f.publisher.render(files());
  await tick();
  assert.equal(f.calls.length, 2);
  assert.deepEqual(scheduled, [25]);
  [...jobs.values()][0]();
  await assert.rejects(pending, code("EXPORT_TIMEOUT"));
  assert.equal(jobs.size, 0);
  assert.equal(f.clients[0].disposed, 1);
  gate.resolve(receipt("epub", 2)); await tick();
  assert.equal(f.calls.length, 2);
});

test("success removes the shared deadline and abort listener", async (t) => {
  const jobs = new Map(), listeners = new Set();
  t.mock.method(globalThis, "setTimeout", fn => { const id = {}; jobs.set(id, fn); return id; });
  t.mock.method(globalThis, "clearTimeout", id => jobs.delete(id));
  const signal = { aborted: false, addEventListener(kind, fn) { listeners.add(fn); }, removeEventListener(kind, fn) { listeners.delete(fn); } };
  const f = fixture();
  await f.publisher.render(files(), undefined, {}, { signal });
  assert.equal(jobs.size, 0);
  assert.equal(listeners.size, 0);
});

test("reentrant input capture cannot reserve a second job and can cancel without constructing a worker", async () => {
  let publisher, nested;
  const f = fixture({ captureInput() {
    nested = publisher.render(files()); publisher.cancel(); return capture(files(), {});
  } });
  publisher = f.publisher;
  const pending = publisher.render(files());
  await assert.rejects(nested, code("BOOK_BUSY"));
  await assert.rejects(pending, code("EXPORT_CANCELLED"));
  assert.equal(f.clients.length, 0);
});

test("cancellation during client creation disposes the newly returned endpoint exactly once", async () => {
  let publisher;
  const f = fixture({ create() { publisher.cancel(); } }); publisher = f.publisher;
  await assert.rejects(publisher.render(files()), code("EXPORT_CANCELLED"));
  assert.equal(f.clients[0].disposed, 1);
  assert.equal(f.calls.length, 0);
});

test("synchronously aborting signal adapters cannot leave a timer, worker or busy slot", async () => {
  const signal = { aborted: false, addEventListener(kind, fn) { fn(); }, removeEventListener() {} };
  const f = fixture();
  await assert.rejects(f.publisher.render(files(), ["pdf"], {}, { signal }), code("EXPORT_CANCELLED"));
  assert.equal(f.captures, 0);
  assert.equal(f.clients.length, 0);
  assert.equal(f.publisher.busy, false);
});

test("capture errors and malformed client factories release the slot without masking the cause", async () => {
  const cause = new Error("invalid source");
  const f = fixture({ captureInput() { throw cause; } });
  await assert.rejects(f.publisher.render(files()), error => error === cause);
  assert.equal(f.clients.length, 0);
  for (const client of [null, {}, { render() {}, dispose() {} }]) {
    const publisher = createBookPublicationClient({ captureInput: capture, createClient: () => client });
    await assert.rejects(publisher.render(files()), code("WORKER_FAILED"));
    assert.equal(publisher.busy, false);
  }
});

test("cleanup failures do not erase a valid edition or replace an original export failure", async () => {
  const f = fixture({ dispose() { throw new Error("cleanup"); } });
  assert.equal((await f.publisher.render(files())).outputs.length, 3);
  const cause = new Error("export");
  const bad = fixture({ handler() { throw cause; }, dispose() { throw new Error("cleanup"); } });
  await assert.rejects(bad.publisher.render(files()), error => error === cause);
  assert.equal(bad.publisher.busy, false);
});

test("a failed edition can be retried from a fresh capture with no outputs from the old attempt", async () => {
  const f = fixture({ handler(kind, args, index) {
    if (index === 0 && kind === "delta") throw new Error("first failed");
    return receipt(args[1], kind === "render" ? 1 : args[2].expectedRevision + 1, [index + 1]);
  } });
  await assert.rejects(f.publisher.render(files()));
  const next = await f.publisher.render(files());
  assert.equal(f.captures, 2);
  assert.equal(f.clients.length, 2);
  assert.ok(next.outputs.every(output => output.bytes[0] === 2));
  assert.deepEqual(f.clients.map(client => client.disposed), [1, 1]);
});

// Execute the unchanged production retained-session manager underneath the new
// coordinator. Only transport scheduling/receipts and native exporters are
// explicit doubles: this is not a WASM or publication-format conformance test.
test("actual retained-session code renders all formats through one native book and disposes it", async () => {
  let created = 0, disposed = 0, calls = [];
  const engine = { async createBook(files, options) {
    created++;
    const emit = format => {
      calls.push(format);
      assert.equal(files[0].source, "# Guide");
      assert.equal(options.title, "Edition");
      return { bytes: Uint8Array.from([format.length]), sourceLength: 7 };
    };
    return { renderPdf: () => emit("pdf"), renderEpub: () => emit("epub"), renderSite: () => emit("site"),
      dispose() { disposed++; } };
  } };
  const retained = createRetainedBook(engine, () => { throw new Error("unexpected preview"); });
  let input, revision = 0;
  const publish = async format => ({ ...await retained.render(input.files, input.options, format),
    format: `book-${format}`, retainedInputRevision: ++revision });
  const publisher = createBookPublicationClient({ captureInput: capture, createClient: () => ({
    render(files, format, options) { input = { files, options }; return publish(format); },
    renderSourceUpdate(files, format, options) {
      assert.deepEqual(files, []); assert.equal(options.expectedRevision, revision); return publish(format);
    },
    dispose() { retained.clear(); },
  }) });
  const result = await publisher.render(files(), undefined, { title: "Edition", includeSources: [], images: [], fontAssets: [] });
  assert.equal(created, 1);
  assert.equal(disposed, 1);
  assert.deepEqual(calls, ["pdf", "epub", "site"]);
  assert.equal(result.outputs.length, 3);
});
