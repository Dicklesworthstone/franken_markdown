// Source/asset ownership for the publishing workbench, not a Markdown renderer.
import { bookTextBytes, prepareBookInput } from "../book_session.mjs";
import { bookError } from "../book_worker.mjs";
import { createBookFontStore } from "./book_font_assets.mjs";

const MIB = 1024 * 1024;
export const BOOK_WORKBENCH_LIMITS = Object.freeze({
  chapters: 128,
  includeSources: 128,
  chapterBytes: 4 * MIB,
  sourceBytes: 16 * MIB,
  images: 128,
  imageBytes: 8 * MIB,
  totalImageBytes: 32 * MIB,
  projectBytes: 64 * MIB,
});
const L = BOOK_WORKBENCH_LIMITS;
const newlineView = (source) => source.replace(/\r\n?/g, "\n");
export function bookPath(value) {
  if (
    typeof value !== "string" ||
    !value ||
    value !== value.trim() ||
    /[\\\x00-\x1f\x7f<>:"|?*#%]/.test(value) ||
    value.split("/").some((part) => !part || part === "." || part === ".." || /[. ]$/.test(part))
  ) {
    throw bookError(
      "INVALID_PATH",
      "Use a relative book path without dot segments, URL escapes or reserved characters.",
    );
  }
  bookTextBytes(value, 1024);
  return value;
}
// The editor keeps one ordered source list. Only publication partitions it;
// include-only files never acquire a chapter number, page, or spine item.
function sourceFiles(value) {
  if (!Array.isArray(value) || value.length > L.chapters + L.includeSources) {
    throw bookError("BOOK_LIMIT", "Use at most 128 chapters and 128 include-only sources.");
  }
  const paths = new Set();
  let total = 0,
    chapterCount = 0,
    includeCount = 0;
  return Array.from(value, (file) => {
    const path = bookPath(file?.path),
      source = file.source,
      givenRole = file.role;
    const role = givenRole === undefined ? "chapter" : givenRole;
    if (role !== "chapter" && role !== "include")
      throw bookError("INVALID_ROLE", "Choose chapter or include-only source.");
    if (role === "chapter" && !/\.(md|markdown)$/i.test(path)) {
      throw bookError("INVALID_PATH", "Chapter paths must end in .md or .markdown.");
    }
    if (role === "include") includeCount++;
    else chapterCount++;
    if (chapterCount > L.chapters || includeCount > L.includeSources) {
      throw bookError("BOOK_LIMIT", "Use at most 128 chapters and 128 include-only sources.");
    }
    if (paths.has(path))
      throw bookError("DUPLICATE_PATH", "Two sources have the same book path; rename one first.");
    paths.add(path);
    total += bookTextBytes(source, L.chapterBytes) + bookTextBytes(path, 1024);
    if (total > L.sourceBytes)
      throw bookError("BOOK_LIMIT", "Workbench source and paths exceed 16 MiB.");
    return { path, source, ...(role === "include" ? { role } : {}) };
  });
}
function settings(value = {}) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw bookError("INVALID_OPTIONS", "Invalid book settings.");
  const defaults = {
    title: "My book",
    author: "",
    lang: "en",
    font: "sans",
    darkMode: "disabled",
    fontScale: 1,
    toc: true,
    pageNumbers: true,
  };
  if (Object.keys(value).some((key) => !Object.hasOwn(defaults, key)))
    throw bookError(
      "INVALID_OPTIONS",
      "Unsupported project settings; assets cannot be restored from a source project.",
    );
  const next = { ...defaults, ...value };
  for (const key of ["title", "author", "lang"]) bookTextBytes(next[key], 4096);
  if (!Number.isFinite(next.fontScale) || next.fontScale < 0.5 || next.fontScale > 3)
    throw bookError("INVALID_OPTIONS", "Font scale must be between 0.5 and 3.");
  // Reuse the production facade's remaining option semantics without assets.
  prepareBookInput([{ path: "validation.md", source: "" }], next);
  return next;
}
/** Validated, source-only snapshot. Copies all mutable containers and never
 * includes image bytes, file handles, fonts, or ambient access grants. */
export function normalizeBookProject(project) {
  if (
    !project ||
    typeof project !== "object" ||
    Array.isArray(project) ||
    ![1, 2].includes(project.schemaVersion) ||
    Object.keys(project).some((key) => !["schemaVersion", "files", "options"].includes(key))
  ) {
    throw bookError("INVALID_PROJECT", "Unsupported source-project schema.");
  }
  // Never interpret a role-bearing v1 file as a chapter: older workbenches
  // cannot represent resources. v2 makes that compatibility boundary explicit.
  if (
    project.schemaVersion === 1 &&
    Array.isArray(project.files) &&
    project.files.some((file) => file && Object.hasOwn(file, "role"))
  ) {
    throw bookError("INVALID_PROJECT", "Source roles require project schema version 2.");
  }
  return {
    schemaVersion: project.schemaVersion,
    files: sourceFiles(project.files),
    options: settings(project.options),
  };
}

/** Admit escaped JSON one chapter at a time before allocating the full string. */
export function serializeBookProject(project) {
  const value = normalizeBookProject(project);
  const parts = [`{"schemaVersion":${value.schemaVersion},"files":[`];
  let total = parts[0].length;
  for (const [i, file] of value.files.entries()) {
    const part = (i ? "," : "") + JSON.stringify(file);
    total += bookTextBytes(part, L.projectBytes - total);
    parts.push(part);
  }
  const tail = '],"options":' + JSON.stringify(value.options) + "}\n";
  bookTextBytes(tail, L.projectBytes - total);
  parts.push(tail);
  return parts.join("");
}

function imageList(value) {
  if (!Array.isArray(value) || value.length > L.images)
    throw bookError("BOOK_LIMIT", "Use at most 128 image files.");
  const paths = new Set();
  let total = 0;
  return value.map((image) => {
    const destination = bookPath(image?.destination),
      bytes = image.bytes;
    if (!/\.(png|jpe?g|svg)$/i.test(destination))
      throw bookError("INVALID_IMAGE", "Choose PNG, JPEG or SVG images.");
    if (paths.has(destination))
      throw bookError(
        "DUPLICATE_PATH",
        "Two images have the same book path; revoke or rename one first.",
      );
    paths.add(destination);
    if (
      !(bytes instanceof Uint8Array) ||
      Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]" ||
      !bytes.byteLength ||
      bytes.byteLength > L.imageBytes
    )
      throw bookError("BOOK_LIMIT", "Images must contain 1 byte through 8 MiB.");
    total += bytes.byteLength;
    if (total > L.totalImageBytes)
      throw bookError("BOOK_LIMIT", "Workbench image bytes exceed 32 MiB.");
    return { destination, bytes };
  });
}
function indexOf(index, length) {
  if (!Number.isInteger(index) || index < 0 || index >= length)
    throw bookError("INVALID_SELECTION", "Choose a chapter first.");
  return index;
}
export function createBookCollection() {
  let files = [],
    images = [],
    options = settings(),
    revision = 0,
    disposed = false;
  const fonts = createBookFontStore();
  const listeners = new Set();
  const alive = () => {
    if (disposed) throw bookError("SESSION_DISPOSED", "Book collection is disposed.");
  };
  function changed() {
    revision++;
    for (const listener of listeners) {
      try {
        listener();
      } catch {
        /* Observers cannot roll back installed source. */
      }
    }
  }
  const remember = (file) => ({ ...file, original: file.source, view: newlineView(file.source) });
  const sources = () =>
    files.map(({ path, source, role }) => ({
      path,
      source,
      ...(role === "include" ? { role } : {}),
    }));
  const project = () => ({
    schemaVersion: files.some((file) => file.role === "include") ? 2 : 1,
    files: sources(),
    options,
  });
  return Object.freeze({
    get revision() {
      alive();
      return revision;
    },
    get files() {
      alive();
      return sources();
    },
    get options() {
      alive();
      return { ...options };
    },
    get images() {
      alive();
      return images.map(({ destination, bytes }) => ({ destination, size: bytes.byteLength }));
    },
    get fonts() {
      alive();
      return fonts.list();
    },
    setFonts(assets, expectedRevision) {
      alive();
      const fence = () => {
        alive();
        if (!Number.isSafeInteger(expectedRevision) || expectedRevision !== revision) {
          throw bookError("STALE_SOURCE", "The book changed; font assignments were not replaced.");
        }
      };
      fence();
      fonts.set(assets, fence);
      changed();
    },
    revokeFont(slot) {
      alive();
      if (fonts.remove(slot)) changed();
    },
    revokeFonts() {
      alive();
      if (fonts.clear()) changed();
    },
    subscribe(listener) {
      alive();
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    append(batch) {
      alive();
      if (
        !Array.isArray(batch?.chapters) ||
        !Array.isArray(batch?.images) ||
        (batch.includeSources !== undefined && !Array.isArray(batch.includeSources))
      ) {
        throw bookError(
          "INVALID_INPUT",
          "Import needs chapter, image and optional include-source arrays.",
        );
      }
      const next = sourceFiles([
        ...sources(),
        ...batch.chapters,
        ...(batch.includeSources ?? []).map((file) => ({ ...file, role: "include" })),
      ]);
      const assets = imageList([...images, ...batch.images]);
      // Validate everything before installing either half. Previously authorized
      // images are immutable; take ownership of only newly admitted input bytes.
      const added = assets
        .slice(images.length)
        .map((image) => ({ ...image, bytes: image.bytes.slice() }));
      files = [...files, ...next.slice(files.length).map(remember)];
      images = [...images, ...added];
      changed();
    },
    edit(index, path, sourceView) {
      alive();
      const file = files[indexOf(index, files.length)];
      if (typeof path !== "string" || typeof sourceView !== "string")
        throw bookError("INVALID_INPUT", "Source and path must be strings.");
      // Keep even temporarily invalid typed text. Export validates it; editing
      // never silently drops a keystroke or replaces the user's source.
      const source = sourceView === file.view ? file.original : sourceView;
      if (path === file.path && source === file.source) return;
      files[index] = { ...file, path, source };
      changed();
    },
    setRole(index, role) {
      alive();
      const file = files[indexOf(index, files.length)];
      if (role !== "chapter" && role !== "include")
        throw bookError("INVALID_ROLE", "Choose chapter or include-only source.");
      if ((file.role ?? "chapter") === role) return;
      const next = sources();
      next[index] = { ...file, role };
      // Validate before mutation, preserving the imported bytes/view relation.
      const validated = sourceFiles(next);
      files[index] = { ...file, role: validated[index].role };
      changed();
    },
    move(index, delta) {
      alive();
      indexOf(index, files.length);
      if (delta !== -1 && delta !== 1)
        throw bookError("INVALID_SELECTION", "Move one chapter at a time.");
      const next = index + delta;
      if (next < 0 || next >= files.length) return index;
      [files[index], files[next]] = [files[next], files[index]];
      changed();
      return next;
    },
    remove(index) {
      alive();
      files.splice(indexOf(index, files.length), 1);
      changed();
    },
    configure(value) {
      alive();
      const next = settings(value);
      options = next;
      changed();
    },
    revokeImages() {
      alive();
      images = [];
      changed();
    },
    snapshot() {
      alive();
      const all = sourceFiles(sources());
      const ordered = all.filter((file) => file.role !== "include");
      const includeSources = all
        .filter((file) => file.role === "include")
        .map(({ path, source }) => ({ path, source }));
      if (!ordered.length)
        throw bookError("EMPTY_BOOK", "Add at least one chapter before exporting.");
      return prepareBookInput(ordered, {
        ...settings(options), images, includeSources, fontAssets: fonts.snapshot(),
      });
    },
    chapterDownload(index) {
      alive();
      const file = files[indexOf(index, files.length)];
      bookTextBytes(file.source, L.chapterBytes);
      const include = file.role === "include";
      let filename = include ? `source-${index + 1}.txt` : `chapter-${index + 1}.md`;
      try {
        const path = bookPath(file.path);
        if (include || /\.(md|markdown)$/i.test(path)) filename = path.split("/").at(-1);
      } catch {
        /* A half-edited path must not prevent downloading valid source. */
      }
      return {
        filename,
        blob: new Blob([file.source], {
          type: include ? "text/plain; charset=utf-8" : "text/markdown; charset=utf-8",
        }),
      };
    },
    project() {
      alive();
      return normalizeBookProject(project());
    },
    projectDownload() {
      alive();
      const json = serializeBookProject(project());
      return {
        filename: "book.fmdbook.json",
        blob: new Blob([json], { type: "application/json" }),
      };
    },
    /** One source-only transaction: paths/order/settings/image grants cannot
     * change. Validation and optimistic revision checks precede all mutation.
     * Unchanged chapters keep their imported-byte/editor-view relationship. */
    replaceSources(value, expectedRevision) {
      alive();
      const fence = () => {
        if (!Number.isSafeInteger(expectedRevision) || expectedRevision !== revision) {
          throw bookError("STALE_SOURCE", "The book changed; no source transaction was installed.");
        }
      };
      fence();
      const next = sourceFiles(value);
      fence();
      if (
        next.length !== files.length ||
        next.some((file, i) => file.path !== files[i].path || file.role !== files[i].role)
      ) {
        throw bookError(
          "INVALID_TRANSACTION",
          "Source transactions cannot add, remove, rename, reorder or change source roles.",
        );
      }
      if (next.every((file, i) => file.source === files[i].source)) return revision;
      const installed = revision + 1;
      files = next.map((file, i) => (file.source === files[i].source ? files[i] : remember(file)));
      changed();
      return installed;
    },
    replaceProject(project) {
      alive();
      const value = normalizeBookProject(project);
      files = value.files.map(remember);
      options = value.options;
      images = [];
      fonts.clear();
      changed();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      files = [];
      images = [];
      fonts.dispose();
      listeners.clear();
    },
  });
}

function localFile(file, limit) {
  if (
    Object.prototype.toString.call(file) !== "[object File]" ||
    typeof file.arrayBuffer !== "function" ||
    !Number.isSafeInteger(file.size) ||
    file.size < 0 ||
    file.size > limit
  ) {
    throw bookError("FILE_LIMIT", "Choose a local file within the documented byte limit.");
  }
}
async function read(file) {
  const buffer = await file.arrayBuffer();
  if (
    Object.prototype.toString.call(buffer) !== "[object ArrayBuffer]" ||
    buffer.byteLength !== file.size
  ) {
    throw bookError("FILE_READ_FAILED", "File size changed while reading; nothing was imported.");
  }
  return buffer;
}
/** Folder imports preserve relative paths beneath the selected root. Unknown
 * extensions are counted, never interpreted or granted authority. No sorting:
 * array order remains explicit, and the UI can reorder chapters afterward.
 */
export async function readBookFiles(selected, { folder = false, role = "chapter" } = {}) {
  if (role !== "chapter" && role !== "include")
    throw bookError("INVALID_ROLE", "Choose chapter or include-only import.");
  const input = Array.from(selected);
  if (!input.length || input.length > 4096)
    throw bookError("FILE_LIMIT", "Choose 1 through 4096 files.");
  const admitted = [];
  let sourceBytes = 0,
    imageBytes = 0,
    ignored = 0,
    root = null,
    chapterCount = 0,
    imageCount = 0;
  for (const file of input) {
    let path = folder ? file.webkitRelativePath : file.name;
    if (folder) {
      if (typeof path !== "string" || !path.includes("/"))
        throw bookError("INVALID_PATH", "Folder files must include their relative path.");
      const split = path.indexOf("/");
      root ??= path.slice(0, split);
      if (root !== path.slice(0, split))
        throw bookError("INVALID_PATH", "Choose one root folder at a time.");
      path = path.slice(split + 1);
    }
    const kind =
      role === "include"
        ? "include"
        : /\.(md|markdown)$/i.test(path)
          ? "chapter"
          : /\.(png|jpe?g|svg)$/i.test(path)
            ? "image"
            : null;
    if (!kind) {
      ignored++;
      continue;
    }
    bookPath(path);
    localFile(file, kind !== "image" ? L.chapterBytes : L.imageBytes);
    if (kind !== "image") {
      sourceBytes += file.size + bookTextBytes(path, 1024);
      chapterCount++;
    } else {
      imageBytes += file.size;
      imageCount++;
    }
    admitted.push({ file, path, kind });
  }
  if (
    chapterCount > (role === "include" ? L.includeSources : L.chapters) ||
    imageCount > L.images ||
    sourceBytes > L.sourceBytes ||
    imageBytes > L.totalImageBytes
  ) {
    throw bookError(
      "BOOK_LIMIT",
      "The selected collection exceeds workbench source or image limits.",
    );
  }
  const result = {
    chapters: [],
    images: [],
    ignored,
    ...(role === "include" ? { includeSources: [] } : {}),
  };
  for (const { file, path, kind } of admitted) {
    const bytes = await read(file);
    if (kind === "image") result.images.push({ destination: path, bytes: new Uint8Array(bytes) });
    else {
      let source;
      try {
        source = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
      } catch {
        throw bookError("INVALID_UNICODE", "A source file is not UTF-8; no files were imported.");
      }
      (kind === "include" ? result.includeSources : result.chapters).push({ path, source });
    }
  }
  sourceFiles([
    ...result.chapters,
    ...(result.includeSources ?? []).map((file) => ({ ...file, role: "include" })),
  ]);
  imageList(result.images);
  return result;
}
export async function readBookProject(file) {
  localFile(file, L.projectBytes);
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(await read(file)));
  } catch (error) {
    throw bookError(
      "INVALID_PROJECT",
      error?.code === "FILE_READ_FAILED"
        ? error.message
        : "Source project is not valid UTF-8 JSON.",
    );
  }
}
