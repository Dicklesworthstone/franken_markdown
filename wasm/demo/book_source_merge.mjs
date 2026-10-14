// Bounded, exact-text three-way reconciliation. This is deliberately not a
// Markdown parser: line terminators, BOMs and Unicode are compared verbatim.
export const BOOK_SOURCE_MERGE_LIMITS = Object.freeze({ cells: 2_000_000, lines: 100_000 });
const MAX_UNITS = 4 * 1024 * 1024;
const invalid = () => Object.assign(new Error("Use bounded source strings and merge limits."), { code: "INVALID_SOURCE_MERGE" });
const result = (action, source, reason = null) => Object.freeze({ action, source, reason });
const conflict = reason => result("conflict", null, reason);

/** One merger shares its work budget across ALL files in a review. Exceeding
 * it is a conflict for explicit whole-file resolution, never a guessed merge.
 * null means an absent disk file; it is not interchangeable with empty text.
 */
export function createBookSourceMerger(limits = BOOK_SOURCE_MERGE_LIMITS) {
  const cap = { ...BOOK_SOURCE_MERGE_LIMITS, ...limits };
  for (const name of Object.keys(BOOK_SOURCE_MERGE_LIMITS))
    if (!Number.isSafeInteger(cap[name]) || cap[name] < 1 || cap[name] > BOOK_SOURCE_MERGE_LIMITS[name]) throw invalid();
  let cells = cap.cells, remainingLines = cap.lines;
  function lines(source) {
    const out = [];
    let start = 0;
    for (let i = 0; i < source.length; i++) {
      const unit = source.charCodeAt(i);
      if (unit !== 10 && unit !== 13) continue;
      if (unit === 13 && source.charCodeAt(i + 1) === 10) i++;
      if (!remainingLines) return null;
      remainingLines--; out.push(source.slice(start, i + 1)); start = i + 1;
    }
    if (start < source.length) {
      if (!remainingLines) return null;
      remainingLines--; out.push(source.slice(start));
    }
    return out;
  }
  function changes(base, next) {
    let start = 0, end = 0;
    while (start < base.length && start < next.length && base[start] === next[start]) start++;
    while (end < base.length - start && end < next.length - start
        && base[base.length - 1 - end] === next[next.length - 1 - end]) end++;
    const n = base.length - start - end, m = next.length - start - end;
    if (!n && !m) return [];
    if (!n || !m) return [{ start, end: start + n, lines: next.slice(start, start + m) }];
    // Charge allocations, not just comparisons; a very narrow table is bounded.
    const size = (n + 1) * (m + 1);
    if (size > cells) return null;
    cells -= size;
    const row = m + 1, lcs = new Uint32Array(size);
    for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--)
      lcs[i * row + j] = base[start + i] === next[start + j]
        ? lcs[(i + 1) * row + j + 1] + 1
        : Math.max(lcs[(i + 1) * row + j], lcs[i * row + j + 1]);
    const out = [];
    let i = 0, j = 0, edit = null;
    while (i < n || j < m) {
      if (i < n && j < m && base[start + i] === next[start + j]) {
        if (edit) { out.push(edit); edit = null; }
        i++; j++; continue;
      }
      edit ??= { start: start + i, end: start + i, lines: [] };
      if (i < n && (j === m || lcs[(i + 1) * row + j] >= lcs[i * row + j + 1]))
        edit.end = start + ++i;
      else edit.lines.push(next[start + j++]);
    }
    if (edit) out.push(edit);
    return out;
  }
  const identical = (a, b) => a.start === b.start && a.end === b.end
    && a.lines.length === b.lines.length && a.lines.every((line, i) => line === b.lines[i]);
  function overlaps(a, b) {
    // An insertion touching an edited boundary is intentionally ambiguous.
    // Adjacent nonempty replacements, in contrast, affect different lines.
    if (a.start === a.end) return a.start >= b.start && a.start <= b.end;
    if (b.start === b.end) return b.start >= a.start && b.start <= a.end;
    return a.start < b.end && b.start < a.end;
  }
  return (base, local, disk) => {
    if (typeof local !== "string" || ![base, disk].every(text => text === null || typeof text === "string")
        || [base, local, disk].some(text => text !== null && text.length > MAX_UNITS)) throw invalid();
    if (local === disk) return result("unchanged", local);
    if (base === disk) return result("local", local);
    if (disk === null) return conflict("deleted-on-disk");
    if (base === null) return conflict("created-on-disk");
    if (local === base) return result("disk", disk);
    const old = lines(base), left = old && lines(local), right = left && lines(disk);
    if (!right) return conflict("merge-budget");
    const a = changes(old, left), b = a && changes(old, right);
    if (!b) return conflict("merge-budget");
    const edits = [];
    let i = 0, j = 0;
    while (i < a.length || j < b.length) {
      const x = a[i], y = b[j];
      if (x && y && overlaps(x, y)) {
        if (!identical(x, y)) return conflict("overlapping-edits");
        edits.push(x); i++; j++;
      } else if (x && (!y || x.start < y.start)) { edits.push(x); i++; }
      else { edits.push(y); j++; }
    }
    const output = [];
    let cursor = 0, units = 0;
    const append = line => { units += line.length; output.push(line); };
    for (const edit of edits) {
      for (; cursor < edit.start; cursor++) append(old[cursor]);
      for (const line of edit.lines) append(line);
      cursor = edit.end;
    }
    for (; cursor < old.length; cursor++) append(old[cursor]);
    if (units > MAX_UNITS) return conflict("merge-size");
    return result("merged", output.join(""));
  };
}
