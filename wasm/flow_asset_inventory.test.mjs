// Execute the public factory and production export coordinator. Native parsing
// and rendering are explicit doubles; Rust tests cover the real AST inventory.
import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const utf8 = value => new TextEncoder().encode(value);
const code = expected => error => error.code === expected;
const MIME = { html: "text/html; charset=utf-8", pdf: "application/pdf",
  epub: "application/epub+zip", svg: "image/svg+xml" };
const SOURCE = "# Heading ![badge](heading.png)\n\n| Image |\n| --- |\n| ![plot](table.svg) |\n\n![body](body.png)";
const image = (id, destination, isResolved = true) => ({
  kind: "image", requestId: String(id), destination, isResolved,
});

async function fixture({ items = [image(1, "heading.png"), image(2, "table.svg"), image(3, "body.png")],
  payloads = new Map([["1", new Uint8Array([1, 2])], ["2", new Uint8Array([3])], ["3", new Uint8Array([4, 5])]]),
  legacy = false, transform = page => page, hold } = {}) {
  const state = { inventoryReads: [], displayReads: 0, byteReads: [], calls: [], raw: null };
  class NativeSessionDouble {
    revision = "9007199254740993";
    layoutRevision = "9007199254740997";
    constructor(source) {
      this.source = source;
      state.raw = this;
      if (legacy) this.assetInventoryJson = undefined;
    }
    assetInventoryJson(revision, layoutRevision, offset, limit) {
      assert.equal(revision, this.revision);
      assert.equal(layoutRevision, this.layoutRevision);
      state.inventoryReads.push({ revision, layoutRevision, offset, limit });
      const end = Math.min(offset + limit, items.length);
      const page = { schemaVersion: 1, revision, layoutRevision, offset, total: items.length,
        nextOffset: end < items.length ? end : null, items: items.slice(offset, end) };
      const changed = transform(page, this);
      return typeof changed === "string" ? changed : JSON.stringify(changed);
    }
    snapshotJson(revision, layoutRevision, offset, limit) {
      state.displayReads++;
      // The heading/table images are intentionally absent from the display.
      const painted = [{ kind: "text", text: "Heading badge" }];
      return JSON.stringify({ schemaVersion: 1, revision, layoutRevision, offset,
        total: painted.length, nextOffset: null, items: painted.slice(offset, offset + limit) });
    }
    assetBytes(id, revision) {
      assert.equal(revision, this.revision);
      state.byteReads.push(id);
      return payloads.get(id) ?? null;
    }
    replaceSource(revision, source) {
      assert.equal(revision, this.revision);
      this.source = source;
      this.revision = String(BigInt(this.revision) + 1n);
      this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
    }
    free() { this.freed = true; }
  }
  const render = format => async (source, options) => {
    state.calls.push({ format, source, options });
    if (hold) await hold;
    return { format, mimeType: MIME[format], diagnostics: [],
      ...(format === "pdf" && options.pdfA ? { pdfA: options.pdfA, pdfAStrict: options.pdfAStrict } : {}),
      bytes: utf8(JSON.stringify(options.pdfImages.map(asset => [asset.destination, [...asset.bytes]]))) };
  };
  // Load the actual public entrypoint with ordinary Node ESM, following the
  // existing flow_session.test.mjs fixture pattern. Setters exist only in this
  // temporary package's explicit native/renderer doubles.
  const dir = await mkdtemp(join(tmpdir(), "fmd-flow-inventory-"));
  const cleanup = () => rm(dir, { recursive: true, force: true });
  await mkdir(join(dir, "pkg"));
  for (const file of ["flow.js", "flow_session.mjs", "flow_asset_batch.mjs",
    "flow_export.mjs", "flow_outlines.mjs", "pdf_page.mjs", "pdf_running.mjs",
    "render_typography.mjs"])
    await copyFile(new URL(file, import.meta.url), join(dir, file));
  await writeFile(join(dir, "package.json"), '{"type":"module"}');
  await writeFile(join(dir, "pkg", "franken_markdown.js"),
    "export let FmdFlowSession; export function configure(value) { FmdFlowSession = value; }\n");
  await writeFile(join(dir, "franken_markdown.js"),
    "let current; export function configure(value) { current = value; }\n" +
    "export async function init() {}\n" +
    "export const renderHtml = (...args) => current.html(...args);\n" +
    "export const renderPdf = (...args) => current.pdf(...args);\n" +
    "export const renderEpub = (...args) => current.epub(...args);\n" +
    "export const renderSvg = (...args) => current.svg(...args);\n");
  const native = await import(pathToFileURL(join(dir, "pkg", "franken_markdown.js")));
  native.configure(NativeSessionDouble);
  const renderers = await import(pathToFileURL(join(dir, "franken_markdown.js")));
  renderers.configure({ html: render("html"), pdf: render("pdf"), epub: render("epub"), svg: render("svg") });
  const facade = await import(pathToFileURL(join(dir, "flow.js")));
  const session = await facade.createFlowSession(SOURCE, { font: "serif" });
  return { state, session, items, payloads, cleanup };
}

test("public Flow factory exports heading/table images absent from the visible display", async t => {
  const f = await fixture();
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  assert.equal(f.session.snapshot().items[0].kind, "text");
  for (const format of ["html", "pdf", "epub", "svg"]) {
    const result = await f.session.exportDocument(format, {}, f.session.token);
    assert.deepEqual(JSON.parse(new TextDecoder().decode(result.bytes)),
      [["heading.png", [1, 2]], ["table.svg", [3]], ["body.png", [4, 5]]]);
    assert.equal(result.assetCount, 3);
    assert.equal(result.assetBytes, 5);
    assert.equal(f.state.calls.at(-1).source, SOURCE);
  }
  assert.equal(f.state.displayReads, 1, "exports never use visible snapshots as a complete asset inventory");
  assert.equal(f.state.inventoryReads.length, 4);
});

test("multi-format publication captures hidden assets once and owns their exact byte views", async t => {
  let release;
  const hold = new Promise(resolve => { release = resolve; });
  const backing = new Uint8Array([99, 6, 7, 99]);
  const f = await fixture({ items: [image(1, "heading.png"), image(2, "heading.png")],
    payloads: new Map([["1", backing.subarray(1, 3)], ["2", new Uint8Array([6, 7])]]), hold });
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  const pending = f.session.exportDocuments([
    { format: "html" }, { format: "pdf", options: { pdfA: "2b", pdfAStrict: true } },
    { format: "epub" }, { format: "svg" },
  ], f.session.token);
  assert.equal(f.state.inventoryReads.length, 1);
  backing.fill(42);
  f.payloads.get("2").fill(42);
  release();
  const result = await pending;
  assert.deepEqual(result.map(output => output.assetCount), [1, 1, 1, 1]);
  for (const output of result)
    assert.deepEqual(JSON.parse(new TextDecoder().decode(output.bytes)), [["heading.png", [6, 7]]]);
  assert.equal(result[1].pdfA, "2b");
  assert.equal(result[1].pdfAStrict, true);
  assert.equal(f.state.inventoryReads.length, 1);
  assert.equal(f.state.displayReads, 0);
});

test("complete inventory pages include late hidden images and preserve lossless request IDs", async t => {
  const items = Array.from({ length: 257 }, (_, i) => image(9007199254740993n + BigInt(i), "same.png"));
  const f = await fixture({ items,
    payloads: new Map(items.map(item => [item.requestId, new Uint8Array([8])])) });
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  const result = await f.session.exportDocument("epub", {}, f.session.token);
  assert.deepEqual(f.state.inventoryReads.map(read => read.offset), [0, 256]);
  assert.equal(f.state.byteReads.length, 257);
  assert.equal(f.state.byteReads.at(-1), "9007199254741249");
  assert.equal(result.assetCount, 1);
  assert.equal(result.assetBytes, 1);
  assert.equal(f.state.displayReads, 0);
});

test("older generated packages retain editing but refuse exports without complete inventory proof", async t => {
  const f = await fixture({ legacy: true });
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  assert.equal(f.session.snapshot().items.length, 1);
  f.session.replaceSource("# Edited", { expectedRevision: f.session.revision });
  assert.equal(f.session.source, "# Edited");
  for (const format of Object.keys(MIME))
    await assert.rejects(f.session.exportDocument(format, {}, f.session.token),
      error => error.code === "UNSUPPORTED_WASM_PACKAGE" && /assetInventoryJson.*rebuild/.test(error.message));
  await assert.rejects(f.session.exportDocuments([{ format: "html" }, { format: "pdf" }], f.session.token),
    code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(f.state.calls.length, 0);
  assert.equal(f.state.byteReads.length, 0);
  assert.equal(f.state.displayReads, 1);
  assert.equal(f.session.disposed, false);
});

for (const [name, transform] of [
  ["malformed JSON", () => "{"],
  ["wrong schema", page => ({ ...page, schemaVersion: 2 })],
  ["forged source token", page => ({ ...page, revision: "1" })],
  ["forged layout token", page => ({ ...page, layoutRevision: "1" })],
  ["wrong offset", page => ({ ...page, offset: 1 })],
  ["partial page", page => ({ ...page, items: page.items.slice(0, -1) })],
  ["stalled page", page => ({ ...page, nextOffset: 0 })],
  ["invalid total", page => ({ ...page, total: Number.MAX_SAFE_INTEGER + 1 })],
  ["duplicate request", page => ({ ...page, items: [page.items[0], page.items[0], page.items[2]] })],
  ["non-image record", page => ({ ...page, items: [{ ...page.items[0], kind: "text" }, ...page.items.slice(1)] })],
  ["invalid request ID", page => ({ ...page, items: [{ ...page.items[0], requestId: "01" }, ...page.items.slice(1)] })],
  ["numeric request ID", page => ({ ...page, items: [{ ...page.items[0], requestId: 1 }, ...page.items.slice(1)] })],
  ["empty destination", page => ({ ...page, items: [{ ...page.items[0], destination: " \t " }, ...page.items.slice(1)] })],
  ["malformed destination", page => ({ ...page, items: [{ ...page.items[0], destination: "\ud800" }, ...page.items.slice(1)] })],
  ["missing resolution state", page => ({ ...page, items: [{ ...page.items[0], isResolved: undefined }, ...page.items.slice(1)] })],
]) test("native inventory rejects " + name + " before invoking any renderer", async t => {
  const f = await fixture({ transform });
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  await assert.rejects(f.session.exportDocument("pdf", {}, f.session.token), code("INVALID_WASM_RESPONSE"));
  assert.equal(f.state.calls.length, 0);
  assert.equal(f.session.disposed, false);
});

for (const fault of ["repeated request", "changed total"])
  test("native inventory rejects " + fault + " across page boundaries", async t => {
    const items = Array.from({ length: 257 }, (_, i) => image(i + 1, "same.png"));
    const f = await fixture({ items,
      payloads: new Map(items.map(item => [item.requestId, new Uint8Array([1])])),
      transform: page => page.offset === 0 ? page : fault === "repeated request"
        ? { ...page, items: [items[0]] } : { ...page, total: 256, items: [] },
    });
    t.after(async () => { f.session.dispose(); await f.cleanup(); });
    await assert.rejects(f.session.exportDocument("html", {}, f.session.token), code("INVALID_WASM_RESPONSE"));
    assert.equal(f.state.calls.length, 0);
  });

for (const mode of ["pending", "dimensions", "empty", "conflicting"])
  test("publication-only " + mode + " images refuse export instead of disappearing", async t => {
    const items = [image(1, "hidden.png", mode !== "pending")];
    const payloads = new Map();
    if (mode === "empty") payloads.set("1", new Uint8Array());
    if (mode === "conflicting") {
      items.push(image(2, "hidden.png"));
      payloads.set("1", new Uint8Array([1]));
      payloads.set("2", new Uint8Array([2]));
    }
    const f = await fixture({ items, payloads });
    t.after(async () => { f.session.dispose(); await f.cleanup(); });
    await assert.rejects(f.session.exportDocuments([{ format: "html" }, { format: "epub" }], f.session.token),
      code(mode === "conflicting" ? "AMBIGUOUS_EXPORT_ASSET" : "UNRESOLVED_EXPORT_ASSET"));
    assert.equal(f.state.calls.length, 0);
  });

test("inventory reads preserve typed native errors and reject reentrant layout changes", async t => {
  let mode = "native error";
  const f = await fixture({ transform(page, raw) {
    if (mode === "native error") throw JSON.stringify({ code: "STALE_LAYOUT", message: "native inventory changed" });
    if (mode === "mutation") raw.layoutRevision = String(BigInt(raw.layoutRevision) + 1n);
    return page;
  } });
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  await assert.rejects(f.session.exportDocument("html", {}, f.session.token),
    { code: "STALE_LAYOUT", message: "native inventory changed" });
  mode = "mutation";
  await assert.rejects(f.session.exportDocument("html", {}, f.session.token), code("STALE_LAYOUT"));
  assert.equal(f.state.byteReads.length, 0);
  assert.equal(f.state.calls.length, 0);
  mode = "ready";
  assert.equal((await f.session.exportDocument("html", {}, f.session.token)).assetCount, 3);
});

test("native inventory respects the occurrence budget before reading payloads", async t => {
  const f = await fixture({ items: Array.from({ length: 1025 }, (_, i) => image(i + 1, "same.png")) });
  t.after(async () => { f.session.dispose(); await f.cleanup(); });
  await assert.rejects(f.session.exportDocument("svg", {}, f.session.token), code("BUDGET_EXCEEDED"));
  assert.equal(f.state.byteReads.length, 0);
  assert.equal(f.state.calls.length, 0);
});
