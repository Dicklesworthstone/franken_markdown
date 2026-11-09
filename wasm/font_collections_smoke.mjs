// Real generated-WASM proof for TTC/OTC extraction and font-slot compatibility.
// Usage: node wasm/font_collections_smoke.mjs <assembled-package-dir> <generated.wasm>
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const [packageArgument, wasmArgument] = process.argv.slice(2);
if (!packageArgument || !wasmArgument)
  throw new Error("expected assembled package and generated WASM paths");
const packageDir = path.resolve(packageArgument);
const api = await import(pathToFileURL(path.join(packageDir, "franken_markdown.js")).href);
const native = await import(pathToFileURL(path.join(packageDir, "pkg/franken_markdown.js")).href);
await api.init(new Uint8Array(await readFile(path.resolve(wasmArgument))));
const bytes = async file => new Uint8Array(await readFile(new URL(`../fmd-font/fonts/${file}`, import.meta.url)));
const collections = await Promise.all(["test-collection/FmdMixedFacesV1.ttc", "test-collection/FmdMixedFacesV2.otc"].map(bytes));
const expected = await Promise.all(["test-collection/FmdCollectionTrueType.ttf", "test-cid-cff/FmdCidFormat3.otf"].map(bytes));

const caps = await api.capabilities();
assert.deepEqual(caps.font_collections, { formats: ["ttc", "otc"], versions: [1, 2],
  max_bytes: 33554432, max_faces: 1024, output: "standalone_sfnt" });
for (const collection of collections) {
  assert.equal(await api.fontCollectionFaceCount(collection), 2);
  for (let index = 0; index < expected.length; index++) {
    const padded = new Uint8Array(collection.length + 8).fill(0xee);
    padded.set(collection, 4);
    const pending = api.extractFontCollectionFace(new DataView(padded.buffer, 4, collection.length), index);
    padded.fill(0);
    assert.deepEqual(await pending, expected[index], `exact standalone bytes for face ${index}`);
  }
  await assert.rejects(api.extractFontCollectionFace(collection, 2), { code: "FONT_COLLECTION_FACE_INDEX" });
}
await assert.rejects(api.fontCollectionFaceCount(expected[0]), { code: "FONT_COLLECTION_NOT_COLLECTION" });
await assert.rejects(api.extractFontCollectionFace(Uint8Array.of(116, 116, 99, 102), 0), { code: "FONT_COLLECTION_MALFORMED" });
for (const index of [-1, 0.5, 1024, 2 ** 32, NaN, Infinity]) {
  await assert.rejects(api.extractFontCollectionFace(collections[0], index), { code: "FONT_COLLECTION_FACE_INDEX" });
  // Bypass the handwritten facade: raw ABI numbers must not truncate/wrap.
  assert.throws(() => native.extractFontCollectionFace(collections[0], index), error =>
    typeof error === "string" && error.startsWith("FONT_COLLECTION_FACE_INDEX:"));
}

const renderer = await api.createRenderer();
for (let index = 0; index < expected.length; index++) {
  const extracted = await renderer.extractFontCollectionFace(collections[1], index);
  const options = font => ({ fontAssets: [{ slot: "body-regular", bytes: font }] });
  for (const method of ["renderHtml", "renderPdf", "renderSvg", "renderEpub"]) {
    const actual = await renderer[method]("A B 日", options(extracted));
    const reference = await renderer[method]("A B 日", options(expected[index]));
    assert.deepEqual(actual.bytes, reference.bytes, `${method} accepts extracted face ${index}`);
  }
}
console.log("font-collections: real TTC/OTC headers, both outlines, errors, ownership and HTML/PDF/SVG/EPUB parity passed");
