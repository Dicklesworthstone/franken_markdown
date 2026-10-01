import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

const read = path => readFileSync(new URL(path, import.meta.url), "utf8");
const bootstrap = read("../demo/book.js");
const names = [...bootstrap.matchAll(/import\s+\{([^}]+)\}\s+from\s+"[^"]+";/g)]
  .flatMap(match => match[1].split(",").map(name => name.trim()).filter(Boolean));
function boot(failImages = false) {
  const events = new Map(), calls = [], instances = new Map(), elements = new Map();
  const root = { querySelector(id) { if (!elements.has(id)) elements.set(id, {}); return elements.get(id); } };
  const win = { confirm: () => true, addEventListener(type, fn) { events.set(type, fn); } };
  const factories = Object.fromEntries(names.map(name => [name, argument => {
    calls.push({ name, argument });
    if (name === "createBookImageControls" && failImages) throw Error("intentional initialization failure");
    const instance = Object.fromEntries(["suspend", "resume", "dispose", "detach"].map(method => [method, () => calls.push({ name: `${name}.${method}` })]));
    instances.set(name, instance);
    return instance;
  }]));
  const executable = bootstrap.replace(/import\s+\{[^}]+\}\s+from\s+"[^"]+";\s*/g, "");
  new Function("document", "window", ...names, executable)(root, win, ...names.map(name => factories[name]));
  return { events, calls, instances, elements };
}

test("publisher constructs image controls with the existing collection and source owner", () => {
  const f = boot();
  const call = f.calls.find(call => call.name === "createBookImageControls");
  assert(call, "publisher never constructs image management");
  assert.equal(call.argument.collection, f.instances.get("createBookCollection"));
  assert.equal(call.argument.controls, f.instances.get("createBookControls"));
  assert.equal(call.argument.confirm("remove?"), true);
  assert.equal(call.argument.worker, undefined, "byte authorization must not own another renderer");
  assert.equal(f.calls.filter(call => call.name === "createBookImagePanel").length, 1);
});

test("publisher suspends, resumes and disposes image management before collection ownership ends", () => {
  const f = boot();
  f.events.get("pagehide")({ persisted: true });
  f.events.get("pageshow")({ persisted: true });
  f.events.get("pagehide")({ persisted: false });
  for (const method of ["suspend", "resume", "dispose"])
    assert.equal(f.calls.filter(call => call.name === `createBookImageControls.${method}`).length, 1);
  assert(f.calls.findIndex(call => call.name === "createBookImageControls.dispose")
    < f.calls.findIndex(call => call.name === "createBookControls.dispose"));
});

test("image-control initialization failure does not disable other publishing features", () => {
  const f = boot(true);
  assert.match(f.elements.get("#book-image-status")?.textContent ?? "", /unavailable/);
  for (const name of ["createBookLibraryControls", "createBookPreviewControls", "createBookPdfControls", "createBookFontControls", "createBookSearchControls", "createBookInspectionControls", "createBookLinkControls"])
    assert(f.instances.has(name), `initialization failure prevented ${name}`);
  assert.doesNotThrow(() => f.events.get("pagehide")({ persisted: false }));
});

test("manifest and both package assemblers ship image management and retain native gates", () => {
  const manifest = JSON.parse(read("../package.json"));
  for (const path of ["demo/book_image_controls.mjs", "demo/book_collection.mjs", "BOOK_IMAGES.md"])
    assert(manifest.files.includes(path), `manifest omits ${path}`);
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = read(`../../scripts/${name}`);
    const copyLoops = [...script.matchAll(/for file in ([\s\S]*?); do\s+cp "wasm\/(demo\/)?\$file"/g)];
    assert(copyLoops.some(match => match[2] === "demo/" && match[1].split(/\s+/).includes("book_image_controls.mjs")), `${name} omits image controller`);
    assert(copyLoops.some(match => !match[2] && match[1].split(/\s+/).includes("BOOK_IMAGES.md")), `${name} omits image documentation`);
    assert(script.includes("node --test wasm/tests/book_image_*.test.mjs"), `${name} omits image regressions`);
    assert(script.includes("cargo build") && script.includes("cmp "), `${name} lost native build/parity checks`);
  }
});
