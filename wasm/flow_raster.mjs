import { parseStaticSvg } from "./flow_svg.mjs";

// Admission only, not an image codec. Pixels are decoded by browser image APIs.
// Walk containers before decoding so size limits do not depend on allocation
// succeeding. GIFs supply a fixed first-frame preview; publication retains the
// complete original file. Static SVG has a separate bounded inline-only profile.
// No external references or MIME/extension guessing.
export class FlowAssetError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = "FlowAssetError";
    this.code = code;
  }
}
export const assetFailure = (code, message) => new FlowAssetError(code, message);
const invalid = () => {
  throw assetFailure("INVALID_IMAGE", "invalid or unsupported image structure");
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
  else if ([71, 73, 70].every((v, i) => bytes[i] === v)) info = gif(bytes, view);
  else {
    // Never infer an SVG MIME from a filename. Decode only a plausible XML
    // prefix, with strict UTF-8 and a smaller bounded static SVG profile.
    if (![9, 10, 13, 32, 60, 239].includes(bytes[0])) {
      throw assetFailure("UNSUPPORTED_IMAGE", "unsupported image container");
    }
    if (bytes.length > 2 * 1024 * 1024)
      throw assetFailure("ASSET_BYTE_LIMIT", "SVG exceeds the 2 MiB source limit");
    let source;
    try { source = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
    catch (cause) { throw new FlowAssetError("INVALID_IMAGE", "invalid SVG UTF-8", { cause }); }
    if (!source.trimStart().startsWith("<"))
      throw assetFailure("UNSUPPORTED_IMAGE", "unsupported image container");
    info = parseStaticSvg(source, assetFailure);
    if (info.previewByteLength > limits.maxAssetBytes)
      throw assetFailure("ASSET_BYTE_LIMIT", "SVG preview exceeds the encoded-byte limit");
  }
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

// Walk every block through the trailer, but never decode animation frames.
// A preview ends just after the first image's data terminator. The manager
// appends a trailer to that immutable prefix so even a host decoder receives
// exactly one frame; the complete source Blob is retained separately.
function gif(bytes, view) {
  if (bytes.length < 13 || !["GIF87a", "GIF89a"].includes(
    String.fromCharCode(...bytes.subarray(0, 6)),
  )) invalid();
  const width = view.getUint16(6, true), height = view.getUint16(8, true);
  const globalColors = bytes[10] & 0x80 ? 1 << ((bytes[10] & 7) + 1) : 0;
  let offset = 13 + globalColors * 3, firstFrameEnd, control = false;
  if (offset > bytes.length || (globalColors && bytes[11] >= globalColors)) invalid();
  const blocks = () => {
    let size = 0;
    while (offset < bytes.length) {
      const length = bytes[offset++];
      if (!length) return size;
      if (length > bytes.length - offset) invalid();
      size += length;
      offset += length;
    }
    invalid();
  };
  while (offset < bytes.length) {
    const kind = bytes[offset++];
    if (kind === 0x3b) {
      if (firstFrameEnd === undefined || control || offset !== bytes.length) invalid();
      return { mime: "image/gif", width, height, firstFrameEnd };
    }
    if (kind === 0x21) {
      const extension = bytes[offset++];
      if (extension === 0xf9) {
        // At most one graphic control belongs to each image. Reserved bits,
        // undefined disposal methods and unterminated controls are malformed.
        if (control || offset + 6 > bytes.length || bytes[offset] !== 4
            || bytes[offset + 5] !== 0 || (bytes[offset + 1] & 0xe0)
            || ((bytes[offset + 1] >> 2) & 7) > 3) invalid();
        control = true;
        offset += 6;
      } else if (extension === 0xfe) {
        blocks(); // Comment sub-blocks have no graphics or allocation semantics.
      } else if (extension === 0xff) {
        if (bytes[offset++] !== 11 || offset + 11 > bytes.length) invalid();
        offset += 11;
        blocks(); // Application data, including the bounded animation loop count.
      } else {
        // Plain-text graphics need a font. Unknown graphic extensions cannot
        // be treated as harmless metadata without knowing their semantics.
        throw assetFailure("UNSUPPORTED_IMAGE", "GIF plain-text and unknown extensions are not supported");
      }
      continue;
    }
    if (kind !== 0x2c || offset + 9 > bytes.length) invalid();
    const left = view.getUint16(offset, true), top = view.getUint16(offset + 2, true);
    const frameWidth = view.getUint16(offset + 4, true), frameHeight = view.getUint16(offset + 6, true);
    const packed = bytes[offset + 8];
    if (!frameWidth || !frameHeight || left + frameWidth > width || top + frameHeight > height
        || (packed & 0x18)) invalid();
    const localColors = packed & 0x80 ? 1 << ((packed & 7) + 1) : 0;
    const colors = localColors || globalColors;
    if (!colors) invalid();
    offset += 9 + localColors * 3;
    if (offset >= bytes.length || bytes[offset] < 2 || bytes[offset] > 8) invalid();
    offset++;
    if (!blocks()) invalid();
    firstFrameEnd ??= offset;
    control = false;
  }
  invalid();
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
    sawScan = false,
    colourFrame = null,
    adobe = null,
    badAdobe = false;
  const colourKnown = () => !badAdobe && (adobe === 0 || adobe === 2);
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
      if (!info || !sawScan || offset !== bytes.length || (colourFrame && !colourKnown())) invalid();
      return info;
    }
    if (offset + 2 > bytes.length) invalid();
    const length = view.getUint16(offset),
      end = offset + length;
    if (length < 2 || end > bytes.length) invalid();
    // Match the publication core's explicit Adobe polarity contract. Do not
    // guess how an unmarked four-channel frame should be interpreted. Identical
    // duplicates are harmless; malformed/conflicting APP14 metadata is not.
    if (marker === 0xee && length >= 7
        && [65, 100, 111, 98, 101].every((v, i) => bytes[offset + 2 + i] === v)) {
      if (length < 14) badAdobe = true;
      else {
        const transform = bytes[offset + 13];
        if (adobe !== null && adobe !== transform) badAdobe = true;
        adobe = transform;
      }
    }
    if (marker === 0xc0 || marker === 0xc2) {
      if (info || sawScan || length < 8 || bytes[offset + 2] !== 8) invalid();
      const components = bytes[offset + 7];
      if (![1, 3, 4].includes(components) || length !== 8 + 3 * components) invalid();
      if (components === 4) colourFrame = jpegColourFrame(bytes, offset, marker === 0xc2);
      info = {
        mime: "image/jpeg",
        height: view.getUint16(offset + 3),
        width: view.getUint16(offset + 5),
      };
    } else if (marker === 0xda) {
      if (!info || length < 6 || length !== 6 + 2 * bytes[offset + 2]) invalid();
      if (colourFrame) {
        // Metadata must precede the first scan, as in the native passthrough.
        // Walk every progressive scan; the browser still owns entropy decoding.
        if (!colourKnown() || end + 2 >= bytes.length) invalid();
        jpegColourScan(bytes, offset, colourFrame);
      }
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

function jpegColourFrame(bytes, offset, progressive) {
  const ids = [];
  for (let i = 0; i < 4; i++) {
    const at = offset + 8 + 3 * i;
    const id = bytes[at], sampling = bytes[at + 1], table = bytes[at + 2];
    if (ids.includes(id) || (sampling >> 4) < 1 || (sampling >> 4) > 4
        || (sampling & 15) < 1 || (sampling & 15) > 4 || table > 3) invalid();
    ids.push(id);
  }
  return { ids, progressive };
}

function jpegColourScan(bytes, offset, frame) {
  const count = bytes[offset + 2], seen = new Set();
  if (count < 1 || count > 4) invalid();
  for (let i = 0; i < count; i++) {
    const at = offset + 3 + 2 * i, id = bytes[at], tables = bytes[at + 1];
    if (!frame.ids.includes(id) || seen.has(id) || (tables >> 4) > 3 || (tables & 15) > 3) invalid();
    seen.add(id);
  }
  const tail = offset + 3 + 2 * count;
  const start = bytes[tail], end = bytes[tail + 1], approximation = bytes[tail + 2];
  if (frame.progressive) {
    if (start > end || end > 63 || (approximation >> 4) > 13 || (approximation & 15) > 13
        || (start === 0 && end !== 0) || (start !== 0 && count !== 1)) invalid();
  } else if (start !== 0 || end !== 63 || approximation !== 0) invalid();
}

// Some browsers decode SVG Blobs only through an image element. The element
// is detached, uses a local object URL for the admitted preview, and survives
// until bitmap extraction settles. It is never inserted into the host document.
async function svgBitmap(blob, signal) {
  if (typeof globalThis.Image !== "function" ||
      typeof globalThis.URL?.createObjectURL !== "function" ||
      typeof globalThis.URL?.revokeObjectURL !== "function") {
    throw assetFailure("IMAGE_DECODER_UNAVAILABLE", "SVG image decoding is unavailable; supply a host decoder");
  }
  signal?.throwIfAborted();
  const url = globalThis.URL.createObjectURL(blob);
  let image, abort;
  try {
    image = new globalThis.Image();
    await new Promise((resolve, reject) => {
      const finish = callback => value => {
        image.onload = image.onerror = null;
        signal?.removeEventListener("abort", abort);
        callback(value);
      };
      image.onload = finish(resolve);
      image.onerror = finish(() => reject(assetFailure("INVALID_IMAGE", "the browser could not decode the static SVG")));
      abort = finish(() => reject(signal.reason));
      signal?.addEventListener("abort", abort, { once: true });
      if (signal?.aborted) abort();
      else image.src = url;
    });
    signal?.throwIfAborted();
    return await globalThis.createImageBitmap(image);
  } finally {
    signal?.removeEventListener("abort", abort);
    if (image) {
      image.onload = image.onerror = null;
      image.src = "";
    }
    globalThis.URL.revokeObjectURL(url);
  }
}

export async function decodeRaster(blob, context = {}) {
  if (typeof globalThis.createImageBitmap !== "function") {
    throw assetFailure(
      "IMAGE_DECODER_UNAVAILABLE",
      "createImageBitmap is unavailable; supply a host decoder",
    );
  }
  const signal = context.signal;
  signal?.throwIfAborted();
  // EXIF orientation may swap JPEG dimensions. The manager validates actual
  // dimensions, including this allowed transposition, before layout delivery.
  try {
    let bitmap;
    try { bitmap = await globalThis.createImageBitmap(blob); }
    catch (cause) {
      signal?.throwIfAborted();
      if (blob.type !== "image/svg+xml") throw cause;
      bitmap = await svgBitmap(blob, signal);
    }
    if (signal?.aborted) {
      bitmap.close();
      signal.throwIfAborted();
    }
    return bitmap;
  } catch (cause) {
    signal?.throwIfAborted();
    if (cause instanceof FlowAssetError) throw cause;
    throw new FlowAssetError(
      "INVALID_IMAGE", "the browser could not decode the admitted image", { cause },
    );
  }
}
