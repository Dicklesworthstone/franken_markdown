// Exercise the published file inventory and real package copy commands. Native
// bindings are a declared build boundary; this does not claim WASM rendering.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const source = fileURLToPath(new URL("../", import.meta.url));
const root = resolve(source, "..");
const manifest = JSON.parse(await readFile(join(source, "package.json"), "utf8"));

async function stageManifest(omit) {
  const stage = await mkdtemp(join(tmpdir(), "fmd-flow-package-"));
  for (const file of manifest.files) {
    if (file === omit || file.startsWith("pkg/")) continue;
    await mkdir(dirname(join(stage, file)), { recursive: true });
    await copyFile(join(source, file), join(stage, file));
  }
  await writeFile(join(stage, "package.json"), JSON.stringify(manifest));
  return stage;
}

// This function runs in a bounded child with the V8 module parser enabled, so
// ordinary `node --test` remains usable for this file and the full test corpus.
async function inspectDependencies(stage, files) {
  const { default: assert } = await import("node:assert/strict");
  const { readFile } = await import("node:fs/promises");
  const { dirname, join, relative, resolve } = await import("node:path");
  const { default: vm } = await import("node:vm");
  const declared = new Set(files);
  const pending = [], visited = new Set();
  const dependency = (owner, specifier) => {
    assert.match(specifier, /^\.\.?\//, `Nonlocal dependency in ${owner}`);
    const target = relative(stage, resolve(stage, dirname(owner), specifier));
    assert(!target.startsWith(".."), `Dependency escapes package: ${target}`);
    assert(declared.has(target), `Unpublished dependency: ${owner} -> ${target}`);
    pending.push(target);
  };
  // Both public demo pages and the fetched Markdown example must survive npm's
  // files[] filtering, not merely exist in the source checkout or build folder.
  for (const file of ["demo/flow-canvas.html", "demo/web-component.html"]) {
    assert(declared.has(file), `Unpublished demo: ${file}`);
    const html = await readFile(join(stage, file), "utf8");
    for (const [, specifier] of html.matchAll(/\bsrc="(\.\.?\/[^"?#]+)"/g))
      dependency(file, specifier);
    for (const [, script] of html.matchAll(/<script type="module">([\s\S]*?)<\/script>/g)) {
      const parsed = new vm.SourceTextModule(script);
      for (const specifier of parsed.dependencySpecifiers) dependency(file, specifier);
    }
  }
  // Check every shipped JS root as well: this covers worker entries and modules
  // loaded on demand without executing DOM, worker, native or renderer code.
  pending.push(...files.filter(file => /\.m?js$/.test(file)));
  while (pending.length) {
    const file = pending.pop();
    if (visited.has(file)) continue;
    visited.add(file);
    if (file.startsWith("pkg/")) continue;
    const code = await readFile(join(stage, file), "utf8");
    if (!/\.m?js$/.test(file)) continue;
    const parsed = new vm.SourceTextModule(code);
    for (const specifier of parsed.dependencySpecifiers) dependency(file, specifier);
    for (const [, , specifier] of code.matchAll(/\b(?:import\s*\(|new URL\s*\()(["'])(\.\.?\/[^"']+)\1/g))
      dependency(file, specifier);
  }
  assert(visited.has("demo/flow_canvas_navigation.mjs"));
  assert(visited.has("flow_worker.js"));
  assert(visited.has("demo/sample.md"));
}

async function inspectPackage(stage) {
  const probe = `const inspect = ${inspectDependencies};
    let input = "";
    for await (const part of process.stdin) input += part;
    const { stage, files } = JSON.parse(input);
    try { await inspect(stage, files); console.log(JSON.stringify({ ok: true })); }
    catch (error) {
      console.log(JSON.stringify({ code: error.code, message: error.message, path: error.path }));
      process.exitCode = 1;
    }`;
  const result = spawnSync(process.execPath, ["--experimental-vm-modules", "--input-type=module", "-e", probe], {
    input: JSON.stringify({ stage, files: manifest.files }), encoding: "utf8", timeout: 10000,
  });
  assert.equal(result.error, undefined, result.error?.message);
  const receipt = JSON.parse(result.stdout);
  if (result.status !== 0) throw Object.assign(new Error(receipt.message), receipt);
  assert.equal(receipt.ok, true, result.stderr);

  // Import and initialize the actual staged navigation module. DOM input is an
  // explicit EventTarget fixture; no hit testing or rendered pixels are implied.
  const { createCanvasNavigationControls } = await import(
    pathToFileURL(join(stage, "demo/flow_canvas_navigation.mjs"))
  );
  const element = () => Object.assign(new EventTarget(), { disabled: false, value: "", textContent: "" });
  const sourceButton = element(), followButton = element(), status = element();
  const controls = createCanvasNavigationControls({
    canvas: element(), viewport: element(), sourceEditor: element(),
    sourceButton, followButton, status, getOwner: () => null,
  });
  assert(sourceButton.disabled);
  assert(followButton.disabled);
  controls.suspend();
  assert.match(status.textContent, /suspended/);
  controls.dispose();
}

test("the published inventory contains the complete demo, navigation and worker dependency closure", async () => {
  await inspectPackage(await stageManifest());
});

for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
  test(`${name} stages a runnable Flow navigation module and complete demo dependencies`, async () => {
    const script = await readFile(join(root, "scripts", name), "utf8");
    const stage = await mkdtemp(join(tmpdir(), "fmd-flow-assembly-"));
    await mkdir(join(stage, "demo"));
    // Execute only the existing hand-written file-copy commands, never build,
    // release, cleanup or rendering commands. Use the actual checkout inputs.
    const copies = [...script.matchAll(/^for file in [\s\S]*?^done$|^cp wasm\/[^\n]+$/gm)]
      .filter(match => match[0].startsWith("cp ") || match[0].includes('cp "wasm/'))
      .map(match => match[0]);
    assert(copies.length > 0, "No package copy commands found");
    const result = spawnSync("bash", ["-eu", "-c", copies.join("\n")], {
      cwd: root, encoding: "utf8", timeout: 10000,
      env: { ...process.env, PACKAGE: stage, package_dir: stage },
    });
    assert.equal(result.status, 0, result.stderr);
    await inspectPackage(stage);
  });
}

test("the staged-package check rejects an omitted runtime even when checkout tests can import it", async () => {
  await assert.rejects(inspectPackage(await stageManifest("demo/flow_canvas_navigation.mjs")),
    error => error.code === "ENOENT" && error.path.endsWith("/demo/flow_canvas_navigation.mjs"));
});
