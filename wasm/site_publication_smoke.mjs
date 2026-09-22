// Real-engine smoke, deliberately separate from generated-binding-double tests.
// Run after building wasm/pkg from this checkout on a configured DSR host:
//   node wasm/site_publication_smoke.mjs
// No network, output files, substituted engine, or third-party ZIP dependency.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { inflateRawSync } from "node:zlib";

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}

// Independently decode the classic ZIP central directory and every payload.
// This smoke does not use any parser/compressor from the renderer under test.
function unzip(bytes) {
  const zip = Buffer.from(bytes);
  assert.ok(zip.length >= 22);
  const eocd = zip.length - 22;
  assert.equal(zip.readUInt32LE(eocd), 0x06054b50);
  assert.equal(zip.readUInt16LE(eocd + 20), 0);
  assert.equal(zip.readUInt16LE(eocd + 4), 0);
  assert.equal(zip.readUInt16LE(eocd + 6), 0);
  const count = zip.readUInt16LE(eocd + 10);
  assert.equal(zip.readUInt16LE(eocd + 8), count);
  let cursor = zip.readUInt32LE(eocd + 16);
  assert.equal(cursor + zip.readUInt32LE(eocd + 12), eocd);
  const entries = new Map();
  for (let i = 0; i < count; i++) {
    assert.ok(cursor + 46 <= eocd);
    assert.equal(zip.readUInt32LE(cursor), 0x02014b50);
    const method = zip.readUInt16LE(cursor + 10), crc = zip.readUInt32LE(cursor + 16);
    const compressed = zip.readUInt32LE(cursor + 20), expanded = zip.readUInt32LE(cursor + 24);
    const nameSize = zip.readUInt16LE(cursor + 28), extra = zip.readUInt16LE(cursor + 30);
    const comment = zip.readUInt16LE(cursor + 32), local = zip.readUInt32LE(cursor + 42);
    const next = cursor + 46 + nameSize + extra + comment;
    assert.ok(next <= eocd);
    const name = zip.subarray(cursor + 46, cursor + 46 + nameSize).toString("utf8");
    assert.ok(!entries.has(name), `duplicate ${name}`);
    assert.ok(local + 30 <= cursor);
    assert.equal(zip.readUInt32LE(local), 0x04034b50);
    assert.equal(zip.readUInt16LE(local + 8), method);
    const localNameSize = zip.readUInt16LE(local + 26), localExtra = zip.readUInt16LE(local + 28);
    assert.equal(zip.subarray(local + 30, local + 30 + localNameSize).toString("utf8"), name);
    const start = local + 30 + localNameSize + localExtra;
    assert.ok(start + compressed <= cursor);
    assert.ok(expanded <= 32 * 1024 * 1024, "smoke entry unexpectedly large");
    assert.ok(method === 0 || method === 8, `unsupported ZIP method ${method}`);
    const payload = zip.subarray(start, start + compressed);
    const decoded = method === 8 ? inflateRawSync(payload, { maxOutputLength: expanded || 1 }) : payload;
    assert.equal(decoded.length, expanded, name);
    assert.equal(crc32(decoded), crc, name);
    entries.set(name, decoded);
    cursor = next;
  }
  assert.equal(cursor, eocd);
  return entries;
}

async function main() {
  const { init, renderBookSite } = await import("./franken_markdown.js");
  await init(await readFile(new URL("./pkg/franken_markdown_bg.wasm", import.meta.url)));
  const chapters = [
    { path: "guide/start.md", source: "---\ntitle: Opening\nlang: de\n---\n# Shared\n\n[Next](../appendix/end.md#shared)\n\n{{#include snippet.md}}\n\n![First](figure.svg)\n\n<script>alert(1)</script>\n" },
    { path: "appendix/end.md", source: "# Shared\n\n[Back](../guide/start.md#shared)\n\n![Second](figure.svg)\n" },
  ];
  const resources = [{ path: "guide/snippet.md", source: "Included publication prose.\n" }];
  const encode = text => new TextEncoder().encode(text);
  const firstSvg = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16"/></svg>';
  const secondSvg = '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><circle cx="16" cy="16" r="8"/></svg>';
  const options = { title: "Offline manual", customCss: ".fmd{line-height:1.7}", lang: "fr",
    toc: true, tocDepth: 2, includeSources: resources,
    pdfImages: [
      { destination: "guide/figure.svg", bytes: encode(firstSvg) },
      { destination: "appendix/figure.svg", bytes: encode(secondSvg) },
    ],
  };
  const output = await renderBookSite(chapters, options);
  assert.equal(output.format, "book-site");
  assert.equal(output.extension, "zip");
  assert.equal(output.mimeType, "application/zip");
  assert.deepEqual(output.diagnostics, [], "legacy-package warnings must fail this proof");
  assert.equal(output.sourceLength, [...chapters, ...resources].reduce((sum, file) => sum + encode(file.source).length, 0));
  const entries = unzip(output.bytes), text = name => {
    assert.ok(entries.has(name), `missing ${name}`);
    return entries.get(name).toString("utf8");
  };
  const first = text("guide__start.html"), second = text("appendix__end.html");
  assert.ok(first.includes('href="appendix__end.html#shared"'));
  assert.ok(second.includes('href="guide__start.html#shared"'));
  assert.ok(first.includes('id="shared"') && second.includes('id="shared"'));
  assert.ok(first.includes("Included publication prose."));
  assert.ok(first.includes(".fmd{line-height:1.7}"));
  assert.ok(first.includes('lang="de"') && second.includes('lang="fr"'));
  assert.ok(first.includes(Buffer.from(firstSvg).toString("base64")));
  assert.ok(second.includes(Buffer.from(secondSvg).toString("base64")));
  assert.ok(!first.includes("<script>alert(1)</script>"));
  assert.ok(!entries.has("guide__snippet.html"));
  const index = JSON.parse(text("search-index.json"));
  assert.equal(index.schema, "fmd-book-search-index-v1");
  assert.deepEqual(index.chapters.map(chapter => [chapter.source, chapter.page]), [
    ["guide/start.md", "guide__start.html"], ["appendix/end.md", "appendix__end.html"],
  ]);
  for (const chapter of index.chapters) {
    const html = text(chapter.page);
    for (const entry of chapter.index.entries) {
      if (entry.anchor) assert.ok(html.includes(`id="${entry.anchor}"`), `${chapter.page}#${entry.anchor}`);
    }
  }
  const receipt = JSON.parse(text("frankenmarkdown-receipt.json"));
  assert.equal(receipt.schema, "fmd-book-receipt-v1");
  assert.equal(receipt.chapter_count, 2);
  assert.deepEqual(receipt.chapters.map(chapter => [chapter.path, chapter.output]),
    index.chapters.map(chapter => [chapter.source, chapter.page]));
  assert.ok(text("~fmd-search.html").includes('id="search-data"'));
  assert.ok(first.includes("./~fmd-search.html") && second.includes("./~fmd-search.html"));
  assert.ok(text("index.html").includes("Offline manual"));
  assert.deepEqual(output.bytes, (await renderBookSite(chapters, options)).bytes);
  process.stderr.write("PASS real-WASM site: links, assets, includes, CSS, language, receipt, search, ZIP CRCs and determinism\n");
}

try { await main(); }
catch (error) {
  process.stderr.write(`FAIL real-WASM site: ${String(error?.message ?? error)}\n`);
  process.exitCode = 1;
}
