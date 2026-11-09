import assert from "node:assert/strict";
import test from "node:test";
import { readFile, mkdtemp, mkdir, copyFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { join, resolve, relative } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";

const wasm = fileURLToPath(new URL("../", import.meta.url)), root = resolve(wasm, "..");
const feature = ["demo_workspace_runtime.mjs", "demo_workspace_worker.mjs", "demo_workspace_worker_entry.js"];
const manifest = JSON.parse(await readFile(join(wasm, "package.json"), "utf8"));

test("the demo worker's complete static module closure is explicitly published", async () => {
  const pending = ["demo/demo.js", "demo/demo_workspace_worker_entry.js"], checked = new Set();
  while (pending.length) {
    const file = pending.pop(); if (checked.has(file)) continue; checked.add(file);
    assert(manifest.files.includes(file), `Unpublished workspace dependency ${file}`);
    // Generated glue is an explicit build artifact, not a checked-in source.
    // The physical worker test separately supplies and loads a matching pair.
    if (file.startsWith("pkg/")) continue;
    const source = await readFile(join(wasm, file), "utf8");
    for (const match of source.matchAll(/(?:\bfrom\s+|\bimport\s*\(|\bnew URL\s*\()(["'])(\.\.?\/[^"']+)\1/g)) {
      const target = relative(wasm, fileURLToPath(new URL(match[2], pathToFileURL(join(wasm, file)))));
      assert(!target.startsWith(".."), `Dependency escapes package: ${target}`);
      if (/\.[cm]?js$/.test(target)) pending.push(target);
    }
  }
  assert(checked.has("interactive_export.mjs")); assert(checked.has("native_workspace_publisher.mjs"));
  assert(checked.has("worker_transport.mjs")); assert(checked.has("demo/demo_workspace_runtime.mjs"));
  assert(manifest.sideEffects.includes("./demo/demo_workspace_worker_entry.js"));
});

for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
  test(`${name} executes its workspace copy command and loads the staged transport without WASM`, async () => {
    const script = await readFile(join(root, "scripts", name), "utf8");
    const copy = script.match(/^cp wasm\/demo\/demo_workspace_runtime\.mjs[^\n]+$/m);
    assert(copy, `${name} omits the workspace files`);
    assert(script.includes("node --test --test-concurrency=1 wasm/tests/demo_workspace*.test.mjs"));
    const stage = await mkdtemp(join(tmpdir(), "fmd-demo-workspace-package-"));
    await mkdir(join(stage, "demo"));
    const result = spawnSync("bash", ["-eu", "-c", copy[0]], {
      cwd: root, encoding: "utf8", env: { ...process.env, PACKAGE: stage, package_dir: stage }, timeout: 10000,
    });
    assert.equal(result.status, 0, result.stderr);
    for (const file of feature) {
      assert.equal(manifest.files.filter(path => path === `demo/${file}`).length, 1);
      assert.deepEqual(await readFile(join(stage, "demo", file)), await readFile(join(wasm, "demo", file)));
    }
    assert.match(script, /\bworker_transport\.mjs\b/);
    await copyFile(join(wasm, "worker_transport.mjs"), join(stage, "worker_transport.mjs"));
    const module = await import(pathToFileURL(join(stage, "demo/demo_workspace_worker.mjs")));
    const client = module.createDemoWorkspaceWorker({ workerFactory() { throw new Error("Admission must precede worker startup"); } });
    await assert.rejects(client.prepare("Source", { customCss: "p{}" }), { code: "UNSUPPORTED_WORKSPACE_SETTINGS" });
    client.dispose();
    const runtime = await import(pathToFileURL(join(stage, "demo/demo_workspace_runtime.mjs")));
    assert.equal(runtime.DEMO_WORKSPACE_RUNTIME_LIMITS.wasmBytes, 64 * 1024 * 1024);
  });
}
