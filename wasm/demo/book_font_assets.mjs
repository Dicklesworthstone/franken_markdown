// Explicit, in-memory font authorization for the book workbench. Font parsing,
// instancing and subsetting belong to Rust; no browser FontFace/network loads.
const MIB = 1024 * 1024;
export const BOOK_FONT_LIMITS = Object.freeze({ faceBytes: 8 * MIB, totalBytes: 32 * MIB,
  collectionBytes: 32 * MIB, selectedFileBytes: 64 * MIB, nameUnits: 512 });
export const BOOK_FONT_SLOTS = Object.freeze([
  "body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular",
]);
const error = (code, message) => Object.assign(new Error(message), { code });
function slotName(slot) {
  if (!BOOK_FONT_SLOTS.includes(slot)) throw error("INVALID_FONT_SLOT", "Choose a supported font role.");
  return slot;
}
function weightValue(weight) {
  if (weight !== undefined && (!Number.isInteger(weight) || weight < 1 || weight > 1000)) {
    throw error("INVALID_FONT_WEIGHT", "Font weight must be blank or an integer from 1 through 1000.");
  }
  return weight;
}
function fontName(name) {
  if (typeof name !== "string" || !name || name.length > BOOK_FONT_LIMITS.nameUnits
      || /[\u0000-\u001f\u007f-\u009f]/u.test(name)) {
    throw error("INVALID_FONT", "Font names must be nonempty text of at most 512 characters without controls.");
  }
  for (let i = 0; i < name.length; i++) {
    const c = name.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = name.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw error("INVALID_FONT", "Font name is not valid Unicode.");
    } else if (c >= 0xdc00 && c <= 0xdfff) throw error("INVALID_FONT", "Font name is not valid Unicode.");
  }
  return name;
}
function asset(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw error("INVALID_FONT", "A font needs a role, filename and owned byte array.");
  }
  // Capture metadata once. Only these private primitive values are retained.
  const { slot, name, bytes, weight } = value;
  slotName(slot); fontName(name); weightValue(weight);
  if (!(bytes instanceof Uint8Array) || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
      || !bytes.byteLength || bytes.byteLength > BOOK_FONT_LIMITS.faceBytes) {
    throw error("FONT_LIMIT", "Each font must contain 1 byte through 8 MiB in an owned Uint8Array.");
  }
  return { slot, name, bytes, weight };
}

/** Five explicit slots. Replacing a batch is atomic and releases superseded
 * payloads. Every snapshot owns its bytes; metadata reads never expose them. */
export function createBookFontStore() {
  let fonts = new Map(), disposed = false;
  const alive = () => { if (disposed) throw error("SESSION_DISPOSED", "Book fonts are disposed."); };
  const ordered = () => BOOK_FONT_SLOTS.filter(slot => fonts.has(slot)).map(slot => fonts.get(slot));
  return Object.freeze({
    list() {
      alive();
      return ordered().map(({ slot, name, weight, bytes }) => ({ slot, name, weight, size: bytes.byteLength }));
    },
    set(values, beforeInstall = () => {}) {
      alive();
      const count = Array.isArray(values) ? values.length : -1;
      if (count < 1 || count > BOOK_FONT_SLOTS.length) throw error("FONT_LIMIT", "Assign one through five font roles at once.");
      const next = new Map(fonts), added = [], seen = new Set();
      for (let i = 0; i < count; i++) {
        const candidate = asset(values[i]);
        if (seen.has(candidate.slot)) throw error("INVALID_FONT_SLOT", "A font batch cannot assign the same role twice.");
        seen.add(candidate.slot); next.set(candidate.slot, candidate); added.push(candidate);
      }
      let total = 0;
      for (const font of next.values()) total += font.bytes.byteLength;
      if (total > BOOK_FONT_LIMITS.totalBytes) throw error("FONT_LIMIT", "Combined supplied font roles exceed 32 MiB.");
      // Validate the complete replacement before allocating or installing it.
      // Uint8Array construction copies bytes, not an overridable slice method.
      for (const candidate of added) next.set(candidate.slot, { ...candidate, bytes: new Uint8Array(candidate.bytes) });
      alive();
      beforeInstall();
      fonts = next;
    },
    remove(slot) {
      alive(); slotName(slot);
      return fonts.delete(slot);
    },
    clear() {
      alive(); const changed = fonts.size !== 0;
      fonts = new Map(); return changed;
    },
    snapshot() {
      alive();
      return ordered().map(({ slot, bytes, weight }) => ({ slot, weight, bytes: new Uint8Array(bytes) }));
    },
    dispose() { if (!disposed) { fonts.clear(); disposed = true; } },
  });
}

/** Capture local file authority and primitive settings before asynchronous work.
 * A collection needs an explicit zero-based face; its tables are interpreted
 * only by Rust in the publishing worker, never by this file-reader module. */
export function captureBookFont(file, slot, weight, faceIndex) {
  slotName(slot); weightValue(weight);
  if (faceIndex !== undefined && (!Number.isInteger(faceIndex) || faceIndex < 0 || faceIndex >= 1024))
    throw error("FONT_COLLECTION_FACE_INDEX", "Collection face must be an integer from 0 through 1023, or blank for a standalone font.");
  if (!file || Object.prototype.toString.call(file) !== "[object File]") {
    throw error("INVALID_FONT", "Choose a local .ttf, .otf, .ttc or .otc font file.");
  }
  const name = fontName(file.name), size = file.size, read = file.arrayBuffer;
  if (!/\.(?:ttf|otf|ttc|otc)$/i.test(name) || typeof read !== "function") {
    throw error("INVALID_FONT", "Choose a local .ttf, .otf, .ttc or .otc file; WOFF and remote fonts are not supported here.");
  }
  if (/\.(?:ttc|otc)$/i.test(name) && faceIndex === undefined)
    throw error("FONT_COLLECTION_FACE_INDEX", "Choose a zero-based face index for this collection; 0 selects its first face.");
  const maximum = faceIndex === undefined ? BOOK_FONT_LIMITS.faceBytes : BOOK_FONT_LIMITS.collectionBytes;
  if (!Number.isSafeInteger(size) || size < 1 || size > maximum) {
    throw error("FONT_LIMIT", faceIndex === undefined ? "A selected font must contain 1 byte through 8 MiB."
      : "A selected font collection must contain 1 byte through 32 MiB.");
  }
  return Object.freeze({ file, read, name, size, slot, weight,
    ...(faceIndex === undefined ? {} : { faceIndex: faceIndex === 0 ? 0 : faceIndex }) });
}

/** Read a previously admitted selection without consulting mutable host metadata
 * again. Collection bytes still require worker extraction before installation. */
export async function readCapturedBookFont(selection) {
  const { file, read, name, size, slot, weight, faceIndex } = selection;
  let buffer;
  try { buffer = await read.call(file); }
  catch { throw error("FONT_READ_FAILED", "The font file could not be read; the previous assignment was kept."); }
  if (Object.prototype.toString.call(buffer) !== "[object ArrayBuffer]" || buffer.byteLength !== size) {
    throw error("FONT_READ_FAILED", "The font file changed size while reading; the previous assignment was kept.");
  }
  const bytes = new Uint8Array(buffer);
  if (faceIndex === undefined && bytes[0] === 0x74 && bytes[1] === 0x74 && bytes[2] === 0x63 && bytes[3] === 0x66)
    throw error("FONT_COLLECTION_FACE_INDEX", "This file contains a font collection; choose a zero-based face index before assigning it.");
  return { slot, name, weight, bytes, ...(faceIndex === undefined ? {} : { faceIndex }) };
}

/** Read only an explicitly selected font file. This admits bytes, not font
 * validity or embedding rights. Collections are returned with faceIndex for
 * extraction by the publishing worker; standalone bytes preserve their shape. */
export async function readBookFont(file, slot, weight, faceIndex) {
  return readCapturedBookFont(captureBookFont(file, slot, weight, faceIndex));
}
