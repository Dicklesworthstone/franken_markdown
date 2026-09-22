import { init, renderInteractiveHtml } from './franken_markdown.js';
import { bootNativeWorkspace, createNativeWorkspaceRenderer } from './interactive_runtime.mjs';

const MiB = 1024 * 1024;
const slots = ['body-regular', 'body-bold', 'body-italic', 'body-bold-italic', 'mono-regular'];
const fields = ['font', 'darkMode', 'title', 'lang', 'fontScale', 'toc', 'tocDepth',
  'author', 'metadataEpochSeconds', 'pageNumbers', 'codeLineNumbers', 'pdfImages', 'fontAssets'];

// Count without allocating an encoded copy. Reject unpaired surrogates rather
// than silently changing the Markdown, asset keys, or trusted binding source.
function textBytes(text, maximum, label) {
  if (typeof text !== 'string' || text.length > maximum) throw new RangeError(`${label} exceeds its UTF-8 limit`);
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const low = text.charCodeAt(++i);
      if (!(low >= 0xdc00 && low <= 0xdfff)) throw new TypeError(`${label} contains an unpaired surrogate`);
      bytes += 4;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw new TypeError(`${label} contains an unpaired surrogate`);
    } else bytes += unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3;
    if (bytes > maximum) throw new RangeError(`${label} exceeds its UTF-8 limit`);
  }
  return bytes;
}
function view(value, maximum, label) {
  const isView = ArrayBuffer.isView(value);
  const buffer = isView ? value.buffer : value;
  // Intrinsic branding rejects shared memory and spoofed toStringTag values.
  const size = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
  const bytes = new Uint8Array(buffer, isView ? value.byteOffset : 0, isView ? value.byteLength : size);
  if (!bytes.length || bytes.length > maximum) throw new RangeError(`${label} has an invalid byte length`);
  return bytes;
}
function base64(bytes) {
  let out = '';
  // Chunk length must be divisible by three: concatenation then equals one
  // canonical base64 encoding, without spread/argument-count or quadratic joins.
  for (let start = 0; start < bytes.length; start += 24576) {
    const part = bytes.subarray(start, start + 24576);
    let binary = '';
    for (const byte of part) binary += String.fromCharCode(byte);
    out += btoa(binary);
  }
  return out;
}
function capture(markdown, runtime, input) {
  if (!runtime || typeof runtime !== 'object' || Array.isArray(runtime)
      || !input || typeof input !== 'object' || Array.isArray(input)) {
    throw new TypeError('Runtime and workspace options must be objects');
  }
  for (const key of Reflect.ownKeys(input)) {
    if (!fields.includes(key)) throw new TypeError(`Unsupported offline workspace option: ${String(key)}`);
  }
  const source = String(markdown);
  const sourceLength = textBytes(source, 32 * MiB, 'source');
  const { bindings, wasm } = runtime;
  const moduleLength = textBytes(bindings, 4 * MiB, 'bindings');
  if (!moduleLength) throw new TypeError('bindings must contain trusted wasm-bindgen --target web JavaScript');
  const wasmView = view(wasm, 64 * MiB, 'WASM');
  if (wasmView.length < 8 || ![0, 97, 115, 109, 1, 0, 0, 0].every((byte, i) => wasmView[i] === byte)) {
    throw new TypeError('WASM must contain a version-1 WebAssembly module');
  }
  const captured = Object.fromEntries(fields.map(key => [key, input[key]]));
  const options = {
    font: captured.font ?? 'sans', darkMode: captured.darkMode ?? 'auto',
    title: captured.title, lang: captured.lang, fontScale: captured.fontScale ?? 1,
    toc: captured.toc ?? false, tocDepth: captured.tocDepth,
    author: captured.author, metadataEpochSeconds: captured.metadataEpochSeconds,
    pageNumbers: captured.pageNumbers ?? false, codeLineNumbers: captured.codeLineNumbers ?? false,
  };
  if (!['sans', 'serif'].includes(options.font)) throw new TypeError('font must be sans or serif');
  if (!['auto', 'disabled'].includes(options.darkMode)) throw new TypeError('darkMode must be auto or disabled');
  if (typeof options.fontScale !== 'number' || !Number.isFinite(options.fontScale)
      || options.fontScale < 0.5 || options.fontScale > 3) throw new RangeError('fontScale must be 0.5..3');
  for (const field of ['toc', 'pageNumbers', 'codeLineNumbers']) {
    if (typeof options[field] !== 'boolean') throw new TypeError(`${field} must be boolean`);
  }
  if (options.tocDepth !== undefined && (!Number.isInteger(options.tocDepth) || options.tocDepth < 1 || options.tocDepth > 6)) {
    throw new RangeError('tocDepth must be 1..6');
  }
  if (options.metadataEpochSeconds !== undefined && (!Number.isSafeInteger(options.metadataEpochSeconds)
      || options.metadataEpochSeconds < 0)) throw new RangeError('metadataEpochSeconds must be a non-negative safe integer');
  for (const key of ['title', 'author', 'lang']) {
    if (options[key] !== undefined) textBytes(options[key], key === 'lang' ? 128 : 4096, key);
  }
  const images = captured.pdfImages ?? [], fonts = captured.fontAssets ?? [];
  // Count admission precedes entry getters, payload copies, and initialization.
  if (!Array.isArray(images) || images.length > 4096 || !Array.isArray(fonts) || fonts.length > 5) {
    throw new RangeError('A workspace supports at most 4096 images and five font slots');
  }
  const imageViews = [], fontViews = [], seenImages = new Set(), seenFonts = new Set();
  let names = 0, total = moduleLength + wasmView.length;
  for (const image of images) {
    if (!image || typeof image !== 'object') throw new TypeError('Each image must be an object');
    const {destination: rawDestination, bytes} = image;
    if (typeof rawDestination !== 'string') throw new TypeError('Image destinations must be strings');
    const destination = rawDestination.trim();
    names += textBytes(destination, 8192, 'image destination');
    if (!destination || /[\u0000-\u001f\u007f-\u009f]/u.test(destination) || seenImages.has(destination)) {
      throw new TypeError('Image destinations must be nonempty, control-free and unique after trimming');
    }
    if (names > 65536) throw new RangeError('Image destinations exceed 64 KiB');
    seenImages.add(destination);
    const data = view(bytes, 32 * MiB, 'image'); total += data.length;
    imageViews.push({destination, data});
  }
  for (const font of fonts) {
    if (!font || typeof font !== 'object') throw new TypeError('Each font must be an object');
    const {slot, bytes, weight} = font;
    if (!slots.includes(slot) || seenFonts.has(slot)) throw new TypeError('Font slots must be supported and unique');
    if (weight !== undefined && (!Number.isInteger(weight) || weight < 1 || weight > 1000)) {
      throw new RangeError('Font weight must be 1..1000');
    }
    seenFonts.add(slot);
    const data = view(bytes, 32 * MiB, 'font'); total += data.length;
    fontViews.push({slot, weight, data});
  }
  if (total > 128 * MiB) throw new RangeError('Runtime and assets exceed 128 MiB combined');
  // No await above: strings, primitives, and exact owned byte representations
  // are all fixed before the existing WASM package begins async initialization.
  const payload = {version: 1, bindings, wasm: base64(wasmView), options,
    images: imageViews.map(({destination, data}) => ({destination, bytes: base64(data)})),
    fonts: fontViews.map(({slot, weight, data}) => ({slot, weight, bytes: base64(data)})),
  };
  return {source, sourceLength, payload, wasmBytes: new Uint8Array(wasmView)};
}

/**
 * Build a zero-network HTML workspace containing trusted generated bindings,
 * their matching WASM bytes, and all supplied image/font assets. The existing
 * lightweight renderInteractiveHtml API remains unchanged.
 */
export async function renderOfflineWorkspace(markdown, runtime, options = {}) {
  const prepared = capture(markdown, runtime, options);
  await init(prepared.wasmBytes);
  const shell = await renderInteractiveHtml(prepared.source, prepared.payload.options);
  const html = shell.text();
  const appStart = html.lastIndexOf('<script>\n');
  const footer = '\n</script>\n</body>\n</html>\n';
  if (appStart < 0 || !html.endsWith(footer)
      || !html.slice(appStart).includes("window.addEventListener('fmd-native-ready'")) {
    throw Object.assign(new Error('Rebuild the matching WASM package: its interactive controller lacks native workspace support'),
      {code: 'UNSUPPORTED_WASM_PACKAGE'});
  }
  const json = JSON.stringify(prepared.payload).replace(/</g, '\\u003c')
    .replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
  const bootstrap = `(${bootNativeWorkspace.toString()})(${createNativeWorkspaceRenderer.toString()});`;
  if (/<\/script/i.test(bootstrap)) throw new Error('Unsafe native workspace bootstrap serialization');
  const extra = '<script type="application/json" id="fmd-native-runtime">' + json + '</script>\n'
    + '<script>\n' + bootstrap + '\n</script>\n';
  // Permit the bundled app/module and WASM compilation, not JavaScript eval,
  // network requests, remote fonts/images, object plugins or form submissions.
  const policy = "default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; frame-src 'self' about:; base-uri 'none'; form-action 'none'";
  let document = html.slice(0, appStart) + extra + html.slice(appStart);
  if (!document.includes('<head>')) throw new Error('Interactive HTML is missing its document head');
  document = document.replace('<head>', '<head>\n<meta http-equiv="Content-Security-Policy" content="' + policy + '">');
  const bytes = new TextEncoder().encode(document);
  if (bytes.length > 256 * MiB) throw new RangeError('Generated workspace exceeds 256 MiB');
  const output = {...shell, bytes, sourceLength: prepared.sourceLength,
    text() { return new TextDecoder().decode(bytes); },
    blob() { return new Blob([bytes], {type: shell.mimeType}); },
  };
  return Object.freeze(output);
}
