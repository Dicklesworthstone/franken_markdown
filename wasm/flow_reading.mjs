// Semantic data from the existing engine, never a second Markdown parser.
export class FlowReadingError extends Error {
  constructor(code, message) { super(message); this.name = "FlowReadingError"; this.code = code; }
}
const fail = (code, message) => { throw new FlowReadingError(code, message); };
const invalid = message => fail("INVALID_READING_DATA", message);
const DEFAULTS = Object.freeze({ maxNodes: 10000, maxDepth: 64, maxTextUnits: 1048576, maxInlineRuns: 50000, maxLinkUnits: 1048576, maxListEntries: 50000, maxAnchorUnits: 1048576 });
const ROLES = new Set(["document", "heading", "paragraph", "code-block", "list", "list-item", "table",
  "table-header-row", "table-row", "table-header-cell", "table-cell", "blockquote", "thematic-break", "image"]);
const ROWS = new Set(["table-header-row", "table-row"]);
const CELLS = new Set(["table-header-cell", "table-cell"]);
const LEAVES = new Set(["heading", "paragraph", "code-block", "thematic-break", "image"]);
const owned = new WeakSet(), sessions = new WeakMap();
function record(value, keys) {
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).some(k => !keys.includes(k))) {
    fail("INVALID_OPTIONS", "unsupported reading options");
  }
}
function identity(value) {
  if (typeof value === "bigint") value = String(value);
  if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(value) || BigInt(value) > 18446744073709551615n) {
    invalid("expected a lossless u64 identity");
  }
  return value;
}
function token(value) {
  return Object.freeze({ revision: identity(value?.revision), layoutRevision: identity(value?.layoutRevision) });
}
function equal(a, b) { return a.revision === b.revision && a.layoutRevision === b.layoutRevision; }
function check(session, expected) {
  if (session.disposed) fail("SESSION_DISPOSED", "reading session is disposed");
  const now = token(session.token);
  if (now.revision !== expected.revision) fail("STALE_REVISION", "reading text belongs to an older source");
  if (now.layoutRevision !== expected.layoutRevision) fail("STALE_LAYOUT", "reading geometry belongs to an older layout");
}
function integer(value, max = 0xffffffff) { return Number.isSafeInteger(value) && value >= 0 && value <= max; }
function wellFormed(text) {
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const low = text.charCodeAt(++i);
      if (!(low >= 0xdc00 && low <= 0xdfff)) return false;
    } else if (c >= 0xdc00 && c <= 0xdfff) return false;
  }
  return true;
}
function boundary(text, index) {
  const c = text.charCodeAt(index);
  return !(c >= 0xdc00 && c <= 0xdfff);
}
function rectangle(value) {
  if (!value || !["x", "y", "width", "height"].every(k => typeof value[k] === "number" && Number.isFinite(value[k])
      && Math.abs(value[k]) <= 1000000000) || value.width < 0 || value.height < 0) invalid("invalid reading bounds");
  return Object.freeze({ x: value.x, y: value.y, width: value.width, height: value.height });
}
function span(value) {
  if (!integer(value?.startByte) || !integer(value?.endByte) || value.startByte > value.endByte) invalid("invalid enclosing source span");
  return Object.freeze({ startByte: value.startByte, endByte: value.endByte });
}
function limits(value = {}) {
  record(value, Object.keys(DEFAULTS));
  const result = { ...DEFAULTS };
  for (const key of Object.keys(value)) {
    if (!integer(value[key], DEFAULTS[key]) || value[key] < 1) fail("INVALID_OPTIONS", "reading limits may only lower positive defaults");
    result[key] = value[key];
  }
  return result;
}
function cancelled(signal) { if (signal?.aborted) fail("ABORTED", "reading was aborted"); }
// Only cancel the wait. Never forward AbortSignal to a worker RPC, which could
// terminate the editing session. Every late resolution/rejection remains observed.
function wait(promise, signal) {
  if (!signal) return Promise.resolve(promise);
  return new Promise((resolve, reject) => {
    const abort = () => { cleanup(); reject(new FlowReadingError("ABORTED", "reading was aborted")); };
    const cleanup = () => signal.removeEventListener("abort", abort);
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(promise).then(value => { cleanup(); resolve(value); }, error => { cleanup(); reject(error); });
    if (signal.aborted) abort();
  });
}

// Mirrors the core's conservative activation filter. A target is still data;
// passing this filter never authorizes the host to navigate or fetch anything.
function activeTarget(target) {
  let start = 0, end = target.length;
  const whitespace = /\p{White_Space}/u;
  while (start < end && whitespace.test(target[start])) start++;
  while (end > start && whitespace.test(target[end - 1])) end--;
  const value = target.slice(start, end);
  if (!value || /[\u0000-\u001f\u007f-\u009f\\]/u.test(value)) return null;
  const prefix = value.split(/[/?#]/u, 1)[0], colon = prefix.indexOf(":");
  if (colon !== -1 && !["http", "https", "mailto"].includes(prefix.slice(0, colon).toLowerCase())) return null;
  return value;
}
function inlineMetadata(raw, budget, retained) {
  const input = raw.inlineRuns === undefined ? [] : raw.inlineRuns;
  if (!Array.isArray(input)) invalid("inline runs must be an array");
  retained.runs += input.length;
  if (retained.runs > budget.maxInlineRuns) fail("READING_LIMIT", "reading inline-run limit exceeded");
  if (input.length && (raw.children.length || ["image", "thematic-break", "code-block"].includes(raw.role))) {
    invalid("inline ranges require a semantic text leaf");
  }
  function link(value) {
    if (value === null) return null;
    if (!value || typeof value !== "object" || Array.isArray(value) || typeof value.target !== "string"
        || !(value.activeTarget === null || typeof value.activeTarget === "string")) invalid("invalid reading link");
    // Charge both strings, even if the JS engine shares their storage.
    retained.links += value.target.length + (value.activeTarget?.length ?? 0);
    if (retained.links > budget.maxLinkUnits) fail("READING_LIMIT", "reading link retention limit exceeded");
    if (!wellFormed(value.target) || (value.activeTarget !== null
        && (!wellFormed(value.activeTarget) || activeTarget(value.target) !== value.activeTarget))) {
      invalid("inconsistent or unsafe reading link activation");
    }
    return Object.freeze({ target: value.target, activeTarget: value.activeTarget });
  }
  // Advance monotonically across reading text: O(text + runs), without a
  // byte-sized lookup table or rescanning a prefix for every styled fragment.
  let byte = 0, utf16 = 0, previousEnd = 0;
  function position(offset) {
    while (byte < offset && utf16 < raw.text.length) {
      const cp = raw.text.codePointAt(utf16);
      byte += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      utf16 += cp > 65535 ? 2 : 1;
    }
    if (byte !== offset) invalid("inline range splits UTF-8 or exceeds reading text");
    return utf16;
  }
  const inlineRuns = Array.from(input, run => {
    if (!run || !integer(run.startByte) || !integer(run.endByte)
        || run.startByte < previousEnd || run.endByte <= run.startByte) invalid("unordered or empty inline range");
    previousEnd = run.endByte;
    const style = run.style, keys = ["bold", "italic", "code", "strikethrough"];
    if (!style || typeof style !== "object" || Array.isArray(style) || Object.keys(style).length !== keys.length
        || !keys.every(key => Object.hasOwn(style, key) && typeof style[key] === "boolean")) invalid("invalid reading inline style");
    const startUtf16 = position(run.startByte), endUtf16 = position(run.endByte);
    return Object.freeze({ startByte: run.startByte, endByte: run.endByte, startUtf16, endUtf16,
      style: Object.freeze({ bold: style.bold, italic: style.italic, code: style.code, strikethrough: style.strikethrough }),
      link: link(run.link) });
  });
  const imageLink = raw.imageLink === undefined ? null : link(raw.imageLink);
  if (imageLink !== null && raw.role !== "image") invalid("image link requires an image node");
  return { inlineRuns: Object.freeze(inlineRuns), imageLink };
}

// Paths are supplied by the native AST projection, never inferred from source
// envelopes, list-looking text, role alone, or Canvas indentation.
function listMetadata(raw, depth, budget, retained) {
  if (raw.listPath === undefined) return null; // Legacy schema-1 producer.
  const path = raw.listPath;
  if (!Array.isArray(path)) invalid("list ancestry must be an array");
  if (depth !== 0 && path.length) invalid("nested reading children inherit their root's list ownership");
  retained.listEntries += path.length;
  if (path.length > budget.maxDepth || retained.listEntries > budget.maxListEntries) {
    fail("READING_LIMIT", "reading list ancestry limit exceeded");
  }
  return Object.freeze(Array.from(path, item => {
    const keys = ["listId", "ordered", "start", "itemIndex", "task"];
    if (!item || typeof item !== "object" || Array.isArray(item)
        || Object.keys(item).length !== keys.length || !keys.every(key => Object.hasOwn(item, key))
        || typeof item.listId !== "string" || typeof item.start !== "string"
        || typeof item.ordered !== "boolean" || !integer(item.itemIndex)
        || !(item.task === null || typeof item.task === "boolean")) invalid("invalid list item identity");
    const listId = identity(item.listId), start = identity(item.start);
    if (listId === "0" || (item.ordered && BigInt(start) + BigInt(item.itemIndex) > 18446744073709551615n)) {
      invalid("invalid list identity or overflowing ordinal");
    }
    return Object.freeze({ listId, ordered: item.ordered, start, itemIndex: item.itemIndex, task: item.task });
  }));
}

function validateListOrder(roots) {
  let structured = null, previous = [];
  const seen = new Set();
  const sameList = (a, b) => a.listId === b.listId && a.ordered === b.ordered && a.start === b.start;
  for (const node of roots) {
    const hasPath = node.listPath !== null;
    if (structured === null) structured = hasPath;
    if (structured !== hasPath) invalid("mixed legacy and structured list ownership");
    if (!hasPath) continue;
    const path = node.listPath;
    let common = 0;
    while (common < path.length && common < previous.length
        && path[common].listId === previous[common].listId && path[common].itemIndex === previous[common].itemIndex) {
      if (!sameList(path[common], previous[common]) || path[common].task !== previous[common].task) {
        invalid("list item metadata changed within one snapshot");
      }
      common++;
    }
    if (common < path.length) {
      // Every item has one leading ListItem block, even for an empty/code-first
      // item. A continuation cannot silently introduce a missing ancestor.
      if (path.length !== common + 1 || node.role !== "list-item" || node.children.length) {
        invalid("list item is missing its leading reading block");
      }
      const item = path[common], before = previous[common];
      if (before?.listId === item.listId) {
        if (!sameList(item, before) || item.itemIndex !== before.itemIndex + 1) {
          invalid("list items are not consecutive within one list");
        }
      } else {
        if (seen.has(item.listId) || item.itemIndex !== 0) invalid("closed or foreign list identity was reused");
        seen.add(item.listId);
      }
    } else if (node.role === "list-item") {
      invalid("duplicate or ownerless list-item reading block");
    }
    previous = path;
  }
}

/** Immutable, session-bound semantic snapshot. Construct with readFlowDocument. */
export class FlowReadingDocument {
  #session; #matches = new WeakSet(); #anchors;
  constructor(secret, session, expected, roots, nodes, textUnits, retained, anchors) {
    if (secret !== owned) fail("INVALID_ARGUMENT", "use readFlowDocument to obtain a reading document");
    this.#session = session; this.#anchors = anchors;
    this.token = expected; this.roots = Object.freeze(roots); this.nodes = Object.freeze(nodes);
    this.headings = Object.freeze(nodes.filter(node => node.role === "heading"));
    // Container transcripts summarize children. Search/copy leaves once, not
    // both a table row's aggregate "A | B" and its individually typed cells.
    this.text = nodes.filter(node => !node.children.length && node.text).map(node => node.text).join("\n\n");
    this.textUnits = textUnits; this.inlineRunCount = retained.runs; this.linkUnits = retained.links; this.listEntryCount = retained.listEntries;
    this.anchorUnits = retained.anchors;
    owned.add(this); sessions.set(this, session); Object.freeze(this);
  }
  assertCurrent() { check(this.#session, this.token); }
  locate(index) {
    this.assertCurrent();
    if (!integer(index, this.nodes.length - 1)) fail("INVALID_ARGUMENT", "unknown reading node");
    const node = this.nodes[index];
    return Object.freeze({ ...this.token, nodeIndex: index, bounds: node.bounds, enclosingSourceSpan: node.enclosingSourceSpan });
  }
  /** Resolve an exact local fragment against engine-supplied destinations.
   * No slug guessing, document reload, location.hash writes, or external I/O. */
  locateFragment(target) {
    this.assertCurrent();
    if (typeof target !== "string" || target.length > 4096 || !wellFormed(target)) {
      fail("INVALID_ARGUMENT", "invalid bounded fragment target");
    }
    if (!target.startsWith("#") || target.length === 1) return null;
    let id;
    try { id = decodeURIComponent(target.slice(1)); } catch { return null; }
    // Percent decode exactly once. '+' is literal, and matching is case-sensitive.
    const index = this.#anchors.get(id);
    return index === undefined ? null : this.locate(index);
  }
  /** Literal matches in unsplit logical leaf text. Never fragment/source offsets. */
  find(query, options = {}) {
    this.assertCurrent(); record(options, ["asciiCaseInsensitive", "maxMatches"]);
    const max = options.maxMatches ?? 1000, insensitive = options.asciiCaseInsensitive ?? false;
    if (!integer(max, 1000) || max < 1 || typeof insensitive !== "boolean" || typeof query !== "string"
        || query.length > 1024 || !wellFormed(query)) fail("INVALID_ARGUMENT", "invalid literal reading search");
    const fold = text => insensitive ? text.replace(/[A-Z]/g, c => c.toLowerCase()) : text;
    const needle = fold(query), matches = [];
    if (needle) for (const node of this.nodes) {
      if (node.children.length || !node.text) continue;
      const text = fold(node.text); let from = 0;
      while (from <= text.length - needle.length) {
        const start = text.indexOf(needle, from);
        if (start < 0) break;
        const end = start + needle.length; from = end;
        if (!boundary(node.text, start) || !boundary(node.text, end)) continue;
        if (matches.length === max) return Object.freeze({ matches: Object.freeze(matches), truncated: true });
        const match = Object.freeze({ ...this.token, nodeIndex: node.index, startUtf16: start, endUtf16: end });
        this.#matches.add(match); matches.push(match);
      }
    }
    return Object.freeze({ matches: Object.freeze(matches), truncated: false });
  }
  matchText(match) {
    this.assertCurrent();
    if (!this.#matches.has(match)) fail("INVALID_ARGUMENT", "match does not belong to this reading snapshot");
    return this.nodes[match.nodeIndex].text.slice(match.startUtf16, match.endUtf16);
  }
}
export function requireReadingDocument(value, session) {
  if (!owned.has(value)) fail("INVALID_ARGUMENT", "an admitted reading document is required");
  if (session !== undefined && sessions.get(value) !== session) fail("INVALID_ARGUMENT", "reading document belongs to another session");
  value.assertCurrent(); return value;
}

export async function readFlowDocument(session, options = {}) {
  record(options, ["token", "signal", "limits"]);
  if (!session || typeof session.readingOrder !== "function") fail("INVALID_ARGUMENT", "a flow session is required");
  const budget = limits(options.limits), expected = token(options.token ?? session.token), signal = options.signal;
  if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function"
      || typeof signal.removeEventListener !== "function")) fail("INVALID_OPTIONS", "signal must be an AbortSignal");
  const roots = [], nodes = [], seen = new WeakSet(), retained = { runs: 0, links: 0, listEntries: 0, anchors: 0 }; let textUnits = 0;
  const anchors = new Map();
  function anchorMetadata(raw, index) {
    if (raw.anchorId === undefined || raw.anchorId === null) return null; // Legacy producer.
    const id = raw.anchorId;
    if (raw.role !== "heading" || typeof id !== "string" || !id.length) invalid("invalid heading destination");
    retained.anchors += id.length;
    if (retained.anchors > budget.maxAnchorUnits) fail("READING_LIMIT", "reading anchor retention limit exceeded");
    if (!wellFormed(id) || /[\u0000-\u001f\u007f-\u009f]/u.test(id) || anchors.has(id)) {
      invalid("invalid or duplicate heading destination");
    }
    anchors.set(id, index);
    return id;
  }
  function clone(raw, depth, parent = null) {
    if (!raw || typeof raw !== "object" || seen.has(raw)) invalid("cyclic or aliased reading tree");
    if (depth > budget.maxDepth || nodes.length >= budget.maxNodes) fail("READING_LIMIT", "reading node/depth limit exceeded");
    seen.add(raw);
    if (!ROLES.has(raw.role) || typeof raw.text !== "string" || !Array.isArray(raw.children)) invalid("invalid reading node");
    textUnits += raw.text.length;
    if (textUnits > budget.maxTextUnits || raw.children.length > budget.maxNodes - nodes.length) fail("READING_LIMIT", "reading retention limit exceeded");
    if (!wellFormed(raw.text)) invalid("reading text contains an unpaired surrogate");
    if ((raw.role === "heading" && (!integer(raw.level, 6) || raw.level < 1))
        || (LEAVES.has(raw.role) && raw.children.length)
        || (raw.role === "thematic-break" && raw.text !== "")
        || (CELLS.has(raw.role) && !ROWS.has(parent))
        || (parent === "table" && !ROWS.has(raw.role))
        || (ROWS.has(parent) && !CELLS.has(raw.role))
        || (parent === "list" && raw.role !== "list-item")) invalid("inconsistent reading structure");
    const node = { index: nodes.length, role: raw.role, text: raw.text, bounds: rectangle(raw.bounds),
      anchorId: anchorMetadata(raw, nodes.length),
      ...inlineMetadata(raw, budget, retained), listPath: listMetadata(raw, depth, budget, retained),
      enclosingSourceSpan: span(raw.enclosingSourceSpan), ...(raw.role === "heading" ? { level: raw.level } : {}), children: [] };
    nodes.push(node);
    for (const child of raw.children) node.children.push(clone(child, depth + 1, raw.role));
    Object.freeze(node.children); return Object.freeze(node);
  }
  let offset = 0, total = null;
  do {
    cancelled(signal); check(session, expected);
    const page = await wait(session.readingOrder({ offset, limit: 256, token: expected }), signal);
    cancelled(signal); check(session, expected);
    if (page?.schemaVersion !== 1 || typeof page.revision !== "string" || typeof page.layoutRevision !== "string" || !equal(token(page), expected) || page.offset !== offset || !integer(page.total)
        || !Array.isArray(page.nodes) || page.nodes.length !== Math.min(256, page.total - offset)
        || (total !== null && total !== page.total)) invalid("inconsistent reading page");
    if (page.total > budget.maxNodes) fail("READING_LIMIT", "too many reading roots");
    const end = offset + page.nodes.length;
    if (page.nextOffset !== (end < page.total ? end : null)) invalid("reading pagination did not advance");
    total = page.total;
    for (const raw of page.nodes) roots.push(clone(raw, 0));
    offset = page.nextOffset;
  } while (offset !== null);
  cancelled(signal); check(session, expected);
  validateListOrder(roots);
  return new FlowReadingDocument(owned, session, expected, roots, nodes, textUnits, retained, anchors);
}

/** Convert an ENCLOSING original Markdown UTF-8 span for textarea navigation.
 * The host must first fence the source revision. No exact inline map is implied. */
export function sourceSpanToUtf16(source, range) {
  const bytes = span(range);
  if (typeof source !== "string" || source.length > 4 * 1024 * 1024 || !wellFormed(source)) fail("INVALID_ARGUMENT", "invalid bounded Markdown source");
  let offset = 0, start = null, end = null;
  for (let i = 0; i <= source.length;) {
    if (offset === bytes.startByte) start = i;
    if (offset === bytes.endByte) end = i;
    if (i === source.length) break;
    const cp = source.codePointAt(i);
    offset += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
    if (offset > 4 * 1024 * 1024) fail("READING_LIMIT", "Markdown source exceeds 4 MiB");
    i += cp > 65535 ? 2 : 1;
  }
  if (start === null || end === null) fail("INVALID_ARGUMENT", "source span splits UTF-8 or exceeds source length");
  return Object.freeze({ start, end });
}
