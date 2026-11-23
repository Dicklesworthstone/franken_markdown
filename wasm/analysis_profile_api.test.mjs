// Production facade with generated-binding doubles. These tests verify dispatch
// and request ownership; native grammar semantics have separate Rust tests.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";
import { normalizeAnalysisOptions } from "./render_typography.mjs";

const methods = ["documentStats", "accessibilityAudit", "searchIndex"];
const source = "\ufeffTerm\r\n: **Definition β**\r\n";
function gate() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
async function fixture({ profiled = true, short = false, hold, failure, report } = {}) {
  const calls = [];
  let inits = 0;
  const bindings = { default() { inits++; return hold?.promise ?? Promise.resolve(); },
    renderHtmlConfiguredAdvanced() {}, renderPdfConfiguredMulti() {},
    renderSvgConfigured() {}, renderEpubConfigured() {}, capabilities() {},
    searchIndex() {}, semanticDiff() {}, renderSemanticDiffHtml() {},
  };
  for (const method of methods) {
    const binding = name => (...args) => {
      calls.push({ name, args });
      if (failure) throw failure;
      return JSON.stringify(report === undefined ? { fixture: name, source: args[0],
        additive: { retained: ["complete native report", 7] } } : report);
    };
    bindings[method] = binding(method);
    if (profiled) {
      bindings[method + "Profiled"] = binding(method + "Profiled");
      Object.defineProperty(bindings[method + "Profiled"], "length", { value: short ? 1 : 2 });
    }
  }
  const native = new SyntheticModule(Object.keys(bindings), function() {
    for (const [name, value] of Object.entries(bindings)) this.setExport(name, value);
  });
  const helpers = new Map();
  for (const file of ["pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    helpers.set("./" + file, new SourceTextModule(await readFile(new URL(file, import.meta.url), "utf8")));
  const module = new SourceTextModule(await readFile(new URL("franken_markdown.js", import.meta.url), "utf8"));
  await module.link(name => name === "./pkg/franken_markdown.js" ? native : helpers.get(name));
  await module.evaluate();
  return { api: module.namespace, calls, get inits() { return inits; } };
}

test("analysis accepts only a captured, canonical data-valued grammar choice", () => {
  const options = { profile: "commonmark-gfm" }, captured = normalizeAnalysisOptions(options);
  options.profile = "gfm-plus";
  assert.deepEqual(captured, { profile: "commonmark-gfm" });
  assert.equal(Object.isFrozen(captured), true);
  assert.deepEqual(normalizeAnalysisOptions(Object.create(null)), {});
  assert.deepEqual(normalizeAnalysisOptions({ profile: undefined }), {});
  for (const value of [null, [], "gfm-plus", { profile: "gfm" }, { profile: null },
    { profile: { toString() { assert.fail("profile coercion"); } } },
    Object.create({ profile: "gfm-plus" }), { title: "not an analysis option" },
    { get profile() { assert.fail("profile accessor"); } },
    { get images() { assert.fail("resource accessor"); } }, { [Symbol("profile")]: "gfm-plus" }])
    assert.throws(() => normalizeAnalysisOptions(value), { code: "INVALID_OPTIONS" });
});

for (const method of methods) {
  test(method + " runs the selected native binding and retains the complete report", async () => {
    const f = await fixture();
    for (const profile of ["commonmark-gfm", "gfm-plus"]) {
      const report = await f.api[method](source, { profile });
      assert.deepEqual(f.calls.at(-1), { name: method + "Profiled", args: [source, profile] });
      assert.deepEqual(report, { fixture: method + "Profiled", source,
        additive: { retained: ["complete native report", 7] }, profile });
    }
    assert.equal(f.inits, 1);
  });

  test(method + " keeps omitted-profile argument and report shapes on older packages", async () => {
    const f = await fixture({ profiled: false });
    const expected = { fixture: method, source, additive: { retained: ["complete native report", 7] } };
    assert.deepEqual(await f.api[method](source), expected);
    assert.deepEqual(await f.api[method](source, {}), expected);
    assert.deepEqual(await f.api[method](source, { profile: undefined }), expected);
    assert.deepEqual(f.calls, Array.from({ length: 3 }, () => ({ name: method, args: [source] })));
  });

  for (const warm of [false, true]) test(method + " captures source and dialect before " + (warm ? "warm" : "cold") + " init yields", async () => {
    const hold = warm ? undefined : gate(), f = await fixture({ hold });
    if (warm) await f.api.init();
    let markdown = source, reads = 0;
    const options = { profile: "commonmark-gfm" };
    const pending = f.api[method]({ toString() { reads++; return markdown; } }, options);
    markdown = "changed"; options.profile = "gfm-plus";
    assert.equal(f.calls.length, 0);
    hold?.resolve();
    const report = await pending;
    assert.equal(reads, 1);
    assert.deepEqual(f.calls, [{ name: method + "Profiled", args: [source, "commonmark-gfm"] }]);
    assert.equal(report.profile, "commonmark-gfm");
  });

  test(method + " rejects invalid grammar and resource grants before init or coercion", async () => {
    const f = await fixture();
    const markdown = { toString() { assert.fail("source coercion"); } };
    for (const options of [{ profile: "gfm" }, { profile: true }, { profile: "gfm-plus", pdfImages: [] },
      { allowRawHtml: true }, { get profile() { assert.fail("profile accessor"); } },
      { get fontAssets() { assert.fail("font accessor"); } }])
      await assert.rejects(f.api[method](markdown, options), { code: "INVALID_OPTIONS" });
    assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  });

  test(method + " refuses stale generated bindings without fallback", async () => {
    for (const settings of [{ profiled: false }, { short: true }]) {
      const f = await fixture(settings);
      await assert.rejects(f.api[method]({ toString() { assert.fail("source coercion"); } },
        { profile: "commonmark-gfm" }), { code: "UNSUPPORTED_WASM_PACKAGE" });
      assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
    }
  });

  test(method + " preserves native failure and rejects conflicting report receipts", async () => {
    const failure = new Error("fixture native refusal"), f = await fixture({ failure });
    await assert.rejects(f.api[method](source, { profile: "gfm-plus" }), error => error === failure);
    assert.deepEqual(f.calls, [{ name: method + "Profiled", args: [source, "gfm-plus"] }]);
    for (const report of [null, [], 1, "report", { profile: "commonmark-gfm" }, { profile: null }]) {
      const malformed = await fixture({ report });
      await assert.rejects(malformed.api[method](source, { profile: "gfm-plus" }), { code: "INVALID_WASM_RESPONSE" });
      assert.equal(malformed.calls.length, 1);
    }
  });
}

test("renderer instances retain independent dialects for both analyses", async () => {
  const f = await fixture(), renderer = await f.api.createRenderer();
  const [stats, audit] = await Promise.all([
    renderer.documentStats("one", { profile: "commonmark-gfm" }),
    renderer.accessibilityAudit("two", { profile: "gfm-plus" }),
  ]);
  assert.equal(stats.profile, "commonmark-gfm"); assert.equal(audit.profile, "gfm-plus");
  assert.deepEqual(f.calls, [
    { name: "documentStatsProfiled", args: ["one", "commonmark-gfm"] },
    { name: "accessibilityAuditProfiled", args: ["two", "gfm-plus"] },
  ]);
  assert.equal(f.inits, 1);
});
