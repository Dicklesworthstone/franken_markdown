// Packaging checks, not a generated-WASM execution claim.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("both assemblers and the manifest ship the actual export dependency and execute its WASM proof", async () => {
  const manifest = JSON.parse(await readFile(new URL("./package.json", import.meta.url), "utf8"));
  for (const file of ["flow_export.mjs", "EXPORT.md"]) {
    assert(manifest.files.includes(file));
    assert((await readFile(new URL(file, import.meta.url))).length);
  }
  for (const file of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../scripts/${file}`, import.meta.url), "utf8");
    const files = script.match(/for file in ([\s\S]*?); do/)[1].split(/\s+/);
    assert(files.includes("flow_export.mjs"));
    assert(files.includes("EXPORT.md"));
    assert(script.includes("node wasm/flow_export_smoke.mjs"));
  }
});
