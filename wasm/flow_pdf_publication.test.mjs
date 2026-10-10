// Production facade/exporter/shared paper normalizer; native rendering is an
// explicit double. This is request/ownership proof, not a PDF layout golden.
import assert from "node:assert/strict";
import test from "node:test";
import { createFlowAdapter, FlowError } from "./flow_session.mjs";
import { normalizeFlowExport, withFlowExports } from "./flow_export.mjs";
import { normalizePdfPage, pdfPageGeometry } from "./pdf_page.mjs";
const code = value => error => error instanceof FlowError && error.code === value;
const token = { revision: "9007199254740993", layoutRevision: "9007199254740999" };
function fixture() {
  const calls = [];
  let reads = 0, hold = null;
  const raw = {
    ...token, source: "# Print 😀", free() {},
    snapshotJson(revision, layoutRevision) {
      reads++;
      return JSON.stringify({ schemaVersion: 1, revision, layoutRevision,
        offset: 0, total: 0, nextOffset: null, items: [] });
    },
  };
  const session = createFlowAdapter(raw);
  const render = async (source, options) => {
    calls.push({ source, options });
    if (hold) await hold;
    return { format: "pdf", mimeType: "application/pdf",
      bytes: new TextEncoder().encode("%PDF-native-double"), diagnostics: [] };
  };
  return { raw, calls, get reads() { return reads; },
    api: withFlowExports(session, { html() { throw new Error("wrong renderer"); }, pdf: render }),
    // Observe the gate even when a regressed exporter refuses before awaiting it.
    hold(value) { hold = value; hold?.catch(() => {}); } };
}

test("persistent PDF exports forward canonical paper and literal native running templates", async () => {
  const f = fixture();
  const options = {
    title: "Report", author: "Author", pageNumbers: true,
    page: { size: "a4", orientation: "landscape", margins: { topPt: 40, leftPt: 60 } },
    running: { header: { left: "{title}", right: "{author}", rule: true },
      footer: { center: "{page} / {pages} · {date} · {unknown}" }, skipFirstPage: true },
  };
  const before = f.api.layoutOptions;
  const result = await f.api.exportDocument("pdf", options, f.api.token);
  const captured = f.calls[0].options;
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].source, f.raw.source);
  assert.deepEqual(captured.page, {
    size: { widthPt: 297 * 72 / 25.4, heightPt: 210 * 72 / 25.4 },
    margins: { topPt: 40, rightPt: 72, bottomPt: 72, leftPt: 60 },
  });
  assert.deepEqual([...pdfPageGeometry(captured.page)], [297 * 72 / 25.4, 210 * 72 / 25.4, 40, 72, 72, 60]);
  assert.deepEqual(captured.running, options.running);
  assert.equal(captured.metadataEpochSeconds, 0);
  assert.equal(captured.title, "Report");
  assert.equal(captured.allowRawHtml, false);
  assert.equal(captured.pageNumbers, true);
  assert.deepEqual(f.api.layoutOptions, before);
  assert.equal(result.revision, token.revision);
  assert.equal(result.layoutRevision, token.layoutRevision);
});

test("print settings are deeply owned before await and stable after worker-style cloning", async () => {
  const f = fixture();
  let finish;
  f.hold(new Promise(resolve => { finish = resolve; }));
  const options = { page: { size: { widthPt: 400, heightPt: 600 }, margins: { topPt: 30 } },
    running: { header: { left: "before", rule: true }, footer: { right: "{page}" } } };
  const normalized = normalizeFlowExport("pdf", options, token);
  const pending = f.api.exportDocument(...normalized);
  options.page.size.widthPt = 900; options.page.margins.topPt = 100;
  options.running.header.left = "after"; options.running.footer.right = "after";
  const captured = f.calls[0].options;
  assert.equal(captured.page.size.widthPt, 400);
  assert.equal(captured.page.margins.topPt, 30);
  assert.equal(captured.running.header.left, "before");
  assert.equal(captured.running.footer.right, "{page}");
  assert(Object.isFrozen(normalized[1].page));
  assert(Object.isFrozen(normalized[1].page.size));
  assert(Object.isFrozen(normalized[1].page.margins));
  assert(Object.isFrozen(normalized[1].running));
  assert(Object.isFrozen(normalized[1].running.header));
  assert.deepEqual(normalizeFlowExport(...structuredClone(normalized)), normalized);
  finish();
  await pending;
});

test("custom paper order, orientation and asymmetric margins use the shared native contract", () => {
  for (const orientation of [undefined, "portrait", "landscape"]) {
    const page = { size: { widthPt: 800, heightPt: 400 }, orientation,
      margins: { topPt: 20, rightPt: 30, bottomPt: 40, leftPt: 50 } };
    const actual = normalizeFlowExport("pdf", { page }, token)[1].page;
    assert.deepEqual(actual, normalizePdfPage(page));
    assert.equal(actual.size.widthPt, orientation === "portrait" ? 400 : 800);
    assert.deepEqual(actual.margins, page.margins);
  }
  const letter = normalizeFlowExport("pdf", { page: {} }, token)[1].page;
  assert.deepEqual([...pdfPageGeometry(letter)], [612, 792, 72, 72, 72, 72]);
});

test("invalid print geometry is rejected before inventory reads or native rendering", async () => {
  const f = fixture();
  for (const page of [null, [], 1, { size: "legal" }, { orientation: "sideways" },
    { size: { widthPt: 143, heightPt: 600 } }, { size: { widthPt: Infinity, heightPt: 600 } },
    { margins: -1 }, { margins: 1000 }, { margins: { topPt: "72" } },
    { widthPt: 300 }, { size: { widthPt: 144, heightPt: 144 }, margins: 40 }]) {
    await assert.rejects(f.api.exportDocument("pdf", { page }, token), code("INVALID_OPTIONS"));
  }
  assert.equal(f.reads, 0);
  assert.equal(f.calls.length, 0);
});

test("nested print accessors, inherited settings and unknown keys cannot cross the boundary", async () => {
  const f = fixture();
  let invoked = 0;
  const accessor = Object.defineProperty({}, "left", { get() { invoked++; return "bad"; } });
  const pageAccessor = Object.defineProperty({}, "size", { get() { invoked++; return "a4"; } });
  const hidden = Object.defineProperty({}, "url", { value: "https://example.test" });
  for (const options of [
    { running: { header: accessor } }, { page: pageAccessor },
    { running: { footer: hidden } }, { running: { [Symbol("foreign")]: true } },
    { running: Object.create({ header: { left: "inherited" } }) },
    { running: { header: { center: () => "executable" } } },
  ]) await assert.rejects(f.api.exportDocument("pdf", options, token), code("INVALID_OPTIONS"));
  assert.equal(invoked, 0);
  assert.equal(f.calls.length, 0);
  assert.equal(f.reads, 0);
  const plain = Object.assign(Object.create(null), { header: { left: "owned" } });
  await f.api.exportDocument("pdf", { running: plain }, token);
  assert.equal(f.calls[0].options.running.header.left, "owned");
});

test("running templates validate scalar text, per-slot and aggregate budgets", async () => {
  const f = fixture();
  for (const running of [null, [], { header: "text" }, { header: { left: null } },
    { footer: { right: 42 } }, { header: { rule: 1 } }, { skipFirstPage: "true" },
    { header: { left: "x".repeat(4097) } }]) {
    await assert.rejects(f.api.exportDocument("pdf", { running }, token), code("INVALID_OPTIONS"));
  }
  await assert.rejects(f.api.exportDocument("pdf", { running: { header: { left: "\ud800" } } }, token),
    code("INVALID_UNICODE"));
  const full = "x".repeat(4096);
  await assert.rejects(f.api.exportDocument("pdf", { running: {
    header: { left: full, center: full, right: full }, footer: { left: full, right: "x" },
  } }, token), code("BUDGET_EXCEEDED"));
  assert.equal(f.calls.length, 0);
  await f.api.exportDocument("pdf", { running: {
    header: { left: full, center: full, right: full }, footer: { left: full },
  } }, token);
  assert.equal(f.calls.length, 1);
});

test("non-ASCII running slots reject above the native UTF-8 limit before reading the session", async () => {
  const f = fixture();
  const boundaries = ["中".repeat(1365) + "x", "😀".repeat(1024)];
  for (const exact of boundaries) {
    assert.equal(new TextEncoder().encode(exact).length, 4096);
    await assert.rejects(f.api.exportDocument("pdf", {
      running: { header: { left: exact + "x" } },
    }, token), code("INVALID_OPTIONS"));
  }
  assert.equal(f.calls.length, 0);
  assert.equal(f.reads, 0);
  for (const exact of boundaries) {
    await f.api.exportDocument("pdf", { running: { header: { left: exact } } }, token);
    assert.equal(f.calls.at(-1).options.running.header.left, exact);
  }
});

test("skip-first never silently disappears when no running band would draw", async () => {
  const f = fixture();
  for (const running of [{ skipFirstPage: true }, { header: {}, skipFirstPage: true },
    { footer: { center: "", rule: false }, skipFirstPage: true }]) {
    await assert.rejects(f.api.exportDocument("pdf", { running }, token), code("INVALID_OPTIONS"));
  }
  assert.equal(f.calls.length, 0);
  await f.api.exportDocument("pdf", { running: { header: { rule: true }, skipFirstPage: true } }, token);
  assert.equal(f.calls[0].options.running.skipFirstPage, true);
});

test("ordinary PDF exports retain absent geometry and native defaults", async () => {
  const f = fixture();
  await f.api.exportDocument("pdf", {}, token);
  assert.equal(Object.hasOwn(f.calls[0].options, "page"), false);
  assert.equal(Object.hasOwn(f.calls[0].options, "running"), false);
  assert.equal(f.calls[0].options.metadataEpochSeconds, 0);
  await f.api.exportDocument("pdf", { running: { header: { left: "" }, skipFirstPage: false } }, token);
  assert.equal(f.calls[1].options.running.header.left, "");
  assert.equal(f.calls[1].options.running.skipFirstPage, false);
});

test("configured PDF preserves native failures, releases busy state and refuses stale output", async () => {
  const f = fixture();
  let fail;
  f.hold(new Promise((_, reject) => { fail = reject; }));
  const options = { page: { size: "a4" }, running: { header: { left: "{title}" } } };
  const pending = f.api.exportDocument("pdf", options, token);
  fail(Object.assign(new Error("configured native ABI missing"), { code: "UNSUPPORTED_WASM_PACKAGE" }));
  await assert.rejects(pending, code("UNSUPPORTED_WASM_PACKAGE"));
  f.hold(null);
  await f.api.exportDocument("pdf", options, token);
  let finish;
  f.hold(new Promise(resolve => { finish = resolve; }));
  const stale = f.api.exportDocument("pdf", options, token);
  f.raw.layoutRevision = "9007199254741000";
  finish();
  await assert.rejects(stale, code("STALE_LAYOUT"));
  assert.equal(f.api.disposed, false);
});

test("paper and running options are refused on HTML, EPUB and SVG", () => {
  for (const format of ["html", "epub", "svg"]) {
    for (const options of [{ page: { size: "a4" } }, { running: { header: { left: "x" } } }]) {
      assert.throws(() => normalizeFlowExport(format, options, token), code("INVALID_OPTIONS"));
    }
  }
});
