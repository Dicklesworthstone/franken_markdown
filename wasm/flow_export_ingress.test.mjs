import assert from "node:assert/strict";
import test from "node:test";
import { normalizeFlowRequest, requestWeight, snapshotArguments } from "./flow_worker_protocol.mjs";

const token = { revision: "3", layoutRevision: "4" };
for (const [method, argsFor, pdfOptions] of [
  ["exportDocument", options => ["pdf", options, token], args => args[1]],
  ["exportDocuments", options => [[{ format: "html" }, { format: "pdf", options }], token],
    args => args[0][1].options],
  ["verifyPdf", options => [options, token], args => args[0]],
]) {
  test(`${method} charges borrowed image views, then transfers only the admitted ranges`, () => {
    const buffer = Uint8Array.of(90, 1, 2, 3, 4, 91);
    const options = { pdfImages: [
      { destination: "a.png", bytes: buffer.subarray(1, 3) },
      { destination: "b.png", bytes: buffer.subarray(2, 5) },
    ] };
    const prepared = normalizeFlowRequest(method, argsFor(options));
    const admitted = pdfOptions(prepared).pdfImages;
    for (let i = 0; i < admitted.length; i++) {
      assert.equal(admitted[i].bytes.buffer, buffer.buffer, "queue admission must not copy image bytes");
      assert.equal(admitted[i].bytes.byteOffset, options.pdfImages[i].bytes.byteOffset);
    }
    const smaller = normalizeFlowRequest(method, argsFor({ pdfImages: options.pdfImages.map(
      image => ({ ...image, bytes: image.bytes.subarray(0, 1) })) }));
    assert.equal(requestWeight(prepared) - requestWeight(smaller), 3, "charge view lengths, including overlaps");

    const payload = snapshotArguments(method, prepared);
    assert.deepEqual(payload.transfer.map(range => range.byteLength), [2, 3]);
    assert.deepEqual(payload.transfer, pdfOptions(payload.args).pdfImages.map(image => image.bytes.buffer));
    buffer.fill(8);
    const wire = structuredClone(payload.args, { transfer: payload.transfer });
    assert.deepEqual(pdfOptions(wire).pdfImages.map(image => [...image.bytes]), [[1, 2], [2, 3, 4]]);
    assert.equal(buffer.byteLength, 6, "the caller's larger backing buffer stays attached");
    assert.ok(payload.transfer.every(range => range.byteLength === 0));
  });

  test(`${method} rejects an image detached between admission and capture`, () => {
    const bytes = Uint8Array.of(1, 2);
    const prepared = normalizeFlowRequest(method, argsFor({ pdfImages: [{ destination: "a.png", bytes }] }));
    structuredClone(bytes, { transfer: [bytes.buffer] });
    assert.throws(() => snapshotArguments(method, prepared), { code: "INVALID_OPTIONS" });
  });
}
