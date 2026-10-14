import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("../../", import.meta.url));
const manifest = JSON.parse(readFileSync(join(root, "wasm/package.json"), "utf8"));
const required = ["book-worker.js", "book-worker.d.ts", "book_publication.mjs", "BOOK_PUBLICATION.md"];

test("existing book-worker package entry includes the additive publication API and runtime", () => {
  assert.equal(manifest.exports["./book-worker"].import, "./book-worker.js");
  assert.equal(manifest.exports["./book-worker"].types, "./book-worker.d.ts");
  for (const name of required) assert.ok(manifest.files.includes(name), `not shipped: ${name}`);
  const declaration = readFileSync(join(root, "wasm/book-worker.d.ts"), "utf8");
  for (const name of ["createBookWorker", "createBookPublicationWorker", "BookPublicationWorker", "BookPublicationOutput"])
    assert.match(declaration, new RegExp(`export (?:function|interface) ${name}\\b`));
});

test("actual public ESM facade executes against explicit capture/transport adapters", () => {
  const stdout = execFileSync(process.execPath, ["--experimental-vm-modules",
    fileURLToPath(new URL("./book_publication_facade.mjs", import.meta.url))], { encoding: "utf8" });
  const result = JSON.parse(stdout);
  assert.equal(result.checks.length, 12);
});

for (const [script, destination] of [["check-wasm-package.sh", "package_dir"], ["dsr-wasm-package.sh", "PACKAGE"]]) {
  test(`${script} copies the actual publication module and runs its regression gate`, async () => {
    const source = readFileSync(join(root, "scripts", script), "utf8");
    // Execute only the existing package-copy loop with explicit fixture files.
    // No Cargo/build/cleanup command or native budget gate is executed here.
    const match = /^for file in ([\s\S]+?); do\n  cp "wasm\/\$file" "\$[^"\n]+"\ndone/m.exec(source);
    assert.ok(match, "runtime package loop must be identifiable");
    assert.match(source, /node --test wasm\/tests\/book_publication\*\.test\.mjs/);
    const names = match[1].replace(/\\\n/g, " ").split(/\s+/).filter(Boolean);
    for (const name of required) assert.ok(names.includes(name), `copy loop omits ${name}`);
    const fixture = mkdtempSync(join(tmpdir(), "fmd-publication-package-"));
    const packageDir = join(fixture, "package");
    mkdirSync(join(fixture, "wasm")); mkdirSync(packageDir);
    for (const name of names) writeFileSync(join(fixture, "wasm", name),
      required.includes(name) ? readFileSync(join(root, "wasm", name)) : `fixture: ${name}\n`);
    execFileSync("bash", ["-c", `set -eu\n${match[0]}`], {
      cwd: fixture, env: { ...process.env, [destination]: packageDir },
    });
    for (const name of required)
      assert.deepEqual(readFileSync(join(packageDir, name)), readFileSync(join(root, "wasm", name)));
    const { createBookPublicationClient } = await import(pathToFileURL(join(packageDir, "book_publication.mjs")));
    const publisher = createBookPublicationClient({
      captureInput: (files, options) => ({ files, options }),
      createClient: () => ({
        render: (_files, format) => ({ format: `book-${format}`, bytes: Uint8Array.of(7), sourceLength: 7, retainedInputRevision: 1 }),
        renderSourceUpdate() { throw new Error("one format only"); }, dispose() {},
      }),
    });
    assert.equal((await publisher.render([{ path: "a.md", source: "# Guide" }], ["pdf"])).byteLength, 1);
    publisher.dispose();
  });
}
