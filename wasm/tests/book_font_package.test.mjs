import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, mkdtempSync, mkdirSync, copyFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, posix } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

// Helper/package/bootstrap checks, not a generated WASM or font parser gate.
// Scratch directories are retained rather than recursively deleted.
const wasm = fileURLToPath(new URL("../", import.meta.url));
const repo = dirname(wasm.replace(/[\\/]$/, ""));
const read = path => readFileSync(path, "utf8");
const required = ["demo/book_font_controls.mjs", "demo/book_font_authoring.mjs", "demo/book_font_assets.mjs"];
const scripts = ["check-wasm-package.sh", "dsr-wasm-package.sh"];
function copied(script) {
  const paths = new Set();
  const loops = /for file in ([\s\S]*?); do\s+cp "wasm\/(demo\/)?\$file" "\$(?:PACKAGE|package_dir)\/(?:demo\/)?\$file"\s+done/g;
  for (const match of script.matchAll(loops)) {
    const prefix = match[2] ?? "";
    for (const name of match[1].replace(/\\\n/g, " ").trim().split(/\s+/)) paths.add(prefix + name);
  }
  return paths;
}

test("font authoring's complete static helper closure and guide are publishable", () => {
  const manifest = JSON.parse(read(join(wasm, "package.json")));
  for (const path of [...required, "BOOK_FONTS.md"]) assert(manifest.files.includes(path), `manifest omits ${path}`);
  assert.equal(new Set(manifest.files).size, manifest.files.length);
  for (const path of required) for (const match of read(join(wasm, path)).matchAll(/^\s*import\s+[^;]+?\s+from\s+["'](\.[^"']+)["']/gm)) {
    const dependency = posix.normalize(posix.join(posix.dirname(path), match[1]));
    assert(required.includes(dependency), `unexamined runtime dependency: ${dependency}`);
  }
});
for (const name of scripts) test(`${name} stages and loads the authoring dependency graph`, async () => {
  const script = read(join(repo, "scripts", name)), includes = copied(script);
  for (const path of [...required, "BOOK_FONTS.md"]) assert(includes.has(path), `${name} omits ${path}`);
  for (const testFile of ["book_font_authoring", "book_font_controls", "book_font_package"])
    assert(script.includes(`wasm/tests/${testFile}.test.mjs`), `${name} does not run ${testFile}`);
  const stage = mkdtempSync(join(tmpdir(), "fmd-font-package-"));
  for (const path of required) {
    mkdirSync(dirname(join(stage, path)), { recursive: true }); copyFileSync(join(wasm, path), join(stage, path));
  }
  const controls = await import(pathToFileURL(join(stage, required[0])));
  assert.equal(typeof controls.createBookFontControls, "function");
  assert.equal(typeof controls.createBookFontPanel, "function");
});

async function bootstrap(failFonts) {
  const stage = mkdtempSync(join(tmpdir(), "fmd-font-bootstrap-"));
  mkdirSync(join(stage, "demo")); writeFileSync(join(stage, "package.json"), '{"type":"module"}');
  const source = read(join(wasm, "demo/book.js"));
  copyFileSync(join(wasm, "demo/book.js"), join(stage, "demo/book.js"));
  const configs = {}, events = {}, workers = [], calls = [], statuses = new Map();
  const collection = {}, controls = {};
  const lifecycle = name => Object.fromEntries(["suspend", "resume", "dispose", "detach"].map(method => [method, () => calls.push(`${name}.${method}`)]));
  Object.assign(controls, lifecycle("controls"));
  const stubs = {
    createBookWorker() { const worker = { id: workers.length, ...lifecycle(`worker${workers.length}`) }; workers.push(worker); return worker; },
    createBookCollection() { return collection; },
    createBookControls(options) { configs.controls = options; return controls; },
  };
  for (const match of source.matchAll(/import\s*\{([^}]+)\}\s*from\s*["']([^"']+)["']/g)) {
    const names = match[1].split(",").map(x => x.trim()).filter(Boolean);
    for (const name of names) if (!stubs[name]) stubs[name] = options => {
      configs[name] = options;
      if (name === "createBookFontControls" && failFonts) throw Error("font initialization failed");
      return lifecycle(name);
    };
    const destination = join(stage, "demo", match[2]); mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, names.map(name => `export const ${name} = globalThis.__fmdFontBootstrap.${name};`).join("\n"));
  }
  const prior = ["window", "document", "__fmdFontBootstrap"].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]);
  globalThis.__fmdFontBootstrap = stubs;
  globalThis.window = { addEventListener(name, fn) { events[name] = fn; }, confirm: () => true };
  globalThis.document = { querySelector(id) { if (!statuses.has(id)) statuses.set(id, {}); return statuses.get(id); } };
  try { await import(pathToFileURL(join(stage, "demo/book.js"))); }
  finally { for (const [key, descriptor] of prior) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else Reflect.deleteProperty(globalThis, key); } }
  return { configs, events, workers, calls, statuses, collection, controls };
}

test("publisher owns a separate font worker and forwards lifecycle before disposing its collection", async () => {
  const f = await bootstrap(false), fonts = f.configs.createBookFontControls;
  assert(fonts, "publisher never initializes font authoring");
  assert.equal(fonts.collection, f.collection); assert.equal(fonts.controls, f.controls);
  for (const key of ["controls", "createBookPdfControls", "createBookPreviewControls", "createBookInspectionControls", "createBookLinkControls"])
    assert.notEqual(fonts.worker, f.configs[key].worker, `font worker is shared with ${key}`);
  f.events.pagehide({ persisted: true });
  assert(f.calls.indexOf("createBookFontControls.suspend") < f.calls.indexOf("controls.suspend"));
  f.events.pageshow({ persisted: true }); assert(f.calls.includes("createBookFontControls.resume"));
  f.events.pagehide({ persisted: false });
  assert(f.calls.indexOf("createBookFontControls.dispose") < f.calls.indexOf("controls.dispose"));
});

test("font initialization failure disposes its client without disabling other publisher features", async () => {
  const f = await bootstrap(true), fonts = f.configs.createBookFontControls;
  assert(fonts, "publisher never attempts font authoring");
  assert(f.calls.includes(`worker${fonts.worker.id}.dispose`));
  assert(!f.calls.includes(`worker${f.configs.controls.worker.id}.dispose`));
  assert(f.configs.createBookSearchControls && f.configs.createBookLinkControls);
  assert.match(f.statuses.get("#book-font-status").textContent, /unavailable/);
});
