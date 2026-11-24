import assert from "node:assert/strict";
import test from "node:test";
import { normalizeFlowPdfVerification, encodeFlowPdfVerification } from "./flow_export.mjs";
import { createVerificationFixture } from "./tests/flow_pdf_verification_fixture.mjs";

const code = expected => error => error.code === expected;
const gate = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

for (const profile of [undefined, "commonmark-gfm", "gfm-plus"]) {
  test("Flow PDF verification retains complete report, assets and " + (profile ?? "legacy") + " dialect", async () => {
    const f = createVerificationFixture({}, undefined, { profile }), token = f.api.token;
    const options = { title: "Configured", page: { size: "a4", margins: { topPt: 48 } },
      typography: { homogeneous: true, lineHeight: 1.4 }, strictPagination: true,
      footnoteBacklinks: true, running: { footer: { right: "{page} / {pages}" } } };
    const result = await f.api.verifyPdf(options, token);
    await f.api.exportDocument("pdf", options, token);
    assert.deepEqual(f.calls[0].options, f.calls[1].options);
    assert.equal(result.format, "pdf-verification"); assert.equal(result.font, "serif");
    assert.deepEqual({ revision: result.revision, layoutRevision: result.layoutRevision }, token);
    assert.equal(result.profile, profile); assert.equal(result.report.profile, profile);
    assert.equal(result.sourceLengthBytes, new TextEncoder().encode(f.state.source).length);
    assert.equal(result.report.pages[0].runs[0].text, f.state.source);
    assert.deepEqual(result.report.anchors, { resolved: 1, unresolved: ["missing-anchor"] });
    assert.equal(result.report.digest, "fnv1a64:0123456789abcdef");
    assert.equal(result.assetCount, 1); assert.equal(result.assetBytes, 3);
    assert.deepEqual(result.report.probe.images, [["offscreen.png", [1, 2, 3]]]);
    assert.equal(f.calls[0].options.allowRawHtml, false);
    assert.equal(f.api.supportsPdfVerification, true); assert(Object.isFrozen(result));
    f.state.report.findings[0].detail = "backend reused storage";
    assert.equal(result.report.findings[0].detail, "native detail \n \"é中😀\"");
    assert.deepEqual(f.api.token, token);
  });
}

test("verification captures print controls and exact logo bytes before asynchronous work", async () => {
  const f = createVerificationFixture(), hold = gate(); f.state.hold = hold.promise;
  const bytes = Uint8Array.of(99, 4, 5, 88), options = {
    page: { size: { widthPt: 400, heightPt: 600 }, margins: { leftPt: 40 } },
    typography: { pareto: true }, running: { header: { image: { dest: "logo.png", position: "left", heightPt: 20 } } },
    pdfImages: [{ destination: "logo.png", bytes: bytes.subarray(1, 3) }],
  };
  const pending = f.api.verifyPdf(options, f.api.token);
  options.page.size.widthPt = 900; options.typography.pareto = false;
  options.running.header.image.heightPt = 30; options.pdfImages[0].destination = "later.png"; bytes.fill(9);
  hold.resolve(); const result = await pending;
  assert.equal(result.report.probe.page.size.widthPt, 400);
  assert.equal(result.report.probe.typography.pareto, true);
  assert.equal(result.report.probe.running.header.image.heightPt, 20);
  assert.deepEqual(result.report.probe.images, [["offscreen.png", [1, 2, 3]], ["logo.png", [4, 5]]]);
  assert.equal(bytes.byteLength, 4); assert.equal(result.assetBytes, 5);
});

for (const mutation of ["source", "layout", "dispose"]) {
  test("verification refuses " + mutation + " changes before publishing a report", async () => {
    const f = createVerificationFixture(), hold = gate(); f.state.hold = hold.promise;
    const pending = f.api.verifyPdf({}, f.api.token);
    if (mutation === "source") f.session.replaceSource("new", { expectedRevision: f.state.revision });
    else if (mutation === "layout") f.state.layoutRevision = String(BigInt(f.state.layoutRevision) + 1n);
    else f.api.dispose();
    const rejected = assert.rejects(pending, code({ source: "STALE_REVISION", layout: "STALE_LAYOUT", dispose: "SESSION_DISPOSED" }[mutation]));
    hold.resolve(); await rejected;
  });
}

test("verification and exports share one physical slot, and native failures release it", async () => {
  const f = createVerificationFixture(), hold = gate(); f.state.hold = hold.promise;
  const pending = f.api.verifyPdf({}, f.api.token);
  await assert.rejects(f.api.verifyPdf({}, f.api.token), code("EXPORT_BUSY"));
  await assert.rejects(f.api.exportDocument("pdf", {}, f.api.token), code("EXPORT_BUSY"));
  hold.resolve(); await pending; f.state.hold = null;
  f.state.failure = JSON.stringify({ code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "exact plan refused" });
  await assert.rejects(f.api.verifyPdf({ strictPagination: true }, f.api.token),
    error => error.code === "PAGINATION_INFEASIBLE" && error.message === "exact plan refused");
  f.state.failure = null;
  assert.equal((await f.api.exportDocument("pdf", {}, f.api.token)).format, "pdf");
  const again = gate(); f.state.hold = again.promise;
  const exported = f.api.exportDocument("pdf", {}, f.api.token);
  await assert.rejects(f.api.verifyPdf({}, f.api.token), code("EXPORT_BUSY"));
  again.resolve(); await exported;
});

test("verification admits options before source work and forbids policy or grammar replacement", async () => {
  const f = createVerificationFixture(); let getters = 0;
  for (const options of [
    { pdfA: "off" }, { pdfA: "2b" }, { pdfAStrict: false }, { profile: "gfm-plus" }, { font: "sans" },
    { allowRawHtml: true }, { customCss: "" }, { fontAssets: [] }, { maxOutputBytes: 0 },
    { maxOutputBytes: 8 * 1024 * 1024 + 1 }, { strictPagination: "true" },
    Object.defineProperty({}, "title", { get() { getters++; return "unexpected"; } }),
  ]) await assert.rejects(f.api.verifyPdf(options, f.api.token), code("INVALID_OPTIONS"));
  assert.equal(getters, 0); assert.equal(f.calls.length, 0); assert.equal(f.state.inventoryReads, 0);
  const normalized = normalizeFlowPdfVerification({}, f.api.token);
  assert.equal(normalized[0].maxOutputBytes, 8 * 1024 * 1024);
});

test("unresolved or conflicting assets refuse verification instead of auditing a different document", async () => {
  const unresolved = createVerificationFixture({ images: [{ requestId: "1", destination: "offscreen.png", isResolved: false }] });
  await assert.rejects(unresolved.api.verifyPdf({}, unresolved.api.token), code("UNRESOLVED_EXPORT_ASSET"));
  assert.equal(unresolved.calls.length, 0);
  const f = createVerificationFixture();
  await assert.rejects(f.api.verifyPdf({ pdfImages: [{ destination: "offscreen.png", bytes: Uint8Array.of(9) }] },
    f.api.token), code("AMBIGUOUS_EXPORT_ASSET"));
  assert.equal(f.calls.length, 0);
});

test("complete Unicode report budgets are exact and refusals preserve later verification", async () => {
  const f = createVerificationFixture(), first = await f.api.verifyPdf({}, f.api.token);
  const size = new TextEncoder().encode(JSON.stringify(first.report)).length;
  assert.equal((await f.api.verifyPdf({ maxOutputBytes: size }, f.api.token)).report.digest, first.report.digest);
  await assert.rejects(f.api.verifyPdf({ maxOutputBytes: size - 1 }, f.api.token), code("BUDGET_EXCEEDED"));
  assert.equal((await f.api.verifyPdf({}, f.api.token)).report.verdict, "findings");
  let reads = 0;
  const report = { ...first.report, extra: Object.defineProperty({}, "hidden", {
    enumerable: true, get() { reads++; return "must not execute"; },
  }) };
  assert.throws(() => encodeFlowPdfVerification({ ...first, report }, f.api.token), code("INVALID_WASM_RESPONSE"));
  assert.equal(reads, 0);
  assert.throws(() => encodeFlowPdfVerification({ ...first, report: { ...first.report, extra: "x".repeat(8 * 1024 * 1024) } },
    f.api.token), code("BUDGET_EXCEEDED"));
});

test("malformed reports and missing dialect receipts fail explicitly", async () => {
  const malformed = createVerificationFixture({ reportFault: "page-count" });
  await assert.rejects(malformed.api.verifyPdf({}, malformed.api.token), code("INVALID_WASM_RESPONSE"));
  const profiled = createVerificationFixture({ reportFault: "profile" }, undefined, { profile: "gfm-plus" });
  await assert.rejects(profiled.api.verifyPdf({}, profiled.api.token), code("UNSUPPORTED_WASM_PACKAGE"));
  const legacy = createVerificationFixture({ legacy: true });
  assert.equal(legacy.api.supportsPdfVerification, false);
  await assert.rejects(legacy.api.verifyPdf({}, legacy.api.token), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(legacy.state.inventoryReads, 0);
  assert.equal((await legacy.api.exportDocument("pdf", {}, legacy.api.token)).format, "pdf");
});

test("report admission refuses backend accessors and custom iterators before executing them", async () => {
  const f = createVerificationFixture(), first = await f.api.verifyPdf({}, f.api.token);
  let reads = 0;
  const runs = [null];
  Object.defineProperty(runs, "0", { enumerable: true, get() { reads++; return first.report.pages[0].runs[0]; } });
  const withAccessor = { ...first.report, pages: [{ number: 1, runs }] };
  assert.throws(() => encodeFlowPdfVerification({ ...first, report: withAccessor }, f.api.token), code("INVALID_WASM_RESPONSE"));
  const findings = [...first.report.findings];
  Object.setPrototypeOf(findings, {
    [Symbol.iterator]() { reads++; return [][Symbol.iterator](); },
  });
  assert.throws(() => encodeFlowPdfVerification({ ...first, report: { ...first.report, findings } }, f.api.token),
    code("INVALID_WASM_RESPONSE"));
  const withReportAccessor = Object.defineProperty({ ...first }, "report", {
    get() { reads++; return first.report; },
  });
  assert.throws(() => encodeFlowPdfVerification(withReportAccessor, f.api.token), code("INVALID_WASM_RESPONSE"));
  assert.equal(reads, 0);
});

test("worker snapshot normalization is idempotent without introducing archival fields", () => {
  const f = createVerificationFixture(), options = {
    page: { size: "a4" }, running: { footer: { right: "{page}" } },
    pdfImages: [{ destination: "logo.png", bytes: Uint8Array.of(4, 5) }],
  };
  const first = normalizeFlowPdfVerification(options, f.api.token, false);
  const second = normalizeFlowPdfVerification(first[0], first[1]);
  assert.deepEqual(second, first);
  assert.equal(Object.hasOwn(second[0], "pdfA"), false);
  assert.equal(Object.hasOwn(second[0], "pdfAStrict"), false);
  assert.notEqual(second[0].pdfImages[0].bytes.buffer, options.pdfImages[0].bytes.buffer);
});
