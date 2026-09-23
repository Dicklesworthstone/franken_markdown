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
  const currentSource = () => editor.value === originalEditorValue ? originalSource : editor.value;
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
  const isModified = () => editor.value !== originalEditorValue
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

  // One synchronous path is used by debounce, print and workspace export.
  // Do not replace native output until source changes, or publish a source
  // revision as rendered before the parser and DOM update both succeed.
  let debounceTimer = null;
  function renderCurrent() {
    clearTimeout(debounceTimer);
    debounceTimer = null;
    const source = currentSource();
    if (source !== lastRenderedSource) {
      if (native) {
        native.render(source, preview, displaySettings());
        saveStatus.textContent = nativeNotice() || modifiedNotice();
      } else {
        const html = source === originalSource ? originalRendered : parseMarkdownClient(source, imageAssets);
        preview.innerHTML = html;
      }
      lastRenderedSource = source;
    }
    updateStats();
  }
  function attempt(action) {
    try { action(); }
    catch (error) { saveStatus.textContent = 'Unable to complete: ' + String(error?.message || error); }
  }
  function nativeNotice() {
    const findings = native?.diagnostics ?? [];
    return findings.length ? `Renderer diagnostics (${findings.length}): `
      + findings.slice(0, 3).map(item => String(item?.message ?? item).slice(0, 300)).join('; ') : '';
  }
  function refreshNative() {
    if (!native) return;
    lastRenderedSource = null;
    attempt(renderCurrent);
  }
  window.addEventListener('fmd-native-ready', refreshNative);
  window.addEventListener('fmd-native-error', event => {
    if (native) saveStatus.textContent = String(event.detail).slice(0, 2048);
  });
  // Also handle an engine that settled before this controller was evaluated.
  // Read currentSource at completion: typing during startup is never lost.
  if (native?.ready) native.ready.then(() => attempt(renderCurrent));
  editor.addEventListener('input', () => {
    updateStats();
    saveStatus.textContent = modifiedNotice();
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => attempt(renderCurrent), 150);
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
    renderCurrent();
    const source = currentSource();
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
    copy.querySelector('#editor-pane > .fmd-pane-header > #fmd-save-status').textContent = '';
    download('<!DOCTYPE html>\n' + copy.outerHTML, 'text/html;charset=utf-8', 'html');
  }
  document.getElementById('btn-save-markdown').addEventListener('click', () => attempt(saveMarkdown));
  document.getElementById('btn-save-html').addEventListener('click', () => attempt(saveHtml));
  document.addEventListener('keydown', event => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
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
    for (const [url, timer] of downloadUrls) {
      clearTimeout(timer);
      URL.revokeObjectURL(url);
    }
    downloadUrls.clear();
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
    if (native.ready) native.ready.then(enable);
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

  installSettings();

  // Initial stats calculation
  updateStats();
})();
