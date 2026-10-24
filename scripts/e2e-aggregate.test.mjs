// Execute the actual embedded Python aggregator on explicit receipt fixtures.
// This validates evidence admission, not the CLI/WASM capability in a receipt.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const source = await readFile(new URL("./e2e/run-all.sh", import.meta.url), "utf8");
const program = source.match(/python3[^\n]*aggregate\.json[^\n]*<<'PY'\n([\s\S]*?)\nPY/)[1];
const summary = () => ({
  schema: "fmd-e2e-v1", run_id: "review-render",
  totals: { steps: 1, steps_passed: 1, steps_failed: 0,
    assertions: 1, assertions_passed: 1, assertions_failed: 0 },
  // A negative CLI scenario may correctly expect a nonzero command exit.
  steps: [{ step: 1, exit: 2, verdict: "pass", assertions: [{ verdict: "pass", desc: "expected usage error" }] }],
});
async function run(receipt, { batch = "ok", empty = false, duplicate = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "fmd-e2e-aggregate-"));
  const path = join(root, "summary.json"), output = join(root, "aggregate.json");
  if (receipt !== null) await writeFile(path, typeof receipt === "string" ? receipt : JSON.stringify(receipt));
  const pairs = empty ? [] : [`render=${path}`];
  if (duplicate) pairs.push(`render=${path}`);
  const result = spawnSync("python3", ["-c", program, output, "review", batch, root, ...pairs], {
    encoding: "utf8", timeout: 10000,
  });
  assert.equal(result.error, undefined);
  return { ...result, report: JSON.parse(await readFile(output, "utf8")) };
}

test("complete current evidence admits an intentionally expected command error", async () => {
  const result = await run(summary());
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.ok, true);
  assert.deepEqual(result.report.errors, []);
  assert.equal(result.report.totals.steps, 1);
  assert.equal(result.report.totals.assertions_failed, 0);
});

test("explicitly selected batch omission remains labeled skipped", async () => {
  const result = await run(summary(), { batch: "skipped" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.batch_throughput, "skipped");
});

const cases = [
  ["missing receipt", () => null],
  ["malformed JSON", () => "{"],
  ["wrong schema", d => ({ ...d, schema: "unknown" })],
  ["stale or foreign run", d => ({ ...d, run_id: "old-render" })],
  ["zero executed steps", d => ({ ...d, steps: [], totals: Object.fromEntries(Object.keys(d.totals).map(k => [k, 0])) })],
  ["invented totals", d => { d.totals.steps = 2; return d; }],
  ["Boolean counter", d => { d.totals.steps = true; return d; }],
  ["omitted assertions", d => { d.steps[0].assertions = []; return d; }],
  ["noncontiguous step", d => { d.steps[0].step = 2; return d; }],
  ["unknown verdict", d => { d.steps[0].assertions[0].verdict = "skipped"; return d; }],
  ["hidden assertion failure", d => { d.steps[0].assertions[0].verdict = "fail"; return d; }],
];
for (const [label, change] of cases) {
  test(`${label} cannot produce passing evidence`, async () => {
    const result = await run(change(summary()));
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.report.ok, false);
    assert(result.report.errors.length > 0);
    assert.match(result.stderr, /invalid\/failed evidence/);
  });
}

test("an honestly reported failed suite retains its counts and fails the aggregate", async () => {
  const d = summary();
  d.steps[0].verdict = d.steps[0].assertions[0].verdict = "fail";
  d.totals.steps_passed = d.totals.assertions_passed = 0;
  d.totals.steps_failed = d.totals.assertions_failed = 1;
  const result = await run(d);
  assert.equal(result.status, 1);
  assert.equal(result.report.totals.steps, 1);
  assert.equal(result.report.totals.assertions_failed, 1);
});

for (const [label, options] of [
  ["empty suite selection", { empty: true }],
  ["duplicate suite", { duplicate: true }],
  ["failed batch check", { batch: "FAILED" }],
]) {
  test(`${label} cannot produce passing evidence`, async () => {
    const result = await run(summary(), options);
    assert.equal(result.status, 1);
    assert.equal(result.report.ok, false);
  });
}

test("shell forwards every selected summary and propagates aggregation failure", () => {
  assert.match(source, /SUMMARIES\+=\("\$s=\$summ"\)/);
  assert.doesNotMatch(source, /\[ -f "\$summ" \] && SUMMARIES/);
  assert.match(source, /if ! python3[^\n]*aggregate\.json/);
  assert.match(source, /\nPY\nthen\n\s*fail=1\nfi/);
});
