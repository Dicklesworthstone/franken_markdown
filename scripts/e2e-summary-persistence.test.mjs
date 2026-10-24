// Exercise the real shell harness, real printf command, and real Python summary
// writer. No renderer is substituted or claimed. Avoid e2e_init's cleanup by
// initializing its state in a new private scratch directory without deletions.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const library = fileURLToPath(new URL("./e2e/lib.sh", import.meta.url));
const script = `
source "$HARNESS_LIBRARY"
E2E_RUN_ID=persistence
E2E_ART="$HARNESS_SCRATCH"
E2E_STEPS_DIR="$HARNESS_SCRATCH/steps"
E2E_LEDGER="$HARNESS_SCRATCH/ledger.txt"
E2E_MANIFEST="$HARNESS_SCRATCH/manifest.tsv"
E2E_STEP_NO=0; E2E_PASS=0; E2E_FAIL=0
E2E_ASSERT_PASS=0; E2E_ASSERT_FAIL=0; E2E_OPEN=0
E2E_RUN_CWD="$E2E_REPO_ROOT"
e2e_run 'real command' -- printf HELLO
e2e_expect_exit 0
e2e_expect_stdout_contains "$EXPECTED_TEXT"
e2e_finish
`;
async function run({ unwritable = false, fail = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "fmd-e2e-summary-"));
  await mkdir(join(root, "steps"));
  await writeFile(join(root, "manifest.tsv"), "");
  const summary = unwritable ? root : join(root, "summary.json");
  const result = spawnSync("/bin/bash", ["-c", script], { encoding: "utf8", timeout: 10000,
    env: { ...process.env, HARNESS_LIBRARY: library, HARNESS_SCRATCH: root,
      E2E_SUMMARY_JSON: summary, EXPECTED_TEXT: fail ? "ABSENT" : "HELLO" },
  });
  assert.equal(result.error, undefined);
  return { ...result, summary };
}

test("a passing real command persists its complete step and assertion inventory", async () => {
  const result = await run();
  assert.equal(result.status, 0, result.stderr);
  const receipt = JSON.parse(await readFile(result.summary, "utf8"));
  assert.equal(receipt.schema, "fmd-e2e-v1");
  assert.equal(receipt.run_id, "persistence");
  assert.deepEqual(receipt.totals, { steps: 1, steps_passed: 1, steps_failed: 0,
    assertions: 2, assertions_passed: 2, assertions_failed: 0 });
  assert.equal(receipt.steps[0].exit, 0);
  assert.equal(receipt.steps[0].stdout_bytes, 5);
});

test("failed summary persistence is an environment failure, never success", async () => {
  const result = await run({ unwritable: true });
  assert.equal(result.status, 66, result.stderr);
  assert.match(result.stderr, /could not persist summary.json/);
  assert.doesNotMatch(result.stdout, /e2e persistence: ok/);
});

test("a failed assertion is persisted and retains its normal failure status", async () => {
  const result = await run({ fail: true });
  assert.equal(result.status, 70, result.stderr);
  const receipt = JSON.parse(await readFile(result.summary, "utf8"));
  assert.equal(receipt.totals.steps_failed, 1);
  assert.equal(receipt.totals.assertions_failed, 1);
  assert.equal(receipt.steps[0].verdict, "fail");
});
