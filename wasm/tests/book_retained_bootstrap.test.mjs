import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// Execute the actual publisher bootstrap with only its imported constructors
// replaced. This proves selection/ownership/lifecycle wiring, not DOM rendering
// or loading the generated WASM binary. No production source body is rewritten.
const source = readFileSync(new URL("../demo/book.js", import.meta.url), "utf8");
let serial = 0;
async function bootstrap(failPreview = false) {
  const workers = [], instances = new Map(), status = new Map();
  const window = new EventTarget(); window.confirm = () => true;
  const document = { querySelector(selector) {
    if (!status.has(selector)) status.set(selector, { textContent: "" });
    return status.get(selector);
  } };
  const context = {
    createBookWorker(options = {}) {
      const worker = { options, disposed: 0, dispose() { this.disposed++; } };
      workers.push(worker); return worker;
    },
    createBookCollection: () => ({}),
  };
  for (const name of ["Controls", "FontControls", "InspectionControls", "LinkControls",
    "LibraryControls", "PdfControls", "PreviewControls", "SearchControls"]) {
    context[`createBook${name}`] = options => {
      if (name === "PreviewControls" && failPreview) throw Error("intentional startup failure");
      const instance = { options, suspended: 0, resumed: 0, disposed: 0,
        suspend() { this.suspended++; }, resume() { this.resumed++; },
        dispose() { this.disposed++; options.worker?.dispose(); }, detach() {} };
      instances.set(name, instance); return instance;
    };
  }
  for (const name of ["Font", "Inspection", "Link", "Pdf"]) context[`createBook${name}Panel`] = () => {};
  const key = `__fmdRetainedBootstrap${serial++}`;
  let replacements = 0;
  const code = source.replace(/^import\s*\{([^}]+)\}\s*from\s*"([^"]+)";/gm, (_all, symbols) => {
    const names = symbols.split(",").map(name => name.trim()).filter(Boolean);
    for (const name of names) assert.equal(typeof context[name], "function", `unhandled production import: ${name}`);
    const dependency = names.map(name => `export const ${name} = globalThis.${key}.${name};`).join("\n");
    replacements++;
    return `import { ${names.join(", ")} } from "data:text/javascript;base64,${Buffer.from(dependency).toString("base64")}";`;
  });
  assert(replacements > 0);
  const previous = new Map(["window", "document", key].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  globalThis[key] = context; globalThis.window = window; globalThis.document = document;
  try { await import(`data:text/javascript;base64,${Buffer.from(code + `\n// ${key}`).toString("base64")}`); }
  finally {
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  }
  return { workers, instances, status, dispatch(type, persisted) {
    const event = new Event(type); Object.assign(event, { persisted }); window.dispatchEvent(event);
  } };
}

test("publisher opts only the isolated HTML preview into retention and preserves lifecycle ordering", async () => {
  const f = await bootstrap();
  assert.equal(f.workers.length, 6);
  const preview = f.instances.get("PreviewControls");
  assert.deepEqual(preview.options.worker.options, { maxOutputBytes: 64 * 1024 * 1024, retainPreview: true });
  assert.equal(f.workers.filter(worker => worker.options.retainPreview === true).length, 1);
  assert.notEqual(preview.options.worker, f.instances.get("PdfControls").options.worker);
  assert.notEqual(preview.options.worker, f.instances.get("Controls").options.worker);
  f.dispatch("pagehide", true); assert.equal(preview.suspended, 1);
  f.dispatch("pageshow", true); assert.equal(preview.resumed, 1);
  f.dispatch("pagehide", false); assert.equal(preview.disposed, 1);
  assert(f.workers.every(worker => worker.disposed === 1));
});

test("failed preview startup disposes its owned client without preventing the publisher from loading", async () => {
  const f = await bootstrap(true);
  assert.equal(f.workers.length, 6);
  const previewWorker = f.workers.find(worker => worker.options.retainPreview === true);
  assert(previewWorker, "publisher must request retained previews");
  assert.equal(previewWorker.disposed, 1);
  assert.match(f.status.get("#preview-status").textContent, /unavailable/);
  assert(f.instances.has("PdfControls") && f.instances.has("FontControls") && f.instances.has("SearchControls"));
  f.dispatch("pagehide", false);
  assert(f.workers.every(worker => worker.disposed === 1));
});
