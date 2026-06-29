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

  console.log(`smoke: ${stem} -> html ${html.bytes.byteLength}B, pdf ${pdf.bytes.byteLength}B`);
}

console.log("smoke: ok — generated wasm module loaded and rendered real HTML+PDF for the corpus.");
