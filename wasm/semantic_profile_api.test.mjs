// The complete production facade runs with explicit generated-binding doubles.
// Grammar semantics are covered by the native analysis_profiles tests; this
// suite proves native dispatch, request ownership and stale-package refusal.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { SourceTextModule, SyntheticModule } from "node:vm";

const methods = [["semanticDiff", "semanticDiff"], ["renderSemanticDiff", "renderSemanticDiffHtml"]];
const before = "\ufeff[Term\r\n: café](/before)\r\n";
const after = "\ufeff[Term\r\n: café](/after)\r\n";
function gate() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
async function fixture({ profiled = true, short = false, hold, failure, report } = {}) {
  const calls = [];
  let inits = 0, frees = 0;
  const bindings = { default() { inits++; return hold?.promise ?? Promise.resolve(); },
    renderHtmlConfiguredAdvanced() {}, renderPdfConfiguredMulti() {},
    renderSvgConfigured() {}, renderEpubConfigured() {}, capabilities() {},
    accessibilityAudit() {}, documentStats() {}, searchIndex() {},
  };
  for (const [, method] of methods) {
    const binding = name => (...args) => {
      calls.push({ name, args });
      if (failure) throw failure;
      const value = { fixture: name, before: args[0], after: args[1],
        old_name: args[2], new_name: args[3], additive: { retained: ["native report", 7] } };
      if (method === "semanticDiff") return JSON.stringify(report === undefined ? value : report);
      return { format: "diff-html", mimeType: "text/html; charset=utf-8", extension: "html",
        sourceLength: new TextEncoder().encode(args[0] + args[1]).length,
        bytes: new TextEncoder().encode(JSON.stringify(value)), diagnosticsJson() { return "[]"; },
        free() { frees++; } };
    };
    bindings[method] = binding(method);
    if (profiled) {
      bindings[method + "Profiled"] = binding(method + "Profiled");
      Object.defineProperty(bindings[method + "Profiled"], "length", { value: short ? 4 : 5 });
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
  return { api: module.namespace, calls, get inits() { return inits; }, get frees() { return frees; } };
}

for (const [method, native] of methods) {
  test(method + " compares both revisions through the selected native dialect", async () => {
    const f = await fixture(), api = await f.api.createRenderer();
    for (const profile of ["commonmark-gfm", "gfm-plus"]) {
      const result = await api[method](before, after, { oldName: "  Before Ω  ", newName: " Current <&> ", profile });
      assert.deepEqual(f.calls.at(-1), { name: native + "Profiled",
        args: [before, after, "  Before Ω  ", " Current <&> ", profile] });
      assert.equal(result.profile, profile);
      const report = method === "semanticDiff" ? result : JSON.parse(result.text());
      assert.equal(report.fixture, native + "Profiled");
      assert.equal(report.before, before); assert.equal(report.after, after);
      assert.deepEqual(report.additive, { retained: ["native report", 7] });
      if (method === "renderSemanticDiff") {
        assert.equal(result.format, "diff-html");
        assert.equal(result.sourceLength, Buffer.byteLength(before + after));
        assert.equal(result.filename("review"), "review.html");
        assert.equal(Object.isFrozen(result), true);
      }
    }
    assert.equal(f.inits, 1);
    assert.equal(f.frees, method === "renderSemanticDiff" ? 2 : 0);
  });

  test(method + " preserves omitted-profile ABI and output on legacy packages", async () => {
    const f = await fixture({ profiled: false });
    for (const options of [undefined, {}, { profile: undefined }, { oldName: "", newName: null }]) {
      const result = await f.api[method](before, after, options);
      assert.deepEqual(f.calls.at(-1), { name: native, args: [before, after, undefined, undefined] });
      assert.equal(Object.hasOwn(result, "profile"), false);
    }
    const result = await f.api[method](before, after, { oldName: " Custom Ω ", newName: "After" });
    assert.equal(Object.hasOwn(result, "profile"), false);
    assert.deepEqual(f.calls.at(-1), { name: native, args: [before, after, " Custom Ω ", "After"] });
  });

  for (const warm of [false, true]) test(method + " captures sources, labels and profile before " + (warm ? "warm" : "cold") + " init yields", async () => {
    const hold = warm ? undefined : gate(), f = await fixture({ hold });
    if (warm) await f.api.init();
    let left = before, right = after, oldName = " Original ", reads = 0;
    const options = { oldName: { toString() { return oldName; } }, newName: "Current", profile: "commonmark-gfm" };
    const pending = f.api[method]({ toString() { reads++; return left; } },
      { toString() { reads++; return right; } }, options);
    left = right = oldName = "changed"; options.newName = "changed"; options.profile = "gfm-plus";
    assert.equal(f.calls.length, 0);
    hold?.resolve();
    assert.equal((await pending).profile, "commonmark-gfm");
    assert.equal(reads, 2);
    assert.deepEqual(f.calls, [{ name: native + "Profiled", args: [before, after, " Original ", "Current", "commonmark-gfm"] }]);
  });

  test(method + " rejects invalid or accessor-valued profiles before native setup", async () => {
    const f = await fixture(), source = { toString() { assert.fail("source coercion"); } };
    for (const options of [{ profile: "gfm" }, { profile: null }, { profile: true },
      { profile: { toString() { assert.fail("profile coercion"); } } },
      { get profile() { assert.fail("profile accessor"); } }, Object.create({ profile: "gfm-plus" })])
      await assert.rejects(f.api[method](source, source, options), { code: "INVALID_OPTIONS" });
    assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
  });

  test(method + " refuses missing or short profiled bindings without legacy fallback", async () => {
    for (const settings of [{ profiled: false }, { short: true }]) {
      const f = await fixture(settings), source = { toString() { assert.fail("source coercion"); } };
      await assert.rejects(f.api[method](source, source, { profile: "gfm-plus" }), { code: "UNSUPPORTED_WASM_PACKAGE" });
      assert.equal(f.inits, 0); assert.equal(f.calls.length, 0);
    }
  });

  test(method + " retains a native refusal without retrying a different dialect", async () => {
    const failure = new Error("native semantic failure"), f = await fixture({ failure });
    await assert.rejects(f.api[method](before, after, { profile: "commonmark-gfm" }), error => error === failure);
    assert.deepEqual(f.calls, [{ name: native + "Profiled", args: [before, after, undefined, undefined, "commonmark-gfm"] }]);
  });
}

test("semantic diff refuses malformed or conflicting native profile reports", async () => {
  for (const report of [null, [], true, "report", { profile: "gfm-plus" }, { profile: null }]) {
    const f = await fixture({ report });
    await assert.rejects(f.api.semanticDiff(before, after, { profile: "commonmark-gfm" }), { code: "INVALID_WASM_RESPONSE" });
    assert.equal(f.calls.length, 1);
  }
});
