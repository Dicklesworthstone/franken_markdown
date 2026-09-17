// Package shipping checks, not generated-WASM or Rust execution proof.
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { FlowImageAssets, FlowAssetError } from "@franken-suite/franken-markdown/flow-assets";
import { Session, png, image } from "./tests/flow_image_fixtures.mjs";
const manifest = JSON.parse(await readFile(new URL("./package.json", import.meta.url), "utf8"));
const runtime = ["flow-assets.js", "flow-assets.d.ts", "flow_raster.mjs", "ASSETS.md"];

test("public package subpath imports and performs dimension-only image delivery", async () => {
  const session = new Session();
  const assets = new FlowImageAssets(session, { load: () => png(), decode: () => image() });
  try {
    assert.equal((await assets.loadPending()).loaded, 1);
    assert.equal(session.writes[0].bytes, undefined);
    assert.equal(new FlowAssetError("TEST", "test").code, "TEST");
  } finally { assets.dispose(); }
});

test("manifest ships the image API, decoder seam, local grant helper and integration", async () => {
  assert.deepEqual(manifest.exports["./flow-assets"], { types: "./flow-assets.d.ts", import: "./flow-assets.js" });
  for (const file of [...runtime, "demo/local_image_sources.mjs", "demo/flow_preview_controller.mjs", "demo/flow-canvas.js", "demo/flow-canvas.html"]) {
    assert(manifest.files.includes(file), `missing manifest file ${file}`);
    assert((await readFile(new URL(file, import.meta.url))).length > 0, `empty file ${file}`);
  }
  const module = await readFile(new URL("./flow-assets.js", import.meta.url), "utf8");
  for (const [, dependency] of module.matchAll(/from\s+"\.\/([^"]+)"/g)) {
    assert(manifest.files.includes(dependency), `unshipped runtime dependency ${dependency}`);
  }
});

test("both package assemblers copy the image runtime and explicit local-file helper", async () => {
  for (const script of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const source = await readFile(new URL(`../scripts/${script}`, import.meta.url), "utf8");
    const list = source.match(/for file in ([\s\S]*?); do/)[1];
    for (const file of runtime) assert(list.includes(file), `${script} does not assemble ${file}`);
    assert(source.split("\n").some(line => line.startsWith("cp ") && line.includes("wasm/demo/local_image_sources.mjs")), `${script} misses helper`);
  }
});
