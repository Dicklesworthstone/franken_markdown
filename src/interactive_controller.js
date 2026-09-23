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
  // Keep the full initial renderer output available for exact undo, including
  // diagrams and other features outside the offline JavaScript subset.
  const originalRendered = preview.innerHTML;
  let lastRenderedSource = native ? null : originalSource;
  const storedScale = parseFloat(document.documentElement.style.getPropertyValue('--fmd-base')) / 16;
  let currentScale = Number.isFinite(storedScale) ? Math.min(2, Math.max(0.7, storedScale)) : 1;
  let viewMode = body.classList.contains('view-read') ? 'read' : 'split';
  let settingsBaseline = null;
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
  function nativeNotice() {
    const findings = native?.diagnostics ?? [];
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
    invalidatePreview();
    updateStats();
    saveStatus.textContent = modifiedNotice();
    clearTimeout(debounceTimer);
    if (!previewComposing && !previewSuspended) debounceTimer = setTimeout(() => attempt(renderCurrent), 150);
  });

  editor.addEventListener('compositionstart', () => {
    previewComposing = true; invalidatePreview();
    clearTimeout(debounceTimer); debounceTimer = null;
  });
  editor.addEventListener('compositionend', () => {
    previewComposing = false; invalidatePreview();
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
  function download(content, mime, extension) {
    let url = null, anchor = null;
    try {
      const blob = new Blob([content], {type: mime, endings: 'transparent'});
      url = URL.createObjectURL(blob);
      anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename(extension);
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
    copy.querySelector('body > dialog#fmd-document-settings')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-document-settings')?.remove();
    copy.querySelector('body > .fmd-app-header #fmd-source-controls')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-publish-html')?.remove();
    copy.querySelector('body > .fmd-app-header #btn-restart-preview')?.remove();
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
    previewSuspended = true; invalidatePreview();
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

  installSettings();
  installSourceFiles();
  installPublishing();
  installPreviewControls();

  // Initial stats calculation
  updateStats();
})();