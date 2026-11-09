import { normalizeBookProject, readBookProject, readBookSourceFile,
  serializeBookProject } from "../demo/book_collection.mjs";
import { createBookSourceArchive } from "../demo/book_source_archive.mjs";
import { readSourceZip } from "./book_source_archive_helpers.mjs";

export const codec = { normalizeBookProject, readBookProject, readBookSourceFile, serializeBookProject };
export const sample = () => ({ schemaVersion: 7, files: [
  { path: "guide/start.md", source: "\ufeff# Start 日本 😀\r\n\r\n{{#include ../parts/shared.txt}}\r\n" },
  { path: "parts/shared.txt", source: "Original snippet\r\n", role: "include" },
  { path: "last.md", source: "# Last\n" },
], options: { title: "Saved book", author: "Ada", lang: "de", profile: "commonmark-gfm",
  page: { size: "a5" }, toc: true, baselineGridPt: 12 } });
export function selectedFile(path, bytes, folder = "Edited book 😀") {
  const file = new File([bytes], path.slice(path.lastIndexOf("/") + 1));
  Object.defineProperty(file, "webkitRelativePath", { value: `${folder}/${path}` });
  return file;
}
export async function exportedFolder(project = sample(), edits = {}) {
  const archive = await createBookSourceArchive(project, { projectCodec: codec });
  const entries = await readSourceZip(archive.blob);
  return Array.from(entries, ([path, bytes]) => selectedFile(path, Object.hasOwn(edits, path) ? edits[path] : bytes)).reverse();
}
export function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
