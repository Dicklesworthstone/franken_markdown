// smoke.mjs — headless node proof that the GENERATED wasm-bindgen module loads
// and renders (bead 3i5.6). Imports the assembled package wrapper (which loads
// ./pkg/franken_markdown_bg.wasm), calls capabilities/renderHtml/renderPdf, and
// writes the wasm-side outputs for the parity comparison done by the gate.
//
// Usage: node smoke.mjs <pkgDir> <wasmPath> <outDir> <epoch> <md...>
//   pkgDir   assembled package dir containing franken_markdown.js + pkg/
//   wasmPath path to the generated franken_markdown_bg.wasm
//   outDir   where to write <stem>.wasm.html / <stem>.wasm.pdf
//   epoch    SOURCE_DATE_EPOCH-equivalent integer for deterministic PDF metadata
//   md...    one or more markdown file paths (the corpus)
import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";
import { pathToFileURL } from "node:url";
import { deflateSync } from "node:zlib";

const [pkgDir, wasmPath, outDir, epochArg, ...corpus] = process.argv.slice(2);
if (!pkgDir || !wasmPath || !outDir || corpus.length === 0) {
  console.error("usage: node smoke.mjs <pkgDir> <wasmPath> <outDir> <epoch> <md...>");
  process.exit(2);
}
const epoch = Number(epochArg);

function fail(msg) {
  console.error(`smoke: FAIL ${msg}`);
  process.exit(1);
}

function bytesEqual(a, b) {
  if (a.byteLength !== b.byteLength) return false;
  for (let i = 0; i < a.byteLength; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

// Assert a package-API call rejects (invalid options must be refused).
async function expectThrow(label, fn) {
  let threw = false;
  try {
    await fn();
  } catch {
    threw = true;
  }
  if (!threw) fail(`${label} should have thrown`);
}

const wrapperUrl = pathToFileURL(`${pkgDir}/franken_markdown.js`).href;
const mod = await import(wrapperUrl);

// Load the GENERATED module from its real .wasm bytes (not the native adapter).
const wasmBytes = readFileSync(wasmPath);
await mod.init(wasmBytes);
console.log(`smoke: loaded generated module (${wasmBytes.byteLength} wasm bytes)`);

// capabilities() must return the parsed wasm capability contract.
const caps = await mod.capabilities();
if (!caps || typeof caps !== "object" || caps.schema !== "fmd-wasm-capabilities-v1") {
  fail(`capabilities() did not return the wasm contract (schema=${caps && caps.schema})`);
}
if (!Array.isArray(caps.outputs) || !caps.outputs.includes("html") || !caps.outputs.includes("pdf")) {
  fail("capabilities().outputs must include html and pdf");
}
console.log(`smoke: capabilities ok (schema=${caps.schema} outputs=${caps.outputs.join(",")})`);

const dec = new TextDecoder();
for (const md of corpus) {
  const stem = basename(md).replace(/\.[^.]+$/, "");
  const source = readFileSync(md, "utf8");

  const html = await mod.renderHtml(source);
  const htmlText = dec.decode(html.bytes);
  if (!htmlText.startsWith("<!DOCTYPE html>")) fail(`renderHtml(${stem}) did not start with <!DOCTYPE html>`);
  writeFileSync(`${outDir}/${stem}.wasm.html`, html.bytes);

  const pdf = await mod.renderPdf(source, { metadataEpochSeconds: epoch });
  const sig = dec.decode(pdf.bytes.slice(0, 5));
  if (sig !== "%PDF-") fail(`renderPdf(${stem}) did not start with %PDF- (got ${JSON.stringify(sig)})`);
  writeFileSync(`${outDir}/${stem}.wasm.pdf`, pdf.bytes);

  // WASM-side determinism: a second render through the module must be byte-identical.
  const html2 = await mod.renderHtml(source);
  if (!bytesEqual(html.bytes, html2.bytes)) fail(`renderHtml(${stem}) is not byte-deterministic across runs`);
  const pdf2 = await mod.renderPdf(source, { metadataEpochSeconds: epoch });
  if (!bytesEqual(pdf.bytes, pdf2.bytes)) fail(`renderPdf(${stem}) is not byte-deterministic across runs`);

  console.log(`smoke: ${stem} -> html ${html.bytes.byteLength}B, pdf ${pdf.bytes.byteLength}B (deterministic)`);
}

// Negative-path contract through the package API (generated module + wrapper).
console.log("smoke: negative-path checks");
// Empty Markdown still renders a valid document.
const empty = await mod.renderHtml("");
if (!dec.decode(empty.bytes).startsWith("<!DOCTYPE html>")) fail("empty Markdown should still render a document");
// Malformed Markdown renders and exposes a diagnostics array (no throw).
const diag = await mod.renderHtml("[unclosed](\n\n```\nunterminated\n");
if (!Array.isArray(diag.diagnostics)) fail("renderHtml result must expose a diagnostics array");
// Raw HTML is escaped by default (fail-closed).
const escaped = await mod.renderHtml("<script>alert(1)</script>");
if (dec.decode(escaped.bytes).includes("<script>alert(1)")) fail("raw HTML must be escaped by default");
// Invalid options are refused through the package API.
await expectThrow("invalid darkMode", () => mod.renderHtml("x", { darkMode: "bogus" }));
await expectThrow("negative metadataEpochSeconds", () => mod.renderPdf("x", { metadataEpochSeconds: -1 }));
await expectThrow("non-integer metadataEpochSeconds", () => mod.renderPdf("x", { metadataEpochSeconds: 1.5 }));
console.log("smoke: negative-path ok");

// Multiple PDF image assets must ALL embed (the multi-image ABI path). Build two
// distinct minimal 1x1 RGB PNGs in-process (CRCs are not verified by the decoder).
console.log("smoke: multi-image checks");
function tinyPng(r, g, b) {
  const be32 = (n) => {
    const a = Buffer.alloc(4);
    a.writeUInt32BE(n >>> 0);
    return a;
  };
  const chunk = (type, data) =>
    Buffer.concat([be32(data.length), Buffer.from(type, "latin1"), data, be32(0)]);
  const ihdr = Buffer.concat([be32(1), be32(1), Buffer.from([8, 2, 0, 0, 0])]);
  const idat = deflateSync(Buffer.from([0, r, g, b]));
  return new Uint8Array(
    Buffer.concat([
      Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      chunk("IHDR", ihdr),
      chunk("IDAT", idat),
      chunk("IEND", Buffer.alloc(0)),
    ]),
  );
}
const multi = await mod.renderPdf("![Alpha](a.png)\n\n![Beta](b.png)", {
  metadataEpochSeconds: 1700000000,
  pdfImages: [
    { destination: "a.png", bytes: tinyPng(230, 60, 60) },
    { destination: "b.png", bytes: tinyPng(60, 140, 200) },
  ],
});
const multiText = dec.decode(multi.bytes);
if (!multiText.includes("/Alt (Alpha)") || !multiText.includes("/Alt (Beta)")) {
  fail("renderPdf with multiple images must embed every image (multi-image ABI)");
}
console.log("smoke: multi-image ok");

console.log("smoke: ok — generated wasm module loaded, rendered deterministically, and enforced the API contract.");
