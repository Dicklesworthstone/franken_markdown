// smoke.mjs — headless node proof that the GENERATED wasm-bindgen module loads
// and renders (bead 3i5.6). Imports the assembled package wrapper (which loads
// ./pkg/franken_markdown_bg.wasm), exercises the real document exporters, and
// writes the wasm-side HTML/PDF outputs for the parity comparison done by the gate.
//
// Usage: node smoke.mjs <pkgDir> <wasmPath> <outDir> <epoch> <md...>
//   pkgDir   assembled package dir containing franken_markdown.js + pkg/
//   wasmPath path to the generated franken_markdown_bg.wasm
//   outDir   where to write <stem>.wasm.html / <stem>.wasm.pdf
//   epoch    SOURCE_DATE_EPOCH-equivalent integer for deterministic PDF metadata
//   md...    one or more markdown file paths (the corpus)
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";
import { pathToFileURL } from "node:url";
import { deflateSync, inflateRawSync } from "node:zlib";

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

// These use the real package before initialization. Invalid dialects must be
// refused before touching host resources or trying to instantiate the module.
const documentMethods = ["renderHtml", "renderPdf", "renderSvg", "renderEpub"];
let invalidProfileResourceReads = 0;
for (const method of documentMethods) {
  for (const profile of [null, "", "gfm", "GFM-PLUS", " gfm-plus ", "2b"]) {
    await assert.rejects(mod[method]("Term\n: Definition", {
      profile,
      get pdfImages() {
        invalidProfileResourceReads++;
        throw new Error("invalid profile reached resource capture");
      },
    }), { code: "INVALID_OPTIONS" }, `${method} rejects ${JSON.stringify(profile)}`);
  }
}
assert.equal(invalidProfileResourceReads, 0);
console.log("smoke: invalid authoring profiles rejected before initialization/resources");

// Load the GENERATED module from its real .wasm bytes (not the native adapter).
const wasmBytes = readFileSync(wasmPath);
await mod.init(wasmBytes);
console.log(`smoke: loaded generated module (${wasmBytes.byteLength} wasm bytes)`);

// capabilities() must return the parsed wasm capability contract.
const caps = await mod.capabilities();
if (!caps || typeof caps !== "object" || caps.schema !== "fmd-wasm-capabilities-v1") {
  fail(`capabilities() did not return the wasm contract (schema=${caps && caps.schema})`);
}
if (
  !Array.isArray(caps.outputs) ||
  !caps.outputs.includes("html") ||
  !caps.outputs.includes("pdf")
) {
  fail("capabilities().outputs must include html and pdf");
}
console.log(`smoke: capabilities ok (schema=${caps.schema} outputs=${caps.outputs.join(",")})`);

const dec = new TextDecoder();
for (const md of corpus) {
  const stem = basename(md).replace(/\.[^.]+$/, "");
  const source = readFileSync(md, "utf8");

  const html = await mod.renderHtml(source);
  const htmlText = dec.decode(html.bytes);
  if (!htmlText.startsWith("<!DOCTYPE html>"))
    fail(`renderHtml(${stem}) did not start with <!DOCTYPE html>`);
  writeFileSync(`${outDir}/${stem}.wasm.html`, html.bytes);

  const pdf = await mod.renderPdf(source, { metadataEpochSeconds: epoch });
  const sig = dec.decode(pdf.bytes.slice(0, 5));
  if (sig !== "%PDF-")
    fail(`renderPdf(${stem}) did not start with %PDF- (got ${JSON.stringify(sig)})`);
  writeFileSync(`${outDir}/${stem}.wasm.pdf`, pdf.bytes);

  // WASM-side determinism: a second render through the module must be byte-identical.
  const html2 = await mod.renderHtml(source);
  if (!bytesEqual(html.bytes, html2.bytes))
    fail(`renderHtml(${stem}) is not byte-deterministic across runs`);
  const pdf2 = await mod.renderPdf(source, { metadataEpochSeconds: epoch });
  if (!bytesEqual(pdf.bytes, pdf2.bytes))
    fail(`renderPdf(${stem}) is not byte-deterministic across runs`);

  console.log(
    `smoke: ${stem} -> html ${html.bytes.byteLength}B, pdf ${pdf.bytes.byteLength}B (deterministic)`,
  );
}

// Negative-path contract through the package API (generated module + wrapper).
console.log("smoke: negative-path checks");
// Empty Markdown still renders a valid document.
const empty = await mod.renderHtml("");
if (!dec.decode(empty.bytes).startsWith("<!DOCTYPE html>"))
  fail("empty Markdown should still render a document");
// Malformed Markdown renders and exposes a diagnostics array (no throw).
const diag = await mod.renderHtml("[unclosed](\n\n```\nunterminated\n");
if (!Array.isArray(diag.diagnostics)) fail("renderHtml result must expose a diagnostics array");
// Raw HTML is escaped by default (fail-closed).
const escaped = await mod.renderHtml("<script>alert(1)</script>");
if (dec.decode(escaped.bytes).includes("<script>alert(1)"))
  fail("raw HTML must be escaped by default");
// Invalid options are refused through the package API.
await expectThrow("invalid darkMode", () => mod.renderHtml("x", { darkMode: "bogus" }));
await expectThrow("negative metadataEpochSeconds", () =>
  mod.renderPdf("x", { metadataEpochSeconds: -1 }),
);
await expectThrow("non-integer metadataEpochSeconds", () =>
  mod.renderPdf("x", { metadataEpochSeconds: 1.5 }),
);
console.log("smoke: negative-path ok");

// Multiple PDF image assets must ALL embed (the multi-image ABI path). Build two
// distinct valid 1x1 RGB PNGs in-process, also consumed by the stricter SVG path.
console.log("smoke: multi-image checks");
function tinyPng(r, g, b) {
  const be32 = (n) => {
    const a = Buffer.alloc(4);
    a.writeUInt32BE(n >>> 0);
    return a;
  };
  const chunk = (type, data) => {
    const payload = Buffer.concat([Buffer.from(type, "latin1"), data]);
    let crc = 0xffffffff;
    for (const byte of payload) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    return Buffer.concat([be32(data.length), payload, be32((crc ^ 0xffffffff) >>> 0)]);
  };
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

// Title/author must reach output VERBATIM through the wrapper (no trimming), to
// keep byte-identical native↔WASM parity for padded metadata.
console.log("smoke: verbatim-metadata check");
const padded = await mod.renderHtml("# body", { title: "  Padded Title  " });
if (!dec.decode(padded.bytes).includes("<title>  Padded Title  </title>")) {
  fail("renderHtml must pass a padded title verbatim (no trimming)");
}
console.log("smoke: verbatim-metadata ok");

// Inspect EPUB records independently with Node's DEFLATE, rather than trusting
// the renderer's own archive decoder or merely comparing ZIP signatures.
function zipEntries(bytes) {
  const zip = Buffer.from(bytes), entries = new Map();
  let at = 0;
  while (at + 4 <= zip.length && zip.readUInt32LE(at) === 0x04034b50) {
    assert.ok(at + 30 <= zip.length, "complete ZIP local header");
    assert.equal(zip.readUInt16LE(at + 6) & 9, 0, "no encryption/data descriptors");
    const method = zip.readUInt16LE(at + 8), compressed = zip.readUInt32LE(at + 18);
    const size = zip.readUInt32LE(at + 22), nameEnd = at + 30 + zip.readUInt16LE(at + 26);
    const start = nameEnd + zip.readUInt16LE(at + 28), end = start + compressed;
    assert.ok(nameEnd <= start && end <= zip.length, "complete ZIP entry");
    const name = zip.subarray(at + 30, nameEnd).toString("utf8");
    assert.ok(!entries.has(name), `unique ZIP entry: ${name}`);
    assert.ok(method === 0 || method === 8, "stored or DEFLATE ZIP entry");
    const value = method === 0 ? zip.subarray(start, end) : inflateRawSync(zip.subarray(start, end));
    assert.equal(value.length, size, `uncompressed size: ${name}`);
    entries.set(name, value);
    at = end;
  }
  assert.ok(entries.size > 0, "EPUB contains real archive entries");
  return entries;
}

// Follow the writer's xref: binary font/image streams may contain text that
// resembles PDF syntax, so annotation checks must use actual object boundaries.
function pdfObjects(bytes) {
  const pdf = Buffer.from(bytes), marker = pdf.lastIndexOf("startxref");
  assert.ok(marker >= 0, "PDF has a cross-reference offset");
  const offset = Number(/^\s*(\d+)/u.exec(pdf.subarray(marker + 9).toString("ascii"))?.[1]);
  assert.ok(Number.isSafeInteger(offset) && offset > 0 && offset < marker);
  const lines = pdf.subarray(offset, marker).toString("ascii").split(/\r?\n/u);
  assert.equal(lines[0], "xref");
  const [first, count] = lines[1].trim().split(/\s+/u).map(Number);
  assert.equal(first, 0);
  assert.ok(Number.isSafeInteger(count) && count > 1);
  assert.equal(lines[2].trim(), "0000000000 65535 f");
  const positions = [];
  for (let id = 1; id < count; id++) {
    const row = /^(\d{10}) 00000 n\s*$/u.exec(lines[id + 2]);
    assert.ok(row, `xref entry ${id}`);
    const start = Number(row[1]);
    assert.ok(start > 0 && start < offset);
    assert.ok(pdf.subarray(start).toString("ascii", 0, 32).startsWith(`${id} 0 obj`));
    positions.push({ id, start });
  }
  assert.equal(lines[count + 2], "trailer");
  positions.sort((a, b) => a.start - b.start);
  return new Map(positions.map(({ id, start }, index) => {
    const end = positions[index + 1]?.start ?? offset;
    assert.ok(end > start, "unique PDF object offsets");
    return [id, pdf.subarray(start, end)];
  }));
}

console.log("smoke: generated authoring-profile rendering");
const profileSource = "# Profile source\n\nTerm\n: A **definition** with a [link](https://example.com).\n\n"
  + "> Nested term\n> : Nested definition.\n\nClaim.[^n]\n\n![Profile chart](profile.png)\n\n"
  + "[^n]: Supporting evidence.\n\n<script>alert('unsafe')</script>\n";
const profileImage = tinyPng(30, 110, 190);
const profileFont = new Uint8Array(readFileSync(new URL(
  "../tests/fixtures/fonts/source-sans-3/SourceSans3-Regular.otf", import.meta.url,
)));
const profileResources = {
  pdfImages: [{ destination: "profile.png", bytes: profileImage }],
  fontAssets: [{ slot: "body-regular", bytes: profileFont }],
};
const profileOptions = {
  renderHtml: { ...profileResources, title: "Profiles", htmlFontFormat: "ttf", fontEmbedding: "full" },
  renderPdf: { ...profileResources, title: "Profiles", metadataEpochSeconds: epoch,
    page: { size: { widthPt: 420, heightPt: 600 },
      margins: { topPt: 36, rightPt: 30, bottomPt: 36, leftPt: 30 } },
    running: { header: { left: "{title}", image: { dest: "profile.png", position: "right", heightPt: 12 } },
      footer: { right: "{page}/{pages}" } },
    baselineGridPt: 12.5, footnoteBacklinks: true },
  renderSvg: { ...profileResources, maxWidthPt: 360 },
  renderEpub: { ...profileResources, title: "Profiles", fontEmbedding: "full" },
};
const profiledOutputs = new Map();
for (const method of documentMethods) {
  const outputs = [];
  for (const profile of [undefined, "gfm-plus", "commonmark-gfm"]) {
    const options = profile === undefined ? profileOptions[method] : { ...profileOptions[method], profile };
    const output = await mod[method](profileSource, options);
    assert.equal(output.sourceLength, Buffer.byteLength(profileSource), `${method} source ownership`);
    assert.ok(Array.isArray(output.diagnostics));
    outputs.push(output);
  }
  assert.ok(bytesEqual(outputs[0].bytes, outputs[1].bytes), `${method}: default = explicit gfm-plus`);
  assert.ok(!bytesEqual(outputs[0].bytes, outputs[2].bytes), `${method}: commonmark-gfm changes definitions`);
  profiledOutputs.set(method, outputs);
}

function checkDefinitions(text, extended, label) {
  assert.equal(text.includes("<dl>"), extended, `${label}: selected definition-list structure`);
  assert.ok(text.includes("<strong>definition</strong>"), `${label}: inline semantics retained`);
  assert.ok(text.includes("Nested definition."), `${label}: nested source retained`);
  assert.ok(!text.includes("<script>"), `${label}: profile cannot enable raw HTML`);
}
const imageUri = "data:image/png;base64," + Buffer.from(profileImage).toString("base64");
const fontUri = "data:font/otf;base64," + Buffer.from(profileFont).toString("base64");
for (let index = 0; index < 3; index++) {
  const label = ["default", "gfm-plus", "commonmark-gfm"][index];
  const html = dec.decode(profiledOutputs.get("renderHtml")[index].bytes);
  checkDefinitions(html, index !== 2, `HTML ${label}`);
  assert.ok(html.includes(imageUri), `HTML ${label}: exact host image`);
  assert.ok(html.includes(fontUri), `HTML ${label}: exact full host font`);
  const archive = zipEntries(profiledOutputs.get("renderEpub")[index].bytes);
  const chapter = archive.get("OEBPS/chapter-1.xhtml");
  assert.ok(chapter, `EPUB ${label}: content document`);
  checkDefinitions(chapter.toString("utf8"), index !== 2, `EPUB ${label}`);
  assert.ok([...archive.values()].some(bytes => bytesEqual(bytes, profileImage)), `EPUB ${label}: exact host image`);
  assert.ok([...archive.values()].some(bytes => bytesEqual(bytes, profileFont)), `EPUB ${label}: exact full host font`);
  const svg = dec.decode(profiledOutputs.get("renderSvg")[index].bytes);
  assert.ok(svg.includes(imageUri), `SVG ${label}: host image retained`);
  const pdfOutput = profiledOutputs.get("renderPdf")[index];
  const pdf = Buffer.from(pdfOutput.bytes).toString("latin1");
  assert.ok(pdf.includes("/MediaBox [0 0 420 600]"), `PDF ${label}: page ABI slots retained`);
  assert.ok(pdf.includes("/Alt (Profile chart)") && pdf.includes("/Subtype /Image"), `PDF ${label}: image retained`);
  assert.ok(pdf.includes("/FontFile3"), `PDF ${label}: supplied CFF font retained`);
  assert.ok(!pdf.includes("/OutputIntents"), `PDF ${label}: authoring profile does not enable PDF/A`);
  assert.equal(Object.hasOwn(pdfOutput, "pdfA"), false);
}
const svgWithoutHostFont = await mod.renderSvg(profileSource, {
  ...profileOptions.renderSvg, profile: "commonmark-gfm", fontAssets: [],
});
assert.ok(!bytesEqual(svgWithoutHostFont.bytes, profiledOutputs.get("renderSvg")[2].bytes),
  "SVG profile consumes host font outlines");
assert.ok(profileImage.byteLength > 0 && profileFont.byteLength > 0, "host resources stay owned by the caller");
const archivedProfile = await mod.renderPdf(profileSource, {
  ...profileOptions.renderPdf, profile: "commonmark-gfm", pdfA: "2b", pdfAStrict: true,
});
assert.equal(archivedProfile.pdfA, "2b");
assert.equal(archivedProfile.pdfAStrict, true);
assert.ok(Buffer.from(archivedProfile.bytes).includes("/OutputIntents"));
console.log("smoke: authoring profiles, native structure, resources and independent PDF/A ok");

// Author-requested boundaries must survive the real source -> WASM -> exporter
// path. Inspect PDF page objects independently; compare continuous SVG with
// ordinary prose and with a literal code panel, rather than trusting a flag.
console.log("smoke: generated page-break authoring");
const pageBreakSource = "ALPHAPROOF.\r\n\r\n<!-- pagebreak -->\r\n\r\nBETAPROOF.\r\n";
const continuousSource = "ALPHAPROOF.\n\nBETAPROOF.\n";
const literalControlSource = "ALPHAPROOF.\n\n```\n<!-- pagebreak -->\n```\n\nBETAPROOF.\n";
const continuousPdf = await mod.renderPdf(continuousSource, { metadataEpochSeconds: epoch });
const continuousSvg = await mod.renderSvg(continuousSource);
const literalControlSvg = await mod.renderSvg(literalControlSource);
assert.ok(!bytesEqual(continuousSvg.bytes, literalControlSvg.bytes),
  "SVG keeps the marker visible when it is a literal code example");
const pageBreakOutputs = new Map();
for (const profile of [undefined, "gfm-plus", "commonmark-gfm"]) {
  const options = profile === undefined ? {} : { profile };
  const label = profile ?? "default", extended = profile !== "commonmark-gfm";
  const html = await mod.renderHtml(pageBreakSource, options);
  const htmlText = dec.decode(html.bytes);
  assert.equal((htmlText.match(/class="fmd-pagebreak"/gu) ?? []).length, extended ? 1 : 0,
    `HTML ${label}: only extended authoring inserts the boundary`);
  assert.ok(!htmlText.includes("<!-- pagebreak -->"), `HTML ${label}: comment source is not displayed`);
  const pdf = await mod.renderPdf(pageBreakSource, { ...options, metadataEpochSeconds: epoch });
  const pages = [...pdfObjects(pdf.bytes).values()].filter(object =>
    /\/Type\s+\/Page(?:\s|\/|>>)/u.test(object.subarray(0, 160).toString("ascii")));
  assert.equal(pages.length, extended ? 2 : 1, `PDF ${label}: actual page boundary`);
  if (!extended) assert.ok(bytesEqual(pdf.bytes, continuousPdf.bytes),
    "CommonMark PDF keeps ordinary comment behavior without hidden content");
  const svg = await mod.renderSvg(pageBreakSource, options);
  assert.ok(bytesEqual(svg.bytes, continuousSvg.bytes),
    `SVG ${label}: boundaries and ordinary comments preserve continuous output`);
  for (const result of [html, pdf, svg]) {
    assert.equal(result.sourceLength, Buffer.byteLength(pageBreakSource), "original CRLF source ownership");
  }
  pageBreakOutputs.set(label, { html, pdf, svg });
}
for (const target of ["html", "pdf", "svg"]) {
  assert.ok(bytesEqual(pageBreakOutputs.get("default")[target].bytes,
    pageBreakOutputs.get("gfm-plus")[target].bytes), `${target}: default page breaks match explicit GFM-plus`);
}
const trustedComment = await mod.renderHtml(pageBreakSource, {
  profile: "commonmark-gfm", allowRawHtml: true,
});
assert.ok(dec.decode(trustedComment.bytes).includes("<!-- pagebreak -->"),
  "CommonMark trusted HTML retains the original comment");
const literalComment = await mod.renderHtml("`<!-- pagebreak -->`\n\nBefore <!-- pagebreak --> after.");
assert.ok(!dec.decode(literalComment.bytes).includes('class="fmd-pagebreak"'),
  "inline/code comments never become page boundaries");
console.log("smoke: generated page breaks, profile isolation and continuous SVG ok");

const conditionSource = "$$\nx=1\\text{ if $n_1>0$ and $m^2=4$}\n$$\n";
const conditionHtml = dec.decode((await mod.renderHtml(conditionSource)).bytes);
for (const token of ["<mtext> if </mtext>", "<msub><mi>n</mi><mn>1</mn></msub>",
  "<mo>&gt;</mo><mn>0</mn>", "<mtext> and </mtext>", "<msup><mi>m</mi><mn>2</mn></msup>"]) {
  assert.ok(conditionHtml.includes(token), `MathML retains text and embedded formula: ${token}`);
}
assert.ok(!conditionHtml.includes("<mtext> if  and </mtext>"), "MathML conditions cannot silently lose formulas");

// Exercise the other newly shipped render paths through generated WASM too.
const taggedSource = "$$\nx\\tag*{Energy}\n$$\n";
const taggedHtml = dec.decode((await mod.renderHtml(taggedSource)).bytes);
assert.ok(taggedHtml.includes('class="fmd-tagged-equation"') && taggedHtml.includes("Energy"));
const taggedSvg = await mod.renderSvg(taggedSource);
assert.equal((dec.decode(taggedSvg.bytes).match(/<use href="#g/gu) ?? []).length, 7,
  "tagged SVG paints the equation and all six label glyphs");
assert.ok(!taggedSvg.diagnostics.some(item => /unsupported.*math|math.*unsupported/iu.test(item.message)));
const noteSource = "[Read](#fn-note) [Return](#fnref-1)\n\nClaim.[^note]\n\n[^note]: Evidence.\n";
const notePdf = await mod.renderPdf(noteSource, {
  profile: "commonmark-gfm", metadataEpochSeconds: epoch, footnoteBacklinks: true,
});
const noteObjects = pdfObjects(notePdf.bytes);
const destinations = [...noteObjects.values()]
  .filter(object => object.subarray(0, 100).includes("/Type /Annot /Subtype /Link"))
  .map(object => {
    const dictionary = object.toString("ascii");
    assert.ok(!dictionary.includes("/S /URI"), "public note fragments are PDF destinations");
    const match = /\/Dest \[(\d+) 0 R \/XYZ null ([\d.]+) null\]/u.exec(dictionary);
    assert.ok(match, "each note navigation annotation resolves");
    assert.ok(noteObjects.get(Number(match[1]))?.includes("/Type /Page "));
    return match[0];
  });
assert.equal(destinations.length, 4, "ordinary note links, citation and backlink all render");
assert.equal(destinations[0], destinations[2], "ordinary read link reaches the actual note");
assert.equal(destinations[1], destinations[3], "ordinary return link reaches the actual citation");
assert.notEqual(destinations[0], destinations[1]);
assert.ok(!Buffer.from(notePdf.bytes).includes("fmd-book-pdf-nav/"), "temporary actions never escape");
console.log("smoke: generated equation tags and note/citation destinations ok");

console.log(
  "smoke: ok — generated wasm module loaded, rendered deterministically, and enforced the API contract.",
);
