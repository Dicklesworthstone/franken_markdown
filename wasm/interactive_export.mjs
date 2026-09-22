// Packaging is independent of the generated package URL. Hosts explicitly supply
// one trusted, matching wasm-bindgen --target web JavaScript/WASM pair.
import {bootNativeWorkspace, createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {pdfPageGeometry} from './pdf_page.mjs';

const MiB = 1024 * 1024;
const SLOTS = ['body-regular', 'body-bold', 'body-italic', 'body-bold-italic', 'mono-regular'];
export const NATIVE_WORKSPACE_LIMITS = Object.freeze({
  sourceBytes: 32 * MiB, wasmBytes: 64 * MiB, bindingsBytes: 4 * MiB,
  assetBytes: 32 * MiB, totalAssetBytes: 128 * MiB, imageCount: 4096,
  destinationBytes: 8192, totalDestinationBytes: 65536, outputBytes: 256 * MiB,
});

function record(value, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new TypeError(`${label} must be an object`);
  }
  return value;
}

// Count UTF-8 without allocating a second copy. Reject lone surrogates rather
// than silently changing source or resource identity through TextEncoder.
function textSize(text, maximum, label) {
  if (typeof text !== 'string') throw new TypeError(`${label} must be a string`);
  if (text.length > maximum) throw new RangeError(`${label} exceeds its byte limit`);
  let size = 0;
  for (const character of text) {
    const cp = character.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError(`${label} contains an unpaired surrogate`);
    size += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (size > maximum) throw new RangeError(`${label} exceeds its byte limit`);
  }
  return size;
}

function byteView(value, maximum, label) {
  const isView = ArrayBuffer.isView(value);
  const buffer = isView ? value.buffer : value;
  // Use the intrinsic brand: Symbol.toStringTag must not turn a SharedArrayBuffer
  // (or arbitrary object) into an apparently private ArrayBuffer.
  let size;
  try { size = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer); }
  catch { throw new TypeError(`${label} must use a non-shared ArrayBuffer`); }
  // Construction checks detachment and the exact selected range (DataView too).
  const bytes = new Uint8Array(buffer, isView ? value.byteOffset : 0, isView ? value.byteLength : size);
  if (!bytes.length || bytes.length > maximum) throw new RangeError(`${label} exceeds its byte limit or is empty`);
  return bytes;
}

function base64(bytes) {
  const chunks = [];
  for (let start = 0; start < bytes.length; start += 16384) {
    chunks.push(String.fromCharCode(...bytes.subarray(start, start + 16384)));
  }
  return btoa(chunks.join(''));
}

function jsonData(value) {
  return JSON.stringify(value).replace(/</g, '\\u003c')
    .replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
}

function attribute(text) {
  return text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
}

function captureRuntime(input) {
  const {wasm, bindings} = record(input, 'runtime');
  const view = byteView(wasm, NATIVE_WORKSPACE_LIMITS.wasmBytes, 'runtime WASM');
  textSize(bindings, NATIVE_WORKSPACE_LIMITS.bindingsBytes, 'runtime bindings');
  if (!bindings.trim()) throw new TypeError('runtime bindings must not be empty');
  if (view.length < 8 || ![0, 97, 115, 109, 1, 0, 0, 0].every((b, i) => view[i] === b)) {
    throw new TypeError('runtime WASM must be a WebAssembly version-1 binary');
  }
  return {wasm: view.slice(), bindings};
}

function captureOptions(input) {
  const allowed = new Set(['font', 'darkMode', 'fontScale', 'title', 'author', 'lang',
    'metadataEpochSeconds', 'pageNumbers', 'codeLineNumbers', 'toc', 'tocDepth',
    'pdfImages', 'fontAssets', 'allowRawHtml', 'page']);
  record(input, 'workspace options');
  for (const key of Object.keys(input)) if (!allowed.has(key)) {
    throw new TypeError(`Unsupported native workspace option: ${key}`);
  }
  const {font = 'sans', darkMode = 'auto', fontScale = 1, title, author, lang,
    metadataEpochSeconds, pageNumbers = false, codeLineNumbers = false,
    toc = false, tocDepth, pdfImages = [], fontAssets = [], allowRawHtml = false, page} = input;
  const geometry = pdfPageGeometry(page);
  if (!['sans', 'serif'].includes(font)) throw new TypeError('font must be sans or serif');
  if (!['auto', 'disabled'].includes(darkMode)) throw new TypeError('darkMode must be auto or disabled');
  if (!Number.isFinite(fontScale) || fontScale < 0.5 || fontScale > 3) throw new RangeError('fontScale must be from 0.5 through 3');
  if (allowRawHtml !== false) throw new TypeError('Native workspaces require safe Markdown; allowRawHtml must be false');
  for (const [value, name] of [[pageNumbers, 'pageNumbers'], [codeLineNumbers, 'codeLineNumbers'], [toc, 'toc']]) {
    if (typeof value !== 'boolean') throw new TypeError(`${name} must be a boolean`);
  }
  for (const [value, name, maximum] of [[title, 'title', 65536], [author, 'author', 65536], [lang, 'lang', 1024]]) {
    if (value !== undefined) textSize(value, maximum, name);
  }
  if (metadataEpochSeconds !== undefined && (!Number.isSafeInteger(metadataEpochSeconds) || metadataEpochSeconds < 0)) {
    throw new RangeError('metadataEpochSeconds must be a nonnegative safe integer');
  }
  if (tocDepth !== undefined && (!Number.isInteger(tocDepth) || tocDepth < 1 || tocDepth > 6)) throw new RangeError('tocDepth must be 1..6');
  // Count before looking at elements, allocating mapped arrays, or reading bytes.
  if (!Array.isArray(pdfImages) || pdfImages.length > NATIVE_WORKSPACE_LIMITS.imageCount) throw new RangeError('pdfImages must contain at most 4096 entries');
  if (!Array.isArray(fontAssets) || fontAssets.length > SLOTS.length) throw new RangeError('fontAssets must contain at most five entries');
  const keys = new Set(), slots = new Set(), images = [], fonts = [];
  let total = 0, names = 0;
  for (const item of pdfImages) {
    const {destination, bytes} = record(item, 'image asset');
    textSize(destination, NATIVE_WORKSPACE_LIMITS.destinationBytes, 'image destination');
    const key = destination.trim();
    if (!key || /[\u0000-\u001f\u007f-\u009f]/u.test(key) || keys.has(key)) throw new TypeError('Image destinations must be nonempty, unique after trimming, and control-free');
    names += textSize(key, NATIVE_WORKSPACE_LIMITS.destinationBytes, 'image destination');
    if (names > NATIVE_WORKSPACE_LIMITS.totalDestinationBytes) throw new RangeError('Image destinations exceed 64 KiB');
    keys.add(key);
    const view = byteView(bytes, NATIVE_WORKSPACE_LIMITS.assetBytes, 'image asset');
    total += view.byteLength;
    if (total > NATIVE_WORKSPACE_LIMITS.totalAssetBytes) throw new RangeError('Workspace assets exceed 128 MiB');
    images.push({destination: key, view});
  }
  for (const item of fontAssets) {
    const {slot, bytes, weight} = record(item, 'font asset');
    if (!SLOTS.includes(slot) || slots.has(slot)) throw new TypeError('Font slots must be recognized and unique');
    if (weight !== undefined && (!Number.isInteger(weight) || weight < 1 || weight > 1000)) throw new RangeError('Font weight must be an integer from 1 through 1000');
    slots.add(slot);
    const view = byteView(bytes, NATIVE_WORKSPACE_LIMITS.assetBytes, 'font asset');
    total += view.byteLength;
    if (total > NATIVE_WORKSPACE_LIMITS.totalAssetBytes) throw new RangeError('Workspace assets exceed 128 MiB');
    fonts.push({slot, weight, view});
  }
  return {
    options: {font, darkMode, fontScale, title, author, lang, metadataEpochSeconds,
      pageNumbers, codeLineNumbers, toc, tocDepth,
      ...(geometry.length ? {pageGeometry: Array.from(geometry)} : {})},
    // All admission has finished. Serialize exact views now, never caller buffers
    // or remote URLs. Stable font-slot order avoids incidental input-order drift.
    images: images.map(({destination, view}) => ({destination, bytes: base64(view)})),
    fonts: fonts.sort((a, b) => SLOTS.indexOf(a.slot) - SLOTS.indexOf(b.slot))
      .map(({slot, weight, view}) => ({slot, weight, bytes: base64(view)})),
  };
}

function takeShell(result) {
  try {
    const bytes = result?.bytes;
    if (result?.mimeType?.split(';')[0] !== 'text/html' || !(bytes instanceof Uint8Array)) throw new TypeError('Native workspace shell has an invalid output envelope');
    if (!bytes.length || bytes.length > NATIVE_WORKSPACE_LIMITS.outputBytes) throw new RangeError('Native workspace shell exceeds its output limit');
    const json = result.diagnosticsJson();
    const diagnostics = json === '' ? [] : JSON.parse(json);
    if (!Array.isArray(diagnostics)) throw new TypeError('Native workspace shell has invalid diagnostics');
    return {html: new TextDecoder('utf-8', {fatal: true}).decode(bytes), diagnostics};
  } finally {
    if (typeof result?.free === 'function') result.free();
  }
}

function assemble(shell, preview, payload, source) {
  const sourceOpen = '<script type="application/json" id="fmd-raw-source">';
  const sourceStart = shell.indexOf(sourceOpen);
  const sourceEnd = shell.indexOf('</script>', sourceStart + sourceOpen.length);
  const appStart = '<div class="fmd-content" id="fmd-content">';
  const appEnd = '</div>\n  </main>\n</div>\n';
  const previewStart = shell.indexOf(appStart), previewEnd = shell.lastIndexOf(appEnd, sourceStart);
  const scriptStart = shell.indexOf('<script>\n', sourceEnd + 9);
  const scriptEnd = shell.indexOf('</script>', scriptStart + 9);
  if (sourceStart < 0 || sourceEnd < 0 || previewStart < 0 || previewEnd < previewStart
      || scriptStart < sourceEnd || scriptEnd < scriptStart
      || !shell.slice(scriptStart, scriptEnd).includes('__fmdNativeRuntime')
      || JSON.parse(shell.slice(sourceStart + sourceOpen.length, sourceEnd)) !== source) {
    throw Object.assign(new Error('Native workspace requires a rebuilt matching WASM package with the native editor controller'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  }
  // Replace, do not append to, the initial unsandboxed fragment. Even before
  // initialization the complete native preview gets the iframe resource policy.
  const frame = '<iframe title="Rust/WASM Markdown preview" sandbox="allow-same-origin" referrerpolicy="no-referrer" '
    + 'style="display:block;width:100%;border:0;min-height:320px;height:320px" srcdoc="' + attribute(preview) + '"></iframe>';
  const bootstrap = '<script type="application/json" id="fmd-native-runtime">' + jsonData(payload) + '</script>\n'
    + '<script id="fmd-native-bootstrap">\n;(' + bootNativeWorkspace.toString() + ')('
    + createNativeWorkspaceRenderer.toString() + ');\n</script>\n';
  // These offsets refer to the original shell. Assembly never searches the
  // newly injected source, binary, binding text or rendered document for tags.
  const output = shell.slice(0, previewStart + appStart.length) + frame
    + shell.slice(previewEnd, scriptStart) + bootstrap + shell.slice(scriptStart);
  const head = /<head(?:\s[^>]*)?>/i;
  if (!head.test(output)) throw new Error('Native workspace shell is missing its document head');
  // Allow only the embedded application, Blob module and WASM compilation.
  // The separate iframe policy remains stricter: no script execution at all.
  const policy = "default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; frame-src 'self' about:; base-uri 'none'; form-action 'none'";
  return output.replace(head, match => match + '\n<meta http-equiv="Content-Security-Policy" content="' + policy + '">');
}

function filename(value = 'document') {
  let name = Array.from(String(value)).slice(0, 80).join('')
    .replace(/[<>:"/\\|?*\u0000-\u001f\u007f]/g, '_').trim().replace(/[. ]+$/, '');
  if (!name || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name)) name = 'document';
  return name + '.html';
}

// Internal injection seam: tests exercise packaging with explicit ABI adapters,
// rather than pretending an unavailable Rust build was executed.
export async function createWorkspaceExporterWithLoader(runtime, load) {
  const owned = captureRuntime(runtime); // Before the first asynchronous boundary.
  const encodedWasm = base64(owned.wasm);
  const bindingSource = owned.bindings;
  const engine = await load(owned);
  for (const key of ['renderInteractiveHtmlConfigured', 'renderHtmlConfiguredAdvanced', 'renderPdfConfiguredMulti']) {
    if (typeof engine?.[key] !== 'function') throw Object.assign(new Error(`Matching WASM bindings required: ${key}`), {code: 'UNSUPPORTED_WASM_PACKAGE'});
  }
  return Object.freeze({
    render(markdown, options = {}) {
      const sourceLength = textSize(markdown, NATIVE_WORKSPACE_LIMITS.sourceBytes, 'workspace source');
      const prepared = captureOptions(options);
      const payload = {version: 1, wasm: encodedWasm, bindings: bindingSource, ...prepared};
      const opts = prepared.options;
      const renderer = createNativeWorkspaceRenderer(engine, payload);
      const shell = takeShell(engine.renderInteractiveHtmlConfigured(markdown,
        opts.font, opts.darkMode, opts.title, opts.lang, opts.fontScale));
      const preview = renderer.html(markdown);
      const html = assemble(shell.html, preview, payload, markdown);
      textSize(html, NATIVE_WORKSPACE_LIMITS.outputBytes, 'native workspace output');
      const bytes = new TextEncoder().encode(html);
      const diagnostics = [...shell.diagnostics, ...renderer.diagnostics];
      return Object.freeze({format: 'interactive-html', mimeType: 'text/html;charset=utf-8',
        extension: 'html', sourceLength, bytes, diagnostics,
        text: () => new TextDecoder('utf-8', {fatal: true}).decode(bytes),
        blob: () => new Blob([bytes], {type: 'text/html;charset=utf-8'}), filename});
    },
  });
}

async function loadRuntime(owned) {
  let url, revoke = false;
  try {
    // Node cannot import blob: modules, but Blob URL identities are unique even
    // across multiple loaded copies of this exporter. A per-module counter is
    // insufficient: two package copies could otherwise reuse the same JS module
    // and silently change each other's initialized WASM instance.
    if (typeof process !== 'undefined' && process.versions?.node) {
      const identity = URL.createObjectURL(new Blob());
      URL.revokeObjectURL(identity);
      url = 'data:text/javascript;base64,' + base64(new TextEncoder().encode(owned.bindings)) + '#' + identity;
    } else {
      url = URL.createObjectURL(new Blob([owned.bindings], {type: 'text/javascript'}));
      revoke = true;
    }
    const bindings = await import(url);
    if (typeof bindings.default !== 'function') throw new TypeError('Runtime bindings must be wasm-bindgen --target web JavaScript');
    await bindings.default({module_or_path: owned.wasm});
    return bindings;
  } finally {
    if (revoke) URL.revokeObjectURL(url);
  }
}

/** Load one explicitly supplied trusted runtime, then reuse it to export files. */
export function createNativeWorkspaceExporter(runtime) {
  return createWorkspaceExporterWithLoader(runtime, loadRuntime);
}
