// Dependency and inventory checks, not browser or native renderer acceptance.
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const root = new URL("../../", import.meta.url);
const read = path => readFile(new URL(path, root), "utf8");
const demo = ["flow-source.js", "flow_document.mjs", "flow_draft_store.mjs", "flow_draft_session.mjs", "flow_draft_controls.mjs"];
test("manifest and both assemblers ship the complete source/recovery entrypoint", async () => {
  const manifest = JSON.parse(await read("wasm/package.json"));
  assert(manifest.sideEffects.includes("./demo/flow-source.js"));
  assert(manifest.files.includes("SOURCE.md")); assert(await read("wasm/SOURCE.md"));
  for (const name of demo) { assert(manifest.files.includes(`demo/${name}`)); assert(await read(`wasm/demo/${name}`)); }
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await read(`scripts/${name}`);
    const loops = [...script.matchAll(/for file in ([\s\S]*?); do\n([^]*?)\ndone/g)];
    assert(loops.some(loop => loop[1].split(/\s+/).includes("SOURCE.md") && loop[2].includes('cp "wasm/$file"')));
    for (const file of demo) assert(loops.some(loop => loop[1].split(/\s+/).includes(file) && loop[2].includes('cp "wasm/demo/$file"')));
  }
});
test("source controls have a separate script and their import graph never reaches WASM or preview code", async () => {
  const html = await read("wasm/demo/flow-canvas.html"), source = await read("wasm/demo/flow-source.js");
  assert(html.includes('<script type="module" src="./flow-source.js"></script>'));
  assert(html.includes('<script type="module" src="./flow-canvas.js"></script>'));
  for (const match of source.matchAll(/querySelector\("#([^\"]+)"\)/g)) assert(html.includes(`id="${match[1]}"`), match[1]);
  const visited = new Set();
  async function inspect(path) {
    if (visited.has(path)) return;
    visited.add(path);
    assert(!/pkg|worker|canvas|franken_markdown\.js/.test(path), path);
    const code = await read(path);
    for (const match of code.matchAll(/^import .*? from "([^"]+)";/gm)) {
      assert(match[1].startsWith("."));
      const next = new URL(match[1], new URL(path, root));
      assert(next.href.startsWith(root.href)); await inspect(next.href.slice(root.href.length));
    }
  }
  await inspect("wasm/demo/flow-source.js");
  assert(visited.has("wasm/flow_session.mjs")); assert.equal(visited.size, 6);
});
test("source replacement revokes native image ownership and image insertion notifies source observers", async () => {
  const code = await read("wasm/demo/flow-canvas.js");
  const handler = code.slice(code.indexOf('source.addEventListener("fmd-document-replaced"'), code.indexOf('source.addEventListener("input", update)'));
  assert(handler.includes('files.value = ""')); assert(handler.includes("exportControls?.invalidate()"));
  assert(handler.includes("changeImages(createLocalImageSources([]))"));
  assert(code.includes('source.dispatchEvent(new Event("input", { bubbles: true }))'));
});
