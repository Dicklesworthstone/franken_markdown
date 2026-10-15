// Workbench ownership of one captured publication. The injected publisher owns
// native rendering; this session owns only cancellation, receipts and Blob URLs.
const EDITION_LIMIT = 128 * 1024 * 1024;
const FORMATS = Object.freeze({
  pdf: Object.freeze({ mimeType: "application/pdf", filename: "book.pdf" }),
  epub: Object.freeze({ mimeType: "application/epub+zip", filename: "book.epub" }),
  site: Object.freeze({ mimeType: "application/zip", filename: "book-site.zip" }),
});
const fail = (code, message) => Object.assign(new Error(message), { code });
function selectedFormats(value) {
  if (!Array.isArray(value) || value.length > 3)
    throw fail("INVALID_FORMAT", "Select PDF, EPUB and/or HTML site.");
  const result = [];
  for (let i = 0; i < value.length; i++) {
    const field = Object.getOwnPropertyDescriptor(value, String(i));
    if (!field || !Object.hasOwn(field, "value") || typeof field.value !== "string"
        || !Object.hasOwn(FORMATS, field.value) || result.includes(field.value))
      throw fail("INVALID_FORMAT", "Select each supported publication format at most once.");
    result.push(field.value);
  }
  return Object.freeze(result);
}
function waitFor(pending, signal) {
  return new Promise((resolve, reject) => {
    const cleanup = () => signal.removeEventListener("abort", abort);
    const abort = () => { cleanup(); reject(signal.reason); };
    signal.addEventListener("abort", abort, { once: true });
    // Also observe late failures from a cancelled/uncooperative endpoint.
    Promise.resolve(pending).then(value => { cleanup(); resolve(value); },
      cause => { cleanup(); reject(cause); });
    if (signal.aborted) abort();
  });
}

/** capture() synchronously flushes the source host and returns collection.snapshot().
 * checkpoint() covers raw editor input as well as collection/resource revision.
 * changed() tolerates notifications produced by that initial synchronous capture;
 * invalidate() always retires work (including uncommitted input and composition).
 * There is no renderer, persistence, permission request or automatic download here.
 */
export function createBookEditionSession({
  publisher, capture, checkpoint, isBlocked = () => false, urls = URL, onState = () => {},
  crypto = globalThis.crypto,
} = {}) {
  if (!publisher || ["render", "cancel", "dispose"].some(key => typeof publisher[key] !== "function")
      || [capture, checkpoint, isBlocked, onState].some(fn => typeof fn !== "function")
      || !urls || ["createObjectURL", "revokeObjectURL"].some(key => typeof urls[key] !== "function"))
    throw fail("INVALID_OPTIONS", "Edition sessions require a publisher, source host and URL owner.");
  let disposed = false, suspended = false, capturing = false;
  let active = null, ready = null, packing = null, formats = selectedFormats(["pdf", "epub", "site"]);
  let phase = "idle", errorCode = null;
  let message = "Select formats and prepare one edition. No files are saved automatically.";
  const blocked = () => {
    try { return !!isBlocked(); } catch { return true; }
  };
  function state() {
    return Object.freeze({
      phase, busy: active !== null || packing !== null, bundleBusy: packing !== null,
      canPrepare: !disposed && !suspended && !active && !packing && formats.length > 0 && !blocked(),
      canBundle: !!ready && !active && !packing && !ready.bundleByteLength && current(ready),
      bundleByteLength: ready?.bundleByteLength ?? 0,
      formats,
      downloads: Object.freeze(ready ? ready.downloads.slice() : []),
      byteLength: ready?.byteLength ?? 0, sourceLength: ready?.sourceLength ?? 0,
      errorCode, message,
    });
  }
  function announce() {
    try { onState(state()); } catch { /* View observers cannot own a publication outcome. */ }
  }
  function release(record) {
    if (!record) return;
    const owned = record.urls.splice(0);
    record.downloads = [];
    record.blobs = [];
    for (const url of owned) {
      try { urls.revokeObjectURL(url); } catch { /* Retire all URLs even if one adapter fails. */ }
    }
  }
  function retire(code, text, nextPhase = "idle") {
    const job = active, previous = ready, bundle = packing;
    active = ready = packing = null;
    phase = nextPhase; message = text; errorCode = null;
    release(previous);
    release(job);
    release(bundle);
    bundle?.abort.abort(fail(code, text));
    if (job) {
      job.abort.abort(fail(code, text));
      try { publisher.cancel(); } catch { /* Source and URLs are still retired. */ }
    }
    announce();
  }
  function alive() {
    if (disposed) throw fail("SESSION_DISPOSED", "Edition session is disposed.");
    if (suspended) throw fail("WORKBENCH_SUSPENDED", "Resume the workbench before preparing an edition.");
  }
  function owned(job) {
    alive();
    if (active !== job || job.abort.signal.aborted)
      throw job.abort.signal.reason ?? fail("STALE_SOURCE", "This edition was retired.");
    if (blocked()) throw fail("BOOK_BUSY", "Finish importing or composing text before publishing.");
  }
  function fence(job) {
    owned(job);
    if (checkpoint() !== job.checkpoint)
      throw fail("STALE_SOURCE", "The book or editor changed; prepare a new edition.");
  }
  function current(record) {
    if (disposed || suspended || blocked()) return false;
    try { return checkpoint() === record.checkpoint; } catch { return false; }
  }
  async function prepare() {
    alive();
    if (active || packing) throw fail("BOOK_BUSY", "An edition or its delivery bundle is already being prepared.");
    if (blocked()) throw fail("BOOK_BUSY", "Finish importing or composing text before publishing.");
    if (!formats.length) throw fail("INVALID_FORMAT", "Select at least one publication format.");
    release(ready); ready = null;
    const job = { abort: new AbortController(), urls: [], downloads: [], checkpoint: null };
    const selected = formats;
    active = job; phase = "busy"; errorCode = null;
    message = `Preparing ${selected.map(format => format.toUpperCase()).join(" + ")} from one captured book. No downloads are ready yet.`;
    announce();
    try {
      owned(job);
      let input;
      capturing = true;
      try { input = capture(); } finally { capturing = false; }
      owned(job);
      job.checkpoint = checkpoint();
      const pending = publisher.render(input.files, selected, input.options, { signal: job.abort.signal });
      input = null; // Do not retain an extra resource snapshot while awaiting the worker.
      const result = await waitFor(pending, job.abort.signal);
      fence(job);
      if (result?.format !== "book-publication" || !Array.isArray(result.outputs)
          || result.outputs.length !== selected.length || !Number.isSafeInteger(result.byteLength)
          || result.byteLength < 1 || result.byteLength > EDITION_LIMIT
          || !Number.isSafeInteger(result.sourceLength) || result.sourceLength < 0
          || result.sourceLength > 64 * 1024 * 1024)
        throw fail("WORKER_PROTOCOL_ERROR", "The publisher did not return a complete captured edition.");
      // Admit every receipt/Blob before creating even the first download URL.
      let total = 0;
      const blobs = result.outputs.map((output, index) => {
        const format = selected[index], expected = FORMATS[format];
        if (output?.format !== `book-${format}` || output.sourceLength !== result.sourceLength
            || output.mimeType !== expected.mimeType || typeof output.blob !== "function")
          throw fail("WORKER_PROTOCOL_ERROR", "An edition output does not match its requested format.");
        const blob = output.blob();
        if (!(blob instanceof Blob) || blob.size < 1 || blob.type !== expected.mimeType)
          throw fail("WORKER_PROTOCOL_ERROR", "An edition output is not a valid publication Blob.");
        total += blob.size;
        if (total > EDITION_LIMIT)
          throw fail("OUTPUT_LIMIT", "Combined edition output exceeds 128 MiB.");
        return { format, blob, ...expected };
      });
      if (total !== result.byteLength)
        throw fail("WORKER_PROTOCOL_ERROR", "Edition byte totals do not match the captured outputs.");
      fence(job);
      for (const { format, blob, mimeType, filename } of blobs) {
        const href = urls.createObjectURL(blob);
        job.urls.push(href);
        fence(job);
        job.downloads.push(Object.freeze({ format, filename, href, mimeType, byteLength: blob.size }));
      }
      fence(job);
      // Keep the same immutable Blobs used by the individual downloads. Bundle
      // preparation neither rerenders nor asks output.blob() for different bytes.
      job.blobs = blobs.map(({ format, blob }) => ({ format, blob }));
      job.byteLength = total; job.sourceLength = result.sourceLength;
      ready = job; active = null; phase = "ready";
      message = `Edition ready: ${selected.length} files, ${total} bytes. Use each download link to save it; preparation alone saves nothing.`;
      announce();
      return state();
    } catch (cause) {
      release(job);
      if (active === job) {
        active = null; phase = "error";
        errorCode = typeof cause?.code === "string" ? cause.code : "EDITION_FAILED";
        message = `${errorCode}: ${cause?.message ?? "Edition preparation failed."} No edition downloads were published. Source remains editable.`;
        job.abort.abort(cause);
        try { publisher.cancel(); } catch { /* Preserve the original failure. */ }
        announce();
      }
      throw cause;
    }
  }
  async function prepareBundle() {
    alive();
    if (active || packing) throw fail("BOOK_BUSY", "An edition or its delivery bundle is already being prepared.");
    if (!ready) throw fail("NO_EDITION", "Prepare an edition before packaging its delivery ZIP.");
    if (!current(ready)) {
      retire("STALE_SOURCE", "The edition changed; prepare current publications before packaging.");
      throw fail("STALE_SOURCE", "The edition changed; no delivery ZIP was prepared.");
    }
    if (ready.bundleByteLength) return state();
    const publication = ready, job = { abort: new AbortController(), urls: [], downloads: [] };
    packing = job; phase = "bundling"; errorCode = null;
    message = "Packaging the exact edition files and SHA-256 manifest. Individual downloads remain available.";
    const check = () => {
      alive();
      if (packing !== job || ready !== publication || job.abort.signal.aborted)
        throw job.abort.signal.reason ?? fail("STALE_SOURCE", "This edition bundle was retired.");
      if (!current(publication)) throw fail("STALE_SOURCE", "The book or editor changed during packaging.");
    };
    announce();
    try {
      check();
      const result = await waitFor(createBookEditionBundle(publication.blobs, {
        signal: job.abort.signal, crypto, sourceLength: publication.sourceLength,
      }), job.abort.signal);
      check();
      const href = urls.createObjectURL(result.blob);
      job.urls.push(href);
      check();
      const download = Object.freeze({ format: "bundle", filename: result.filename, href,
        mimeType: result.mimeType, byteLength: result.byteLength });
      // Transfer URL ownership only after the final source/lifecycle fence.
      publication.urls.push(href); job.urls = [];
      publication.downloads.push(download); publication.bundleByteLength = result.byteLength;
      packing = null; phase = "ready";
      message = `Delivery ZIP ready: ${result.byteLength} bytes, containing the exact edition files and SHA-256 manifest. Click its download link to save it. Checksums are not signatures.`;
      announce();
      return state();
    } catch (cause) {
      release(job);
      if (packing === job) {
        packing = null;
        job.abort.abort(cause);
        if (!current(publication)) {
          retire("STALE_SOURCE", "The edition changed during packaging; all stale downloads were retired.");
        } else {
          // Crypto/packaging/URL failure must not destroy a valid rendered edition.
          phase = "ready";
          errorCode = typeof cause?.code === "string" ? cause.code : "BUNDLE_FAILED";
          message = `${errorCode}: ${cause?.message ?? "Edition packaging failed."} Individual edition downloads remain available; packaging can be retried.`;
          announce();
        }
      }
      throw cause;
    }
  }
  return Object.freeze({
    get state() { return state(); },
    prepare,
    prepareBundle,
    setFormats(value) {
      alive();
      const next = selectedFormats(value);
      if (next.length === formats.length && next.every((format, index) => format === formats[index])) return;
      formats = next;
      retire("EXPORT_CANCELLED", "Publication selection changed. Prepare a new edition.");
    },
    changed() {
      if (disposed || suspended || capturing) return;
      if (active || ready) retire("STALE_SOURCE", "The book changed. Prepare a new edition from the current sources, settings and resources.");
      else announce();
    },
    invalidate() {
      if (disposed || suspended) return;
      if (active || ready) retire("STALE_SOURCE", "The editor changed or is busy. Prepare a new edition when editing is complete.");
      else announce();
    },
    /** Used immediately before both normal and auxiliary-link activation.
     * An old detached link cannot revoke an unrelated newer edition. */
    authorizeDownload(href) {
      if (!ready || !ready.downloads.some(item => item.href === href)) return false;
      if (current(ready)) return true;
      retire("STALE_SOURCE", "Edition downloads are stale. Prepare them again from the current book.");
      return false;
    },
    cancel() {
      if (!disposed && !suspended)
        retire("EXPORT_CANCELLED", "Edition cancelled or discarded. Source and other exports are unchanged.");
    },
    suspend() {
      if (disposed || suspended) return;
      suspended = true;
      retire("WORKBENCH_SUSPENDED", "Edition downloads were discarded while the workbench was suspended.", "suspended");
    },
    resume() {
      if (disposed || !suspended) return;
      suspended = false; phase = "idle";
      message = "Workbench resumed. Explicitly prepare a new edition when ready.";
      announce();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      retire("SESSION_DISPOSED", "Edition session is disposed.", "disposed");
      try { publisher.dispose(); } finally { onState = () => {}; }
    },
  });
}

// Packaging, not rendering: store the native publication Blobs byte-for-byte.
// Fixed entry names/order and a fixed DOS date make the archive deterministic.
// No source project, resource grants, file handles or URLs enter the manifest.
export const BOOK_EDITION_BUNDLE_LIMIT = EDITION_LIMIT + 8192;
const BUNDLE_CHUNK = 256 * 1024;
const CRC_TABLE = Uint32Array.from({ length: 256 }, (_, value) => {
  for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
  return value >>> 0;
});
function bundleCheck(signal) {
  if (signal.aborted) throw fail("EXPORT_CANCELLED", "Edition packaging cancelled.");
}
function zipRecord(name, size, crc, offset) {
  const central = offset !== undefined, base = central ? 46 : 30;
  const bytes = new Uint8Array(base + name.length), view = new DataView(bytes.buffer);
  const u16 = (at, value) => view.setUint16(at, value, true);
  const u32 = (at, value) => view.setUint32(at, value, true);
  u32(0, central ? 0x02014b50 : 0x04034b50);
  if (central) u16(4, 20); // Made by ZIP 2.0, portable DOS attributes.
  const start = central ? 6 : 4;
  u16(start, 20); u16(start + 2, 0x800); // UTF-8; method/time stay STORE/zero.
  u16(start + 8, 33); // 1980-01-01, independent of clocks and local timezone.
  u32(start + 10, crc); u32(start + 14, size); u32(start + 18, size);
  u16(start + 22, name.length);
  if (central) u32(42, offset);
  for (let i = 0; i < name.length; i++) bytes[base + i] = name.charCodeAt(i);
  return bytes;
}

/** Build a delivery ZIP from 1..3 already rendered immutable Blobs. Accepts
 * [{format: "pdf"|"epub"|"site", blob}], never arbitrary archive paths.
 * The manifest records SHA-256 and byte lengths of the EXACT contained files;
 * checksums detect corruption, not authorship or authenticity. No native render,
 * network access, output parsing, compression, persistence or download occurs.
 * At most 128 MiB of publications plus 8 KiB of archive/manifest overhead.
 * One file is read for hashing at a time; CRC traversal yields every 256 KiB,
 * and cancellation settles even while an underlying digest is still running.
 */
export async function createBookEditionBundle(outputs, {
  signal = new AbortController().signal, crypto = globalThis.crypto, sourceLength = 0,
} = {}) {
  if (!signal || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function"
      || typeof signal.removeEventListener !== "function")
    throw fail("INVALID_OPTIONS", "Edition packaging requires an AbortSignal.");
  bundleCheck(signal);
  if (!Number.isSafeInteger(sourceLength) || sourceLength < 0 || sourceLength > 64 * 1024 * 1024)
    throw fail("INVALID_OPTIONS", "Invalid captured edition source length.");
  const count = Array.isArray(outputs) ? Object.getOwnPropertyDescriptor(outputs, "length")?.value : -1;
  if (!Number.isInteger(count) || count < 1 || count > 3)
    throw fail("INVALID_FORMAT", "Package one through three rendered edition formats.");
  const owned = new Map();
  let total = 0;
  for (let i = 0; i < count; i++) {
    const field = Object.getOwnPropertyDescriptor(outputs, String(i));
    const value = field && Object.hasOwn(field, "value") ? field.value : null;
    if (!value || typeof value !== "object" || Array.isArray(value)
        || ![Object.prototype, null].includes(Object.getPrototypeOf(value))
        || Reflect.ownKeys(value).some(key => key !== "format" && key !== "blob"))
      throw fail("INVALID_FORMAT", "Use plain format/Blob records without accessors.");
    const fields = Object.getOwnPropertyDescriptors(value);
    if (!fields.format || !Object.hasOwn(fields.format, "value")
        || !fields.blob || !Object.hasOwn(fields.blob, "value"))
      throw fail("INVALID_FORMAT", "Use plain format/Blob records without accessors.");
    const format = fields.format.value;
    if (typeof format !== "string" || !Object.hasOwn(FORMATS, format) || owned.has(format))
      throw fail("INVALID_FORMAT", "Package each supported edition format at most once.");
    let blob, size, type;
    try {
      // Intrinsics reject lookalikes and ignore subclass size/read/slice overrides.
      const input = fields.blob.value;
      size = Object.getOwnPropertyDescriptor(Blob.prototype, "size").get.call(input);
      type = Object.getOwnPropertyDescriptor(Blob.prototype, "type").get.call(input);
      blob = Blob.prototype.slice.call(input, 0, size, type);
    } catch { throw fail("INVALID_BLOB", "Edition outputs must be immutable publication Blobs."); }
    if (size < 1 || type !== FORMATS[format].mimeType)
      throw fail("INVALID_BLOB", "An edition Blob has an empty payload or mismatched media type.");
    total += size;
    if (total > EDITION_LIMIT) throw fail("OUTPUT_LIMIT", "Combined edition output exceeds 128 MiB.");
    owned.set(format, { format, blob, byteLength: size, ...FORMATS[format] });
  }
  // Complete admission precedes any hashing or payload read. The captured
  // data cannot be retargeted by edits to the caller's containers while awaiting.
  const subtle = crypto?.subtle;
  if (typeof subtle?.digest !== "function")
    throw fail("CRYPTO_UNAVAILABLE", "SHA-256 needs Web Crypto in a secure context; individual edition downloads remain available.");
  const digest = subtle.digest.bind(subtle);
  const wait = async pending => {
    try { const result = await waitFor(pending, signal); bundleCheck(signal); return result; }
    catch (cause) { bundleCheck(signal); throw cause; }
  };
  async function crc32(bytes) {
    let crc = 0xffffffff;
    for (let offset = 0; offset < bytes.length; offset += BUNDLE_CHUNK) {
      bundleCheck(signal);
      const end = Math.min(offset + BUNDLE_CHUNK, bytes.length);
      for (let i = offset; i < end; i++) crc = CRC_TABLE[(crc ^ bytes[i]) & 255] ^ (crc >>> 8);
      if (end < bytes.length) await wait(new Promise(resolve => setTimeout(resolve, 0)));
    }
    return (crc ^ 0xffffffff) >>> 0;
  }
  const entries = [], receipts = [];
  // Canonical order also makes reordered selection controls yield the same ZIP.
  for (const format of Object.keys(FORMATS)) {
    const output = owned.get(format);
    if (!output) continue;
    bundleCheck(signal);
    const bytes = new Uint8Array(await wait(Blob.prototype.arrayBuffer.call(output.blob)));
    const hash = await wait(digest("SHA-256", bytes));
    if (!(hash instanceof ArrayBuffer) || hash.byteLength !== 32)
      throw fail("BUNDLE_DIGEST_FAILED", "SHA-256 returned an invalid receipt.");
    const sha256 = Array.from(new Uint8Array(hash), value => value.toString(16).padStart(2, "0")).join("");
    const crc = await crc32(bytes);
    entries.push({ name: output.filename, size: output.byteLength, blob: output.blob, crc });
    receipts.push(Object.freeze({ format, filename: output.filename, mimeType: output.mimeType,
      byteLength: output.byteLength, sha256 }));
  }
  const manifest = Object.freeze({ schema: "fmd-book-edition-v1", sourceLength,
    files: Object.freeze(receipts) });
  const manifestBytes = new TextEncoder().encode(JSON.stringify(manifest, null, 2) + "\n");
  entries.push({ name: "manifest.json", size: manifestBytes.length,
    blob: new Blob([manifestBytes], { type: "application/json" }), crc: await crc32(manifestBytes) });
  const parts = [], directory = [];
  let offset = 0, directorySize = 0;
  for (const entry of entries) {
    const local = zipRecord(entry.name, entry.size, entry.crc);
    const central = zipRecord(entry.name, entry.size, entry.crc, offset);
    parts.push(local, entry.blob); directory.push(central);
    offset += local.length + entry.size; directorySize += central.length;
  }
  const end = new Uint8Array(22), view = new DataView(end.buffer);
  view.setUint32(0, 0x06054b50, true);
  view.setUint16(8, entries.length, true); view.setUint16(10, entries.length, true);
  view.setUint32(12, directorySize, true); view.setUint32(16, offset, true);
  if (offset + directorySize + end.length > BOOK_EDITION_BUNDLE_LIMIT)
    throw fail("OUTPUT_LIMIT", "Edition archive exceeds its payload and metadata budget.");
  bundleCheck(signal);
  const blob = new Blob([...parts, ...directory, end], { type: "application/zip" });
  return Object.freeze({ format: "book-edition-bundle", filename: "book-edition.zip",
    mimeType: "application/zip", byteLength: blob.size, blob, manifest });
}
