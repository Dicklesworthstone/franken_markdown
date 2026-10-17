import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import {
  createBookPublicationClient, createBookPublicationLinkCheck,
  createBookPublicationRetainedLinkCheck,
} from "../book_publication.mjs";

// Real production coordinator and both checker adapters; transport and decoder
// are explicit boundary doubles, not a second Markdown implementation.
const code = expected => cause => cause?.code === expected;
const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
function report({ findings = 0, checked = 1, external = 2, path = "guide.md" } = {}) {
  return Object.freeze({ schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
    summary: Object.freeze({ chapters: 1, checked, external, unchecked: 3, findings }),
    chapters: Object.freeze([Object.freeze({ path, checked, external, unchecked: 3,
      findings: Object.freeze(Array.from({ length: findings }, () => Object.freeze({
        code: "missing_anchor", destination: "#lost", message: "Missing heading.",
      }))) })]) });
}
const files = [{ path: "guide.md", source: "# Guide" }];
const options = { includeSources: [{ path: "part.md", source: "abc" }],
  images: [{ destination: "logo.png", bytes: new Uint8Array([1, 2]) }] };
function fixture(t, settings = {}) {
  const h = { calls: [], disposed: 0, initial: report(), final: report(), captures: 0, decode: [] };
  h.parse = (bytes, paths) => {
    h.decode.push({ bytes, paths });
    if (h.decodeError) throw h.decodeError;
    assert.deepEqual(paths, ["guide.md"]);
    assert.deepEqual([...bytes], [42]);
    return h.final;
  };
  h.client = {
    render(captured, format, opts, request) {
      h.calls.push({ kind: "full", captured, format, opts, request });
      return { format: `book-${format}`, retainedInputRevision: 1, sourceLength: 10, bytes: new Uint8Array([1]) };
    },
    renderSourceUpdate(changes, format, opts, request) {
      h.calls.push({ kind: "delta", changes, format, opts, request });
      if (format !== "links") return { format: `book-${format}`, retainedInputRevision: opts.expectedRevision + 1,
        sourceLength: 10, bytes: new Uint8Array([2]) };
      const output = { format: "book-links", mimeType: "application/json", sourceLength: 10,
        retainedInputRevision: opts.expectedRevision + 1, bytes: new Uint8Array([42]) };
      return h.onFinal ? h.onFinal(output, request) : output;
    },
    dispose() { h.disposed++; },
  };
  h.publisher = createBookPublicationClient({
    captureInput(chapters, opts) { h.captures++; return structuredClone({ files: chapters, options: opts }); },
    checkLinks() { h.calls.push({ kind: "early" }); return { report: h.initial, sourceLength: 10 }; },
    checkRetainedLinks: createBookPublicationRetainedLinkCheck({ parseReport: h.parse }),
    createClient: () => h.client,
    ...settings,
  });
  h.run = (formats = ["pdf", "epub", "site"], request = { strictLinks: true }) =>
    h.publisher.render(files, formats, options, request);
  t.after(() => h.publisher.dispose());
  return h;
}

test("final native check uses the last capture after all formats, with no source or asset retransmission", async t => {
  const h = fixture(t); const result = await h.run();
  assert.deepEqual(h.calls.map(call => call.format ?? call.kind), ["early", "pdf", "epub", "site", "links"]);
  const last = h.calls.at(-1);
  assert.deepEqual(last.changes, []); assert.deepEqual(last.opts, { expectedRevision: 3 });
  assert.equal(last.captured, undefined); assert.equal(h.captures, 1); assert.equal(h.disposed, 1);
  assert.equal(result.linkReport, h.final); assert.notEqual(result.linkReport, h.initial);
  assert.equal(result.outputs.length, 3); assert.equal(result.byteLength, 3);
  assert.equal(h.decode.length, 1);
});

for (const formats of [["pdf"], ["epub"], ["site"], ["site", "pdf"]]) {
  test(`single formats and arbitrary reading selection use final verification: ${formats}`, async t => {
    const h = fixture(t); const result = await h.run(formats);
    assert.equal(h.calls.at(-1).opts.expectedRevision, formats.length);
    assert.deepEqual(result.outputs.map(value => value.format), formats.map(value => `book-${value}`));
  });
}

test("already rendered bytes remain private while the final native check is pending", async t => {
  const h = fixture(t), gate = deferred(); let published = false;
  h.onFinal = () => gate.promise;
  const pending = h.run().then(value => { published = true; return value; });
  await tick(); assert.equal(h.calls.at(-1).format, "links");
  assert.equal(h.publisher.busy, true); assert.equal(published, false);
  await assert.rejects(h.run(), code("BOOK_BUSY"));
  gate.resolve({ format: "book-links", mimeType: "application/json", sourceLength: 10,
    retainedInputRevision: 4, bytes: new Uint8Array([42]) });
  assert.equal((await pending).outputs.length, 3);
});

test("findings in the actual retained book refuse all previously rendered outputs", async t => {
  const h = fixture(t); h.final = report({ findings: 1 });
  await assert.rejects(h.run(), cause => cause.code === "BOOK_LINKS_FAILED" && cause.linkReport === h.final);
  assert.equal(h.disposed, 1); assert.equal(h.publisher.busy, false);
});

for (const change of [{ checked: 0 }, { checked: 2 }, { external: 1 }, { path: "other.md" }]) {
  test(`even a clean retained report must agree with early preflight: ${JSON.stringify(change)}`, async t => {
    const h = fixture(t); h.final = report(change);
    await assert.rejects(h.run(), code("BOOK_VERIFICATION_MISMATCH")); assert.equal(h.disposed, 1);
  });
}

for (const change of [{ retainedInputRevision: undefined }, { retainedInputRevision: 3 },
  { retainedInputRevision: NaN }, { sourceLength: 9 }, { format: "book-pdf" }, { mimeType: "text/html" }]) {
  test(`invalid retained capture envelope is rejected before decoding: ${JSON.stringify(change)}`, async t => {
    const h = fixture(t); h.onFinal = value => ({ ...value, ...change });
    await assert.rejects(h.run(), code("INVALID_LINK_REPORT")); assert.equal(h.decode.length, 0);
    assert.equal(h.disposed, 1);
  });
}

test("native/worker capability failures never fall back to the earlier clean report", async t => {
  const h = fixture(t); const refusal = Object.assign(Error("old worker"), { code: "UNSUPPORTED_BOOK_VERIFICATION" });
  h.onFinal = () => { throw refusal; };
  await assert.rejects(h.run(), cause => cause === refusal);
  assert.equal(h.calls.filter(value => value.kind === "full").length, 1); assert.equal(h.disposed, 1);
});

test("decoder refusal remains a failure even after every format rendered", async t => {
  const h = fixture(t); h.decodeError = Object.assign(Error("malformed or oversized JSON"), { code: "INVALID_LINK_REPORT" });
  await assert.rejects(h.run(), code("INVALID_LINK_REPORT")); assert.equal(h.disposed, 1);
});

for (const method of ["cancel", "dispose", "signal"]) {
  test(`${method} terminates final verification and ignores late success`, async t => {
    const h = fixture(t), gate = deferred(), abort = new AbortController();
    h.onFinal = () => gate.promise;
    const pending = h.run(undefined, { strictLinks: true, signal: abort.signal });
    await tick(); const finalSignal = h.calls.at(-1).request.signal;
    if (method === "signal") abort.abort(); else h.publisher[method]();
    await assert.rejects(pending, code("EXPORT_CANCELLED"));
    assert.equal(h.disposed, 1); assert.equal(finalSignal.aborted, true);
    gate.resolve({ format: "book-links", mimeType: "application/json", sourceLength: 10,
      retainedInputRevision: 4, bytes: new Uint8Array([42]) });
    await tick(); assert.equal(h.decode.length, 0); assert.equal(h.disposed, 1);
  });
}

test("one shared deadline also covers a held final check", async t => {
  const jobs = new Map(); t.mock.method(performance, "now", () => 0);
  t.mock.method(globalThis, "setTimeout", fn => { const id = {}; jobs.set(id, fn); return id; });
  t.mock.method(globalThis, "clearTimeout", id => jobs.delete(id));
  const h = fixture(t), gate = deferred(); h.onFinal = () => gate.promise;
  const pending = h.run(); await tick(); assert.equal(jobs.size, 1);
  [...jobs.values()][0](); await assert.rejects(pending, code("EXPORT_TIMEOUT"));
  assert.equal(h.disposed, 1); assert.equal(jobs.size, 0);
  gate.reject(Error("late native refusal")); await tick();
});

test("synchronous final decoding cannot overrun the deadline and still publish", async t => {
  let now = 0; t.mock.method(performance, "now", () => now);
  const h = fixture(t, { timeoutMs: 5 }); h.onFinal = value => { now = 6; return value; };
  await assert.rejects(h.run(), code("EXPORT_TIMEOUT")); assert.equal(h.disposed, 1);
});

test("non-strict editions perform neither check", async t => {
  const h = fixture(t); h.onFinal = () => { throw Error("must not check"); };
  const result = await h.run(undefined, { strictLinks: false });
  assert.equal(h.calls.length, 3); assert.equal(result.linkReport, undefined);
});

test("early findings still prevent all resource-bearing rendering and final checking", async t => {
  const h = fixture(t); h.initial = report({ findings: 1 });
  await assert.rejects(h.run(), code("BOOK_LINKS_FAILED")); assert.deepEqual(h.calls, [{ kind: "early" }]);
  assert.equal(h.disposed, 0);
});

test("public worker factory wires both gates and uses one source-only checker plus one retained renderer", async () => {
  // Preserve the real public factory body and replace only its module imports
  // with explicit transport/decoder adapters. No generated WASM is involved.
  const events = [], clients = [];
  const key = `__fmd_publication_test_${Date.now()}`;
  const expected = report();
  globalThis[key] = {
    createBookWorkerClient(settings) {
      const retained = settings.retainBook === true;
      const client = { dispose() { events.push(retained ? "release-renderer" : "release-checker"); },
        render(chapters, format) {
          events.push(format); assert.equal(format === "links", !retained);
          return { format: `book-${format}`, mimeType: "application/json", sourceLength: 10,
            bytes: new Uint8Array([42]), ...(retained ? { retainedInputRevision: 1 } : {}) };
        },
        renderSourceUpdate(changes, format, opts) {
          events.push(`retained-${format}`); assert.deepEqual(changes, []); assert.deepEqual(opts, { expectedRevision: 1 });
          return { format: "book-links", mimeType: "application/json", sourceLength: 10,
            bytes: new Uint8Array([42]), retainedInputRevision: 2 };
        },
      }; clients.push({ retained, settings }); return client;
    },
    prepareBookInput: (files, options) => structuredClone({ files, options }),
    parseBookLinkReport: () => expected,
    bookTextBytes: value => new TextEncoder().encode(value).length,
    createBookPublicationClient, createBookPublicationLinkCheck, createBookPublicationRetainedLinkCheck,
  };
  const module = values => `data:text/javascript,${encodeURIComponent(values.map(name => `export const ${name} = globalThis[${JSON.stringify(key)}].${name};`).join("\n"))}`;
  const source = (await readFile(new URL("../book-worker.js", import.meta.url), "utf8"))
    .replace('"./book_worker.mjs"', JSON.stringify(module(["createBookWorkerClient"])))
    .replace('"./book_session.mjs"', JSON.stringify(module(["prepareBookInput", "parseBookLinkReport", "bookTextBytes"])))
    .replace('"./book_publication.mjs"', JSON.stringify(module(["createBookPublicationClient", "createBookPublicationLinkCheck", "createBookPublicationRetainedLinkCheck"])));
  let publisher;
  try {
    const { createBookPublicationWorker } = await import(`data:text/javascript,${encodeURIComponent(source)}`);
    publisher = createBookPublicationWorker({ maxOutputBytes: 1, workerFactory() { throw Error("transport owns factory"); } });
    const output = await publisher.render(files, ["pdf"], options, { strictLinks: true });
    assert.equal(output.linkReport, expected); assert.equal(clients.length, 2);
    assert.equal(clients[0].settings.maxOutputBytes, 4 * 1024 * 1024);
    assert.equal(clients[1].retained, true);
    assert.equal(clients[1].settings.maxOutputBytes, 4 * 1024 * 1024);
    assert.equal(output.byteLength, 1); // The report has its own receive budget.
    assert.deepEqual(events, ["links", "release-checker", "pdf", "retained-links", "release-renderer"]);
    events.length = 0;
    // Raising the private receive cap cannot raise the public aggregate cap.
    await assert.rejects(publisher.render(files, ["pdf", "epub"], options, { strictLinks: true }), code("OUTPUT_LIMIT"));
    assert.deepEqual(events, ["links", "release-checker", "pdf", "release-renderer"]);
    events.length = 0;
    const unchecked = await publisher.render(files, ["pdf"], options);
    assert.equal(unchecked.linkReport, undefined);
    assert.equal(clients.at(-1).settings.maxOutputBytes, 1);
    assert.deepEqual(events, ["pdf", "release-renderer"]);
  } finally { publisher?.dispose(); delete globalThis[key]; }
});
