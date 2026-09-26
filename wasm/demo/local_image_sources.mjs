import { FlowAssetError } from "../flow_raster.mjs";

const fail = (message) => {
  throw new FlowAssetError("INVALID_LOCAL_IMAGES", message);
};
const MAX_FILES = 128;
const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES = 32 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES = 4096;
const MAX_PATH_UNITS = 4096;
const MAX_DIRECTORY_PATH_UNITS = 1024 * 1024;
const MAX_URL_UNITS = 65536;
const MAX_REFERENCE_UNITS = 1024 * 1024;
const encodedName = (name) => encodeURIComponent(name).replace(
  /[!'()*]/g,
  (ch) => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`,
);
const reference = (name, destination) => `![${name.replace(/[\\[\]]/g, "\\$&")}](${destination})`;

function wellFormed(text) {
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return false;
  }
  return true;
}

function validName(name) {
  return typeof name === "string" && name.length > 0 && name.length <= 512 &&
    wellFormed(name) && !/[\\/:?#<>\u0000-\u001f\u007f]/.test(name) &&
    name !== "." && name !== "..";
}

function loader(lookup) {
  return async (request, { signal, maxBytes }) => {
    const file = lookup(request.url);
    if (!file) return null;
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1 || file.size > maxBytes)
      fail("Selected image exceeds this loader's byte limit");
    signal.throwIfAborted();
    const bytes = new Uint8Array(await file.arrayBuffer());
    signal.throwIfAborted();
    if (bytes.byteLength !== file.size || bytes.byteLength > maxBytes)
      fail("Selected image changed while reading");
    return bytes;
  };
}

/** Snapshot an explicit flat picker grant. No directory traversal, URL resolution,
 * percent decoding, credential inference, network fallback or mutable file map. */
export function createLocalImageSources(files) {
  const byName = new Map(), references = [];
  let count = 0, total = 0;
  for (const file of files) {
    if (++count > MAX_FILES) fail("Select at most 128 images");
    if (!(file instanceof Blob) || !validName(file.name))
      fail("Select files with plain local names");
    if (!file.size || file.size > MAX_FILE_BYTES || (total += file.size) > MAX_TOTAL_BYTES)
      fail("Selected files exceed the 8 MiB per-file or 32 MiB total limit");
    const encoded = encodedName(file.name);
    for (const name of new Set([file.name, encoded])) {
      if (byName.has(name)) fail("Duplicate or ambiguously encoded local filenames");
      byName.set(name, file);
    }
    references.push(reference(file.name, encoded));
  }
  return Object.freeze({
    count,
    references: Object.freeze(references),
    load: loader((url) => typeof url === "string"
      ? byName.get(url.startsWith("./") ? url.slice(2) : url) : undefined),
  });
}

function pathParts(path) {
  if (typeof path !== "string" || path.length > MAX_PATH_UNITS) return null;
  const parts = path.split("/");
  return parts.length <= 64 && parts.every(validName) ? parts : null;
}

/** Capture only PNG/JPEG File objects from one explicit directory selection.
 * documentPath is a literal, root-relative Markdown filename, NOT a URL.
 * Nothing is read until load(); nonimage files grant no readable authority.
 * The directory name is removed, not guessed from a request or browser URL. */
export function createDirectoryImageSources(files, { documentPath = "" } = {}) {
  let base = [];
  if (documentPath !== "") {
    const parts = pathParts(documentPath);
    if (!parts || !/\.(md|markdown|txt)$/i.test(parts.at(-1)))
      fail("Markdown path must be a plain relative .md, .markdown or .txt filename");
    base = parts.slice(0, -1);
  }
  const byPath = new Map(), seen = new Set(), entries = [];
  let root = null, count = 0, total = 0, pathUnits = 0, scanned = 0;
  for (const file of files) {
    if (++scanned > MAX_DIRECTORY_ENTRIES) fail("Select a folder with at most 4096 files");
    if (!(file instanceof Blob)) fail("Directory entries must be selected File objects");
    const path = file.webkitRelativePath;
    const parts = pathParts(path);
    if (!parts || parts.length < 2 || parts.at(-1) !== file.name)
      fail("Directory files need an intact picker-relative path");
    if ((pathUnits += path.length) > MAX_DIRECTORY_PATH_UNITS)
      fail("Directory path metadata exceeds the 1 MiB limit");
    if (root === null) root = parts[0];
    if (parts[0] !== root) fail("Select one folder, not files from different roots");
    const relative = parts.slice(1).join("/");
    if (seen.has(relative)) fail("Duplicate directory paths are not allowed");
    seen.add(relative);
    // Extensions restrict the grant, not the decoder. FlowImageAssets still
    // validates the actual container and pixel limits before allocating ink.
    if (!/\.(png|jpe?g)$/i.test(parts.at(-1))) continue;
    if (++count > MAX_FILES) fail("Select a folder with at most 128 PNG/JPEG images");
    if (!file.size || file.size > MAX_FILE_BYTES || (total += file.size) > MAX_TOTAL_BYTES)
      fail("Selected images exceed the 8 MiB per-file or 32 MiB total limit");
    byPath.set(relative, file);
    entries.push({ path: parts.slice(1), name: parts.at(-1) });
  }
  // Stable insertion order independent of a platform's directory enumeration.
  entries.sort((a, b) => {
    const left = a.path.join("/"), right = b.path.join("/");
    return left < right ? -1 : left > right ? 1 : 0;
  });
  let referenceUnits = 0;
  const references = entries.map(({ path, name }) => {
    let shared = 0;
    while (shared < base.length && shared < path.length - 1 && base[shared] === path[shared]) shared++;
    const destination = [
      ...Array(base.length - shared).fill(".."),
      ...path.slice(shared).map(encodedName),
    ].join("/");
    const markdown = reference(name, destination);
    if ((referenceUnits += markdown.length) > MAX_REFERENCE_UNITS)
      fail("Generated image references exceed the 1 MiB limit");
    return markdown;
  });
  const lookup = (url) => {
    if (typeof url !== "string" || !url.length || url.length > MAX_URL_UNITS ||
      !wellFormed(url) || /^[\/\\]/.test(url) || /[\\:?#\u0000-\u001f\u007f]/.test(url)) return null;
    const parts = url.split("/");
    if (parts.length > 128) return null;
    const resolved = [...base];
    for (const raw of parts) {
      if (raw === ".") continue;
      if (raw === "..") {
        if (!resolved.length) return null;
        resolved.pop();
        continue;
      }
      let name;
      try { name = decodeURIComponent(raw); } catch { return null; }
      // Decode exactly once. Encoded separators/dot segments, absolute paths,
      // schemes, queries and fragments never acquire authority from a grant.
      if (!validName(name)) return null;
      resolved.push(name);
    }
    return byPath.get(resolved.join("/"));
  };
  return Object.freeze({
    count,
    rootName: root ?? "",
    documentPath,
    references: Object.freeze(references),
    load: loader(lookup),
  });
}

/** Optional demo controls. The host owns revoking pixels, native bytes and
 * prepared exports in onChange, and calls clear when switching documents. */
export function createDirectoryImageControls({ container, status, onChange }) {
  const document = container.ownerDocument;
  const panel = document.createElement("details");
  const summary = document.createElement("summary");
  summary.textContent = "Load nested images from a local folder";
  const folder = document.createElement("input");
  folder.type = "file";
  folder.id = "image-folder";
  folder.multiple = true;
  const supported = "webkitdirectory" in folder;
  if (supported) folder.webkitdirectory = true;
  const folderLabel = document.createElement("label");
  folderLabel.htmlFor = folder.id;
  folderLabel.textContent = "Choose the folder containing the Markdown and its images";
  const path = document.createElement("input");
  path.type = "text";
  path.id = "image-document-path";
  path.maxLength = MAX_PATH_UNITS;
  path.autocomplete = "off";
  path.spellcheck = false;
  path.placeholder = "docs/guide.md (leave blank for the folder root)";
  const pathLabel = document.createElement("label");
  pathLabel.htmlFor = path.id;
  pathLabel.textContent = "Markdown file path within that folder";
  const apply = document.createElement("button");
  apply.type = "button";
  apply.id = "apply-image-folder";
  apply.textContent = "Use this image folder";
  const help = document.createElement("p");
  help.id = "image-folder-help";
  help.textContent = supported
    ? "Choose a small document folder, not your home directory. Only its PNG/JPEG files are read, on demand; other file contents are ignored. At most 4096 entries and 128 images. The path is relative to the selected folder, without its name. Apply replaces image access, clears old pixels and prepared exports, and restarts preview; it does not open or edit Markdown. Nothing is uploaded."
    : "Folder selection is unavailable in this browser. The individual-image picker above remains available.";
  folder.setAttribute("aria-describedby", `${help.id} image-status`);
  path.setAttribute("aria-describedby", `${help.id} image-status`);
  panel.append(summary, folderLabel, folder, pathLabel, path, help, apply);
  container.append(panel);
  let disposed = false, suspended = false;
  const enabled = () => {
    folder.disabled = path.disabled = disposed || suspended || !supported;
    apply.disabled = folder.disabled || !folder.files?.length;
  };
  const clear = () => {
    folder.value = "";
    path.value = "";
    enabled();
  };
  const selected = () => {
    if (disposed || suspended || !supported) {
      clear();
      return;
    }
    enabled();
    if (folder.files?.length)
      status.textContent = "Folder selection is pending. Set the Markdown path, then use this image folder to replace the current image grant.";
  };
  const activate = () => {
    if (disposed || suspended || !supported || !folder.files?.length) return;
    let next;
    try {
      next = createDirectoryImageSources(folder.files, { documentPath: path.value });
    } catch (error) {
      status.textContent = `${error.code ?? "IMAGE_ERROR"}: ${error.message}. The previous grant is unchanged.`;
      return;
    }
    try {
      onChange(next);
    } catch (error) {
      // A host may have revoked its old grant before failing to start its new
      // renderer. Do not claim that an arbitrary host side effect rolled back.
      status.textContent = `Could not activate the image folder: ${error.message}. Source is unchanged; revoke image access before retrying.`;
    }
  };
  folder.addEventListener("change", selected);
  apply.addEventListener("click", activate);
  enabled();
  return Object.freeze({
    clear,
    suspend() { suspended = true; clear(); },
    resume() { if (!disposed) { suspended = false; clear(); } },
    dispose() {
      if (disposed) return;
      disposed = true;
      clear();
      folder.removeEventListener("change", selected);
      apply.removeEventListener("click", activate);
      panel.remove();
    },
  });
}
