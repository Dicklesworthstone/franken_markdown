import { createWorkerRenderer, DOCUMENT_SOURCE_LIMIT, DOCUMENT_OUTPUT_LIMIT } from '../document_worker.mjs';

// The parser/differ lives only in the Rust/WASM renderer. This controller owns
// two source revisions, the isolated worker, and explicit local downloads.
export function installRevisionReview(document, { rendererFactory = createWorkerRenderer } = {}) {
  const window = document.defaultView;
  const get = id => {
    const node = document.getElementById(id);
    if (!node) throw new Error('Missing revision review control: ' + id);
    return node;
  };
  const compareButton = get('compare'), cancelButton = get('cancel-review'), swapButton = get('swap-revisions');
  const status = get('review-status'), results = get('review-results'), summary = get('review-summary');
  const preview = get('review-preview'), findings = get('review-findings');
  const htmlButton = get('download-review-html'), jsonButton = get('download-review-json');
  const sides = ['before', 'after'].map(id => ({id, editor: get(id + '-source'), name: get(id + '-name'),
    picker: get(id + '-file'), save: get('save-' + id), anchor: null, composing: false}));
  for (const side of sides) side.anchor = {source: side.editor.value, view: side.editor.value};
  let revision = 0, pending = null, reading = null, latest = null, suspended = false, disposed = false;
  const listeners = [], urls = new Map();
  const on = (node, kind, handler) => { node.addEventListener(kind, handler); listeners.push([node, kind, handler]); };
  const source = side => side.editor.value === side.anchor.view ? side.anchor.source : side.editor.value;
  const snapshot = () => ({revision, sources: sides.map(source), names: sides.map(side => side.name.value),
    anchors: sides.map(side => side.anchor)});
  const current = saved => !disposed && !suspended && saved && saved.revision === revision
    && sides.every((side, index) => !side.composing && source(side) === saved.sources[index]
      && side.name.value === saved.names[index] && side.anchor === saved.anchors[index]);
  function textSize(value, maximum, label) {
    if (typeof value !== 'string' || value.length > maximum) throw Error(label + ' exceeds its UTF-8 byte limit');
    let length = 0;
    for (const character of value) {
      const code = character.codePointAt(0);
      if (code >= 0xd800 && code <= 0xdfff) throw Error(label + ' contains an unpaired surrogate');
      length += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
      if (length > maximum) throw Error(label + ' exceeds its UTF-8 byte limit');
    }
    return length;
  }
  function refresh() {
    const paused = disposed || suspended || sides.some(side => side.composing);
    compareButton.disabled = paused || pending !== null || reading !== null;
    swapButton.disabled = paused || reading !== null;
    cancelButton.hidden = !pending && !reading;
    htmlButton.disabled = jsonButton.disabled = paused || !!pending || !!reading || !current(latest);
    for (const side of sides) {
      side.save.disabled = paused;
      side.picker.disabled = paused;
    }
    results.setAttribute('aria-busy', String(pending !== null));
  }
  function revokeDownloads() {
    for (const [url, timer] of urls) { window.clearTimeout(timer); URL.revokeObjectURL(url); }
    urls.clear();
  }
  function clearResult() {
    latest = null; summary.replaceChildren(); findings.textContent = ''; results.hidden = true;
    preview.removeAttribute('srcdoc'); preview.removeAttribute('src');
    // Invalidation must not revoke a URL the browser is still consuming from a
    // prior explicit download. Those private copies expire or retire on exit.
  }
  function stop(message) {
    const operation = pending; pending = null;
    operation?.renderer.dispose();
    const read = reading; reading = null; read?.cancel();
    if (message) status.textContent = message;
  }
  function invalidate(message) {
    revision++; stop(); clearResult(); status.textContent = message; refresh();
  }
  function message(error) { return String(error?.message ?? error).slice(0, 2048); }
  function safeName(value, extension) {
    let name = Array.from(value || 'document').slice(0, 80).join('')
      .replace(/[<>:"/\\|?*\u0000-\u001f\u007f]/g, '_').trim().replace(/[. ]+$/, '');
    if (!name || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name)) name = 'document';
    return name + '.' + extension;
  }
  function download(content, mime, name, isCurrent) {
    let url = null, link = null;
    try {
      const blob = new Blob([content], {type: mime, endings: 'transparent'});
      url = URL.createObjectURL(blob);
      // Recheck after potentially expensive serialization/allocation, before
      // the user-authorized download activation. No old result is replayed.
      if (!isCurrent()) throw Error('Revisions changed; prepare the current data again');
      link = document.createElement('a'); link.hidden = true; link.href = url; link.download = name;
      document.body.appendChild(link); link.click();
      const owned = url;
      urls.set(owned, window.setTimeout(() => { URL.revokeObjectURL(owned); urls.delete(owned); }, 30000));
      url = null;
      status.textContent = 'Download started — check your downloads';
    } finally { link?.remove(); if (url) URL.revokeObjectURL(url); }
  }
  function safeHtml(bytes) {
    const html = new TextDecoder('utf-8', {fatal: true}).decode(bytes);
    const head = /<head(?:\s[^>]*)?>/i;
    if (!head.test(html)) throw Error('Native comparison is missing its HTML head');
    // This policy also travels with the HTML download. Never place the native
    // document in the application's DOM or give its iframe script permission.
    const policy = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'";
    const guarded = html.replace(head, match => match + '<meta http-equiv="Content-Security-Policy" content="' + policy + '">');
    textSize(guarded, DOCUMENT_OUTPUT_LIMIT, 'Comparison HTML');
    return guarded;
  }
  async function compare() {
    if (pending || reading || disposed || suspended || sides.some(side => side.composing)) return;
    let operation;
    try {
      const saved = snapshot();
      for (let index = 0; index < 2; index++) {
        textSize(saved.sources[index], DOCUMENT_SOURCE_LIMIT, 'Markdown');
        textSize(saved.names[index], 4096, 'Revision label');
      }
      clearResult();
      const renderer = rendererFactory();
      operation = {...saved, renderer}; pending = operation;
      status.textContent = 'Comparing revisions in the native worker…'; refresh();
      const value = await renderer.compare(saved.sources[0], saved.sources[1], {
        oldName: saved.names[0], newName: saved.names[1],
      });
      if (pending !== operation || !current(operation)) return;
      const html = safeHtml(value.html.bytes), report = value.report;
      const content = document.createDocumentFragment();
      for (const [label, number] of [
        ['Unchanged blocks', report.stats.unchanged_blocks], ['Inserted blocks', report.stats.inserted_blocks],
        ['Deleted blocks', report.stats.deleted_blocks], ['Modified blocks', report.stats.modified_blocks],
        ['Words inserted', report.stats.words_inserted], ['Words deleted', report.stats.words_deleted],
        ['Structural similarity', (report.stats.similarity_ratio * 100).toFixed(1) + '%'],
      ]) {
        const group = document.createElement('div'), term = document.createElement('dt'), definition = document.createElement('dd');
        term.textContent = label; definition.textContent = String(number);
        group.append(term, definition); content.appendChild(group);
      }
      if (!current(operation)) return;
      latest = {...saved, html, json: value.json.bytes};
      summary.replaceChildren(content);
      findings.textContent = value.html.diagnostics.length
        ? 'Renderer diagnostics: ' + value.html.diagnostics.slice(0, 5).map(item => message(item)).join('; ')
        : 'No renderer diagnostics. This compares parsed Markdown structure, not exact source bytes.';
      preview.setAttribute('sandbox', ''); preview.setAttribute('referrerpolicy', 'no-referrer');
      preview.srcdoc = html; results.hidden = false;
      status.textContent = 'Comparison ready for the captured revisions. Editing either revision retires this result.';
    } catch (error) {
      if (!operation || pending === operation) status.textContent = 'Comparison failed: ' + message(error) + ' — source downloads remain available.';
    } finally {
      operation?.renderer.dispose();
      if (operation && pending === operation) {
        pending = null;
        if (!latest && !current(operation)) status.textContent = 'Comparison discarded: revisions changed. Compare again.';
      }
      refresh();
    }
  }
  function openFile(side) {
    const files = Array.from(side.picker.files ?? []);
    if (!files?.length) return;
    const file = files[0]; side.picker.value = '';
    if (disposed || suspended || sides.some(item => item.composing)) return;
    let operation;
    try {
      if (files.length !== 1 || !/\.(md|markdown|txt)$/i.test(file.name)
          || !Number.isSafeInteger(file.size) || file.size < 0 || file.size > DOCUMENT_SOURCE_LIMIT) {
        throw Error('Choose one .md, .markdown or .txt UTF-8 file of at most 4 MiB');
      }
      textSize(file.name, 4096, 'Filename');
      // New selections explicitly supersede prior reads and comparisons, but
      // do not replace source until decoding, confirmation and identity pass.
      invalidate('Reading the selected revision…');
      const saved = snapshot(), reader = new window.FileReader();
      let timer = null, done = false;
      const finish = () => {
        if (done) return;
        done = true; window.clearTimeout(timer);
        reader.onload = reader.onerror = reader.onabort = null;
        if (reading === operation) reading = null;
        if (reader.readyState === 1) reader.abort();
        refresh();
      };
      operation = {...saved, cancel: finish}; reading = operation;
      const active = () => reading === operation && current(operation);
      reader.onerror = () => { if (active()) status.textContent = 'Could not read the selected file; existing source is unchanged.'; finish(); };
      reader.onabort = () => { if (active()) status.textContent = 'File opening cancelled.'; finish(); };
      reader.onload = () => {
        try {
          if (!active()) {
            if (reading === operation) status.textContent = 'File opening discarded: revisions changed; select the file again.';
            return;
          }
          const bytes = reader.result;
          if (!(bytes instanceof ArrayBuffer) || bytes.byteLength !== file.size) throw Error('File size changed while reading');
          const decoded = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true}).decode(bytes);
          textSize(decoded, DOCUMENT_SOURCE_LIMIT, 'Markdown');
          const prior = source(side);
          if (prior && decoded !== prior && !window.confirm('Replace the ' + side.id + ' revision? Download its source first to keep a separate copy.')) {
            status.textContent = 'File opening declined; existing source is unchanged.'; return;
          }
          if (!active()) {
            if (reading === operation) status.textContent = 'File opening discarded: revisions changed during confirmation.';
            return;
          }
          const beforeView = side.editor.value, beforeAnchor = side.anchor, beforeName = side.name.value;
          try {
            side.editor.value = decoded;
            if (side.editor.value !== decoded.replace(/\r\n?/g, '\n')) throw Error('Editor did not accept the complete source');
            side.anchor = {source: decoded, view: side.editor.value}; side.name.value = file.name;
          } catch (error) {
            side.editor.value = beforeView; side.anchor = beforeAnchor; side.name.value = beforeName; throw error;
          }
          revision++; clearResult(); status.textContent = 'Opened ' + side.id + ' revision. Compare when both sides are ready.';
        } catch (error) { if (reading === operation) status.textContent = 'File not opened: ' + message(error); }
        finally { finish(); }
      };
      timer = window.setTimeout(() => {
        if (active()) status.textContent = 'File read timed out; existing source is unchanged.';
        finish();
      }, 10000);
      refresh(); reader.readAsArrayBuffer(file);
    } catch (error) { operation?.cancel(); status.textContent = 'File not opened: ' + message(error); refresh(); }
  }
  on(compareButton, 'click', compare);
  on(cancelButton, 'click', () => invalidate('Operation cancelled — both source revisions are unchanged.'));
  on(swapButton, 'click', () => {
    if (disposed || suspended || reading || sides.some(side => side.composing)) return;
    const saved = snapshot(); invalidate('Revisions swapped; compare again.');
    sides.forEach((side, index) => {
      const value = saved.sources[1 - index]; side.editor.value = value;
      side.anchor = {source: value, view: side.editor.value}; side.name.value = saved.names[1 - index];
    });
  });
  for (const side of sides) {
    for (const node of [side.editor, side.name]) on(node, 'input', () => invalidate('Revisions changed — compare again.'));
    for (const node of [side.editor, side.name]) {
      on(node, 'compositionstart', () => { side.composing = true; invalidate('Text composition in progress.'); });
      on(node, 'compositionend', () => { side.composing = false; invalidate('Composition finished — compare again.'); });
    }
    on(side.picker, 'change', () => openFile(side));
    on(side.save, 'click', () => {
      try {
        const saved = snapshot(); if (!current(saved)) return;
        const index = sides.indexOf(side), value = saved.sources[index];
        textSize(value, DOCUMENT_SOURCE_LIMIT, 'Markdown');
        download(value, 'text/markdown;charset=utf-8', safeName(saved.names[index], 'md'), () => current(saved));
      } catch (error) { status.textContent = 'Source not downloaded: ' + message(error); }
    });
  }
  for (const [button, kind] of [[htmlButton, 'html'], [jsonButton, 'json']]) on(button, 'click', () => {
    if (!current(latest) || reading || pending) { invalidate('Revisions changed — compare again before downloading.'); return; }
    const saved = latest;
    try { download(saved[kind], kind === 'html' ? 'text/html;charset=utf-8' : 'application/json',
      'revision-comparison.' + kind, () => latest === saved && current(saved)); }
    catch (error) { status.textContent = 'Comparison not downloaded: ' + message(error); }
  });
  on(window, 'pagehide', () => { suspended = true; invalidate('Page suspended; sources are retained, comparison must be rerun.'); revokeDownloads(); });
  on(window, 'pageshow', () => { suspended = false; sides.forEach(side => { side.composing = false; }); refresh(); });
  on(window, 'beforeunload', event => {
    if (sides.some(side => source(side) !== '')) { event.preventDefault(); event.returnValue = ''; }
  });
  refresh();
  return Object.freeze({ dispose() {
    if (disposed) return;
    disposed = true; invalidate('Review disposed; original files were not changed.'); revokeDownloads();
    for (const [node, kind, handler] of listeners) node.removeEventListener(kind, handler);
  }});
}
