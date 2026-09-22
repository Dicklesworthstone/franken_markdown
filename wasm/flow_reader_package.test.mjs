// Public imports and package assembly inventory, not a generated-WASM proof.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { FlowReaderView, readFlowDocument } from "@franken-suite/franken-markdown/flow-reader";
import { ReadingSession } from "./tests/flow_reading_fixtures.mjs";

const manifest = JSON.parse(await readFile(new URL("./package.json", import.meta.url), "utf8"));
const runtime = ["flow-reader.js", "flow-reader.d.ts", "flow_reading.mjs", "READER.md"];
test("public package export loads without DOM/WASM side effects and searches engine data", async () => {
  const document = await readFlowDocument(new ReadingSession());
  assert.equal(document.find("text").matches.length, 1);
  assert.equal(typeof FlowReaderView, "function");
  assert.deepEqual(manifest.exports["./flow-reader"], {
    types: "./flow-reader.d.ts",
    import: "./flow-reader.js",
  });
});
test("manifest ships all reading runtime and live-control dependencies", async () => {
  for (const file of [...runtime, "demo/flow_reading_controls.mjs"]) {
    assert(manifest.files.includes(file), `unshipped ${file}`);
    assert((await readFile(new URL(file, import.meta.url))).length > 0);
  }
  for (const path of [
    "flow-reader.js",
    "demo/flow_reading_controls.mjs",
    "demo/flow_preview_controller.mjs",
  ]) {
    const code = await readFile(new URL(path, import.meta.url), "utf8");
    for (const [, dependency] of code.matchAll(/from\s+"(\.{1,2}\/[^"]+)"/g)) {
      const target = new URL(dependency, new URL(path, import.meta.url)).pathname;
      const root = new URL("./", import.meta.url).pathname;
      assert(
        manifest.files.includes(target.slice(root.length)),
        `${path} has an unshipped dependency ${dependency}`,
      );
    }
  }
});
test("both assemblers ship the new public reader and the actual demo controls", async () => {
  for (const script of ["dsr-wasm-package.sh", "check-wasm-package.sh"]) {
    const source = await readFile(new URL(`../scripts/${script}`, import.meta.url), "utf8");
    const loop = source.match(/for file in ([\s\S]*?); do/)[1];
    for (const file of runtime) assert(loop.includes(file), `${script} misses ${file}`);
    assert(
      source
        .split("\n")
        .some(
          (line) => line.startsWith("cp ") && line.includes("wasm/demo/flow_reading_controls.mjs"),
        ),
      `${script} misses UI`,
    );
  }
});
