// Literal source editing for both lightweight and native single-file workspaces.
// No parser, network, storage, inferred AST spans, or renderer-owned state here.
// Search is explicit; mutation publishes one ordinary textarea input event.
const FmdSourceSearch = (() => {
  'use strict';
  const limits = Object.freeze({sourceBytes: 32 * 1024 * 1024, queryBytes: 4096,
    replacementBytes: 64 * 1024, matches: 10000});
  const admitted = new WeakSet();
  function fail(message) {
    const error = Error(message); error.code = 'FMD_SOURCE_SEARCH'; throw error;
  }
  // Validate before Blob/TextEncoder can silently replace a lone surrogate.
  function size(text, maximum, label) {
    if (typeof text !== 'string') fail(label + ' must be text');
    if (text.length > maximum) fail(label + ' exceeds its UTF-8 byte limit');
    let bytes = 0;
    for (let i = 0; i < text.length; i++) {
      const ch = text.charCodeAt(i);
      if (ch >= 0xd800 && ch <= 0xdbff) {
        const low = text.charCodeAt(++i);
        if (!(low >= 0xdc00 && low <= 0xdfff)) fail(label + ' contains invalid Unicode');
        bytes += 4;
      } else if (ch >= 0xdc00 && ch <= 0xdfff) fail(label + ' contains invalid Unicode');
      else bytes += ch < 128 ? 1 : ch < 2048 ? 2 : 3;
      if (bytes > maximum) fail(label + ' exceeds its UTF-8 byte limit');
    }
    return bytes;
  }
  function find(source, query, matchCase = true) {
    const sourceBytes = size(source, limits.sourceBytes, 'Source');
    size(query, limits.queryBytes, 'Query');
    if (!query) fail('Enter text to find');
    if (typeof matchCase !== 'boolean') fail('Match case must be a boolean');
    // This is a literal pattern, never a user-supplied regular expression.
    // Unicode matching retains original UTF-16 offsets (lowercasing the source
    // would change offsets for characters such as U+0130). There are no pattern
    // quantifiers, alternations or empty matches that could cause backtracking.
    const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), matchCase ? 'gu' : 'giu');
    const matches = [];
    let match;
    while ((match = pattern.exec(source)) !== null) {
      if (matches.length === limits.matches) fail('More than 10,000 matches; narrow the query. No partial replace-all set is available');
      matches.push(Object.freeze({start: match.index, end: pattern.lastIndex}));
    }
    const result = Object.freeze({source, sourceBytes, query, matchCase, matches: Object.freeze(matches)});
    admitted.add(result);
    return result;
  }
  function replace(result, replacement, index = null) {
    if (!admitted.has(result)) fail('Find matches before preparing a replacement');
    const replacementBytes = size(replacement, limits.replacementBytes, 'Replacement');
    if (index !== null && (!Number.isSafeInteger(index) || index < 0 || index >= result.matches.length)) {
      fail('Select a current match to replace');
    }
    const matches = index === null ? result.matches : [result.matches[index]];
    if (!matches.length) fail('There are no matches to replace');
    let removed = 0;
    for (const match of matches) removed += size(result.source.slice(match.start, match.end), limits.sourceBytes, 'Matched text');
    const outputBytes = result.sourceBytes - removed + replacementBytes * matches.length;
    if (outputBytes > limits.sourceBytes) fail('Replacement would exceed the 32 MiB source limit; source is unchanged');
    // Calculate the complete byte budget before allocating replacement output.
    // Dollar signs, backslashes and Markdown/HTML syntax remain literal data.
    const parts = []; let offset = matches[0].start;
    for (const match of matches) {
      parts.push(result.source.slice(offset, match.start), replacement);
      offset = match.end;
    }
    const text = parts.join(''), start = matches[0].start, end = matches.at(-1).end;
    const source = result.source.slice(0, start) + text + result.source.slice(end);
    return Object.freeze({source, start, end, text, count: matches.length,
      caret: start + replacement.length, changed: source !== result.source});
  }
  return Object.freeze({limits, find, replace});
})();

const fmdSourceSearchHosts = new WeakMap();
function fmdInstallSourceSearch(doc) {
  'use strict';
  if (doc === undefined) doc = document;
  const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
  const header = doc.querySelector('body > .fmd-app-header');
  const save = header?.querySelector('#btn-save-markdown');
  if (!editor || !save) return null;
  if (fmdSourceSearchHosts.has(editor)) return fmdSourceSearchHosts.get(editor);
  const win = doc.defaultView;
  const make = (tag, text, parent) => {
    const node = doc.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (parent) parent.appendChild(node);
    return node;
  };
  let toggle = header.querySelector('#btn-source-search');
  if (!toggle) {
    toggle = make('button', 'Find / replace'); toggle.id = 'btn-source-search';
    toggle.type = 'button'; toggle.className = 'fmd-btn';
    save.parentNode.insertBefore(toggle, save);
  }
  toggle.setAttribute('aria-controls', 'fmd-source-search');
  toggle.setAttribute('aria-expanded', 'false');
  toggle.title = 'Find and replace literal Markdown source (Ctrl/Cmd+F or H in the source editor)';
  let panel = doc.querySelector('body > section#fmd-source-search');
  if (!panel) { panel = make('section', undefined, doc.body); panel.id = 'fmd-source-search'; }
  // Saved workspaces can contain a detached copy of this shell. Rebuild only
  // constant controls; queries, match sets and undo buffers are never persisted.
  panel.replaceChildren(); panel.hidden = true;
  panel.setAttribute('role', 'region'); panel.setAttribute('aria-labelledby', 'fmd-source-search-title');
  panel.style.cssText = 'position:fixed;z-index:1100;right:16px;bottom:16px;width:min(38rem,calc(100vw - 32px));max-height:80vh;overflow:auto;box-sizing:border-box;padding:20px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222);box-shadow:0 4px 24px #0003';
  make('h2', 'Find / replace source', panel).id = 'fmd-source-search-title';
  make('p', 'Literal text, including Markdown syntax and code. Find is explicit; no regular expressions. Line endings follow the source editor. One replacement batch can be undone until the next source edit.', panel);
  function textField(id, label) {
    const wrapper = make('label', label, panel), field = make('textarea', undefined, wrapper);
    field.id = id; field.rows = 2; field.spellcheck = false;
    field.setAttribute('autocomplete', 'off');
    wrapper.style.cssText = 'display:block;margin:12px 0';
    field.style.cssText = 'display:block;width:100%;box-sizing:border-box;font:inherit;padding:6px;background:var(--bg-editor,#fff);color:inherit';
    return field;
  }
  const query = textField('fmd-find-query', 'Find (up to 4 KiB UTF-8)');
  const replacement = textField('fmd-find-replacement', 'Replace with (up to 64 KiB UTF-8; empty deletes matches)');
  const caseLabel = make('label', undefined, panel), matchCase = make('input', undefined, caseLabel);
  matchCase.type = 'checkbox'; matchCase.checked = true; matchCase.id = 'fmd-find-case';
  caseLabel.appendChild(doc.createTextNode(' Match case'));
  const actions = make('div', undefined, panel);
  actions.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px;margin-top:12px';
  function button(id, label) {
    const node = make('button', label, actions); node.id = id;
    node.type = 'button'; node.className = 'fmd-btn'; return node;
  }
  const find = button('btn-find-source', 'Find');
  const previous = button('btn-find-previous', 'Previous');
  const next = button('btn-find-next', 'Next');
  const replaceOne = button('btn-replace-source', 'Replace match');
  const replaceAll = button('btn-replace-all-source', 'Replace all');
  const undo = button('btn-undo-source-replace', 'Undo replacement');
  const close = button('btn-close-source-search', 'Close');
  const status = make('p', 'Enter a query and choose Find. Shift+Enter inserts a query newline; F3 / Shift+F3 navigate matches.', panel);
  status.id = 'fmd-source-search-status'; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  status.style.cssText = 'margin-top:12px;overflow-wrap:anywhere';
  let revision = 0, composing = false, suspended = false, model = null, modelRevision = -1;
  let active = -1, history = null, ownInput = null;
  const blocked = () => suspended || composing || editor.disabled || editor.readOnly;
  const current = () => model && modelRevision === revision && model.source === editor.value
    && model.query === query.value && model.matchCase === matchCase.checked;
  function buttons() {
    const ready = !blocked(), matched = ready && current() && model.matches.length > 0;
    toggle.disabled = suspended; find.disabled = !ready;
    for (const node of [previous, next, replaceOne, replaceAll]) node.disabled = !matched;
    undo.disabled = !ready || !history;
  }
  function invalidate(message) {
    model = null; modelRevision = -1; active = -1;
    if (message) status.textContent = message;
    buttons();
  }
  function attempt(action) {
    try { action(); }
    catch (error) { status.textContent = String(error?.message ?? error).slice(0, 2048); buttons(); }
  }
  function available() {
    if (blocked()) throw Error('Finish text composition or return to the editable document before replacing source');
  }
  function showEditor() {
    if (doc.querySelector('body > #fmd-app-body')?.classList.contains('view-read')) {
      header.querySelector('#btn-toggle-view')?.click();
    }
  }
  function reveal(start) {
    if (!editor.clientWidth || !editor.clientHeight) return;
    // setSelectionRange alone does not scroll a textarea in Chromium. Measure a
    // plain-text mirror with the editor's real wrapping/font metrics; byte or
    // line-count proportions give incorrect positions with long wrapped lines.
    // Keep only the prefix through this source line, never generated HTML. The
    // temporary node is removed synchronously, including on measurement failure.
    const mirror = make('div'), style = win.getComputedStyle(editor);
    mirror.setAttribute('aria-hidden', 'true');
    mirror.style.cssText = 'position:fixed;left:0;top:0;visibility:hidden;pointer-events:none;height:auto;min-height:0;margin:0;border:0;box-sizing:border-box;overflow:visible';
    for (const key of ['fontFamily', 'fontSize', 'fontStyle', 'fontWeight', 'fontStretch', 'fontVariant',
      'letterSpacing', 'wordSpacing', 'lineHeight', 'textIndent', 'textTransform', 'tabSize',
      'paddingTop', 'paddingBottom', 'paddingLeft', 'paddingRight', 'wordBreak', 'direction', 'textAlign']) {
      mirror.style[key] = style[key];
    }
    mirror.style.width = editor.clientWidth + 'px';
    mirror.style.whiteSpace = editor.wrap === 'off' ? 'pre' : 'pre-wrap';
    mirror.style.overflowWrap = 'break-word';
    const source = editor.value, newline = source.indexOf('\n', start);
    const text = doc.createTextNode(source.slice(0, newline < 0 ? source.length : newline + 1));
    mirror.appendChild(text); doc.body.appendChild(mirror);
    try {
      const range = doc.createRange();
      range.setStart(text, start);
      range.setEnd(text, start + (source.codePointAt(start) > 0xffff ? 2 : 1));
      const rect = range.getBoundingClientRect(), origin = mirror.getBoundingClientRect();
      const top = rect.top - origin.top, left = rect.left - origin.left;
      if (top < editor.scrollTop || top + rect.height > editor.scrollTop + editor.clientHeight) {
        editor.scrollTop = Math.max(0, top - (editor.clientHeight - rect.height) / 2);
      }
      if (editor.wrap === 'off' && (left < editor.scrollLeft || left + rect.width > editor.scrollLeft + editor.clientWidth)) {
        editor.scrollLeft = Math.max(0, left - editor.clientWidth / 2);
      }
    } finally { mirror.remove(); }
  }
  function select(index) {
    available();
    if (!current() || !model.matches.length) throw Error('Source or query changed; choose Find again');
    active = (index + model.matches.length) % model.matches.length;
    const match = model.matches[active];
    showEditor(); editor.focus(); editor.setSelectionRange(match.start, match.end); reveal(match.start);
    status.textContent = `Match ${active + 1} of ${model.matches.length}. Replacements use this complete, current source snapshot.`;
    buttons();
  }
  function search(from = editor.selectionStart) {
    available(); invalidate();
    const source = editor.value, ticket = revision, needle = query.value, caseFlag = matchCase.checked;
    const found = FmdSourceSearch.find(source, needle, caseFlag);
    if (revision !== ticket || editor.value !== source || query.value !== needle || matchCase.checked !== caseFlag) {
      throw Error('Source or query changed while searching; choose Find again');
    }
    model = found; modelRevision = ticket;
    if (!found.matches.length) { status.textContent = 'No matches. Source is unchanged.'; buttons(); return; }
    const index = found.matches.findIndex(match => match.start >= from);
    select(index < 0 ? 0 : index);
  }
  function publish(plan, keepUndo = true, restore = null) {
    available();
    const before = {source: editor.value, start: editor.selectionStart, end: editor.selectionEnd,
      direction: editor.selectionDirection, scrollTop: editor.scrollTop, revision};
    const priorHistory = history;
    history = null;
    try {
      editor.setRangeText(plan.text, plan.start, plan.end, 'end');
      if (editor.value !== plan.source) throw Error('Editor did not accept the complete replacement');
    } catch (error) {
      editor.value = before.source;
      editor.setSelectionRange(before.start, before.end, before.direction); editor.scrollTop = before.scrollTop;
      if (revision === before.revision && editor.value === before.source) history = priorHistory;
      throw error;
    }
    showEditor(); editor.focus();
    if (restore) {
      editor.setSelectionRange(restore.start, restore.end, restore.direction); editor.scrollTop = restore.scrollTop;
    } else editor.setSelectionRange(plan.caret, plan.caret);
    const expectedRevision = revision + 1, event = new win.Event('input', {bubbles: true});
    ownInput = event;
    try { editor.dispatchEvent(event); } finally { ownInput = null; }
    // A synchronous subscriber may make a further edit. Never let this older
    // operation offer to undo that newer edit, including an edit-and-revert ABA.
    const unchanged = revision === expectedRevision && editor.value === plan.source;
    if (keepUndo && unchanged) history = {before, after: plan.source, revision};
    buttons();
    return unchanged;
  }
  function replace(all) {
    available();
    if (!current()) { invalidate(); throw Error('Source or query changed; choose Find again'); }
    const result = model, ticket = revision;
    if (!all) {
      const match = result.matches[active];
      if (!match || editor.selectionStart !== match.start || editor.selectionEnd !== match.end) {
        throw Error('Select a match with Next or Previous before replacing it');
      }
    }
    const plan = FmdSourceSearch.replace(result, replacement.value, all ? null : active);
    if (!current() || revision !== ticket) throw Error('Source or query changed; replacement was not applied');
    if (!plan.changed) { status.textContent = 'Replacement is identical; source is unchanged.'; return; }
    const owned = publish(plan);
    status.textContent = `Replaced ${plan.count} ${plan.count === 1 ? 'match' : 'matches'} in one source edit. `
      + (owned ? 'Undo replacement is available until the next source edit. Choose Find to search the new source.'
        : 'Further source activity disabled undo; newer edits have been retained.');
  }
  function undoReplacement() {
    available();
    const saved = history;
    if (!saved) throw Error('There is no replacement to undo');
    history = null; buttons();
    if (saved.revision !== revision || saved.after !== editor.value) throw Error('Source changed after replacement; undo would discard newer edits');
    const plan = {source: saved.before.source, text: saved.before.source, start: 0, end: editor.value.length};
    const owned = publish(plan, false, saved.before);
    status.textContent = owned ? 'Replacement undone. Original selection restored; choose Find again.'
      : 'Replacement undone; subsequent source edits have been retained.';
  }
  function open(focusReplacement = false) {
    if (suspended) return;
    panel.hidden = false; toggle.setAttribute('aria-expanded', 'true');
    showEditor(); buttons();
    const field = focusReplacement ? replacement : query;
    field.focus(); field.select();
  }
  function hide() {
    panel.hidden = true; toggle.setAttribute('aria-expanded', 'false'); editor.focus();
  }
  toggle.addEventListener('click', () => panel.hidden ? open() : hide());
  close.addEventListener('click', hide);
  find.addEventListener('click', () => attempt(() => search()));
  previous.addEventListener('click', () => attempt(() => select(active - 1)));
  next.addEventListener('click', () => attempt(() => select(active + 1)));
  replaceOne.addEventListener('click', () => attempt(() => replace(false)));
  replaceAll.addEventListener('click', () => attempt(() => replace(true)));
  undo.addEventListener('click', () => attempt(undoReplacement));
  query.addEventListener('input', () => invalidate('Query changed — choose Find again.'));
  matchCase.addEventListener('change', () => invalidate('Match case changed — choose Find again.'));
  editor.addEventListener('input', event => {
    revision++;
    if (event !== ownInput) history = null;
    invalidate('Source changed — choose Find again.');
  });
  editor.addEventListener('compositionstart', () => {
    composing = true; revision++; history = null; invalidate('Text composition in progress — find/replace is paused.');
  });
  editor.addEventListener('compositionend', () => { composing = false; revision++; invalidate('Composition finished — choose Find again.'); });
  panel.addEventListener('compositionstart', () => { composing = true; invalidate('Text composition in progress — find/replace is paused.'); });
  panel.addEventListener('compositionend', () => { composing = false; invalidate('Composition finished — choose Find again.'); });
  win.addEventListener('pagehide', () => {
    suspended = true; revision++; history = null; invalidate('Workspace suspended — choose Find again after returning.');
  });
  win.addEventListener('pageshow', () => { suspended = false; composing = false; history = null; revision++; invalidate(); });
  doc.addEventListener('keydown', event => {
    // Leave browser Find/History shortcuts alone in the preview and other
    // settings/import controls. Only the source editor and this panel own them.
    if (event.defaultPrevented || event.isComposing || composing || suspended || event.altKey
        || !(event.target === editor || panel.contains(event.target))) return;
    const command = event.ctrlKey || event.metaKey, key = event.key.toLowerCase();
    if (command && (key === 'f' || key === 'h')) {
      event.preventDefault(); if (!event.repeat) open(key === 'h');
    } else if (!command && key === 'escape' && !panel.hidden) {
      event.preventDefault(); hide();
    } else if (!command && (key === 'f3' || (key === 'enter' && event.target === query && !event.shiftKey))) {
      event.preventDefault();
      attempt(() => current() ? select(active + (event.shiftKey ? -1 : 1)) : search());
    }
  });
  buttons();
  const installed = Object.freeze({panel, open}); fmdSourceSearchHosts.set(editor, installed);
  return installed;
}
if (typeof document !== 'undefined') fmdInstallSourceSearch();
