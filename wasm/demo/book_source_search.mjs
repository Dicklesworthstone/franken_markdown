// Literal source operations only. Markdown syntax and rendering remain Rust-owned.
import { bookTextBytes } from "../book_session.mjs";
import { bookError } from "../book_worker.mjs";
import { normalizeBookProject, BOOK_WORKBENCH_LIMITS as B } from "./book_collection.mjs";
export const BOOK_SEARCH_LIMITS = Object.freeze({ queryBytes: 4096, replacementBytes: 65536, matches: 10000 });
const L = BOOK_SEARCH_LIMITS, searches = new WeakMap();
const lines = text => text.replace(/\r\n?/g, "\n");
const literal = text => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const freezeFiles = files => Object.freeze(files.map(file => Object.freeze({ ...file })));

/** Search a validated immutable source snapshot in reading order. Match offsets
 * are UTF-16 positions in ORIGINAL source; line/column use Unicode code points.
 * Query newlines match LF, CRLF or CR. Case-insensitive mode uses ECMAScript's
 * Unicode simple case folding (not locale-sensitive linguistic equivalence).
 * An over-limit search throws, never presents a partial set as "all" matches.
 */
export function findBookSource(project, query, { matchCase = true, chapter = null } = {}) {
  bookTextBytes(query, L.queryBytes);
  if (!query) throw bookError("EMPTY_QUERY", "Enter literal text to find.");
  if (typeof matchCase !== "boolean" || (chapter !== null && !Number.isInteger(chapter))) {
    throw bookError("INVALID_SEARCH", "Invalid case or chapter search option.");
  }
  const value = normalizeBookProject(project);
  if (chapter !== null && (chapter < 0 || chapter >= value.files.length)) {
    throw bookError("INVALID_SELECTION", "Choose an existing chapter to search.");
  }
  const pattern = new RegExp(lines(query).split("\n").map(literal).join("(?:\\r\\n|\\r(?!\\n)|\\n)"), matchCase ? "gu" : "giu");
  const matches = [];
  for (const [index, file] of value.files.entries()) {
    if (chapter !== null && chapter !== index) continue;
    let at = 0, line = 1, column = 1; pattern.lastIndex = 0;
    for (let found; (found = pattern.exec(file.source)) !== null;) {
      if (matches.length === L.matches) throw bookError("SEARCH_LIMIT", "More than 10,000 matches; narrow the text or search one chapter. Nothing was changed.");
      while (at < found.index) {
        const point = file.source.codePointAt(at);
        if (point === 13) { at += file.source[at + 1] === "\n" ? 2 : 1; line++; column = 1; }
        else if (point === 10) { at++; line++; column = 1; }
        else { at += point > 0xffff ? 2 : 1; column++; }
      }
      matches.push(Object.freeze({ chapter: index, start: found.index, end: pattern.lastIndex, line, column }));
    }
  }
  const result = Object.freeze({ query, matchCase, chapter, files: freezeFiles(value.files), matches: Object.freeze(matches) });
  searches.set(result, value); return result;
}

/** Build a complete source transaction before returning it. Replacements are
 * literal, including $&, $1 and backslashes; no regex/capture substitution.
 * New replacement lines use each chapter's first existing newline convention.
 * All unmatched bytes, including BOMs and mixed line endings, are untouched.
 * Omit matchIndex to replace the entire (possibly chapter-scoped) search set.
 */
export function planBookReplacement(search, replacement, matchIndex = null) {
  const original = searches.get(search);
  if (!original) throw bookError("INVALID_SEARCH", "Run a source search before preparing a replacement.");
  bookTextBytes(replacement, L.replacementBytes);
  if (matchIndex !== null && (!Number.isInteger(matchIndex) || matchIndex < 0 || matchIndex >= search.matches.length)) {
    throw bookError("INVALID_SELECTION", "Choose a search result to replace.");
  }
  const selected = matchIndex === null ? search.matches : [search.matches[matchIndex]];
  const groups = new Map();
  for (const match of selected) {
    if (!groups.has(match.chapter)) groups.set(match.chapter, []);
    groups.get(match.chapter).push(match);
  }
  let beforeBytes = 0, afterBytes = 0, total = 0, count = 0;
  const admitted = [];
  // Admit the entire output before constructing any replacement chapter.
  for (const [index, file] of original.files.entries()) {
    const all = groups.get(index) ?? [], newline = file.source.match(/\r\n|\r|\n/)?.[0] ?? "\n";
    const insert = lines(replacement).replace(/\n/g, () => newline);
    const insertBytes = bookTextBytes(insert, L.replacementBytes * 2), oldBytes = bookTextBytes(file.source, B.chapterBytes);
    // Matching equal text is a no-op, not an undo entry or a source revision.
    const edits = all.filter(match => file.source.slice(match.start, match.end) !== insert);
    let newBytes = oldBytes;
    for (const match of edits) newBytes += insertBytes - bookTextBytes(file.source.slice(match.start, match.end), B.chapterBytes);
    if (newBytes > B.chapterBytes) throw bookError("BOOK_LIMIT", "Replacement would exceed the 4 MiB chapter limit. Nothing was changed.");
    total += newBytes + bookTextBytes(file.path, 1024);
    if (total > B.sourceBytes) throw bookError("BOOK_LIMIT", "Replacement would exceed the 16 MiB book limit. Nothing was changed.");
    beforeBytes += oldBytes; afterBytes += newBytes; count += edits.length;
    admitted.push({ file, index, edits, insert, oldBytes, newBytes });
  }
  if (!count) throw bookError("NO_CHANGE", "The replacement would not change any source.");
  const chapters = [];
  const files = admitted.map(({ file, index, edits, insert, oldBytes, newBytes }) => {
    if (!edits.length) return file;
    const parts = []; let at = 0;
    for (const match of edits) { parts.push(file.source.slice(at, match.start), insert); at = match.end; }
    parts.push(file.source.slice(at));
    chapters.push(Object.freeze({ index, path: file.path, count: edits.length, beforeBytes: oldBytes, afterBytes: newBytes }));
    return { path: file.path, source: parts.join("") };
  });
  return Object.freeze({ count, beforeBytes, afterBytes, chapters: Object.freeze(chapters),
    before: search.files, after: freezeFiles(files) });
}

/** Convert an original-source offset to a textarea's LF-normalized offset. */
export function bookEditorOffset(source, offset) {
  if (typeof source !== "string" || !Number.isInteger(offset) || offset < 0 || offset > source.length
      || (offset > 0 && offset < source.length && /[\udc00-\udfff]/.test(source[offset]) && /[\ud800-\udbff]/.test(source[offset - 1]))) {
    throw bookError("INVALID_SELECTION", "Invalid source selection offset.");
  }
  let result = offset;
  for (let i = 0; i < offset; i++) if (source[i] === "\r" && source[i + 1] === "\n" && i + 1 < offset) { result--; i++; }
  return result;
}
