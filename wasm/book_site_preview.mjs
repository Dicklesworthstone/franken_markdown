// Read the Rust book engine's classic ZIP subset, not arbitrary user archives.
// Markdown parsing, chapter addressing, transclusion and HTML remain Rust-owned.
export const BOOK_PREVIEW_LIMITS = Object.freeze({
  archiveBytes: 64 * 1024 * 1024,
  pageBytes: 8 * 1024 * 1024,
  totalBytes: 32 * 1024 * 1024,
  messageBytes: 64 * 1024 * 1024,
  chapters: 128,
});
const L = BOOK_PREVIEW_LIMITS;
// Generated host pages are not chapters and must never become iframe targets.
// Older engine archives omit the optional offline search page.
const auxiliaryNames = new Set(["index.html", "search-index.json", "~fmd-search.html"]);
const error = (code, message) => Object.assign(new Error(message), { code });
const invalid = () =>
  error(
    "INVALID_BOOK_PREVIEW",
    "The renderer returned an invalid or unsupported book-site archive.",
  );
const limit = () =>
  error(
    "PREVIEW_LIMIT",
    "This book exceeds the preview budget. Export its site, PDF or EPUB instead.",
  );
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const encoder = new TextEncoder();
const crcTable = Uint32Array.from({ length: 256 }, (_, byte) => {
  for (let i = 0; i < 8; i++) byte = (byte >>> 1) ^ (byte & 1 ? 0xedb88320 : 0);
  return byte >>> 0;
});
function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 255];
  return (crc ^ 0xffffffff) >>> 0;
}
function text(value, maximum) {
  if (typeof value !== "string") throw invalid();
  if (value.length > maximum) throw limit();
  let bytes = 0;
  for (const character of value) {
    const point = character.codePointAt(0);
    if (point >= 0xd800 && point <= 0xdfff) throw invalid();
    bytes += point < 128 ? 1 : point < 2048 ? 2 : point < 65536 ? 3 : 4;
    if (bytes > maximum) throw limit();
  }
  return bytes;
}
function ownedBytes(value, maximum) {
  if (
    !(value instanceof Uint8Array) ||
    Object.prototype.toString.call(value.buffer) !== "[object ArrayBuffer]"
  )
    throw invalid();
  if (!value.byteLength || value.byteLength > maximum) throw limit();
  return value;
}
function pageName(value) {
  text(value, 255);
  // Rust emits flat, portable output names; source-directory resolution is not
  // guessed here. Only names attested by the generated search index can open.
  if (
    !value ||
    /[\\/\x00-\x20\x7f<>:"|?*#%]/.test(value) ||
    !value.endsWith(".html") ||
    auxiliaryNames.has(value) ||
    value.startsWith(".")
  )
    throw invalid();
  return value;
}
function decode(bytes) {
  try {
    return decoder.decode(bytes);
  } catch {
    throw invalid();
  }
}

/** Validate every header and every declared allocation before decompressing.
 * The engine emits UTF-8 names, no extras/comments/descriptors, methods 0/8.
 * ZIP64, encryption, split archives and ambiguous/overlapping layouts fail shut.
 */
function directory(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const u16 = (at) => view.getUint16(at, true),
    u32 = (at) => view.getUint32(at, true);
  const end = bytes.length - 22;
  if (end < 0 || u32(end) !== 0x06054b50 || u16(end + 4) || u16(end + 6) || u16(end + 20))
    throw invalid();
  const count = u16(end + 10),
    start = u32(end + 16),
    size = u32(end + 12);
  if (count < 3 || count > L.chapters + auxiliaryNames.size) throw limit();
  if (u16(end + 8) !== count || start + size !== end) throw invalid();
  const entries = [],
    names = new Set();
  let at = start,
    localEnd = 0,
    total = 0;
  for (let i = 0; i < count; i++) {
    if (at + 46 > end || u32(at) !== 0x02014b50) throw invalid();
    const method = u16(at + 10),
      crc = u32(at + 16),
      compressed = u32(at + 20),
      length = u32(at + 24);
    const nameLength = u16(at + 28),
      local = u32(at + 42);
    if (
      u16(at + 6) > 20 ||
      u16(at + 8) !== 0x800 ||
      ![0, 8].includes(method) ||
      u16(at + 30) ||
      u16(at + 32) ||
      u16(at + 34) ||
      !nameLength ||
      nameLength > 255 ||
      at + 46 + nameLength > end ||
      local !== localEnd ||
      local + 30 + nameLength > start
    )
      throw invalid();
    const name = decode(bytes.subarray(at + 46, at + 46 + nameLength));
    if (!auxiliaryNames.has(name)) pageName(name);
    const folded = name.toLowerCase();
    if (names.has(folded)) throw invalid();
    names.add(folded);
    if (length > L.pageBytes || (total += length) > L.totalBytes) throw limit();
    if (
      u32(local) !== 0x04034b50 ||
      u16(local + 4) !== u16(at + 6) ||
      u16(local + 6) !== 0x800 ||
      u16(local + 8) !== method ||
      u32(local + 14) !== crc ||
      u32(local + 18) !== compressed ||
      u32(local + 22) !== length ||
      u16(local + 26) !== nameLength ||
      u16(local + 28) ||
      decode(bytes.subarray(local + 30, local + 30 + nameLength)) !== name
    )
      throw invalid();
    const offset = local + 30 + nameLength;
    localEnd = offset + compressed;
    if (localEnd > start || (method === 0 && compressed !== length)) throw invalid();
    entries.push({ name, method, crc, offset, compressed, length });
    at += 46 + nameLength;
  }
  if (
    at !== end ||
    localEnd !== start ||
    !names.has("index.html") ||
    !names.has("search-index.json")
  )
    throw invalid();
  return entries;
}

async function inflate(bytes, expected) {
  let stream;
  try {
    stream = new DecompressionStream("deflate-raw");
  } catch {
    throw error(
      "PREVIEW_UNAVAILABLE",
      "This browser cannot decompress book previews. Download a site export instead.",
    );
  }
  const reader = new Blob([bytes]).stream().pipeThrough(stream).getReader();
  // Allocate exactly the previously admitted size, never concatenate unbounded
  // chunks or trust a compressed stream's own expansion ratio.
  const output = new Uint8Array(expected);
  let written = 0,
    complete = false;
  try {
    for (;;) {
      const part = await reader.read();
      if (part.done) break;
      if (written + part.value.byteLength > expected) throw invalid();
      output.set(part.value, written);
      written += part.value.byteLength;
    }
    if (written !== expected) throw invalid();
    complete = true;
    return output;
  } catch {
    throw invalid();
  } finally {
    if (!complete) {
      try {
        await reader.cancel();
      } catch {
        /* Preserve the admission/decompression error. */
      }
    }
    reader.releaseLock();
  }
}

/** Called in the disposable book worker. No file, URL or network resolution. */
export async function decodeBookSiteArchive(value) {
  // Snapshot before the first await; callers cannot mutate a validated header
  // while another entry is decompressing.
  const bytes = new Uint8Array(ownedBytes(value, L.archiveBytes)),
    entries = directory(bytes),
    contents = new Map();
  for (const entry of entries) {
    const payload = bytes.subarray(entry.offset, entry.offset + entry.compressed);
    const body = entry.method === 0 ? payload : await inflate(payload, entry.length);
    if (crc32(body) !== entry.crc) throw invalid();
    contents.set(entry.name, decode(body));
  }
  let index;
  try {
    index = JSON.parse(contents.get("search-index.json"));
  } catch {
    throw invalid();
  }
  const chapterEntries = entries.filter((entry) => !auxiliaryNames.has(entry.name));
  if (
    !index ||
    index.schema !== "fmd-book-search-index-v1" ||
    !Array.isArray(index.chapters) ||
    index.chapters.length !== chapterEntries.length
  )
    throw invalid();
  const pages = index.chapters.map((chapter) => ({
    path: chapter?.page,
    source: chapter?.source,
    title: chapter?.title,
    html: contents.get(chapter?.page),
  }));
  return checkedPreview({ schema: "fmd-book-preview-v1", pages });
}
function checkedPreview(value) {
  if (
    !value ||
    value.schema !== "fmd-book-preview-v1" ||
    !Array.isArray(value.pages) ||
    !value.pages.length ||
    value.pages.length > L.chapters
  )
    throw invalid();
  const paths = new Set(),
    sources = new Set();
  let total = 0;
  const pages = Array.from(value.pages, (page) => {
    if (!page || typeof page !== "object") throw invalid();
    const path = pageName(page.path);
    text(page.source, 4096);
    text(page.title, 4096);
    if (!page.source || paths.has(path.toLowerCase()) || sources.has(page.source)) throw invalid();
    paths.add(path.toLowerCase());
    sources.add(page.source);
    total += text(page.html, L.pageBytes);
    if (total > L.totalBytes) throw limit();
    return Object.freeze({ path, source: page.source, title: page.title, html: page.html });
  });
  return Object.freeze({ schema: "fmd-book-preview-v1", pages: Object.freeze(pages) });
}

/** Existing worker envelopes transport owned bytes. This payload contains only
 * chapter metadata and original generated HTML; not the landing redirect or a
 * second JS renderer. Export PDFs/EPUBs/site ZIPs remain unchanged.
 */
export async function renderBookPreview(engine, files, options) {
  const site = await engine.renderBookSite(files, options);
  const preview = await decodeBookSiteArchive(site.bytes);
  const parts = ['{"schema":"fmd-book-preview-v1","pages":['];
  let total = parts[0].length;
  for (const [i, page] of preview.pages.entries()) {
    const part = (i ? "," : "") + JSON.stringify(page);
    total += text(part, L.messageBytes - total - 2);
    parts.push(part);
  }
  parts.push("]}");
  return { bytes: encoder.encode(parts.join("")), sourceLength: site.sourceLength };
}
export function parseBookPreview(value) {
  const bytes = ownedBytes(value, L.messageBytes);
  let parsed;
  try {
    parsed = JSON.parse(decode(bytes));
  } catch {
    throw invalid();
  }
  return checkedPreview(parsed);
}

/** Resolve only exact output names supplied by Rust. Never follow external,
 * data, script, parent-directory, query-string or escaped-path destinations.
 */
export function resolveBookPreviewLink(preview, current, href) {
  if (typeof href !== "string" || href.length > 8192 || /[\x00-\x20\x7f\\]/.test(href)) return null;
  const split = href.indexOf("#"),
    rawPath = split < 0 ? href : href.slice(0, split);
  const path = rawPath.startsWith("./") ? rawPath.slice(2) : rawPath;
  if (/[/%?:]/.test(path) || path === "." || path === "..") return null;
  const page =
    path === "index.html"
      ? preview.pages[0]
      : preview.pages.find((item) => item.path === (path || current));
  if (!page) return null;
  let fragment;
  try {
    fragment = split < 0 ? "" : decodeURIComponent(href.slice(split + 1));
  } catch {
    return null;
  }
  if (fragment.length > 4096 || /[\x00-\x1f\x7f]/.test(fragment)) return null;
  return { path: page.path, fragment };
}
