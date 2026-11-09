import assert from "node:assert/strict";
import test from "node:test";
import { readFile, mkdtemp, mkdir, copyFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { codec, exportedFolder, sample } from "./book_source_folder_helpers.mjs";

const root = new URL("../", import.meta.url);
const modules = ["book_source_folder.mjs", "book_source_folder_controls.mjs", "book_source_archive.mjs", "book_zip.mjs"];
for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
  test(`${name} stages and loads the complete source-folder runtime without WASM`, async () => {
    const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
    const script = await readFile(new URL(`../scripts/${name}`, root), "utf8");
    const loop = script.match(/for file in book\.html ([^\n]*); do/);
    assert(loop, "Missing book demo assembly loop");
    const copied = new Set(["book.html", ...loop[1].split(/\s+/)]);
    assert(script.includes("node --test wasm/tests/book_source_folder*.test.mjs"));
    const stage = await mkdtemp(join(tmpdir(), "fmd-source-folder-package-"));
    await mkdir(join(stage, "demo"));
    for (const module of modules) {
      assert.equal(manifest.files.filter(file => file === `demo/${module}`).length, 1, module);
      assert(copied.has(module), `${name} omits ${module}`);
      await copyFile(new URL(`demo/${module}`, root), join(stage, "demo", module));
    }
    const runtime = await import(pathToFileURL(join(stage, "demo/book_source_folder.mjs")));
    const view = await import(pathToFileURL(join(stage, "demo/book_source_folder_controls.mjs")));
    assert.throws(() => view.createBookSourceFolderControls({}), TypeError);
    const result = await runtime.readBookSourceFolder(await exportedFolder(), { projectCodec: codec });
    assert.deepEqual(result.project, codec.normalizeBookProject(sample()));
  });
}
