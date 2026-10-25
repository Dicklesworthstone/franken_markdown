// Exercise the real shell gate and real Node against four file-backed proof
// tests. Native/coverage/e2e endpoints are explicit control-flow doubles: these
// controls establish dispatch/exit handling, never Rust or renderer readiness.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { access, mkdir, mkdtemp, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const suiteSource = await readFile(new URL("./test-all.sh", import.meta.url), "utf8");
const jsGateSource = await readFile(new URL("./check-javascript.sh", import.meta.url), "utf8");
const families = ["wasm", "wasm/tests", "scripts", "scripts/font-oracles"];

async function systemTool(name) {
  for (const directory of ["/usr/bin", "/bin"]) {
    const path = join(directory, name);
    try { await access(path); return path; } catch { /* Try the next system directory. */ }
  }
  throw new Error(`Required test prerequisite is absent: ${name}`);
}

async function run({ args = ["--fast"], node = true, omit = [], nullglob = false,
  failFamily, malformedFamily, cargoExit = 0, inheritedContext } = {}) {
  const root = await mkdtemp(join(tmpdir(), "fmd-test-all-js-"));
  for (const directory of [...families, "scripts/e2e", "bin"])
    await mkdir(join(root, directory), { recursive: true });
  await writeFile(join(root, "scripts/test-all.sh"), suiteSource, { mode: 0o755 });
  await writeFile(join(root, "scripts/check-javascript.sh"), jsGateSource, { mode: 0o755 });
  const trace = join(root, "native-control-trace");
  // A minimal PATH actually removes Node, rather than relying on whichever
  // runtimes happen to be installed later in a developer's host PATH.
  for (const name of ["dirname", "date", "find", "sort", "sed", "bash"])
    await symlink(await systemTool(name), join(root, "bin", name));
  if (node) await symlink(process.execPath, join(root, "bin/node"));
  const nativeControl = `#!/bin/sh\nprintf 'native-control:%s\\n' "$0" >> "$FMD_TEST_ALL_TRACE"\nexit 0\n`;
  await writeFile(join(root, "bin/cargo"), nativeControl.replace("exit 0", `exit ${cargoExit}`), { mode: 0o755 });
  for (const path of ["install.sh", "scripts/check-policy.sh", "scripts/check-wasm-core.sh",
    "scripts/check-determinism.sh", "scripts/check-test-doubles.sh", "scripts/commonmark-conformance.sh", "scripts/coverage.sh",
    "scripts/e2e/lib.sh", "scripts/e2e/run-all.sh"])
    await writeFile(join(root, path), nativeControl, { mode: 0o755 });
  for (const family of families) {
    if (omit.includes(family)) continue;
    const witness = `witness:${family}\n`;
    await writeFile(join(root, family, "witness.txt"), failFamily === family ? "planted failure\n" : witness);
    await writeFile(join(root, family, "proof.test.mjs"), malformedFamily === family
      ? "this is not valid JavaScript !!!\n"
      : `import test from 'node:test';\nimport assert from 'node:assert/strict';\nimport { readFileSync } from 'node:fs';\ntest(${JSON.stringify(`executed ${family} proof`)}, () => assert.equal(readFileSync(new URL('./witness.txt', import.meta.url), 'utf8'), ${JSON.stringify(witness)}));\n`);
  }
  const environmentFile = join(root, "bash-env");
  await writeFile(environmentFile, nullglob ? "shopt -s nullglob\n" : "");
  const env = { ...process.env, PATH: join(root, "bin"), BASH_ENV: environmentFile,
    FMD_TARGET_DIR: join(root, "target"), FMD_TEST_ALL_TRACE: trace, NO_COLOR: "1" };
  // Isolate this new CLI run from the parent Node test runner. Production
  // rejects an inherited context rather than accepting Node's zero-run green.
  delete env.NODE_TEST_CONTEXT;
  if (inheritedContext !== undefined) env.NODE_TEST_CONTEXT = inheritedContext;
  for (const key of Object.keys(env)) if (key.startsWith("BASH_FUNC_")) delete env[key];
  const result = spawnSync(await systemTool("bash"), [join(root, "scripts/test-all.sh"), ...args], {
    encoding: "utf8", env, timeout: 15000,
  });
  assert.equal(result.error, undefined, `${result.error}\n${result.stdout}\n${result.stderr}`);
  let nativeCalls = "";
  try { nativeCalls = await readFile(trace, "utf8"); } catch (error) { if (error.code !== "ENOENT") throw error; }
  return { ...result, nativeCalls };
}

for (const [mode, args] of [["fast", ["--fast"]], ["full", []]]) {
  test(`${mode} gauntlet executes real Node tests from all four required families`, async () => {
    const r = await run({ args });
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.match(r.stdout, /javascript-tests: executing 4 test files/);
    for (const family of families) assert(r.stdout.includes(`executed ${family} proof`), r.stdout);
    if (mode === "fast") {
      assert.match(r.stdout, /selected fast gates passed; coverage and e2e run-all were not executed/);
      assert.doesNotMatch(r.stdout, /every (?:required )?gate passed/);
    } else {
      assert.match(r.nativeCalls, /coverage\.sh/);
      assert.match(r.nativeCalls, /e2e\/run-all\.sh/);
      assert.match(r.stdout, /every required gate passed/);
    }
  });
}

for (const family of families) {
  test(`a failing real Node test in ${family} fails the combined gauntlet`, async () => {
    const r = await run({ failFamily: family });
    assert.equal(r.status, 70, r.stdout + r.stderr);
    assert.match(r.stdout, /JavaScript runtime\+helper tests FAILED/);
    assert.match(r.stdout, /planted failure/);
    assert.doesNotMatch(r.stdout, /test-all: ok/);
  });
}

test("missing Node is a required-gate failure, never a passing skip", async () => {
  const r = await run({ node: false });
  assert.equal(r.status, 70, r.stdout + r.stderr);
  assert.match(r.stderr, /required node executable not found/);
  assert.match(r.stdout, /JavaScript runtime\+helper tests.*rc=66/);
  assert.doesNotMatch(r.stdout, /test-all: ok/);
});

for (const family of families) {
  test(`an omitted ${family} test family fails before Node execution`, async () => {
    const r = await run({ omit: [family] });
    assert.equal(r.status, 70, r.stdout + r.stderr);
    assert(r.stderr.includes(`required test family ${family}/*.test.mjs is missing or empty`), r.stderr);
    assert.doesNotMatch(r.stdout, /executed .* proof/);
  });
}

test("nullglob cannot silently omit a required test family", async () => {
  const r = await run({ omit: ["wasm/tests"], nullglob: true });
  assert.equal(r.status, 70, r.stdout + r.stderr);
  assert.match(r.stderr, /required test family wasm\/tests/);
  assert.doesNotMatch(r.stdout, /executed .* proof/);
});

test("empty discovery cannot become a zero-test green", async () => {
  const r = await run({ omit: families, nullglob: true });
  assert.equal(r.status, 70, r.stdout + r.stderr);
  assert.match(r.stderr, /tests were not executed/);
  assert.doesNotMatch(r.stdout, /test-all: ok/);
});

test("existing native failures remain failures after adding the JavaScript gate", async () => {
  const r = await run({ cargoExit: 7 });
  assert.equal(r.status, 70, r.stdout + r.stderr);
  assert.match(r.stdout, /check-all-targets.*rc=7/);
  assert.doesNotMatch(r.stdout, /test-all: ok/);
});

for (const args of [["--fast", "extra"], ["--help", "extra"], ["--unknown"]]) {
  test(`usage ${JSON.stringify(args)} is rejected before running gates`, async () => {
    const r = await run({ args, node: false });
    assert.equal(r.status, 2, r.stdout + r.stderr);
    assert.equal(r.nativeCalls, "");
    assert.doesNotMatch(r.stdout, /test-all: ok/);
  });
}

test("help remains available without Node or any native execution", async () => {
  const r = await run({ args: ["--help"], node: false });
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /Usage:/);
  assert.equal(r.nativeCalls, "");
});

test("a malformed JavaScript suite cannot pass the gauntlet", async () => {
  const r = await run({ malformedFamily: "scripts" });
  assert.equal(r.status, 70, r.stdout + r.stderr);
  assert.match(r.stdout + r.stderr, /SyntaxError/);
  assert.doesNotMatch(r.stdout, /test-all: ok/);
});


for (const inheritedContext of ["child-v8", ""]) {
  test(`inherited Node context ${JSON.stringify(inheritedContext)} cannot produce a skipped zero-run green`, async () => {
    const r = await run({ inheritedContext });
    assert.equal(r.status, 70, r.stdout + r.stderr);
    assert.match(r.stderr, /inherited NODE_TEST_CONTEXT would skip/);
    assert.doesNotMatch(r.stdout, /executed .* proof|test-all: ok/);
  });
}
