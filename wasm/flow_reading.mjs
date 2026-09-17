// Semantic data from the existing engine, never a second Markdown parser.
export class FlowReadingError extends Error {
  constructor(code, message) { super(message); this.name = "FlowReadingError"; this.code = code; }
}
const fail = (code, message) => { throw new FlowReadingError(code, message); };
const invalid = message => fail("INVALID_READING_DATA", message);
const DEFAULTS = Object.freeze({ maxNodes: 10000, maxDepth: 64, maxTextUnits: 1048576 });
const ROLES = new Set(["document", "heading", "paragraph", "code-block", "list", "list-item", "table",
  "table-header-row", "table-row", "table-header-cell", "table-cell", "blockquote", "thematic-break", "image"]);
const ROWS = new Set(["table-header-row", "table-row"]);
const CELLS = new Set(["table-header-cell", "table-cell"]);
const LEAVES = new Set(["heading", "paragraph", "code-block", "thematic-break", "image"]);
const owned = new WeakSet();
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

/** Immutable, session-bound semantic snapshot. Construct with readFlowDocument. */
export class FlowReadingDocument {
  #session; #matches = new WeakSet();
  constructor(secret, session, expected, roots, nodes, textUnits) {
    if (secret !== owned) fail("INVALID_ARGUMENT", "use readFlowDocument to obtain a reading document");
    this.#session = session;
    this.token = expected; this.roots = Object.freeze(roots); this.nodes = Object.freeze(nodes);
    this.headings = Object.freeze(nodes.filter(node => node.role === "heading"));
    // Container transcripts summarize children. Search/copy leaves once, not
    // both a table row's aggregate "A | B" and its individually typed cells.
    this.text = nodes.filter(node => !node.children.length && node.text).map(node => node.text).join("\n\n");
    this.textUnits = textUnits;
    owned.add(this); Object.freeze(this);
  }
  assertCurrent() { check(this.#session, this.token); }
  locate(index) {
    this.assertCurrent();
    if (!integer(index, this.nodes.length - 1)) fail("INVALID_ARGUMENT", "unknown reading node");
    const node = this.nodes[index];
    return Object.freeze({ ...this.token, nodeIndex: index, bounds: node.bounds, enclosingSourceSpan: node.enclosingSourceSpan });
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
export function requireReadingDocument(value) {
  if (!owned.has(value)) fail("INVALID_ARGUMENT", "an admitted reading document is required");
  value.assertCurrent(); return value;
}

export async function readFlowDocument(session, options = {}) {
  record(options, ["token", "signal", "limits"]);
  if (!session || typeof session.readingOrder !== "function") fail("INVALID_ARGUMENT", "a flow session is required");
  const budget = limits(options.limits), expected = token(options.token ?? session.token), signal = options.signal;
  if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function"
      || typeof signal.removeEventListener !== "function")) fail("INVALID_OPTIONS", "signal must be an AbortSignal");
  const roots = [], nodes = [], seen = new WeakSet(); let textUnits = 0;
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
  return new FlowReadingDocument(owned, session, expected, roots, nodes, textUnits);
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
