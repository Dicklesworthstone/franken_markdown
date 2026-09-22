// Public-wrapper behavioral tests. The generated WASM bindings are stubbed;
// these prove admission/packing/lifecycle, not Rust rendering or byte parity.
// Run: node --experimental-vm-modules --test wasm/svg_api_test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const wrapper = await readFile(process.env.FMD_SVG_TEST_WRAPPER ??
  new URL("./franken_markdown.js", import.meta.url), "utf8");
const named = ["renderEpubConfigured", "renderHtmlConfiguredAdvanced",
  "renderInteractiveHtmlConfigured", "renderPdfConfiguredMulti", "renderSvgConfigured",
  "accessibilityAudit", "capabilities", "documentStats", "renderBookPdf", "renderBookSite",
  "renderSemanticDiffHtml", "searchIndex", "semanticDiff"];
const plain = value => JSON.parse(JSON.stringify(value));
const image = (bytes = new Uint8Array([1]), destination = "a.png") => ({ destination, bytes });
const font = (bytes = new Uint8Array([1]), slot = "body-regular", weight) => ({ slot, bytes, weight });

async function harness({ advanced = true, init, result, invoke } = {}) {
  const state = { initializations: 0, calls: [], frees: 0 };
  const context = vm.createContext({ Uint8Array, Uint32Array, ArrayBuffer, TextDecoder, Blob });
  function render(route, args) {
    state.calls.push({ route, args });
    if (invoke) invoke(args);
    return result ? result(state, args) : {
      bytes: new TextEncoder().encode("<svg/>"), format: "svg", mimeType: "image/svg+xml",
      extension: "svg", sourceLength: Buffer.byteLength(args[0]),
      diagnosticsJson: () => JSON.stringify([{ severity: "warning", start: 0, end: 0,
        scope: "document", code: "svg_image_missing", message: 'missing "asset"\\path\n' }]),
      free: () => { state.frees++; },
    };
  }
  const exports = [...named, "default", ...(advanced ? ["renderSvgConfiguredResources"] : [])];
  const bindings = new vm.SyntheticModule(exports, function () {
    for (const name of named) this.setExport(name, (...args) => render(name, args));
    this.setExport("default", () => {
      state.initializations++;
      return init ? init(state.initializations) : Promise.resolve();
    });
    if (advanced) this.setExport("renderSvgConfiguredResources", (...args) => render("resources", args));
  }, { context });
  const geometry = new vm.SyntheticModule(["pdfPageGeometry"], function () {
    this.setExport("pdfPageGeometry", () => []);
  }, { context });
  const root = new vm.SourceTextModule(wrapper, { context });
  await root.link(specifier => {
    if (specifier === "./pkg/franken_markdown.js") return bindings;
    if (specifier === "./pdf_page.mjs") return geometry;
    throw new Error(`Unexpected import: ${specifier}`);
  });
  await root.evaluate();
  return { api: root.namespace, state };
}

async function refuses(options, pattern = /./, settings) {
  const { api, state } = await harness(settings);
  await assert.rejects(api.renderSvg("# Source", options), pattern);
  assert.equal(state.initializations, 0, "invalid requests must not initialize WASM");
  assert.equal(state.calls.length, 0, "invalid requests must not invoke a renderer");
}

test("packs actual sliced image bytes and canonical font slot order", async () => {
  const { api, state } = await harness();
  const source = "# é 🦀";
  const backing = new Uint8Array([99, 1, 2, 3, 4, 88]);
  const output = await api.renderSvg(source, {
    font: " serif ", darkMode: "light", fontScale: "125%", maxWidthPt: 360,
    pdfImages: [image(backing.subarray(1, 3), " a.png "),
      image(new DataView(backing.buffer, 3, 2), "b.jpg")],
    fontAssets: [font(backing.subarray(2, 4), "mono-regular", 650),
      font(backing.subarray(1, 2), "body-bold", 700)],
  });
  const { route, args } = state.calls[0];
  assert.equal(route, "resources");
  assert.equal(args.length, 14);
  assert.deepEqual(plain(args.slice(0, 6)), [source, "serif", "disabled", 1.25, 360, ["a.png", "b.jpg"]]);
  assert.deepEqual([...args[6]], [1, 2, 3, 4]);
  assert.deepEqual([...args[7]], [2, 2]);
  assert.deepEqual(args.slice(8, 13).map(a => Array.from(a)).flat(), [1, 2, 3]);
  assert.deepEqual([...args[9]], [1]);
  assert.deepEqual([...args[12]], [2, 3]);
  assert.deepEqual([...args[13]], [0, 700, 0, 0, 650]);
  assert.equal(output.sourceLength, Buffer.byteLength(source));
  assert.equal(output.filename(" plot "), "plot.svg");
  assert.equal(output.blob().type, "image/svg+xml");
  assert.equal(output.text(), "<svg/>");
  assert.equal(state.frees, 1);
  assert.ok(Object.isFrozen(output));
});

test("snapshots text, primitive options and bytes before asynchronous initialization", async () => {
  let resume, sourceReads = 0;
  const gate = new Promise(resolve => { resume = resolve; });
  const { api, state } = await harness({ init: () => gate });
  const bytes = new Uint8Array([1, 2]);
  const doc = { value: "old", toString() { sourceReads++; return this.value; } };
  const options = { font: "serif", darkMode: "off", typeSize: "xl", maxWidthPt: 360,
    pdfImages: [image(bytes)], fontAssets: [font(bytes, "body-regular", 500)] };
  const pending = api.renderSvg(doc, options);
  assert.equal(state.initializations, 1);
  doc.value = "new"; bytes.fill(9);
  options.font = "sans"; options.darkMode = "auto"; options.typeSize = "small";
  options.maxWidthPt = 612; options.pdfImages[0].destination = "changed.png";
  options.fontAssets[0].weight = 900;
  options.pdfImages.push(image()); options.fontAssets.length = 0;
  resume(); await pending;
  const args = state.calls[0].args;
  assert.equal(sourceReads, 1);
  assert.deepEqual(plain(args.slice(0, 6)), ["old", "serif", "disabled", 1.25, 360, ["a.png"]]);
  assert.deepEqual([...args[6]], [1, 2]);
  assert.deepEqual([...args[8]], [1, 2]);
  assert.deepEqual([...args[13]], [500, 0, 0, 0, 0]);
});

test("reads requested top-level settings once", async () => {
  const { api } = await harness();
  const reads = new Map(), options = {};
  for (const [name, value] of Object.entries({ font: "sans", darkMode: "auto", fontScale: 1,
    typeSize: "sm", maxWidthPt: 612, pdfImages: [], fontAssets: [] })) {
    Object.defineProperty(options, name, { get() {
      reads.set(name, (reads.get(name) ?? 0) + 1); return value;
    } });
  }
  await api.renderSvg("x", options);
  assert.deepEqual([...reads.values()], [1, 1, 1, 1, 1, 1, 1]);
});

test("accepts ArrayBuffer, multibyte typed-array views and Node Buffer slices", async () => {
  const { api, state } = await harness();
  const buffer = Buffer.from([90, 10, 11, 91]);
  const word = new Uint16Array([0x1234]);
  await api.renderSvg("x", { pdfImages: [image(buffer.subarray(1, 3), "a"),
    image(word, "b"), image(new Uint8Array([12]).buffer, "c")] });
  assert.deepEqual([...state.calls[0].args[6]], [10, 11, ...new Uint8Array(word.buffer), 12]);
  assert.deepEqual([...state.calls[0].args[7]], [2, 2, 1]);
});

test("rejects shared buffers even with a spoofed ArrayBuffer tag", async () => {
  const shared = new SharedArrayBuffer(2);
  Object.defineProperty(shared, Symbol.toStringTag, { value: "ArrayBuffer" });
  await refuses({ pdfImages: [image(new Uint8Array(shared))] });
  await refuses({ fontAssets: [font(new Uint8Array(shared))] });
});

test("rejects detached buffers before WASM initialization", async () => {
  const bytes = new Uint8Array([1, 2]);
  structuredClone(bytes.buffer, { transfer: [bytes.buffer] });
  await refuses({ pdfImages: [image(bytes)] });
  await refuses({ fontAssets: [font(bytes)] });
});

test("rejects duplicate trimmed destinations and duplicate font slots", async () => {
  await refuses({ pdfImages: [image(undefined, "a"), image(undefined, " a ")] }, /unique/);
  await refuses({ fontAssets: [font(), font()] }, /duplicate/);
});

test("rejects invalid resource payloads and font weights rather than dropping them", async () => {
  for (const bytes of [new Uint8Array(), [], "base64", null]) {
    await refuses({ pdfImages: [image(bytes)] });
    await refuses({ fontAssets: [font(bytes)] });
  }
  for (const weight of [0, -1, 1001, NaN, Infinity, 400.5, "400"]) {
    await refuses({ fontAssets: [font(undefined, "body-regular", weight)] }, /weight/);
  }
  await refuses({ fontAssets: [font(undefined, "wrong-slot")] }, /slot/);
});

test("rejects invalid width and scale settings before asset copies", async () => {
  for (const width of [0, 143, 14401, NaN, Infinity, "612"]) {
    await refuses({ maxWidthPt: width });
  }
  for (const scale of [0, -1, NaN, Infinity]) await refuses({ fontScale: scale });
  const { api, state } = await harness();
  for (const width of [144, 14400]) await api.renderSvg("x", { maxWidthPt: width });
  assert.deepEqual(state.calls.map(call => call.args[4]), [144, 14400]);
});

test("count admission occurs before reading oversized collections", async () => {
  let reads = 0;
  const images = new Array(4097), fonts = new Array(6);
  for (const array of [images, fonts]) Object.defineProperty(array, 0, { get() {
    reads++; throw new Error("must not access oversized entries");
  } });
  await refuses({ pdfImages: images }, /at most 4096/);
  await refuses({ fontAssets: fonts }, /at most 5/);
  assert.equal(reads, 0);
});

test("invalid collection shapes and sparse entries cannot reach WASM", async () => {
  for (const options of [null, [], 1, "options", { pdfImages: {} }, { fontAssets: {} },
    { pdfImages: new Array(1) }, { fontAssets: new Array(1) }]) await refuses(options);
});

test("UTF-8 admission preserves astral text and rejects unpaired surrogates", async () => {
  const { api, state } = await harness();
  const output = await api.renderSvg("é🦀");
  assert.equal(output.sourceLength, 6);
  assert.equal(state.calls[0].args[0], "é🦀");
  for (const text of ["\ud800", "\udfff", "x\ud800x"]) {
    const { api, state } = await harness();
    await assert.rejects(api.renderSvg(text), /surrogate/);
    assert.equal(state.initializations, 0);
    await refuses({ pdfImages: [image(undefined, text)] }, /surrogate/);
  }
});

test("destination limits count UTF-8 and total bytes, not character count", async () => {
  await refuses({ pdfImages: [image(undefined, "é".repeat(4097))] }, /byte limit/);
  await refuses({ pdfImages: [image(undefined, "a\nq")] }, /controls/);
  const names = Array.from({ length: 9 }, (_, index) => `${index}${"a".repeat(8191)}`);
  await refuses({ pdfImages: names.map(name => image(undefined, name)) }, /64 KiB/);
});

test("source and asset budgets refuse without initializing or rendering", async () => {
  const { api, state } = await harness();
  await assert.rejects(api.renderSvg("x".repeat(32 * 1024 * 1024 + 1)), /byte limit/);
  assert.equal(state.initializations, 0);
  const huge = new Uint8Array(32 * 1024 * 1024 + 1);
  await refuses({ pdfImages: [image(huge)] }, /32 MiB/);
  await refuses({ fontAssets: [font(huge)] }, /32 MiB/);
  const max = huge.subarray(0, 32 * 1024 * 1024);
  await refuses({ pdfImages: Array.from({ length: 5 }, (_, i) => image(max, `${i}.png`)) }, /128 MiB/);
  await refuses({ fontAssets: ["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"]
    .map(slot => font(max, slot)) }, /128 MiB/);
});

test("old binaries reject requested resources before initialization", async () => {
  await refuses({ pdfImages: [image()] }, error => error.code === "UNSUPPORTED_WASM_PACKAGE", { advanced: false });
  await refuses({ fontAssets: [font()] }, error => error.code === "UNSUPPORTED_WASM_PACKAGE", { advanced: false });
});

test("old binaries still render basic SVG and explicitly disclose limited diagnostics", async () => {
  const { api, state } = await harness({ advanced: false });
  const output = await api.renderSvg("x", { font: "serif", maxWidthPt: 612, pdfImages: [], fontAssets: [] });
  assert.equal(state.calls[0].route, "renderSvgConfigured");
  assert.equal(state.calls[0].args.length, 5);
  assert.equal(output.diagnostics.at(-1).code, "svg_legacy_package");
  assert.equal(output.diagnostics.at(-1).scope, "document");
  assert.equal(output.text(), "<svg/>");
  assert.equal(state.frees, 1);
});

test("renderer facade preserves complete diagnostic fields and result helpers", async () => {
  const { api, state } = await harness();
  const renderer = await api.createRenderer();
  const output = await renderer.renderSvg("x");
  assert.deepEqual(plain(output.diagnostics), [{ severity: "warning", start: 0, end: 0,
    scope: "document", code: "svg_image_missing", message: 'missing "asset"\\path\n' }]);
  assert.equal(state.initializations, 1);
  assert.equal(state.calls[0].route, "resources");
});

test("invalid diagnostics release the generated result even when normalization fails", async () => {
  const { api, state } = await harness({ result: state => ({
    bytes: new Uint8Array(), diagnosticsJson: () => "broken-json",
    free() { state.frees++; },
  }) });
  await assert.rejects(api.renderSvg("x"), /Invalid diagnostics JSON/);
  assert.equal(state.frees, 1);
});

test("binding errors propagate and thrown strings become bounded Error objects", async () => {
  const { api } = await harness({ invoke() { throw "x".repeat(4096); } });
  await assert.rejects(api.renderSvg("x"), error => error.name === "Error" && error.message.length === 2048);
  const original = new Error("native failure");
  const { api: second } = await harness({ invoke() { throw original; } });
  await assert.rejects(second.renderSvg("x"), error => error === original);
});

test("initialization failure is retryable and does not invoke a renderer", async () => {
  const { api, state } = await harness({ init: count => count === 1 ? Promise.reject(new Error("init failed")) : Promise.resolve() });
  await assert.rejects(api.renderSvg("x"), /init failed/);
  assert.equal(state.calls.length, 0);
  await api.renderSvg("second");
  assert.equal(state.initializations, 2);
  assert.equal(state.calls.length, 1);
  assert.equal(state.calls[0].args[0], "second");
});

test("concurrent requests share initialization but retain independent snapshots", async () => {
  let resume;
  const gate = new Promise(resolve => { resume = resolve; });
  const { api, state } = await harness({ init: () => gate });
  const bytes = new Uint8Array([1]);
  const first = api.renderSvg("first", { pdfImages: [image(bytes)] });
  bytes[0] = 2;
  const second = api.renderSvg("second", { pdfImages: [image(bytes)] });
  bytes[0] = 3;
  resume(); await Promise.all([first, second]);
  assert.equal(state.initializations, 1);
  assert.deepEqual(state.calls.map(call => [...call.args[6]]), [[1], [2]]);
});
