import assert from "node:assert/strict";
import test from "node:test";
import { createBookPublicationClient, createBookPublicationLinkCheck } from "../book_publication.mjs";

// The production coordinator and source-only adapter execute unchanged. Native
// transport and the pre-existing report decoder are explicit boundary doubles;
// these tests prove orchestration, not Markdown semantics or generated WASM.
const code = expected => cause => cause?.code === expected;
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const textBytes = text => new TextEncoder().encode(text).length;
const input = () => ({ files: [{ path: "guide/one.md", source: "# One\r\n\n🙂" }], options: {
  includeSources: [{ path: "parts/two.txt", source: "α shared" }], expandIncludes: true,
  title: "Manual", images: [{ destination: "logo.png", bytes: new Uint8Array([1, 2]) }],
  fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([3, 4]) }],
} });
const size = value => [...value.files, ...value.options.includeSources].reduce((sum, file) => sum + textBytes(file.source), 0);
function report(findings = []) {
  return Object.freeze({ schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
    summary: Object.freeze({ chapters: 1, checked: findings.length, findings: findings.length, external: 0, unchecked: 0 }),
    chapters: Object.freeze([Object.freeze({ path: "guide/one.md", checked: findings.length, external: 0, unchecked: 0,
      findings: Object.freeze(findings.map(finding => Object.freeze(finding))) })]) });
}
const finding = { code: "missing_chapter", destination: "lost.md", message: "Missing chapter." };
function fixture(t, overrides = {}) {
  const h = { captured: null, checks: [], renders: [], calls: [], decoderCalls: [], captures: 0, freedChecks: 0, freedRenders: 0 };
  h.nativeReport = report();
  h.verify = createBookPublicationLinkCheck({ textBytes,
    parseReport(bytes, paths) {
      h.decoderCalls.push({ bytes, paths });
      if (h.decodeError) throw h.decodeError;
      return h.nativeReport;
    },
    createClient() {
      h.calls.push("checker"); overrides.createCheck?.(h);
      return { dispose() { h.freedChecks++; }, render(files, format, options, request) {
        const call = { files, format, options, request }; h.checks.push(call);
        const reply = { format: "book-links", mimeType: "application/json", sourceLength: size(h.captured), bytes: new Uint8Array([123, 125]) };
        return overrides.check?.(reply, call, h) ?? reply;
      } };
    },
  });
  h.publisher = createBookPublicationClient({
    timeoutMs: 10000, ...overrides.settings,
    captureInput(files, options) { h.captures++; h.captured = structuredClone({ files, options }); return h.captured; },
    checkLinks: (files, options, request) => h.verify(files, options, request),
    createClient() {
      h.calls.push("renderer"); let revision = 0;
      const render = (kind, files, format, options, request) => {
        const call = { kind, files, format, options, request }; h.renders.push(call);
        return overrides.render?.(call, h) ?? { format: `book-${format}`, sourceLength: size(h.captured),
          retainedInputRevision: ++revision, bytes: new TextEncoder().encode(`native-${format}`) };
      };
      return { render: (...args) => render("full", ...args), renderSourceUpdate: (...args) => render("delta", ...args),
        dispose() { h.freedRenders++; } };
    },
    ...overrides.client,
  });
  h.run = (request = { strictLinks: true }, formats = ["pdf", "epub", "site"], value = input()) =>
    h.publisher.render(value.files, formats, value.options, request);
  t.after(() => h.publisher.dispose());
  return h;
}

test("native verification gates every format against one owned source/include capture", async t => {
  const h = fixture(t); const result = await h.run();
  assert.deepEqual(h.calls, ["checker", "renderer"]); assert.equal(h.captures, 1);
  assert.equal(h.freedChecks, 1); assert.equal(h.freedRenders, 1);
  assert.deepEqual(Object.keys(h.checks[0].options).sort(), ["expandIncludes", "includeSources"]);
  assert.deepEqual(h.checks[0].files, h.captured.files);
  assert.notEqual(h.checks[0].files, h.captured.files);
  assert.notEqual(h.checks[0].options.includeSources, h.captured.options.includeSources);
  assert.deepEqual(h.decoderCalls[0].paths, ["guide/one.md"]);
  assert.equal(result.linkReport, h.nativeReport); assert(Object.isFrozen(result.linkReport));
  assert.equal(result.sourceLength, size(h.captured)); assert(result.sourceLength > h.captured.files[0].source.length);
  assert.deepEqual(h.renders.map(call => [call.kind, call.format]), [["full", "pdf"], ["delta", "epub"], ["delta", "site"]]);
  assert.deepEqual(h.renders.slice(1).map(call => call.options), [{ expectedRevision: 1 }, { expectedRevision: 2 }]);
  assert.deepEqual(h.renders[0].options.images, input().options.images);
  assert.deepEqual(h.renders[0].options.fontAssets, input().options.fontAssets);
  assert.equal(result.byteLength, result.outputs.reduce((sum, output) => sum + output.bytes.length, 0));
});

test("default and explicit non-strict publications retain their old result shape and never check", async t => {
  const h = fixture(t); h.decodeError = Error("must not check");
  for (const request of [{}, { strictLinks: false }]) {
    const output = await h.run(request, ["epub"]);
    assert.equal(Object.hasOwn(output, "linkReport"), false); assert.equal(output.outputs.length, 1);
  }
  assert.equal(h.checks.length, 0); assert.equal(h.decoderCalls.length, 0); assert.equal(h.renders.length, 2);
});

for (const findingCode of ["missing_chapter", "missing_anchor", "ambiguous_anchor", "invalid_fragment", "invalid_local_destination", "missing_footnote"]) {
  test(`${findingCode} refuses the complete edition before any renderer is created`, async t => {
    const h = fixture(t); h.nativeReport = report([{ ...finding, code: findingCode }]);
    await assert.rejects(h.run(), cause => {
      assert.equal(cause.code, "BOOK_LINKS_FAILED"); assert.equal(cause.linkReport, h.nativeReport);
      assert(cause.message.length < 256); return true;
    });
    assert.equal(h.renders.length, 0); assert.deepEqual(h.calls, ["checker"]);
    assert.equal(h.freedChecks, 1); assert.equal(h.publisher.busy, false);
  });
}

test("a source repair checks again and succeeds; no unchecked retry follows a failed check", async t => {
  const h = fixture(t); h.nativeReport = report([finding]);
  await assert.rejects(h.run(), code("BOOK_LINKS_FAILED"));
  h.nativeReport = report(); const value = input(); value.files[0].source += "\nrepaired";
  const output = await h.run({ strictLinks: true }, ["site"], value);
  assert.equal(output.sourceLength, size(value)); assert.equal(h.checks.length, 2); assert.equal(h.renders.length, 1);
});

test("invalid policy values and missing verification capability fail before source admission", async t => {
  const h = fixture(t);
  for (const strictLinks of [null, "true", 1, {}, []]) await assert.rejects(h.run({ strictLinks }), code("INVALID_OPTIONS"));
  assert.equal(h.captures, 0); assert.equal(h.calls.length, 0);
  const other = fixture(t, { client: { checkLinks: undefined } });
  await assert.rejects(other.run(), code("UNSUPPORTED_BOOK_VERIFICATION")); assert.equal(other.captures, 0);
  await other.run({ strictLinks: false }, ["pdf"]); assert.equal(other.renders.length, 1);
});

test("policy access is single-read and reentrant publication cannot steal the active operation", async t => {
  const h = fixture(t); let reads = 0, reentered;
  const request = { get strictLinks() { reads++; reentered = assert.rejects(h.run(), code("BOOK_BUSY")); return true; } };
  await h.run(request); await reentered; assert.equal(reads, 1); assert.equal(h.captures, 1);
});

test("caller edits during verification cannot change the checked publication or resource snapshot", async t => {
  const pending = deferred(); const h = fixture(t, { check: () => pending.promise });
  const value = input(); const running = h.run({ strictLinks: true }, ["pdf"], value);
  value.files[0].source = "replaced"; value.options.includeSources[0].source = "changed";
  value.options.images[0].bytes.fill(9);
  pending.resolve({ format: "book-links", mimeType: "application/json", bytes: new Uint8Array([1]), sourceLength: size(h.captured) });
  await running;
  assert.deepEqual(h.renders[0].files, input().files); assert.deepEqual(h.renders[0].options, input().options);
});

test("verification never owns the mutable containers used by the renderer", async t => {
  const h = fixture(t, { check(reply, call) {
    call.files[0].source = "checker-side mutation"; call.options.includeSources[0].source = "different";
    return reply;
  } });
  await h.run(); assert.deepEqual(h.renders[0].files, input().files);
  assert.deepEqual(h.renders[0].options.includeSources, input().options.includeSources);
});

test("native analysis refusal and decoder failure are not treated as a clean report", async t => {
  const h = fixture(t); h.decodeError = Object.assign(Error("invalid report"), { code: "INVALID_LINK_REPORT" });
  await assert.rejects(h.run(), code("INVALID_LINK_REPORT")); assert.equal(h.renders.length, 0);
  const other = fixture(t, { check() { throw Object.assign(Error("analysis budget exceeded"), { code: "BOOK_ERROR" }); } });
  await assert.rejects(other.run(), code("BOOK_ERROR")); assert.equal(other.renders.length, 0);
  assert.equal(other.freedChecks, 1);
});

for (const mismatch of [{ format: "book-pdf" }, { mimeType: "text/html" }, { sourceLength: 1 }, { sourceLength: NaN }]) {
  test(`mismatched verification envelopes fail before decoding: ${JSON.stringify(mismatch)}`, async t => {
    const h = fixture(t, { check: reply => ({ ...reply, ...mismatch }) });
    await assert.rejects(h.run(), code("INVALID_LINK_REPORT")); assert.equal(h.decoderCalls.length, 0);
    assert.equal(h.renders.length, 0); assert.equal(h.freedChecks, 1);
  });
}

test("the first renderer must match the verified source metric", async t => {
  const h = fixture(t, { render(call) { return { format: `book-${call.format}`, sourceLength: 1,
    retainedInputRevision: 1, bytes: new Uint8Array([1]) }; } });
  await assert.rejects(h.run(), code("WORKER_PROTOCOL_ERROR")); assert.equal(h.renders.length, 1);
  assert.equal(h.freedRenders, 1);
});

for (const method of ["cancel", "dispose", "signal"]) {
  test(`${method} settles a held checker and prevents late verification from rendering`, async t => {
    const pending = deferred(), abort = new AbortController(); const h = fixture(t, { check: () => pending.promise });
    const running = h.run({ strictLinks: true, signal: abort.signal });
    if (method === "signal") abort.abort(); else h.publisher[method]();
    await assert.rejects(running, code("EXPORT_CANCELLED")); assert.equal(h.freedChecks, 1); assert.equal(h.publisher.busy, false);
    assert.equal(h.checks[0].request.signal.aborted, true);
    pending.resolve({ format: "book-links", mimeType: "application/json", sourceLength: size(h.captured), bytes: new Uint8Array([1]) });
    await tick(); assert.equal(h.renders.length, 0); assert.equal(h.freedChecks, 1);
  });
}

test("late checker rejection cannot affect a newer successful publication", async t => {
  const pending = deferred(); let first = true;
  const h = fixture(t, { check(reply) { if (first) { first = false; return pending.promise; } return reply; } });
  const old = h.run(); h.publisher.cancel(); await assert.rejects(old, code("EXPORT_CANCELLED"));
  const next = await h.run(); pending.reject(Error("late old error")); await tick();
  assert.equal(next.outputs.length, 3); assert.equal(h.renders.length, 3); assert.equal(h.freedChecks, 2);
});

test("one deadline spans verification and every output without a fresh verification allowance", async t => {
  const jobs = new Map(), scheduled = [];
  t.mock.method(performance, "now", () => 0);
  t.mock.method(globalThis, "setTimeout", (fn, ms) => { const id = {}; jobs.set(id, fn); scheduled.push(ms); return id; });
  t.mock.method(globalThis, "clearTimeout", id => jobs.delete(id));
  const pending = deferred(); const h = fixture(t, { settings: { timeoutMs: 17 }, check: () => pending.promise });
  const running = h.run(); assert.deepEqual(scheduled, [17]); [...jobs.values()][0]();
  await assert.rejects(running, code("EXPORT_TIMEOUT")); assert.equal(jobs.size, 0); assert.equal(h.freedChecks, 1);
  pending.reject(Error("late timeout")); await tick(); assert.equal(h.renders.length, 0);
});

test("expired synchronous verification cannot start the native renderer", async t => {
  let now = 0; t.mock.method(performance, "now", () => now);
  const h = fixture(t, { settings: { timeoutMs: 5 }, check(reply) { now = 6; return reply; } });
  await assert.rejects(h.run(), code("EXPORT_TIMEOUT")); assert.equal(h.renders.length, 0);
});

test("cancellation during checker construction still disposes the returned client", async t => {
  const h = fixture(t, { createCheck(h) { h.publisher.cancel(); } });
  await assert.rejects(h.run(), code("EXPORT_CANCELLED")); assert.equal(h.freedChecks, 1);
  assert.equal(h.checks.length, 0); assert.equal(h.renders.length, 0);
});

test("pre-aborted strict publication does not capture or initialize", async t => {
  const h = fixture(t), abort = new AbortController(); abort.abort();
  await assert.rejects(h.run({ strictLinks: true, signal: abort.signal }), code("EXPORT_CANCELLED"));
  assert.equal(h.captures, 0); assert.equal(h.calls.length, 0);
});

test("invalid verifier receipts cannot silently authorize output", async t => {
  for (const invalid of [null, {}, { sourceLength: 2, report: { summary: { findings: 0, chapters: 1 } } },
    { sourceLength: 2, report: { ...report(), summary: { chapters: 1, findings: -1 } } }]) {
    const h = fixture(t, { client: { checkLinks: async () => invalid } });
    await assert.rejects(h.run(), code("INVALID_LINK_REPORT")); assert.equal(h.renders.length, 0);
  }
});
