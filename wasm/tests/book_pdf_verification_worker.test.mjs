import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { BOOK_PDF_VERIFICATION_MAX_BYTES, parseBookPdfVerificationReport } from "../book_pdf_verification.mjs";
import { chapters, included, geometry, code } from "./book_pdfa_fixture.mjs";
import { verificationReport, verificationBytes } from "./book_pdf_verification_worker_fixture.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const sourceLength = (files, resources = []) => [...files, ...resources]
  .reduce((size, file) => size + new TextEncoder().encode(file.source).length, 0);
const observation = output => JSON.parse(parseBookPdfVerificationReport(output.bytes).layout.pages[0].runs[0].text);

function threaded(t, { retainBook = false, timeoutMs = 10000, maxOutputBytes, old = false, oldEngine = false } = {}) {
  const counters = new Int32Array(new SharedArrayBuffer(16));
  const state = { created: 0, terminated: 0, messages: [], stops: [], counters };
  const client = createBookWorkerClient({ retainBook, timeoutMs, maxOutputBytes,
    workerFactory() {
      state.created++;
      const worker = new Worker(new URL("./book_pdf_verification_worker_fixture.mjs", import.meta.url),
        { workerData: { old, oldEngine, counters: counters.buffer } });
      const listeners = new Map();
      return {
        postMessage(data, transfer) { state.messages.push(structuredClone(data)); worker.postMessage(data, transfer); },
        addEventListener(kind, handler) {
          if (!listeners.has(kind)) listeners.set(kind, new Map());
          const wrapped = kind === "message" ? data => handler({ data }) : error => handler({ error });
          listeners.get(kind).set(handler, wrapped); worker.on(kind, wrapped);
        },
        removeEventListener(kind, handler) {
          const wrapped = listeners.get(kind)?.get(handler);
          if (wrapped) worker.off(kind, wrapped);
          listeners.get(kind)?.delete(handler);
        },
        terminate() { state.terminated++; const stopped = worker.terminate(); state.stops.push(stopped); return stopped; },
      };
    },
  });
  t.after(async () => { client.dispose(); await Promise.all(state.stops); });
  return { client, state };
}

for (const retainBook of [false, true]) test(`physical worker owns configured verification before transfer: retained=${retainBook}`, async t => {
  const { client, state } = threaded(t, { retainBook });
  const files = chapters(), resources = included(); files[0].path = "./guide/one.md";
  const image = Uint8Array.of(90, 1, 2, 91), font = Uint8Array.of(80, 3, 4, 81);
  const options = { profile: "gfm-plus", pdfA: "2b", pdfAStrict: true, page: geometry(),
    includeSources: resources, title: "Captured title", typography: "strict-pagination,line-height=1.5",
    running: { header: { left: "{chapter}", image: { dest: "logo", position: "right", heightPt: 12 } } },
    images: [{ destination: "logo", bytes: new DataView(image.buffer, 1, 2) }],
    fontAssets: [{ slot: "body-regular", bytes: font.subarray(1, 3), weight: 650 }] };
  const expectedSourceLength = sourceLength(files, resources);
  const pending = client.render(files, "pdf-verification", options);
  files[0].source = "replaced after admission"; files[0].path = "wrong.md";
  resources[0].source = "changed include"; image.fill(0); font.fill(0);
  options.title = "wrong title"; options.profile = "commonmark-gfm";
  options.page.size.widthPt = 900; options.running.header.left = "wrong header";
  options.pdfAStrict = false;
  const output = await pending, got = observation(output), report = parseBookPdfVerificationReport(output.bytes);
  assert.equal(output.format, "book-pdf-verification"); assert.equal(output.mimeType, "application/json");
  assert.equal(output.extension, "json"); assert.equal(output.profile, "gfm-plus");
  assert.equal(output.pdfA, "2b"); assert.equal(output.pdfAStrict, true);
  assert.equal(output.sourceLength, expectedSourceLength);
  assert.deepEqual(report.navigation.chapters.map(chapter => chapter.path), ["guide/one.md", "two.md"]);
  assert.equal(got.files[0].source, chapters()[0].source); assert.equal(got.includes[0].source, included()[0].source);
  assert.deepEqual(got.page, [720, 540, 18, 24, 30, 36]); assert.equal(got.pdfA, "2b"); assert.equal(got.strict, true);
  assert.equal(got.settings.profile, "gfm-plus"); assert.equal(got.settings.metadata[0], "Captured title");
  assert.equal(got.settings.pdf[0], "strict-pagination,line-height=1.5");
  assert.equal(got.settings.pdf[8][0], "{chapter}");
  assert.deepEqual(got.images, [["logo", [1, 2]]]); assert.deepEqual(got.fonts, [["body-regular", [3, 4]]]);
  assert.deepEqual(got.settings.weight, ["body-regular", 650]);
  assert.deepEqual(report.destinations[0].source.ranges, [{ path: "shared.txt", start: 0, end: 4, kind: "rewritten" }]);
  assert.equal(report.destinations[0].parsedDestination, "../two.md#two");
  assert.equal(image.byteLength, 4); assert.equal(font.byteLength, 4);
  assert.deepEqual(new Uint8Array(await output.blob().arrayBuffer()), output.bytes);
  assert.equal(state.created, 1); assert.equal(state.terminated, retainBook ? 0 : 1);
});

test("retained PDF verification shares exact publication resources across deltas, undo cache hits and collection changes", async t => {
  const { client, state } = threaded(t, { retainBook: true });
  await client.render(chapters(), "pdf", { profile: "commonmark-gfm", pdfA: "2b", pdfAStrict: true,
    includeSources: included(), page: geometry(), images: [{ destination: "logo", bytes: Uint8Array.of(1, 2) }] });
  const check = changes => client.renderSourceUpdate(changes, "pdf-verification", { expectedRevision: client.retainedInputRevision });
  const first = await check([]), repeat = await check([]);
  assert.deepEqual(first.bytes, repeat.bytes); assert.equal(Atomics.load(state.counters, 0), 1);
  assert.equal(repeat.profile, "commonmark-gfm"); assert.equal(repeat.pdfAStrict, true);
  const revision = client.retainedInputRevision;
  await assert.rejects(client.renderSourceUpdate([], "pdf-verification", { expectedRevision: revision - 1 }), code("STALE_BOOK_CAPTURE"));
  assert.equal(client.retainedInputRevision, revision);
  const changed = await check([{ path: "shared.txt", source: "changed α" }]);
  assert.equal(observation(changed).includes[0].source, "changed α"); assert.equal(Atomics.load(state.counters, 0), 2);
  const restored = await check(included());
  assert.deepEqual(restored.bytes, first.bytes); assert.equal(Atomics.load(state.counters, 0), 2);
  const files = chapters().reverse(); files.push({ path: "extra.md", source: "# Extra" });
  const replaced = await client.renderSourceSet(files, "pdf-verification", { expectedRevision: client.retainedInputRevision });
  assert.deepEqual(parseBookPdfVerificationReport(replaced.bytes).navigation.chapters.map(chapter => chapter.path),
    ["two.md", "guide/one.md", "extra.md"]);
  assert.deepEqual(observation(replaced).includes, []); assert.equal(Atomics.load(state.counters, 0), 3);
  const recaptured = await client.renderSources(files, "pdf-verification", { expectedRevision: client.retainedInputRevision });
  assert.deepEqual(recaptured.bytes, replaced.bytes); assert.equal(Atomics.load(state.counters, 0), 3);
  assert.deepEqual(observation(recaptured).images, [["logo", [1, 2]]]);
  assert.equal(state.created, 1); assert.equal(state.terminated, 0);
  for (const message of state.messages.slice(1)) {
    assert.equal(message.files, undefined); assert.equal(message.options, undefined);
    assert.ok(message.sourceUpdate || message.sourceSet);
  }
});

for (const [label, settings] of [["old native ABI", { old: true }], ["old one-shot engine", { oldEngine: true }],
  ["old retained native ABI", { old: true, retainBook: true }]]) {
  test(`${label} refuses full verification without downgrading to source links or default PDF`, async t => {
    const { client, state } = threaded(t, settings);
    await assert.rejects(client.render(chapters(), "pdf-verification"), code("UNSUPPORTED_BOOK_PDF_VERIFICATION"));
    assert.equal(client.busy, false); assert.equal(client.hasRetainedBook, false);
    assert.equal(state.terminated, 1); assert.equal(Atomics.load(state.counters, 0), 0);
    const pdf = await client.render(chapters(), "pdf");
    assert.equal(pdf.format, "book-pdf"); assert.equal(state.created, 2);
  });
}

function endpoint(t, reply, settings = {}) {
  let handler; const state = { terminated: 0 };
  const client = createBookWorkerClient({ ...settings, workerFactory: () => ({
    addEventListener(kind, fn) { if (kind === "message") handler = fn; },
    removeEventListener(kind, fn) { if (kind === "message" && handler === fn) handler = null; },
    postMessage(request) { const data = reply(request); queueMicrotask(() => handler?.({ data })); },
    terminate() { state.terminated++; },
  }) });
  t.after(() => client.dispose()); return { client, state };
}
const response = request => ({ schemaVersion: 1, id: request.id, format: request.format,
  bytes: verificationBytes(request.files.map(file => file.path)), sourceLength: sourceLength(request.files) });

test("old worker scripts get an actionable unsupported-package error", async t => {
  const { client, state } = endpoint(t, request => ({ schemaVersion: 1, id: request.id, format: request.format,
    error: { code: "INVALID_FORMAT", message: "unknown format" } }));
  await assert.rejects(client.render(chapters(), "pdf-verification"), code("UNSUPPORTED_BOOK_PDF_VERIFICATION"));
  assert.equal(state.terminated, 1);
});

test("client rejects valid-looking reports for another source size, chapter order or nested layout", async t => {
  for (const mode of ["paths", "size", "layout", "request-mutation"]) {
    const { client, state } = endpoint(t, request => {
      if (mode === "request-mutation") request.files[0].path = "substituted.md";
      const reply = response(request), report = JSON.parse(new TextDecoder().decode(reply.bytes));
      if (mode === "paths") report.navigation.chapters.reverse();
      if (mode === "size") reply.sourceLength++;
      if (mode === "layout") report.layout.pages[0].number = 2;
      reply.bytes = encode(report); return reply;
    });
    await assert.rejects(client.render(chapters(), "pdf-verification"),
      code(mode === "size" ? "WORKER_PROTOCOL_ERROR" : "INVALID_BOOK_PDF_VERIFICATION"));
    assert.equal(state.terminated, 1); assert.equal(client.busy, false);
  }
});

test("both worker endpoints enforce the complete report ceiling and smaller caller budgets", async t => {
  const { client: limited, state } = threaded(t, { maxOutputBytes: 32 });
  await assert.rejects(limited.render(chapters(), "pdf-verification"), code("OUTPUT_LIMIT"));
  assert.equal(state.terminated, 1);
  for (const [maximum, length] of [[32, 33], [128 * 1024 * 1024, BOOK_PDF_VERIFICATION_MAX_BYTES + 1]]) {
    const { client } = endpoint(t, request => ({ ...response(request), bytes: new Uint8Array(length) }),
      { maxOutputBytes: maximum });
    await assert.rejects(client.render(chapters(), "pdf-verification"), code("OUTPUT_LIMIT"));
  }
});

test("worker-side validation refuses malformed, misidentified and oversized native results before posting bytes", async () => {
  const files = [{ path: "one.md", source: "hello" }];
  for (const mode of ["paths", "size", "layout", "oversized", "profile", "archival", "valid"]) {
    let receive; const replies = [];
    const options = mode === "profile" ? { profile: "gfm-plus" } : mode === "archival" ? { pdfA: "2b" } : {};
    installBookWorker({ addEventListener(_, listener) { receive = listener; }, postMessage(data) { replies.push(data); } }, {
      async verifyBookPdf() {
        const report = verificationReport(mode === "paths" ? ["another.md"] : ["one.md"]);
        if (mode === "layout") report.layout.verdict = "findings";
        return { bytes: mode === "oversized" ? new Uint8Array(BOOK_PDF_VERIFICATION_MAX_BYTES + 1) : encode(report),
          sourceLength: mode === "size" ? 6 : 5 };
      },
    });
    await receive({ data: { schemaVersion: 1, id: 1, format: "pdf-verification", files, options,
      maxOutputBytes: 128 * 1024 * 1024 } });
    const reply = replies[0]; assert.equal(replies.length, 1);
    if (mode === "valid") { assert.equal(reply.error, undefined); assert.ok(reply.bytes.length); }
    else {
      assert.equal(reply.bytes, undefined);
      const expected = { size: "WORKER_PROTOCOL_ERROR", oversized: "OUTPUT_LIMIT",
        profile: "UNSUPPORTED_BOOK_PROFILE", archival: "UNSUPPORTED_BOOK_PDFA" }[mode] ?? "INVALID_BOOK_PDF_VERIFICATION";
      assert.equal(reply.error.code, expected, mode);
    }
  }
});

test("strict pagination failure remains typed and a later corrected request can verify", async t => {
  const { client, state } = threaded(t, { retainBook: true });
  await assert.rejects(client.render([{ path: "one.md", source: "REFUSE" }], "pdf-verification",
    { strictPagination: true }), code("PAGINATION_INFEASIBLE"));
  assert.equal(client.hasRetainedBook, false); assert.equal(state.terminated, 1);
  const fixed = await client.render(chapters(), "pdf-verification", { strictPagination: true });
  assert.equal(parseBookPdfVerificationReport(fixed.bytes).layout.verdict, "clean");
  assert.equal(state.created, 2); assert.equal(Atomics.load(state.counters, 0), 2);
});

async function until(testValue) {
  const deadline = performance.now() + 5000;
  while (!testValue()) {
    assert(performance.now() < deadline, "worker did not reach the synchronous native boundary");
    await new Promise(resolve => setTimeout(resolve, 5));
  }
}

test("aborting in-flight retained verification terminates native work and retires the captured edition", async t => {
  const { client, state } = threaded(t, { retainBook: true });
  await client.render(chapters(), "pdf");
  const abort = new AbortController();
  const pending = client.renderSourceUpdate([{ path: chapters()[0].path, source: "BLOCK" }], "pdf-verification",
    { expectedRevision: client.retainedInputRevision }, { signal: abort.signal });
  const rejected = assert.rejects(pending, code("EXPORT_CANCELLED"));
  await until(() => Atomics.load(state.counters, 1) === 1); abort.abort(); await rejected;
  assert.equal(state.terminated, 1); assert.equal(client.hasRetainedBook, false); assert.equal(client.busy, false);
  const current = await client.render(chapters(), "pdf-verification");
  assert.equal(observation(current).files[0].source, chapters()[0].source); assert.equal(state.created, 2);
});

test("verification deadlines terminate a blocked native call without publishing a partial report", async t => {
  const { client, state } = threaded(t, { retainBook: true, timeoutMs: 2500 });
  await client.render(chapters(), "pdf");
  const pending = client.renderSourceUpdate([{ path: chapters()[0].path, source: "BLOCK" }], "pdf-verification",
    { expectedRevision: client.retainedInputRevision });
  const rejected = assert.rejects(pending, code("EXPORT_TIMEOUT"));
  await until(() => Atomics.load(state.counters, 1) === 1); await rejected;
  assert.equal(state.terminated, 1); assert.equal(client.retainedInputRevision, null); assert.equal(client.busy, false);
});
