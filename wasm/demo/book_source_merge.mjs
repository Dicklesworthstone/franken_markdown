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
    if (size > cells) return sparseChanges(base, next, start, n, m);
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
  // A distant pair of tiny edits need not allocate the entire rectangle
  // between them. Explore shortest edit paths by distance instead. Keep the
  // historical LCS/tie policy above whenever that matrix fits. Both paths
  // share one allowance: charge every retained frontier cell and every line
  // comparison BEFORE work. Dense edits still refuse instead of guessing.
  function sparseChanges(base, next, start, n, m) {
    const trace = [];
    function step(previous, d, i) {
      const k = 2 * i - d;
      const across = i ? previous[i - 1] : -1;
      const down = i < d ? previous[i] : -1;
      const canAcross = across >= 0 && across < n && across + 1 - k >= 0 && across + 1 - k <= m;
      const canDown = down >= 0 && down <= n && down - k > 0 && down - k <= m;
      if (!canAcross && !canDown) return -1;
      return canDown && (!canAcross || across < down) ? 1 : 0;
    }
    for (let d = 0; d <= n + m; d++) {
      if (d + 1 > cells) { cells = 0; return null; }
      cells -= d + 1;
      const frontier = new Int32Array(d + 1).fill(-1);
      const previous = trace.at(-1);
      trace.push(frontier);
      for (let i = 0; i <= d; i++) {
        const down = d ? step(previous, d, i) : 0;
        if (down < 0) continue;
        let x = d ? down ? previous[i] : previous[i - 1] + 1 : 0;
        let y = x - (2 * i - d);
        while (x < n && y < m) {
          if (!cells) return null;
          cells--;
          if (base[start + x] !== next[start + y]) break;
          x++; y++;
        }
        frontier[i] = x;
        if (x !== n || y !== m) continue;
        // Backtrack only the edit steps; equal runs need no operation objects.
        // Each predecessor comes from a fully explored earlier frontier.
        const reversed = [];
        for (let depth = d; depth > 0; depth--) {
          const index = (x - y + depth) / 2, prior = trace[depth - 1];
          const insert = step(prior, depth, index);
          const px = insert ? prior[index] : prior[index - 1];
          const py = px - (2 * index - depth + (insert ? 1 : -1));
          reversed.push({ start: start + px, end: start + px + (insert ? 0 : 1),
            lines: insert ? [next[start + py]] : [] });
          x = px; y = py;
        }
        const edits = [];
        for (let j = reversed.length - 1; j >= 0; j--) {
          const edit = reversed[j], last = edits.at(-1);
          if (last && last.end === edit.start) {
            last.end = edit.end;
            for (const line of edit.lines) last.lines.push(line);
          } else edits.push(edit);
        }
        return edits;
      }
    }
    return null;
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
  function reconcile(base, local, disk, detailed) {
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
    if (detailed) return mergePlan(old, a, b);
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
  }
  // Ordinary callers retain the original three-field result contract. Detailed
  // planning uses the very same diff and shared allowance, never a second grammar.
  return Object.freeze(Object.assign((base, local, disk) => reconcile(base, local, disk, false), {
    plan: (base, local, disk) => reconcile(base, local, disk, true),
  }));
}

const owned = (value, key) => {
  const field = value && Object.getOwnPropertyDescriptor(value, key);
  if (!field || !Object.hasOwn(field, "value")) throw invalid();
  return field.value;
};
function resolutionText(text) {
  if (typeof text !== "string" || text.length > MAX_UNITS) throw invalid();
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw invalid();
    } else if (unit >= 0xdc00 && unit <= 0xdfff) throw invalid();
  }
  return text;
}

// Connected overlap components, not the first conflicting pair: one deletion
// may intersect several edits from the other side. An insertion on a boundary
// connects both neighboring ranges; adjacent replacements remain independent.
function mergePlan(base, local, disk) {
  const pieces = [], conflicts = [];
  let i = 0, j = 0, cursor = 0, group = null;
  function slice(start, end, edits) {
    const text = [];
    let at = start;
    for (const edit of edits) {
      for (; at < edit.start; at++) text.push(base[at]);
      for (const line of edit.lines) text.push(line);
      at = edit.end;
    }
    for (; at < end; at++) text.push(base[at]);
    return text.join("");
  }
  function finish() {
    if (!group) return true;
    const { start, end, sides } = group;
    pieces.push(slice(cursor, start, [])); cursor = end;
    const left = slice(start, end, sides[0]), right = slice(start, end, sides[1]);
    if (!sides[0].length) pieces.push(right);
    else if (!sides[1].length || left === right) pieces.push(left);
    else {
      if (conflicts.length === 256) return false;
      const index = conflicts.length;
      conflicts.push(Object.freeze({ index, startLine: start, endLine: end,
        base: slice(start, end, []), local: left, disk: right }));
      pieces.push(index);
    }
    return true;
  }
  while (i < local.length || j < disk.length) {
    const side = i < local.length && (j === disk.length || local[i].start <= disk[j].start) ? 0 : 1;
    const edit = side ? disk[j++] : local[i++], point = edit.start === edit.end;
    if (!group || edit.start > group.end || (edit.start === group.end && !point && !group.point)) {
      if (!finish()) return conflict("merge-hunks");
      group = { start: edit.start, end: edit.end, point, sides: [[], []] };
    } else if (edit.end > group.end) { group.end = edit.end; group.point = point; }
    else if (edit.end === group.end && point) group.point = true;
    group.sides[side].push(edit);
  }
  if (!finish()) return conflict("merge-hunks");
  pieces.push(slice(cursor, base.length, []));
  const frozen = Object.freeze(conflicts);
  function resolve(choices) {
    if (!Array.isArray(choices) || owned(choices, "length") !== frozen.length) throw invalid();
    const chosen = new Map();
    for (let at = 0; at < frozen.length; at++) {
      const item = owned(choices, String(at)), index = owned(item, "index"), choice = owned(item, "choice");
      if (!Number.isInteger(index) || index < 0 || index >= frozen.length || chosen.has(index)
          || !["local", "disk", "edited"].includes(choice)) throw invalid();
      chosen.set(index, choice === "edited" ? resolutionText(owned(item, "source")) : frozen[index][choice]);
    }
    let units = 0;
    const text = pieces.map(piece => {
      const value = typeof piece === "string" ? piece : chosen.get(piece);
      units += value.length;
      if (units > MAX_UNITS) throw invalid();
      return value;
    });
    return text.join("");
  }
  if (!conflicts.length) {
    try { return result("merged", resolve([])); }
    catch { return conflict("merge-size"); }
  }
  return Object.freeze({ ...conflict("overlapping-edits"), conflicts: frozen, resolve });
}
