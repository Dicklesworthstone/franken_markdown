import test from "node:test";
import assert from "node:assert/strict";
import { readFile, mkdir, mkdtemp, copyFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const root = new URL("../", import.meta.url);
const modules = ["book_directory_session.mjs", "book_directory_controls.mjs"];

test("both assemblers and the manifest ship the complete folder runtime and its guide", async () => {
  const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  for (const path of [...modules.map(name => `demo/${name}`), "BOOK_DIRECTORY.md"])
    assert.equal(manifest.files.filter(file => file === path).length, 1, path);
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../scripts/${name}`, root), "utf8");
    const list = script.match(/for file in book\.html ([^\n]*); do/);
    assert(list, `${name}: demo assembly list`);
    for (const module of modules) assert(list[1].split(/\s+/).includes(module));
    assert(script.includes("BOOK_EDITING.md BOOK_DIRECTORY.md INSPECTION.md"));
    assert(script.includes("node --test wasm/tests/book_directory_*.test.mjs"));
    assert(script.includes("node --test wasm/tests/book_file_*.test.mjs"));
    assert(script.includes("node --test wasm/tests/book_source_history*.test.mjs"));
  }
});

test("the copied runtime loads normally without WASM, rendering or source-host substitutes", async () => {
  const output = await mkdtemp(join(tmpdir(), "fmd-directory-package-"));
  await mkdir(join(output, "demo"));
  for (const name of modules) {
    const source = new URL(`demo/${name}`, root), target = join(output, "demo", name);
    await copyFile(source, target); assert.deepEqual(await readFile(source), await readFile(target));
  }
  const core = await import(pathToFileURL(join(output, "demo/book_directory_session.mjs")));
  const ui = await import(pathToFileURL(join(output, "demo/book_directory_controls.mjs")));
  assert.throws(() => ui.createBookDirectoryControls({}), TypeError);
  const session = core.createBookDirectorySession({ read: () => ({ files: [{ path: "a.md", source: "A" }], stamp: "0" }) });
  await assert.rejects(session.bind(() => { throw new DOMException("cancelled", "AbortError"); }), { code: "DIRECTORY_CANCELLED" });
  assert.equal(session.state.folder, null); assert.equal(session.state.busy, null); session.dispose();
  // This copies only the feature's actual dependency closure, not a WASM build.
});

test("bootstrap installs folder ownership with the file-aware host and retires it before source disposal", async () => {
  const source = await readFile(new URL("demo/book.js", root), "utf8");
  assert(source.includes('from "./book_directory_controls.mjs"'));
  const at = source.indexOf("sourceDirectory = createBookDirectoryControls");
  assert(at > source.indexOf("controls = projectFiles.controls"));
  assert(at < source.indexOf("sourceHistory = createBookSourceHistoryControls"));
  for (const action of ["suspend", "dispose", "resume"]) {
    const position = source.indexOf(`sourceDirectory?.${action}()`);
    assert(position > 0); assert(position < source.indexOf(`projectFiles?.${action}()`));
  }
  // Preserve the existing file/library/history identity callback contract.
  assert(source.includes("onProjectReplaced: () => { projectFiles?.detach(); library?.detach(); sourceHistory?.reset(); }"));
  const ui = await readFile(new URL("demo/book_directory_controls.mjs", root), "utf8");
  assert(ui.includes("epoch !== projectEpoch"));
  assert(ui.includes("session.detach()"));
});
