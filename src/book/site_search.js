/* Offline book search. Included verbatim in the generated search page. */
(function () {
  "use strict";
  const LIMITS = Object.freeze({chapters: 4096, entries: 250000, characters: 64 * 1024 * 1024,
    query: 512, terms: 8, results: 200, page: 25});
  const fail = message => { throw new Error(message); };
  const text = value => typeof value === "string" ? value : fail("Invalid search text.");
  const pageName = value => typeof value === "string" && /^[a-zA-Z0-9_~.-]+\.html$/.test(value)
    && value.length <= 255 ? value : fail("Invalid chapter destination.");

  function prepare(value) {
    if (!value || value.schema !== "fmd-book-search-index-v1" || !Array.isArray(value.chapters)
        || value.chapters.length < 1 || value.chapters.length > LIMITS.chapters) {
      fail("Unsupported book search index.");
    }
    let characters = 0;
    const rows = [], pages = new Set();
    const account = value => {
      characters += value.length;
      if (characters > LIMITS.characters) fail("Book search text exceeds its size limit.");
      return value;
    };
    for (const chapter of value.chapters) {
      const page = pageName(chapter.page);
      if (pages.has(page.toLowerCase())) fail("Duplicate chapter destination.");
      pages.add(page.toLowerCase());
      const title = account(text(chapter.title));
      const source = account(text(chapter.source));
      if (!chapter.index || chapter.index.schema !== "fmd-search-index-v1"
          || !Array.isArray(chapter.index.entries)) fail("Unsupported chapter search index.");
      // A title-only row makes empty and heading-less chapters discoverable.
      const add = (body, anchor, heading) => {
        if (rows.length >= LIMITS.entries) fail("Book search has too many entries.");
        rows.push({page, title, source, text: account(body), anchor: account(anchor), heading,
          order: rows.length});
      };
      add(title, "", true);
      for (const entry of chapter.index.entries) {
        if (!entry || !["heading", "paragraph", "code", "table", "definition", "math"].includes(entry.kind)) {
          fail("Unsupported search entry kind.");
        }
        add(text(entry.text), text(entry.anchor), entry.kind === "heading");
      }
    }
    return rows;
  }

  function parseQuery(query) {
    if (typeof query !== "string" || query.length > LIMITS.query) fail("Search is limited to 512 characters.");
    const terms = [];
    let position = 0;
    while (position < query.length) {
      while (position < query.length && /\s/u.test(query[position])) position++;
      if (position === query.length) break;
      let term = "";
      if (query[position] === '"') {
        const end = query.indexOf('"', ++position);
        if (end < 0) fail('Close the quotation mark to search for a phrase.');
        term = query.slice(position, end);
        position = end + 1;
        if (position < query.length && !/\s/u.test(query[position])) fail("Separate phrases and words with spaces.");
      } else {
        const start = position;
        while (position < query.length && !/\s/u.test(query[position])) position++;
        term = query.slice(start, position);
      }
      term = normalize(term).trim();
      if (term && !terms.includes(term)) terms.push(term);
      if (terms.length > LIMITS.terms) fail("Use at most eight search words or phrases.");
    }
    return terms;
  }

  // Folding final sigma as well makes caseless matching independent of word
  // context: scalar/chunk lowercasing must agree with whole-query lowercasing.
  const normalize = value => value.toLowerCase().replace(/ς/gu, "σ").replace(/\s+/gu, " ");
  const CHUNK = 4096, WORK_SLICE = 16384;

  // The bound is on SOURCE work, not normalized output: a megabyte of spaces
  // still yields, even though it contributes only one searchable character.
  // Offsets map every folded UTF-16 unit back to the original scalar's start.
  function* normalizedChunks(value) {
    let position = 0, space = false;
    while (position < value.length) {
      const start = position, stop = Math.min(value.length, start + CHUNK);
      let body = "";
      const offsets = [];
      while (position < stop) {
        const at = position, code = value.codePointAt(position);
        const scalar = String.fromCodePoint(code);
        position += scalar.length;
        const whitespace = code <= 127
          ? code === 32 || (code >= 9 && code <= 13) : /\s/u.test(scalar);
        if (whitespace) {
          if (!space) { body += " "; offsets.push(at); }
          space = true;
        } else {
          space = false;
          const folded = code <= 127
            ? String.fromCharCode(code >= 65 && code <= 90 ? code + 32 : code)
            : normalize(scalar);
          body += folded;
          for (let i = 0; i < folded.length; i++) offsets.push(at);
        }
      }
      yield {body, offsets, work: position - start};
    }
  }

  function matcher(terms) {
    const longest = Math.max(...terms.map(term => term.length)), all = (1 << terms.length) - 1;
    let tail = "", positions = [], head = "", length = 0, mask = 0, offset = -1;
    return {
      add(chunk) {
        const window = tail + chunk.body, origins = positions.concat(chunk.offsets);
        for (let i = 0; i < terms.length; i++) {
          if (mask & (1 << i)) continue;
          const at = window.indexOf(terms[i]);
          if (at < 0) continue;
          mask |= 1 << i;
          const source = origins[at];
          if (offset < 0 || source < offset) offset = source;
        }
        head += chunk.body.slice(0, Math.max(0, longest + 2 - head.length));
        length += chunk.body.length;
        // Retain enough context for a phrase split across arbitrarily many
        // chunks (including chunks consisting entirely of collapsed spaces).
        const keep = Math.min(longest - 1, window.length);
        tail = keep ? window.slice(-keep) : "";
        positions = keep ? origins.slice(-keep) : [];
        // Once all terms occur and exact equality is impossible, later text
        // cannot change membership, ranking, or the earliest source offset.
        return mask === all && length > longest + 2;
      },
      result() {
        return {mask, offset, prefix: head.startsWith(terms[0]),
          exact: terms.length === 1 && length === head.length && head.trim() === terms[0]};
      },
    };
  }
  const compare = (a, b) => b.score - a.score || a.row.order - b.row.order;
  function retain(best, result) {
    if (best.length === LIMITS.results && compare(result, best[best.length - 1]) >= 0) return;
    let low = 0, high = best.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (compare(result, best[middle]) < 0) high = middle;
      else low = middle + 1;
    }
    best.splice(low, 0, result);
    if (best.length > LIMITS.results) best.pop();
  }

  async function search(rows, query, options = {}) {
    const terms = parseQuery(query);
    const cancelled = options.cancelled || (() => false);
    const yieldTask = options.yieldTask || (() => new Promise(resolve => setTimeout(resolve, 0)));
    if (!terms.length) return {results: [], total: 0, terms};
    const results = [], all = (1 << terms.length) - 1;
    let total = 0, work = 0, visited = 0;
    const scan = async value => {
      if (cancelled()) return null;
      const match = matcher(terms);
      for (const chunk of normalizedChunks(value)) {
        const done = match.add(chunk);
        work += chunk.work;
        if (work >= WORK_SLICE) {
          await yieldTask();
          work = 0; visited = 0;
          if (cancelled()) return null;
        }
        if (done) break;
      }
      return match.result();
    };
    // Prepared rows are chapter-contiguous. Keep only the last title summary,
    // not another full-sized lowercase copy or one cache entry per paragraph.
    let lastTitle, titleMatch;
    // Group repeated matches within a section; headings outrank its body hits.
    let section = null, representative = null;
    const flush = () => {
      if (representative) { total++; retain(results, representative); }
      representative = null;
    };
    for (const row of rows) {
      if (cancelled()) return null;
      const key = row.page + "#" + row.anchor;
      if (key !== section) { flush(); section = key; }
      if (row.title !== lastTitle) {
        titleMatch = await scan(row.title);
        if (!titleMatch) return null;
        lastTitle = row.title;
      }
      const body = row.text === lastTitle ? titleMatch : await scan(row.text);
      if (!body || cancelled()) return null;
      if ((body.mask | titleMatch.mask) === all) {
        const inBody = body.mask === all;
        const score = (inBody ? 20 : 0) + (row.heading ? 10 : 0)
          + (body.exact ? 30 : 0) + (inBody && body.prefix ? 5 : 0);
        const result = {row, score, offset: body.offset};
        if (!representative || compare(result, representative) < 0) representative = result;
      }
      visited++;
      if (visited >= 128) {
        await yieldTask();
        work = 0; visited = 0;
      }
    }
    if (cancelled()) return null;
    flush();
    return {results, total, terms};
  }

  function excerpt(value, terms) {
    // Find context in original UTF-16 coordinates even when lowercase expands
    // a scalar (e.g. U+0130). Never cut a surrogate pair at either boundary.
    const lower = value.toLowerCase();
    let at = -1;
    for (const term of terms) {
      const found = lower.indexOf(term);
      if (found >= 0 && (at < 0 || found < at)) at = found;
    }
    let original = 0, folded = 0;
    if (at >= 0) {
      for (const scalar of value) {
        if (folded >= at) break;
        folded += scalar.toLowerCase().length;
        original += scalar.length;
      }
    }
    let start = Math.max(0, original - 65), end = Math.min(value.length, start + 260);
    if (start > 0 && /[\uDC00-\uDFFF]/u.test(value[start])) start--;
    if (end < value.length && /[\uDC00-\uDFFF]/u.test(value[end])) end++;
    return (start ? "…" : "") + value.slice(start, end) + (end < value.length ? "…" : "");
  }

  function mount(document, address) {
    const get = id => document.getElementById(id);
    const input = get("query"), form = get("search-form"), status = get("status");
    const list = get("results"), previous = get("previous"), next = get("next");
    let rows;
    try { rows = prepare(JSON.parse(get("search-data").textContent)); }
    catch (error) { status.textContent = error.message; input.disabled = true; return null; }
    let generation = 0, timer, answer = null, page = 0;
    const clear = () => {
      answer = null; page = 0; list.replaceChildren(); previous.disabled = next.disabled = true;
    };
    const show = () => {
      list.replaceChildren();
      if (!answer) return;
      const start = page * LIMITS.page;
      for (const result of answer.results.slice(start, start + LIMITS.page)) {
        const row = result.row, item = document.createElement("li");
        const link = document.createElement("a"), detail = document.createElement("p");
        link.href = "./" + row.page + (row.anchor ? "#" + encodeURIComponent(row.anchor) : "");
        link.textContent = row.title + (row.anchor ? " — " + row.anchor : "");
        detail.textContent = excerpt(row.text, answer.terms);
        item.append(link, detail); list.append(item);
      }
      list.start = start + 1;
      const end = Math.min(start + LIMITS.page, answer.results.length);
      status.textContent = answer.total
        ? `Results ${start + 1}–${end} of ${answer.total} matching sections.`
          + (answer.total > answer.results.length ? ` Showing the best ${answer.results.length}; narrow your search for more.` : "")
        : "No matching sections.";
      previous.disabled = page === 0;
      next.disabled = end === answer.results.length;
    };
    const run = async token => {
      if (token !== generation) return;
      clear();
      if (!input.value.trim()) { status.textContent = "Enter words or a quoted phrase to search this book."; return; }
      status.textContent = "Searching…";
      try {
        const result = await search(rows, input.value, {cancelled: () => token !== generation});
        if (token !== generation || !result) return;
        answer = result; show();
      } catch (error) {
        if (token === generation) { clear(); status.textContent = error.message; }
      }
    };
    form.addEventListener("submit", event => {
      event.preventDefault(); clearTimeout(timer); void run(++generation);
    });
    input.addEventListener("input", () => {
      clearTimeout(timer); const token = ++generation; clear();
      status.textContent = input.value.trim() ? "Ready to search…" : "Enter words or a quoted phrase to search this book.";
      timer = setTimeout(() => { void run(token); }, 150);
    });
    input.addEventListener("keydown", event => {
      if (event.key === "Escape") {
        event.preventDefault(); clearTimeout(timer); generation++; input.value = ""; clear();
        status.textContent = "Search cleared.";
      }
    });
    previous.addEventListener("click", () => { if (page > 0) { page--; show(); } });
    next.addEventListener("click", () => {
      if (answer && (page + 1) * LIMITS.page < answer.results.length) { page++; show(); }
    });
    try {
      const query = new URL(address || document.location.href).searchParams.get("q");
      if (query !== null) { input.value = query.slice(0, LIMITS.query + 1); void run(++generation); }
    } catch (_) { /* An opaque local origin does not prevent explicit searches. */ }
    return {dispose() { generation++; clearTimeout(timer); rows = []; clear(); }};
  }

  const api = Object.freeze({LIMITS, prepare, parseQuery, search, excerpt, mount});
  if (typeof module === "object" && module.exports) module.exports = api;
  else if (typeof document === "object") mount(document);
})();
