// Shared deterministic ZIP packaging for already admitted book payloads.
// STORE preserves exact bytes; no parser, renderer, compression or I/O is used.
const CHUNK = 256 * 1024;
const blobSize = Object.getOwnPropertyDescriptor(Blob.prototype, "size").get;
const CRC_TABLE = Uint32Array.from({ length: 256 }, (_, value) => {
  for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
  return value >>> 0;
});
const fail = (code, message) => Object.assign(new Error(message), { code });

/** Internal callers own their byte arrays or immutable Blobs before entry.
 * CRC reads at most 256 KiB of a Blob at once and yields between chunks.
 * wait() owns cancellation while a read is pending; check() fences every turn.
 */
export async function bookZipCrc32(payload, { check, wait }) {
  const bytes = payload instanceof Uint8Array ? payload : null;
  const size = bytes ? bytes.byteLength : blobSize.call(payload);
  let crc = 0xffffffff;
  check();
  for (let offset = 0; offset < size; offset += CHUNK) {
    check();
    const end = Math.min(offset + CHUNK, size);
    const part = bytes ? bytes.subarray(offset, end)
      : new Uint8Array(await wait(Blob.prototype.arrayBuffer.call(Blob.prototype.slice.call(payload, offset, end))));
    check();
    for (let i = 0; i < part.length; i++) crc = CRC_TABLE[(crc ^ part[i]) & 255] ^ (crc >>> 8);
    if (end < size) await wait(new Promise(resolve => setTimeout(resolve, 0)));
  }
  check();
  return (crc ^ 0xffffffff) >>> 0;
}

/** UTF-8 archive names must stay literal and portable on extraction. Neither
 * normalization nor lossy replacement of malformed Unicode changes filenames.
 */
export function bookZipPath(name) {
  if (typeof name !== "string" || !name || name.length > 65535
      || /[\\\x00-\x1f\x7f<>:"|?*#%]/.test(name)
      || name.split("/").some(part => !part || part === "." || part === ".." || /[. ]$/.test(part)
        || /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part)))
    throw fail("INVALID_PATH", "Use portable relative source paths without reserved names or traversal.");
  for (let i = 0; i < name.length; i++) {
    const value = name.charCodeAt(i);
    if (value >= 0xd800 && value <= 0xdbff) {
      const next = name.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw fail("INVALID_UNICODE", "Repair incomplete Unicode in source paths.");
    } else if (value >= 0xdc00 && value <= 0xdfff)
      throw fail("INVALID_UNICODE", "Repair incomplete Unicode in source paths.");
  }
  const bytes = new TextEncoder().encode(name);
  if (bytes.length > 65535) throw fail("INVALID_PATH", "The UTF-8 source path is too long for a ZIP entry.");
  return bytes;
}

/** Admit the complete path set before reading payloads or computing checksums. */
export function bookZipPaths(names) {
  const paths = new Set();
  const encoded = names.map(name => {
    const bytes = bookZipPath(name), key = name.normalize("NFC").toLowerCase();
    if (paths.has(key)) throw fail("PATH_COLLISION", "Case or Unicode-equivalent source paths cannot share a ZIP archive.");
    paths.add(key);
    return bytes;
  });
  for (const path of paths) {
    let at = path.indexOf("/");
    while (at >= 0) {
      if (paths.has(path.slice(0, at))) throw fail("PATH_COLLISION", "A source file cannot also be another source's directory.");
      at = path.indexOf("/", at + 1);
    }
  }
  return encoded;
}

function zipRecord(name, size, crc, offset) {
  const central = offset !== undefined, base = central ? 46 : 30;
  const bytes = new Uint8Array(base + name.length), view = new DataView(bytes.buffer);
  const u16 = (at, value) => view.setUint16(at, value, true);
  const u32 = (at, value) => view.setUint32(at, value, true);
  u32(0, central ? 0x02014b50 : 0x04034b50);
  if (central) u16(4, 20);
  const start = central ? 6 : 4;
  u16(start, 20); u16(start + 2, 0x800); // UTF-8; method/time stay STORE/zero.
  u16(start + 8, 33); // 1980-01-01, independent of clocks and local timezone.
  u32(start + 10, crc); u32(start + 14, size); u32(start + 18, size);
  u16(start + 22, name.length);
  if (central) u32(42, offset);
  bytes.set(name, base);
  return bytes;
}

/** Assemble owned {name, blob, size, crc} entries. Callers have separate input
 * budgets; this final bounded assembly charges all UTF-8 names and ZIP records.
 * Immutable Blob parts avoid allocating another concatenated payload buffer.
 */
export function createBookZip(entries, maxBytes) {
  if (!Array.isArray(entries) || entries.length < 1 || entries.length > 65535
      || !Number.isSafeInteger(maxBytes) || maxBytes < 22 || maxBytes > 0xffffffff)
    throw fail("INVALID_OPTIONS", "Use a bounded nonempty ZIP entry list.");
  const names = bookZipPaths(entries.map(entry => entry.name));
  let total = 22;
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    if (!Number.isSafeInteger(entry.size) || entry.size < 0 || entry.size > 0xffffffff
        || blobSize.call(entry.blob) !== entry.size
        || !Number.isInteger(entry.crc) || entry.crc < 0 || entry.crc > 0xffffffff)
      throw fail("INVALID_ZIP_ENTRY", "ZIP payload size or checksum is invalid.");
    total += entry.size + 76 + 2 * names[i].length;
    if (total > maxBytes) throw fail("OUTPUT_LIMIT", "ZIP archive exceeds its payload and metadata budget.");
  }
  const parts = [], directory = [];
  let offset = 0, directorySize = 0;
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i], name = names[i];
    const local = zipRecord(name, entry.size, entry.crc);
    const central = zipRecord(name, entry.size, entry.crc, offset);
    parts.push(local, entry.blob); directory.push(central);
    offset += local.length + entry.size; directorySize += central.length;
  }
  const end = new Uint8Array(22), view = new DataView(end.buffer);
  view.setUint32(0, 0x06054b50, true);
  view.setUint16(8, entries.length, true); view.setUint16(10, entries.length, true);
  view.setUint32(12, directorySize, true); view.setUint32(16, offset, true);
  return new Blob([...parts, ...directory, end], { type: "application/zip" });
}
