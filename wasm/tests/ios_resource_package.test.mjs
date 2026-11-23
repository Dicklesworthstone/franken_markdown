import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { Worker, isMainThread, parentPort } from "node:worker_threads";
import { deflateSync, inflateSync, inflateRawSync } from "node:zlib";
import { resourceHarness, uploadResources, readResourceExport } from "./support/ios_resource_upload.mjs";
import { sourceCRC32 } from "./support/ios_source_upload.mjs";

// Genuine execution of the checked-in Apple renderer package. WebKit DOM/IPC
// are adapters; Rust/WASM rendering, image decoding, font subsetting, archives
// and artifact bytes are real. This qualifies that historical bundled artifact
// with the current bridge, not current-source Rust or iOS visual appearance.
const root = new URL("../../ios/Renderer/", import.meta.url);
const cases = ["HTML image payloads and custom embedded font", "PDF decoded image pixels and custom font",
  "SVG images and custom glyph geometry", "EPUB image/font archive resources", "explicit attachment authority"];

async function probe() {
  const worker = new Worker(new URL(import.meta.url));
  let deadline;
  try {
    return await new Promise((resolve, reject) => {
      deadline = setTimeout(() => reject(new Error("Bundled attachment rendering exceeded 40 seconds")), 40_000);
      worker.once("message", resolve); worker.once("error", reject);
      worker.once("exit", code => reject(new Error(`Attachment worker exited without results (${code})`)));
    });
  } finally { clearTimeout(deadline); await worker.terminate(); }
}

if (isMainThread) {
  let pending;
  for (const name of cases) test(`bundled native resource path: ${name}`, { timeout: 45_000 }, async t => {
    pending ??= probe();
    const report = await pending, result = report.results.find(value => value.name === name);
    assert(result, `Missing result: ${name}`); assert.equal(result.ok, true, result.error);
    t.diagnostic(`Actual bundled ${report.version} (${report.tag}), WASM SHA-256 ${report.wasmSha256}; current bridge SHA-256 ${report.bridgeSha256}.`);
  });
} else {
  const hash = bytes => createHash("sha256").update(bytes).digest("hex");
  const engine = await import(new URL("franken_markdown.js", root));
  const [wasm, bridge, wrapper, manifestText, customFont] = await Promise.all([
    readFile(new URL("pkg/franken_markdown_bg.wasm", root)), readFile(new URL("bridge.html", root)),
    readFile(new URL("franken_markdown.js", root)), readFile(new URL("RendererManifest.json", root), "utf8"),
    readFile(new URL("../../fmd-font/fonts/computer-modern/cmunrm.ttf", import.meta.url)),
  ]);
  const manifest = JSON.parse(manifestText);
  assert.equal(hash(wasm), manifest.wasmSha256); assert.equal(hash(wrapper), manifest.wrapperSha256);

  // Independent tiny PNG encoder; rows are known RGB colors, not fixture bytes
  // obtained from the rendering engine or its image decoder.
  function png(pixels) {
    const part = (tag, data) => {
      const name = Buffer.from(tag), length = Buffer.alloc(4), crc = Buffer.alloc(4);
      length.writeUInt32BE(data.length); crc.writeUInt32BE(Number.parseInt(sourceCRC32(Buffer.concat([name, data])), 16));
      return Buffer.concat([length, name, data, crc]);
    };
    const header = Buffer.alloc(13); header.writeUInt32BE(2); header.writeUInt32BE(2, 4); header[8] = 8; header[9] = 2;
    const rows = Buffer.concat([Buffer.of(0), pixels.subarray(0, 6), Buffer.of(0), pixels.subarray(6)]);
    return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), part("IHDR", header),
      part("IDAT", deflateSync(rows)), part("IEND", Buffer.alloc(0))]);
  }
  const pixels = [Buffer.from([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]),
    Buffer.from([17, 31, 47, 64, 80, 96, 112, 128, 144, 160, 176, 192])];
  const images = pixels.map(png), keys = ["caf\u00e9.png", "cafe\u0301.png"];
  const entries = [...images.map((bytes, index) => ({ kind: "image", key: keys[index], bytes })),
    { kind: "font", key: "body-regular", bytes: customFont }];
  const h = await resourceHarness({ engine, wasm, bridgeSource: bridge.toString("utf8") });
  uploadResources(h.window, entries, 7);
  const source = `# Attachment proof\n\nAVATAR office 123.\n\n![First](${keys[0]})\n\n![Second](${keys[1]})\n`;
  const options = { font: "sans", darkMode: "disabled", htmlFontFormat: "ttf", metadataEpochSeconds: 0 };
  const directOptions = { ...options, pdfImages: images.map((bytes, index) => ({ destination: keys[index], bytes })),
    fontAssets: [{ slot: "body-regular", bytes: customFont }] };
  let requestID = 0;
  async function exportFormat(format, resourceSetID = 7) {
    const command = { requestID: ++requestID, markdown: source, options,
      ...(resourceSetID === undefined ? {} : { resourceSetID }) };
    const value = format === "html" ? await h.window.frankenExportHtml(command)
      : format === "pdf" ? await h.window.frankenExportPdf(command)
      : await h.window.frankenExportArtifact({ ...command, format });
    const output = readResourceExport(h.window, value);
    assert.equal(output.extension, format);
    if (resourceSetID === 7) assert.equal(output.diagnosticCount, 0);
    return output.bytes;
  }

  // Read SFNT metrics independently. Subset glyph IDs differ from the original;
  // cmap plus hmtx must still reproduce the selected font's A/V advances.
  function fontMetrics(input) {
    const bytes = Buffer.from(input), tables = new Map();
    assert.equal(bytes.readUInt32BE(0), 0x00010000, "TrueType sfnt");
    for (let i = 0; i < bytes.readUInt16BE(4); i++) {
      const at = 12 + i * 16, tag = bytes.toString("ascii", at, at + 4), start = bytes.readUInt32BE(at + 8), length = bytes.readUInt32BE(at + 12);
      assert(start + length <= bytes.length); tables.set(tag, bytes.subarray(start, start + length));
    }
    const cmap = tables.get("cmap"), hmtx = tables.get("hmtx"), hhea = tables.get("hhea"), head = tables.get("head");
    assert(cmap && hmtx && hhea && head);
    let subtable;
    for (let i = 0; i < cmap.readUInt16BE(2); i++) {
      const at = cmap.readUInt32BE(4 + i * 8 + 4);
      if (cmap.readUInt16BE(at) === 4) { subtable = cmap.subarray(at); break; }
    }
    assert(subtable, "BMP cmap for probe text");
    const count = subtable.readUInt16BE(6) / 2, end = 14, start = end + count * 2 + 2;
    const delta = start + count * 2, ranges = delta + count * 2;
    function advance(codepoint) {
      for (let i = 0; i < count; i++) {
        const first = subtable.readUInt16BE(start + i * 2), last = subtable.readUInt16BE(end + i * 2);
        if (codepoint < first || codepoint > last) continue;
        const shift = subtable.readInt16BE(delta + i * 2), range = subtable.readUInt16BE(ranges + i * 2);
        let glyph = range ? subtable.readUInt16BE(ranges + i * 2 + range + (codepoint - first) * 2) : codepoint;
        if (!range || glyph) glyph = (glyph + shift) & 65535;
        assert(glyph > 0, "probe glyph retained");
        return hmtx.readUInt16BE(Math.min(glyph, hhea.readUInt16BE(34) - 1) * 4);
      }
      assert.fail("probe character missing from cmap");
    }
    return { units: head.readUInt16BE(18), ascender: hhea.readInt16BE(4), A: advance(65), V: advance(86) };
  }
  const expectedFont = fontMetrics(customFont);
  const usesCustomFont = fonts => fonts.some(bytes => {
    try { return JSON.stringify(fontMetrics(bytes)) === JSON.stringify(expectedFont); } catch { return false; }
  });
  function htmlFonts(html) {
    return [...html.matchAll(/data:font\/ttf;base64,([A-Za-z0-9+/=]+)/g)].map(match => Buffer.from(match[1], "base64"));
  }
  function pdfStreams(pdf) {
    const text = pdf.toString("latin1"), streams = [];
    for (const match of text.matchAll(/(\d+) 0 obj\s*/g)) {
      const start = match.index + match[0].length, endObject = text.indexOf("endobj", start);
      const startStream = text.indexOf("stream\n", start);
      if (startStream < 0 || startStream > endObject) continue;
      const dictionary = text.slice(start, startStream), size = /\/Length\s+(\d+)\b/.exec(dictionary);
      assert(size, "direct stream length");
      const offset = startStream + "stream\n".length, compressed = pdf.subarray(offset, offset + Number(size[1]));
      assert.equal(compressed.length, Number(size[1]));
      const bytes = /\/FlateDecode\b/.test(dictionary) ? inflateSync(compressed) : compressed;
      streams.push({ dictionary, bytes });
    }
    return streams;
  }
  // PNG predictor reconstruction is independent of the native decoder. Honor
  // all standard row filters so equivalent writer choices keep this test valid.
  function imagePixels(stream) {
    const { dictionary, bytes } = stream;
    assert.match(dictionary, /\/Width 2\b/); assert.match(dictionary, /\/Height 2\b/);
    assert.match(dictionary, /\/DeviceRGB\b/); assert.match(dictionary, /\/BitsPerComponent 8\b/);
    if (!/\/Predictor 15\b/.test(dictionary)) { assert.equal(bytes.length, 12); return bytes; }
    assert.equal(bytes.length, 14);
    const output = Buffer.alloc(12), stride = 6;
    for (let y = 0; y < 2; y++) {
      const filter = bytes[y * 7]; assert(filter <= 4);
      for (let x = 0; x < stride; x++) {
        const a = x >= 3 ? output[y * stride + x - 3] : 0;
        const b = y ? output[(y - 1) * stride + x] : 0, c = y && x >= 3 ? output[(y - 1) * stride + x - 3] : 0;
        const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
        const predictor = filter === 0 ? 0 : filter === 1 ? a : filter === 2 ? b : filter === 3 ? Math.floor((a + b) / 2)
          : pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
        output[y * stride + x] = (bytes[y * 7 + x + 1] + predictor) & 255;
      }
    }
    return output;
  }
  function zipEntries(zip) {
    const entries = new Map(); let at = 0;
    while (at + 4 <= zip.length && zip.readUInt32LE(at) === 0x04034b50) {
      assert(at + 30 <= zip.length); assert.equal(zip.readUInt16LE(at + 6) & 9, 0);
      const method = zip.readUInt16LE(at + 8), compressedSize = zip.readUInt32LE(at + 18), size = zip.readUInt32LE(at + 22);
      const nameEnd = at + 30 + zip.readUInt16LE(at + 26), start = nameEnd + zip.readUInt16LE(at + 28), end = start + compressedSize;
      assert(end <= zip.length && nameEnd <= start); assert(method === 0 || method === 8);
      const name = zip.toString("utf8", at + 30, nameEnd), bytes = method === 0 ? zip.subarray(start, end) : inflateRawSync(zip.subarray(start, end));
      assert.equal(bytes.length, size); assert.equal(entries.has(name), false);
      assert.equal(Number.parseInt(sourceCRC32(bytes), 16), zip.readUInt32LE(at + 14));
      entries.set(name, bytes); at = end;
    }
    assert(entries.size > 0); return entries;
  }

  const results = [];
  const check = async (name, operation) => {
    try { await operation(); results.push({ name, ok: true }); }
    catch (error) { results.push({ name, ok: false, error: String(error.stack ?? error).slice(0, 6000) }); }
  };
  await check(cases[0], async () => {
    const bytes = await exportFormat("html"), html = bytes.toString("utf8");
    assert.deepEqual(bytes, Buffer.from((await engine.renderHtml(source, directOptions)).bytes));
    const emitted = [...html.matchAll(/<img\b[^>]*\bsrc="data:image\/png;base64,([A-Za-z0-9+/=]+)"[^>]*>/g)].map(match => Buffer.from(match[1], "base64"));
    assert.deepEqual(emitted, images, "both exact Unicode keys resolve to their own original image");
    assert(usesCustomFont(htmlFonts(html)), "embedded font metrics match the selected font");
    const without = (await engine.renderHtml(source, { ...options, pdfImages: directOptions.pdfImages })).text();
    assert.equal(usesCustomFont(htmlFonts(without)), false, "default fonts are a real negative control");
    assert.equal(await h.window.frankenRender({ requestID: 1, markdown: source, resourceSetID: 7, options }), true);
    assert.equal(h.window.frankenCommitPreview(1), true); assert.equal(h.preview.srcdoc, html);
  });
  await check(cases[1], async () => {
    const bytes = await exportFormat("pdf");
    assert.deepEqual(bytes, Buffer.from((await engine.renderPdf(source, directOptions)).bytes));
    assert.equal(bytes.toString("ascii", 0, 5), "%PDF-");
    const streams = pdfStreams(bytes), imageStreams = streams.filter(value => /\/Subtype \/Image\b/.test(value.dictionary));
    assert.equal(imageStreams.length, 2);
    assert.deepEqual(imageStreams.map(imagePixels).map(hash).sort(), pixels.map(hash).sort());
    assert(usesCustomFont(streams.map(value => value.bytes)), "PDF font program has the supplied font's glyph metrics");
  });
  await check(cases[2], async () => {
    const bytes = await exportFormat("svg"), svg = bytes.toString("utf8");
    assert.deepEqual(bytes, Buffer.from((await engine.renderSvg(source, directOptions)).bytes));
    const emitted = [...svg.matchAll(/(?:href|xlink:href)="data:image\/png;base64,([A-Za-z0-9+/=]+)"/g)].map(match => Buffer.from(match[1], "base64"));
    assert.deepEqual(emitted, images);
    const paths = text => [...text.matchAll(/<path\b[^>]*\bd="([^"]+)"/g)].map(match => match[1]);
    assert(paths(svg).length > 0);
    const without = (await engine.renderSvg(source, { ...options, pdfImages: directOptions.pdfImages })).text();
    assert.notDeepEqual(paths(svg), paths(without), "selected font changes actual emitted glyph outlines");
  });
  await check(cases[3], async () => {
    const bytes = await exportFormat("epub");
    assert.deepEqual(bytes, Buffer.from((await engine.renderEpub(source, directOptions)).bytes));
    const archive = zipEntries(bytes);
    assert.equal(archive.get("mimetype").toString(), "application/epub+zip");
    const embedded = [...archive].filter(([name]) => /\.png$/.test(name)).map(([, bytes]) => bytes);
    assert.deepEqual(embedded.map(hash).sort(), images.map(hash).sort());
    assert(usesCustomFont([...archive].filter(([name]) => /\.ttf$/.test(name)).map(([, bytes]) => bytes)));
    const chapters = [...archive].filter(([name]) => /\.xhtml$/.test(name)).map(([, bytes]) => bytes.toString("utf8")).join("\n");
    assert.match(chapters, /alt="First"/); assert.match(chapters, /alt="Second"/);
  });
  await check(cases[4], async () => {
    // No resourceSetID means no inherited grant even while set 7 is resident.
    const json = await h.window.frankenExportHtml({ requestID: ++requestID, markdown: source, options });
    const html = readResourceExport(h.window, json).bytes.toString("utf8");
    for (const image of images) assert.equal(html.includes(image.toString("base64")), false);
    assert.equal(usesCustomFont(htmlFonts(html)), false);
    uploadResources(h.window, [], 8);
    const empty = (await exportFormat("html", 8)).toString("utf8");
    assert.equal(usesCustomFont(htmlFonts(empty)), false);
    await assert.rejects(h.window.frankenExportPdf({ requestID: ++requestID, markdown: source, resourceSetID: 7, options }), /handle/);
  });
  parentPort.postMessage({ version: manifest.packageVersion, tag: manifest.source.tag,
    wasmSha256: hash(wasm), bridgeSha256: hash(bridge), results });
  parentPort.close();
}
