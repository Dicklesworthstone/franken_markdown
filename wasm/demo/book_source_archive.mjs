import { bookZipCrc32, bookZipPaths, createBookZip } from "./book_zip.mjs";

export const BOOK_SOURCE_ARCHIVE_LIMITS = Object.freeze({ files: 256, fileBytes: 4 * 1024 * 1024,
  sourceBytes: 16 * 1024 * 1024, projectBytes: 64 * 1024 * 1024,
  archiveBytes: 81 * 1024 * 1024 });
export const BOOK_SOURCE_ARCHIVE_PROJECT = "book.fmdbook.json";
const ROOT = "sources/", L = BOOK_SOURCE_ARCHIVE_LIMITS;
const fail = (code, message) => Object.assign(new Error(message), { code });
const own = (object, key) => {
  const field = object && Object.getOwnPropertyDescriptor(object, key);
  if (!field || !Object.hasOwn(field, "value"))
    throw fail("INVALID_PROJECT", "Use owned source-project data without accessors or sparse files.");
  return field.value;
};
function utf8Length(text, maximum) {
  if (typeof text !== "string" || text.length > maximum)
    throw fail("BOOK_LIMIT", "Source archive text exceeds its byte limit.");
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw fail("INVALID_UNICODE", "Repair incomplete Unicode before saving source files.");
      bytes += 4;
    } else if (unit >= 0xdc00 && unit <= 0xdfff)
      throw fail("INVALID_UNICODE", "Repair incomplete Unicode before saving source files.");
    else bytes += unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3;
    if (bytes > maximum) throw fail("BOOK_LIMIT", "Source archive text exceeds its byte limit.");
  }
  return bytes;
}

/** Download an editable source tree and its existing source-only project.
 * The host injects the production project codec, as connected project files do;
 * this module never parses Markdown, reads resources, or starts a renderer.
 * Every source/path/setting is captured synchronously before the first await.
 * Source paths live under sources/ so no user source can replace project metadata.
 * STORE keeps exact BOMs, CR/CRLF/LF endings and all valid Unicode unchanged.
 */
export async function createBookSourceArchive(project, {
  projectCodec, signal = new AbortController().signal,
} = {}) {
  if (!signal || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function"
      || typeof signal.removeEventListener !== "function")
    throw fail("INVALID_OPTIONS", "Source ZIP preparation requires an AbortSignal.");
  const check = () => { if (signal.aborted) throw fail("EXPORT_CANCELLED", "Source ZIP preparation cancelled."); };
  check();
  const { normalizeBookProject, serializeBookProject } = projectCodec ?? {};
  if (typeof normalizeBookProject !== "function" || typeof serializeBookProject !== "function")
    throw fail("INVALID_OPTIONS", "Source ZIP preparation requires the existing source-project codec.");
  const snapshot = normalizeBookProject(project), files = own(snapshot, "files");
  const count = Array.isArray(files) ? own(files, "length") : 0;
  if (count < 1 || count > L.files)
    throw fail("BOOK_LIMIT", "Add between 1 and 256 source files before preparing their ZIP.");
  const entries = [];
  let sourceBytes = 0, admittedBytes = 0;
  for (let i = 0; i < count; i++) {
    const file = own(files, String(i)), path = own(file, "path"), source = own(file, "source");
    const size = utf8Length(source, L.fileBytes);
    admittedBytes += size + utf8Length(path, 1024);
    sourceBytes += size;
    if (admittedBytes > L.sourceBytes) throw fail("BOOK_LIMIT", "Source files and paths exceed the 16 MiB ZIP input limit.");
    entries.push({ name: ROOT + path, size, source });
  }
  bookZipPaths([...entries.map(entry => entry.name), BOOK_SOURCE_ARCHIVE_PROJECT]);
  const json = serializeBookProject(snapshot), projectSize = utf8Length(json, L.projectBytes);
  // Immutable Blobs, not retained caller containers, own the admitted bytes.
  for (const entry of entries) {
    entry.blob = new Blob([entry.source], { type: "text/plain; charset=utf-8" });
    delete entry.source;
  }
  entries.push({ name: BOOK_SOURCE_ARCHIVE_PROJECT, size: projectSize,
    blob: new Blob([json], { type: "application/json" }) });
  const wait = pending => new Promise((resolve, reject) => {
    let settled = false;
    const finish = (handler, value) => {
      if (settled) return;
      settled = true; signal.removeEventListener("abort", abort); handler(value);
    };
    const abort = () => finish(reject, fail("EXPORT_CANCELLED", "Source ZIP preparation cancelled."));
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(pending).then(value => finish(resolve, value), error => finish(reject, error));
    if (signal.aborted) abort();
  });
  for (let i = 0; i < entries.length; i++) {
    check();
    entries[i].crc = await bookZipCrc32(entries[i].blob, { check, wait });
    if (i % 16 === 15) await wait(new Promise(resolve => setTimeout(resolve, 0)));
  }
  check();
  const blob = createBookZip(entries, L.archiveBytes);
  check();
  return Object.freeze({ format: "book-source-archive", mimeType: "application/zip", filename: "book-sources.zip",
    blob, byteLength: blob.size, sourceCount: count, sourceBytes,
    sourceRoot: ROOT, projectFile: BOOK_SOURCE_ARCHIVE_PROJECT });
}
