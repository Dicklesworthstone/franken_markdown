#!/usr/bin/env node
// Build-time only: assemble a closed local ESM package without evaluating source
// modules or fetching anything. Run with --experimental-vm-modules on Node 18+.
import { createHash } from "node:crypto";
import * as fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import * as vm from "node:vm";

const GENERATED = ["franken_markdown.js", "franken_markdown.d.ts",
  "franken_markdown_bg.wasm", "franken_markdown_bg.wasm.d.ts"];
const RENDER_FILES = ["franken_markdown.js", "franken_markdown.d.ts",
  "fmd-view.js", "fmd-view.d.ts", "README.md"];
const MAX_FILES = 4096, MAX_BYTES = 256 * 1024 * 1024;
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const record = value => value && typeof value === "object" && !Array.isArray(value);
function relativeName(value) {
  if (typeof value !== "string" || !value || value.length > 512
      || /[\\\u0000-\u0020\u007f:#?%*]/u.test(value)
      || path.posix.isAbsolute(value) || path.posix.normalize(value) !== value
      || value === "." || value === ".." || value.startsWith("../")) {
    throw new Error(`Invalid package path: ${String(value)}`);
  }
  return value;
}
function dependency(owner, specifier) {
  if (typeof specifier !== "string" || !/^\.\.?\//.test(specifier)) {
    throw new Error(`Non-local module dependency in ${owner}: ${String(specifier)}`);
  }
  const target = relativeName(path.posix.join(path.posix.dirname(owner), specifier));
  if (!/\.(?:mjs|js)$/.test(target)) {
    throw new Error(`Non-JavaScript static dependency in ${owner}: ${specifier}`);
  }
  return target;
}
function exportFiles(value, result = []) {
  if (typeof value === "string") {
    if (!value.startsWith("./")) throw new Error("Package exports must be relative files");
    result.push(relativeName(value.slice(2)));
  } else if (record(value)) {
    for (const child of Object.values(value)) exportFiles(child, result);
  } else {
    throw new Error("Unsupported package export condition");
  }
  return result;
}
async function readOwned(root, name, maximum) {
  const parts = relativeName(name).split("/");
  let filename = root;
  for (let index = 0; index < parts.length; index++) {
    filename = path.join(filename, parts[index]);
    const stat = await fs.lstat(filename);
    if (stat.isSymbolicLink()) throw new Error(`Symlink in package input: ${name}`);
    if (index < parts.length - 1 && !stat.isDirectory()) throw new Error(`Not a directory: ${name}`);
    if (index === parts.length - 1 && (!stat.isFile() || stat.size > maximum)) {
      throw new Error(`Invalid or oversized package input: ${name}`);
    }
  }
  const bytes = await fs.readFile(filename);
  if (bytes.length > maximum) throw new Error(`Package byte budget exceeded: ${name}`);
  return bytes;
}

/** Input roots are trusted checkout/build directories, never executable module
 * inputs. Static imports/re-exports are parsed with V8, NOT executed. Dynamic
 * imports, worker entries and non-JS resources must be listed in files/exports.
 * Validation precedes output creation; writes are create-only. A disk error may
 * leave a new incomplete directory, but never overwrites or deletes prior files.
 */
export async function assembleWasmProfile({ sourceDir, generatedDir, outputDir, profile }) {
  if (!["render", "full"].includes(profile)) throw new Error("Profile must be render or full");
  if (typeof vm.SourceTextModule !== "function") {
    throw new Error("Package assembly requires node --experimental-vm-modules");
  }
  const source = await fs.realpath(sourceDir), generated = await fs.realpath(generatedDir);
  const output = path.join(await fs.realpath(path.dirname(path.resolve(outputDir))), path.basename(outputDir));
  for (const input of [source, generated]) {
    const rel = path.relative(input, output);
    if (rel === "" || (!rel.startsWith(`..${path.sep}`) && rel !== ".." && !path.isAbsolute(rel))) {
      throw new Error("Output must be outside both package input directories");
    }
  }
  try {
    await fs.lstat(output);
    throw new Error("Output directory already exists; choose a fresh path");
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const sourceManifest = JSON.parse((await readOwned(source, "package.json", 1024 * 1024)).toString("utf8"));
  if (!record(sourceManifest) || sourceManifest.type !== "module"
      || typeof sourceManifest.name !== "string" || !sourceManifest.name
      || !record(sourceManifest.exports) || !Array.isArray(sourceManifest.files)
      || sourceManifest.files.length > MAX_FILES) throw new Error("Invalid source package manifest");
  const selectedExports = profile === "full" ? sourceManifest.exports : {
    ".": sourceManifest.exports["."],
    "./web-component": sourceManifest.exports["./web-component"],
  };
  const roots = [...(profile === "full" ? [...sourceManifest.files, "README.md"] : RENDER_FILES),
    ...GENERATED.map(name => `pkg/${name}`), ...exportFiles(selectedExports)];
  for (const name of roots) relativeName(name);
  const queue = [...new Set(roots)].sort(), pending = new Set(queue), files = new Map();
  let total = 0;
  for (let index = 0; index < queue.length; index++) {
    if (queue.length > MAX_FILES) throw new Error("Package file budget exceeded");
    const name = queue[index];
    if (name === "package.json" || name === "fmd-profile.json") {
      throw new Error(`Reserved generated package path: ${name}`);
    }
    const isGenerated = name.startsWith("pkg/");
    const bytes = await readOwned(isGenerated ? generated : source, isGenerated ? name.slice(4) : name, MAX_BYTES - total);
    files.set(name, bytes);
    total += bytes.length;
    if (/\.(?:js|mjs)$/.test(name)) {
      // Parsing does not link/evaluate source, invoke getters, or run top-level awaits.
      const module = new vm.SourceTextModule(new TextDecoder("utf-8", { fatal: true }).decode(bytes), { identifier: name });
      const specifiers = module.moduleRequests
        ? module.moduleRequests.map(request => request.specifier) : module.dependencySpecifiers;
      for (const specifier of specifiers) {
        const target = dependency(name, specifier);
        if (!pending.has(target)) { pending.add(target); queue.push(target); }
      }
    }
  }
  const wasm = files.get("pkg/franken_markdown_bg.wasm");
  if (!WebAssembly.validate(wasm)) throw new Error("Generated WASM is not a valid module");
  const inventory = [...files].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
    .map(([name, bytes]) => ({ path: name, bytes: bytes.length, sha256: digest(bytes) }));
  const receipt = {
    schema: "fmd-wasm-package-profile-v1", profile,
    cargoFeature: profile === "render" ? "wasm-bindgen" : "wasm-full",
    // Assembly is not execution proof. The separate DSR probe validates ABI and parity.
    renderingVerified: false, payloadBytes: total, files: inventory,
  };
  const manifest = { ...sourceManifest, name: `${sourceManifest.name}-${profile}`, private: true,
    exports: selectedExports, files: [...inventory.map(item => item.path), "fmd-profile.json"] };
  delete manifest.publishConfig;
  if (Array.isArray(manifest.sideEffects)) {
    manifest.sideEffects = manifest.sideEffects.filter(name => files.has(name.replace(/^\.\//, "")));
  }
  await fs.mkdir(output); // Exclusive: a pre-existing file/directory is never reused.
  for (const item of inventory) {
    const filename = path.join(output, item.path);
    await fs.mkdir(path.dirname(filename), { recursive: true });
    await fs.writeFile(filename, files.get(item.path), { flag: "wx" });
  }
  await fs.writeFile(path.join(output, "fmd-profile.json"), `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx" });
  // Write the manifest last: only a completely written tree becomes an ESM package.
  await fs.writeFile(path.join(output, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`, { flag: "wx" });
  return receipt;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [sourceDir, generatedDir, outputDir, profile, ...extra] = process.argv.slice(2);
  if (!sourceDir || !generatedDir || !outputDir || !profile || extra.length) {
    console.error("Usage: node --experimental-vm-modules scripts/assemble-wasm-profile.mjs SOURCE GENERATED OUTPUT render|full");
    process.exitCode = 64;
  } else {
    try {
      const receipt = await assembleWasmProfile({ sourceDir, generatedDir, outputDir, profile });
      console.log(JSON.stringify({ profile: receipt.profile, files: receipt.files.length, payloadBytes: receipt.payloadBytes, outputDir }));
    } catch (error) { console.error(error.message); process.exitCode = 1; }
  }
}
