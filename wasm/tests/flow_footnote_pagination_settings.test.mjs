// Production settings, session adapter and export admission. The native session
// and PDF renderer are explicit doubles; emitted-page guarantees are exercised
// by tests/pdf_footnote_continuation_test.rs, not by these adapter checks.
import assert from "node:assert/strict";
import test from "node:test";
import { normalizeRenderSettings } from "../demo/flow_render_settings.mjs";
import { withFlowExports } from "../flow_export.mjs";
import { createFlowAdapter } from "../flow_session.mjs";

const source = "# Guide\n\nBody with a long note[^note].\n\n[^note]: Note prose.\n";

function publishingFixture() {
  const calls = [];
  let failure = null;
  const raw = {
    revision: "9007199254740993",
    layoutRevision: "9007199254740997",
    source,
    free() {},
    snapshotJson(revision, layoutRevision, offset) {
      return JSON.stringify({
        schemaVersion: 1, revision, layoutRevision, offset,
        total: 0, items: [], nextOffset: null,
      });
    },
  };
  const session = withFlowExports(createFlowAdapter(raw), {
    html() { throw new Error("PDF publication must not use the HTML renderer"); },
    async pdf(markdown, options) {
      calls.push({ markdown, options });
      if (failure) throw failure;
      return {
        format: "pdf", mimeType: "application/pdf",
        bytes: new TextEncoder().encode("%PDF-native-double"), diagnostics: [],
      };
    },
  }, "serif");
  return { session, calls, failWith(value) { failure = value; } };
}

for (const [name, policy] of [
  ["optimal", { optimalPagination: true }],
  ["strict", { strictPagination: true }],
  ["optimal and strict", { optimalPagination: true, strictPagination: true }],
]) {
  for (const baselineGridPt of [null, 12]) {
    test(`continued footnotes retain ${name} publication through settings recovery${baselineGridPt ? " with a grid" : ""}`, async (t) => {
      const settings = normalizeRenderSettings({
        font: "serif", footnoteBacklinks: true, splitFootnotes: true,
        ...policy, baselineGridPt,
      });
      const restored = normalizeRenderSettings(JSON.parse(JSON.stringify(settings.values)));
      assert.deepEqual(restored, settings);
      const defaults = normalizeRenderSettings({ font: "serif" });
      for (const format of ["preview", "html", "epub", "svg"])
        assert.deepEqual(restored[format], defaults[format]);

      const f = publishingFixture(), token = f.session.token;
      t.after(() => f.session.dispose());
      const output = await f.session.exportDocument("pdf", restored.pdf, token);
      assert.equal(f.calls.length, 1);
      const call = f.calls[0];
      assert.equal(call.markdown, source);
      assert.equal(call.options.splitFootnotes, true);
      assert.equal(call.options.footnoteBacklinks, true);
      assert.equal(call.options.optimalPagination, policy.optimalPagination);
      assert.equal(call.options.strictPagination, policy.strictPagination);
      assert.equal(call.options.baselineGridPt, baselineGridPt ?? undefined);
      assert.equal(call.options.font, "serif");
      assert.equal(call.options.allowRawHtml, false);
      assert.deepEqual(call.options.pdfImages, []);
      assert.equal(output.format, "pdf");
      assert.equal(output.mimeType, "application/pdf");
      assert.equal(output.sourceLengthBytes, new TextEncoder().encode(source).length);
      assert.deepEqual({ revision: output.revision, layoutRevision: output.layoutRevision }, token);
      assert.deepEqual(f.session.token, token);
      assert.equal(f.session.source, source);
    });
  }
}

test("strict continued footnotes preserve native pagination refusal and release publication ownership", async (t) => {
  const settings = normalizeRenderSettings({
    font: "serif", splitFootnotes: true, strictPagination: true, baselineGridPt: 12,
  });
  const f = publishingFixture(), token = f.session.token;
  t.after(() => f.session.dispose());
  f.failWith(JSON.stringify({
    code: "PAGINATION_INFEASIBLE", reason: "infeasible",
    message: "exact continuation plan cannot honor the body keep",
  }));
  await assert.rejects(f.session.exportDocument("pdf", settings.pdf, token), (error) => {
    assert.equal(error.code, "PAGINATION_INFEASIBLE");
    assert.equal(error.message, "exact continuation plan cannot honor the body keep");
    assert.equal(error.cause.reason, "infeasible");
    return true;
  });
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].options.splitFootnotes, true);
  assert.equal(f.calls[0].options.strictPagination, true);
  assert.equal(f.calls[0].options.baselineGridPt, 12);
  assert.deepEqual(f.session.token, token);
  assert.equal(f.session.source, source);
  assert.equal(f.session.disposed, false);

  f.failWith(null);
  assert.equal((await f.session.exportDocument("pdf", settings.pdf, token)).format, "pdf");
  assert.equal(f.calls.length, 2);
  assert.deepEqual(f.calls[1].options, f.calls[0].options);
  assert.equal(settings.pdf.strictPagination, true);
});
