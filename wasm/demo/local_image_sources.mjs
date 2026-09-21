import { FlowAssetError } from "../flow-assets.js";

const fail = (message) => {
  throw new FlowAssetError("INVALID_LOCAL_IMAGES", message);
};

/** Snapshot an explicit picker grant. No directory traversal, URL resolution,
 * percent decoding, credential inference, network fallback or mutable file map. */
export function createLocalImageSources(files) {
  const byName = new Map(),
    references = [];
  let count = 0,
    total = 0;
  for (const file of files) {
    if (++count > 128) fail("Select at most 128 images");
    if (
      !(file instanceof Blob) ||
      typeof file.name !== "string" ||
      !file.name.length ||
      file.name.length > 512 ||
      /[\\/:?#<>\u0000-\u001f\u007f]/.test(file.name) ||
      file.name === "." ||
      file.name === ".."
    )
      fail("Select files with plain local names");
    if (!file.size || file.size > 8 * 1024 * 1024 || (total += file.size) > 32 * 1024 * 1024)
      fail("Selected files exceed the 8 MiB per-file or 32 MiB total limit");
    const encoded = encodeURIComponent(file.name).replace(
      /[!'()*]/g,
      (ch) => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`,
    );
    for (const name of new Set([file.name, encoded])) {
      if (byName.has(name)) fail("Duplicate or ambiguously encoded local filenames");
      byName.set(name, file);
    }
    references.push(`![${file.name.replace(/[\\[\]]/g, "\\$&")}](${encoded})`);
  }
  return Object.freeze({
    count,
    references: Object.freeze(references),
    async load(request, { signal, maxBytes }) {
      const url = request.url.startsWith("./") ? request.url.slice(2) : request.url;
      const file = byName.get(url);
      if (!file) return null;
      if (file.size > maxBytes) fail("Selected image exceeds this loader's byte limit");
      signal.throwIfAborted();
      const bytes = new Uint8Array(await file.arrayBuffer());
      signal.throwIfAborted();
      return bytes;
    },
  });
}
