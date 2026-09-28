// Run against a freshly generated matching package. Missing native artifacts
// fail explicitly; this runner never substitutes a fake Markdown/font engine.
import assert from "node:assert/strict";
import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const directory = resolve(process.argv[2] ?? fileURLToPath(new URL(".", import.meta.url)));
const path = name => resolve(directory, name);
try {
  for (const name of ["pkg/franken_markdown_bg.wasm", "pkg/franken_markdown.js", "book.js",
    "book_worker.mjs", "book_site_preview.mjs", "book_session.mjs", "franken_markdown.js"])
    assert(statSync(path(name)).isFile(), `missing ${name}`);
} catch (error) {
  console.error("UNAVAILABLE: rebuild the matching native WASM package before retained-preview smoke testing.");
  console.error(String(error.message));
  process.exit(1);
}
const renderer = await import(pathToFileURL(path("franken_markdown.js")));
await renderer.init(new Uint8Array(readFileSync(path("pkg/franken_markdown_bg.wasm"))));
const book = await import(pathToFileURL(path("book.js")));
const { createRetainedBookPreview } = await import(pathToFileURL(path("book_worker.mjs")));
const { prepareBookInput } = await import(pathToFileURL(path("book_session.mjs")));
const { renderBookPreview, parseBookPreview } = await import(pathToFileURL(path("book_site_preview.mjs")));
let creates = 0, updates = 0;
// Count calls without replacing native parsing, updates, publication or decoding.
const retained = createRetainedBookPreview({ async createBook(files, options) {
  creates++;
  const native = await book.createBook(files, options);
  return { get sourceRevision() { return native.sourceRevision; },
    updateSources(...args) { updates++; return native.updateSources(...args); },
    renderSite: () => native.renderSite(), dispose: () => native.dispose() };
} }, renderBookPreview);
const files = [
  { path: "start.md", source: "# Start\n\n{{#include shared.md}}\n\n[End](end.md#end)\n" },
  { path: "end.md", source: "# End\n\nUnchanged chapter.\n" },
];
const options = { title: "Retained preview smoke", toc: true,
  includeSources: [{ path: "shared.md", source: "Shared original.\n" }] };
const checks = [];
async function compare(label) {
  const input = prepareBookInput(files, options);
  const reused = await retained.render(input.files, input.options);
  const fresh = await renderBookPreview(book, files, options);
  assert.deepEqual(reused.bytes, fresh.bytes, label);
  assert.equal(reused.sourceLength, fresh.sourceLength, label);
  assert.deepEqual(parseBookPreview(reused.bytes).pages.map(page => page.source), files.map(file => file.path));
  checks.push(label);
}
try {
  await compare("initial native preview matches a fresh export");
  files[0].source += "\n**Source edit** with ffi, AV and e\u0301.\n";
  await compare("changed chapter matches a fresh export");
  options.includeSources[0].source = "Changed shared include with $x^2$.\n";
  await compare("changed include matches a fresh export");
  await compare("unchanged source retains its native revision");
  assert.equal(creates, 1); assert.equal(updates, 2);
  options.title = "Changed metadata";
  await compare("configuration change reconstructs with current metadata");
  assert.equal(creates, 2);
  const original = files[0].source;
  files[0].source = "{{#include missing.md}}\n";
  const invalid = prepareBookInput(files, options);
  await assert.rejects(retained.render(invalid.files, invalid.options));
  files[0].source = original;
  await compare("failed update retires the native cache and valid retry recovers");
  assert.equal(creates, 3);
  console.log(JSON.stringify({ schema: "fmd-retained-preview-smoke-v1", native: true,
    passed: checks, retainedBookCreations: creates, nativeUpdateCalls: updates }, null, 2));
} finally { retained.clear(); }
