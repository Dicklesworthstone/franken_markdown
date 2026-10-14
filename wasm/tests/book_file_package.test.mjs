import test from "node:test";
import assert from "node:assert/strict";
import { readFile, mkdir, mkdtemp, copyFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";

const root = new URL("../", import.meta.url);
const modules = ["book_file_session.mjs", "book_file_controls.mjs"];

test("manifest and both assembler lists retain the file-session dependency closure", async () => {
  const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  for (const name of modules) assert.equal(manifest.files.filter(path => path === `demo/${name}`).length, 1);
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../scripts/${name}`, root), "utf8");
    const list = script.match(/for file in book\.html ([^\n]*); do/);
    assert(list, `${name}: demo assembly list`);
    for (const name of modules) assert(list[1].split(/\s+/).includes(name));
    assert(script.includes("node --test wasm/tests/book_file_*.test.mjs"));
  }
  // This is a metadata guard, not a claim that the native build was executed.
});

test("copied runtime modules load by normal ESM imports and execute file-session cancellation", async () => {
  const directory = await mkdtemp(join(tmpdir(), "fmd-file-package-"));
  await mkdir(join(directory, "demo"));
  for (const name of modules) {
    const source = new URL(`demo/${name}`, root), destination = join(directory, "demo", name);
    await copyFile(source, destination);
    assert.deepEqual(await readFile(destination), await readFile(source));
  }
  const ui = await import(pathToFileURL(join(directory, "demo/book_file_controls.mjs")));
  const core = await import(pathToFileURL(join(directory, "demo/book_file_session.mjs")));
  assert.throws(() => ui.createBookFileControls({}), TypeError);
  const session = core.createBookFileSession({
    read: () => ({ source: '{"schemaVersion":2,"files":[],"options":{}}\n', key: "empty", stamp: "0" }),
    decode() { throw Error("Cancelled picker must not decode a file"); },
    replace() { throw Error("Cancelled picker must not install a project"); },
  });
  await assert.rejects(session.open(() => { throw new DOMException("cancelled", "AbortError"); }), { code: "FILE_CANCELLED" });
  assert.equal(session.state.filename, null); assert.equal(session.state.busy, null); session.dispose();
  // Only this feature's complete runtime closure was copied, not a WASM package.
});

test("bootstrap assigns the file-aware source host before library/history consumers and releases it first", async () => {
  const source = await readFile(new URL("demo/book.js", root), "utf8");
  assert(source.includes('from "./book_file_controls.mjs"'));
  const binding = source.indexOf("controls = projectFiles.controls");
  assert(binding > source.indexOf("projectFiles = createBookFileControls"));
  assert(binding < source.indexOf("sourceHistory = createBookSourceHistoryControls"));
  assert(binding < source.indexOf("library = createBookLibraryControls"));
  assert(source.includes("projectCodec: { normalizeBookProject, readBookProject, serializeBookProject }"));
  assert(source.includes("onProjectReplaced: () => { projectFiles?.detach(); library?.detach(); sourceHistory?.reset(); }"));
  for (const action of ["suspend", "dispose", "resume"])
    assert(source.indexOf(`projectFiles?.${action}()`) < source.indexOf(`sourceHistory?.${action}()`));
  // Orchestration metadata only; browser DOM contracts have a separate runner.
});
