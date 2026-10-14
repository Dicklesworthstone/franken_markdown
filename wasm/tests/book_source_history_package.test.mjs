import test from "node:test";
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import vm from "node:vm";

const root = new URL("../", import.meta.url);
const modulePath = "demo/book_source_history.mjs";

test("both actual demo assembly loops ship a normally importable source-history module", async () => {
  const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  assert.equal(manifest.files.filter(file => file === modulePath).length, 1);
  const code = await readFile(new URL(modulePath, root));
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../scripts/${name}`, root), "utf8");
    const loop = script.match(/for file in book\.html[^\n]*; do\n[^\n]+\n(?:done)/)?.[0];
    assert(loop, `${name}: exact demo assembly loop must exist`);
    const names = loop.split("; do")[0].slice("for file in ".length).split(/\s+/);
    assert(names.includes("book_source_history.mjs"));
    // Execute the real copy loop, but NOT the Rust/WASM build or any cleanup.
    // Unrelated entries are explicit inert placeholders; the history module
    // is copied byte-for-byte and then imported/executed from the output tree.
    const scratch = await mkdtemp(join(tmpdir(), "fmd-history-assembly-"));
    await mkdir(join(scratch, "wasm/demo"), { recursive: true });
    await mkdir(join(scratch, "package/demo"), { recursive: true });
    for (const file of names) await writeFile(join(scratch, "wasm/demo", file), file === "book_source_history.mjs" ? code : "// assembly fixture\n");
    execFileSync("bash", ["-eu", "-c", loop], { cwd: scratch,
      env: { ...process.env, package_dir: join(scratch, "package"), PACKAGE: join(scratch, "package") }, timeout: 5000 });
    const output = join(scratch, "package", modulePath);
    assert.deepEqual(await readFile(output), code);
    const { createBookSourceHistory } = await import(pathToFileURL(output));
    const history = createBookSourceHistory([{ path: "a.md", source: "a" }]);
    history.record([{ path: "a.md", source: "b" }]);
    const plan = history.plan("undo"); history.commit(plan, plan.files);
    assert.equal(plan.files[0].source, "a"); assert.equal(history.state.redo, 1);
    history.dispose();
  }
});

async function startup(fail = false) {
  const source = await readFile(new URL("demo/book.js", root), "utf8");
  const body = source.replace(/^import\s+[\s\S]*?from\s+"[^"\n]+";\n/gm, "");
  const calls = [], events = new Map(), nodes = new Map();
  const document = { querySelector(id) { if (!nodes.has(id)) nodes.set(id, { textContent: "" }); return nodes.get(id); } };
  const collection = {}, host = { suspend() { calls.push("host.suspend"); }, dispose() { calls.push("host.dispose"); } };
  const empty = () => ({ suspend() {}, resume() {}, dispose() {}, detach() {} });
  let hostOptions;
  const globals = { document, window: { confirm: () => true, addEventListener(type, fn) { events.set(type, fn); } },
    createBookCollection: () => collection,
    createBookControls: options => { hostOptions = options; return host; },
    createBookWorker: () => { calls.push("worker"); return empty(); },
    createBookSourceHistoryControls(options) {
      assert.equal(options.collection, collection); assert.equal(options.controls, host);
      assert.equal(options.root, document); assert.equal(Object.hasOwn(options, "worker"), false);
      calls.push("history.create");
      if (fail) throw Error("explicit history initialization fixture failure");
      return { suspend() { calls.push("history.suspend"); }, resume() { calls.push("history.resume"); },
        dispose() { calls.push("history.dispose"); }, reset() { calls.push("history.reset"); } };
    },
  };
  for (const name of ["createBookImageControls", "createBookFontControls", "createBookLibraryControls", "createBookPdfControls",
    "createBookPublicationControls", "createBookPreviewControls", "createBookSearchControls", "createBookInspectionControls", "createBookLinkControls"])
    globals[name] = empty;
  for (const name of ["createBookImagePanel", "createBookFontPanel", "createBookPdfPanel", "createBookInspectionPanel", "createBookLinkPanel"])
    globals[name] = () => {};
  // Execute the complete application orchestration body with named factory
  // doubles. This verifies ownership/wiring, not normal browser/WASM startup.
  vm.runInNewContext(body, globals, { timeout: 1000 });
  return { calls, events, nodes, hostOptions };
}

test("real application orchestration brackets host lifetime and resets successful imports", async () => {
  const h = await startup();
  assert.equal(h.calls.filter(call => call === "worker").length, 6);
  h.hostOptions.onProjectReplaced(); assert(h.calls.includes("history.reset"));
  h.events.get("pagehide")({ persisted: true });
  assert(h.calls.indexOf("history.suspend") < h.calls.indexOf("host.suspend"));
  h.events.get("pageshow")({ persisted: true }); assert(h.calls.includes("history.resume"));
  h.events.get("pagehide")({ persisted: false });
  assert(h.calls.indexOf("history.dispose") < h.calls.indexOf("host.dispose"));
});

test("optional history initialization failure leaves publication and host lifecycle available", async () => {
  const h = await startup(true);
  assert.match(h.nodes.get("#book-history-status").textContent, /Source editing, replacement-specific undo, saves and publication remain available/);
  assert.equal(h.calls.filter(call => call === "worker").length, 6);
  h.hostOptions.onProjectReplaced(); h.events.get("pagehide")({ persisted: false });
  assert(h.calls.includes("host.dispose"));
});
