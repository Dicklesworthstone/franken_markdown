// Source/asset ownership for the publishing workbench, not a Markdown renderer.
import { bookTextBytes, prepareBookInput } from "../book_session.mjs";
import { bookError } from "../book_worker.mjs";
import { BOOK_FONT_LIMITS, BOOK_FONT_SLOTS, createBookFontStore } from "./book_font_assets.mjs";

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
  if (Object.keys(value).some((key) => key !== "page" && !Object.hasOwn(defaults, key)))
    throw bookError(
      "INVALID_OPTIONS",
      "Unsupported project settings; assets cannot be restored from a source project.",
    );
  const next = { ...defaults, ...value };
  for (const key of ["title", "author", "lang"]) bookTextBytes(next[key], 4096);
  if (!Number.isFinite(next.fontScale) || next.fontScale < 0.5 || next.fontScale > 3)
    throw bookError("INVALID_OPTIONS", "Font scale must be between 0.5 and 3.");
  // Reuse the production facade's remaining option semantics without assets.
  const { page } = prepareBookInput([{ path: "validation.md", source: "" }], next).options;
  // Keep the deeply owned/frozen canonical geometry, never caller-owned nested
  // objects. Absence stays absent so old/default books keep their original ABI.
  if (page === undefined) delete next.page;
  else next.page = page;
  return next;
}
/** Validated, source-only snapshot. Copies all mutable containers and never
 * includes image bytes, file handles, fonts, or ambient access grants. */
export function normalizeBookProject(project) {
  if (
    !project ||
    typeof project !== "object" ||
    Array.isArray(project) ||
    ![1, 2, 3].includes(project.schemaVersion) ||
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
  const options = settings(project.options);
  if (project.schemaVersion < 3 && options.page !== undefined)
    throw bookError("INVALID_PROJECT", "PDF page settings require source-project schema version 3.");
  return {
    schemaVersion: project.schemaVersion,
    files: sourceFiles(project.files),
    options,
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
// Capture image changes without executing property getters, custom iterators,
// slice overrides or typed-array species. These are byte transactions, not an
// image decoder: publication uses the same engine admission as ordinary imports.
function imageChanges(current, changes) {
  const invalid = () => bookError("INVALID_IMAGE_CHANGE", "Use existing image paths with either bytes or remove: true.");
  const data = (object, key) => {
    const field = Object.getOwnPropertyDescriptor(object, key);
    if (!field || !Object.hasOwn(field, "value")) throw invalid();
    return field.value;
  };
  if (!Array.isArray(changes)) throw invalid();
  const count = data(changes, "length");
  if (!Number.isSafeInteger(count) || count < 1 || count > L.images) throw invalid();
  const existing = new Map(current.map(image => [image.destination, image]));
  const replacements = new Map();
  const typed = Object.getPrototypeOf(Uint8Array.prototype);
  const intrinsic = (key, value) => Object.getOwnPropertyDescriptor(typed, key).get.call(value);
  for (let i = 0; i < count; i++) {
    const value = data(changes, String(i));
    if (!value || typeof value !== "object" || Array.isArray(value)
        || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) throw invalid();
    const keys = Reflect.ownKeys(value);
    if (keys.length !== 2 || !keys.includes("destination")
        || !(keys.includes("bytes") || keys.includes("remove"))) throw invalid();
    const destination = bookPath(data(value, "destination"));
    if (!existing.has(destination))
      throw bookError("IMAGE_NOT_FOUND", "The image path is not authorized; import new images separately.");
    if (replacements.has(destination))
      throw bookError("DUPLICATE_PATH", "Change each image path at most once in a batch.");
    if (keys.includes("remove")) {
      if (data(value, "remove") !== true) throw invalid();
      replacements.set(destination, null);
      continue;
    }
    const input = data(value, "bytes");
    let bytes;
    try {
      if (intrinsic(Symbol.toStringTag, input) !== "Uint8Array") throw invalid();
      const buffer = intrinsic("buffer", input);
      // The ArrayBuffer intrinsic rejects shared buffers even with a spoofed tag.
      Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(buffer);
      const resizable = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "resizable");
      if (resizable?.get.call(buffer)) throw invalid();
      bytes = new Uint8Array(buffer, intrinsic("byteOffset", input), intrinsic("byteLength", input));
    } catch {
      throw bookError("INVALID_IMAGE_CHANGE", "Replacement bytes need a live, fixed, non-shared Uint8Array.");
    }
    if (bytes.length < 1 || bytes.length > L.imageBytes)
      throw bookError("BOOK_LIMIT", "Images must contain 1 byte through 8 MiB.");
    replacements.set(destination, bytes);
  }
  // Admit the final set, independent of change order. A removal/shrink can fund
  // a larger replacement, and a same-size replacement works at full capacity.
  let total = 0;
  for (const image of current) {
    const bytes = replacements.has(image.destination) ? replacements.get(image.destination) : image.bytes;
    total += bytes?.byteLength ?? 0;
  }
  if (total > L.totalImageBytes)
    throw bookError("BOOK_LIMIT", "Workbench image bytes exceed 32 MiB.");
  let different = false;
  const next = [];
  for (const image of current) {
    if (!replacements.has(image.destination)) { next.push(image); continue; }
    const bytes = replacements.get(image.destination);
    if (bytes === null) { different = true; continue; }
    // Copy only after complete metadata/size admission; never retain a caller's
    // mutable view. Construction ignores overridden slice/iterator/species.
    const owned = new Uint8Array(bytes);
    let equal = owned.length === image.bytes.length;
    for (let i = 0; equal && i < owned.length; i++) equal = owned[i] === image.bytes[i];
    if (equal) next.push(image);
    else { next.push({ destination: image.destination, bytes: owned }); different = true; }
  }
  return different ? next : null;
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
    configurationRevision = 0,
    disposed = false;
  let fonts = createBookFontStore();
  const listeners = new Set();
  const alive = () => {
    if (disposed) throw bookError("SESSION_DISPOSED", "Book collection is disposed.");
  };
  function changed(sourceOnly = false) {
    revision++;
    if (!sourceOnly) configurationRevision++;
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
    schemaVersion: options.page !== undefined ? 3 : files.some((file) => file.role === "include") ? 2 : 1,
    files: sources(),
    options,
  });
  return Object.freeze({
    get revision() {
      alive();
      return revision;
    },
    // Ephemeral identity for the paths/order/settings/resource configuration.
    // Source-only edits keep it stable; every other transaction invalidates it.
    // This is a lifecycle hint, never a substitute for full input validation.
    get renderConfigurationRevision() {
      alive();
      return configurationRevision;
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
      changed(path === file.path);
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
    /** Change only PDF geometry, preserving source, settings and resources.
     * A reset to undefined restores the legacy renderer's default-page path. */
    setPage(page, expectedRevision) {
      alive();
      const fence = () => {
        alive();
        if (!Number.isSafeInteger(expectedRevision) || expectedRevision !== revision)
          throw bookError("STALE_SOURCE", "The book changed; its PDF page was not replaced.");
      };
      fence();
      const next = settings({ ...options, page });
      fence();
      if (JSON.stringify(next.page) === JSON.stringify(options.page)) return revision;
      const installed = revision + 1;
      options = next;
      changed();
      return installed;
    },
    /** Replace/remove existing image bindings in one observable revision.
     * Source, image order, fonts and settings are untouched. No-op byte copies
     * retain both revision counters and do not invalidate previews or proofs. */
    changeImages(changes, expectedRevision) {
      const fence = () => {
        alive();
        if (!Number.isSafeInteger(expectedRevision) || expectedRevision !== revision)
          throw bookError("STALE_SOURCE", "The book changed; its images were not replaced.");
      };
      fence();
      const next = imageChanges(images, changes);
      fence(); // Proxy introspection may have run host code during admission.
      if (next === null) return revision;
      const installed = revision + 1;
      images = next;
      changed();
      return installed;
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
    /** Explicit portable download. Unlike projectDownload/local-library saves,
     * this includes the currently authorized image and font bytes. */
    async portableDownload({ signal } = {}) {
      alive();
      portableSignal(signal);
      const expected = revision;
      const fence = () => {
        alive();
        portableCheck(signal);
        if (revision !== expected)
          throw bookError("STALE_SOURCE", "The book changed; prepare its portable copy again.");
      };
      // Source and resource bytes are captured before yielding; later caller
      // mutation or collection disposal cannot retarget this snapshot.
      const source = serializeBookProject(project());
      const names = new Map(fonts.list().map(font => [font.slot, font.name]));
      const faces = fonts.snapshot().map(font => ({ ...font, name: names.get(font.slot) }));
      const assets = images.map(image => ({ destination: image.destination, bytes: new Uint8Array(image.bytes) }));
      const output = await portableEncode(source, assets, faces, signal, fence);
      fence();
      return { ...output, revision: expected };
    },
    /** Install a privately admitted portable file as ONE source/resource
     * revision. Reading a file is not authorization: the host must ask first. */
    replacePortableProject(prepared, expectedRevision, { authorizeResources = false } = {}) {
      alive();
      if (authorizeResources !== true)
        throw bookError("RESOURCE_AUTHORIZATION_REQUIRED", "Approve restoring the portable book's embedded resources first.");
      if (!Number.isSafeInteger(expectedRevision) || expectedRevision !== revision)
        throw bookError("STALE_SOURCE", "The book changed; no portable project was installed.");
      const value = portablePrepared.get(prepared);
      if (!value)
        throw bookError("INVALID_PROJECT", "Read a portable book file before restoring it; this review is no longer available.");
      const next = value.project.files.map(remember);
      const installed = revision + 1;
      // Everything, including all font resource admission/copies, has already
      // succeeded. Transfer private ownership; observers never see half a book.
      portablePrepared.delete(prepared);
      const oldFonts = fonts;
      files = next;
      options = value.project.options;
      images = value.images;
      fonts = value.fonts;
      oldFonts.dispose();
      changed();
      return installed;
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
      changed(true);
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

// A separate, opt-in format: older/source-only readers must refuse it instead
// of quietly dropping resources. Base64 is a byte encoding, not a ZIP parser.
export const PORTABLE_BOOK_FORMAT = "franken-markdown-portable-book";
export const PORTABLE_BOOK_MAX_BYTES = 192 * MIB;
const portablePrepared = new WeakMap();
const BASE64_CHUNK = 16 * 1024; // Multiple of four; independent of asset size.
function portableSignal(signal) {
  if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
      || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
    throw bookError("INVALID_OPTIONS", "Use an AbortSignal for portable book cancellation.");
  portableCheck(signal);
}
function portableCheck(signal) {
  if (signal?.aborted) throw bookError("ABORTED", "Portable book operation cancelled; the collection is unchanged.");
}
function portableWait(promise, signal) {
  if (!signal) return Promise.resolve(promise);
  return new Promise((resolve, reject) => {
    const abort = () => { cleanup(); reject(bookError("ABORTED", "Portable book operation cancelled.")); };
    const cleanup = () => signal.removeEventListener("abort", abort);
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(promise).then(value => { cleanup(); resolve(value); }, error => { cleanup(); reject(error); });
    if (signal.aborted) abort();
  });
}
async function portableTurn(signal, fence = () => {}) {
  portableCheck(signal); fence();
  await portableWait(new Promise(resolve => setTimeout(resolve, 0)), signal);
  portableCheck(signal); fence();
}
function portableRecord(value, keys) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || Object.keys(value).some(key => !keys.includes(key)))
    throw bookError("INVALID_PROJECT", "Invalid portable book record or unsupported field.");
}
function portableAssetSize(asset, maximum) {
  if (asset.encoding !== "base64" || typeof asset.data !== "string"
      || !asset.data.length || asset.data.length % 4 !== 0)
    throw bookError("INVALID_PROJECT", "Embedded resources require nonempty, padded base64.");
  const padding = asset.data.endsWith("==") ? 2 : asset.data.endsWith("=") ? 1 : 0;
  const size = asset.data.length / 4 * 3 - padding;
  if (size < 1 || size > maximum)
    throw bookError("BOOK_LIMIT", "An embedded resource exceeds its byte limit.");
  return size;
}
async function portableDecode(asset, signal) {
  const bytes = new Uint8Array(asset.size);
  let written = 0;
  for (let offset = 0; offset < asset.data.length; offset += BASE64_CHUNK) {
    const chunk = asset.data.slice(offset, offset + BASE64_CHUNK);
    let binary;
    try {
      binary = atob(chunk);
      // atob alone accepts whitespace and nonzero padding bits. Require the
      // canonical byte spelling and forbid padding before the final chunk.
      if (btoa(binary) !== chunk || (offset + chunk.length < asset.data.length && chunk.includes("=")))
        throw new Error("noncanonical base64");
    } catch {
      throw bookError("INVALID_PROJECT", "An embedded resource is not canonical base64.");
    }
    for (let i = 0; i < binary.length; i++) bytes[written++] = binary.charCodeAt(i);
    if (offset % (BASE64_CHUNK * 16) === 0) await portableTurn(signal);
  }
  if (written !== bytes.length)
    throw bookError("INVALID_PROJECT", "An embedded resource has an inconsistent byte length.");
  portableCheck(signal);
  return bytes;
}
async function portableEncode(source, images, fontAssets, signal, fence) {
  const parts = [];
  let total = 0;
  const add = (text, ascii = false) => {
    total += ascii ? text.length : bookTextBytes(text, PORTABLE_BOOK_MAX_BYTES - total);
    if (total > PORTABLE_BOOK_MAX_BYTES)
      throw bookError("BOOK_LIMIT", "Portable book exceeds 192 MiB; save source and resources separately.");
    parts.push(text);
  };
  add(`{"format":"${PORTABLE_BOOK_FORMAT}","schemaVersion":1,"project":`);
  add(source);
  for (const [key, assets] of [["images", images], ["fontAssets", fontAssets]]) {
    add(`,"${key}":[`);
    for (let i = 0; i < assets.length; i++) {
      const { bytes, ...metadata } = assets[i];
      add((i ? "," : "") + JSON.stringify({ ...metadata, encoding: "base64" }).slice(0, -1) + ',"data":"');
      const rawChunk = BASE64_CHUNK / 4 * 3;
      for (let offset = 0; offset < bytes.length; offset += rawChunk) {
        add(btoa(String.fromCharCode(...bytes.subarray(offset, offset + rawChunk))), true);
        if (offset % (rawChunk * 16) === 0) await portableTurn(signal, fence);
      }
      add('"}');
    }
    add("]");
  }
  add("}\n");
  fence();
  const blob = new Blob(parts, { type: "application/json" });
  return { filename: "book.fmdbook.bundle.json", blob };
}

/** Read/admit without touching a collection. Only frozen review metadata is
 * exposed; decoded bytes stay private until an explicitly authorized restore.
 * Does not fetch URLs, load fonts, parse images, or claim embedding rights. */
export async function readPortableBookProject(file, { signal } = {}) {
  portableSignal(signal);
  localFile(file, PORTABLE_BOOK_MAX_BYTES);
  const buffer = await portableWait(read(file), signal);
  portableCheck(signal);
  let raw;
  try { raw = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(buffer)); }
  catch { throw bookError("INVALID_PROJECT", "Portable book is not valid UTF-8 JSON."); }
  portableRecord(raw, ["format", "schemaVersion", "project", "images", "fontAssets"]);
  if (raw.format !== PORTABLE_BOOK_FORMAT || raw.schemaVersion !== 1)
    throw bookError("INVALID_PROJECT", "Unsupported portable book format or version.");
  const project = normalizeBookProject(raw.project);
  serializeBookProject(project); // Same escaped-source budget as source-only files.
  if (!Array.isArray(raw.images) || raw.images.length > L.images
      || !Array.isArray(raw.fontAssets) || raw.fontAssets.length > BOOK_FONT_SLOTS.length)
    throw bookError("BOOK_LIMIT", "Portable book has too many image or font resources.");
  const destinations = new Set(), slots = new Set();
  let imageBytes = 0, fontBytes = 0;
  const images = raw.images.map(asset => {
    portableRecord(asset, ["destination", "encoding", "data"]);
    const destination = bookPath(asset.destination);
    if (!/\.(png|jpe?g|svg)$/i.test(destination))
      throw bookError("INVALID_IMAGE", "Portable images must be PNG, JPEG or SVG.");
    if (destinations.has(destination)) throw bookError("DUPLICATE_PATH", "Duplicate portable image path.");
    destinations.add(destination);
    const size = portableAssetSize(asset, L.imageBytes);
    imageBytes += size;
    return { destination, data: asset.data, size };
  });
  const fontAssets = raw.fontAssets.map(asset => {
    portableRecord(asset, ["slot", "name", "weight", "encoding", "data"]);
    const { slot, name, weight } = asset;
    if (slots.has(slot)) throw bookError("INVALID_FONT_SLOT", "Duplicate portable font role.");
    slots.add(slot);
    // Reuse the existing metadata policy before decoding any potentially large
    // payload. One placeholder byte is admission only, never a published font.
    const probe = createBookFontStore();
    try { probe.set([{ slot, name, weight, bytes: new Uint8Array([0]) }]); }
    finally { probe.dispose(); }
    const size = portableAssetSize(asset, BOOK_FONT_LIMITS.faceBytes);
    fontBytes += size;
    return { slot, name, weight, data: asset.data, size };
  });
  if (imageBytes > L.totalImageBytes || fontBytes > BOOK_FONT_LIMITS.totalBytes)
    throw bookError("BOOK_LIMIT", "Portable images or fonts exceed their 32 MiB aggregate limit.");
  // All metadata/count/decoded-size admission precedes every large decode.
  const decoded = [], fonts = createBookFontStore();
  try {
    for (const asset of images)
      decoded.push({ destination: asset.destination, bytes: await portableDecode(asset, signal) });
    for (const asset of fontAssets) {
      const { slot, name, weight } = asset;
      fonts.set([{ slot, name, weight, bytes: await portableDecode(asset, signal) }]);
    }
    portableCheck(signal);
    const review = Object.freeze({
      title: project.options.title,
      chapters: project.files.filter(file => file.role !== "include").length,
      includeSources: project.files.filter(file => file.role === "include").length,
      sourceBytes: project.files.reduce((sum, file) => sum + bookTextBytes(file.source), 0),
      imageBytes, fontBytes,
      sources: Object.freeze(project.files.map(file => Object.freeze({ path: file.path, role: file.role ?? "chapter" }))),
      images: Object.freeze(images.map(({ destination, size }) => Object.freeze({ destination, size }))),
      fonts: Object.freeze(fonts.list().map(font => Object.freeze(font))),
    });
    portablePrepared.set(review, { project, images: decoded, fonts });
    return review;
  } catch (error) { fonts.dispose(); throw error; }
}
/** Release an unused/cancelled review. Successfully restored reviews have
 * already transferred ownership and are harmless to discard. */
export function discardPortableBookProject(review) {
  const value = portablePrepared.get(review);
  if (value) { portablePrepared.delete(review); value.fonts.dispose(); }
}
