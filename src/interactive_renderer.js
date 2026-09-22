// Dependency-free fallback for self-contained interactive exports. This is not
// the Rust parser: keep its supported subset explicit and test real execution.
// All state is document-local, including definitions, heading IDs and footnotes.
function parseMarkdownClient(source) {
  'use strict';
  const definitions = new Map(), notes = new Map(), usedNotes = [], headings = new Map();
  const MAX_DEPTH = 64;
  const escape = text => String(text).replace(/[&<>"']/g, ch => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[ch]);
  const normalize = label => label.trim().replace(/\s+/g, ' ').toLowerCase();
  // Decode before applying the URL policy, and escape once when emitting HTML.
  // Unknown named entities remain literal rather than gaining browser semantics.
  const decode = text => text.replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/gi, (all, entity) => {
    if (entity[0] !== '#') return ({amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: '\u00a0'})[entity.toLowerCase()];
    const n = entity[1].toLowerCase() === 'x' ? parseInt(entity.slice(2), 16) : Number(entity.slice(1));
    return n > 0 && n <= 0x10ffff && !(n >= 0xd800 && n <= 0xdfff) ? String.fromCodePoint(n) : '\ufffd';
  });
  const unescape = text => decode(text.replace(/\\([!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~])/g, '$1'));
  function safeUrl(raw, image) {
    const url = unescape(raw).trim();
    if (/[\u0000-\u0020\u007f-\u009f]/.test(url)) return null;
    const scheme = url.match(/^([a-z][a-z0-9+.-]*):/i);
    if (scheme && !(image ? /^(https?)$/i : /^(https?|mailto|tel)$/i).test(scheme[1])) return null;
    // A backslash is a slash in browser URL parsing; do not accept its
    // protocol-relative spellings as an apparently local destination.
    if (url.includes('\\')) return null;
    return url;
  }
  function destination(text) {
    let i = 0, url = '', title = '';
    while (/\s/.test(text[i] || '') && i < text.length) i++;
    if (text[i] === '<') {
      const end = text.indexOf('>', i + 1);
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
  function bracketEnd(text, start, open, close) {
    let level = 0;
    for (let i = start; i < text.length; i++) {
      if (text[i] === '\\') { i++; continue; }
      if (text[i] === open) level++;
      else if (text[i] === close && --level === 0) return i;
    }
    return -1;
  }
  function uniqueId(slug) {
    let id = slug, suffix = headings.get(slug) || 0;
    while (headings.has(id)) id = slug + '-' + (++suffix);
    headings.set(slug, suffix); headings.set(id, 0);
    return id;
  }
  function noteReference(label) {
    const key = normalize(label), note = notes.get(key);
    if (!note) return null;
    if (!note.number) { note.number = usedNotes.length + 1; note.id = uniqueId('fn-' + note.number); note.referenceIds = []; usedNotes.push(note); }
    note.references++;
    const id = uniqueId('fnref-' + note.number + (note.references > 1 ? '-' + note.references : ''));
    note.referenceIds.push(id);
    return '<sup class="footnote-ref" id="' + id + '"><a href="#' + note.id + '" role="doc-noteref">' + note.number + '</a></sup>';
  }
  function inline(text, depth = 0, links = true, footnotes = true) {
    if (depth > MAX_DEPTH) return escape(text);
    let out = '', i = 0;
    while (i < text.length) {
      const ch = text[i];
      if (ch === '\\' && text[i + 1] === '\n') { out += '<br>\n'; i += 2; continue; }
      if (ch === '\\' && /[!"#$%&'()*+,\-./:;<=>?@[\]\\^_`{|}~]/.test(text[i + 1] || '')) {
        out += escape(text[i + 1]); i += 2; continue;
      }
      if (ch === ' ') {
        const run = text.slice(i).match(/^ +/)[0];
        if (text[i + run.length] === '\n') { out += run.length >= 2 ? '<br>\n' : '\n'; i += run.length + 1; continue; }
      }
      if (ch === '`') {
        const marker = text.slice(i).match(/^`+/)[0];
        let end = i + marker.length;
        while ((end = text.indexOf(marker, end)) >= 0) {
          if (text[end - 1] !== '`' && text[end + marker.length] !== '`') break;
          end += marker.length;
        }
        if (end >= 0) {
          let code = text.slice(i + marker.length, end).replace(/\n/g, ' ');
          if (code.startsWith(' ') && code.endsWith(' ') && /[^ ]/.test(code)) code = code.slice(1, -1);
          out += '<code>' + escape(code) + '</code>'; i = end + marker.length; continue;
        }
        out += marker; i += marker.length; continue;
      }
      const image = ch === '!' && text[i + 1] === '[';
      if (ch === '[' || image) {
        const start = i + (image ? 1 : 0), end = bracketEnd(text, start, '[', ']');
        if (end >= 0) {
          const label = text.slice(start + 1, end);
          if (!image && footnotes && label.startsWith('^')) {
            const reference = noteReference(label.slice(1));
            if (reference) { out += reference; i = end + 1; continue; }
          }
          let target = null, after = end + 1;
          if (text[after] === '(') {
            const close = bracketEnd(text, after, '(', ')');
            if (close >= 0) { target = destination(text.slice(after + 1, close)); if (target) after = close + 1; }
          } else if (text[after] === '[') {
            const close = bracketEnd(text, after, '[', ']');
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
              out += url === null ? alt : '<img src="' + escape(url) + '" alt="' + alt + '"' + title + '>';
            } else out += url === null ? content : '<a href="' + escape(url) + '"' + title + '>' + content + '</a>';
            i = after; continue;
          }
        }
      }
      if (ch === '<' && links) {
        const end = text.indexOf('>', i + 1);
        if (end >= 0) {
          const label = text.slice(i + 1, end);
          const email = /^[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+$/.test(label);
          if (email || /^[a-z][a-z0-9+.-]*:/i.test(label)) {
            const url = safeUrl(email ? 'mailto:' + label : label, false);
            if (url !== null) { out += '<a href="' + escape(url) + '">' + escape(label) + '</a>'; i = end + 1; continue; }
          }
        }
      }
      if ('*_~'.includes(ch)) {
        // Never interpret intraword underscores or single tildes as delimiters.
        const run = text.slice(i).match(/^(\*+|_+|~+)/)[0];
        const size = ch === '~' ? (run.length >= 2 ? 2 : 0) : Math.min(run.length, 3);
        const marker = ch.repeat(size);
        if (size && !/\s/.test(text[i + size] || ' ') && !(ch === '_' && /[\p{L}\p{N}]/u.test(text[i - 1] || ''))) {
          let end = text.indexOf(marker, i + size);
          while (end >= 0 && (/\s/.test(text[end - 1]) || (ch === '_' && /[\p{L}\p{N}]/u.test(text[end + size] || '')) || text[end - 1] === '\\')) end = text.indexOf(marker, end + size);
          if (end > i + size) {
            const tags = ch === '~' ? ['del'] : size === 3 ? ['strong', 'em'] : [size === 2 ? 'strong' : 'em'];
            out += tags.map(t => '<' + t + '>').join('') + inline(text.slice(i + size, end), depth + 1, links, footnotes) + tags.slice().reverse().map(t => '</' + t + '>').join('');
            i = end + size; continue;
          }
        }
      }
      if (ch === '&') {
        const entity = text.slice(i).match(/^&(?:#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/i);
        if (entity) { out += escape(decode(entity[0])); i += entity[0].length; continue; }
      }
      out += escape(ch); i++;
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
    line = line.trim();
    if (line.startsWith('|')) line = line.slice(1);
    if (line.endsWith('|') && !/(?:^|[^\\])(?:\\\\)*\\\|$/.test(line)) line = line.slice(0, -1);
    const cells = [], buffer = [];
    for (let i = 0; i < line.length; i++) {
      if (line[i] === '\\' && i + 1 < line.length) { if (line[i + 1] !== '|') buffer.push(line[i]); buffer.push(line[++i]); continue; }
      if (line[i] === '|') { cells.push(buffer.join('').trim()); buffer.length = 0; }
      else buffer.push(line[i]);
    }
    cells.push(buffer.join('').trim()); return cells;
  }
  function tableAlignments(header, delimiter) {
    if (!header.includes('|')) return null;
    const cells = tableCells(delimiter);
    if (cells.length !== tableCells(header).length || !cells.every(c => /^:?-+:?$/.test(c))) return null;
    return cells.map(c => c.startsWith(':') && c.endsWith(':') ? 'center' : c.endsWith(':') ? 'right' : c.startsWith(':') ? 'left' : '');
  }
  // Definitions are removed before inline rendering, so forward references
  // work across paragraphs and containers. Fenced/indented code is never mined.
  function collect(lines, depth = 0) {
    if (depth > MAX_DEPTH) return lines;
    let out = [], i = 0;
    while (i < lines.length) {
      const fm = fence(lines[i]);
      if (fm) {
        out.push(lines[i++]);
        const end = new RegExp('^ {0,3}' + fm[1][0] + '{' + fm[1].length + ',} *$');
        while (i < lines.length) { const line = lines[i++]; out.push(line); if (end.test(line)) break; }
        continue;
      }
      if (/^ {0,3}>/.test(lines[i])) {
        const quote = [];
        while (i < lines.length && /^ {0,3}>/.test(lines[i])) quote.push(lines[i++].replace(/^ {0,3}> ?/, ''));
        out.push(...collect(quote, depth + 1).map(line => '> ' + line)); continue;
      }
      const foot = lines[i].match(/^ {0,3}\[\^([^\]]+)\]:[ \t]*(.*)$/);
      if (foot) {
        const content = [foot[2]]; i++;
        while (i < lines.length) {
          if (/^( {4}|\t)/.test(lines[i])) content.push(lines[i++].replace(/^( {4}|\t)/, ''));
          else if (!lines[i].trim() && i + 1 < lines.length && /^( {4}|\t)/.test(lines[i + 1])) content.push(lines[i++]);
          else break;
        }
        const key = normalize(foot[1]);
        if (!notes.has(key)) notes.set(key, {content: collect(content, depth + 1), references: 0, number: 0});
        out.push(''); continue;
      }
      const ref = lines[i].match(/^ {0,3}\[([^\]^][^\]]*)\]:[ \t]*(.*)$/);
      if (ref) {
        const target = destination(ref[2]);
        if (target && target.url) {
          const key = normalize(ref[1]);
          if (!definitions.has(key)) definitions.set(key, target);
          out.push(''); i++; continue;
        }
      }
      out.push(lines[i++]);
    }
    return out;
  }
  function blocks(lines, depth = 0, tight = false, footnotes = true) {
    if (depth > MAX_DEPTH) return '<pre>' + escape(lines.join('\n')) + '</pre>\n';
    let out = '', i = 0;
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
        out += '<pre><code' + (lang ? ' class="language-' + escape(lang) + '"' : '') + '>' + escape(code.join('\n') + (code.length ? '\n' : '')) + '</code></pre>\n'; continue;
      }
      if (/^( {4}|\t)/.test(line)) {
        const code = [];
        while (i < lines.length && (/^( {4}|\t)/.test(lines[i]) || !lines[i].trim())) code.push(lines[i++].replace(/^( {4}|\t)/, ''));
        while (code.length && !code.at(-1).trim()) code.pop();
        out += '<pre><code>' + escape(code.join('\n') + '\n') + '</code></pre>\n'; continue;
      }
      let heading = line.match(/^ {0,3}(#{1,6})(?:[ \t]+(.*)|$)$/), title, level;
      if (heading) { level = heading[1].length; title = (heading[2] || '').replace(/[ \t]+#+[ \t]*$/, '').trim(); i++; }
      else if (i + 1 < lines.length && /^ {0,3}(?:=+|-+) *$/.test(lines[i + 1]) && !listMarker(line)) {
        level = lines[i + 1].trim()[0] === '=' ? 1 : 2; title = line.trim(); i += 2;
      }
      if (level) {
        const content = inline(title, 0, true, footnotes);
        const slug = decode(content.replace(/<[^>]*>/g, '')).toLowerCase().replace(/[^\p{L}\p{N}_\s-]/gu, '').trim().replace(/\s+/g, '-') || 'section';
        const id = uniqueId(slug);
        out += '<h' + level + ' id="' + escape(id) + '">' + content + '</h' + level + '>\n'; continue;
      }
      if (rule(line)) { out += '<hr>\n'; i++; continue; }
      if (/^ {0,3}>/.test(line)) {
        const quote = [];
        while (i < lines.length && /^ {0,3}>/.test(lines[i])) quote.push(lines[i++].replace(/^ {0,3}> ?/, ''));
        const callout = quote[0].match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*(.*)$/i);
        if (callout) {
          const kind = callout[1].toLowerCase(); quote.shift();
          const label = callout[2] || kind[0].toUpperCase() + kind.slice(1);
          out += '<aside class="callout callout-' + kind + '"><p class="callout-title">' + inline(label, 0, true, footnotes) + '</p>\n' + blocks(quote, depth + 1, false, footnotes) + '</aside>\n';
        } else out += '<blockquote>\n' + blocks(quote, depth + 1, false, footnotes) + '</blockquote>\n';
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
              let j = i; while (j < lines.length && !lines[j].trim()) j++;
              const following = j < lines.length && listMarker(lines[j]);
              const continued = j < lines.length && lines[j].startsWith(' '.repeat(next.width));
              if (!continued && !(following && following.indent === marker.indent && following.ordered === marker.ordered && following.marker.at(-1) === marker.marker.at(-1))) break;
              blank = true; item.push(''); i++; continue;
            }
            if (lines[i].startsWith(' '.repeat(next.width))) { item.push(lines[i++].slice(next.width)); continue; }
            if (listMarker(lines[i]) || startsBlock(lines[i]) || blank) break;
            item.push(lines[i++]); // lazy paragraph continuation
          }
          loose ||= blank; items.push(item);
        }
        const tag = marker.ordered ? 'ol' : 'ul';
        out += '<' + tag + (marker.ordered && marker.start !== 1 ? ' start="' + marker.start + '"' : '') + '>\n';
        for (const item of items) {
          const task = item[0].match(/^\[([ xX])\](?:\s+|$)(.*)$/);
          if (task) item[0] = task[2];
          let content = blocks(item, depth + 1, !loose, footnotes);
          if (task) {
            const checkbox = '<input type="checkbox" disabled' + (task[1] !== ' ' ? ' checked' : '') + '> ';
            content = content.startsWith('<p>') ? '<p>' + checkbox + content.slice(3) : checkbox + content;
          }
          out += '<li' + (task ? ' class="task"' : '') + '>' + content.replace(/\n$/, '') + '</li>\n';
        }
        out += '</' + tag + '>\n'; continue;
      }
      const aligns = i + 1 < lines.length && tableAlignments(line, lines[i + 1]);
      if (aligns) {
        const row = (cells, tag) => '<tr>' + aligns.map((align, index) => '<' + tag + (align ? ' style="text-align:' + align + '"' : '') + '>' + inline(cells[index] || '', 0, true, footnotes) + '</' + tag + '>').join('') + '</tr>\n';
        out += '<div class="table-wrap" role="region" aria-label="Markdown table" tabindex="0"><table>\n<thead>\n' + row(tableCells(line), 'th') + '</thead>\n<tbody>\n';
        i += 2;
        while (i < lines.length && lines[i].includes('|') && !startsBlock(lines[i])) out += row(tableCells(lines[i++]), 'td');
        out += '</tbody>\n</table></div>\n'; continue;
      }
      const paragraph = [line]; i++;
      while (i < lines.length && !startsBlock(lines[i]) && !(i + 1 < lines.length && tableAlignments(lines[i], lines[i + 1]))) paragraph.push(lines[i++]);
      const content = inline(paragraph.join('\n'), 0, true, footnotes);
      out += tight ? content + '\n' : '<p>' + content + '</p>\n';
    }
    return out;
  }
  const lines = String(source).replace(/\r\n?/g, '\n').replace(/\0/g, '\ufffd').split('\n');
  const body = blocks(collect(lines));
  if (!usedNotes.length) return body;
  // Render the growing work queue once per note, then emit all backlinks.
  // Nested notes can append work; cycles only append references, never work.
  const renderedNotes = [];
  for (let index = 0; index < usedNotes.length; index++) renderedNotes.push(blocks(usedNotes[index].content));
  let footer = '<section class="footnotes" role="doc-endnotes"><hr><ol>\n';
  usedNotes.forEach((note, index) => {
    let backlinks = '';
    for (let ref = 1; ref <= note.references; ref++) {
      const id = note.referenceIds[ref - 1];
      backlinks += ' <a href="#' + id + '" class="footnote-backref" role="doc-backlink" aria-label="Back to reference ' + note.number + (ref > 1 ? '-' + ref : '') + '">↩</a>';
    }
    footer += '<li id="' + note.id + '">' + renderedNotes[index] + backlinks + '</li>\n';
  });
  return body + footer + '</ol></section>\n';
}
