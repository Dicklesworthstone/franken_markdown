// Real filesystem/package assembly; the eight-byte WASM fixture is a valid
// empty module, not a renderer. No native linkage or rendering claim is made.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import * as fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { assembleWasmProfile } from "./assemble-wasm-profile.mjs";
const wasmHeader = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "fmd-profile-test-"));
  const sourceDir = path.join(root, "source"), generatedDir = path.join(root, "generated");
  await fs.mkdir(sourceDir); await fs.mkdir(generatedDir);
  const write = async (name, text) => {
    const file = path.join(sourceDir, name);
    await fs.mkdir(path.dirname(file), { recursive: true }); await fs.writeFile(file, text);
  };
  const manifest = { name: "@test/renderer", version: "1.0.0", type: "module",
    publishConfig: { access: "public" }, sideEffects: ["./fmd-view.js", "./book.js"],
    exports: { ".": { types: "./franken_markdown.d.ts", import: "./franken_markdown.js" },
      "./web-component": { types: "./fmd-view.d.ts", import: "./fmd-view.js" },
      "./book": "./book.js", "./worker": "./worker_entry.mjs" },
    files: ["franken_markdown.js", "franken_markdown.d.ts", "fmd-view.js", "fmd-view.d.ts",
      "book.js", "worker_entry.mjs", "README.md", "pkg/franken_markdown.js"] };
  const saveManifest = () => write("package.json", JSON.stringify(manifest));
  await saveManifest();
  await write("franken_markdown.js", 'export { default } from "./pkg/franken_markdown.js";\nimport { x } from "./nested/helper.mjs";\nthrow new Error("must never evaluate input");');
  await write("nested/helper.mjs", 'export { x } from "../leaf.js";');
  await write("leaf.js", "export const x = 3;");
  await write("fmd-view.js", 'import "./franken_markdown.js";');
  await write("book.js", 'import "./book_helper.mjs";');
  await write("book_helper.mjs", "export const book = true;");
  await write("worker_entry.mjs", 'import "./worker_helper.js";');
  await write("worker_helper.js", "export const worker = true;");
  await write("franken_markdown.d.ts", "export function render(): void;\n");
  await write("fmd-view.d.ts", "export class View {}\n");
  await write("README.md", "# Source package\n");
  await fs.writeFile(path.join(generatedDir, "franken_markdown.js"), "export default async function init() {}\n");
  for (const name of ["franken_markdown.d.ts", "franken_markdown_bg.wasm.d.ts"]) {
    await fs.writeFile(path.join(generatedDir, name), "export {};\n");
  }
  await fs.writeFile(path.join(generatedDir, "franken_markdown_bg.wasm"), wasmHeader);
  const options = { sourceDir, generatedDir, outputDir: path.join(root, "output"), profile: "render" };
  return { root, options, manifest, write, saveManifest, assemble: () => assembleWasmProfile(options) };
}
async function absent(filename) { await assert.rejects(fs.lstat(filename), { code: "ENOENT" }); }

test("render profile closes nested imports and re-exports without evaluating modules", async () => {
  const f = await fixture(), receipt = await f.assemble();
  assert.equal(receipt.renderingVerified, false);
  assert.equal(receipt.cargoFeature, "wasm-bindgen");
  assert(receipt.files.some(item => item.path === "nested/helper.mjs"));
  assert(receipt.files.some(item => item.path === "leaf.js"));
  const manifest = JSON.parse(await fs.readFile(path.join(f.options.outputDir, "package.json"), "utf8"));
  assert.deepEqual(Object.keys(manifest.exports), [".", "./web-component"]);
  assert.equal(manifest.name, "@test/renderer-render");
  assert.equal(manifest.private, true);
  assert.equal(manifest.publishConfig, undefined);
  assert.deepEqual(manifest.sideEffects, ["./fmd-view.js"]);
  for (const name of ["book.js", "book_helper.mjs", "worker_entry.mjs", "worker_helper.js"]) {
    await absent(path.join(f.options.outputDir, name));
  }
  for (const item of receipt.files) {
    const bytes = await fs.readFile(path.join(f.options.outputDir, item.path));
    assert.equal(item.sha256, hash(bytes)); assert.equal(item.bytes, bytes.length);
    assert(manifest.files.includes(item.path));
  }
});

test("full profile includes declared worker roots and closes previously unlisted helpers", async () => {
  const f = await fixture(); f.options.profile = "full";
  const receipt = await f.assemble();
  assert.equal(receipt.cargoFeature, "wasm-full");
  for (const name of ["book.js", "book_helper.mjs", "worker_entry.mjs", "worker_helper.js"]) {
    assert(receipt.files.some(item => item.path === name), name);
  }
  const manifest = JSON.parse(await fs.readFile(path.join(f.options.outputDir, "package.json"), "utf8"));
  assert.deepEqual(manifest.exports, f.manifest.exports);
  assert.equal(manifest.name, "@test/renderer-full");
});

test("cycles are bounded and deduplicated, and assembly is byte deterministic", async () => {
  const f = await fixture();
  await f.write("leaf.js", 'import "./nested/helper.mjs"; export const x = 3;');
  const first = await f.assemble(), previous = f.options.outputDir;
  f.options.outputDir = path.join(f.root, "second");
  const second = await f.assemble();
  assert.deepEqual(second, first);
  assert.equal(new Set(first.files.map(item => item.path)).size, first.files.length);
  for (const name of ["package.json", "fmd-profile.json"]) {
    assert.deepEqual(await fs.readFile(path.join(previous, name)), await fs.readFile(path.join(f.options.outputDir, name)));
  }
});

test("V8 parses multiline imports and ignores import-looking comments/strings", async () => {
  const f = await fixture();
  await f.write("franken_markdown.js", `import {\n x\n} from\n "./leaf.js";\n// import './missing.js'\nconst prose = "import './also-missing.js'";\nexport { x };`);
  const receipt = await f.assemble();
  assert(receipt.files.some(item => item.path === "leaf.js"));
});

test("invalid syntax fails before the output directory is created", async () => {
  const f = await fixture(); await f.write("leaf.js", "export const = broken;");
  await assert.rejects(f.assemble(), SyntaxError); await absent(f.options.outputDir);
});

test("missing transitive dependencies fail closed rather than ship a broken import", async () => {
  const f = await fixture(); await f.write("leaf.js", 'export { x } from "./missing.js";');
  await assert.rejects(f.assemble(), { code: "ENOENT" }); await absent(f.options.outputDir);
});

test("remote, bare and escaping imports cannot enter an offline package", async () => {
  for (const specifier of ["https://example.test/a.js", "node:fs", "some-package", "../outside.js", "./%2e%2e/outside.js", "./leaf.js?x=1"]) {
    const f = await fixture(); await f.write("franken_markdown.js", `import ${JSON.stringify(specifier)};`);
    await assert.rejects(f.assemble(), /dependency|path/); await absent(f.options.outputDir);
  }
});

test("source and generated payloads do not follow file or directory symlinks", async () => {
  for (const directory of [false, true]) {
    const f = await fixture();
    if (directory) {
      await fs.mkdir(path.join(f.root, "foreign"));
      await fs.writeFile(path.join(f.root, "foreign", "escape.js"), "export {};");
      await fs.symlink(path.join(f.root, "foreign"), path.join(f.options.sourceDir, "linked"), "dir");
      await f.write("franken_markdown.js", 'import "./linked/escape.js";');
    } else {
      await fs.symlink(path.join(f.root, "generated", "franken_markdown.js"), path.join(f.options.sourceDir, "linked.js"));
      await f.write("franken_markdown.js", 'import "./linked.js";');
    }
    await assert.rejects(f.assemble(), /Symlink/); await absent(f.options.outputDir);
  }
});

test("an existing destination is untouched", async () => {
  const f = await fixture(); await fs.mkdir(f.options.outputDir);
  const sentinel = path.join(f.options.outputDir, "important.txt"); await fs.writeFile(sentinel, "retain me");
  await assert.rejects(f.assemble(), /already exists/);
  assert.equal(await fs.readFile(sentinel, "utf8"), "retain me");
  assert.deepEqual(await fs.readdir(f.options.outputDir), ["important.txt"]);
});

test("invalid profile and an output nested in either input are refused", async () => {
  const f = await fixture();
  await assert.rejects(assembleWasmProfile({ ...f.options, profile: "unknown" }), /Profile/);
  for (const input of [f.options.sourceDir, f.options.generatedDir]) {
    const outputDir = path.join(input, "nested-output");
    await assert.rejects(assembleWasmProfile({ ...f.options, outputDir }), /outside/);
    await absent(outputDir);
  }
});

test("invalid WASM bytes cannot be packaged", async () => {
  const f = await fixture();
  await fs.writeFile(path.join(f.options.generatedDir, "franken_markdown_bg.wasm"), "not wasm");
  await assert.rejects(f.assemble(), /not a valid module/); await absent(f.options.outputDir);
});

test("malformed manifests and non-local export targets fail before publication", async () => {
  for (const mutate of [m => { m.files = ["../outside.js"]; },
    m => { m.exports["."] = "https://example.test/a.js"; },
    m => { m.exports["."] = null; }, m => { m.files = Array(4097).fill("leaf.js"); },
    m => { m.files.push("package.json"); }, m => { m.files.push("fmd-profile.json"); }]) {
    const f = await fixture(); f.options.profile = "full"; mutate(f.manifest); await f.saveManifest();
    await assert.rejects(f.assemble()); await absent(f.options.outputDir);
  }
});

test("copied generated bindings and wasm are the exact selected build, not source pkg leftovers", async () => {
  const f = await fixture(); await f.write("pkg/franken_markdown_bg.wasm", "stale source artifact");
  const receipt = await f.assemble();
  assert.deepEqual(new Uint8Array(await fs.readFile(path.join(f.options.outputDir, "pkg/franken_markdown_bg.wasm"))), wasmHeader);
  assert.equal(receipt.files.find(item => item.path.endsWith(".wasm")).sha256, hash(wasmHeader));
});
