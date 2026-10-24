import assert from "node:assert/strict";
import test from "node:test";
import { readFile, writeFile, mkdir, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import vm from "node:vm";

const root = new URL("../", import.meta.url);
const runtime = ["book_source_formatting.mjs", "source_formatting.mjs", "source_commands.mjs"];
test("both actual assembly loops ship the complete importable formatting module graph", async () => {
  const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  for (const file of runtime) assert.equal(manifest.files.filter(path => path === `demo/${file}`).length, 1);
  for (const scriptName of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../scripts/${scriptName}`, root), "utf8");
    const loop = script.match(/for file in book\.html[^\n]*; do\n[^\n]+\n(?:done)/)?.[0];
    assert(loop, `${scriptName}: real copy loop must exist`);
    const names = loop.split("; do")[0].slice("for file in ".length).split(/\s+/);
    for (const file of runtime) assert(names.includes(file), `${scriptName}: missing ${file}`);
    const scratch = await mkdtemp(join(tmpdir(), "fmd-book-format-assembly-"));
    await mkdir(join(scratch, "wasm/demo"), { recursive: true });
    await mkdir(join(scratch, "package/demo"), { recursive: true });
    // Execute the real copy loop, not its build/cleanup commands. Unrelated
    // entries are inert; every module in this feature's import graph is real.
    for (const file of names) await writeFile(join(scratch, "wasm/demo", file), runtime.includes(file)
      ? await readFile(new URL(`demo/${file}`, root)) : "// unrelated assembly fixture\n");
    execFileSync("bash", ["-eu", "-c", loop], { cwd: scratch, timeout: 5000,
      env: { ...process.env, package_dir: join(scratch, "package"), PACKAGE: join(scratch, "package") } });
    for (const file of runtime) assert.deepEqual(await readFile(join(scratch, "package/demo", file)), await readFile(new URL(`demo/${file}`, root)));
    const module = await import(pathToFileURL(join(scratch, "package/demo/book_source_formatting.mjs")));
    assert.equal(typeof module.createBookSourceFormattingControls, "function");
    const planner = await import(pathToFileURL(join(scratch, "package/demo/source_commands.mjs")));
    assert.equal(planner.planSourceCommand("word", { start: 0, end: 4 }, "bold").text, "**word**");
  }
});

async function startup(fail = false) {
  const source = await readFile(new URL("demo/book.js", root), "utf8");
  const body = source.replace(/^import\s+[\s\S]*?from\s+"[^"\n]+";\n/gm, "");
  const calls = [], events = new Map(), nodes = new Map(), collection = {};
  const document = { querySelector(id) { if (!nodes.has(id)) nodes.set(id, { textContent: "" }); return nodes.get(id); } };
  const host = { suspend() { calls.push("host.suspend"); }, dispose() { calls.push("host.dispose"); } };
  const empty = () => ({ suspend() {}, resume() {}, dispose() {}, detach() {} });
  const globals = { document, window: { confirm: () => true, addEventListener(type, fn) { events.set(type, fn); } },
    createBookCollection: () => collection, createBookControls: () => host,
    createBookWorker: () => { calls.push("worker"); return empty(); },
    createBookSourceFormattingControls(options) {
      assert.equal(options.collection, collection); assert.equal(options.controls, host); assert.equal(options.root, document);
      assert.equal(Object.hasOwn(options, "worker"), false); calls.push("formatting.create");
      if (fail) throw Error("Explicit formatting factory failure");
      return { suspend() { calls.push("formatting.suspend"); }, resume() { calls.push("formatting.resume"); }, dispose() { calls.push("formatting.dispose"); } };
    },
  };
  for (const name of ["createBookImageControls", "createBookFontControls", "createBookLibraryControls", "createBookPdfControls",
    "createBookPublicationControls", "createBookPreviewControls", "createBookSearchControls", "createBookInspectionControls",
    "createBookLinkControls", "createBookSourceHistoryControls"]) globals[name] = empty;
  for (const name of ["createBookImagePanel", "createBookFontPanel", "createBookPdfPanel", "createBookInspectionPanel", "createBookLinkPanel"])
    globals[name] = () => {};
  // Named factory adapters qualify application ownership, not real DOM or WASM.
  // The standalone .browser.mjs suite executes the unmodified module graph.
  vm.runInNewContext(body, globals, { timeout: 1000 });
  return { calls, events, nodes };
}
test("application brackets source formatting within the host lifetime", async () => {
  const h = await startup(); assert.equal(h.calls.filter(call => call === "formatting.create").length, 1);
  h.events.get("pagehide")({ persisted: true });
  assert(h.calls.indexOf("formatting.suspend") < h.calls.indexOf("host.suspend"));
  h.events.get("pageshow")({ persisted: true }); assert(h.calls.includes("formatting.resume"));
  h.events.get("pagehide")({ persisted: false });
  assert(h.calls.indexOf("formatting.dispose") < h.calls.indexOf("host.dispose"));
});
test("optional formatting initialization failure preserves the rest of startup", async () => {
  const h = await startup(true);
  assert.match(h.nodes.get("#status").textContent, /formatting is unavailable.*Ordinary source editing/);
  assert.equal(h.calls.filter(call => call === "worker").length, 6);
  h.events.get("pagehide")({ persisted: false }); assert(h.calls.includes("host.dispose"));
});
