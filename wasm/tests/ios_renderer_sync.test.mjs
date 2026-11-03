import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

const syncSource = readFileSync(new URL("../../ios/sync-renderer.sh", import.meta.url), "utf8");
const files = {
  "franken_markdown.js": 'import "./pdf_page.mjs"; import "./pkg/franken_markdown.js";\n',
  "franken_markdown.d.ts": "export declare function render(): string;\n",
  "package.json": '{"type":"module","version":"fixture"}\n',
  "README.md": "Verified fixture package\n",
  "pdf_page.mjs": 'export { page } from "./helpers/shared.mjs";\n',
  "helpers/shared.mjs": "export const page = 42;\n",
  "pkg/franken_markdown.js": "export function init() {}\n",
  "pkg/franken_markdown_bg.wasm": Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]),
  "demo/entry.js": 'import "./nested/view.mjs";\n',
  "demo/nested/view.mjs": "export const view = true;\n",
  "assets/example data.txt": "Resource with spaces\n",
  ".package-note": "Hidden package resource\n",
};
const bridge = "Native app-owned preview bridge\n";
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const initialManifest = JSON.stringify({
  schema: "frankenmarkdown-ios-renderer-v1",
  wrapperSha256: sha(files["franken_markdown.js"]),
  wasmSha256: sha(files["pkg/franken_markdown_bg.wasm"]),
}) + "\n";

function put(root, path, content) {
  const destination = join(root, path);
  mkdirSync(dirname(destination), {recursive: true});
  writeFileSync(destination, content);
}

function fixture({omit = [], changed = {}, failBuild = false, missingPackage = false} = {}) {
  const root = mkdtempSync(join(tmpdir(), "fmd-ios-renderer-sync-"));
  const pkg = join(root, "target/fmd-checks/wasm-package");
  const dest = join(root, "ios/Renderer");
  put(root, "ios/sync-renderer.sh", syncSource);
  put(root, "scripts/check-wasm-package.sh", `#!/usr/bin/env bash\nset -euo pipefail\n[[ "$*" == "local" ]]\nprintf '%s\\n' verified > build-called\nexit ${failBuild ? 23 : 0}\n`);
  for (const [path, content] of Object.entries(files)) {
    if (!missingPackage) put(pkg, path, content);
    if (!omit.includes(path)) put(dest, path, changed[path] ?? content);
  }
  put(dest, "bridge.html", bridge);
  put(dest, "RendererManifest.json", initialManifest);
  put(dest, "native-only.txt", "Keep app-owned files\n");
  return {root, pkg, dest};
}

function run(stage, args = []) {
  return spawnSync("bash", [join(stage.root, "ios/sync-renderer.sh"), ...args], {
    cwd: stage.root,
    encoding: "utf8",
    timeout: 10_000,
    maxBuffer: 1024 * 1024,
  });
}

function snapshot(root) {
  const result = {};
  function walk(directory, prefix = "") {
    for (const name of readdirSync(directory).sort()) {
      const relative = prefix + name;
      const full = join(directory, name);
      if (lstatSync(full).isDirectory()) walk(full, relative + "/");
      else result[relative] = readFileSync(full).toString("base64");
    }
  }
  walk(root);
  return result;
}

function success(result) {
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr || result.stdout);
}

function refused(result, pattern) {
  assert.equal(result.error, undefined);
  assert.notEqual(result.status, 0, "incomplete or conflicting package must not report success");
  assert.match(result.stderr + result.stdout, pattern);
}

test("iOS renderer check accepts the complete package without building or modifying it", () => {
  const stage = fixture();
  const before = snapshot(stage.dest);
  success(run(stage, ["--check"]));
  assert.equal(existsSync(join(stage.root, "build-called")), false);
  assert.deepEqual(snapshot(stage.dest), before);
});

for (const path of ["pdf_page.mjs", "helpers/shared.mjs", "pkg/franken_markdown.js", "demo/nested/view.mjs"]) {
  test(`iOS renderer check rejects missing runtime module ${path}`, () => {
    const stage = fixture({omit: [path]});
    refused(run(stage, ["--check"]), new RegExp(path.replaceAll(".", "\\.")));
    assert.equal(existsSync(join(stage.root, "build-called")), false);
  });
}

for (const path of ["pdf_page.mjs", "pkg/franken_markdown.js", "demo/entry.js"]) {
  test(`iOS renderer check rejects modified runtime module ${path}`, () => {
    const stage = fixture({changed: {[path]: "stale module\n"}});
    refused(run(stage, ["--check"]), new RegExp(path.replaceAll(".", "\\.")));
  });
}

test("iOS renderer sync rebuilds then copies the entire package and preserves native files", () => {
  const stage = fixture({omit: Object.keys(files)});
  success(run(stage));
  assert.equal(readFileSync(join(stage.root, "build-called"), "utf8"), "verified\n");
  for (const [path, content] of Object.entries(files)) {
    assert.deepEqual(readFileSync(join(stage.dest, path)), Buffer.from(content), path);
  }
  assert.equal(readFileSync(join(stage.dest, "bridge.html"), "utf8"), bridge);
  assert.equal(readFileSync(join(stage.dest, "native-only.txt"), "utf8"), "Keep app-owned files\n");
  const manifest = JSON.parse(readFileSync(join(stage.dest, "RendererManifest.json"), "utf8"));
  assert.equal(manifest.wrapperSha256, sha(files["franken_markdown.js"]));
  assert.equal(manifest.wasmSha256, sha(files["pkg/franken_markdown_bg.wasm"]));
  success(run(stage, ["--check"]));
});

for (const collision of ["bridge.html", "RendererManifest.json", "BRIDGE.HTML"]) {
  test(`iOS renderer sync refuses private-file collision ${collision} before copying`, () => {
    const stage = fixture({changed: {"franken_markdown.js": "Existing native renderer\n"}});
    put(stage.pkg, collision, "Package must not replace native ownership\n");
    const before = snapshot(stage.dest);
    refused(run(stage), /reserved|app-owned|collision/i);
    assert.deepEqual(snapshot(stage.dest), before);
  });
}

test("iOS renderer sync refuses package symlinks before copying", () => {
  const stage = fixture({changed: {"franken_markdown.js": "Existing native renderer\n"}});
  symlinkSync(join(stage.dest, "bridge.html"), join(stage.pkg, "alias.mjs"));
  const before = snapshot(stage.dest);
  refused(run(stage), /symbolic link|symlink/i);
  assert.deepEqual(snapshot(stage.dest), before);
});

test("iOS renderer sync does not publish a package after its verified build fails", () => {
  const stage = fixture({failBuild: true, changed: {"franken_markdown.js": "Existing native renderer\n"}});
  const before = snapshot(stage.dest);
  const result = run(stage);
  assert.equal(result.status, 23);
  assert.deepEqual(snapshot(stage.dest), before);
});

test("iOS renderer check refuses a missing package without starting a build", () => {
  const stage = fixture({missingPackage: true});
  refused(run(stage, ["--check"]), /verified renderer package is missing/);
  assert.equal(existsSync(join(stage.root, "build-called")), false);
});
