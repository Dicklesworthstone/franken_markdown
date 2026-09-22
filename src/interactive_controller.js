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
        native.render(source, preview, {
          scale: currentScale,
          theme: document.body.classList.contains('theme-dark') ? 'dark'
            : document.body.classList.contains('theme-light') ? 'light' : undefined,
        });
        saveStatus.textContent = nativeNotice() || (editor.value === originalEditorValue
          ? '' : 'Modified — download to keep changes');
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
    saveStatus.textContent = editor.value === originalEditorValue ? '' : 'Modified — download to keep changes';
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
    if (editor.value !== originalEditorValue) {
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

  // Initial stats calculation
  updateStats();
})();
