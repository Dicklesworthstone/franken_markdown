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

async function fmdPrepareImageImport(files, verify, current) {
  const limit = FMD_IMAGE_IMPORT_LIMITS;
  if (!files.length || files.length > limit.files) throw Error('Choose between 1 and 8 images');
  let total = 0;
  for (const file of files) {
    if (!Number.isSafeInteger(file.size) || file.size < 1 || file.size > limit.fileBytes) {
      throw Error('Each image must contain 1 byte to 8 MiB');
    }
    total += file.size;
  }
  if (total > limit.batchBytes) throw Error('Selected images exceed the 16 MiB batch limit');
  const checkCurrent = () => {
    if (!current()) throw Error('The document changed while images were being read; select the images again');
  };
  const markdown = [];
  for (const file of files) {
    checkCurrent();
    const bytes = new Uint8Array(await file.arrayBuffer());
    checkCurrent();
    if (bytes.length !== file.size) throw Error('Image size changed while reading');
    const info = fmdImportImageInfo(bytes);
    const uri = 'data:' + info.mime + ';base64,' + fmdImportBase64(bytes);
    await verify(uri, info);
    checkCurrent();
    markdown.push('![' + fmdImportAlt(file.name) + '](' + uri + ')');
  }
  return markdown.join('\n\n');
}

// Keep pure admission helpers executable by the Node regression harness.
if (typeof document !== 'undefined') (function() {
  'use strict';
  const editor = document.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
  const button = document.querySelector('body > .fmd-app-header #btn-insert-image');
  const picker = document.querySelector('body > .fmd-app-header #fmd-image-picker');
  const status = document.querySelector('#editor-pane > .fmd-pane-header > #fmd-save-status');
  if (!editor || !button || !picker || !status) return;
  let revision = 0, busy = false, selection = null;
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
    try {
      image.src = uri;
      await image.decode();
      const width = image.naturalWidth, height = image.naturalHeight;
      fmdImportDimensions(width, height);
      // JPEG EXIF orientation may exchange the two axes.
      if (!(width === info.width && height === info.height) && !(width === info.height && height === info.width)) {
        throw Error('Decoded image dimensions do not match its header');
      }
    } finally {
      image.removeAttribute('src');
    }
  }

  picker.addEventListener('change', async () => {
    if (busy) return;
    const files = Array.from(picker.files || []);
    const before = selection || snapshot();
    selection = null;
    picker.value = ''; // Allow retrying the exact same selection.
    if (!files.length) return;
    busy = true; button.disabled = true; picker.disabled = true;
    status.textContent = 'Reading selected images…';
    try {
      if (before.source.length > FMD_IMAGE_IMPORT_LIMITS.sourceUnits) throw Error('Document exceeds the 32 Mi-character import limit');
      const current = () => revision === before.revision && editor.value === before.source;
      const markdown = await fmdPrepareImageImport(files, verify, current);
      const prefix = before.source.slice(0, before.start), suffix = before.source.slice(before.end);
      const insertion = (prefix && !prefix.endsWith('\n\n') ? '\n\n' : '') + markdown + (suffix && !suffix.startsWith('\n\n') ? '\n\n' : '');
      const expected = prefix + insertion + suffix;
      if (expected.length > FMD_IMAGE_IMPORT_LIMITS.sourceUnits) throw Error('Insertion would exceed the 32 Mi-character document limit');
      if (!current()) throw Error('The document changed; select the images again');
      const app = document.querySelector('body > #fmd-app-body');
      if (app.classList.contains('view-read')) document.querySelector('body > .fmd-app-header #btn-toggle-view').click();
      editor.focus();
      editor.setSelectionRange(before.start, before.end);
      // On browsers supporting insertText, one insertion keeps native undo.
      // setRangeText is the non-undo-preserving fallback, never a full rewrite.
      if (typeof document.execCommand === 'function') document.execCommand('insertText', false, insertion);
      if (editor.value === before.source) editor.setRangeText(insertion, before.start, before.end, 'end');
      if (editor.value !== expected) throw Error('The editor changed during insertion; inspect the document before retrying');
      if (revision === before.revision) editor.dispatchEvent(new Event('input', {bubbles: true}));
      status.textContent = 'Inserted ' + files.length + (files.length === 1 ? ' image' : ' images') + ' — download to keep changes';
    } catch (error) {
      status.textContent = 'Image insertion failed: ' + String(error?.message || error);
    } finally {
      busy = false; button.disabled = false; picker.disabled = false;
    }
  });
})();
