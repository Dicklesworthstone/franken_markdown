import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { assertProfileSizes, compareProfilePackages } from "./compare_profiles.mjs";
const full = { rawBytes: 7_700_000, gzipBytes: 3_300_000 };
const render = { rawBytes: 5_000_000, gzipBytes: 2_200_000 };

test("size proof requires improvement in both raw and gzip on this invocation", () => {
  assertProfileSizes(render, full);
  for (const value of [full, { ...render, rawBytes: full.rawBytes },
    { ...render, gzipBytes: full.gzipBytes }, { ...render, gzipBytes: full.gzipBytes + 1 }]) {
    assert.throws(() => assertProfileSizes(value, full), /did not improve/);
  }
});

test("neither profile may evade the unchanged absolute budget", () => {
  for (const value of [{ ...full, rawBytes: 7_900_001 }, { ...full, gzipBytes: 3_400_001 },
    { ...full, rawBytes: NaN }, { ...full, rawBytes: "7700000" },
    { ...full, gzipBytes: 0 }, { ...full, rawBytes: 7_000_000.5 }]) {
    assert.throws(() => assertProfileSizes(render, value), /ceiling/);
    assert.throws(() => assertProfileSizes(value, full), /ceiling/);
  }
});

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "fmd-profile-compare-"));
  const paths = [path.join(root, "render"), path.join(root, "full")];
  for (const directory of paths) {
    await fs.mkdir(path.join(directory, "pkg"), { recursive: true });
    // Real, valid empty WASM: deliberately lacks all renderer/flow exports.
    await fs.writeFile(path.join(directory, "pkg/franken_markdown_bg.wasm"), new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]));
  }
  return { paths, report: path.join(root, "report.json") };
}

for (const mode of ["direct", "directory-alias", "file-alias", "preserved-directory-alias", "preserved-file-alias"]) {
  test(`comparison CLI executes and reports real failure through ${mode}`, async () => {
    const f = await fixture(), script = fileURLToPath(new URL("./compare_profiles.mjs", import.meta.url));
    const root = path.dirname(f.report); let entry = script;
    if (mode.includes("directory")) {
      const alias = path.join(root, "script directory");
      await fs.symlink(path.dirname(script), alias, "dir"); entry = path.join(alias, path.basename(script));
    } else if (mode.includes("file")) {
      entry = path.join(root, "script alias.mjs"); await fs.symlink(script, entry);
      // Preserved main aliases resolve relative imports at the alias location.
      // Keep the actual sibling probe there so entry-point behavior is tested,
      // rather than an unrelated module-not-found failure before execution.
      await fs.copyFile(new URL("./flow_profile_probe.mjs", import.meta.url), path.join(root, "flow_profile_probe.mjs"));
    }
    const flags = mode.startsWith("preserved") ? ["--preserve-symlinks-main"] : [];
    const result = spawnSync(process.execPath, [...flags, entry, ...f.paths, f.report, "a".repeat(40)], { encoding: "utf8" });
    assert.equal(result.status, 1, result.stderr);
    const report = JSON.parse(await fs.readFile(f.report, "utf8"));
    assert.equal(report.passed, false); assert.equal(report.errors.length, 2);
    assert.equal(report.render.rawBytes, 8); assert.equal(report.full.rawBytes, 8);
    assert.ok(report.errors.every(error => error.message.includes("missing native WASM memory")));
  });
}

test("aliased comparison CLI refuses missing arguments without a false zero exit", async () => {
  const f = await fixture(), alias = path.join(path.dirname(f.report), "compare alias.mjs");
  await fs.symlink(fileURLToPath(new URL("./compare_profiles.mjs", import.meta.url)), alias);
  const result = spawnSync(process.execPath, [alias], { encoding: "utf8" });
  assert.equal(result.status, 64); assert.match(result.stderr, /Usage:/);
  await assert.rejects(fs.lstat(f.report), { code: "ENOENT" });
});

test("a valid-but-nonrendering binary fails and leaves a measured failure report", async () => {
  const f = await fixture();
  await assert.rejects(compareProfilePackages(...f.paths, f.report, "a".repeat(40)), /comparison failed/);
  const report = JSON.parse(await fs.readFile(f.report, "utf8"));
  assert.equal(report.passed, false); assert.equal(report.errors.length, 2);
  assert.equal(report.render.rawBytes, 8); assert.equal(report.full.rawBytes, 8);
  assert.equal(report.render.passed, false); assert.equal(report.full.passed, false);
  assert(report.errors.every(error => error.message.includes("missing native WASM memory")));
  assert.equal(report.render.wasmSha256, report.full.wasmSha256);
});

test("reports never overwrite existing evidence, including on failed comparison", async () => {
  const f = await fixture(); await fs.writeFile(f.report, "retained evidence");
  await assert.rejects(compareProfilePackages(...f.paths, f.report, "a".repeat(40)), { code: "EEXIST" });
  assert.equal(await fs.readFile(f.report, "utf8"), "retained evidence");
});

test("the same package cannot masquerade as two independently built profiles", async () => {
  const f = await fixture();
  await assert.rejects(compareProfilePackages(f.paths[0], f.paths[0], f.report, "a".repeat(40)), /separate builds/);
  await assert.rejects(fs.lstat(f.report), { code: "ENOENT" });
});

test("comparison rejects an ambiguous source identity before executing package code", async () => {
  const f = await fixture();
  await assert.rejects(compareProfilePackages(...f.paths, f.report, "main"), /exact source Git commit/);
  await assert.rejects(fs.lstat(f.report), { code: "ENOENT" });
});
