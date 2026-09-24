(function() {
  const editor = document.getElementById('fmd-editor');
  const originalSource = JSON.parse(document.querySelector('body > script#fmd-raw-source[type="application/json"]').textContent);
  // Textarea markup loses an initial newline and normalizes HTML controls.
  // Initialize from the lossless data block before computing editor statistics.
  editor.value = originalSource;
  // The textarea API itself normalizes CR and CRLF to LF. Keep untouched
  // source bytes (and exact undo) separate from that normalized editing view.
  const assetData = document.querySelector('body > script#fmd-image-assets[type="application/json"]');
  const imageAssets = new Map();
  const assetEntries = assetData ? JSON.parse(assetData.textContent) : [];
  if (Array.isArray(assetEntries)) {
    for (const entry of assetEntries) {
      if (!Array.isArray(entry) || entry.length !== 2 || typeof entry[0] !== 'string') continue;
      const key = entry[0].trim();
      // First entry owns the key. The renderer validates each value as image
      // data; a damaged binding must never silently authorize a URL fetch.
      if (!imageAssets.has(key)) imageAssets.set(key, entry[1]);
    }
  }
  const originalEditorValue = editor.value;
  let sourceAnchor = {source: originalSource, view: originalEditorValue};
  const currentSource = () => editor.value === sourceAnchor.view ? sourceAnchor.source : editor.value;
  const preview = document.getElementById('fmd-content');
  const body = document.getElementById('fmd-app-body');
  const candidate = Object.getOwnPropertyDescriptor(window, '__fmdNativeRuntime')?.value;
  const native = candidate?.version === 1 && typeof candidate.render === 'function'
    && typeof candidate.pdf === 'function' ? candidate : null;
  const lineCountBadge = document.getElementById('source-line-count');
  const statsDrawer = document.querySelector('body > #stats-drawer');

  // Stats elements
  const statWords = statsDrawer.querySelector('#stat-words');
  const statChars = statsDrawer.querySelector('#stat-chars');
  const statReadTime = statsDrawer.querySelector('#stat-read-time');
  const statReadability = statsDrawer.querySelector('#stat-readability');

  const saveStatus = document.getElementById('fmd-save-status');
  const workerExports = native?.exportMode === 'worker' && typeof native.exportDocument === 'function';
  let pendingExport = null, exportRevision = 0, exportControls = null;
  const publicationFormats = Object.freeze({
    html: {mime: 'text/html;charset=utf-8', extension: 'published.html', button: 'btn-publish-html'},
    pdf: {mime: 'application/pdf', extension: 'pdf', button: 'btn-export-pdf'},
    epub: {mime: 'application/epub+zip', extension: 'epub', button: 'btn-export-epub'},
    svg: {mime: 'image/svg+xml', extension: 'svg', button: 'btn-export-svg'},
  });
  // Keep the full initial renderer output available for exact undo, including
  // diagrams and other features outside the offline JavaScript subset.
  const originalRendered = preview.innerHTML;
  let lastRenderedSource = native ? null : originalSource;
  const storedScale = parseFloat(document.documentElement.style.getPropertyValue('--fmd-base')) / 16;
  let currentScale = Number.isFinite(storedScale) ? Math.min(2, Math.max(0.7, storedScale)) : 1;
  let viewMode = body.classList.contains('view-read') ? 'read' : 'split';
  let settingsBaseline = null, documentLab = null;
  const isModified = () => currentSource() !== originalSource
    || (settingsBaseline !== null && JSON.stringify(native.settings) !== settingsBaseline);
  const modifiedNotice = () => isModified() ? 'Modified — download to keep changes' : '';
  const displaySettings = () => ({scale: currentScale,
    theme: document.body.classList.contains('theme-dark') ? 'dark'
      : document.body.classList.contains('theme-light') ? 'light' : undefined});

  // Update line count and stats
  function updateStats() {
    const text = editor.value;
    const lines = text.split('\n').length;
    lineCountBadge.textContent = `Lines: ${lines}`;

    const words = (text.match(/\S+/g) || []).length;
    const chars = text.length;
    const readTimeMinutes = words === 0 ? 0 : Math.max(1, Math.ceil(words / 220));

    // Syllable heuristic for Flesch score
    const sentences = Math.max(1, (text.match(/[.!?]+(\s|$)/g) || []).length);
    let syllables = 0;
    const tokens = text.toLowerCase().match(/[a-z]+/g) || [];
    for (const tok of tokens) {
      let count = (tok.match(/[aeiouy]+/g) || []).length;
      if (tok.endsWith('e') && !tok.endsWith('le') && count > 1) count--;
      syllables += Math.max(1, count);
    }
    const flesch = Math.round(206.835 - 1.015 * (words / sentences) - 84.6 * (syllables / Math.max(1, words)));
    const clampedFlesch = Math.max(0, Math.min(100, isNaN(flesch) ? 70 : flesch));

    statWords.textContent = words.toLocaleString();
    statChars.textContent = chars.toLocaleString();
    statReadTime.textContent = `${readTimeMinutes}m`;
    statReadability.textContent = `${clampedFlesch}/100`;
  }

  // fmd-async-preview-v1: one revision-aware path for debounce and Save HTML.
  // Lightweight previews remain synchronous. Native worker results must commit
  // before a revision is considered rendered or an editable file is exported.
  let debounceTimer = null, renderRevision = 0, pendingRender = null;
  let previewSuspended = false, previewComposing = false, pendingSave = null;
  function invalidatePreview() {
    renderRevision++;
    pendingRender = null;
    native?.invalidatePreview?.();
    preview.setAttribute?.('aria-busy', 'false');
    if (native?.previewMode === 'worker') lastRenderedSource = null;
  }
  function renderCurrent() {
    clearTimeout(debounceTimer); debounceTimer = null;
    updateStats();
    if (previewSuspended || previewComposing) return false;
    const source = currentSource(), display = displaySettings(), settings = native?.settings;
    const view = JSON.stringify(display);
    if (pendingRender?.source === source && pendingRender.view === view && pendingRender.settings === settings) {
      return pendingRender.promise;
    }
    if (source === lastRenderedSource) return true;
    const revision = ++renderRevision;
    const current = () => revision === renderRevision && !previewSuspended && !previewComposing
      && currentSource() === source && JSON.stringify(displaySettings()) === view && native?.settings === settings;
    const complete = committed => {
      if (committed === false || !current()) return false;
      lastRenderedSource = source;
      if (native) saveStatus.textContent = nativeNotice() || modifiedNotice();
      return true;
    };
    if (!native) {
      preview.innerHTML = source === originalSource ? originalRendered : parseMarkdownClient(source, imageAssets);
      return complete(true);
    }
    const result = native.render(source, preview, display, current);
    if (!result || typeof result.then !== 'function') return complete(result);
    const operation = {source, view, settings, promise: null};
    pendingRender = operation;
    preview.setAttribute?.('aria-busy', 'true');
    saveStatus.textContent = 'Rendering preview in background…';
    operation.promise = Promise.resolve(result).then(complete, error => {
      if (!current() || error?.code === 'PREVIEW_SUPERSEDED') return false;
      throw error;
    }).finally(() => {
      if (pendingRender === operation) {
        pendingRender = null;
        preview.setAttribute?.('aria-busy', 'false');
      }
    });
    return operation.promise;
  }
  function attempt(action) {
    const report = error => {
      saveStatus.textContent = 'Unable to complete: ' + String(error?.message || error).slice(0, 2048);
      if (native?.previewMode === 'worker') saveStatus.textContent += ' — Restart preview to retry; Markdown remains downloadable.';
    };
    try {
      const result = action();
      if (result && typeof result.then === 'function') result.catch(report);
    } catch (error) { report(error); }
  }
  function nativeNotice(findings = native?.diagnostics ?? []) {
    return findings.length ? `Renderer diagnostics (${findings.length}): `
      + findings.slice(0, 3).map(item => String(item?.message ?? item).slice(0, 300)).join('; ') : '';
  }
  function refreshNative() {
    if (!native) return;
    invalidatePreview();
    lastRenderedSource = null;
    attempt(renderCurrent);
  }
  window.addEventListener('fmd-native-ready', refreshNative);
  window.addEventListener('fmd-native-error', event => {
    if (native) saveStatus.textContent = String(event.detail).slice(0, 2048);
  });
  // Also handle an engine that settled before this controller was evaluated.
  // Read currentSource at completion: typing during startup is never lost.
  if (native?.ready) native.ready.then(() => attempt(renderCurrent), error => {
    saveStatus.textContent = 'Native runtime failed: ' + String(error?.message ?? error).slice(0, 2048);
  });
  editor.addEventListener('input', () => {
    cancelDocumentExport('Export cancelled: source changed');
    documentLab?.invalidate();
    invalidatePreview();
    updateStats();
    saveStatus.textContent = modifiedNotice();
    clearTimeout(debounceTimer);
    if (!previewComposing && !previewSuspended) debounceTimer = setTimeout(() => attempt(renderCurrent), 150);
  });

  editor.addEventListener('compositionstart', () => {
    cancelDocumentExport('Export cancelled: text composition started');
    previewComposing = true;
    documentLab?.invalidate('Text composition started — analyze again when finished.');
    invalidatePreview();
    clearTimeout(debounceTimer); debounceTimer = null;
  });
  editor.addEventListener('compositionend', () => {
    previewComposing = false; invalidatePreview();
    documentLab?.refresh();
    if (!previewSuspended) debounceTimer = setTimeout(() => attempt(renderCurrent), 150);
  });

  // These are explicit local downloads, never a network upload or an implicit
  // write to browser storage. A download request is not proof the user saved it.
  const downloadUrls = new Map();
  function filename(extension) {
    let stem = Array.from(document.title || 'document').slice(0, 80).join('')
      .replace(/[<>:"/\\|?*\u0000-\u001f\u007f]/g, '_').trim().replace(/[. ]+$/, '');
    if (!stem || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(stem)) stem = 'document';
    return stem + '.' + extension;
  }
  function download(content, mime, extension, name = filename(extension)) {
    let url = null, anchor = null;
    try {
      const blob = new Blob([content], {type: mime, endings: 'transparent'});
      url = URL.createObjectURL(blob);
      anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = name;
      anchor.hidden = true;
      document.body.appendChild(anchor);
      anchor.click();
      // Defer revocation until the browser has consumed the navigation. Keep
      // multiple downloads independent and also clean them up on page exit.
      const pendingUrl = url;
      downloadUrls.set(pendingUrl, setTimeout(() => {
        URL.revokeObjectURL(pendingUrl);
        downloadUrls.delete(pendingUrl);
      }, 30000));
      url = null;
      saveStatus.textContent = 'Download started — check your downloads';
    } finally {
      if (anchor) anchor.remove();
      if (url) URL.revokeObjectURL(url);
    }
  }
  function exportPdf() {
    if (workerExports) return exportDocument('pdf');
    if (native) {
      // Export current source through the shared PDF engine, not a print of
      // the iframe's last loaded revision. Viewing zoom is not PDF typography.
      const bytes = native.pdf(currentSource());
      download(bytes, 'application/pdf', 'pdf');
      const notice = nativeNotice();
      if (notice) saveStatus.textContent += ' — ' + notice;
    } else {
      renderCurrent();
      window.print();
    }
  }
  function saveMarkdown() {
    download(currentSource(), 'text/markdown;charset=utf-8', 'md');
  }
  function saveHtml() {
    if (pendingSave?.revision === renderRevision) return pendingSave.promise;
    const source = currentSource(), rendered = renderCurrent(), revision = renderRevision;
    const save = committed => {
      if (committed === false || revision !== renderRevision || currentSource() !== source) return;
      serializeWorkspace(source);
    };
    if (!rendered || typeof rendered.then !== 'function') return save(rendered);
    const operation = {revision, promise: null};
    pendingSave = operation;
    operation.promise = rendered.then(save).finally(() => {
      if (pendingSave === operation) pendingSave = null;
    });
    return operation.promise;
  }
  function serializeWorkspace(source) {
    const copy = document.documentElement.cloneNode(true);
    // Select application-owned elements, not similarly named headings in the
    // preview. Mutate only the detached copy, leaving the live source intact.
    const data = copy.querySelector('body > script#fmd-raw-source[type="application/json"]');
    const textarea = copy.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
    data.textContent = JSON.stringify(source).replace(/</g, '\\u003c')
      .replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
    textarea.textContent = source;
    copy.querySelector('body > #stats-drawer').classList.remove('open');
    // The controller runs before dynamically appended dialogs are reparsed.
    // Persist settings JSON, not transient controls or unapplied form drafts.
    copy.querySelector('body > aside#fmd-document-lab')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-document-lab')?.remove();
    copy.querySelector('body > dialog#fmd-document-settings')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-document-settings')?.remove();
    copy.querySelector('body > .fmd-app-header #fmd-source-controls')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-publish-html')?.remove();
    copy.querySelector('body > .fmd-app-header #fmd-publication-formats')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-restart-preview')?.remove();
    copy.querySelector('body > .fmd-app-header #fmd-export-controls')?.remove();
    if (workerExports) copy.querySelector('body > .fmd-app-header #btn-export-pdf')?.removeAttribute('disabled');
    copy.querySelector('#editor-pane > .fmd-pane-header > #fmd-save-status').textContent = '';
    download('<!DOCTYPE html>\n' + copy.outerHTML, 'text/html;charset=utf-8', 'html');
  }
  document.getElementById('btn-save-markdown').addEventListener('click', () => attempt(saveMarkdown));
  document.getElementById('btn-save-html').addEventListener('click', () => attempt(saveHtml));
  document.addEventListener('keydown', event => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey || event.isComposing || previewComposing) return;
    const key = event.key.toLowerCase();
    if (native && key === 'p') {
      event.preventDefault();
      if (!event.repeat) attempt(exportPdf);
      return;
    }
    if (key !== 's') return;
    event.preventDefault();
    if (!event.repeat) attempt(event.shiftKey ? saveHtml : saveMarkdown);
  });
  window.addEventListener('beforeunload', event => {
    if (isModified()) {
      event.preventDefault();
      event.returnValue = '';
    }
  });
  window.addEventListener('pagehide', () => {
    cancelDocumentExport('Export cancelled: workspace suspended');
    previewSuspended = true; invalidatePreview();
    documentLab?.invalidate('Workspace suspended — run analysis again after restoring.');
    clearTimeout(debounceTimer); debounceTimer = null;
    for (const [url, timer] of downloadUrls) {
      clearTimeout(timer);
      URL.revokeObjectURL(url);
    }
    downloadUrls.clear();
  });
  window.addEventListener('pageshow', () => {
    const resume = previewSuspended;
    previewSuspended = false; previewComposing = false;
    if (resume) refreshNative();
    documentLab?.refresh();
  });
  // Also cover the browser's own Print menu / keyboard shortcut.
  window.addEventListener('beforeprint', () => {
    if (!native) attempt(renderCurrent);
  });

  // Toolbar actions
  document.getElementById('btn-toggle-view').addEventListener('click', () => {
    if (viewMode === 'split') {
      body.classList.remove('view-split');
      body.classList.add('view-read');
      document.getElementById('view-mode-icon').textContent = '✏️';
      document.getElementById('view-mode-label').textContent = 'Edit Mode';
      viewMode = 'read';
    } else {
      body.classList.remove('view-read');
      body.classList.add('view-split');
      document.getElementById('view-mode-icon').textContent = '📖';
      document.getElementById('view-mode-label').textContent = 'Read Mode';
      viewMode = 'split';
    }
  });

  document.getElementById('btn-zoom-in').addEventListener('click', () => {
    currentScale = Math.min(2.0, currentScale + 0.1);
    document.documentElement.style.setProperty('--fmd-base', (16 * currentScale) + 'px');
    document.getElementById('btn-zoom-reset').textContent = Math.round(currentScale * 100) + '%';
    refreshNative();
  });

  document.getElementById('btn-zoom-out').addEventListener('click', () => {
    currentScale = Math.max(0.7, currentScale - 0.1);
    document.documentElement.style.setProperty('--fmd-base', (16 * currentScale) + 'px');
    document.getElementById('btn-zoom-reset').textContent = Math.round(currentScale * 100) + '%';
    refreshNative();
  });

  document.getElementById('btn-zoom-reset').addEventListener('click', () => {
    currentScale = 1.0;
    document.documentElement.style.setProperty('--fmd-base', '16px');
    document.getElementById('btn-zoom-reset').textContent = '100%';
    refreshNative();
  });

  document.getElementById('btn-theme-toggle').addEventListener('click', () => {
    if (document.body.classList.contains('theme-dark')) {
      document.body.classList.remove('theme-dark');
      document.body.classList.add('theme-light');
    } else {
      document.body.classList.remove('theme-light');
      document.body.classList.add('theme-dark');
    }
    refreshNative();
  });

  document.getElementById('btn-stats-toggle').addEventListener('click', () => {
    statsDrawer.classList.toggle('open');
    updateStats();
  });

  statsDrawer.querySelector('#btn-stats-close').addEventListener('click', () => {
    statsDrawer.classList.remove('open');
  });

  document.getElementById('btn-export-pdf').addEventListener('click', () => attempt(exportPdf));

  function installSettings() {
    if (!native || typeof native.applySettings !== 'function') return;
    const header = document.querySelector('body > .fmd-app-header');
    const exportButton = header?.querySelector('#btn-export-pdf');
    if (!exportButton) return;
    let button = header.querySelector('#btn-document-settings');
    if (!button) {
      button = document.createElement('button'); button.id = 'btn-document-settings';
      button.className = 'fmd-btn'; button.type = 'button'; button.textContent = 'Document settings';
      button.setAttribute('aria-haspopup', 'dialog'); button.setAttribute('aria-controls', 'fmd-document-settings');
      exportButton.parentNode.insertBefore(button, exportButton);
    }
    // Reuse any application-owned shell, rebuilding only constant controls.
    // Committed settings live in runtime JSON, never in these form fields.
    let dialog = document.querySelector('body > dialog#fmd-document-settings');
    if (!dialog) { dialog = document.createElement('dialog'); dialog.id = 'fmd-document-settings'; document.body.appendChild(dialog); }
    dialog.removeAttribute('open');
    dialog.setAttribute('aria-labelledby', 'fmd-settings-title');
    dialog.setAttribute('aria-describedby', 'fmd-settings-help');
    dialog.style.cssText = 'width:min(90vw,44rem);max-height:85vh;overflow:auto;box-sizing:border-box;padding:24px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222)';
    dialog.innerHTML = `<form>
      <h2 id="fmd-settings-title">Document settings</h2>
      <p id="fmd-settings-help">Apply changes to native rendering. Save HTML keeps settings and resources; Save Markdown keeps source only. View zoom does not change PDF type size.</p>
      <fieldset><legend>Typography and navigation</legend><div data-grid>
        <label>Body font<select name="font"><option value="sans">Sans serif</option><option value="serif">Serif</option></select></label>
        <label>Type scale (0.5–3)<input name="fontScale" type="number" min="0.5" max="3" step="any" required></label>
        <label>Document color mode<select name="darkMode"><option value="auto">Automatic</option><option value="disabled">Light</option></select></label>
        <label>TOC depth<input name="tocDepth" type="number" min="1" max="6" step="1" required></label>
        <label><input name="toc" type="checkbox"> Table of contents</label>
        <label><input name="pageNumbers" type="checkbox"> PDF page numbers</label>
        <label><input name="codeLineNumbers" type="checkbox"> PDF code line numbers</label>
      </div></fieldset>
      <fieldset><legend>Document metadata</legend><div data-grid>
        <label>Title<input name="title" type="text" maxlength="65536"></label>
        <label>Author<input name="author" type="text" maxlength="65536"></label>
        <label>Language<input name="lang" type="text" maxlength="1024" placeholder="For example, en or fr"></label>
      </div></fieldset>
      <fieldset><legend>PDF paper and margins</legend>
        <p>Dimensions are points (72 points = 1 inch). Margins must leave at least 72 points of content on each axis. Paper does not paginate the HTML preview.</p>
        <div data-grid>
          <label>Paper<select name="paper"><option value="default">Renderer default (no override)</option><option value="letter">Letter</option><option value="a4">A4</option><option value="custom">Custom / saved dimensions</option></select></label>
          <label>Orientation<select name="orientation" data-page><option value="portrait">Portrait</option><option value="landscape">Landscape</option></select></label>
          <label>Width (pt)<input name="width" data-page type="number" min="144" max="14400" step="any" required></label>
          <label>Height (pt)<input name="height" data-page type="number" min="144" max="14400" step="any" required></label>
          <label>Top margin (pt)<input name="top" data-page type="number" min="0" max="14400" step="any" required></label>
          <label>Right margin (pt)<input name="right" data-page type="number" min="0" max="14400" step="any" required></label>
          <label>Bottom margin (pt)<input name="bottom" data-page type="number" min="0" max="14400" step="any" required></label>
          <label>Left margin (pt)<input name="left" data-page type="number" min="0" max="14400" step="any" required></label>
        </div>
      </fieldset>
      <p data-status role="status" aria-live="polite" tabindex="-1"></p>
      <div style="display:flex;gap:12px;justify-content:flex-end"><button class="fmd-btn" type="button" data-cancel>Cancel</button><button class="fmd-btn fmd-btn-primary" type="submit">Apply settings</button></div>
    </form>`;
    for (const grid of dialog.querySelectorAll('[data-grid]')) grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px';
    for (const fieldset of dialog.querySelectorAll('fieldset')) fieldset.style.cssText = 'margin:16px 0;padding:12px;border:1px solid var(--border-color,#aaa)';
    for (const input of dialog.querySelectorAll('select,input:not([type="checkbox"])')) input.style.cssText = 'display:block;width:100%;min-width:0;box-sizing:border-box;font:inherit;padding:6px;margin-top:4px';
    const form = dialog.querySelector('form'), status = form.querySelector('[data-status]');
    const field = name => form.elements.namedItem(name);
    const flags = ['toc', 'pageNumbers', 'codeLineNumbers'];
    const numbers = ['fontScale', 'tocDepth'];
    const texts = ['font', 'darkMode', 'title', 'author', 'lang'];
    const settingsFields = [...flags, ...numbers, ...texts];
    const geometryFields = ['width', 'height', 'top', 'right', 'bottom', 'left'];
    const raw = name => flags.includes(name) ? field(name).checked : field(name).value;
    let opened = null, initial = null;
    const pageEnabled = () => {
      for (const input of form.querySelectorAll('[data-page]')) input.disabled = field('paper').value === 'default';
    };
    const orient = () => {
      const w = field('width').valueAsNumber, h = field('height').valueAsNumber;
      if ((field('orientation').value === 'portrait' && w > h) || (field('orientation').value === 'landscape' && h > w)) {
        [field('width').value, field('height').value] = [field('height').value, field('width').value];
      }
    };
    field('paper').addEventListener('change', () => {
      const paper = field('paper').value;
      if (paper === 'letter' || paper === 'a4') {
        const dimensions = paper === 'letter' ? [612, 792] : [210 * 72 / 25.4, 297 * 72 / 25.4];
        field('width').value = dimensions[0]; field('height').value = dimensions[1]; orient();
      }
      pageEnabled();
    });
    field('orientation').addEventListener('change', orient);
    for (const name of ['width', 'height']) field(name).addEventListener('input', () => {
      field('paper').value = 'custom';
      field('orientation').value = field('width').valueAsNumber > field('height').valueAsNumber ? 'landscape' : 'portrait';
    });
    button.disabled = true;
    function enable() {
      if (!native.settings) return;
      if (settingsBaseline === null) settingsBaseline = JSON.stringify(native.settings);
      button.disabled = false;
    }
    enable();
    if (native.ready) native.ready.then(enable, () => {});
    button.addEventListener('click', () => attempt(() => {
      opened = native.settings;
      if (!opened) throw Error('Native renderer is not ready');
      for (const name of flags) field(name).checked = opened[name];
      for (const name of numbers) field(name).value = opened[name] ?? (name === 'tocDepth' ? 3 : 1);
      for (const name of texts) field(name).value = opened[name] ?? '';
      initial = Object.fromEntries(settingsFields.map(name => [name, raw(name)]));
      const geometry = opened.pageGeometry ?? [612, 792, 72, 72, 72, 72];
      geometryFields.forEach((name, i) => { field(name).value = geometry[i]; });
      field('paper').value = opened.pageGeometry ? 'custom' : 'default';
      field('orientation').value = geometry[0] > geometry[1] ? 'landscape' : 'portrait';
      pageEnabled(); status.textContent = ''; dialog.showModal();
    }));
    form.querySelector('[data-cancel]').addEventListener('click', () => dialog.close());
    form.addEventListener('input', () => { status.textContent = ''; });
    form.addEventListener('submit', event => {
      event.preventDefault();
      if (!form.reportValidity()) return;
      try {
        if (documentLab?.busy) throw Error('Finish or cancel Document Lab analysis before applying settings');
        if (!opened || native.settings !== opened) throw Error('Document settings changed while this panel was open; reopen it before applying');
        const patch = {};
        // Compare actual displayed values. Unedited metadata remains byte-exact
        // even when an input normalizes newlines; omitted values stay omitted.
        for (const name of settingsFields) if (raw(name) !== initial[name]) {
          patch[name] = flags.includes(name) ? raw(name) : numbers.includes(name) ? field(name).valueAsNumber
            : ['title', 'author', 'lang'].includes(name) && raw(name) === '' ? undefined : raw(name);
        }
        const geometry = field('paper').value === 'default' ? undefined : geometryFields.map(name => field(name).valueAsNumber);
        if (JSON.stringify(geometry) !== JSON.stringify(opened.pageGeometry)) patch.pageGeometry = geometry;
        if (Object.keys(patch).length) {
          const source = currentSource();
          invalidatePreview();
          native.applySettings(patch, source, preview, displaySettings());
          documentLab?.invalidate('Document settings changed — run analysis again.');
          clearTimeout(debounceTimer); debounceTimer = null;
          lastRenderedSource = currentSource() === source ? source : null;
          if (Object.hasOwn(patch, 'title')) {
            document.title = native.settings.title ?? 'FrankenMarkdown Document';
            const title = header.querySelector('.fmd-title'); if (title) title.textContent = document.title;
          }
          if (Object.hasOwn(patch, 'lang')) document.documentElement.lang = native.settings.lang ?? 'en';
          saveStatus.textContent = 'Settings applied — Save HTML to keep document settings';
          const notice = nativeNotice(); if (notice) saveStatus.textContent += ' — ' + notice;
          updateStats();
          if (lastRenderedSource === null) attempt(renderCurrent);
        }
        dialog.close();
      } catch (error) {
        status.textContent = 'Settings not applied: ' + String(error?.message ?? error).slice(0, 2048);
        status.focus();
      }
    });
  }

  function installPreviewControls() {
    if (native?.previewMode !== 'worker' || typeof native.restartPreview !== 'function') return;
    const header = document.querySelector('body > .fmd-app-header');
    const exportButton = header?.querySelector('#btn-export-pdf');
    if (!exportButton) return;
    let button = header.querySelector('#btn-restart-preview');
    if (!button) {
      button = document.createElement('button'); button.id = 'btn-restart-preview';
      button.type = 'button'; button.className = 'fmd-btn'; button.textContent = 'Restart preview';
      button.title = 'Stop background rendering and retry the current source; edits and resources are preserved';
      exportButton.parentNode.insertBefore(button, exportButton);
    }
    button.disabled = !!native.ready;
    if (native.ready) native.ready.then(ready => { button.disabled = ready !== true; }, () => {});
    button.addEventListener('click', () => attempt(() => {
      if (button.disabled || previewSuspended || previewComposing) return;
      native.restartPreview();
      refreshNative();
    }));
  }

  function installPublishing() {
    if (!native || typeof native.html !== 'function') return;
    const header = document.querySelector('body > .fmd-app-header');
    const exportButton = header?.querySelector('#btn-export-pdf');
    if (!exportButton) return;
    let button = header.querySelector('#btn-publish-html');
    if (!button) {
      button = document.createElement('button'); button.id = 'btn-publish-html';
      button.type = 'button'; button.className = 'fmd-btn'; button.textContent = 'Publish HTML';
      button.title = 'Download a native HTML document without the editor or WASM runtime; use Save HTML to keep an editable workspace';
      exportButton.parentNode.insertBefore(button, exportButton);
    }
    button.disabled = !!native.ready;
    if (native.ready) native.ready.then(ready => { button.disabled = ready !== true; }, () => {});
    button.addEventListener('click', () => attempt(() => {
      if (button.disabled) return;
      if (workerExports) return exportDocument('html');
      const source = currentSource(), settings = native.settings;
      // Do not flush or scrape the preview. Publication renders current source
      // independently, so pending edits, view zoom and draft controls cannot
      // silently publish an old revision or a different document configuration.
      const html = native.html(source);
      if (typeof html !== 'string' || html.length === 0) throw Error('Native HTML publishing returned an invalid document');
      if (currentSource() !== source || native.settings !== settings) {
        throw Error('Document changed during HTML publishing; publish the current revision again');
      }
      download(html, 'text/html;charset=utf-8', 'published.html');
      const notice = nativeNotice(); if (notice) saveStatus.textContent += ' — ' + notice;
    }));
  }

  function installPublicationFormats() {
    if (!workerExports) return;
    const header = document.querySelector('body > .fmd-app-header');
    const pdf = header?.querySelector('#btn-export-pdf');
    if (!pdf) return;
    let controls = header.querySelector('#fmd-publication-formats');
    if (!controls) {
      controls = document.createElement('span'); controls.id = 'fmd-publication-formats';
      pdf.parentNode.insertBefore(controls, pdf);
    }
    controls.style.cssText = 'display:inline-flex;gap:8px;flex-shrink:0';
    controls.replaceChildren();
    // No new engine requirement for older portable workspaces. Read advertised
    // native capabilities only after initialization, not from document markup.
    const install = () => {
      for (const format of ['epub', 'svg']) {
        if (!native.exportFormats?.includes(format)) continue;
        const spec = publicationFormats[format];
        if (controls.querySelector('#' + spec.button)) continue;
        const button = document.createElement('button');
        button.id = spec.button; button.type = 'button'; button.className = 'fmd-btn';
        button.textContent = 'Export ' + format.toUpperCase();
        button.title = format === 'epub'
          ? 'Download a native EPUB with document typography, navigation and embedded resources'
          : 'Download a continuous native SVG with document typography and embedded resources; PDF paper settings do not apply';
        button.addEventListener('click', () => attempt(() => {
          if (!button.disabled) return exportDocument(format);
        }));
        if (pendingExport) { pendingExport.buttons.push([button, false]); button.disabled = true; }
        controls.appendChild(button);
      }
      controls.hidden = controls.childElementCount === 0;
      controls.style.display = controls.hidden ? 'none' : 'inline-flex';
    };
    install();
    if (native.ready) native.ready.then(ready => { if (ready === true) install(); }, () => {});
  }

  // fmd-async-export-v1: current-revision publications never wait for or scrape
  // the preview. Capture source and settings, then recheck immediately before
  // downloading. Viewing zoom/theme does not change an export request.
  function releaseExportControls(operation) {
    for (const [button, disabled] of operation.buttons) button.disabled = disabled;
    if (exportControls) { exportControls.cancel.hidden = true; exportControls.cancel.style.display = 'none'; }
    documentLab?.refresh();
  }
  function cancelDocumentExport(message) {
    exportRevision++;
    const operation = pendingExport;
    if (!operation) return;
    pendingExport = null;
    native.cancelExport();
    releaseExportControls(operation);
    if (exportControls) exportControls.status.textContent = message;
  }
  function exportDocument(format) {
    if (!Object.hasOwn(publicationFormats, format)) throw Error('Unsupported publication format');
    if (documentLab?.busy) throw Error('Finish or cancel Document Lab analysis before exporting');
    if (previewSuspended || previewComposing) return;
    if (pendingExport) return pendingExport.promise;
    const source = currentSource(), settings = native.settings, revision = exportRevision;
    const operation = {buttons: [], promise: null};
    pendingExport = operation;
    documentLab?.refresh();
    const current = () => pendingExport === operation && revision === exportRevision
      && !previewSuspended && !previewComposing && currentSource() === source && native.settings === settings;
    const {extension, mime} = publicationFormats[format], name = filename(extension);
    const label = format.toUpperCase();
    const header = document.querySelector('body > .fmd-app-header');
    for (const {button: id} of Object.values(publicationFormats)) {
      const button = header?.querySelector('#' + id);
      if (button) { operation.buttons.push([button, button.disabled]); button.disabled = true; }
    }
    if (exportControls) {
      exportControls.cancel.hidden = false; exportControls.cancel.style.display = '';
      exportControls.status.textContent = 'Preparing ' + label + ' in background…';
    }
    operation.promise = Promise.resolve().then(() => {
      if (!current()) return null;
      return native.exportDocument(format, source, current);
    }).then(result => {
      if (!current()) throw Object.assign(Error('Document changed during export'), {code: 'EXPORT_CANCELLED'});
      if (!result || result.format !== format || result.mimeType !== mime || !(result.bytes instanceof Uint8Array) || !result.bytes.length) {
        throw Error('Native export returned an invalid document');
      }
      download(result.bytes, mime, extension, name);
      if (exportControls) {
        exportControls.status.textContent = label + ' download started — check your downloads';
        const notice = nativeNotice(result.diagnostics);
        if (notice) exportControls.status.textContent += ' — ' + notice;
      }
    }).catch(error => {
      if (pendingExport !== operation) return; // A cancelled job cannot replace newer status.
      if (exportControls) exportControls.status.textContent = !current() || error?.code === 'EXPORT_CANCELLED'
        ? 'Export cancelled: document changed; export the current revision again'
        : 'Export failed: ' + String(error?.message ?? error).slice(0, 2048) + ' — retry export; source is unchanged';
    }).finally(() => {
      if (pendingExport === operation) {
        pendingExport = null;
        releaseExportControls(operation);
      }
    });
    return operation.promise;
  }
  function installExportControls() {
    if (!workerExports) return;
    const header = document.querySelector('body > .fmd-app-header');
    const pdf = header?.querySelector('#btn-export-pdf');
    if (!pdf) return;
    let controls = header.querySelector('#fmd-export-controls');
    if (!controls) {
      controls = document.createElement('span'); controls.id = 'fmd-export-controls';
      pdf.parentNode.insertBefore(controls, pdf.nextSibling);
    }
    controls.style.cssText = 'display:inline-flex;align-items:center;gap:8px;flex-shrink:0';
    controls.replaceChildren();
    const cancel = document.createElement('button'), status = document.createElement('span');
    cancel.id = 'btn-cancel-export'; cancel.type = 'button'; cancel.className = 'fmd-btn';
    cancel.textContent = 'Cancel export'; cancel.hidden = true; cancel.style.display = 'none';
    cancel.title = 'Stop the export worker without changing source, settings, resources or preview';
    status.id = 'fmd-export-status'; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
    status.style.cssText = 'max-width:26em;white-space:normal;font-size:12px';
    controls.appendChild(cancel); controls.appendChild(status);
    exportControls = {cancel, status};
    cancel.addEventListener('click', () => cancelDocumentExport('Export cancelled — source and resources are unchanged'));
  }

  function installSourceFiles() {
    if (typeof FileReader !== 'function') return;
    const header = document.querySelector('body > .fmd-app-header');
    const saveButton = header?.querySelector('#btn-save-markdown');
    if (!saveButton) return;
    // Source-only replacement deliberately retains this workspace's settings
    // and explicitly embedded resources. A filename grants no asset/file access.
    let controls = header.querySelector('#fmd-source-controls');
    if (!controls) {
      controls = document.createElement('span'); controls.id = 'fmd-source-controls';
      saveButton.parentNode.insertBefore(controls, saveButton);
    }
    controls.style.cssText = 'display:inline-flex;gap:8px;flex-shrink:0';
    controls.replaceChildren();
    const open = document.createElement('button'), undo = document.createElement('button');
    for (const button of [open, undo]) { button.className = 'fmd-btn'; button.type = 'button'; controls.appendChild(button); }
    open.id = 'btn-open-markdown'; open.textContent = 'Open Markdown';
    open.title = 'Replace source from a UTF-8 Markdown file; retain document settings and embedded resources';
    undo.id = 'btn-undo-source-open'; undo.textContent = 'Undo source replacement';
    undo.title = 'Restore the preceding source, before further source edits'; undo.disabled = true;
    const picker = document.createElement('input'); picker.id = 'fmd-source-picker';
    picker.type = 'file'; picker.accept = '.md,.markdown,.txt,text/markdown,text/plain'; picker.hidden = true;
    controls.appendChild(picker);
    const maximum = 32 * 1024 * 1024;
    let revision = 0, composing = false, suspended = false, pending = null, previous = null, ownInput = null;
    function updateButtons() { open.disabled = suspended || composing || pending !== null; undo.disabled = open.disabled || previous === null; }
    function finish(operation) {
      if (pending !== operation) return;
      pending = null; picker.value = ''; updateButtons();
    }
    function cancel() {
      const operation = pending;
      if (!operation) return;
      // Invalidate first: abort events and late picker/read callbacks cannot
      // release or publish a newer operation started by another user gesture.
      pending = null;
      operation.cancelRead?.();
      picker.value = ''; updateButtons();
      saveStatus.textContent = modifiedNotice() || 'Source opening cancelled';
    }
    function check(operation) {
      if (pending !== operation || suspended || composing || revision !== operation.revision
          || editor.value !== operation.view || sourceAnchor !== operation.anchor) {
        throw Error('Source changed while opening the file; select it again to replace the current source');
      }
    }
    function boundedSource(text) {
      if (typeof text !== 'string' || text.length > maximum) throw Error('Source exceeds 32 MiB');
      let size = 0;
      for (const char of text) {
        const cp = char.codePointAt(0);
        if (cp >= 0xd800 && cp <= 0xdfff) throw Error('Source contains invalid Unicode');
        size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
        if (size > maximum) throw Error('Source exceeds 32 MiB');
      }
    }
    function read(file, operation) {
      if (!file || typeof file.name !== 'string' || file.name.length > 1024
          || /[\u0000-\u001f\u007f]/u.test(file.name) || !/\.(md|markdown|txt)$/i.test(file.name)) {
        throw Error('Select one .md, .markdown or .txt UTF-8 file');
      }
      if (!Number.isSafeInteger(file.size) || file.size < 0 || file.size > maximum) throw Error('Markdown file exceeds 32 MiB');
      return new Promise((resolve, reject) => {
        const reader = new FileReader(); let done = false, timer;
        const settle = (error, result) => {
          if (done) return;
          done = true; clearTimeout(timer); operation.cancelRead = null;
          reader.onload = reader.onerror = reader.onabort = null;
          if (error) { if (reader.readyState === 1) reader.abort(); reject(error); }
          else resolve(result);
        };
        operation.cancelRead = () => settle(Error('Source opening cancelled'));
        reader.onerror = () => settle(Error('Unable to read the selected Markdown file'));
        reader.onabort = () => settle(Error('Source opening cancelled'));
        reader.onload = () => {
          try {
            check(operation);
            const buffer = reader.result;
            const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
            if (length !== file.size || length > maximum) throw Error('Selected file size changed during reading');
            // ignoreBOM means retain the BOM as a source character, not discard
            // it. Fatal decoding refuses corrupt UTF-8 instead of replacing it.
            const source = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true}).decode(buffer);
            settle(null, source);
          } catch (error) { settle(error); }
        };
        timer = setTimeout(() => settle(Error('Reading Markdown exceeded ten seconds; select the file again')), 10000);
        try { reader.readAsArrayBuffer(file); } catch (error) { settle(error); }
      });
    }
    function install(source, selection) {
      const beforeView = editor.value, beforeAnchor = sourceAnchor;
      try {
        editor.value = source;
        const view = editor.value;
        if (view !== source.replace(/\r\n?/g, '\n')) throw Error('Editor could not accept the complete source');
        sourceAnchor = {source, view};
      } catch (error) {
        editor.value = beforeView; sourceAnchor = beforeAnchor;
        throw error;
      }
      // Publish the lossless anchor before synchronous subscribers run. This
      // invalidates in-flight image insertions through their normal input path.
      const event = new Event('input', {bubbles: true});
      const expectedRevision = revision + 1;
      ownInput = event;
      try { editor.dispatchEvent(event); } finally { ownInput = null; }
      if (revision !== expectedRevision || currentSource() !== source) previous = null;
      if (viewMode === 'read') document.getElementById('btn-toggle-view').click();
      editor.focus();
      if (selection) {
        editor.setSelectionRange(selection.start, selection.end, selection.direction);
        editor.scrollTop = selection.scrollTop;
      } else { editor.setSelectionRange(0, 0); editor.scrollTop = 0; }
      updateButtons();
    }
    open.addEventListener('click', () => attempt(() => {
      if (suspended || composing || pending) return;
      const operation = {revision, view: editor.value, anchor: sourceAnchor, cancelRead: null};
      pending = operation; updateButtons();
      try { picker.value = ''; picker.click(); }
      catch (error) { finish(operation); throw error; }
    }));
    picker.addEventListener('cancel', cancel);
    picker.addEventListener('change', () => {
      const operation = pending;
      if (!operation || operation.reading) return;
      const files = picker.files;
      if (!files || files.length === 0) { finish(operation); return; }
      operation.reading = true;
      const run = async () => {
        if (files.length !== 1) throw Error('Select one Markdown source file at a time');
        check(operation);
        // One reversible replacement retains at most 32 MiB on each side.
        // Validate the old source too, without allocating another UTF-8 copy.
        const beforeSource = currentSource(); boundedSource(beforeSource);
        saveStatus.textContent = 'Reading Markdown source…';
        const source = await read(files[0], operation);
        check(operation);
        if (source === beforeSource) { saveStatus.textContent = 'Selected file already matches the current source'; return; }
        const name = Array.from(files[0].name).slice(0, 160).join('');
        const accepted = window.confirm(`Replace the current Markdown source with “${name}”?\n\nDocument settings and embedded images/fonts are retained. No other files or image paths are opened. Undo source replacement is available until the next source edit; Save HTML keeps the workspace.`);
        check(operation); // Recheck even changes that did not dispatch input.
        if (!accepted) { saveStatus.textContent = modifiedNotice(); return; }
        const backup = {source: beforeSource, selection: {start: editor.selectionStart, end: editor.selectionEnd,
          direction: editor.selectionDirection, scrollTop: editor.scrollTop}};
        // No async boundary separates this final check, anchor publication and
        // input dispatch. Rendering may fail afterward; source remains saveable.
        previous = backup;
        try { install(source); } catch (error) { previous = null; throw error; }
        if (previous) { previous.view = editor.value; previous.anchor = sourceAnchor; previous.revision = revision; }
        saveStatus.textContent = 'Markdown source opened — Save HTML to keep this workspace';
      };
      run().catch(error => {
        if (pending === operation) saveStatus.textContent = 'Source not opened: ' + String(error?.message ?? error).slice(0, 2048);
      }).finally(() => finish(operation));
    });
    undo.addEventListener('click', () => attempt(() => {
      if (!previous || pending || composing || suspended) return;
      const backup = previous;
      previous = null; updateButtons();
      if (revision !== backup.revision || editor.value !== backup.view || sourceAnchor !== backup.anchor) {
        throw Error('Source changed after opening; undo would discard newer edits');
      }
      install(backup.source, backup.selection);
      saveStatus.textContent = modifiedNotice() || 'Previous source restored';
    }));
    editor.addEventListener('input', event => {
      revision++;
      if (event !== ownInput) { previous = null; cancel(); }
      updateButtons();
    });
    editor.addEventListener('compositionstart', () => { composing = true; revision++; previous = null; cancel(); updateButtons(); });
    editor.addEventListener('compositionend', () => { composing = false; revision++; updateButtons(); });
    window.addEventListener('pagehide', () => { suspended = true; revision++; previous = null; cancel(); updateButtons(); });
    window.addEventListener('pageshow', () => { suspended = false; composing = false; revision++; updateButtons(); });
  }

  // fmd-document-lab-v1: native, explicit authoring reports. No timer-driven
  // reparsing, browser storage, inferred source positions or report HTML.
  function installDocumentLab() {
    if (!workerExports || typeof native.analyzeDocument !== 'function') return;
    const install = () => {
      if (documentLab || !native.analysisFormats?.length) return;
      const header = document.querySelector('body > .fmd-app-header');
      const pdf = header?.querySelector('#btn-export-pdf');
      if (!pdf) return;
      const make = (tag, text, parent) => {
        const node = document.createElement(tag);
        if (text !== undefined) node.textContent = text;
        if (parent) parent.appendChild(node);
        return node;
      };
      const button = make('button', 'Document Lab');
      button.id = 'btn-document-lab'; button.type = 'button'; button.className = 'fmd-btn';
      button.setAttribute('aria-controls', 'fmd-document-lab'); button.setAttribute('aria-expanded', 'false');
      pdf.parentNode.insertBefore(button, pdf);
      const panel = make('aside', undefined, document.body);
      panel.id = 'fmd-document-lab'; panel.hidden = true;
      panel.setAttribute('role', 'region'); panel.setAttribute('aria-labelledby', 'fmd-lab-title');
      panel.style.cssText = 'position:fixed;z-index:1000;right:16px;bottom:16px;width:min(38rem,calc(100vw - 32px));max-height:75vh;overflow:auto;box-sizing:border-box;padding:20px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222);box-shadow:0 4px 24px #0003;overflow-wrap:anywhere';
      make('h2', 'Document Lab', panel).id = 'fmd-lab-title';
      make('p', 'Analyze the current Markdown with the embedded Rust engine. Reports are read-only and never change source, settings or resources.', panel);
      make('p', 'Accessibility uses engine-default PDF options, not this workspace’s configured paper, fonts or images. It is an authoring audit, not PDF/UA certification.', panel).id = 'fmd-lab-scope';
      const actions = make('div', undefined, panel);
      actions.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px';
      const action = (id, label) => {
        const node = make('button', label, actions);
        node.id = id; node.type = 'button'; node.className = 'fmd-btn'; return node;
      };
      const stats = action('btn-lab-stats', 'Analyze document');
      const audit = action('btn-lab-audit', 'Audit accessibility');
      const save = action('btn-lab-download', 'Download report JSON');
      const cancel = action('btn-lab-cancel', 'Cancel analysis');
      const close = action('btn-lab-close', 'Close');
      const status = make('p', 'Choose an analysis. Readability scores are estimates, not language-independent measurements.', panel);
      status.id = 'fmd-lab-status'; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
      const output = make('div', undefined, panel); output.id = 'fmd-lab-output';
      let pending = null, latest = null, revision = 0;
      const current = snapshot => snapshot && snapshot.revision === revision
        && snapshot.source === currentSource() && snapshot.settings === native.settings
        && !previewSuspended && !previewComposing;
      function refresh() {
        const busy = pending !== null || pendingExport !== null || native.exportPending || native.settingsPending;
        const paused = previewSuspended || previewComposing;
        stats.hidden = !native.analysisFormats.includes('stats');
        audit.hidden = !native.analysisFormats.includes('accessibility');
        stats.disabled = audit.disabled = busy || paused;
        save.disabled = busy || !current(latest);
        cancel.hidden = pending === null;
        panel.setAttribute('aria-busy', String(pending !== null));
      }
      function release(operation) {
        for (const [node, disabled] of operation.buttons) node.disabled = disabled;
      }
      function cancelPending(message) {
        const operation = pending;
        if (!operation) return;
        pending = null;
        native.cancelAnalysis();
        release(operation); status.textContent = message; refresh();
      }
      function invalidate(message = 'Source changed — run analysis again.') {
        revision++; latest = null;
        cancelPending(message); output.replaceChildren();
        status.textContent = message; refresh();
      }
      function show(result, snapshot) {
        const report = result.report;
        const content = document.createDocumentFragment();
        const short = text => text.length > 1200 ? text.slice(0, 1200) + '…' : text;
        make('h3', result.kind === 'stats' ? 'Document intelligence' : 'Accessibility — engine defaults', content);
        if (result.kind === 'stats') {
          const metrics = make('dl', undefined, content);
          const values = [['Words', report.words], ['Characters (excluding whitespace)', report.characters],
            ['Source bytes', report.bytes], ['Source lines', report.lines],
            ['Reading time (seconds)', report.reading_time_secs], ['Speaking time (seconds)', report.speaking_time_secs],
            ['Flesch reading ease', report.flesch_reading_ease + ' — ' + short(report.reading_ease_label)],
            ['Flesch–Kincaid grade', report.flesch_kincaid_grade]];
          for (const [label, value] of values) { make('dt', label, metrics); make('dd', String(value), metrics); }
          make('h4', 'Structure', content);
          const inventory = make('p', undefined, content), counts = [];
          for (const [key, label] of [['headings_total', 'headings'], ['paragraphs', 'paragraphs'],
            ['code_blocks', 'code blocks'], ['tables', 'tables'], ['lists', 'lists'], ['images', 'images'], ['links_total', 'links']]) {
            const value = report.structure[key];
            if (Number.isSafeInteger(value) && value >= 0) counts.push(value + ' ' + label);
          }
          inventory.textContent = counts.join(' · ');
          make('h4', 'Outline — jump to preview', content);
          const outline = make('ol', undefined, content); outline.id = 'fmd-lab-outline';
          for (const heading of report.outline.slice(0, 200)) {
            const row = make('li', undefined, outline);
            row.style.marginLeft = (heading.level - 1) * 10 + 'px';
            const link = make('button', 'H' + heading.level + ' ' + short(heading.text), row);
            link.type = 'button'; link.className = 'fmd-btn';
            link.addEventListener('click', () => navigate(heading, snapshot));
          }
          if (!report.outline.length) make('p', 'No headings found.', content);
          if (report.outline.length > 200) make('p', 'Showing the first 200 headings; the JSON report contains all ' + report.outline.length + '.', content);
        }
        make('h4', 'Findings (' + report.findings.length + ')', content);
        const findings = make('ul', undefined, content); findings.id = 'fmd-lab-findings';
        for (const finding of report.findings.slice(0, 200)) {
          make('li', short((finding.severity ? finding.severity + ' · ' : '') + finding.code
            + ': ' + (finding.message ?? finding.detail ?? 'No further detail supplied.')), findings);
        }
        if (!report.findings.length) make('p', 'No findings returned by this check. This is not a certification of the exported document.', content);
        if (report.findings.length > 200) make('p', 'Showing the first 200 findings; download JSON for all ' + report.findings.length + '.', content);
        make('p', 'Findings have no source spans in this native contract; no line numbers are inferred. Long display text is abbreviated; JSON retains the full report.', content);
        output.replaceChildren(content);
      }
      async function navigate(heading, snapshot) {
        try {
          if (latest !== snapshot || !current(snapshot)) throw Error('Source or settings changed — analyze again before navigating.');
          status.textContent = 'Preparing the current preview…';
          const committed = await Promise.resolve(renderCurrent());
          if (committed === false || latest !== snapshot || !current(snapshot) || lastRenderedSource !== snapshot.source) {
            throw Error('Preview or source changed — analyze again.');
          }
          const frame = preview.querySelector(':scope > iframe');
          if (!frame) throw Error('The native preview is unavailable. Restart preview and try again.');
          if (frame.contentDocument?.URL !== 'about:srcdoc' || frame.contentDocument?.readyState !== 'complete') {
            await new Promise((resolve, reject) => {
              const done = error => {
                clearTimeout(timer); frame.removeEventListener('load', loaded);
                if (error) reject(error); else resolve();
              };
              const loaded = () => done();
              const timer = setTimeout(() => done(Error('Preview is still loading; try the heading again.')), 10000);
              frame.addEventListener('load', loaded, {once: true});
            });
          }
          if (latest !== snapshot || !current(snapshot) || frame !== preview.querySelector(':scope > iframe')
              || lastRenderedSource !== snapshot.source) throw Error('Preview changed — try the current outline again.');
          // Native slugs are opaque identities, never selectors or URLs. Resolve
          // only a heading inside the current sandboxed preview, not the app DOM.
          const target = frame.contentDocument?.getElementById(heading.slug);
          if (!target || target.tagName !== 'H' + heading.level) throw Error('This heading is not available in the current preview.');
          target.setAttribute('tabindex', '-1'); target.scrollIntoView({block: 'center'}); target.focus({preventScroll: true});
          status.textContent = 'Preview heading: ' + heading.text.slice(0, 200);
        } catch (error) {
          if (latest === snapshot) status.textContent = String(error?.message ?? error).slice(0, 2048);
          refresh();
        }
      }
      function run(kind) {
        if (pending || pendingExport || native.exportPending || native.settingsPending) {
          status.textContent = 'Finish or cancel the current document operation first.'; return;
        }
        if (previewSuspended || previewComposing) return;
        const operation = {source: currentSource(), settings: native.settings, revision: ++revision, buttons: []};
        pending = operation; latest = null; output.replaceChildren();
        for (const id of [...Object.values(publicationFormats).map(spec => spec.button), 'btn-document-settings']) {
          const node = header.querySelector('#' + id);
          if (node) { operation.buttons.push([node, node.disabled]); node.disabled = true; }
        }
        status.textContent = kind === 'stats' ? 'Analyzing document in background…' : 'Auditing accessibility in background with engine-default PDF options…';
        refresh();
        Promise.resolve().then(() => {
          if (pending !== operation || !current(operation)) throw Object.assign(Error('Document changed'), {code: 'ANALYSIS_CANCELLED'});
          return native.analyzeDocument(kind, operation.source, () => pending === operation && current(operation));
        }).then(result => {
          if (pending !== operation || !current(operation)) throw Object.assign(Error('Document changed'), {code: 'ANALYSIS_CANCELLED'});
          if (result?.kind !== kind || result.mimeType !== 'application/json' || !(result.bytes instanceof Uint8Array)
              || !result.report || !Array.isArray(result.report.findings)) throw Error('Invalid native analysis result');
          latest = {...operation, result};
          show(result, latest);
          status.textContent = 'Analysis complete for the captured source. Download JSON to keep the report.';
        }).catch(error => {
          if (pending !== operation) return;
          status.textContent = !current(operation) || error?.code === 'ANALYSIS_CANCELLED'
            ? 'Analysis cancelled: document changed — run analysis again.'
            : 'Analysis failed: ' + String(error?.message ?? error).slice(0, 2048) + ' — source is unchanged; retry explicitly.';
        }).finally(() => {
          if (pending === operation) { pending = null; release(operation); refresh(); }
        });
      }
      button.addEventListener('click', () => {
        panel.hidden = !panel.hidden; button.setAttribute('aria-expanded', String(!panel.hidden));
        if (panel.hidden) cancelPending('Analysis cancelled: Document Lab closed.');
        else { refresh(); (stats.hidden ? audit : stats).focus(); }
      });
      stats.addEventListener('click', () => run('stats'));
      audit.addEventListener('click', () => run('accessibility'));
      cancel.addEventListener('click', () => cancelPending('Analysis cancelled — source and resources are unchanged.'));
      close.addEventListener('click', () => {
        cancelPending('Analysis cancelled: Document Lab closed.');
        panel.hidden = true; button.setAttribute('aria-expanded', 'false'); button.focus();
      });
      panel.addEventListener('keydown', event => {
        if (event.key === 'Escape' && !event.isComposing) { event.preventDefault(); close.click(); }
      });
      save.addEventListener('click', () => {
        if (!current(latest)) { invalidate('Source or settings changed — analyze again before downloading.'); return; }
        attempt(() => download(latest.result.bytes, 'application/json', latest.result.kind + '.json'));
      });
      documentLab = {invalidate, refresh, get busy() { return pending !== null; }};
      refresh();
    };
    if (native.ready) native.ready.then(ready => { if (ready === true) install(); }, () => {});
    else install();
  }

  installSettings();
  installSourceFiles();
  installPublishing();
  installPublicationFormats();
  installPreviewControls();
  installExportControls();
  installDocumentLab();

  // Initial stats calculation
  updateStats();
})();