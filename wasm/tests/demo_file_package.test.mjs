import assert from "node:assert/strict";
import test from "node:test";
import { readFile, mkdtemp, mkdir, copyFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve, relative } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";

const wasm = fileURLToPath(new URL("../", import.meta.url)), root = resolve(wasm, "..");
const entry = "demo/demo_file_controls.mjs";
const manifest = JSON.parse(await readFile(join(wasm, "package.json"), "utf8"));

async function closure() {
  const pending = [entry], visited = new Map();
  while (pending.length) {
    const file = pending.pop(); if (visited.has(file)) continue;
    assert.equal(manifest.files.filter(value => value === file).length, 1, `Unpublished file dependency: ${file}`);
    assert.doesNotMatch(file, /(?:^pkg\/|franken_markdown\.js$|worker_entry)/, "Source saving must not start or import a renderer");
    const source = await readFile(join(wasm, file), "utf8"); visited.set(file, source);
    for (const [, specifier] of source.matchAll(/^import\s[^;]*?from\s["']([^"']+)["']/gm)) {
      assert(specifier.startsWith("."), `Nonlocal file dependency: ${specifier}`);
      const target = relative(wasm, resolve(wasm, dirname(file), specifier));
      assert(!target.startsWith(".."), `Dependency escapes package: ${target}`);
      pending.push(target);
    }
  }
  return visited;
}

test("direct demo saving ships the shared source-only file engine and exposes every recovery action", async () => {
  const visited = await closure();
  for (const file of ["demo/flow_file_session.mjs", "demo/flow_document.mjs", "flow_session.mjs"])
    assert(visited.has(file), `The shared source engine is absent: ${file}`);
  const html = await readFile(join(wasm, "demo/index.html"), "utf8");
  for (const action of ["open", "save", "save-as", "reload", "keep", "backup", "disconnect", "status"])
    assert.equal([...html.matchAll(new RegExp(`id="direct-file-${action}"`, "g"))].length, 1);
  const demo = await readFile(join(wasm, "demo/demo.js"), "utf8");
  assert.match(demo, /import \{ createDemoFileControls \} from "\.\/demo_file_controls\.mjs"/);
  assert(manifest.sideEffects.includes("./demo/demo.js"));
});

for (const scriptName of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
  test(`${scriptName} executes its new file-panel copy and imports the staged production closure`, async () => {
    const script = await readFile(join(root, "scripts", scriptName), "utf8");
    const copy = script.match(/^cp wasm\/demo\/demo_file_controls\.mjs[^\n]+$/m);
    assert(copy, `${scriptName} does not copy the direct file panel`);
    assert(script.includes("node --test --test-concurrency=1 wasm/tests/demo_file_*.test.mjs"));
    const stage = await mkdtemp(join(tmpdir(), "fmd-demo-file-package-"));
    await mkdir(join(stage, "demo"));
    const result = spawnSync("bash", ["-eu", "-c", copy[0]], {
      cwd: root, encoding: "utf8", env: { ...process.env, PACKAGE: stage, package_dir: stage }, timeout: 10000,
    });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(await readFile(join(stage, entry)), await readFile(join(wasm, entry)));
    for (const file of (await closure()).keys()) {
      if (file === entry) continue;
      await mkdir(dirname(join(stage, file)), { recursive: true });
      await copyFile(join(wasm, file), join(stage, file));
    }
    await writeFile(join(stage, "package.json"), '{"type":"module"}\n');
    const module = await import(pathToFileURL(join(stage, entry)));
    assert.equal(typeof module.createDemoFileControls, "function");
    assert.throws(() => module.createDemoFileControls({ root: { querySelector: () => null } }), /controls are missing/);
  });
}
