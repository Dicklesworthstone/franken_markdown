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
  function changes(base, next, trim = true) {
    let start = 0, end = 0;
    while (trim && start < base.length && start < next.length && base[start] === next[start]) start++;
    while (trim && end < base.length - start && end < next.length - start
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
  // Refine only one physical source line. Whitespace-delimited runs are
  // indivisible: never splice a word, combining sequence, emoji, identifier or
  // Markdown destination into a third spelling. CRLF is one exact token.
  // No locale/Intl segmentation, normalization or Markdown interpretation.
  function refineParagraph(base, local, disk) {
    const values = [base, local, disk];
    const cost = values.reduce((sum, text) => sum + text.length, 0);
    if (cost > cells) return null;
    cells -= cost; // Charge the scan before allocating any token arrays.
    const tokens = [];
    for (const text of values) {
      const pattern = /\r\n|[ \t\v\f]+|[^ \t\v\f\r\n]+|[\r\n]/g;
      const parts = [];
      let match;
      while ((match = pattern.exec(text))) {
        if (!cells) return null;
        cells--; parts.push(match[0]);
        if (/^[\r\n]/.test(match[0]) && pattern.lastIndex !== text.length) return null;
      }
      tokens.push(parts);
    }
    // A duplicate word/space can admit different alignments. Require the same
    // clean result with forward AND reverse diffs, including the cross pairs;
    // do not trim common ends here, which would conceal duplicate alignments.
    // ambiguity or exhausted work keeps the original whole-line conflict.
    const count = tokens.reduce((sum, parts) => sum + parts.length, 0);
    if (count > cells) return null;
    cells -= count;
    const repeated = new Set();
    for (const parts of tokens) {
      const seen = new Set();
      for (const part of parts) if (!/^[ \t\v\f\r\n]+$/.test(part)) {
        if (seen.has(part)) repeated.add(part);
        else seen.add(part);
      }
    }
    const reversed = tokens.map(parts => parts.slice().reverse());
    const a = changes(tokens[0], tokens[1], false), b = a && changes(tokens[0], tokens[2], false);
    const ar = b && changes(reversed[0], reversed[1], false), br = ar && changes(reversed[0], reversed[2], false);
    if (!br) return null;
    const forward = edits => edits.slice().reverse().map(edit => ({
      start: tokens[0].length - edit.end, end: tokens[0].length - edit.start,
      lines: edit.lines.slice().reverse(),
    }));
    const lefts = [a, forward(ar)], rights = [b, forward(br)];
    // Moving/adding/removing a repeated word has no stable occurrence identity.
    // Keep that line explicit even when the two extreme alignments happen to
    // agree. Repeated words that remain untouched are useful normal context.
    for (const edits of [...lefts, ...rights]) for (const edit of edits) {
      if (edit.lines.some(token => repeated.has(token))) return null;
      for (let i = edit.start; i < edit.end; i++) if (repeated.has(tokens[0][i])) return null;
    }
    let source = null;
    for (const left of lefts) for (const right of rights) {
      const merged = mergePlan(tokens[0], left, right);
      if (merged.action !== "merged" || (source !== null && source !== merged.source)) return null;
      source = merged.source;
    }
    return source;
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
    if (detailed) return mergePlan(old, a, b, refineParagraph);
    const edits = [];
    let i = 0, j = 0;
    while (i < a.length || j < b.length) {
      const x = a[i], y = b[j];
      if (x && y && overlaps(x, y)) {
        if (!identical(x, y)) {
          const planned = mergePlan(old, a, b, refineParagraph);
          return result(planned.action, planned.source, planned.reason);
        }
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
function mergePlan(base, local, disk, refine = null) {
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
      const original = slice(start, end, []);
      const merged = refine && end === start + 1 ? refine(original, left, right) : null;
      if (merged !== null) { pieces.push(merged); return true; }
      if (conflicts.length === 256) return false;
      const index = conflicts.length;
      conflicts.push(Object.freeze({ index, startLine: start, endLine: end,
        base: original, local: left, disk: right }));
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

// Source-project reconciliation adds membership, roles and reading order to the
// same exact-text merger. It carries no settings, resource bytes or authority.
export const BOOK_SOURCE_TREE_MERGE_LIMITS = Object.freeze({
  chapters: 128, includeSources: 128, fileBytes: MAX_UNITS,
  sourceBytes: 16 * 1024 * 1024, pathBytes: 1024,
});
const treeError = (code, message) => Object.assign(new Error(message), { code });
const invalidTree = () => treeError("INVALID_SOURCE_TREE_MERGE", "Use complete owned source trees and an explicit choice for every conflict.");
const treeLimit = () => treeError("BOOK_LIMIT", "The reconciled source tree exceeds its source, path or role limits.");
function treeOwn(value, key) {
  const field = value && Object.getOwnPropertyDescriptor(value, key);
  if (!field || !Object.hasOwn(field, "value")) throw invalidTree();
  return field.value;
}
function treeLimits(limits) {
  if (!limits || typeof limits !== "object" || Array.isArray(limits)) throw invalidTree();
  const cap = { ...BOOK_SOURCE_TREE_MERGE_LIMITS };
  for (const key of Reflect.ownKeys(limits)) {
    if (!Object.hasOwn(cap, key)) throw invalidTree();
    const value = treeOwn(limits, key);
    if (!Number.isSafeInteger(value) || value < 1 || value > cap[key]) throw invalidTree();
    cap[key] = value;
  }
  return cap;
}
function treeBytes(text, limit) {
  if (typeof text !== "string") throw invalidTree();
  if (text.length > limit) throw treeLimit();
  try { resolutionText(text); } catch { throw invalidTree(); }
  const bytes = new TextEncoder().encode(text).length;
  if (bytes > limit) throw treeLimit();
  return bytes;
}
const treeFile = (path, source, role) => Object.freeze({ path, source,
  ...(role === "include" ? { role } : {}) });
const treeRole = file => file?.role ?? "chapter";
const equalTreeFile = (a, b) => a === b || (!!a && !!b && a.path === b.path
  && a.source === b.source && treeRole(a) === treeRole(b));
const pathOrder = (a, b) => a < b ? -1 : a > b ? 1 : 0;

/** Own and bound a source-only merge baseline. Invalid editor drafts must be
 * repaired first; unlike undo, reconciliation must identify unique paths.
 * Paths and source are never Unicode-normalized or newline-normalized.
 */
export function captureBookSourceTreeForMerge(value, limits = BOOK_SOURCE_TREE_MERGE_LIMITS) {
  const cap = treeLimits(limits);
  if (!Array.isArray(value)) throw invalidTree();
  const count = treeOwn(value, "length");
  if (count > cap.chapters + cap.includeSources) throw treeLimit();
  const files = [], paths = new Set();
  let bytes = 0, chapters = 0, includes = 0;
  for (let i = 0; i < count; i++) {
    const file = treeOwn(value, String(i));
    if (!file || typeof file !== "object" || Array.isArray(file)
        || ![Object.prototype, null].includes(Object.getPrototypeOf(file))
        || Reflect.ownKeys(file).some(key => !["path", "source", "role"].includes(key))) throw invalidTree();
    const path = treeOwn(file, "path"), source = treeOwn(file, "source");
    const givenRole = Object.hasOwn(file, "role") ? treeOwn(file, "role") : undefined;
    const role = givenRole === undefined ? "chapter" : givenRole;
    if (typeof path !== "string" || !path || path !== path.trim()
        || /[\\\x00-\x1f\x7f<>:"|?*#%]/.test(path)
        || path.split("/").some(part => !part || part === "." || part === ".." || /[. ]$/.test(part))
        || !["chapter", "include"].includes(role)
        || (role === "chapter" && !/\.(md|markdown)$/i.test(path)) || paths.has(path)) throw invalidTree();
    paths.add(path);
    if (role === "include") includes++; else chapters++;
    if (chapters > cap.chapters || includes > cap.includeSources) throw treeLimit();
    bytes += treeBytes(path, cap.pathBytes) + treeBytes(source, cap.fileBytes);
    if (bytes > cap.sourceBytes) throw treeLimit();
    files.push(treeFile(path, source, role));
  }
  return Object.freeze(files);
}

const indexTree = files => new Map(files.map((file, index) => [file.path, index]));
const relativeOrder = (index, a, b) => index.has(a) && index.has(b)
  ? index.get(a) < index.get(b) ? -1 : 1 : 0;
function movedInTree(path, base, side) {
  for (const other of base.keys()) if (other !== path && side.has(other)
      && relativeOrder(base, path, other) !== relativeOrder(side, path, other)) return true;
  return false;
}
// Merge pairwise precedence, not absolute indices (insertions shift indices).
// A deterministic topological order retains independent reorders and insertion
// anchors. Concurrent incompatible precedence/cycles require an explicit order.
// There are at most 768 candidate paths, hence at most 294,528 comparisons/edges.
function mergedTreeOrder(paths, base, local, incoming) {
  const edges = paths.map(() => []), degree = new Uint16Array(paths.length);
  for (let i = 0; i < paths.length; i++) for (let j = i + 1; j < paths.length; j++) {
    const b = relativeOrder(base, paths[i], paths[j]);
    const l = relativeOrder(local, paths[i], paths[j]);
    const r = relativeOrder(incoming, paths[i], paths[j]);
    let direction;
    if (!l || !r) direction = l || r || b;
    else if (l === r || r === b) direction = l;
    else if (l === b) direction = r;
    else return null;
    if (!direction) continue;
    const from = direction < 0 ? i : j, to = direction < 0 ? j : i;
    edges[from].push(to); degree[to]++;
  }
  const order = [], used = new Uint8Array(paths.length);
  for (let n = 0; n < paths.length; n++) {
    let next = -1;
    for (let i = 0; i < paths.length; i++) if (!used[i] && !degree[i]) { next = i; break; }
    if (next < 0) return null;
    used[next] = 1; order.push(paths[next]);
    for (const to of edges[next]) degree[to]--;
  }
  return Object.freeze(order);
}

/** A plan is immutable and reusable. Approval is [{index, choice}], exactly one
 * per conflict. local/incoming select that side; source conflicts also accept
 * edited + source, or regions + choices understood by the existing text plan
 * (local/disk/edited). Renames are deletion/addition, never guessed identities.
 * Current settings and resources are outside this API and cannot be restored.
 */
export function planBookSourceTreeMerge(baseFiles, localFiles, incomingFiles,
  limits = BOOK_SOURCE_TREE_MERGE_LIMITS) {
  const cap = treeLimits(limits);
  const base = captureBookSourceTreeForMerge(baseFiles, cap);
  const local = captureBookSourceTreeForMerge(localFiles, cap);
  const incoming = captureBookSourceTreeForMerge(incomingFiles, cap);
  const trees = [base, local, incoming], maps = trees.map(files => new Map(files.map(file => [file.path, file])));
  const indices = trees.map(indexTree), paths = [...new Set(trees.flatMap(files => files.map(file => file.path)))].sort(pathOrder);
  const conflicts = [], changes = [], instructions = [], regionResolvers = new Map();
  const textMerger = createBookSourceMerger();
  let regions = 0;
  function requireChoice(kind, path, b, l, r, details = {}) {
    const index = conflicts.length;
    conflicts.push(Object.freeze({ index, kind, path, base: b, local: l, incoming: r, ...details }));
    return Object.freeze({ conflict: index });
  }
  for (const path of paths) {
    const [b, l, r] = maps.map(map => map.get(path) ?? null);
    const firstConflict = conflicts.length;
    let instruction, proposed;
    if (!l || !r) {
      const present = l ?? r;
      if (!b || !present) proposed = present;
      else if (equalTreeFile(b, present) && !movedInTree(path, indices[0], indices[l ? 1 : 2])) proposed = null;
      else instruction = { path, membership: requireChoice("membership", path, b, l, r,
        { reason: "delete-versus-edit" }) };
      instruction ??= { path, file: proposed };
    } else {
      const text = textMerger.plan(b?.source ?? null, l.source, r.source);
      let source = text.source, role = treeRole(l);
      if (text.action === "conflict") {
        const detailed = text.conflicts && regions + text.conflicts.length <= 256;
        source = requireChoice("source", path, b?.source ?? null, l.source, r.source,
          { reason: text.reason, hunks: detailed ? text.conflicts : null });
        if (detailed) { regions += text.conflicts.length; regionResolvers.set(source.conflict, text.resolve); }
      }
      if (treeRole(l) !== treeRole(r)) {
        if (b && treeRole(l) === treeRole(b)) role = treeRole(r);
        else if (!b || treeRole(r) !== treeRole(b))
          role = requireChoice("role", path, b ? treeRole(b) : null, treeRole(l), treeRole(r));
      }
      instruction = { path, source, role };
      if (conflicts.length === firstConflict) proposed = treeFile(path, source, role);
    }
    instructions.push(instruction);
    changes.push(Object.freeze({ path, base: b, local: l, incoming: r,
      action: conflicts.length !== firstConflict ? "resolve" : equalTreeFile(l, proposed) ? "unchanged"
        : !proposed ? "remove" : !l ? "add" : "update",
      proposed: proposed ?? null }));
  }
  const candidates = instructions.filter(item => !Object.hasOwn(item, "file") || item.file).map(item => item.path);
  const automaticOrder = mergedTreeOrder(candidates, ...indices);
  const orders = trees.map(files => Object.freeze(files.map(file => file.path)));
  const orderChoice = automaticOrder === null
    ? requireChoice("order", null, ...orders, { reason: "incompatible-order" }) : null;
  const order = Object.freeze({ base: orders[0], local: orders[1], incoming: orders[2], merged: automaticOrder });
  const frozen = Object.freeze(conflicts);
  function resolve(choices) {
    if (!Array.isArray(choices) || treeOwn(choices, "length") !== frozen.length) throw invalidTree();
    const selected = new Map();
    for (let i = 0; i < frozen.length; i++) {
      const item = treeOwn(choices, String(i)), index = treeOwn(item, "index"), choice = treeOwn(item, "choice");
      if (!Number.isInteger(index) || index < 0 || index >= frozen.length || selected.has(index)) throw invalidTree();
      const entry = frozen[index];
      if (choice === "local" || choice === "incoming") selected.set(index, entry[choice]);
      else if (choice === "edited" && entry.kind === "source") {
        const source = treeOwn(item, "source"); treeBytes(source, cap.fileBytes); selected.set(index, source);
      } else if (choice === "regions" && entry.kind === "source" && regionResolvers.has(index)) {
        let source;
        try { source = regionResolvers.get(index)(treeOwn(item, "regions")); } catch { throw invalidTree(); }
        treeBytes(source, cap.fileBytes); selected.set(index, source);
      } else throw invalidTree();
    }
    const value = field => field && typeof field === "object" && Object.hasOwn(field, "conflict")
      ? selected.get(field.conflict) : field;
    const files = new Map();
    for (const item of instructions) {
      const file = Object.hasOwn(item, "membership") ? value(item.membership)
        : Object.hasOwn(item, "file") ? item.file : treeFile(item.path, value(item.source), value(item.role));
      if (file) files.set(item.path, file);
    }
    // An explicit order uses that complete side first and appends surviving
    // other-side-only paths in their order. A choice cannot discard any source.
    const paths = automaticOrder ?? [...new Set([...value(orderChoice), ...orders[1], ...orders[2], ...orders[0]])];
    return captureBookSourceTreeForMerge(paths.filter(path => files.has(path)).map(path => files.get(path)), cap);
  }
  return Object.freeze({ changes: Object.freeze(changes), conflicts: frozen, order, resolve });
}
