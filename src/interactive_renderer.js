// Dependency-free fallback for self-contained interactive exports. This is not
// the Rust parser: keep its supported subset explicit and test real execution.
// All state is document-local, including definitions, heading IDs and footnotes.
// Limits are per invocation, including nested blocks, reference expansion and
// footnotes. Exhaustion throws before the controller publishes a new preview;
// it never returns truncated HTML or silently substitutes a different parser.
const FMD_CLIENT_RENDER_LIMITS = Object.freeze({
  sourceUnits: 32 * 1024 * 1024, workUnits: 32 * 1024 * 1024,
  outputUnits: 32 * 1024 * 1024, structures: 262144
});
function parseMarkdownClient(source, imageAssets) {
  'use strict';
  const limits = FMD_CLIENT_RENDER_LIMITS;
  let work = limits.workUnits, structures = limits.structures;
  function exhausted(limit) {
    const error = Error('Lightweight preview exceeds its ' + limit + ' limit. '
      + 'Markdown is unchanged and remains downloadable; use the native renderer for this document.');
    error.code = 'FMD_RENDER_LIMIT'; error.limit = limit;
    throw error;
  }
  function charge(units = 1) {
    if (units > work) exhausted('work');
    work -= units;
  }
  function allocate(units = 1) {
    if (units > structures) exhausted('structure');
    structures -= units;
  }
  function append(out, fragment) {
    if (fragment.length > limits.outputUnits - out.length) exhausted('output');
    charge(fragment.length + 1);
    return out + fragment;
  }
  // Charge searches as they advance, not the entire remaining suffix: ordinary
  // documents containing many short links must not incur a quadratic debit.
  function find(text, marker, start) {
    for (let i = start; i <= text.length - marker.length; i++) {
      charge(marker.length);
      if (text.startsWith(marker, i)) return i;
    }
    return -1;
  }
  function runEnd(text, start, ch) {
    let end = start;
    while (text[end] === ch) { charge(); end++; }
    return end;
  }
  source = String(source);
  if (source.length > limits.sourceUnits) exhausted('source');
  // Bound split() allocation before creating the line array, including CR-only
  // input. A source consisting only of newlines is not a cheap single block.
  charge(source.length);
  let lineCount = 1;
  for (let i = 0; i < source.length; i++) {
    if (source[i] === '\r') { lineCount++; if (source[i + 1] === '\n') i++; }
    else if (source[i] === '\n') lineCount++;
    if (lineCount > limits.structures) exhausted('structure');
  }
  allocate(lineCount);
  if (!imageAssets) imageAssets = new Map();
  const definitions = new Map(), notes = new Map(), usedNotes = [], headings = new Map();
  const MAX_DEPTH = 64;
  const escape = text => {
    text = String(text); charge(text.length);
    let length = text.length;
    if (length > limits.outputUnits) exhausted('output');
    return text.replace(/[&<>"']/g, ch => {
      const encoded = {'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch];
      length += encoded.length - 1;
      if (length > limits.outputUnits) exhausted('output');
      return encoded;
    });
  };
  const normalize = label => { charge(label.length); return label.trim().replace(/\s+/g, ' ').toLowerCase(); };
  // Decode before applying the URL policy, and escape once when emitting HTML.
  // Unknown named entities remain literal rather than gaining browser semantics.
  const decode = text => text.replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/gi, (all, entity) => {
    if (entity[0] !== '#') return ({amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: '\u00a0'})[entity.toLowerCase()];
    const n = entity[1].toLowerCase() === 'x' ? parseInt(entity.slice(2), 16) : Number(entity.slice(1));
    return n > 0 && n <= 0x10ffff && !(n >= 0xd800 && n <= 0xdfff) ? String.fromCodePoint(n) : '\ufffd';
  });
  const unescape = text => { charge(text.length); return decode(text.replace(/\\([!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~])/g, '$1')); };
  function safeUrl(raw, image) {
    const url = unescape(raw).trim();
    if (image && imageAssets.has(url)) {
      // An explicit binding replaces the destination, never grants permission
      // to fetch it. Revalidate saved data before emitting an attribute.
      const bound = imageAssets.get(url);
      // Reference images can expand a tiny source into a large payload. Admit
      // the bound data before validating it or copying it into an attribute.
      if (typeof bound === 'string') {
        if (bound.length > limits.outputUnits) exhausted('output');
        charge(bound.length);
      }
      return fmdEmbeddedImageUrl(bound) ? bound : null;
    }
    if (image && fmdEmbeddedImageUrl(url)) return url;
    if (/[\u0000-\u0020\u007f-\u009f]/.test(url)) return null;
    const scheme = url.match(/^([a-z][a-z0-9+.-]*):/i);
    if (scheme && !(image ? /^(https?)$/i : /^(https?|mailto|tel)$/i).test(scheme[1])) return null;
    // A backslash is a slash in browser URL parsing; do not accept its
    // protocol-relative spellings as an apparently local destination.
    if (url.includes('\\')) return null;
    return url;
  }
  function destination(text) {
    charge(text.length + 1);
    let i = 0, url = '', title = '';
    while (/\s/.test(text[i] || '') && i < text.length) i++;
    if (text[i] === '<') {
      const end = find(text, '>', i + 1);
      if (end < 0 || /[\n<]/.test(text.slice(i + 1, end))) return null;
      url = text.slice(i + 1, end); i = end + 1;
    } else {
      let nesting = 0, start = i;
      for (; i < text.length; i++) {
        if (text[i] === '\\' && i + 1 < text.length) { i++; continue; }
        if (/\s/.test(text[i]) && nesting === 0) break;
        if (text[i] === '(') nesting++;
        if (text[i] === ')' && --nesting < 0) return null;
      }
      if (nesting) return null;
      url = text.slice(start, i);
    }
    const rest = text.slice(i).trim();
    if (rest) {
      const quote = rest[0], close = quote === '(' ? ')' : quote;
      if (!['"', "'", '('].includes(quote) || rest.at(-1) !== close) return null;
      title = unescape(rest.slice(1, -1));
    }
    return {url, title};
  }
  // Match each delimiter pair once per inline buffer. Looking up every '[' in
  // an unmatched run used to rescan the full tail, even after a known miss.
  // Escaped delimiters retain the original bracketEnd semantics; code spans
  // are handled by inline(), not by this lexical index.
  function bracketIndex(text, open, close) {
    charge(text.length);
    const stack = [], pairs = new Map();
    for (let i = 0; i < text.length; i++) {
      if (text[i] === '\\') { i++; continue; }
      if (text[i] === open) { allocate(); stack.push(i); }
      else if (text[i] === close && stack.length) {
        allocate(); pairs.set(stack.pop(), i);
      }
    }
    return pairs;
  }
  function uniqueId(slug) {
    let id = slug, suffix = headings.get(slug) || 0;
    while (headings.has(id)) { charge(id.length + 1); id = slug + '-' + (++suffix); }
    allocate();
    headings.set(slug, suffix); headings.set(id, 0);
    return id;
  }
  function noteReference(label) {
    const key = normalize(label), note = notes.get(key);
    if (!note) return null;
    if (!note.number) { note.number = usedNotes.length + 1; note.id = uniqueId('fn-' + note.number); note.referenceIds = []; usedNotes.push(note); }
    allocate(); charge();
    note.references++;
    const id = uniqueId('fnref-' + note.number + (note.references > 1 ? '-' + note.references : ''));
    note.referenceIds.push(id);
    return '<sup class="footnote-ref" id="' + id + '"><a href="#' + note.id + '" role="doc-noteref">' + note.number + '</a></sup>';
  }
  function emailAddress(label) {
    // The former pair of greedy domain classes backtracked quadratically on
    // a long dotted domain ending in a second '@'. Keep the same subset with
    // independent linear checks, including its permissive trailing-dot cases.
    charge(label.length * 3 + 1);
    const at = label.indexOf('@');
    if (at < 1 || label.indexOf('@', at + 1) >= 0 || /[\s<>]/.test(label)) return false;
    const dot = label.indexOf('.', at + 2);
    return dot >= 0 && dot < label.length - 1;
  }
  function inline(text, depth = 0, links = true, footnotes = true) {
    charge(text.length + 1);
    let brackets, parentheses;
    const bracketEnd = (start, open) => {
      const pairs = open === '[' ? (brackets ??= bracketIndex(text, '[', ']'))
        : (parentheses ??= bracketIndex(text, '(', ')'));
      return pairs.get(start) ?? -1;
    };
    if (depth > MAX_DEPTH) return escape(text);
    let out = '', i = 0;
    while (i < text.length) {
      charge();
      const ch = text[i];
      if (ch === '\\' && text[i + 1] === '\n') { out = append(out, '<br>\n'); i += 2; continue; }
      if (ch === '\\' && /[!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~]/.test(text[i + 1] || '')) {
        out = append(out, escape(text[i + 1])); i += 2; continue;
      }
      if (ch === ' ') {
        const end = runEnd(text, i, ' '), run = text.slice(i, end);
        if (text[end] === '\n') { out = append(out, run.length >= 2 ? '<br>\n' : '\n'); i = end + 1; continue; }
        out = append(out, run); i = end; continue;
      }
      if (ch === '`') {
        const marker = text.slice(i, runEnd(text, i, '`'));
        let end = i + marker.length;
        while ((end = find(text, marker, end)) >= 0) {
          if (text[end - 1] !== '`' && text[end + marker.length] !== '`') break;
          end += marker.length;
        }
        if (end >= 0) {
          let code = text.slice(i + marker.length, end).replace(/\n/g, ' ');
          if (code.startsWith(' ') && code.endsWith(' ') && /[^ ]/.test(code)) code = code.slice(1, -1);
          out = append(out, '<code>' + escape(code) + '</code>'); i = end + marker.length; continue;
        }
        out = append(out, marker); i += marker.length; continue;
      }
      const image = ch === '!' && text[i + 1] === '[';
      if (ch === '[' || image) {
        const start = i + (image ? 1 : 0), end = bracketEnd(start, '[');
        if (end >= 0) {
          const label = text.slice(start + 1, end);
          if (!image && footnotes && label.startsWith('^')) {
            const reference = noteReference(label.slice(1));
            if (reference) { out = append(out, reference); i = end + 1; continue; }
          }
          let target = null, after = end + 1;
          if (text[after] === '(') {
            const close = bracketEnd(after, '(');
            if (close >= 0) { target = destination(text.slice(after + 1, close)); if (target) after = close + 1; }
          } else if (text[after] === '[') {
            const close = bracketEnd(after, '[');
            if (close >= 0) {
              target = definitions.get(normalize(text.slice(after + 1, close) || label));
              if (target) after = close + 1;
            }
          } else target = definitions.get(normalize(label));
          if (target && (image || links)) {
            const url = safeUrl(target.url, image), title = target.title ? ' title="' + escape(target.title) + '"' : '';
            const content = inline(label, depth + 1, false, false);
            if (image) {
              // Alt text is text, not generated markup, even with emphasis/code.
              const alt = content.replace(/<[^>]*>/g, '');
              out = append(out, url === null ? alt : '<img src="' + escape(url) + '" alt="' + alt + '"' + title + '>');
            } else out = append(out, url === null ? content : '<a href="' + escape(url) + '"' + title + '>' + content + '</a>');
            i = after; continue;
          }
        }
      }
      if (ch === '<' && links) {
        const end = find(text, '>', i + 1);
        if (end >= 0) {
          const label = text.slice(i + 1, end);
          const email = emailAddress(label);
          if (email || /^[a-z][a-z0-9+.-]*:/i.test(label)) {
            const url = safeUrl(email ? 'mailto:' + label : label, false);
            if (url !== null) { out = append(out, '<a href="' + escape(url) + '">' + escape(label) + '</a>'); i = end + 1; continue; }
          }
        }
      }
      if ('*_~'.includes(ch)) {
        // Never interpret intraword underscores or single tildes as delimiters.
        const run = text.slice(i, runEnd(text, i, ch));
        const size = ch === '~' ? (run.length >= 2 ? 2 : 0) : Math.min(run.length, 3);
        const marker = ch.repeat(size);
        if (size && !/\s/.test(text[i + size] || ' ') && !(ch === '_' && /[\p{L}\p{N}]/u.test(text[i - 1] || ''))) {
          let end = find(text, marker, i + size);
          while (end >= 0 && (/\s/.test(text[end - 1]) || (ch === '_' && /[\p{L}\p{N}]/u.test(text[end + size] || '')) || text[end - 1] === '\\')) end = find(text, marker, end + size);
          if (end > i + size) {
            const tags = ch === '~' ? ['del'] : size === 3 ? ['strong', 'em'] : [size === 2 ? 'strong' : 'em'];
            out = append(out, tags.map(t => '<' + t + '>').join('') + inline(text.slice(i + size, end), depth + 1, links, footnotes) + tags.slice().reverse().map(t => '</' + t + '>').join(''));
            i = end + size; continue;
          }
        }
      }
      if (ch === '&') {
        const entity = text.slice(i).match(/^&(?:#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/i);
        if (entity) { out = append(out, escape(decode(entity[0]))); i += entity[0].length; continue; }
      }
      out = append(out, escape(ch)); i++;
    }
    return out;
  }
  const fence = line => line.match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
  const rule = line => /^ {0,3}(?:(?:\* *){3,}|(?:- *){3,}|(?:_ *){3,})$/.test(line);
  const listMarker = line => {
    const m = line.match(/^( {0,3})([-+*]|\d{1,9}[.)])(?:([ \t]+)(.*)|$)/);
    if (!m) return null;
    const gap = (m[3] || ' ').replace(/\t/g, '    ');
    return {indent: m[1].length, marker: m[2], ordered: /^\d/.test(m[2]),
      start: parseInt(m[2], 10), width: m[1].length + m[2].length + (gap.length > 4 ? 1 : gap.length),
      text: (gap.length > 4 ? gap.slice(1) : '') + (m[4] || '')};
  };
  function tableCells(line) {
    charge(line.length + 1);
    line = line.trim();
    if (line.startsWith('|')) line = line.slice(1);
    if (line.endsWith('|') && !/(?:^|[^\\])(?:\\\\)*\\\|$/.test(line)) line = line.slice(0, -1);
    const cells = []; let buffer = '';
    for (let i = 0; i < line.length; i++) {
      if (line[i] === '\\' && i + 1 < line.length) { if (line[i + 1] !== '|') buffer += line[i]; buffer += line[++i]; continue; }
      if (line[i] === '|') { allocate(); cells.push(buffer.trim()); buffer = ''; }
      else buffer += line[i];
    }
    allocate(); cells.push(buffer.trim()); return cells;
  }
  function tableAlignments(header, delimiter) {
    charge(header.length + 1);
    if (!header.includes('|')) return null;
    const cells = tableCells(delimiter);
    if (cells.length !== tableCells(header).length || !cells.every(c => /^:?-+:?$/.test(c))) return null;
    return cells.map(c => c.startsWith(':') && c.endsWith(':') ? 'center' : c.endsWith(':') ? 'right' : c.startsWith(':') ? 'left' : '');
  }
  function headingTitle(text) {
    charge(text.length);
    let end = text.length;
    while (end > 0 && /[ \t]/.test(text[end - 1])) end--;
    let hashes = end;
    while (hashes > 0 && text[hashes - 1] === '#') hashes--;
    if (hashes < end && hashes > 0 && /[ \t]/.test(text[hashes - 1])) {
      while (hashes > 0 && /[ \t]/.test(text[hashes - 1])) hashes--;
      text = text.slice(0, hashes);
    }
    return text.trim();
  }
  // Reference definitions have a different grammar from inline destinations:
  // their label, destination and title can continue onto subsequent lines. A
  // cursor reads only this candidate, never joins the remaining document. All
  // speculative scans debit the same invocation-wide work budget.
  function referenceDefinition(lines, start) {
    let line = start, column = 0;
    while (lines[line][column] === ' ' && column < 3) column++;
    if (lines[line][column] !== '[' || lines[line][column + 1] === '^') return null;
    column++;
    let label = '', characters = 0;
    while (line < lines.length) {
      charge();
      const text = lines[line], ch = text[column];
      if (ch === undefined) {
        if (line + 1 >= lines.length) return null;
        charge(lines[line + 1].length + 1);
        if (!lines[line + 1].trim()) return null;
        label += '\n'; characters++; line++; column = 0;
      } else if (ch === ']') {
        column++;
        if (text[column] !== ':' || !label.trim()) return null;
        column++; break;
      } else {
        if (ch === '[') return null;
        if (ch === '\\' && column + 1 < text.length && /[!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~]/.test(text[column + 1])) {
          label += text.slice(column, column + 2); column += 2; characters += 2;
        } else {
          const width = text.codePointAt(column) > 0xffff ? 2 : 1;
          label += text.slice(column, column + width); column += width; characters++;
        }
      }
      if (characters > 999) return null;
    }
    function spaces() {
      const begin = column;
      while (lines[line][column] === ' ' || lines[line][column] === '\t') { charge(); column++; }
      return column - begin;
    }
    spaces();
    if (column === lines[line].length) {
      if (line + 1 >= lines.length) return null;
      line++; column = 0; spaces();
      if (column === lines[line].length) return null; // no blank line before a destination
    }
    const text = lines[line];
    let url;
    if (text[column] === '<') {
      const begin = ++column;
      while (column < text.length && text[column] !== '>') {
        charge();
        if (text[column] === '<') return null;
        if (text[column] === '\\' && column + 1 < text.length) column += 2;
        else column++;
      }
      if (text[column] !== '>') return null;
      url = text.slice(begin, column++); // <> is a valid empty destination
    } else {
      const begin = column;
      let nesting = 0;
      while (column < text.length && text[column] !== ' ' && text[column] !== '\t') {
        charge();
        const ch = text[column];
        if (ch.charCodeAt(0) < 32 || ch.charCodeAt(0) === 127) return null;
        if (ch === '\\' && column + 1 < text.length && /[!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~]/.test(text[column + 1])) {
          column += 2; continue;
        }
        if (ch === '(') nesting++;
        else if (ch === ')' && --nesting < 0) return null;
        column++;
      }
      if (nesting || column === begin) return null;
      url = text.slice(begin, column);
    }
    const destinationLine = line, gap = spaces();
    function titleAt(row, col) {
      const open = lines[row][col], close = open === '(' ? ')' : open;
      if (open !== '"' && open !== "'" && open !== '(') return null;
      col++;
      let title = '';
      while (row < lines.length) {
        charge();
        const text = lines[row], ch = text[col];
        if (ch === undefined) {
          if (row + 1 >= lines.length) return null;
          charge(lines[row + 1].length + 1);
          if (!lines[row + 1].trim()) return null;
          title += '\n'; row++; col = 0; continue;
        }
        if (ch === close) {
          charge(text.length - col);
          if (!/^[ \t]*$/.test(text.slice(col + 1))) return null;
          return {title: unescape(title), end: row};
        }
        if (open === '(' && ch === '(') return null;
        if (ch === '\\' && col + 1 < text.length && /[!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~]/.test(text[col + 1])) {
          title += text.slice(col, col + 2); col += 2;
        } else { title += ch; col++; }
      }
      return null;
    }
    if (column < lines[line].length) {
      if (!gap) return null;
      const result = titleAt(line, column);
      return result ? {label, url, title: result.title, consumed: result.end - start + 1} : null;
    }
    // A malformed same-line title invalidates the whole definition. A malformed
    // next-line title leaves the already-valid URL definition in place and the
    // untouched candidate line available for normal block parsing.
    if (line + 1 < lines.length) {
      line++; column = 0; spaces();
      const result = titleAt(line, column);
      if (result) return {label, url, title: result.title, consumed: result.end - start + 1};
    }
    return {label, url, title: '', consumed: destinationLine - start + 1};
  }
  // Parse container structure exactly once, before rendering any inline text.
  // A global line pre-pass cannot distinguish a definition at the start of a
  // list item from the same spelling inside its code fence or paragraph. Keep
  // definition collection on this shared block path; never reconstruct source
  // after stripping definitions, which can change list/heading/table structure.
  function parseBlocks(lines, depth = 0) {
    for (const line of lines) charge(line.length + 1);
    const out = [];
    const push = node => { allocate(); out.push(node); };
    if (depth > MAX_DEPTH) {
      push({kind: 'fallback', text: lines.join('\n')});
      return out;
    }
    let i = 0;
    const startsBlock = line => !line.trim() || fence(line) || /^ {0,3}(?:#{1,6}(?:\s|$)|>)/.test(line) || rule(line) || listMarker(line);
    while (i < lines.length) {
      const line = lines[i];
      if (!line.trim()) { i++; continue; }
      const fm = fence(line);
      if (fm && !(fm[1][0] === '`' && fm[2].includes('`'))) {
        const code = [], end = new RegExp('^ {0,3}' + fm[1][0] + '{' + fm[1].length + ',} *$');
        const indent = line.length - line.trimStart().length;
        i++;
        while (i < lines.length && !end.test(lines[i])) code.push(lines[i++].replace(new RegExp('^ {0,' + indent + '}'), ''));
        if (i < lines.length) i++;
        const lang = unescape(fm[2].trim().split(/\s+/)[0]);
        push({kind: 'code', lang, text: code.join('\n') + (code.length ? '\n' : '')}); continue;
      }
      if (/^( {4}|\t)/.test(line)) {
        const code = [];
        while (i < lines.length && (/^( {4}|\t)/.test(lines[i]) || !lines[i].trim())) code.push(lines[i++].replace(/^( {4}|\t)/, ''));
        while (code.length && !code.at(-1).trim()) code.pop();
        push({kind: 'code', lang: '', text: code.join('\n') + '\n'}); continue;
      }
      // Only block starts can own definitions: these checks are deliberately
      // absent from paragraph continuation and table/code leaf parsing.
      const foot = line.match(/^ {0,3}\[\^([^\]]+)\]:[ \t]*(.*)$/);
      if (foot) {
        const content = [foot[2]]; i++;
        while (i < lines.length) {
          if (/^( {4}|\t)/.test(lines[i])) content.push(lines[i++].replace(/^( {4}|\t)/, ''));
          else if (!lines[i].trim() && i + 1 < lines.length && /^( {4}|\t)/.test(lines[i + 1])) content.push(lines[i++]);
          else break;
        }
        const key = normalize(foot[1]);
        if (!notes.has(key)) {
          allocate();
          // Reserve before descending: a nested duplicate cannot take the
          // identity of its earlier containing definition, including cycles.
          const note = {content: [], references: 0, number: 0};
          notes.set(key, note);
          note.content = parseBlocks(content, depth + 1);
        }
        continue;
      }
      const ref = referenceDefinition(lines, i);
      if (ref) {
        const key = normalize(ref.label);
        if (!definitions.has(key)) { allocate(); definitions.set(key, {url: ref.url, title: ref.title}); }
        i += ref.consumed; continue;
      }
      let heading = line.match(/^ {0,3}(#{1,6})(?:[ \t]+(.*)|$)$/), title, level;
      if (heading) { level = heading[1].length; title = headingTitle(heading[2] || ''); i++; }
      else if (i + 1 < lines.length && /^ {0,3}(?:=+|-+) *$/.test(lines[i + 1]) && !listMarker(line)) {
        level = lines[i + 1].trim()[0] === '=' ? 1 : 2; title = line.trim(); i += 2;
      }
      if (level) { push({kind: 'heading', title, level}); continue; }
      if (rule(line)) { push({kind: 'rule'}); i++; continue; }
      if (/^ {0,3}>/.test(line)) {
        const quote = [];
        while (i < lines.length && /^ {0,3}>/.test(lines[i])) quote.push(lines[i++].replace(/^ {0,3}> ?/, ''));
        const callout = quote[0].match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*(.*)$/i);
        if (callout) {
          const name = callout[1].toLowerCase(); quote.shift();
          const label = callout[2] || name[0].toUpperCase() + name.slice(1);
          push({kind: 'callout', name, label, children: parseBlocks(quote, depth + 1)});
        } else push({kind: 'quote', children: parseBlocks(quote, depth + 1)});
        continue;
      }
      const marker = listMarker(line);
      if (marker) {
        const items = []; let loose = false;
        while (i < lines.length) {
          const next = listMarker(lines[i]);
          if (!next || next.ordered !== marker.ordered || next.indent !== marker.indent || next.marker.at(-1) !== marker.marker.at(-1)) break;
          const item = [next.text]; i++; let blank = false;
          while (i < lines.length) {
            if (!lines[i].trim()) {
              let j = i; while (j < lines.length) { charge(lines[j].length + 1); if (lines[j].trim()) break; j++; }
              const following = j < lines.length && listMarker(lines[j]);
              const continued = j < lines.length && lines[j].startsWith(' '.repeat(next.width));
              if (!continued && !(following && following.indent === marker.indent && following.ordered === marker.ordered && following.marker.at(-1) === marker.marker.at(-1))) break;
              blank = true;
              // Consume a blank run once; repeated suffix lookahead is quadratic.
              while (i < j) { item.push(''); i++; }
              continue;
            }
            if (lines[i].startsWith(' '.repeat(next.width))) { item.push(lines[i++].slice(next.width)); continue; }
            if (listMarker(lines[i]) || startsBlock(lines[i]) || blank) break;
            item.push(lines[i++]); // lazy paragraph continuation
          }
          const task = item[0].match(/^\[([ xX])\](?:\s+|$)(.*)$/);
          if (task) item[0] = task[2];
          allocate();
          items.push({task: task ? task[1] !== ' ' : null, children: parseBlocks(item, depth + 1)});
          loose ||= blank;
        }
        push({kind: 'list', ordered: marker.ordered, start: marker.start, loose, items}); continue;
      }
      const aligns = i + 1 < lines.length && tableAlignments(line, lines[i + 1]);
      if (aligns) {
        const head = tableCells(line), rows = [];
        i += 2;
        while (i < lines.length && lines[i].includes('|') && !startsBlock(lines[i])) {
          allocate(); rows.push(tableCells(lines[i++]));
        }
        push({kind: 'table', aligns, head, rows}); continue;
      }
      const paragraph = [line]; i++;
      while (i < lines.length && !startsBlock(lines[i]) && !(i + 1 < lines.length && tableAlignments(lines[i], lines[i + 1]))) paragraph.push(lines[i++]);
      push({kind: 'paragraph', text: paragraph.join('\n')});
    }
    return out;
  }
  function renderBlocks(nodes, tight = false, footnotes = true) {
    let out = '';
    for (const node of nodes) {
      charge();
      switch (node.kind) {
        case 'fallback':
          out = append(out, append(append('<pre>', escape(node.text)), '</pre>\n')); break;
        case 'code':
          out = append(out, '<pre><code' + (node.lang ? ' class="language-' + escape(node.lang) + '"' : '')
            + '>' + escape(node.text) + '</code></pre>\n'); break;
        case 'heading': {
          const content = inline(node.title, 0, true, footnotes);
          const slug = decode(content.replace(/<[^>]*>/g, '')).toLowerCase().replace(/[^\p{L}\p{N}_\s-]/gu, '').trim().replace(/\s+/g, '-') || 'section';
          out = append(out, '<h' + node.level + ' id="' + escape(uniqueId(slug)) + '">' + content + '</h' + node.level + '>\n'); break;
        }
        case 'rule': out = append(out, '<hr>\n'); break;
        case 'callout':
          out = append(out, '<aside class="callout callout-' + node.name + '"><p class="callout-title">'
            + inline(node.label, 0, true, footnotes) + '</p>\n' + renderBlocks(node.children, false, footnotes) + '</aside>\n'); break;
        case 'quote':
          out = append(out, '<blockquote>\n' + renderBlocks(node.children, false, footnotes) + '</blockquote>\n'); break;
        case 'list': {
          const tag = node.ordered ? 'ol' : 'ul';
          out = append(out, '<' + tag + (node.ordered && node.start !== 1 ? ' start="' + node.start + '"' : '') + '>\n');
          for (const item of node.items) {
            let content = renderBlocks(item.children, !node.loose, footnotes);
            if (item.task !== null) {
              const checkbox = '<input type="checkbox" disabled' + (item.task ? ' checked' : '') + '> ';
              content = content.startsWith('<p>') ? '<p>' + checkbox + content.slice(3) : checkbox + content;
            }
            out = append(out, '<li' + (item.task !== null ? ' class="task"' : '') + '>' + content.replace(/\n$/, '') + '</li>\n');
          }
          out = append(out, '</' + tag + '>\n'); break;
        }
        case 'table': {
          const row = (cells, tag) => {
            let html = '<tr>';
            for (let index = 0; index < node.aligns.length; index++) {
              const align = node.aligns[index];
              html = append(html, '<' + tag + (align ? ' style="text-align:' + align + '"' : '') + '>'
                + inline(cells[index] || '', 0, true, footnotes) + '</' + tag + '>');
            }
            return append(html, '</tr>\n');
          };
          out = append(out, '<div class="table-wrap" role="region" aria-label="Markdown table" tabindex="0"><table>\n<thead>\n'
            + row(node.head, 'th') + '</thead>\n<tbody>\n');
          for (const cells of node.rows) out = append(out, row(cells, 'td'));
          out = append(out, '</tbody>\n</table></div>\n'); break;
        }
        case 'paragraph': {
          const content = inline(node.text, 0, true, footnotes);
          out = append(out, tight ? content + '\n' : '<p>' + content + '</p>\n'); break;
        }
      }
    }
    return out;
  }
  const lines = source.replace(/\r\n?/g, '\n').replace(/\0/g, '\ufffd').split('\n');
  const body = renderBlocks(parseBlocks(lines));
  if (!usedNotes.length) return body;
  // Render the growing work queue once per note, then emit all backlinks.
  // Nested notes can append work; cycles only append references, never work.
  const renderedNotes = [];
  for (let index = 0; index < usedNotes.length; index++) renderedNotes.push(renderBlocks(usedNotes[index].content));
  let footer = '<section class="footnotes" role="doc-endnotes"><hr><ol>\n';
  usedNotes.forEach((note, index) => {
    let backlinks = '';
    for (let ref = 1; ref <= note.references; ref++) {
      const id = note.referenceIds[ref - 1];
      backlinks = append(backlinks, ' <a href="#' + id + '" class="footnote-backref" role="doc-backlink" aria-label="Back to reference ' + note.number + (ref > 1 ? '-' + ref : '') + '">↩</a>');
    }
    footer = append(footer, '<li id="' + note.id + '">' + renderedNotes[index] + backlinks + '</li>\n');
  });
  return append(append(body, footer), '</ol></section>\n');
}

// Only self-contained image payloads, never HTML, script, blob/file URLs or
// a remotely fetched URL, may be stored in an explicit workspace binding.
// Keep this separate from link admission: data images are not active links.
function fmdEmbeddedImageUrl(uri) {
  if (typeof uri !== 'string' || !/^data:image\/(?:png|jpeg|gif|webp|svg\+xml);base64,/i.test(uri)) return false;
  const data = uri.slice(uri.indexOf(',') + 1);
  return data.length > 0 && data.length % 4 === 0 && /^[A-Za-z0-9+/]*={0,2}$/.test(data) && !/[\r\n]/.test(data);
}
