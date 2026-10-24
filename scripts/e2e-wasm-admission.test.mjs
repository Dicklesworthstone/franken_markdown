// Execute the actual suite's shell admission and dispatch. Harness/build/check
// endpoints are explicit control-flow doubles: these tests do not render WASM,
// compile Rust, or establish package parity. They must never become its receipt.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const suite = new URL("./e2e/wasm-browser.sh", import.meta.url);
const harness = `
E2E_REPO_ROOT="$ADMISSION_ROOT"
source "$ADMISSION_ROOT/scripts/validate-run-id.sh"
ADMISSION_FAILED=0
e2e_init() { fmd_validate_run_id e2e "$1"; printf 'init\\n' >> "$ADMISSION_TRACE"; E2E_ART="$ADMISSION_ROOT/artifacts"; }
e2e_build_bin() { printf 'build\\n' >> "$ADMISSION_TRACE"; }
e2e_run() {
  printf '%s\\n' "$1" >> "$ADMISSION_TRACE"
  shift 2
  E2E_LAST_OUT="$("$@")"; E2E_LAST_EXIT=$?
  printf '%s\\n' "$E2E_LAST_OUT"
}
e2e_expect_exit() { [ "$E2E_LAST_EXIT" -eq "$1" ] || ADMISSION_FAILED=1; }
e2e_expect_stdout_contains() { [[ "$E2E_LAST_OUT" == *"$1"* ]] || ADMISSION_FAILED=1; }
e2e_finish() { [ "$ADMISSION_FAILED" -eq 0 ]; }
`;

async function run({ tools = ["node", "wasm-bindgen"], coreExit = 0,
  packageExit = 0, packageMarker = true, runId = "admission" } = {}) {
  const root = await mkdtemp(join(tmpdir(), "fmd-wasm-admission-"));
  await mkdir(join(root, "scripts/e2e"), { recursive: true });
  await mkdir(join(root, "bin"));
  await copyFile(suite, join(root, "scripts/e2e/wasm-browser.sh"));
  await copyFile(new URL("./validate-run-id.sh", import.meta.url), join(root, "scripts/validate-run-id.sh"));
  await writeFile(join(root, "scripts/e2e/lib.sh"), harness);
  // Minimal PATH controls command discovery without hiding a real tool later in
  // the host PATH. The sentinels are never used as Node or wasm-bindgen runtimes.
  for (const name of tools) await symlink("/usr/bin/true", join(root, "bin", name));
  await symlink("/usr/bin/dirname", join(root, "bin/dirname"));
  await symlink("/bin/mkdir", join(root, "bin/mkdir"));
  for (const [name, marker, code] of [
    ["check-wasm-core.sh", "fmd wasm-core check: ok", coreExit],
    ["check-wasm-package.sh", packageMarker ? "wasm-package gate: ok" : "unfinished", packageExit],
  ]) {
    await writeFile(join(root, "scripts", name),
      `#!/bin/bash\nprintf '%s\\n' '${marker}'\nexit ${code}\n`, { mode: 0o755 });
  }
  const trace = join(root, "trace.txt");
  const result = spawnSync("/bin/bash", [join(root, "scripts/e2e/wasm-browser.sh"), runId], {
    encoding: "utf8", timeout: 10000,
    env: { ...process.env, PATH: join(root, "bin"), ADMISSION_ROOT: root, ADMISSION_TRACE: trace },
  });
  assert.equal(result.error, undefined);
  let calls = [];
  try { calls = (await readFile(trace, "utf8")).trim().split("\n"); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  return { ...result, calls };
}

for (const runId of ["../escape", ".", "bad id", "bad;id"]) {
  test(`unsafe run ID ${JSON.stringify(runId)} keeps usage precedence with missing tools`, async () => {
    const result = await run({ tools: [], runId });
    assert.equal(result.status, 64, result.stderr);
    assert.match(result.stderr, /run-id must match/);
    assert.deepEqual(result.calls, []);
  });
}

for (const tools of [[], ["node"], ["wasm-bindgen"]]) {
  test(`missing WASM tools ${JSON.stringify(tools)} refuse before artifacts or build`, async () => {
    const result = await run({ tools });
    assert.equal(result.status, 66, result.stderr);
    assert.match(result.stderr, /WASM package parity was not executed/);
    for (const missing of ["node", "wasm-bindgen"].filter(name => !tools.includes(name)))
      assert(result.stderr.includes(missing), result.stderr);
    assert.deepEqual(result.calls, [], "admission must precede artifact initialization and compilation");
    assert.doesNotMatch(result.stdout, /skip|gate: ok/);
  });
}

test("available prerequisites dispatch core and package checks before success", async () => {
  const result = await run();
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.calls, ["init", "build", "wasm: check-wasm-core execution",
    "wasm: check-wasm-package headless parity run"]);
  assert.match(result.stdout, /wasm-package gate: ok/);
});

for (const [label, options] of [
  ["failed core", { coreExit: 7 }],
  ["failed package even with success marker", { packageExit: 7 }],
  ["missing package success marker", { packageMarker: false }],
]) {
  test(`${label} cannot turn into a passing suite`, async () => {
    const result = await run(options);
    assert.equal(result.status, 1, result.stderr); // The explicit harness double's failure status.
    assert(result.calls.includes("wasm: check-wasm-package headless parity run"));
  });
}
