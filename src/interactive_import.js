// Explicit local PNG/JPEG/GIF/SVG authoring for standalone workspaces. No fetch,
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

// SVG admission is an offline resource policy, not another SVG renderer. The
// browser's inert XML parser checks structure; existing native and browser
// renderers still interpret geometry, paint and text. Never insert these nodes
// into the editor DOM or rewrite/rasterize the original vector bytes.
function fmdImportSvgLength(value) {
  const match = /^\s*([+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)\s*(px|in|cm|mm|q|pt|pc)?\s*$/i.exec(value);
  if (!match) throw Error('SVG dimensions must use positive numbers or absolute CSS lengths');
  // Match native svg_absolute_length_multiplier: CSS pixels are 96 per inch.
  const units = {px: 1, in: 96, cm: 96 / 2.54, mm: 96 / 25.4, q: 96 / 101.6, pt: 96 / 72, pc: 16};
  const size = Number(match[1]) * units[(match[2] || 'px').toLowerCase()];
  if (!Number.isFinite(size) || size <= 0 || size > FMD_IMAGE_IMPORT_LIMITS.dimension) {
    throw Error('SVG dimensions exceed 16,384 per side or are not positive');
  }
  return size;
}

const fmdImportSvgUrlTrim = value => value.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '');

function fmdImportSvgStyle(value) {
  // Decode CSS escapes before checking resource-bearing tokens. Checking raw
  // "url"/"@import" strings misses legal CSS such as u\\72l and @\\69mport.
  // https://www.w3.org/TR/css-syntax-3/#consume-escaped-code-point
  const parts = [];
  let position = 0, quote = null;
  for (let i = 0; i < value.length; i++) {
    const character = value[i];
    if (character === '\\') { i++; continue; }
    if (quote) {
      if (character === quote || /[\n\r\f]/.test(character)) quote = null;
    } else if (character === '"' || character === "'") quote = character;
    else if (character === '/' && value[i + 1] === '*') {
      const end = value.indexOf('*/', i + 2);
      if (end < 0) throw Error('SVG contains an incomplete style comment');
      // In an unquoted url(...) token, /*...*/ is URL data, not a CSS
      // comment. Retain a marker until resource checks finish; stripping it
      // could turn an external "/*path*/#id" into an apparent local "#id".
      parts.push(value.slice(position, i), '/*fmd-comment*/'); position = end + 2; i = end + 1;
    }
  }
  parts.push(value.slice(position));
  const text = parts.join('').replace(/\\(?:([0-9a-f]{1,6})(?:\r\n|[\t\n\f\r ])?|([\s\S]))/gi,
    (all, hex, character) => {
      // Escaped quotes are URL data, not string delimiters. Preserve that
      // distinction so url(\\27#paint\\27) cannot masquerade as url('#paint').
      if (!hex) return character === '"' || character === "'" ? all : /[\n\r\f]/.test(character) ? '' : character;
      const cp = parseInt(hex, 16);
      if (cp === 34 || cp === 39) return all;
      return !cp || cp > 0x10ffff || cp >= 0xd800 && cp <= 0xdfff ? '\ufffd' : String.fromCodePoint(cp);
    });
  if (/@|(?:^|[^\w-])(?:image|image-set|-webkit-image-set|src)\s*\(/i.test(text)) {
    throw Error('SVG styles must not import stylesheets, fonts or external images');
  }
  const references = /\burl\s*\(/gi;
  while (references.exec(text)) {
    const end = text.indexOf(')', references.lastIndex);
    if (end < 0) throw Error('SVG contains an incomplete style URL');
    const target = fmdImportSvgUrlTrim(fmdImportSvgUrlTrim(text.slice(references.lastIndex, end))
      .replace(/^(?:"([^"]*)"|'([^']*)')$/, (_all, double, single) => double ?? single));
    if (!/^#[^\s"'()<>\\]+$/u.test(target)) throw Error('SVG paint and filter references must stay inside this image');
    references.lastIndex = end + 1;
  }
  // CSS can override an SVG root's intrinsic size. Bound every sizing
  // declaration conservatively before Image.decode; do not evaluate var/calc
  // expressions or rely on the selected file's untrusted root attributes alone.
  const bounds = {width: 0, height: 0};
  const sizing = text.replaceAll('/*fmd-comment*/', '');
  if (/(?:^|[;{\s])(?:aspect-ratio|(?:min-|max-)?(?:inline|block)-size|contain-intrinsic[^:;{}]*)\s*:/i.test(sizing)) {
    throw Error('SVG styles require explicit bounded width and height');
  }
  for (const match of sizing.matchAll(/(?:^|[;{\s])(?:min-|max-)?(width|height)\s*:\s*([^;{}]+)/gi)) {
    const size = match[2].trim().replace(/\s*!important\s*$/i, '');
    if (/^(?:auto|none|initial|inherit|unset)$/i.test(size)) continue;
    if (/^(?:\d+(?:\.\d*)?|\.\d+)%$/.test(size) && parseFloat(size) <= 100) continue;
    const length = fmdImportSvgLength(size);
    bounds[match[1].toLowerCase()] = Math.max(bounds[match[1].toLowerCase()], length);
  }
  return bounds;
}

function fmdImportSvgResource(value, element) {
  // URL parsing removes ASCII whitespace, not NBSP or other Unicode spaces.
  // Treating "\u00a0#id" as a fragment would authorize an external relative URL.
  const target = fmdImportSvgUrlTrim(value);
  if (/^#[^\s"'<>\\]+$/u.test(target) && !['image', 'feimage'].includes(element)) return null;
  // Hyperlinks are not fetched while decoding an image. Preserve ordinary
  // native PDF links, but never admit active, local-file or embedded-page URLs.
  if (element === 'a' && /^(?:https?:\/\/|mailto:)[^\s\u0000-\u001f\u007f]+$/iu.test(target)) return null;
  const match = /^(data:image\/(?:png|jpeg|gif));base64,([A-Za-z0-9+/]+={0,2})$/.exec(target);
  if (!['image', 'feimage'].includes(element) || !match || match[2].length % 4 !== 0) {
    throw Error('SVG resources must be internal fragments or embedded PNG, JPEG or GIF images');
  }
  const binary = atob(match[2]), bytes = Uint8Array.from(binary, c => c.charCodeAt(0));
  const info = fmdImportImageInfo(bytes);
  if (fmdImportBase64(bytes) !== match[2] || 'data:' + info.mime !== match[1]) {
    throw Error('SVG embedded image bytes do not match their declared format');
  }
  return {pixels: info.width * info.height, bytes: bytes.length};
}

function fmdImportSvgInfo(bytes) {
  let source;
  try { source = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true}).decode(bytes); }
  catch { throw Error('SVG images must contain valid UTF-8'); }
  // Refuse DTD/entity processing before parsing, including expansion attacks.
  // Comments and CDATA are inert text and may legitimately describe a DTD.
  let position = 0, open, tags = 0;
  while ((open = source.indexOf('<', position)) !== -1) {
    const comment = source.startsWith('<!--', open), cdata = source.startsWith('<![CDATA[', open);
    if (comment || cdata) {
      const end = source.indexOf(comment ? '-->' : ']]>', open + (comment ? 4 : 9));
      if (end < 0) throw Error('SVG contains an incomplete XML comment or CDATA section');
      position = end + 3;
    } else {
      if (/^<!(?:DOCTYPE|ENTITY)\b/i.test(source.slice(open, open + 12))) throw Error('SVG document types and entity declarations are not supported');
      if (++tags > 32768) throw Error('SVG exceeds the 32,768-node import limit');
      position = open + 1;
    }
  }
  if (typeof DOMParser !== 'function') throw Error('SVG import requires the browser XML parser');
  const parsed = new DOMParser().parseFromString(source, 'image/svg+xml'), root = parsed.documentElement;
  const svgNamespace = 'http://www.w3.org/2000/svg';
  if (!root || root.localName !== 'svg' || root.namespaceURI !== svgNamespace
      || parsed.getElementsByTagName('parsererror').length || parsed.doctype) {
    throw Error('Choose a well-formed SVG image with an SVG namespace');
  }
  const viewBox = root.getAttribute('viewBox');
  let view = null;
  if (viewBox !== null) {
    const tokens = viewBox.trim().split(/[\s,]+/);
    if (tokens.length !== 4 || tokens.some(value => !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(value))) {
      throw Error('SVG viewBox must contain four finite numbers');
    }
    view = tokens.map(Number);
    if (view.some(value => !Number.isFinite(value)) || view[2] <= 0 || view[3] <= 0) throw Error('SVG viewBox dimensions must be positive');
  }
  const dimension = (name, index) => {
    const value = root.getAttribute(name)?.trim();
    if (!value || /^(?:\d+(?:\.\d*)?|\.\d+)%$/.test(value) && parseFloat(value) <= 100) {
      if (!view) throw Error('SVG requires explicit width and height or a positive viewBox');
      return fmdImportSvgLength(String(view[index]));
    }
    return fmdImportSvgLength(value);
  };
  const width = dimension('width', 2), height = dimension('height', 3);
  fmdImportDimensions(Math.ceil(width), Math.ceil(height));
  // Account for the default browser image viewport as well as native geometry.
  let widest = Math.max(width, 300), tallest = Math.max(height, 150), count = 0;
  let rasterPixels = 0, rasterBytes = 0;
  const ids = new Map(), references = new Map(), pixels = new Map();
  const stack = [{node: parsed, depth: 0, metadata: false}];
  while (stack.length) {
    const {node, depth, metadata} = stack.pop();
    if (++count > 32768 || depth > 128) throw Error('SVG exceeds the 32,768-node or 128-level import limit');
    if (node.nodeType === 7 || node.nodeType === 10) throw Error('SVG processing instructions and document types are not supported');
    let insideMetadata = metadata;
    if (node.nodeType === 1) {
      const name = node.localName.toLowerCase();
      insideMetadata ||= name === 'metadata' && node.namespaceURI === svgNamespace;
      if (!insideMetadata && node.namespaceURI !== svgNamespace
          || ['script', 'handler', 'listener', 'foreignobject', 'iframe', 'object', 'embed', 'audio', 'video', 'link',
            'animate', 'animatemotion', 'animatetransform', 'set', 'discard'].includes(name)) {
        throw Error('SVG import supports static vector images without scripts, animation or foreign content');
      }
      const id = node.getAttribute('id');
      if (id && !ids.has(id)) ids.set(id, node); // Match first-in-document ID ownership.
      const href = node.getAttribute('href') ?? node.getAttributeNS('http://www.w3.org/1999/xlink', 'href');
      if (name !== 'a' && href !== null && fmdImportSvgUrlTrim(href).startsWith('#')) {
        try { references.set(node, decodeURIComponent(fmdImportSvgUrlTrim(href).slice(1))); }
        catch { throw Error('SVG contains an invalid local reference'); }
      }
      const checkStyle = value => {
        const bounds = fmdImportSvgStyle(value);
        widest = Math.max(widest, bounds.width); tallest = Math.max(tallest, bounds.height);
      };
      if (name === 'style') checkStyle(node.textContent);
      for (const attribute of node.attributes) {
        const key = attribute.localName.toLowerCase(), value = attribute.value;
        if (/^on/i.test(key) || key === 'base' && attribute.namespaceURI === 'http://www.w3.org/XML/1998/namespace') {
          throw Error('SVG event handlers and external base URLs are not supported');
        }
        if (key === 'href' || key === 'src') {
          const raster = fmdImportSvgResource(value, name);
          if (raster) {
            rasterPixels += raster.pixels; rasterBytes += raster.bytes;
            pixels.set(node, (pixels.get(node) || 0) + raster.pixels);
            if (rasterPixels > FMD_IMAGE_IMPORT_LIMITS.pixels || rasterBytes > FMD_IMAGE_IMPORT_LIMITS.fileBytes) {
              throw Error('SVG embedded images exceed 24 million pixels or 8 MiB in total');
            }
          }
        }
        if (key === 'style' || /(?:url|\\)\s*/i.test(value)) checkStyle(value);
      }
    }
    for (let child = node.lastChild; child; child = child.previousSibling) {
      stack.push({node: child, depth: depth + 1, metadata: insideMetadata});
    }
  }
  // A small XML file can describe exponentially many <use> shadow nodes. Count
  // local reference expansion without constructing it, including raster pixels
  // and text/path payload repeated through a referenced group. Memoization keeps admission linear in
  // the supplied tree/edges; cycles and deep reference chains fail closed.
  const totals = new Map(), visiting = new Set();
  const textBytes = value => {
    let size = 0;
    for (const character of value) {
      const cp = character.codePointAt(0);
      size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
    }
    return size;
  };
  function expanded(node, depth) {
    if (depth > 128) throw Error('SVG local references exceed the 128-level import limit');
    if (visiting.has(node)) throw Error('SVG contains cyclic local references');
    if (totals.has(node)) {
      const saved = totals.get(node);
      if (depth + saved.depth > 128) throw Error('SVG local references exceed the 128-level import limit');
      return saved;
    }
    visiting.add(node);
    let payload = textBytes(node.nodeValue || node.nodeName);
    if (node.nodeType === 1) for (const attribute of node.attributes) {
      payload += textBytes(attribute.name) + textBytes(attribute.value);
    }
    const result = {nodes: 1, pixels: pixels.get(node) || 0, payload, depth: 0};
    const include = child => {
      const size = expanded(child, depth + 1);
      result.nodes += size.nodes; result.pixels += size.pixels; result.payload += size.payload;
      result.depth = Math.max(result.depth, size.depth + 1);
      if (result.nodes > 32768 || result.pixels > FMD_IMAGE_IMPORT_LIMITS.pixels) {
        throw Error('SVG expanded references exceed 32,768 nodes or 24 million embedded pixels');
      }
      if (result.payload > FMD_IMAGE_IMPORT_LIMITS.fileBytes) throw Error('SVG expanded text and attributes exceed 8 MiB');
    };
    for (const child of node.childNodes) include(child);
    const target = ids.get(references.get(node));
    if (target) include(target);
    visiting.delete(node); totals.set(node, result); return result;
  }
  expanded(root, 0);
  fmdImportDimensions(Math.ceil(widest), Math.ceil(tallest));
  return {mime: 'image/svg+xml', width: Math.ceil(width), height: Math.ceil(height)};
}

// Validate GIF87a/89a framing and every frame's bounds before browser decoding.
// Preserve the original stream, including palettes, interlacing, transparency
// and animation; this is not a replacement LZW decoder.
// Container reference: https://www.w3.org/Graphics/GIF/spec-gif89a.txt
function fmdImportGifInfo(bytes) {
  let offset = 6, frames = 0;
  const invalid = () => { throw Error('Invalid or truncated GIF container'); };
  const take = length => {
    if (length > bytes.length - offset) invalid();
    const start = offset; offset += length; return start;
  };
  const byte = () => bytes[take(1)];
  const short = start => bytes[start] + bytes[start + 1] * 256;
  const palette = packed => {
    const colors = packed & 0x80 ? 2 << (packed & 7) : 0;
    take(colors * 3); return colors;
  };
  const blocks = () => {
    let total = 0, length;
    while ((length = byte()) !== 0) { take(length); total += length; }
    return total;
  };
  const screen = take(7), width = short(screen), height = short(screen + 2);
  fmdImportDimensions(width, height);
  const global = palette(bytes[screen + 4]);
  if (global && bytes[screen + 5] >= global) invalid();
  while (offset < bytes.length) {
    const tag = byte();
    if (tag === 0x21) {
      const label = byte();
      if (label === 0xf9) {
        if (byte() !== 4) invalid();
        take(4);
        if (byte() !== 0) invalid();
      } else if (label === 0x01) {
        throw Error('GIF plain-text graphics are not supported; choose a raster image');
      } else blocks();
    } else if (tag === 0x2c) {
      const descriptor = take(9), left = short(descriptor), top = short(descriptor + 2);
      const frameWidth = short(descriptor + 4), frameHeight = short(descriptor + 6);
      if (!frameWidth || !frameHeight || left + frameWidth > width || top + frameHeight > height) {
        throw Error('GIF frame dimensions fall outside the image canvas');
      }
      const local = palette(bytes[descriptor + 8]);
      if (!local && !global) invalid();
      const codeSize = byte();
      if (codeSize < 2 || codeSize > 8 || !blocks()) invalid();
      frames++;
    } else if (tag === 0x3b) {
      if (!frames || offset !== bytes.length) invalid();
      return {mime: 'image/gif', width, height};
    } else invalid();
  }
  return invalid();
}

// Inspect dimensions before allowing browser decoding. The decoder still has
// to validate the real image, not merely these container/frame headers.
function fmdImportImageInfo(bytes) {
  if (!bytes.length || bytes.length > FMD_IMAGE_IMPORT_LIMITS.fileBytes) {
    throw Error('Each image must contain 1 byte to 8 MiB');
  }
  const u16 = n => bytes[n] * 256 + bytes[n + 1];
  const u32 = n => bytes[n] * 0x1000000 + bytes[n + 1] * 0x10000 + bytes[n + 2] * 256 + bytes[n + 3];
  if (bytes.length >= 6 && ['GIF87a', 'GIF89a'].includes(String.fromCharCode(...bytes.subarray(0, 6)))) {
    return fmdImportGifInfo(bytes);
  }
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
      if (tag === 'acTL') throw Error('Animated PNG import is not supported; choose a static PNG, JPEG or GIF');
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
  // SVG is UTF-8 XML, optionally preceded by a BOM or whitespace. Other bytes
  // remain unsupported; file extensions and browser-provided MIME are not proof.
  let start = bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf ? 3 : 0;
  while (start < bytes.length && [9, 10, 13, 32].includes(bytes[start])) start++;
  if (bytes[start] === 60) return fmdImportSvgInfo(bytes);
  throw Error('Choose a PNG, JPEG, GIF or SVG image (file contents, not its extension, determine the format)');
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

async function fmdReadImageImport(files, verify, current, read = file => file.arrayBuffer()) {
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
    const buffer = await read(file);
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
    return {destination: 'fmd-import/' + id + (image.mime === 'image/png' ? '.png' : image.mime === 'image/gif' ? '.gif'
      : image.mime === 'image/svg+xml' ? '.svg' : '.jpg'), bytes: image.bytes};
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
  let revision = 0, pending = null, selection = null, active = true, composing = false;
  let cancelButton = document.querySelector('body > .fmd-app-header #btn-cancel-image-import');
  if (!cancelButton) {
    cancelButton = document.createElement('button');
    cancelButton.id = 'btn-cancel-image-import'; cancelButton.type = 'button';
    cancelButton.className = 'fmd-btn';
    button.parentNode.insertBefore(cancelButton, button.nextSibling);
  }
  cancelButton.textContent = 'Cancel image import';
  cancelButton.title = 'Stop reading or decoding selected images; source and embedded resources are unchanged';
  const editable = () => active && !composing && !editor.disabled && !editor.readOnly && editor.isConnected !== false;
  function refresh() {
    button.disabled = picker.disabled = !editable() || pending !== null;
    cancelButton.hidden = pending === null;
    // Toolbar display rules override the browser's default [hidden] style.
    cancelButton.style.display = pending === null ? 'none' : '';
    cancelButton.disabled = pending === null;
  }
  function cancel(message = 'Image import cancelled; source and embedded resources are unchanged') {
    const operation = pending;
    if (!operation) return;
    // Retire before abort callbacks run. A late read/decode/finally from this
    // operation cannot publish into, or release controls owned by, a retry.
    pending = null;
    operation.stop?.(Error(message));
    status.textContent = message;
    refresh();
  }
  function changed(message, sourceInput = false) {
    revision++;
    if (selection) selection.retired = true;
    if (!sourceInput || !pending?.installing) cancel(message);
    refresh();
  }
  window.addEventListener('pagehide', () => {
    active = false; composing = false;
    changed('Image import cancelled: the document changed or workspace suspended; select the files again after returning');
    picker.value = '';
  });
  window.addEventListener('pageshow', () => { active = true; composing = false; changed('Image import cancelled: workspace restored'); });
  editor.addEventListener('input', () => changed('Image import cancelled: the document changed; select the images again', true));
  editor.addEventListener('compositionstart', () => {
    composing = true; changed('Image import cancelled: text composition started');
  });
  editor.addEventListener('compositionend', () => { composing = false; changed('Image import cancelled: text composition ended'); });
  // Even a read-only/disabled round trip revokes a pending insertion. Checks
  // below also cover a property change before observer delivery.
  new MutationObserver(() => changed('Image import cancelled: editor access changed')).observe(editor,
    {attributes: true, attributeFilter: ['readonly', 'disabled']});
  const snapshot = () => ({source: editor.value, revision, start: editor.selectionStart, end: editor.selectionEnd});
  button.addEventListener('click', () => {
    if (pending || !editable()) return;
    selection = snapshot();
    picker.click();
  });
  picker.addEventListener('cancel', () => { selection = null; cancel(); });
  cancelButton.addEventListener('click', () => cancel());

  function readFile(file, operation) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader(); let done = false, timer;
      const settle = (error, buffer) => {
        if (done) return;
        done = true; clearTimeout(timer);
        if (operation.stop === stop) operation.stop = null;
        reader.onload = reader.onerror = reader.onabort = null;
        if (reader.readyState === 1) reader.abort();
        if (error) reject(error); else resolve(buffer);
      };
      const stop = error => settle(error);
      operation.stop = stop;
      reader.onload = () => settle(null, reader.result);
      reader.onerror = () => settle(Error('Unable to read the selected image'));
      reader.onabort = () => settle(Error('Image reading cancelled'));
      timer = setTimeout(() => settle(Error('Image reading exceeded ten seconds; select the file again')), 10000);
      try { reader.readAsArrayBuffer(file); } catch (error) { settle(error); }
    });
  }

  function verify(uri, info, operation) {
    return new Promise((resolve, reject) => {
      const image = new Image(); let done = false, timer;
      const settle = error => {
        if (done) return;
        done = true; clearTimeout(timer);
        if (operation.stop === stop) operation.stop = null;
        image.removeAttribute('src');
        if (error) reject(error); else resolve();
      };
      const stop = error => settle(error);
      operation.stop = stop;
      timer = setTimeout(() => settle(Error('Image decoding exceeded ten seconds; select the file again')), 10000);
      try {
        image.src = uri;
        Promise.resolve(image.decode()).then(() => {
          if (done) return;
          try {
            const width = image.naturalWidth, height = image.naturalHeight;
            fmdImportDimensions(width, height);
            // JPEG EXIF orientation may exchange the two axes.
            if (info.mime !== 'image/svg+xml' && !(width === info.width && height === info.height) && !(width === info.height && height === info.width)) {
              throw Error('Decoded image dimensions do not match its header');
            }
            settle();
          } catch (error) { settle(error); }
        }, settle);
      } catch (error) { settle(error); }
    });
  }

  async function importFiles(files, before) {
    if (pending) { status.textContent = 'An image import is already in progress'; return; }
    if (!files.length) return;
    if (!editable() || before.retired || revision !== before.revision || editor.value !== before.source) {
      status.textContent = 'Image import refused: the document changed or is not editable; select the files again';
      refresh(); return;
    }
    const operation = {stop: null, installing: false};
    pending = operation; refresh();
    status.textContent = 'Reading selected images…';
    let transaction = null, committed = false;
    try {
      if (before.source.length > FMD_IMAGE_IMPORT_LIMITS.sourceUnits) throw Error('Document exceeds the 32 Mi-character import limit');
      const current = () => pending === operation && editable() && revision === before.revision && editor.value === before.source;
      const candidate = Object.getOwnPropertyDescriptor(window, '__fmdNativeRuntime')?.value;
      const native = candidate?.version === 1 && typeof candidate.render === 'function'
        && typeof candidate.pdf === 'function' ? candidate : null;
      if (native && typeof native.stageImages !== 'function') {
        throw Error('Rebuild the matching workspace runtime to import native image resources');
      }
      const images = await fmdReadImageImport(files, (uri, info) => verify(uri, info, operation), current,
        file => readFile(file, operation));
      // The helper's last check and this continuation are separate microtasks.
      // A cancellation between them must not even stage native resources.
      if (!current()) throw Error('The document changed; select the images again');
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
      operation.installing = true;
      if (transaction) { transaction.commit(); committed = true; }
      if (!current()) throw Error('The document changed; select the images again');
      // On browsers supporting insertText, one insertion keeps native undo.
      // setRangeText is the non-undo-preserving fallback, never a full rewrite.
      if (typeof document.execCommand === 'function') {
        try { document.execCommand('insertText', false, insertion); }
        catch (error) { if (editor.value !== before.source) throw error; }
      }
      if (editor.value === before.source) editor.setRangeText(insertion, before.start, before.end, 'end');
      if (editor.value !== expected) throw Error('The editor changed during insertion; inspect the document before retrying');
      if (revision === before.revision) editor.dispatchEvent(new Event('input', {bubbles: true}));
      if (pending === operation) status.textContent = 'Inserted ' + files.length + (files.length === 1 ? ' image' : ' images')
        + (native ? ' — Save HTML to keep image resources; Markdown alone contains references' : ' — download to keep changes');
    } catch (error) {
      let rollbackError = '';
      if (committed && editor.value === before.source) {
        try { transaction.rollback(); }
        catch (failure) { rollbackError = '; resource rollback failed: ' + String(failure?.message || failure); }
      }
      if (pending === operation) status.textContent = 'Image insertion failed: ' + String(error?.message || error) + rollbackError;
    } finally {
      if (pending === operation) { pending = null; refresh(); }
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
  refresh();
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
  const fileLabel = make('label', 'Replacement file (PNG, JPEG, GIF or SVG, at most 8 MiB)', dialog);
  const file = make('input', undefined, fileLabel); file.type = 'file'; file.accept = 'image/png,image/jpeg,image/gif,image/svg+xml,.png,.jpg,.jpeg,.gif,.svg';
  file.id = 'fmd-image-replacement'; file.style.cssText = 'display:block;margin:8px 0;max-width:100%';
  make('p', 'Removal does not rewrite source references or infer which images are unused. A remaining reference may produce a missing-image diagnostic. Save HTML before removing resources you may need later; source undo does not restore them.', dialog);
  const actions = make('div', undefined, dialog); actions.style.cssText = 'display:flex;flex-wrap:wrap;gap:12px;margin:12px 0';
  const button = (id, text) => {
    const node = make('button', text, actions); node.id = id; node.type = 'button'; node.className = 'fmd-btn'; return node;
  };
  const replace = button('btn-replace-document-image', 'Replace selected image');
  const remove = button('btn-remove-document-image', 'Remove selected image');
  // Binding is an explicit opt-in API: older workspace runtimes retain their
  // existing replace/remove controls without offering unsupported attachment.
  let binding = null;
  if (typeof native.beginImageBindings === 'function') {
    const group = make('fieldset', undefined, dialog);
    make('legend', 'Attach an image to existing Markdown', group);
    make('p', 'Copy the image destination from your Markdown, then choose its local image. All references to that destination will use the attached file. Source stays unchanged; nothing is fetched or opened from that destination.', group);
    const keyLabel = make('label', 'Markdown image destination', group);
    const key = make('input', undefined, keyLabel);
    key.type = 'text'; key.id = 'fmd-image-binding-destination'; key.placeholder = 'images/diagram.png';
    key.autocomplete = 'off'; key.spellcheck = false;
    key.style.cssText = 'display:block;width:100%;box-sizing:border-box;font:inherit;margin:8px 0';
    const sourceLabel = make('label', 'Local image (PNG, JPEG, GIF or SVG, at most 8 MiB)', group);
    const source = make('input', undefined, sourceLabel);
    source.id = 'fmd-image-binding-file'; source.type = 'file'; source.accept = file.accept;
    source.style.cssText = 'display:block;margin:8px 0;max-width:100%';
    const attach = make('button', 'Attach image', group);
    attach.id = 'btn-attach-document-image'; attach.type = 'button'; attach.className = 'fmd-btn';
    binding = {key, file: source, button: attach};
  }
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
  const active = () => !suspended && !composing && !editor.disabled && !editor.readOnly && editor.isConnected !== false;
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
    destination.textContent = selected ? selected.destination : binding
      ? 'No embedded image selected. Attach a local file below to an existing Markdown destination, or use Insert image to add new source.'
      : 'No embedded image selected. Use Insert image to add one.';
    toggle.disabled = suspended || !entries;
    select.disabled = file.disabled = !active() || pending !== null;
    replace.disabled = !active() || pending !== null || !selected || file.files?.length !== 1;
    remove.disabled = !active() || pending !== null || !selected;
    if (binding) {
      // Keep the composing text field editable; disabling it would interrupt
      // the user's IME. Only publication/file actions pause for composition.
      binding.key.disabled = suspended || editor.disabled || editor.readOnly || editor.isConnected === false || pending !== null;
      binding.file.disabled = !active() || pending !== null;
      binding.button.disabled = !active() || pending !== null || !entries
        || !binding.key.value.trim() || binding.file.files?.length !== 1;
    }
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
    && native.documentRevision === operation.resources
    && (operation.mode === 'attach' ? binding.key.value === operation.keyDraft : select.value === operation.destination)
    && JSON.stringify(display()) === operation.display && operation.input.files?.[0] === operation.file;
  function bindingDestination() {
    const key = binding.key.value.trim();
    if (!key || /[\u0000-\u001f\u007f-\u009f]/u.test(key)) throw Error('Enter a nonempty Markdown image destination without control characters');
    let size = 0;
    for (const character of key) {
      const cp = character.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) throw Error('Image destination contains invalid Unicode');
      size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      if (size > 8192) throw Error('Image destination exceeds 8 KiB');
    }
    if (native.imageAssets?.some(entry => entry.destination === key)) {
      throw Error('That destination is already embedded; select it above and use Replace selected image');
    }
    return key;
  }
  function readReplacement(operation) {
    const selected = operation.file;
    if (!selected || !Number.isSafeInteger(selected.size) || selected.size < 1 || selected.size > FMD_IMAGE_IMPORT_LIMITS.fileBytes) {
      throw Error('Choose one local image containing 1 byte through 8 MiB');
    }
    return new Promise((resolve, reject) => {
      const reader = new win.FileReader(), image = new win.Image(); let done = false, timer;
      const check = () => { if (!current(operation)) throw Error('Document or image selection changed; choose the image again'); };
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
          if (info.mime !== 'image/svg+xml' && !(width === info.width && height === info.height) && !(width === info.height && height === info.width)) {
            throw Error('Decoded image dimensions do not match its header');
          }
          settle(null, bytes);
        } catch (error) { settle(error); }
      };
      timer = win.setTimeout(() => settle(Error('Image reading or decoding exceeded ten seconds; retry explicitly')), 10000);
      try { check(); reader.readAsArrayBuffer(selected); } catch (error) { settle(error); }
    });
  }
  async function apply(mode) {
    if (pending || !active() || !dialog.open) return;
    let operation = null;
    try {
      if (native.settingsPending || native.exportPending) throw Error('Finish or cancel the current document operation first');
      const removal = mode === 'remove', attaching = mode === 'attach';
      const entry = attaching ? null : native.imageAssets?.find(entry => entry.destination === select.value);
      if (!attaching && !entry) throw Error('Select an existing embedded image');
      const key = attaching ? bindingDestination() : entry.destination;
      const input = attaching ? binding.file : file;
      if (!removal && input.files?.length !== 1) throw Error('Choose exactly one local image file');
      operation = {mode, destination: key, keyDraft: attaching ? binding.key.value : null,
        input, file: input.files?.[0], source: editor.value,
        revision, resources: native.documentRevision, display: JSON.stringify(display()), handle: null, cancelRead: null};
      pending = operation; refresh();
      if (removal) {
        const accepted = win.confirm('Remove the embedded image “' + entry.destination + '”?\n\nMarkdown references and the original local file stay unchanged. Source undo cannot restore this resource. Save HTML first to retain a backup.');
        if (!current(operation)) throw Error('Document changed during confirmation; select the image again');
        if (!accepted) { status.textContent = 'Removal declined; image is unchanged.'; return; }
      }
      const patch = {destination: key};
      if (removal) patch.remove = true;
      else {
        status.textContent = 'Reading and validating the local image…';
        patch.bytes = await readReplacement(operation);
      }
      if (!current(operation)) throw Error('Document or image selection changed; try again');
      status.textContent = 'Validating document images in the native background worker…';
      operation.handle = attaching
        ? native.beginImageBindings([patch], operation.source, preview, JSON.parse(operation.display), () => current(operation))
        : native.beginImageChanges([patch], operation.source, preview, JSON.parse(operation.display), () => current(operation));
      await operation.handle.promise;
      // Committed resources now have a new revision; do not reject our own edit.
      if (pending !== operation) return;
      input.value = '';
      if (attaching) { binding.key.value = ''; select.value = key; }
      status.textContent = native.documentRevision === operation.resources ? 'Replacement already matches; nothing changed.'
        : (attaching ? 'Image attached to the Markdown destination.' : removal ? 'Image removed.' : 'Image replaced at the same Markdown destination.')
          + ' Source is unchanged; Save HTML to keep resources.';
    } catch (error) {
      if (!operation || pending === operation) status.textContent = 'Image not changed: ' + String(error?.message ?? error).slice(0, 2048);
    } finally {
      if (pending === operation) pending = null;
      refresh();
    }
  }
  function clearFiles() {
    file.value = '';
    if (binding) { binding.file.value = ''; binding.key.value = ''; }
  }
  toggle.addEventListener('click', () => { if (!toggle.disabled && !dialog.open) { clearFiles(); refresh(); dialog.showModal(); select.focus(); } });
  replace.addEventListener('click', () => { apply('replace'); });
  remove.addEventListener('click', () => { apply('remove'); });
  if (binding) {
    binding.button.addEventListener('click', () => { apply('attach'); });
    binding.key.addEventListener('input', () => changed('Image destination changed; attach the current selection again.'));
    binding.key.addEventListener('change', () => changed('Image destination changed; attach the current selection again.'));
    binding.file.addEventListener('change', () => changed('Local image selection changed; attach it again.'));
  }
  cancel.addEventListener('click', () => cancelPending());
  close.addEventListener('click', () => { cancelPending(); clearFiles(); dialog.close(); toggle.focus(); });
  dialog.addEventListener('cancel', () => { cancelPending(); clearFiles(); });
  dialog.addEventListener('close', () => { if (!dialog.open) { cancelPending(); clearFiles(); } });
  select.addEventListener('change', () => { file.value = ''; changed('Image selection changed; retry with the selected resource.'); });
  file.addEventListener('change', () => changed('Replacement selection changed; apply it again.'));
  editor.addEventListener('input', () => changed('Source changed; apply the image change again.'));
  for (const node of [editor, dialog]) {
    node.addEventListener('compositionstart', () => { composing = true; changed('Text composition started; image change cancelled.'); });
    node.addEventListener('compositionend', () => { composing = false; changed(); });
  }
  win.addEventListener('pagehide', () => { suspended = true; composing = false; changed('Workspace suspended; image change cancelled.'); clearFiles(); });
  win.addEventListener('pageshow', () => { suspended = false; composing = false; refresh(); });
  win.addEventListener('fmd-native-images-changed', () => {
    modified = true;
    const notice = doc.querySelector('#editor-pane > .fmd-pane-header > #fmd-save-status');
    if (notice) notice.textContent = 'Images modified — Save HTML to keep embedded resources';
    refresh();
  });
  win.addEventListener('beforeunload', event => { if (modified) { event.preventDefault(); event.returnValue = ''; } });
  const viewObserver = new win.MutationObserver(() => {
    // An access/view change followed by a revert still retires an operation.
    changed('Source, view or document state changed; retry the image change.');
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
