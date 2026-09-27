// Explicit local PNG/JPEG authoring for standalone workspaces. No fetch,
// ambient path access, dependency or persistent browser storage is involved.
const FMD_IMAGE_IMPORT_LIMITS = Object.freeze({
  files: 8, fileBytes: 8 * 1024 * 1024, batchBytes: 16 * 1024 * 1024,
  pixels: 24_000_000, dimension: 16384, sourceUnits: 32 * 1024 * 1024
});

function fmdImportDimensions(width, height) {
  const limit = FMD_IMAGE_IMPORT_LIMITS;
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 ||
      width > limit.dimension || height > limit.dimension || width * height > limit.pixels) {
    throw Error('Image dimensions exceed 16,384 per side or 24 million pixels');
  }
}

// Inspect dimensions before allowing browser decoding. The decoder still has
// to validate the real image, not merely these container/frame headers.
function fmdImportImageInfo(bytes) {
  if (!bytes.length || bytes.length > FMD_IMAGE_IMPORT_LIMITS.fileBytes) {
    throw Error('Each image must contain 1 byte to 8 MiB');
  }
  const u16 = n => bytes[n] * 256 + bytes[n + 1];
  const u32 = n => bytes[n] * 0x1000000 + bytes[n + 1] * 0x10000 + bytes[n + 2] * 256 + bytes[n + 3];
  if (bytes.length >= 33 && [137, 80, 78, 71, 13, 10, 26, 10].every((value, i) => bytes[i] === value)) {
    let offset = 8, width, height, data = false;
    while (offset + 12 <= bytes.length) {
      const length = u32(offset);
      if (length > bytes.length - offset - 12) break;
      const tag = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
      if (offset === 8 && (tag !== 'IHDR' || length !== 13)) break;
      if (tag === 'IHDR') {
        if (offset !== 8 || length !== 13) break;
        width = u32(offset + 8); height = u32(offset + 12);
        fmdImportDimensions(width, height);
      }
      if (tag === 'acTL') throw Error('Animated PNG import is not supported; choose a static PNG or JPEG');
      if (tag === 'IDAT') data = true;
      offset += length + 12;
      if (tag === 'IEND') {
        if (length || !data || offset !== bytes.length) break;
        return {mime: 'image/png', width, height};
      }
    }
    throw Error('Invalid or truncated PNG container');
  }
  if (bytes.length >= 4 && bytes[0] === 255 && bytes[1] === 216) {
    let offset = 2;
    while (offset + 4 <= bytes.length) {
      if (bytes[offset++] !== 255) break;
      while (offset < bytes.length && bytes[offset] === 255) offset++;
      const tag = bytes[offset++];
      if (tag === 218 || tag === 217 || tag === 0 || tag === 216 || (tag >= 208 && tag <= 215)) break;
      if (tag === 1) continue; // TEM has no length field.
      if (offset + 2 > bytes.length) break;
      const length = u16(offset);
      if (length < 2 || length > bytes.length - offset) break;
      if ([192, 193, 194, 195, 197, 198, 199, 201, 202, 203, 205, 206, 207].includes(tag)) {
        if (length < 8 || length !== 8 + 3 * bytes[offset + 7] || bytes[offset + 7] === 0) break;
        const height = u16(offset + 3), width = u16(offset + 5);
        fmdImportDimensions(width, height);
        return {mime: 'image/jpeg', width, height};
      }
      offset += length;
    }
    throw Error('Invalid or missing JPEG frame');
  }
  throw Error('Choose a static PNG or JPEG image (file contents, not its extension, determine the format)');
}

function fmdImportBase64(bytes) {
  // Small chunks avoid the argument/stack limits of spreading an entire file.
  const chunks = [];
  for (let offset = 0; offset < bytes.length; offset += 0x4000) {
    chunks.push(String.fromCharCode(...bytes.subarray(offset, offset + 0x4000)));
  }
  return btoa(chunks.join(''));
}

function fmdImportAlt(name) {
  // A filename is literal alt text, never Markdown/HTML or a destination.
  return (Array.from(String(name || 'image').replace(/[\u0000-\u001f\u007f-\u009f]/g, ' '))
    .slice(0, 200).join('').trim() || 'image')
    .replace(/&/g, '&amp;').replace(/[\\[\]!*_`<>]/g, '\\$&');
}

async function fmdReadImageImport(files, verify, current) {
  const limit = FMD_IMAGE_IMPORT_LIMITS;
  if (!files || !Number.isSafeInteger(files.length) || !files.length || files.length > limit.files) {
    throw Error('Choose between 1 and 8 images');
  }
  let total = 0;
  const selected = [];
  for (let i = 0; i < files.length; i++) {
    const file = files[i], size = file?.size;
    if (typeof file?.arrayBuffer !== 'function' || !Number.isSafeInteger(size) || size < 1 || size > limit.fileBytes) {
      throw Error('Each image must contain 1 byte to 8 MiB');
    }
    total += size;
    selected.push({file, size, alt: fmdImportAlt(file.name)});
  }
  if (total > limit.batchBytes) throw Error('Selected images exceed the 16 MiB batch limit');
  const checkCurrent = () => {
    if (!current()) throw Error('The document changed while images were being read; select the images again');
  };
  const images = [];
  for (const {file, size, alt} of selected) {
    checkCurrent();
    const buffer = await file.arrayBuffer();
    checkCurrent();
    const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
    if (length !== size) throw Error('Image size changed while reading');
    // Own the verified bytes before decoding yields; shared/detached buffers
    // cannot retarget the asset between validation and native publication.
    const bytes = new Uint8Array(buffer).slice();
    const info = fmdImportImageInfo(bytes);
    const uri = 'data:' + info.mime + ';base64,' + fmdImportBase64(bytes);
    await verify(uri, info);
    checkCurrent();
    images.push({alt, bytes, uri, mime: info.mime});
  }
  return images;
}

// Preserve the lightweight editor's portable data-URI Markdown contract.
async function fmdPrepareImageImport(files, verify, current) {
  const images = await fmdReadImageImport(files, verify, current);
  return images.map(image => '![' + image.alt + '](' + image.uri + ')').join('\n\n');
}

function fmdNativeImageImport(images, native) {
  // Resource identities belong to this insertion, not filenames or mutable
  // paths. Random 128-bit names avoid accidentally binding existing unresolved
  // source references. The native store additionally rejects any duplicate key.
  const assets = images.map(image => {
    const id = Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, '0')).join('');
    return {destination: 'fmd-import/' + id + (image.mime === 'image/png' ? '.png' : '.jpg'), bytes: image.bytes};
  });
  const transaction = native.stageImages(assets);
  const markdown = images.map((image, i) => '![' + image.alt + '](' + assets[i].destination + ')').join('\n\n');
  return {transaction, markdown};
}

// Keep pure admission helpers executable by the Node regression harness.
if (typeof document !== 'undefined') (function() {
  'use strict';
  const editor = document.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
  const button = document.querySelector('body > .fmd-app-header #btn-insert-image');
  const picker = document.querySelector('body > .fmd-app-header #fmd-image-picker');
  const status = document.querySelector('#editor-pane > .fmd-pane-header > #fmd-save-status');
  if (!editor || !button || !picker || !status) return;
  let revision = 0, busy = false, selection = null, active = true;
  window.addEventListener('pagehide', () => { active = false; revision++; selection = null; });
  window.addEventListener('pageshow', () => { active = true; revision++; selection = null; });
  editor.addEventListener('input', () => { revision++; });
  const snapshot = () => ({source: editor.value, revision, start: editor.selectionStart, end: editor.selectionEnd});
  button.addEventListener('click', () => {
    if (busy) return;
    selection = snapshot();
    picker.click();
  });
  picker.addEventListener('cancel', () => { selection = null; });

  async function verify(uri, info) {
    const image = new Image();
    let timer;
    try {
      image.src = uri;
      await Promise.race([image.decode(), new Promise((_, reject) => {
        timer = setTimeout(() => reject(Error('Image decoding timed out')), 10000);
      })]);
      const width = image.naturalWidth, height = image.naturalHeight;
      fmdImportDimensions(width, height);
      // JPEG EXIF orientation may exchange the two axes.
      if (!(width === info.width && height === info.height) && !(width === info.height && height === info.width)) {
        throw Error('Decoded image dimensions do not match its header');
      }
    } finally {
      clearTimeout(timer);
      image.removeAttribute('src');
    }
  }

  async function importFiles(files, before) {
    if (busy) { status.textContent = 'An image import is already in progress'; return; }
    if (!files.length) return;
    busy = true; button.disabled = true; picker.disabled = true;
    status.textContent = 'Reading selected images…';
    let transaction = null, committed = false;
    try {
      if (before.source.length > FMD_IMAGE_IMPORT_LIMITS.sourceUnits) throw Error('Document exceeds the 32 Mi-character import limit');
      const current = () => active && revision === before.revision && editor.value === before.source;
      const candidate = Object.getOwnPropertyDescriptor(window, '__fmdNativeRuntime')?.value;
      const native = candidate?.version === 1 && typeof candidate.render === 'function'
        && typeof candidate.pdf === 'function' ? candidate : null;
      if (native && typeof native.stageImages !== 'function') {
        throw Error('Rebuild the matching workspace runtime to import native image resources');
      }
      const images = await fmdReadImageImport(files, verify, current);
      let markdown;
      if (native) ({transaction, markdown} = fmdNativeImageImport(images, native));
      else markdown = images.map(image => '![' + image.alt + '](' + image.uri + ')').join('\n\n');
      const prefix = before.source.slice(0, before.start), suffix = before.source.slice(before.end);
      const insertion = (prefix && !prefix.endsWith('\n\n') ? '\n\n' : '') + markdown + (suffix && !suffix.startsWith('\n\n') ? '\n\n' : '');
      const expected = prefix + insertion + suffix;
      if (expected.length > FMD_IMAGE_IMPORT_LIMITS.sourceUnits) throw Error('Insertion would exceed the 32 Mi-character document limit');
      if (native && new TextEncoder().encode(expected).length > 32 * 1024 * 1024) {
        throw Error('Insertion would exceed the native 32 MiB UTF-8 source limit');
      }
      if (!current()) throw Error('The document changed; select the images again');
      const app = document.querySelector('body > #fmd-app-body');
      if (app.classList.contains('view-read')) document.querySelector('body > .fmd-app-header #btn-toggle-view').click();
      editor.focus();
      editor.setSelectionRange(before.start, before.end);
      if (!current()) throw Error('The document changed; select the images again');
      // Publish before insertText can emit a synchronous input event. On an
      // unchanged-source editing failure, roll back both ABI buffers and saved
      // JSON. If a host partially edits, keep resources so references stay valid.
      if (transaction) { transaction.commit(); committed = true; }
      // On browsers supporting insertText, one insertion keeps native undo.
      // setRangeText is the non-undo-preserving fallback, never a full rewrite.
      if (typeof document.execCommand === 'function') {
        try { document.execCommand('insertText', false, insertion); }
        catch (error) { if (editor.value !== before.source) throw error; }
      }
      if (editor.value === before.source) editor.setRangeText(insertion, before.start, before.end, 'end');
      if (editor.value !== expected) throw Error('The editor changed during insertion; inspect the document before retrying');
      if (revision === before.revision) editor.dispatchEvent(new Event('input', {bubbles: true}));
      status.textContent = 'Inserted ' + files.length + (files.length === 1 ? ' image' : ' images')
        + (native ? ' — Save HTML to keep image resources; Markdown alone contains references' : ' — download to keep changes');
    } catch (error) {
      let rollbackError = '';
      if (committed && editor.value === before.source) {
        try { transaction.rollback(); }
        catch (failure) { rollbackError = '; resource rollback failed: ' + String(failure?.message || failure); }
      }
      status.textContent = 'Image insertion failed: ' + String(error?.message || error) + rollbackError;
    } finally {
      busy = false; button.disabled = false; picker.disabled = false;
    }
  }

  picker.addEventListener('change', () => {
    // Capture file handles and selection before the browser resets its picker.
    const files = Array.from(picker.files || []), before = selection || snapshot();
    selection = null;
    picker.value = ''; // Allow retrying the exact same selection.
    return importFiles(files, before);
  });
  editor.addEventListener('paste', event => {
    // Never read HTML, URLs, paths or ambient clipboard contents. Only File
    // objects explicitly delivered with this paste can become image resources.
    const files = Array.from(event.clipboardData?.files || []);
    if (!files.length) return; // Let the browser handle ordinary text normally.
    event.preventDefault();
    return importFiles(files, snapshot());
  });
  editor.addEventListener('dragover', event => {
    if (!Array.from(event.dataTransfer?.types || []).includes('Files')) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
  });
  editor.addEventListener('drop', event => {
    const files = Array.from(event.dataTransfer?.files || []);
    if (!files.length) return; // Do not turn dropped URLs into network requests.
    event.preventDefault(); // A rejected file must not navigate away from source.
    event.dataTransfer.dropEffect = 'copy'; // Never request a move of the original file.
    return importFiles(files, snapshot());
  });
})();

// The same bundled image-authoring asset also manages existing native bindings.
// Wait for parsing: Save HTML may contain a detached, transient dialog shell.
function fmdInstallImageManager(doc = document) {
  const win = doc.defaultView;
  if (!win) return;
  const native = Object.getOwnPropertyDescriptor(win, '__fmdNativeRuntime')?.value;
  if (native?.version !== 1 || native.imageMode !== 'worker' || typeof native.beginImageChanges !== 'function') return;
  const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
  const preview = doc.querySelector('body > #fmd-app-body > #preview-pane > #fmd-content');
  const header = doc.querySelector('body > .fmd-app-header');
  const save = header?.querySelector('#btn-save-html');
  const payload = doc.querySelector('body > script#fmd-native-runtime[type="application/json"]');
  if (!editor || !preview || !save || !payload || fmdImageManagerHosts.has(editor)) return;
  fmdImageManagerHosts.add(editor);
  header.querySelector('#btn-document-images')?.remove();
  doc.querySelector('body > dialog#fmd-document-images')?.remove();
  const make = (tag, text, parent) => {
    const node = doc.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (parent) parent.appendChild(node);
    return node;
  };
  const toggle = make('button', 'Document images'); toggle.type = 'button';
  toggle.id = 'btn-document-images'; toggle.className = 'fmd-btn';
  toggle.setAttribute('aria-haspopup', 'dialog'); toggle.setAttribute('aria-controls', 'fmd-document-images');
  save.parentNode.insertBefore(toggle, save);
  const dialog = make('dialog', undefined, doc.body); dialog.id = 'fmd-document-images';
  dialog.setAttribute('aria-labelledby', 'fmd-images-title');
  dialog.style.cssText = 'width:min(92vw,52rem);max-height:90vh;overflow:auto;box-sizing:border-box;margin:auto;padding:24px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222)';
  make('h2', 'Document images', dialog).id = 'fmd-images-title';
  make('p', 'Replace an embedded image without changing its Markdown destination or alt text. Remove explicitly to reclaim resource space. Nothing is fetched from a URL or written to the original local file.', dialog);
  const inventory = make('p', '', dialog); inventory.id = 'fmd-images-inventory';
  const label = make('label', 'Embedded image', dialog);
  const select = make('select', undefined, label); select.id = 'fmd-image-selection'; select.size = 6;
  select.style.cssText = 'display:block;width:100%;box-sizing:border-box;font:inherit;margin:8px 0';
  const destination = make('p', '', dialog); destination.id = 'fmd-image-destination'; destination.style.overflowWrap = 'anywhere';
  const fileLabel = make('label', 'Replacement file (static PNG or JPEG, at most 8 MiB)', dialog);
  const file = make('input', undefined, fileLabel); file.type = 'file'; file.accept = 'image/png,image/jpeg,.png,.jpg,.jpeg';
  file.id = 'fmd-image-replacement'; file.style.cssText = 'display:block;margin:8px 0;max-width:100%';
  make('p', 'Removal does not rewrite source references or infer which images are unused. A remaining reference may produce a missing-image diagnostic. Save HTML before removing resources you may need later; source undo does not restore them.', dialog);
  const actions = make('div', undefined, dialog); actions.style.cssText = 'display:flex;flex-wrap:wrap;gap:12px;margin:12px 0';
  const button = (id, text) => {
    const node = make('button', text, actions); node.id = id; node.type = 'button'; node.className = 'fmd-btn'; return node;
  };
  const replace = button('btn-replace-document-image', 'Replace selected image');
  const remove = button('btn-remove-document-image', 'Remove selected image');
  const cancel = button('btn-cancel-image-change', 'Cancel operation');
  const close = button('btn-close-document-images', 'Close');
  const status = make('p', 'Select an embedded image to manage.', dialog);
  status.id = 'fmd-images-status'; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  status.style.overflowWrap = 'anywhere';
  let pending = null, revision = 0, suspended = false, composing = false, inventoryIdentity = null, modified = false;
  const display = () => {
    const scale = parseFloat(doc.documentElement.style.getPropertyValue('--fmd-base')) / 16;
    return {scale: Number.isFinite(scale) ? Math.min(2, Math.max(0.7, scale)) : 1,
      theme: doc.body.classList.contains('theme-dark') ? 'dark' : doc.body.classList.contains('theme-light') ? 'light' : undefined};
  };
  const active = () => !suspended && !composing && !editor.disabled && !editor.readOnly;
  function refresh() {
    const entries = native.imageAssets;
    if (entries !== inventoryIdentity) {
      const chosen = select.value; select.replaceChildren();
      for (const entry of entries ?? []) {
        const option = make('option', entry.destination + ' — ' + entry.bytes.toLocaleString() + ' bytes', select);
        option.value = entry.destination;
      }
      if (entries?.some(entry => entry.destination === chosen)) select.value = chosen;
      else if (entries?.length) select.selectedIndex = 0;
      inventoryIdentity = entries;
    }
    const selected = entries?.find(entry => entry.destination === select.value);
    inventory.textContent = entries ? entries.length + ' embedded images · '
      + entries.reduce((total, entry) => total + entry.bytes, 0).toLocaleString() + ' bytes'
      : 'Native renderer is initializing…';
    destination.textContent = selected ? selected.destination : 'No embedded image selected. Use Insert image to add one.';
    toggle.disabled = suspended || !entries;
    select.disabled = file.disabled = !active() || pending !== null;
    replace.disabled = !active() || pending !== null || !selected || file.files?.length !== 1;
    remove.disabled = !active() || pending !== null || !selected;
    cancel.disabled = pending === null;
    dialog.setAttribute('aria-busy', String(pending !== null));
  }
  function cancelPending(message = 'Image change cancelled; committed resources are unchanged.') {
    const operation = pending;
    if (!operation) return;
    pending = null;
    operation.cancelRead?.(); operation.handle?.cancel();
    status.textContent = message; refresh();
  }
  function changed(message) { revision++; cancelPending(message); refresh(); }
  const current = operation => pending === operation && dialog.open && active()
    && revision === operation.revision && editor.value === operation.source
    && native.documentRevision === operation.resources && select.value === operation.destination
    && JSON.stringify(display()) === operation.display && file.files?.[0] === operation.file;
  function readReplacement(operation) {
    const selected = operation.file;
    if (!selected || !Number.isSafeInteger(selected.size) || selected.size < 1 || selected.size > FMD_IMAGE_IMPORT_LIMITS.fileBytes) {
      throw Error('Choose one replacement file containing 1 byte through 8 MiB');
    }
    return new Promise((resolve, reject) => {
      const reader = new win.FileReader(), image = new win.Image(); let done = false, timer;
      const check = () => { if (!current(operation)) throw Error('Document or image selection changed; choose the replacement again'); };
      function settle(error, bytes) {
        if (done) return;
        done = true; win.clearTimeout(timer); operation.cancelRead = null;
        reader.onload = reader.onerror = reader.onabort = null;
        if (reader.readyState === 1) reader.abort();
        image.removeAttribute('src');
        if (error) reject(error); else resolve(bytes);
      }
      operation.cancelRead = () => settle(Error('Image reading cancelled'));
      reader.onerror = () => settle(Error('Unable to read the selected image'));
      reader.onabort = () => settle(Error('Image reading cancelled'));
      reader.onload = async () => {
        try {
          check();
          const buffer = reader.result;
          const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
          if (length !== selected.size) throw Error('Image size changed while reading');
          const bytes = new Uint8Array(buffer), info = fmdImportImageInfo(bytes);
          image.src = 'data:' + info.mime + ';base64,' + fmdImportBase64(bytes);
          await image.decode(); check();
          const width = image.naturalWidth, height = image.naturalHeight;
          fmdImportDimensions(width, height);
          if (!(width === info.width && height === info.height) && !(width === info.height && height === info.width)) {
            throw Error('Decoded image dimensions do not match its header');
          }
          settle(null, bytes);
        } catch (error) { settle(error); }
      };
      timer = win.setTimeout(() => settle(Error('Image reading or decoding exceeded ten seconds; retry explicitly')), 10000);
      try { check(); reader.readAsArrayBuffer(selected); } catch (error) { settle(error); }
    });
  }
  async function apply(removal) {
    if (pending || !active() || !dialog.open) return;
    let operation = null;
    try {
      if (native.settingsPending || native.exportPending) throw Error('Finish or cancel the current document operation first');
      const entry = native.imageAssets?.find(entry => entry.destination === select.value);
      if (!entry) throw Error('Select an existing embedded image');
      operation = {destination: entry.destination, file: file.files?.[0], source: editor.value,
        revision, resources: native.documentRevision, display: JSON.stringify(display()), handle: null, cancelRead: null};
      pending = operation; refresh();
      if (removal) {
        const accepted = win.confirm('Remove the embedded image “' + entry.destination + '”?\n\nMarkdown references and the original local file stay unchanged. Source undo cannot restore this resource. Save HTML first to retain a backup.');
        if (!current(operation)) throw Error('Document changed during confirmation; select the image again');
        if (!accepted) { status.textContent = 'Removal declined; image is unchanged.'; return; }
      }
      const patch = {destination: entry.destination};
      if (removal) patch.remove = true;
      else {
        status.textContent = 'Reading and validating the local replacement image…';
        patch.bytes = await readReplacement(operation);
      }
      if (!current(operation)) throw Error('Document or image selection changed; try again');
      status.textContent = 'Validating document images in the native background worker…';
      operation.handle = native.beginImageChanges([patch], operation.source, preview, JSON.parse(operation.display), () => current(operation));
      await operation.handle.promise;
      // Committed resources now have a new revision; do not reject our own edit.
      if (pending !== operation) return;
      file.value = '';
      status.textContent = native.documentRevision === operation.resources ? 'Replacement already matches; nothing changed.'
        : (removal ? 'Image removed.' : 'Image replaced at the same Markdown destination.') + ' Source is unchanged; Save HTML to keep resources.';
    } catch (error) {
      if (!operation || pending === operation) status.textContent = 'Image not changed: ' + String(error?.message ?? error).slice(0, 2048);
    } finally {
      if (pending === operation) pending = null;
      refresh();
    }
  }
  toggle.addEventListener('click', () => { if (!toggle.disabled && !dialog.open) { file.value = ''; refresh(); dialog.showModal(); select.focus(); } });
  replace.addEventListener('click', () => { apply(false); });
  remove.addEventListener('click', () => { apply(true); });
  cancel.addEventListener('click', () => cancelPending());
  close.addEventListener('click', () => { cancelPending(); file.value = ''; dialog.close(); toggle.focus(); });
  dialog.addEventListener('cancel', () => { cancelPending(); file.value = ''; });
  dialog.addEventListener('close', () => { if (!dialog.open) { cancelPending(); file.value = ''; } });
  select.addEventListener('change', () => { file.value = ''; changed('Image selection changed; retry with the selected resource.'); });
  file.addEventListener('change', () => changed('Replacement selection changed; apply it again.'));
  editor.addEventListener('input', () => changed('Source changed; apply the image change again.'));
  editor.addEventListener('compositionstart', () => { composing = true; changed('Text composition started; image change cancelled.'); });
  editor.addEventListener('compositionend', () => { composing = false; changed(); });
  win.addEventListener('pagehide', () => { suspended = true; composing = false; changed('Workspace suspended; image change cancelled.'); file.value = ''; });
  win.addEventListener('pageshow', () => { suspended = false; composing = false; refresh(); });
  win.addEventListener('fmd-native-images-changed', () => {
    modified = true;
    const notice = doc.querySelector('#editor-pane > .fmd-pane-header > #fmd-save-status');
    if (notice) notice.textContent = 'Images modified — Save HTML to keep embedded resources';
    refresh();
  });
  win.addEventListener('beforeunload', event => { if (modified) { event.preventDefault(); event.returnValue = ''; } });
  const viewObserver = new win.MutationObserver(() => {
    if (pending && !current(pending)) cancelPending('Source, view or document state changed; retry the image change.');
    refresh();
  });
  viewObserver.observe(doc.documentElement, {attributes: true, attributeFilter: ['style']});
  viewObserver.observe(doc.body, {attributes: true, attributeFilter: ['class']});
  viewObserver.observe(editor, {attributes: true, attributeFilter: ['readonly', 'disabled']});
  // Resource publication may be our own successful transaction. It is checked
  // by the native revision guard; only idle inventories are refreshed here.
  new win.MutationObserver(() => { if (!pending) refresh(); }).observe(payload, {childList: true, characterData: true, subtree: true});
  refresh(); if (native.ready) native.ready.then(refresh, refresh);
}
const fmdImageManagerHosts = new WeakSet();
if (typeof document !== 'undefined') {
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => fmdInstallImageManager(), {once: true});
  else fmdInstallImageManager();
}
