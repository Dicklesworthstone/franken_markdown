// Semantic data from the existing engine, never a second Markdown parser.
export class FlowReadingError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "FlowReadingError";
    this.code = code;
  }
}
const fail = (code, message) => {
  throw new FlowReadingError(code, message);
};
const invalid = (message) => fail("INVALID_READING_DATA", message);
const DEFAULTS = Object.freeze({
  maxNodes: 10000,
  maxDepth: 64,
  maxTextUnits: 1048576,
  maxInlineRuns: 50000,
  maxLinkUnits: 1048576,
  maxListEntries: 50000,
  maxAnchorUnits: 1048576,
});
const ROLES = new Set([
  "document",
  "heading",
  "paragraph",
  "code-block",
  "list",
  "list-item",
  "table",
  "table-header-row",
  "table-row",
  "table-header-cell",
  "table-cell",
  "blockquote",
  "thematic-break",
  "image",
]);
const ROWS = new Set(["table-header-row", "table-row"]);
const CELLS = new Set(["table-header-cell", "table-cell"]);
const LEAVES = new Set(["heading", "paragraph", "code-block", "thematic-break", "image"]);
const owned = new WeakSet(),
  sessions = new WeakMap();
function record(value, keys) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).some((k) => !keys.includes(k))
  ) {
    fail("INVALID_OPTIONS", "unsupported reading options");
  }
}
function identity(value) {
  if (typeof value === "bigint") value = String(value);
  if (
    typeof value !== "string" ||
    !/^(0|[1-9][0-9]{0,19})$/.test(value) ||
    BigInt(value) > 18446744073709551615n
  ) {
    invalid("expected a lossless u64 identity");
  }
  return value;
}
function token(value) {
  return Object.freeze({
    revision: identity(value?.revision),
    layoutRevision: identity(value?.layoutRevision),
  });
}
function equal(a, b) {
  return a.revision === b.revision && a.layoutRevision === b.layoutRevision;
}
function check(session, expected) {
  if (session.disposed) fail("SESSION_DISPOSED", "reading session is disposed");
  const now = token(session.token);
  if (now.revision !== expected.revision)
    fail("STALE_REVISION", "reading text belongs to an older source");
  if (now.layoutRevision !== expected.layoutRevision)
    fail("STALE_LAYOUT", "reading geometry belongs to an older layout");
}
function integer(value, max = 0xffffffff) {
  return Number.isSafeInteger(value) && value >= 0 && value <= max;
}
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
function rectangle(value) {
  if (
    !value ||
    !["x", "y", "width", "height"].every(
      (k) =>
        typeof value[k] === "number" &&
        Number.isFinite(value[k]) &&
        Math.abs(value[k]) <= 1000000000,
    ) ||
    value.width < 0 ||
    value.height < 0
  )
    invalid("invalid reading bounds");
  return Object.freeze({ x: value.x, y: value.y, width: value.width, height: value.height });
}
function span(value) {
  if (!integer(value?.startByte) || !integer(value?.endByte) || value.startByte > value.endByte)
    invalid("invalid enclosing source span");
  return Object.freeze({ startByte: value.startByte, endByte: value.endByte });
}
function limits(value = {}) {
  record(value, Object.keys(DEFAULTS));
  const result = { ...DEFAULTS };
  for (const key of Object.keys(value)) {
    if (!integer(value[key], DEFAULTS[key]) || value[key] < 1)
      fail("INVALID_OPTIONS", "reading limits may only lower positive defaults");
    result[key] = value[key];
  }
  return result;
}
function cancelled(signal) {
  if (signal?.aborted) fail("ABORTED", "reading was aborted");
}
// Only cancel the wait. Never forward AbortSignal to a worker RPC, which could
// terminate the editing session. Every late resolution/rejection remains observed.
function wait(promise, signal) {
  if (!signal) return Promise.resolve(promise);
  return new Promise((resolve, reject) => {
    const abort = () => {
      cleanup();
      reject(new FlowReadingError("ABORTED", "reading was aborted"));
    };
    const cleanup = () => signal.removeEventListener("abort", abort);
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(promise).then(
      (value) => {
        cleanup();
        resolve(value);
      },
      (error) => {
        cleanup();
        reject(error);
      },
    );
    if (signal.aborted) abort();
  });
}

// Mirrors the core's conservative activation filter. A target is still data;
// passing this filter never authorizes the host to navigate or fetch anything.
function activeTarget(target) {
  let start = 0,
    end = target.length;
  const whitespace = /\p{White_Space}/u;
  while (start < end && whitespace.test(target[start])) start++;
  while (end > start && whitespace.test(target[end - 1])) end--;
  const value = target.slice(start, end);
  if (!value || /[\u0000-\u001f\u007f-\u009f\\]/u.test(value)) return null;
  const prefix = value.split(/[/?#]/u, 1)[0],
    colon = prefix.indexOf(":");
  if (colon !== -1 && !["http", "https", "mailto"].includes(prefix.slice(0, colon).toLowerCase()))
    return null;
  return value;
}
function inlineMetadata(raw, budget, retained) {
  const input = raw.inlineRuns === undefined ? [] : raw.inlineRuns;
  if (!Array.isArray(input)) invalid("inline runs must be an array");
  retained.runs += input.length;
  if (retained.runs > budget.maxInlineRuns)
    fail("READING_LIMIT", "reading inline-run limit exceeded");
  if (
    input.length &&
    (raw.children.length || ["image", "thematic-break", "code-block"].includes(raw.role))
  ) {
    invalid("inline ranges require a semantic text leaf");
  }
  function link(value) {
    if (value === null) return null;
    if (
      !value ||
      typeof value !== "object" ||
      Array.isArray(value) ||
      typeof value.target !== "string" ||
      !(value.activeTarget === null || typeof value.activeTarget === "string")
    )
      invalid("invalid reading link");
    // Charge both strings, even if the JS engine shares their storage.
    retained.links += value.target.length + (value.activeTarget?.length ?? 0);
    if (retained.links > budget.maxLinkUnits)
      fail("READING_LIMIT", "reading link retention limit exceeded");
    if (
      !wellFormed(value.target) ||
      (value.activeTarget !== null &&
        (!wellFormed(value.activeTarget) || activeTarget(value.target) !== value.activeTarget))
    ) {
      invalid("inconsistent or unsafe reading link activation");
    }
    return Object.freeze({ target: value.target, activeTarget: value.activeTarget });
  }
  // Advance monotonically across reading text: O(text + runs), without a
  // byte-sized lookup table or rescanning a prefix for every styled fragment.
  let byte = 0,
    utf16 = 0,
    previousEnd = 0;
  function position(offset) {
    while (byte < offset && utf16 < raw.text.length) {
      const cp = raw.text.codePointAt(utf16);
      byte += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      utf16 += cp > 65535 ? 2 : 1;
    }
    if (byte !== offset) invalid("inline range splits UTF-8 or exceeds reading text");
    return utf16;
  }
  const inlineRuns = Array.from(input, (run) => {
    if (
      !run ||
      !integer(run.startByte) ||
      !integer(run.endByte) ||
      run.startByte < previousEnd ||
      run.endByte <= run.startByte
    )
      invalid("unordered or empty inline range");
    previousEnd = run.endByte;
    const style = run.style,
      keys = ["bold", "italic", "code", "strikethrough"];
    if (
      !style ||
      typeof style !== "object" ||
      Array.isArray(style) ||
      Object.keys(style).length !== keys.length ||
      !keys.every((key) => Object.hasOwn(style, key) && typeof style[key] === "boolean")
    )
      invalid("invalid reading inline style");
    const startUtf16 = position(run.startByte),
      endUtf16 = position(run.endByte);
    return Object.freeze({
      startByte: run.startByte,
      endByte: run.endByte,
      startUtf16,
      endUtf16,
      style: Object.freeze({
        bold: style.bold,
        italic: style.italic,
        code: style.code,
        strikethrough: style.strikethrough,
      }),
      link: link(run.link),
    });
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
  if (depth !== 0 && path.length)
    invalid("nested reading children inherit their root's list ownership");
  retained.listEntries += path.length;
  if (path.length > budget.maxDepth || retained.listEntries > budget.maxListEntries) {
    fail("READING_LIMIT", "reading list ancestry limit exceeded");
  }
  return Object.freeze(
    Array.from(path, (item) => {
      const keys = ["listId", "ordered", "start", "itemIndex", "task"];
      if (
        !item ||
        typeof item !== "object" ||
        Array.isArray(item) ||
        Object.keys(item).length !== keys.length ||
        !keys.every((key) => Object.hasOwn(item, key)) ||
        typeof item.listId !== "string" ||
        typeof item.start !== "string" ||
        typeof item.ordered !== "boolean" ||
        !integer(item.itemIndex) ||
        !(item.task === null || typeof item.task === "boolean")
      )
        invalid("invalid list item identity");
      const listId = identity(item.listId),
        start = identity(item.start);
      if (
        listId === "0" ||
        (item.ordered && BigInt(start) + BigInt(item.itemIndex) > 18446744073709551615n)
      ) {
        invalid("invalid list identity or overflowing ordinal");
      }
      return Object.freeze({
        listId,
        ordered: item.ordered,
        start,
        itemIndex: item.itemIndex,
        task: item.task,
      });
    }),
  );
}

function validateListOrder(roots) {
  let structured = null,
    previous = [];
  const seen = new Set();
  const sameList = (a, b) =>
    a.listId === b.listId && a.ordered === b.ordered && a.start === b.start;
  for (const node of roots) {
    const hasPath = node.listPath !== null;
    if (structured === null) structured = hasPath;
    if (structured !== hasPath) invalid("mixed legacy and structured list ownership");
    if (!hasPath) continue;
    const path = node.listPath;
    let common = 0;
    while (
      common < path.length &&
      common < previous.length &&
      path[common].listId === previous[common].listId &&
      path[common].itemIndex === previous[common].itemIndex
    ) {
      if (
        !sameList(path[common], previous[common]) ||
        path[common].task !== previous[common].task
      ) {
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
      const item = path[common],
        before = previous[common];
      if (before?.listId === item.listId) {
        if (!sameList(item, before) || item.itemIndex !== before.itemIndex + 1) {
          invalid("list items are not consecutive within one list");
        }
      } else {
        if (seen.has(item.listId) || item.itemIndex !== 0)
          invalid("closed or foreign list identity was reused");
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
  #session;
  #matches = new WeakSet();
  #anchors;
  constructor(secret, session, expected, roots, nodes, textUnits, retained, anchors) {
    if (secret !== owned)
      fail("INVALID_ARGUMENT", "use readFlowDocument to obtain a reading document");
    this.#session = session;
    this.#anchors = anchors;
    this.token = expected;
    this.roots = Object.freeze(roots);
    this.nodes = Object.freeze(nodes);
    this.headings = Object.freeze(nodes.filter((node) => node.role === "heading"));
    // Container transcripts summarize children. Search/copy leaves once, not
    // both a table row's aggregate "A | B" and its individually typed cells.
    this.text = nodes
      .filter((node) => !node.children.length && node.text)
      .map((node) => node.text)
      .join("\n\n");
    this.textUnits = textUnits;
    this.inlineRunCount = retained.runs;
    this.linkUnits = retained.links;
    this.listEntryCount = retained.listEntries;
    this.anchorUnits = retained.anchors;
    owned.add(this);
    sessions.set(this, session);
    Object.freeze(this);
  }
  assertCurrent() {
    check(this.#session, this.token);
  }
  locate(index) {
    this.assertCurrent();
    if (!integer(index, this.nodes.length - 1)) fail("INVALID_ARGUMENT", "unknown reading node");
    const node = this.nodes[index];
    return Object.freeze({
      ...this.token,
      nodeIndex: index,
      bounds: node.bounds,
      enclosingSourceSpan: node.enclosingSourceSpan,
    });
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
    try {
      id = decodeURIComponent(target.slice(1));
    } catch {
      return null;
    }
    // Percent decode exactly once. '+' is literal, and matching is case-sensitive.
    const index = this.#anchors.get(id);
    return index === undefined ? null : this.locate(index);
  }
  /** Literal matches in unsplit logical leaf text. Never fragment/source offsets. */
  find(query, options = {}) {
    this.assertCurrent();
    const { iterator } = readingSearch(this.nodes, query, options, false, fail);
    let step = iterator.next();
    while (!step.done) step = iterator.next();
    return this.#publishMatches(step.value);
  }
  /** Cooperatively search this snapshot without worker I/O or partial results. */
  async findAsync(query, options = {}) {
    this.assertCurrent();
    const { iterator, signal } = readingSearch(this.nodes, query, options, true, fail);
    const current = () => { cancelled(signal); this.assertCurrent(); };
    try {
      current();
      let step = iterator.next();
      while (!step.done) {
        await readingSearchTurn(signal, current);
        step = iterator.next();
      }
      current();
      return this.#publishMatches(step.value);
    } finally { iterator.return(); }
  }
  #publishMatches(result) {
    this.assertCurrent();
    const matches = result.matches.map(position => {
      const match = Object.freeze({ ...this.token, ...position });
      this.#matches.add(match);
      return match;
    });
    return Object.freeze({ matches: Object.freeze(matches), truncated: result.truncated });
  }
  matchText(match) {
    this.assertCurrent();
    if (!this.#matches.has(match))
      fail("INVALID_ARGUMENT", "match does not belong to this reading snapshot");
    return this.nodes[match.nodeIndex].text.slice(match.startUtf16, match.endUtf16);
  }
}
export function requireReadingDocument(value, session) {
  if (!owned.has(value)) fail("INVALID_ARGUMENT", "an admitted reading document is required");
  if (session !== undefined && sessions.get(value) !== session)
    fail("INVALID_ARGUMENT", "reading document belongs to another session");
  value.assertCurrent();
  return value;
}

export async function readFlowDocument(session, options = {}) {
  record(options, ["token", "signal", "limits"]);
  if (!session || typeof session.readingOrder !== "function")
    fail("INVALID_ARGUMENT", "a flow session is required");
  const budget = limits(options.limits),
    expected = token(options.token ?? session.token),
    signal = options.signal;
  if (
    signal !== undefined &&
    (!signal ||
      typeof signal.aborted !== "boolean" ||
      typeof signal.addEventListener !== "function" ||
      typeof signal.removeEventListener !== "function")
  )
    fail("INVALID_OPTIONS", "signal must be an AbortSignal");
  const roots = [],
    nodes = [],
    seen = new WeakSet(),
    retained = { runs: 0, links: 0, listEntries: 0, anchors: 0 };
  let textUnits = 0;
  const anchors = new Map();
  function anchorMetadata(raw, index) {
    if (raw.anchorId === undefined || raw.anchorId === null) return null; // Legacy producer.
    const id = raw.anchorId;
    if (raw.role !== "heading" || typeof id !== "string" || !id.length)
      invalid("invalid heading destination");
    retained.anchors += id.length;
    if (retained.anchors > budget.maxAnchorUnits)
      fail("READING_LIMIT", "reading anchor retention limit exceeded");
    if (!wellFormed(id) || /[\u0000-\u001f\u007f-\u009f]/u.test(id) || anchors.has(id)) {
      invalid("invalid or duplicate heading destination");
    }
    anchors.set(id, index);
    return id;
  }
  function clone(raw, depth, parent = null) {
    if (!raw || typeof raw !== "object" || seen.has(raw)) invalid("cyclic or aliased reading tree");
    if (depth > budget.maxDepth || nodes.length >= budget.maxNodes)
      fail("READING_LIMIT", "reading node/depth limit exceeded");
    seen.add(raw);
    if (!ROLES.has(raw.role) || typeof raw.text !== "string" || !Array.isArray(raw.children))
      invalid("invalid reading node");
    textUnits += raw.text.length;
    if (textUnits > budget.maxTextUnits || raw.children.length > budget.maxNodes - nodes.length)
      fail("READING_LIMIT", "reading retention limit exceeded");
    if (!wellFormed(raw.text)) invalid("reading text contains an unpaired surrogate");
    if (
      (raw.role === "heading" && (!integer(raw.level, 6) || raw.level < 1)) ||
      (LEAVES.has(raw.role) && raw.children.length) ||
      (raw.role === "thematic-break" && raw.text !== "") ||
      (CELLS.has(raw.role) && !ROWS.has(parent)) ||
      (parent === "table" && !ROWS.has(raw.role)) ||
      (ROWS.has(parent) && !CELLS.has(raw.role)) ||
      (parent === "list" && raw.role !== "list-item")
    )
      invalid("inconsistent reading structure");
    const node = {
      index: nodes.length,
      role: raw.role,
      text: raw.text,
      bounds: rectangle(raw.bounds),
      anchorId: anchorMetadata(raw, nodes.length),
      ...inlineMetadata(raw, budget, retained),
      listPath: listMetadata(raw, depth, budget, retained),
      enclosingSourceSpan: span(raw.enclosingSourceSpan),
      ...(raw.role === "heading" ? { level: raw.level } : {}),
      children: [],
    };
    nodes.push(node);
    for (const child of raw.children) node.children.push(clone(child, depth + 1, raw.role));
    Object.freeze(node.children);
    return Object.freeze(node);
  }
  let offset = 0,
    total = null;
  do {
    cancelled(signal);
    check(session, expected);
    const page = await wait(session.readingOrder({ offset, limit: 256, token: expected }), signal);
    cancelled(signal);
    check(session, expected);
    if (
      page?.schemaVersion !== 1 ||
      typeof page.revision !== "string" ||
      typeof page.layoutRevision !== "string" ||
      !equal(token(page), expected) ||
      page.offset !== offset ||
      !integer(page.total) ||
      !Array.isArray(page.nodes) ||
      page.nodes.length !== Math.min(256, page.total - offset) ||
      (total !== null && total !== page.total)
    )
      invalid("inconsistent reading page");
    if (page.total > budget.maxNodes) fail("READING_LIMIT", "too many reading roots");
    const end = offset + page.nodes.length;
    if (page.nextOffset !== (end < page.total ? end : null))
      invalid("reading pagination did not advance");
    total = page.total;
    for (const raw of page.nodes) roots.push(clone(raw, 0));
    offset = page.nextOffset;
  } while (offset !== null);
  cancelled(signal);
  check(session, expected);
  validateListOrder(roots);
  return new FlowReadingDocument(
    owned,
    session,
    expected,
    roots,
    nodes,
    textUnits,
    retained,
    anchors,
  );
}

/** Convert an ENCLOSING original Markdown UTF-8 span for textarea navigation.
 * The host must first fence the source revision. No exact inline map is implied. */
export function sourceSpanToUtf16(source, range) {
  const bytes = span(range);
  if (typeof source !== "string" || source.length > 4 * 1024 * 1024 || !wellFormed(source))
    fail("INVALID_ARGUMENT", "invalid bounded Markdown source");
  let offset = 0,
    start = null,
    end = null;
  for (let i = 0; i <= source.length; ) {
    if (offset === bytes.startByte) start = i;
    if (offset === bytes.endByte) end = i;
    if (i === source.length) break;
    const cp = source.codePointAt(i);
    offset += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
    if (offset > 4 * 1024 * 1024) fail("READING_LIMIT", "Markdown source exceeds 4 MiB");
    i += cp > 65535 ? 2 : 1;
  }
  if (start === null || end === null)
    fail("INVALID_ARGUMENT", "source span splits UTF-8 or exceeds source length");
  return Object.freeze({ start, end });
}

// Streaming literal search over admitted, immutable semantic leaves. No source
// parser, regular-expression search, worker RPC, or document-sized folded copy.
const WORD = /[\p{L}\p{M}\p{N}\p{Pc}\u200c\u200d]/u;
const NO_BOUNDARY = 0xffffffff;

function readingSearch(nodes, query, options, asynchronous, fail) {
  const keys = ["asciiCaseInsensitive", "caseInsensitive", "wholeWord", "maxMatches",
    ...(asynchronous ? ["signal"] : [])];
  if (!options || typeof options !== "object" || Array.isArray(options)
      || Object.keys(options).some(key => !keys.includes(key))) {
    fail("INVALID_OPTIONS", "unsupported reading search options");
  }
  const ascii = options.asciiCaseInsensitive ?? false;
  const unicode = options.caseInsensitive ?? false;
  const wholeWord = options.wholeWord ?? false, max = options.maxMatches ?? 1000;
  if ([ascii, unicode, wholeWord].some(value => typeof value !== "boolean") || (ascii && unicode)
      || !Number.isInteger(max) || max < 1 || max > 1000 || typeof query !== "string"
      || query.length > 1024) fail("INVALID_ARGUMENT", "invalid literal reading search");
  const signal = asynchronous ? options.signal : undefined;
  if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
      || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")) {
    fail("INVALID_OPTIONS", "signal must be an AbortSignal");
  }
  const fold = unicode ? foldScalar : ascii
    ? cp => String.fromCodePoint(cp >= 65 && cp <= 90 ? cp + 32 : cp)
    : cp => String.fromCodePoint(cp);
  const needle = [];
  for (const scalar of query) {
    const cp = scalar.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) fail("INVALID_ARGUMENT", "search contains an unpaired surrogate");
    for (const part of fold(cp)) needle.push(part.codePointAt(0));
  }
  return { signal, iterator: scan(nodes, needle, fold, wholeWord, max) };
}

// KMP prefix lengths guarantee linear matching work even for repeated prefixes.
// A ring maps only the last needle-length folded scalars back to original UTF-16.
// Expansion interiors are NOT eligible boundaries: 'ss' finds 'ß', 's' does not
// select half of it. Successful matches are non-overlapping in original text.
function* scan(nodes, needle, fold, wholeWord, max) {
  const matches = [], size = needle.length;
  if (!size) return { matches, truncated: false };
  const prefix = new Uint32Array(size), starts = new Uint32Array(size);
  for (let i = 1, length = 0; i < size; i++) {
    while (length && needle[i] !== needle[length]) length = prefix[length - 1];
    if (needle[i] === needle[length]) length++;
    prefix[i] = length;
  }
  let work = 0;
  for (const node of nodes) {
    if (++work >= 4096) { work = 0; yield; }
    if (node.children.length || !node.text) continue;
    const text = node.text;
    let matched = 0, position = 0;
    for (let offset = 0; offset < text.length;) {
      const cp = text.codePointAt(offset), end = offset + (cp > 0xffff ? 2 : 1), folded = fold(cp);
      for (let part = 0; part < folded.length;) {
        const value = folded.codePointAt(part), first = part === 0;
        part += value > 0xffff ? 2 : 1;
        starts[position % size] = first ? offset : NO_BOUNDARY;
        while (matched && needle[matched] !== value) matched = prefix[matched - 1];
        if (needle[matched] === value) matched++;
        if (matched === size) {
          const start = starts[(position + 1 - size) % size];
          if (start !== NO_BOUNDARY && part === folded.length
              && (!wholeWord || (!wordBefore(text, start) && !wordAt(text, end)))) {
            if (matches.length === max) return { matches, truncated: true };
            matches.push({ nodeIndex: node.index, startUtf16: start, endUtf16: end });
            matched = 0;
          } else matched = prefix[matched - 1];
        }
        position++;
      }
      work += end - offset;
      offset = end;
      if (work >= 4096) { work = 0; yield; }
    }
  }
  return { matches, truncated: false };
}
function wordAt(text, offset) {
  return offset < text.length && WORD.test(String.fromCodePoint(text.codePointAt(offset)));
}
function wordBefore(text, offset) {
  if (!offset) return false;
  let at = offset - 1;
  const unit = text.charCodeAt(at);
  if (unit >= 0xdc00 && unit <= 0xdfff) at--;
  return wordAt(text, at);
}

// A real event-loop turn allows editor input to abort a long synchronous leaf.
// Timer/listener ownership is released on success, abort or a revision failure.
function readingSearchTurn(signal, check) {
  check();
  return new Promise((resolve, reject) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", finish);
      try { check(); resolve(); } catch (error) { reject(error); }
    };
    const timer = setTimeout(finish, 0);
    signal?.addEventListener("abort", finish, { once: true });
    if (signal?.aborted) finish();
  });
}

// BEGIN GENERATED CASE FOLDING
// Generated by scripts/generate-reading-casefold.py. Do not edit the tables.
// Frozen Unicode 15.1.0 default FULL (C+F, non-Turkic) case folding.
// Not normalization, accent removal, transliteration, or locale-sensitive casing.
// https://www.unicode.org/Public/15.1.0/ucd/CaseFolding.txt
/*
UNICODE LICENSE V3
Copyright © 1991-2026 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.
Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.
THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.
IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.
Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
*/
export const CASE_FOLD_VERSION = "15.1.0";
// [first, last, stride, delta]; sorted non-overlapping inclusive ranges.
const RANGES = [
  [0x41, 0x5a, 1, 32],
  [0xb5, 0xb5, 1, 775],
  [0xc0, 0xd6, 1, 32],
  [0xd8, 0xde, 1, 32],
  [0x100, 0x12e, 2, 1],
  [0x132, 0x136, 2, 1],
  [0x139, 0x147, 2, 1],
  [0x14a, 0x176, 2, 1],
  [0x178, 0x178, 1, -121],
  [0x179, 0x17d, 2, 1],
  [0x17f, 0x17f, 1, -268],
  [0x181, 0x181, 1, 210],
  [0x182, 0x184, 2, 1],
  [0x186, 0x186, 1, 206],
  [0x187, 0x187, 1, 1],
  [0x189, 0x18a, 1, 205],
  [0x18b, 0x18b, 1, 1],
  [0x18e, 0x18e, 1, 79],
  [0x18f, 0x18f, 1, 202],
  [0x190, 0x190, 1, 203],
  [0x191, 0x191, 1, 1],
  [0x193, 0x193, 1, 205],
  [0x194, 0x194, 1, 207],
  [0x196, 0x196, 1, 211],
  [0x197, 0x197, 1, 209],
  [0x198, 0x198, 1, 1],
  [0x19c, 0x19c, 1, 211],
  [0x19d, 0x19d, 1, 213],
  [0x19f, 0x19f, 1, 214],
  [0x1a0, 0x1a4, 2, 1],
  [0x1a6, 0x1a6, 1, 218],
  [0x1a7, 0x1a7, 1, 1],
  [0x1a9, 0x1a9, 1, 218],
  [0x1ac, 0x1ac, 1, 1],
  [0x1ae, 0x1ae, 1, 218],
  [0x1af, 0x1af, 1, 1],
  [0x1b1, 0x1b2, 1, 217],
  [0x1b3, 0x1b5, 2, 1],
  [0x1b7, 0x1b7, 1, 219],
  [0x1b8, 0x1b8, 1, 1],
  [0x1bc, 0x1bc, 1, 1],
  [0x1c4, 0x1c4, 1, 2],
  [0x1c5, 0x1c5, 1, 1],
  [0x1c7, 0x1c7, 1, 2],
  [0x1c8, 0x1c8, 1, 1],
  [0x1ca, 0x1ca, 1, 2],
  [0x1cb, 0x1db, 2, 1],
  [0x1de, 0x1ee, 2, 1],
  [0x1f1, 0x1f1, 1, 2],
  [0x1f2, 0x1f4, 2, 1],
  [0x1f6, 0x1f6, 1, -97],
  [0x1f7, 0x1f7, 1, -56],
  [0x1f8, 0x21e, 2, 1],
  [0x220, 0x220, 1, -130],
  [0x222, 0x232, 2, 1],
  [0x23a, 0x23a, 1, 10795],
  [0x23b, 0x23b, 1, 1],
  [0x23d, 0x23d, 1, -163],
  [0x23e, 0x23e, 1, 10792],
  [0x241, 0x241, 1, 1],
  [0x243, 0x243, 1, -195],
  [0x244, 0x244, 1, 69],
  [0x245, 0x245, 1, 71],
  [0x246, 0x24e, 2, 1],
  [0x345, 0x345, 1, 116],
  [0x370, 0x372, 2, 1],
  [0x376, 0x376, 1, 1],
  [0x37f, 0x37f, 1, 116],
  [0x386, 0x386, 1, 38],
  [0x388, 0x38a, 1, 37],
  [0x38c, 0x38c, 1, 64],
  [0x38e, 0x38f, 1, 63],
  [0x391, 0x3a1, 1, 32],
  [0x3a3, 0x3ab, 1, 32],
  [0x3c2, 0x3c2, 1, 1],
  [0x3cf, 0x3cf, 1, 8],
  [0x3d0, 0x3d0, 1, -30],
  [0x3d1, 0x3d1, 1, -25],
  [0x3d5, 0x3d5, 1, -15],
  [0x3d6, 0x3d6, 1, -22],
  [0x3d8, 0x3ee, 2, 1],
  [0x3f0, 0x3f0, 1, -54],
  [0x3f1, 0x3f1, 1, -48],
  [0x3f4, 0x3f4, 1, -60],
  [0x3f5, 0x3f5, 1, -64],
  [0x3f7, 0x3f7, 1, 1],
  [0x3f9, 0x3f9, 1, -7],
  [0x3fa, 0x3fa, 1, 1],
  [0x3fd, 0x3ff, 1, -130],
  [0x400, 0x40f, 1, 80],
  [0x410, 0x42f, 1, 32],
  [0x460, 0x480, 2, 1],
  [0x48a, 0x4be, 2, 1],
  [0x4c0, 0x4c0, 1, 15],
  [0x4c1, 0x4cd, 2, 1],
  [0x4d0, 0x52e, 2, 1],
  [0x531, 0x556, 1, 48],
  [0x10a0, 0x10c5, 1, 7264],
  [0x10c7, 0x10c7, 1, 7264],
  [0x10cd, 0x10cd, 1, 7264],
  [0x13f8, 0x13fd, 1, -8],
  [0x1c80, 0x1c80, 1, -6222],
  [0x1c81, 0x1c81, 1, -6221],
  [0x1c82, 0x1c82, 1, -6212],
  [0x1c83, 0x1c84, 1, -6210],
  [0x1c85, 0x1c85, 1, -6211],
  [0x1c86, 0x1c86, 1, -6204],
  [0x1c87, 0x1c87, 1, -6180],
  [0x1c88, 0x1c88, 1, 35267],
  [0x1c90, 0x1cba, 1, -3008],
  [0x1cbd, 0x1cbf, 1, -3008],
  [0x1e00, 0x1e94, 2, 1],
  [0x1e9b, 0x1e9b, 1, -58],
  [0x1ea0, 0x1efe, 2, 1],
  [0x1f08, 0x1f0f, 1, -8],
  [0x1f18, 0x1f1d, 1, -8],
  [0x1f28, 0x1f2f, 1, -8],
  [0x1f38, 0x1f3f, 1, -8],
  [0x1f48, 0x1f4d, 1, -8],
  [0x1f59, 0x1f5f, 2, -8],
  [0x1f68, 0x1f6f, 1, -8],
  [0x1fb8, 0x1fb9, 1, -8],
  [0x1fba, 0x1fbb, 1, -74],
  [0x1fbe, 0x1fbe, 1, -7173],
  [0x1fc8, 0x1fcb, 1, -86],
  [0x1fd8, 0x1fd9, 1, -8],
  [0x1fda, 0x1fdb, 1, -100],
  [0x1fe8, 0x1fe9, 1, -8],
  [0x1fea, 0x1feb, 1, -112],
  [0x1fec, 0x1fec, 1, -7],
  [0x1ff8, 0x1ff9, 1, -128],
  [0x1ffa, 0x1ffb, 1, -126],
  [0x2126, 0x2126, 1, -7517],
  [0x212a, 0x212a, 1, -8383],
  [0x212b, 0x212b, 1, -8262],
  [0x2132, 0x2132, 1, 28],
  [0x2160, 0x216f, 1, 16],
  [0x2183, 0x2183, 1, 1],
  [0x24b6, 0x24cf, 1, 26],
  [0x2c00, 0x2c2f, 1, 48],
  [0x2c60, 0x2c60, 1, 1],
  [0x2c62, 0x2c62, 1, -10743],
  [0x2c63, 0x2c63, 1, -3814],
  [0x2c64, 0x2c64, 1, -10727],
  [0x2c67, 0x2c6b, 2, 1],
  [0x2c6d, 0x2c6d, 1, -10780],
  [0x2c6e, 0x2c6e, 1, -10749],
  [0x2c6f, 0x2c6f, 1, -10783],
  [0x2c70, 0x2c70, 1, -10782],
  [0x2c72, 0x2c72, 1, 1],
  [0x2c75, 0x2c75, 1, 1],
  [0x2c7e, 0x2c7f, 1, -10815],
  [0x2c80, 0x2ce2, 2, 1],
  [0x2ceb, 0x2ced, 2, 1],
  [0x2cf2, 0x2cf2, 1, 1],
  [0xa640, 0xa66c, 2, 1],
  [0xa680, 0xa69a, 2, 1],
  [0xa722, 0xa72e, 2, 1],
  [0xa732, 0xa76e, 2, 1],
  [0xa779, 0xa77b, 2, 1],
  [0xa77d, 0xa77d, 1, -35332],
  [0xa77e, 0xa786, 2, 1],
  [0xa78b, 0xa78b, 1, 1],
  [0xa78d, 0xa78d, 1, -42280],
  [0xa790, 0xa792, 2, 1],
  [0xa796, 0xa7a8, 2, 1],
  [0xa7aa, 0xa7aa, 1, -42308],
  [0xa7ab, 0xa7ab, 1, -42319],
  [0xa7ac, 0xa7ac, 1, -42315],
  [0xa7ad, 0xa7ad, 1, -42305],
  [0xa7ae, 0xa7ae, 1, -42308],
  [0xa7b0, 0xa7b0, 1, -42258],
  [0xa7b1, 0xa7b1, 1, -42282],
  [0xa7b2, 0xa7b2, 1, -42261],
  [0xa7b3, 0xa7b3, 1, 928],
  [0xa7b4, 0xa7c2, 2, 1],
  [0xa7c4, 0xa7c4, 1, -48],
  [0xa7c5, 0xa7c5, 1, -42307],
  [0xa7c6, 0xa7c6, 1, -35384],
  [0xa7c7, 0xa7c9, 2, 1],
  [0xa7d0, 0xa7d0, 1, 1],
  [0xa7d6, 0xa7d8, 2, 1],
  [0xa7f5, 0xa7f5, 1, 1],
  [0xab70, 0xabbf, 1, -38864],
  [0xff21, 0xff3a, 1, 32],
  [0x10400, 0x10427, 1, 40],
  [0x104b0, 0x104d3, 1, 40],
  [0x10570, 0x1057a, 1, 39],
  [0x1057c, 0x1058a, 1, 39],
  [0x1058c, 0x10592, 1, 39],
  [0x10594, 0x10595, 1, 39],
  [0x10c80, 0x10cb2, 1, 64],
  [0x118a0, 0x118bf, 1, 32],
  [0x16e40, 0x16e5f, 1, 32],
  [0x1e900, 0x1e921, 1, 34],
];
const EXPANSIONS = new Map([
  [0xdf, "ss"],
  [0x130, "i\u0307"],
  [0x149, "\u02bcn"],
  [0x1f0, "j\u030c"],
  [0x390, "\u03b9\u0308\u0301"],
  [0x3b0, "\u03c5\u0308\u0301"],
  [0x587, "\u0565\u0582"],
  [0x1e96, "h\u0331"],
  [0x1e97, "t\u0308"],
  [0x1e98, "w\u030a"],
  [0x1e99, "y\u030a"],
  [0x1e9a, "a\u02be"],
  [0x1e9e, "ss"],
  [0x1f50, "\u03c5\u0313"],
  [0x1f52, "\u03c5\u0313\u0300"],
  [0x1f54, "\u03c5\u0313\u0301"],
  [0x1f56, "\u03c5\u0313\u0342"],
  [0x1f80, "\u1f00\u03b9"],
  [0x1f81, "\u1f01\u03b9"],
  [0x1f82, "\u1f02\u03b9"],
  [0x1f83, "\u1f03\u03b9"],
  [0x1f84, "\u1f04\u03b9"],
  [0x1f85, "\u1f05\u03b9"],
  [0x1f86, "\u1f06\u03b9"],
  [0x1f87, "\u1f07\u03b9"],
  [0x1f88, "\u1f00\u03b9"],
  [0x1f89, "\u1f01\u03b9"],
  [0x1f8a, "\u1f02\u03b9"],
  [0x1f8b, "\u1f03\u03b9"],
  [0x1f8c, "\u1f04\u03b9"],
  [0x1f8d, "\u1f05\u03b9"],
  [0x1f8e, "\u1f06\u03b9"],
  [0x1f8f, "\u1f07\u03b9"],
  [0x1f90, "\u1f20\u03b9"],
  [0x1f91, "\u1f21\u03b9"],
  [0x1f92, "\u1f22\u03b9"],
  [0x1f93, "\u1f23\u03b9"],
  [0x1f94, "\u1f24\u03b9"],
  [0x1f95, "\u1f25\u03b9"],
  [0x1f96, "\u1f26\u03b9"],
  [0x1f97, "\u1f27\u03b9"],
  [0x1f98, "\u1f20\u03b9"],
  [0x1f99, "\u1f21\u03b9"],
  [0x1f9a, "\u1f22\u03b9"],
  [0x1f9b, "\u1f23\u03b9"],
  [0x1f9c, "\u1f24\u03b9"],
  [0x1f9d, "\u1f25\u03b9"],
  [0x1f9e, "\u1f26\u03b9"],
  [0x1f9f, "\u1f27\u03b9"],
  [0x1fa0, "\u1f60\u03b9"],
  [0x1fa1, "\u1f61\u03b9"],
  [0x1fa2, "\u1f62\u03b9"],
  [0x1fa3, "\u1f63\u03b9"],
  [0x1fa4, "\u1f64\u03b9"],
  [0x1fa5, "\u1f65\u03b9"],
  [0x1fa6, "\u1f66\u03b9"],
  [0x1fa7, "\u1f67\u03b9"],
  [0x1fa8, "\u1f60\u03b9"],
  [0x1fa9, "\u1f61\u03b9"],
  [0x1faa, "\u1f62\u03b9"],
  [0x1fab, "\u1f63\u03b9"],
  [0x1fac, "\u1f64\u03b9"],
  [0x1fad, "\u1f65\u03b9"],
  [0x1fae, "\u1f66\u03b9"],
  [0x1faf, "\u1f67\u03b9"],
  [0x1fb2, "\u1f70\u03b9"],
  [0x1fb3, "\u03b1\u03b9"],
  [0x1fb4, "\u03ac\u03b9"],
  [0x1fb6, "\u03b1\u0342"],
  [0x1fb7, "\u03b1\u0342\u03b9"],
  [0x1fbc, "\u03b1\u03b9"],
  [0x1fc2, "\u1f74\u03b9"],
  [0x1fc3, "\u03b7\u03b9"],
  [0x1fc4, "\u03ae\u03b9"],
  [0x1fc6, "\u03b7\u0342"],
  [0x1fc7, "\u03b7\u0342\u03b9"],
  [0x1fcc, "\u03b7\u03b9"],
  [0x1fd2, "\u03b9\u0308\u0300"],
  [0x1fd3, "\u03b9\u0308\u0301"],
  [0x1fd6, "\u03b9\u0342"],
  [0x1fd7, "\u03b9\u0308\u0342"],
  [0x1fe2, "\u03c5\u0308\u0300"],
  [0x1fe3, "\u03c5\u0308\u0301"],
  [0x1fe4, "\u03c1\u0313"],
  [0x1fe6, "\u03c5\u0342"],
  [0x1fe7, "\u03c5\u0308\u0342"],
  [0x1ff2, "\u1f7c\u03b9"],
  [0x1ff3, "\u03c9\u03b9"],
  [0x1ff4, "\u03ce\u03b9"],
  [0x1ff6, "\u03c9\u0342"],
  [0x1ff7, "\u03c9\u0342\u03b9"],
  [0x1ffc, "\u03c9\u03b9"],
  [0xfb00, "ff"],
  [0xfb01, "fi"],
  [0xfb02, "fl"],
  [0xfb03, "ffi"],
  [0xfb04, "ffl"],
  [0xfb05, "st"],
  [0xfb06, "st"],
  [0xfb13, "\u0574\u0576"],
  [0xfb14, "\u0574\u0565"],
  [0xfb15, "\u0574\u056b"],
  [0xfb16, "\u057e\u0576"],
  [0xfb17, "\u0574\u056d"],
]);

// Input is one already-admitted Unicode scalar, never an arbitrary string.
export function foldScalar(cp) {
  if (cp < 128) return String.fromCodePoint(cp >= 65 && cp <= 90 ? cp + 32 : cp);
  const expansion = EXPANSIONS.get(cp);
  if (expansion !== undefined) return expansion;
  let low = 0, high = RANGES.length;
  while (low < high) {
    const mid = (low + high) >>> 1, [first, last, stride, delta] = RANGES[mid];
    if (cp < first) high = mid;
    else if (cp > last) low = mid + 1;
    else return String.fromCodePoint((cp - first) % stride === 0 ? cp + delta : cp);
  }
  return String.fromCodePoint(cp);
}
// END GENERATED CASE FOLDING
