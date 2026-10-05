#!/usr/bin/env node
// Execute independently built packages. Assembly/probe doubles are not used by
// this runner: DSR supplies generated wasm-bindgen modules and their binaries.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import * as fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";
import { assertProfileParity, probeFlowProfile } from "./flow_profile_probe.mjs";

const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const fingerprint = bytes => ({ bytes: bytes.length, sha256: hash(bytes) });
// Existing full-package ceilings, NOT an unmeasured smaller-profile ratchet.
const RAW_LIMIT = 7_900_000, GZIP_LIMIT = 3_400_000;

export function assertProfileSizes(render, full) {
  for (const [profile, sizes] of [["render", render], ["full", full]]) {
    for (const [name, limit] of [["rawBytes", RAW_LIMIT], ["gzipBytes", GZIP_LIMIT]]) {
      assert(Number.isSafeInteger(sizes[name]) && sizes[name] > 0 && sizes[name] <= limit,
        `${profile} ${name} exceeds the existing package ceiling or is invalid`);
    }
  }
  assert(render.rawBytes < full.rawBytes, "render-only raw size did not improve over this full build");
  assert(render.gzipBytes < full.gzipBytes, "render-only gzip size did not improve over this full build");
}

async function executeProfile(directory, profile, measurement) {
  const base = path.resolve(directory), bytes = await fs.readFile(path.join(base, "pkg/franken_markdown_bg.wasm"));
  Object.assign(measurement, { rawBytes: bytes.length, gzipBytes: gzipSync(bytes, { level: 9 }).length,
    wasmSha256: hash(bytes), passed: false });
  const module = await WebAssembly.compile(bytes);
  const exports = WebAssembly.Module.exports(module);
  assert(exports.some(item => item.name === "memory" && item.kind === "memory"), "missing native WASM memory");
  assert.equal(exports.some(item => /fmdflowsession/i.test(item.name)), profile === "full",
    `wrong native editor exports in ${profile} binary`);
  const bindings = await import(pathToFileURL(path.join(base, "pkg/franken_markdown.js")).href);
  await bindings.default({ module_or_path: bytes });
  const core = probeFlowProfile(bindings, profile);
  // Also prove that the public facade actually links and renders with the
  // selected ABI, not merely that isolated generated entrypoints exist.
  const api = await import(pathToFileURL(path.join(base, "franken_markdown.js")).href);
  await api.init(bytes);
  const publicOutputs = [];
  for (const format of ["html", "pdf"]) {
    const output = await (format === "html" ? api.renderHtml : api.renderPdf)(
      "# Public profile\n\nA **shared core** with a table.\n\n| A | B |\n|---|---|\n| One | Two |",
      { title: "Profile contract", ...(format === "pdf" ? { metadataEpochSeconds: 0 } : {}) },
    );
    assert.equal(output.format, format);
    assert.equal(output.mimeType, format === "pdf" ? "application/pdf" : "text/html; charset=utf-8");
    assert(output.bytes instanceof Uint8Array && output.bytes.length > 0, "empty public output");
    publicOutputs.push(output.bytes);
  }
  Object.assign(measurement, { passed: true, coreOutputs: core.map(fingerprint), publicOutputs: publicOutputs.map(fingerprint) });
  return { core, publicOutputs };
}

/** Writes one create-only report, including failures and measured sizes. A PASS
 * requires real ABI checks, real renders, byte parity and a same-run size win.
 * This does not replace native-vs-WASM, browser, accessibility or visual gates.
 */
export async function compareProfilePackages(renderDirectory, fullDirectory, reportPath, sourceCommit) {
  assert(/^[0-9a-f]{40}$/.test(sourceCommit), "expected the exact source Git commit");
  assert.notEqual(await fs.realpath(renderDirectory), await fs.realpath(fullDirectory), "profiles must be separate builds");
  const report = { schema: "fmd-wasm-profile-comparison-v1", sourceCommit, node: process.version,
    limits: { rawBytes: RAW_LIMIT, gzipBytes: GZIP_LIMIT }, render: {}, full: {}, errors: [], passed: false };
  const results = {};
  for (const [profile, directory] of [["render", renderDirectory], ["full", fullDirectory]]) {
    try { results[profile] = await executeProfile(directory, profile, report[profile]); }
    catch (error) { report.errors.push({ profile, message: String(error.message).slice(0, 4096) }); }
  }
  if (results.render && results.full) {
    try {
      assertProfileParity(results.render.core, results.full.core);
      assert.equal(results.render.publicOutputs.length, 2);
      assert.deepEqual(results.render.publicOutputs, results.full.publicOutputs, "public profile outputs differ");
      assertProfileSizes(report.render, report.full);
      report.passed = true;
    } catch (error) { report.errors.push({ profile: "comparison", message: String(error.message).slice(0, 4096) }); }
  }
  if (Number.isSafeInteger(report.render.rawBytes) && Number.isSafeInteger(report.full.rawBytes)) {
    report.savedBytes = { raw: report.full.rawBytes - report.render.rawBytes,
      gzip: report.full.gzipBytes - report.render.gzipBytes };
  }
  await fs.writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
  if (!report.passed) throw new Error(`WASM profile comparison failed; see ${reportPath}`);
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [render, full, report, sourceCommit, ...extra] = process.argv.slice(2);
  if (!render || !full || !report || !sourceCommit || extra.length) {
    console.error("Usage: node wasm/compare_profiles.mjs RENDER_PACKAGE FULL_PACKAGE REPORT_JSON SOURCE_COMMIT");
    process.exitCode = 64;
  } else {
    try {
      const value = await compareProfilePackages(render, full, report, sourceCommit);
      console.log(JSON.stringify({ passed: value.passed, savedBytes: value.savedBytes, report }));
    } catch (error) { console.error(error.message); process.exitCode = 1; }
  }
}
