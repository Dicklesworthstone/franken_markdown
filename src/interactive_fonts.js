// Explicit local-font authoring for portable native workspaces. Font tables are
// interpreted only by the native worker; this asset handles bounded file I/O and
// revision-owned UI state, never system-font discovery, URLs or browser storage.
const FmdFontFiles = (() => {
  const limits = Object.freeze({fileBytes: 32 * 1024 * 1024, batchBytes: 64 * 1024 * 1024});
  function capture(file) {
    if (!file || typeof file.name !== 'string' || file.name.length > 1024
        || /[\u0000-\u001f\u007f]/u.test(file.name) || !/\.ttf$/i.test(file.name)) {
      throw Error('Choose a local .ttf file; collections, WOFF and remote URLs are not supported');
    }
    if (!Number.isSafeInteger(file.size) || file.size < 1 || file.size > limits.fileBytes) {
      throw Error('Each font file must contain 1 byte through 32 MiB');
    }
    return Object.freeze({file, size: file.size});
  }
  function read(item, win, operation, check) {
    return new Promise((resolve, reject) => {
      const reader = new win.FileReader(); let done = false, timer;
      function settle(error, bytes) {
        if (done) return;
        done = true; win.clearTimeout(timer); operation.cancelRead = null;
        reader.onload = reader.onerror = reader.onabort = null;
        if (error) { if (reader.readyState === 1) reader.abort(); reject(error); }
        else resolve(bytes);
      }
      operation.cancelRead = () => settle(Error('Font reading cancelled'));
      reader.onerror = () => settle(Error('Unable to read the selected font file'));
      reader.onabort = () => settle(Error('Font reading cancelled'));
      reader.onload = () => {
        try {
          check();
          const buffer = reader.result;
          const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
          if (length !== item.size) throw Error('Font read did not match the admitted file size');
          settle(null, new Uint8Array(buffer));
        } catch (error) { settle(error); }
      };
      timer = win.setTimeout(() => settle(Error('Font reading exceeded ten seconds; select the file again')), 10000);
      try { check(); reader.readAsArrayBuffer(item.file); } catch (error) { settle(error); }
    });
  }
  return Object.freeze({limits, capture, read});
})();

const fmdFontHosts = new WeakMap();
function fmdInstallDocumentFonts(doc = document) {
  const win = doc.defaultView;
  const native = Object.getOwnPropertyDescriptor(win, '__fmdNativeRuntime')?.value;
  if (native?.version !== 1 || native.fontMode !== 'worker' || typeof native.applyFontsAsync !== 'function') return null;
  const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
  const preview = doc.querySelector('body > #fmd-app-body > #preview-pane > #fmd-content');
  const header = doc.querySelector('body > .fmd-app-header');
  const save = header?.querySelector('#btn-save-html');
  if (!editor || !preview || !save) return null;
  if (fmdFontHosts.has(editor)) return fmdFontHosts.get(editor);
  const make = (tag, text, parent) => {
    const node = doc.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (parent) parent.appendChild(node);
    return node;
  };
  let toggle = header.querySelector('#btn-document-fonts');
  if (!toggle) {
    toggle = make('button', 'Document fonts'); toggle.id = 'btn-document-fonts';
    toggle.type = 'button'; toggle.className = 'fmd-btn'; save.parentNode.insertBefore(toggle, save);
  }
  toggle.setAttribute('aria-haspopup', 'dialog'); toggle.setAttribute('aria-controls', 'fmd-document-fonts');
  let dialog = doc.querySelector('body > dialog#fmd-document-fonts');
  if (!dialog) { dialog = make('dialog', undefined, doc.body); dialog.id = 'fmd-document-fonts'; }
  // Saved shells contain no authoritative draft data. Rebuild controls and
  // discard file selections, active work, and transient messages on every boot.
  dialog.removeAttribute('open'); dialog.replaceChildren();
  dialog.setAttribute('aria-labelledby', 'fmd-fonts-title');
  dialog.setAttribute('aria-describedby', 'fmd-fonts-help');
  dialog.style.cssText = 'width:min(92vw,48rem);max-height:85vh;overflow:auto;box-sizing:border-box;margin:auto;padding:24px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222)';
  const form = make('form', undefined, dialog); form.noValidate = true;
  make('h2', 'Document fonts', form).id = 'fmd-fonts-title';
  make('p', 'Choose local TrueType (.ttf) files for the five native font slots. Apply validates the entire batch in a background worker before changing the preview or saved fonts. Source and document settings stay unchanged.', form).id = 'fmd-fonts-help';
  make('p', '32 MiB per file; 64 MiB per selected batch. Save HTML retains embedded fonts; Save Markdown does not. Use only fonts you have permission to embed. Blank weight uses the native default; pins are integers from 1 to 1000.', form);
  const slots = [['body-regular', 'Body regular'], ['body-bold', 'Body bold'], ['body-italic', 'Body italic'],
    ['body-bold-italic', 'Body bold italic'], ['mono-regular', 'Code / monospace']];
  const rows = slots.map(([slot, title]) => {
    const group = make('fieldset', undefined, form); group.style.cssText = 'margin:16px 0;padding:12px;border:1px solid var(--border-color,#aaa)';
    make('legend', title, group);
    const info = make('p', '', group); info.id = 'fmd-font-info-' + slot;
    const label = (text, tag, name) => {
      const wrapper = make('label', text, group), input = make(tag, undefined, wrapper);
      wrapper.style.cssText = 'display:block;margin:8px 0';
      input.id = 'fmd-font-' + name + '-' + slot;
      input.style.cssText = 'display:block;width:100%;box-sizing:border-box;font:inherit;padding:6px;background:var(--bg-editor,#fff);color:inherit';
      return input;
    };
    const action = label('Action', 'select', 'action');
    for (const [value, text] of [['keep', 'Keep current font'], ['file', 'Replace with local .ttf'],
      ['weight', 'Change embedded font weight pin'], ['clear', 'Use native default']]) {
      const option = make('option', text, action); option.value = value;
    }
    const file = label('Local font file', 'input', 'file'); file.type = 'file'; file.accept = '.ttf,font/ttf';
    const weight = label('Weight pin (blank = native default)', 'input', 'weight');
    weight.type = 'number'; weight.min = '1'; weight.max = '1000'; weight.step = '1'; weight.autocomplete = 'off';
    return {slot, action, file, weight, info};
  });
  const status = make('p', 'Native renderer is initializing…', form);
  status.id = 'fmd-fonts-status'; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  status.style.cssText = 'margin:12px 0;overflow-wrap:anywhere';
  const buttons = make('div', undefined, form); buttons.style.cssText = 'display:flex;flex-wrap:wrap;gap:12px;justify-content:flex-end';
  const button = (id, title) => { const node = make('button', title, buttons); node.id = id; node.type = 'button'; node.className = 'fmd-btn'; return node; };
  const cancelButton = button('btn-cancel-fonts', 'Cancel operation');
  const closeButton = button('btn-close-fonts', 'Close');
  const applyButton = button('btn-apply-fonts', 'Apply fonts'); applyButton.type = 'submit';
  let pending = null, revision = 0, composing = false, suspended = false, modified = false;
  const display = () => {
    const scale = parseFloat(doc.documentElement.style.getPropertyValue('--fmd-base')) / 16;
    return {scale: Number.isFinite(scale) ? Math.min(2, Math.max(0.7, scale)) : 1,
      theme: doc.body.classList.contains('theme-dark') ? 'dark'
        : doc.body.classList.contains('theme-light') ? 'light' : undefined};
  };
  const entries = () => rows.map(row => ({slot: row.slot, action: row.action.value, weight: row.weight.value,
    file: row.file.files?.[0], count: row.file.files?.length ?? 0}));
  function refresh() {
    toggle.disabled = suspended || !native.fontSlots;
    applyButton.disabled = suspended || composing || pending !== null || !native.fontSlots || editor.disabled || editor.readOnly;
    cancelButton.disabled = pending === null;
    form.setAttribute('aria-busy', pending ? 'true' : 'false');
    for (const row of rows) {
      const font = native.fontSlots?.find(font => font.slot === row.slot);
      row.info.textContent = !font ? 'Initializing…' : font.bytes
        ? `Embedded: ${font.bytes.toLocaleString()} bytes; ${font.weight === undefined ? 'native weight' : 'weight ' + font.weight}` : 'Native default';
      row.file.disabled = row.action.value !== 'file';
      row.weight.disabled = !['file', 'weight'].includes(row.action.value);
    }
  }
  function reset() {
    for (const row of rows) {
      row.action.value = 'keep'; row.file.value = '';
      row.weight.value = native.fontSlots?.find(font => font.slot === row.slot)?.weight ?? '';
    }
    refresh();
  }
  function cancel(message = 'Font operation cancelled; committed fonts are unchanged.') {
    const operation = pending;
    if (!operation) return;
    pending = null; // Retire ownership before abort handlers can settle.
    operation.cancelRead?.();
    if (operation.preflight) native.cancelFonts();
    status.textContent = message; refresh();
  }
  function changed(message) { revision++; cancel(message); refresh(); }
  function check(operation) {
    const now = entries();
    if (pending !== operation || !dialog.open || suspended || composing || editor.disabled || editor.readOnly
        || revision !== operation.revision || editor.value !== operation.source
        || native.settings !== operation.settings || native.fontSlots !== operation.fonts
        || JSON.stringify(display()) !== operation.view
        || now.some((entry, i) => Object.keys(entry).some(key => entry[key] !== operation.entries[i][key]))) {
      throw Error('Source, view or font draft changed; apply the current draft again');
    }
  }
  async function apply() {
    if (pending || suspended || composing || editor.disabled || editor.readOnly || !native.fontSlots) return;
    if (native.settingsPending || native.exportPending) throw Error('Finish or cancel the current document operation first');
    const operation = {source: editor.value, settings: native.settings, fonts: native.fontSlots,
      view: JSON.stringify(display()), revision, entries: entries(), cancelRead: null, preflight: false};
    const patches = [];
    let total = 0;
    // Admit the entire selected batch before the first FileReader allocation.
    for (const entry of operation.entries) {
      if (entry.action === 'keep') continue;
      if (entry.action === 'clear') { patches.push({slot: entry.slot, clear: true}); continue; }
      if (!['file', 'weight'].includes(entry.action)) throw Error('Choose a supported font action');
      const value = entry.weight.trim();
      if (value && (!/^\d+$/.test(value) || Number(value) < 1 || Number(value) > 1000)) throw Error('Weight pins must be integers from 1 through 1000, or blank');
      const patch = {slot: entry.slot, weight: value ? Number(value) : undefined};
      if (entry.action === 'file') {
        if (entry.count !== 1) throw Error('Choose exactly one local .ttf file for each replacement slot');
        patch.input = FmdFontFiles.capture(entry.file); total += patch.input.size;
        if (total > FmdFontFiles.limits.batchBytes) throw Error('Selected font files exceed the 64 MiB batch limit');
      } else if (!operation.fonts.find(font => font.slot === entry.slot)?.bytes) {
        throw Error('Weight changes require an embedded font; choose a local file first');
      }
      patches.push(patch);
    }
    if (!patches.length) { status.textContent = 'No font changes selected.'; return; }
    pending = operation; refresh();
    try {
      check(operation);
      for (const patch of patches) {
        if (!patch.input) continue;
        status.textContent = `Reading local font for ${patch.slot}…`;
        patch.bytes = await FmdFontFiles.read(patch.input, win, operation, () => check(operation));
        delete patch.input;
      }
      check(operation);
      operation.preflight = true; status.textContent = 'Validating and rendering fonts in background…';
      await native.applyFontsAsync(patches, operation.source, preview, JSON.parse(operation.view), () => {
        try { check(operation); return true; } catch { return false; }
      });
      // The runtime already checked and atomically published this revision.
      // Comparing old font identities here would reject our own successful edit.
      if (pending !== operation) return;
      const didChange = native.fontSlots !== operation.fonts;
      pending = null; reset();
      status.textContent = didChange ? 'Fonts applied. Save HTML to retain embedded fonts; Markdown source is unchanged.'
        : 'Selected fonts and weights already match; nothing changed.';
    } catch (error) {
      if (pending === operation) status.textContent = 'Fonts not applied: ' + String(error?.message ?? error).slice(0, 2048);
    } finally {
      if (pending === operation) pending = null;
      refresh();
    }
  }
  form.addEventListener('submit', event => {
    event.preventDefault();
    apply().catch(error => { status.textContent = String(error?.message ?? error).slice(0, 2048); refresh(); });
  });
  for (const row of rows) {
    row.action.addEventListener('change', () => { if (row.action.value !== 'file') row.file.value = ''; refresh(); });
    row.file.addEventListener('change', () => { row.weight.value = ''; });
  }
  for (const name of ['input', 'change']) form.addEventListener(name, () => changed('Font draft changed; apply it again when ready.'));
  editor.addEventListener('input', () => changed('Source changed; apply fonts again for the current document.'));
  for (const node of [editor, form]) {
    node.addEventListener('compositionstart', () => { composing = true; changed('Text composition started; apply fonts again when finished.'); });
    node.addEventListener('compositionend', () => { composing = false; changed(); });
  }
  cancelButton.addEventListener('click', () => cancel());
  closeButton.addEventListener('click', () => { cancel(); dialog.close(); reset(); });
  dialog.addEventListener('cancel', () => cancel());
  dialog.addEventListener('close', () => { if (!dialog.open) { cancel(); reset(); } });
  toggle.addEventListener('click', () => {
    if (toggle.disabled || dialog.open) return;
    revision++; reset(); status.textContent = 'Choose the slots to change. All selected changes apply together.'; dialog.showModal();
  });
  win.addEventListener('pagehide', () => { suspended = true; composing = false; changed('Workspace suspended; pending font changes were discarded.'); reset(); });
  win.addEventListener('pageshow', () => { suspended = false; composing = false; changed(); });
  win.addEventListener('fmd-native-fonts-changed', () => {
    modified = true; refresh();
    const notice = doc.querySelector('body > #fmd-app-body > #editor-pane #fmd-save-status');
    if (notice) notice.textContent = 'Fonts modified — Save HTML to keep embedded resources';
  });
  win.addEventListener('beforeunload', event => {
    // A download request is not proof the user saved it. Reopening a saved
    // workspace establishes a new baseline; failed and no-op imports stay clean.
    if (modified) { event.preventDefault(); event.returnValue = ''; }
  });
  // Direct display-style changes have no input event. Mutation records also
  // catch a theme/zoom change followed by a revert before observer delivery.
  const observer = new win.MutationObserver(() => changed('Document view changed; apply fonts again.'));
  observer.observe(doc.documentElement, {attributes: true, attributeFilter: ['style']});
  observer.observe(doc.body, {attributes: true, attributeFilter: ['class']});
  if (native.ready) native.ready.then(() => { reset(); if (!native.fontSlots) status.textContent = 'Native renderer is unavailable; source and saved resources are unchanged.'; }, () => refresh());
  reset();
  const installed = Object.freeze({dialog}); fmdFontHosts.set(editor, installed); return installed;
}
if (typeof document !== 'undefined') {
  // Serialized dialogs follow the application script in saved HTML. Wait until
  // parsing finishes before looking for that shell, or reopening creates two.
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded',
    () => fmdInstallDocumentFonts(), {once: true});
  else fmdInstallDocumentFonts();
}
