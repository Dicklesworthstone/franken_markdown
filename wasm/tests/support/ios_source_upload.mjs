import assert from "node:assert/strict";

export const sourceChunkSize = 65_536;

// Deliberately independent of the bridge's slicing-by-8 implementation.
export function sourceCRC32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return ((crc ^ 0xffffffff) >>> 0).toString(16).padStart(8, "0");
}

export function sourceChunk(sourceID, index, bytes) {
  return {sourceID, index, byteLength: bytes.length,
    base64: Buffer.from(bytes).toString("base64"), checksum: sourceCRC32(bytes)};
}

// Test fixtures may own complete Buffers; production Swift traverses its
// String.UTF8View and allocates only the next chunk.
export function uploadSourceBytes(window, bytes, sourceID = 1) {
  const manifest = {sourceID, byteLength: bytes.length, chunkSize: sourceChunkSize};
  assert.deepEqual(JSON.parse(window.frankenBeginSource(manifest)), manifest);
  let chunkCount = 0, receivedBytes = 0;
  for (let start = 0; start < bytes.length; start += sourceChunkSize) {
    const part = bytes.subarray(start, start + sourceChunkSize);
    const acknowledged = JSON.parse(window.frankenAppendSourceChunk(sourceChunk(sourceID, chunkCount, part)));
    receivedBytes += part.length;
    assert.deepEqual(acknowledged, {sourceID, index: chunkCount, receivedBytes});
    chunkCount++;
  }
  const receipt = {sourceID, byteLength: bytes.length, chunkCount};
  assert.deepEqual(JSON.parse(window.frankenFinishSource(receipt)), receipt);
  return receipt;
}

export function uploadSource(window, markdown, sourceID = 1) {
  return uploadSourceBytes(window, Buffer.from(markdown, "utf8"), sourceID);
}
