import assert from "node:assert/strict";
import test from "node:test";
import { resourceChunkSize, resourceManifest, resourceChunk, uploadResources,
  readResourceExport, resourceOutput, resourceHarness } from "./support/ios_resource_upload.mjs";

const image = (key, bytes = Uint8Array.of(0, 255, 128, 1)) => ({ kind: "image", key, bytes });
const font = (key = "body-regular", bytes = Uint8Array.of(0, 1, 0, 0)) => ({ kind: "font", key, bytes });
const deferred = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const render = (h, resourceSetID, requestID = 1, options = {}) =>
  h.window.frankenRender({ requestID, markdown: "# Snapshot", resourceSetID, options });
const finish = (h, resourceSetID, byteLength, chunkCount, entryCount) =>
  JSON.parse(h.window.frankenFinishResources({ resourceSetID, byteLength, chunkCount, entryCount }));

for (const size of [1, 65_535, 65_536, 65_537, 2 * 65_536 + 7, 1_048_577]) {
  test(`${size} attachment bytes use bounded ordered chunks without text conversion`, async () => {
    const h = await resourceHarness(), bytes = Uint8Array.from({ length: size }, (_, index) => index % 256);
    const receipt = uploadResources(h.window, [image("figure.png", bytes), font()], 7);
    assert.equal(receipt.chunkCount, Math.ceil(size / resourceChunkSize) + 1);
    assert.equal(receipt.byteLength, size + 4);
    assert.equal(await render(h, 7), true);
    assert.deepEqual(h.calls[0].options.pdfImages[0].bytes, bytes);
    assert.deepEqual(h.calls[0].options.fontAssets[0].bytes, font().bytes);
    assert.equal(h.decodedSizes.length, receipt.chunkCount);
    assert(h.decodedSizes.every(length => length <= Math.ceil(resourceChunkSize / 3) * 4));
  });
}

test("canonically equivalent Unicode destinations remain distinct exact keys", async () => {
  const h = await resourceHarness();
  const entries = [image("caf\u00e9.png", Uint8Array.of(1)), image("cafe\u0301.png", Uint8Array.of(2)),
    image("figures/\u{1f9ea}.png", Uint8Array.of(3))];
  uploadResources(h.window, entries);
  assert.equal(await render(h, 1), true);
  assert.deepEqual(Array.from(h.calls[0].options.pdfImages, value => value.destination), entries.map(value => value.key));
  assert.deepEqual(Array.from(h.calls[0].options.pdfImages, value => [...value.bytes]), [[1], [2], [3]]);
});

test("manifest and chunk caller mutations cannot change an admitted resource set", async () => {
  const h = await resourceHarness(), bytes = Uint8Array.of(1, 2, 3), entries = [image("original.png", bytes)];
  const manifest = resourceManifest(1, entries);
  h.window.frankenBeginResources(manifest);
  manifest.entries[0].key = "changed.png"; manifest.entries[0].kind = "font"; manifest.entries[0].byteLength = 99;
  const chunk = resourceChunk(1, 0, 0, bytes);
  h.window.frankenAppendResourceChunk(chunk); bytes.fill(9); chunk.base64 = "AAAA";
  finish(h, 1, 3, 1, 1);
  await render(h, 1);
  assert.equal(h.calls[0].options.pdfImages[0].destination, "original.png");
  assert.deepEqual([...h.calls[0].options.pdfImages[0].bytes], [1, 2, 3]);
});

test("one completed set reaches preview and every resource-aware export with no inline IPC payload", async () => {
  const h = await resourceHarness(), entries = [image("diagram.svg"), font(), font("mono-regular")];
  uploadResources(h.window, entries, 7);
  const common = { markdown: "# One source", resourceSetID: 7, options: { title: "Captured" } };
  assert.equal(await h.window.frankenRender({ ...common, requestID: 1 }), true);
  h.window.frankenCommitPreview(1);
  readResourceExport(h.window, await h.window.frankenExportHtml({ ...common, requestID: 2 }));
  readResourceExport(h.window, await h.window.frankenExportPdf({ ...common, requestID: 3 }));
  let requestID = 3;
  for (const format of ["svg", "epub", "interactive-html"])
    readResourceExport(h.window, await h.window.frankenExportArtifact({ ...common, requestID: ++requestID, format }));
  for (const format of ["pdf", "site"])
    readResourceExport(h.window, await h.window.frankenBook({ ...common, requestID: ++requestID, format,
      files: [{ path: "chapter.md", source: "# One chapter" }] }));
  assert.equal(h.calls.length, 8);
  for (const call of h.calls) {
    assert.equal(call.options.pdfImages[0].destination, "diagram.svg");
    assert.deepEqual(Array.from(call.options.fontAssets, value => value.slot), ["body-regular", "mono-regular"]);
    assert.deepEqual(call.options.pdfImages[0].bytes, entries[0].bytes);
  }
  assert.equal(Object.hasOwn(common.options, "pdfImages"), false);
  assert.equal(Object.hasOwn(common.options, "fontAssets"), false);
});

test("missing, stale and cancelled handles never inherit resident attachment authority", async () => {
  const h = await resourceHarness(); uploadResources(h.window, [image("old.png")], 1);
  assert.equal(await h.window.frankenRender({ requestID: 1, markdown: "No explicit resources", options: {} }), true);
  assert.equal(Object.hasOwn(h.calls.at(-1).options, "pdfImages"), false);
  uploadResources(h.window, [image("new.png")], 2);
  assert.equal(await render(h, 1, 2), false);
  await assert.rejects(h.window.frankenExportPdf({ requestID: 1, markdown: "Fallback", resourceSetID: 1 }), /handle/);
  assert.equal(h.window.frankenCancelResources(1), false);
  assert.equal(await render(h, 2, 3), true);
  assert.equal(h.calls.at(-1).options.pdfImages[0].destination, "new.png");
  assert.equal(h.window.frankenCancelResources(2), true);
  assert.equal(await render(h, 2, 4), false);
  assert.equal(h.calls.length, 2);
});

test("empty explicitly completed sets clear resources without ambient inheritance", async () => {
  const h = await resourceHarness(); uploadResources(h.window, [image("old.png"), font()], 1);
  assert.deepEqual(uploadResources(h.window, [], 2), { resourceSetID: 2, byteLength: 0, chunkCount: 0, entryCount: 0 });
  await render(h, 2);
  assert.equal(Object.hasOwn(h.calls[0].options, "pdfImages"), false);
  assert.equal(Object.hasOwn(h.calls[0].options, "fontAssets"), false);
  assert.equal(await render(h, 1, 2), false);
});

test("handles cannot be combined with a second inline resource grant", async () => {
  const h = await resourceHarness(); uploadResources(h.window, [image("one.png")]);
  let requestID = 0;
  for (const options of [{ pdfImages: [] }, { fontAssets: [] }, { pdfImages: undefined }]) {
    await assert.rejects(h.window.frankenExportPdf({ requestID: ++requestID, markdown: "Source", resourceSetID: 1, options }), /mixed/);
  }
  assert.equal(h.calls.length, 0);
});

test("replacement sets publish atomically only after the final receipt", async () => {
  const h = await resourceHarness(); uploadResources(h.window, [image("resident.png")], 1);
  const bytes = new Uint8Array(resourceChunkSize + 1).fill(9), manifest = resourceManifest(2, [image("pending.png", bytes)]);
  h.window.frankenBeginResources(manifest);
  assert.equal(await render(h, 2, 1), false);
  assert.equal(await render(h, 1, 2), true, "an unfinished replacement preserves the existing complete set");
  h.window.frankenAppendResourceChunk(resourceChunk(2, 0, 0, bytes.subarray(0, resourceChunkSize)));
  for (const receipt of [[bytes.length, 1, 1], [bytes.length - 1, 2, 1], [bytes.length, 2, 2]])
    assert.throws(() => finish(h, 2, ...receipt), /incomplete/);
  h.window.frankenAppendResourceChunk(resourceChunk(2, 0, 1, bytes.subarray(resourceChunkSize)));
  finish(h, 2, bytes.length, 2, 1);
  assert.equal(await render(h, 2, 3), true);
  assert.equal(h.calls.at(-1).options.pdfImages[0].destination, "pending.png");
  assert.equal(await render(h, 1, 4), false);
});

test("resource ownership, chunk order and CRC are checked before advancing state", async () => {
  const h = await resourceHarness(), bytes = new Uint8Array(resourceChunkSize + 1).fill(0x80);
  h.window.frankenBeginResources(resourceManifest(7, [image("one.png", bytes), font()]));
  const first = resourceChunk(7, 0, 0, bytes.subarray(0, resourceChunkSize));
  for (const invalid of [
    { ...first, resourceSetID: 6 }, { ...first, resourceSetID: 8 }, { ...first, index: -1 },
    { ...first, index: 1 }, { ...first, index: 0.5 }, { ...first, index: "0" },
    { ...first, assetIndex: 1 }, { ...first, assetIndex: "0" }, { ...first, byteLength: 1 },
    { ...first, base64: first.base64 + "A" }, { ...first, checksum: "bad" },
    { ...first, checksum: "00000000" }, { ...first, base64: "!".repeat(first.base64.length) },
  ]) assert.throws(() => h.window.frankenAppendResourceChunk(invalid));
  assert.equal(h.allocations.includes(bytes.length), false, "invalid chunks do not allocate their declared whole asset");
  h.window.frankenAppendResourceChunk(first);
  assert.throws(() => h.window.frankenAppendResourceChunk(first), /order/);
  h.window.frankenAppendResourceChunk(resourceChunk(7, 0, 1, bytes.subarray(resourceChunkSize)));
  assert.throws(() => h.window.frankenAppendResourceChunk(resourceChunk(7, 1, 0, font().bytes)), /order/);
  h.window.frankenAppendResourceChunk(resourceChunk(7, 1, 2, font().bytes));
  finish(h, 7, bytes.length + 4, 3, 2);
  await render(h, 7); assert.deepEqual(h.calls[0].options.pdfImages[0].bytes, bytes);
});

test("invalid manifest budgets and keys preserve the resident set without decoding payloads", async () => {
  const h = await resourceHarness(); uploadResources(h.window, [image("resident.png")], 1);
  const base = resourceManifest(2, [image("next.png")]);
  const entry = overrides => ({ ...base, entries: [{ ...base.entries[0], ...overrides }] });
  const many = count => ({ ...base, byteLength: count,
    entries: Array.from({ length: count }, (_, i) => ({ kind: "image", key: `${i}.png`, byteLength: 1 })) });
  const invalid = [
    ...[0, -1, 1, 1.5, "2", Number.MAX_SAFE_INTEGER + 1].map(resourceSetID => ({ ...base, resourceSetID })),
    ...[-1, 0.5, 32 * 1024 * 1024 + 1].map(byteLength => ({ ...base, byteLength })),
    { ...base, chunkSize: 1 }, { ...base, byteLength: 3 }, many(129), many(134),
    entry({ kind: "script" }), entry({ key: "" }), entry({ key: " " }), entry({ key: "a\0b" }),
    entry({ key: "a\nb" }), entry({ key: "é".repeat(2049) }), entry({ key: "x".repeat(4097) }),
    entry({ kind: "font", key: "unknown" }), entry({ byteLength: 0 }), entry({ byteLength: 0.5 }),
    entry({ byteLength: 16 * 1024 * 1024 + 1 }),
    { ...base, byteLength: 8, entries: [base.entries[0], base.entries[0]] },
  ];
  const decoded = h.decodedSizes.length, allocations = h.allocations.length;
  for (const value of invalid) assert.throws(() => h.window.frankenBeginResources(value));
  assert.equal(h.decodedSizes.length, decoded); assert.equal(h.allocations.length, allocations);
  assert.equal(await render(h, 1), true);
  assert.equal(h.calls[0].options.pdfImages[0].destination, "resident.png");
  uploadResources(h.window, [image("next.png")], 2); // Invalid requests did not consume the next identifier.
});

test("malformed UTF16 destinations cannot collapse onto a different UTF8 key", async () => {
  const h = await resourceHarness();
  for (const key of ["\ud800.png", "\udfff.png", "a\ud800b", "\udc00\ud800"])
    assert.throws(() => h.window.frankenBeginResources(resourceManifest(1, [image(key)])), /invalid/);
  uploadResources(h.window, [image("\u{1f9ea}.png")], 1);
  assert.equal(await render(h, 1), true);
});

test("cancelled and late uploads cannot overwrite a newer completed set", async () => {
  const h = await resourceHarness(), bytes = new Uint8Array(resourceChunkSize + 1).fill(1);
  h.window.frankenBeginResources(resourceManifest(1, [image("old.png", bytes)]));
  assert.throws(() => h.window.frankenBeginResources(resourceManifest(2, [image("new.png")])), /still in progress/);
  h.window.frankenAppendResourceChunk(resourceChunk(1, 0, 0, bytes.subarray(0, resourceChunkSize)));
  assert.equal(h.window.frankenCancelResources(1), true);
  assert.throws(() => h.window.frankenBeginResources(resourceManifest(1, [image("old.png")])), /increase/);
  uploadResources(h.window, [image("new.png")], 2);
  assert.throws(() => h.window.frankenAppendResourceChunk(resourceChunk(1, 0, 1, bytes.subarray(resourceChunkSize))), /no longer/);
  assert.throws(() => finish(h, 1, bytes.length, 2, 1), /no longer/);
  assert.equal(h.window.frankenCancelResources(1), false);
  await render(h, 2); assert.equal(h.calls[0].options.pdfImages[0].destination, "new.png");
});

test("a captured render retains its original set when a later upload replaces the cache", async () => {
  const gate = deferred(), calls = [];
  const h = await resourceHarness({ renderer: { async renderHtml(source, options) {
    calls.push({ source, options }); await gate.promise; return resourceOutput();
  } } });
  uploadResources(h.window, [image("old.png", Uint8Array.of(1))], 1);
  const pending = render(h, 1);
  uploadResources(h.window, [image("new.png", Uint8Array.of(2))], 2);
  assert.equal(calls[0].options.pdfImages[0].destination, "old.png");
  assert.deepEqual([...calls[0].options.pdfImages[0].bytes], [1]);
  gate.resolve(); assert.equal(await pending, true);
  // Native request/generation checks separately decide whether to commit this
  // staged preview; replacement does not rewrite an in-flight render's inputs.
  assert.equal(h.window.frankenDiscardPreview(1), true);
  assert.equal(h.window.frankenCommitPreview(1), false);
});
