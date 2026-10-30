// Admission only, not an image codec. Pixels are decoded by createImageBitmap.
// Walk containers before decoding so size limits do not depend on allocation
// succeeding. No SVG, animation, external references or MIME/extension guessing.
export class FlowAssetError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = "FlowAssetError";
    this.code = code;
  }
}
export const assetFailure = (code, message) => new FlowAssetError(code, message);
const invalid = () => {
  throw assetFailure("INVALID_IMAGE", "invalid or unsupported raster image structure");
};

export function rasterInfo(bytes, limits) {
  if (
    !ArrayBuffer.isView(bytes) ||
    Object.prototype.toString.call(bytes) !== "[object Uint8Array]" ||
    Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
  )
    invalid();
  // This also rejects a detached zero-length buffer. Shared bytes cannot be
  // safely inspected and then snapshotted without a concurrent writer.
  let view;
  try {
    view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  } catch {
    invalid();
  }
  if (!bytes.length || bytes.length > limits.maxAssetBytes) {
    throw assetFailure("ASSET_BYTE_LIMIT", "image exceeds the encoded-byte limit");
  }
  let info;
  if ([137, 80, 78, 71, 13, 10, 26, 10].every((v, i) => bytes[i] === v)) info = png(bytes, view);
  else if (bytes[0] === 0xff && bytes[1] === 0xd8) info = jpeg(bytes, view);
  else
    throw assetFailure(
      "UNSUPPORTED_IMAGE",
      "only static PNG and 8-bit baseline/progressive JPEG are supported",
    );
  const pixels = info.width * info.height;
  if (
    !info.width ||
    !info.height ||
    info.width > limits.maxDimension ||
    info.height > limits.maxDimension ||
    !Number.isSafeInteger(pixels) ||
    pixels > limits.maxImagePixels
  ) {
    throw assetFailure("ASSET_PIXEL_LIMIT", "image exceeds the dimension or decoded-pixel limit");
  }
  return Object.freeze({ ...info, pixels });
}

function png(bytes, view) {
  let offset = 8,
    info = null,
    data = false,
    dataEnded = false,
    palette = false;
  while (offset + 12 <= bytes.length) {
    const length = view.getUint32(offset),
      end = offset + 12 + length;
    if (length > 0x7fffffff || end > bytes.length) invalid();
    const kind = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
    const body = offset + 8;
    if (!/^[A-Za-z]{4}$/.test(kind) || bytes[offset + 6] & 32) invalid();
    if (offset === 8 && kind !== "IHDR") invalid();
    if (["acTL", "fcTL", "fdAT"].includes(kind)) {
      throw assetFailure(
        "UNSUPPORTED_IMAGE",
        "animated PNG is not admitted by the static-image loader",
      );
    }
    if (kind === "IHDR") {
      if (info || offset !== 8 || length !== 13) invalid();
      const color = bytes[body + 9],
        depth = bytes[body + 8];
      const depths = { 0: [1, 2, 4, 8, 16], 2: [8, 16], 3: [1, 2, 4, 8], 4: [8, 16], 6: [8, 16] };
      if (
        !depths[color]?.includes(depth) ||
        bytes[body + 10] !== 0 ||
        bytes[body + 11] !== 0 ||
        bytes[body + 12] > 1
      )
        invalid();
      info = {
        mime: "image/png",
        width: view.getUint32(body),
        height: view.getUint32(body + 4),
        color,
      };
    } else if (kind === "PLTE") {
      if (palette || data || !length || length > 768 || length % 3 || [0, 4].includes(info.color))
        invalid();
      palette = true;
    } else if (kind === "IDAT") {
      if (dataEnded || (info.color === 3 && !palette)) invalid();
      data = true;
    } else if (kind === "IEND") {
      if (length || !data || end !== bytes.length) invalid();
      return { mime: info.mime, width: info.width, height: info.height };
    } else {
      if (!(bytes[offset + 4] & 32)) invalid(); // Unknown critical chunk.
      if (data) dataEnded = true;
      // Compressed ancillary metadata can expand independently of image size.
      if (["iCCP", "zTXt", "iTXt"].includes(kind)) {
        throw assetFailure(
          "UNSUPPORTED_IMAGE",
          "compressed PNG metadata is not admitted by this bounded decoder profile",
        );
      }
    }
    offset = end;
  }
  invalid();
}

function jpeg(bytes, view) {
  let offset = 2,
    info = null,
    scan = false,
    sawScan = false;
  while (offset < bytes.length) {
    if (scan) {
      while (offset < bytes.length && bytes[offset] !== 0xff) offset++;
    }
    if (bytes[offset++] !== 0xff) invalid();
    while (bytes[offset] === 0xff) offset++;
    const marker = bytes[offset++];
    if (scan && (marker === 0 || (marker >= 0xd0 && marker <= 0xd7))) continue;
    scan = false;
    if (marker === 0xd9) {
      if (!info || !sawScan || offset !== bytes.length) invalid();
      return info;
    }
    if (offset + 2 > bytes.length) invalid();
    const length = view.getUint16(offset),
      end = offset + length;
    if (length < 2 || end > bytes.length) invalid();
    if (marker === 0xc0 || marker === 0xc2) {
      if (info || sawScan || length < 8 || bytes[offset + 2] !== 8) invalid();
      const components = bytes[offset + 7];
      if (![1, 3].includes(components) || length !== 8 + 3 * components) invalid();
      info = {
        mime: "image/jpeg",
        height: view.getUint16(offset + 3),
        width: view.getUint16(offset + 5),
      };
    } else if (marker === 0xda) {
      if (!info || length < 6 || length !== 6 + 2 * bytes[offset + 2]) invalid();
      scan = true;
      sawScan = true;
    } else if (
      !(
        marker === 0xc4 ||
        marker === 0xdb ||
        marker === 0xdd ||
        marker === 0xfe ||
        (marker >= 0xe0 && marker <= 0xef)
      )
    )
      invalid();
    offset = end;
  }
  invalid();
}

export async function decodeRaster(blob) {
  if (typeof globalThis.createImageBitmap !== "function") {
    throw assetFailure(
      "IMAGE_DECODER_UNAVAILABLE",
      "createImageBitmap is unavailable; supply a host decoder",
    );
  }
  // EXIF orientation may swap JPEG dimensions. The manager checks actual
  // dimensions before submitting them, including this allowed transposition.
  return globalThis.createImageBitmap(blob);
}
