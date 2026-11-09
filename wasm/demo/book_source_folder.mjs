import { bookZipPath, bookZipPaths } from "./book_zip.mjs";
import { BOOK_SOURCE_ARCHIVE_LIMITS, BOOK_SOURCE_ARCHIVE_PROJECT,
  BOOK_SOURCE_ARCHIVE_ROOT } from "./book_source_archive.mjs";

export const BOOK_SOURCE_FOLDER_LIMITS = Object.freeze({ selectedFiles: 4096,
  pathBytes: 4096, totalPathBytes: 4 * 1024 * 1024, ...BOOK_SOURCE_ARCHIVE_LIMITS });
const L = BOOK_SOURCE_FOLDER_LIMITS;
const fail = (code, message) => Object.assign(new Error(message), { code });
const pathKey = path => path.split("/").map(part => part.replace(/[. ]+$/, "").normalize("NFC").toLowerCase()).join("/");

function requiredFiles(entries, names) {
  bookZipPaths(names);
  const required = new Map(names.map(name => [pathKey(name), name]));
  const directories = new Set(), result = new Map();
  for (const key of required.keys()) {
    for (let slash = key.indexOf("/"); slash >= 0; slash = key.indexOf("/", slash + 1))
      directories.add(key.slice(0, slash));
  }
  for (const entry of entries) {
    const key = entry.key, name = required.get(key);
    if (name !== undefined) {
      if (entry.path !== name || result.has(name))
        throw fail("PATH_COLLISION", `The selected folder has duplicate or ambiguous paths for ${name}.`);
      result.set(name, entry.file);
    } else {
      if (directories.has(key))
        throw fail("PATH_COLLISION", "A required source directory is also named by a selected file.");
      for (let slash = key.indexOf("/"); slash >= 0; slash = key.indexOf("/", slash + 1)) {
        if (required.has(key.slice(0, slash)))
          throw fail("PATH_COLLISION", "A required source file is also named as another selected file's directory.");
      }
    }
  }
  return result;
}

/** The selected File objects are the complete read authority. A manifest can
 * name only files already selected beneath its one root; no filesystem handle,
 * network URL, resource grant, archive parser or Markdown parser enters here.
 * The production source-project codec owns roles, order and applied settings.
 */
export async function readBookSourceFolder(selected, { projectCodec,
  signal = new AbortController().signal } = {}) {
  const { normalizeBookProject, readBookProject, readBookSourceFile } = projectCodec ?? {};
  if (![normalizeBookProject, readBookProject, readBookSourceFile].every(fn => typeof fn === "function"))
    throw fail("INVALID_OPTIONS", "Source-folder reopening requires the existing project and source-file readers.");
  if (!signal || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function"
      || typeof signal.removeEventListener !== "function")
    throw fail("INVALID_OPTIONS", "Source-folder reopening requires an AbortSignal.");
  const check = () => { if (signal.aborted) throw fail("IMPORT_CANCELLED", "Source-folder reading cancelled; nothing was reopened."); };
  const wait = pending => new Promise((resolve, reject) => {
    let settled = false;
    const finish = (handler, value) => {
      if (settled) return;
      settled = true; signal.removeEventListener("abort", abort); handler(value);
    };
    const abort = () => finish(reject, fail("IMPORT_CANCELLED", "Source-folder reading cancelled; nothing was reopened."));
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(pending).then(value => finish(resolve, value), error => finish(reject, error));
    if (signal.aborted) abort();
  });
  check();
  const count = selected?.length;
  if (!Number.isSafeInteger(count) || count < 1 || count > L.selectedFiles)
    throw fail("FILE_LIMIT", "Choose one source folder containing 1 through 4096 files.");
  // Snapshot the picker before the first await; avoid an unbounded iterator.
  const entries = [];
  let root = null, pathBytes = 0;
  for (let i = 0; i < count; i++) {
    const file = selected[i], relative = file?.webkitRelativePath;
    if (Object.prototype.toString.call(file) !== "[object File]" || typeof file.arrayBuffer !== "function"
        || typeof relative !== "string" || relative.length > L.pathBytes + 1024)
      throw fail("INVALID_SOURCE_FOLDER", "Choose an extracted source folder using the folder picker.");
    const slash = relative.indexOf("/"), prefix = relative.slice(0, slash);
    if (slash < 1 || prefix === "." || prefix === ".." || prefix.length > 1024
        || /[\\\x00-\x1f\x7f]/.test(prefix))
      throw fail("INVALID_SOURCE_FOLDER", "Each selected file must belong to one named root folder.");
    root ??= prefix;
    if (root !== prefix) throw fail("INVALID_SOURCE_FOLDER", "Choose one root folder at a time.");
    const path = relative.slice(slash + 1);
    if (/[\\\x00-\x1f\x7f]/.test(path) || path.split("/").some(part => !part || part === "." || part === ".."))
      throw fail("INVALID_PATH", "Selected folder paths must be relative paths without traversal or empty components.");
    const encoded = new TextEncoder().encode(path);
    if (new TextDecoder("utf-8", { ignoreBOM: true }).decode(encoded) !== path)
      throw fail("INVALID_UNICODE", "Selected folder paths must contain well-formed Unicode.");
    pathBytes += encoded.length;
    if (encoded.length > L.pathBytes || pathBytes > L.totalPathBytes)
      throw fail("FILE_LIMIT", "Selected source-folder paths exceed the metadata limit.");
    entries.push({ path, key: pathKey(path), file });
  }
  // Unrelated assets may have names such as plot#1.svg. Bound their metadata,
  // but apply portable namespace rules only where a selected name could alias
  // a required file or make one of its ancestors/descendants ambiguous.
  const manifest = requiredFiles(entries, [BOOK_SOURCE_ARCHIVE_PROJECT]).get(BOOK_SOURCE_ARCHIVE_PROJECT);
  if (!manifest) throw fail("SOURCE_MANIFEST_MISSING", `Choose the folder containing ${BOOK_SOURCE_ARCHIVE_PROJECT} at its root.`);
  check();
  const project = normalizeBookProject(await wait(readBookProject(manifest)));
  check();
  if (!project.files.length || project.files.length > L.files)
    throw fail("BOOK_LIMIT", "A source folder must declare between 1 and 256 source files.");
  const names = project.files.map(file => BOOK_SOURCE_ARCHIVE_ROOT + file.path);
  const available = requiredFiles(entries, [BOOK_SOURCE_ARCHIVE_PROJECT, ...names]);
  let sourceBytes = 0, admittedBytes = 0;
  const sources = project.files.map((source, index) => {
    const name = names[index], file = available.get(name);
    if (!file) throw fail("SOURCE_FILE_MISSING", `The selected folder is missing ${name}. No source was reopened.`);
    const size = file.size;
    if (!Number.isSafeInteger(size) || size < 0 || size > L.fileBytes)
      throw fail("FILE_LIMIT", `Source file ${name} exceeds the 4 MiB limit.`);
    sourceBytes += size;
    admittedBytes += size + bookZipPath(source.path).length;
    if (admittedBytes > L.sourceBytes)
      throw fail("BOOK_LIMIT", "Current source files and paths exceed 16 MiB.");
    return { source, file, size };
  });
  const files = [], review = [];
  let changedSources = 0, chapters = 0, includeSources = 0;
  for (const item of sources) {
    check();
    const source = await wait(readBookSourceFile(item.file));
    check();
    if (item.file.size !== item.size)
      throw fail("FILE_READ_FAILED", "A source file changed size while reading; choose the folder again.");
    const changed = source !== item.source.source;
    if (changed) changedSources++;
    const role = item.source.role === "include" ? "include" : "chapter";
    if (role === "include") includeSources++; else chapters++;
    files.push({ ...item.source, source });
    review.push(Object.freeze({ path: item.source.path, role, changed }));
  }
  check();
  // Reapply the production aggregate/Unicode/schema admission to current text.
  const restored = normalizeBookProject({ ...project, files });
  return Object.freeze({ project: restored, review: Object.freeze(review),
    folder: root, title: restored.options.title, sourceCount: files.length,
    chapters, includeSources, changedSources, sourceBytes,
    ignored: entries.length - sources.length - 1 });
}
