// These two functions are also serialized into standalone documents. Keep them
// self-contained: no imports, outer-scope helpers, or generated package URLs.
export function createNativeWorkspaceRenderer(bindings, payload) {
  const required = ['renderHtmlConfiguredAdvanced', 'renderPdfConfiguredMulti'];
  for (const name of required) {
    if (typeof bindings[name] !== 'function') {
      throw new Error(`Native workspace requires matching WASM bindings: ${name} is missing`);
    }
  }
  if (payload.version !== 1) throw new Error('Unsupported native workspace version');
  const settingKeys = ['font', 'darkMode', 'fontScale', 'title', 'author', 'lang',
    'metadataEpochSeconds', 'pageNumbers', 'codeLineNumbers', 'toc', 'tocDepth', 'pageGeometry'];
  function settingData(value) {
    // Inspect descriptors before reading values: accessors, inherited options,
    // symbols and unsupported switches must not enter a saved workspace.
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new TypeError('Settings must be a data object');
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== null && Object.getPrototypeOf(prototype) !== null) throw new TypeError('Settings must be a plain data object');
    const result = {};
    for (const key of Reflect.ownKeys(value)) {
      const field = Object.getOwnPropertyDescriptor(value, key);
      if (!settingKeys.includes(key) || !field || !Object.hasOwn(field, 'value')) {
        throw new TypeError('Unsupported settings field or accessor');
      }
      result[key] = field.value;
    }
    return result;
  }
  function settingsText(value, limit, name) {
    if (value === undefined) return;
    if (typeof value !== 'string' || value.length > limit) throw new TypeError('Invalid ' + name);
    let size = 0;
    for (const character of value) {
      const cp = character.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError('Invalid Unicode in ' + name);
      size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      if (size > limit) throw new RangeError(name + ' exceeds its UTF-8 byte limit');
    }
  }
  function captureSettings(input) {
    const value = settingData(input);
    if (!['sans', 'serif'].includes(value.font)) throw new TypeError('font must be sans or serif');
    if (!['auto', 'disabled'].includes(value.darkMode)) throw new TypeError('darkMode must be auto or disabled');
    if (!Number.isFinite(value.fontScale) || value.fontScale < 0.5 || value.fontScale > 3) throw new RangeError('fontScale must be 0.5..3');
    for (const key of ['pageNumbers', 'codeLineNumbers', 'toc']) {
      if (typeof value[key] !== 'boolean') throw new TypeError(key + ' must be boolean');
    }
    settingsText(value.title, 65536, 'title'); settingsText(value.author, 65536, 'author'); settingsText(value.lang, 1024, 'lang');
    if (value.metadataEpochSeconds !== undefined && (!Number.isSafeInteger(value.metadataEpochSeconds) || value.metadataEpochSeconds < 0)) {
      throw new RangeError('metadataEpochSeconds must be a nonnegative safe integer');
    }
    if (value.tocDepth !== undefined && (!Number.isInteger(value.tocDepth) || value.tocDepth < 1 || value.tocDepth > 6)) {
      throw new RangeError('tocDepth must be 1..6');
    }
    if (value.pageGeometry !== undefined) {
      const input = value.pageGeometry;
      // Copy data elements without invoking an iterator, getter or sparse-array
      // prototype lookup. The saved ABI order is width,height,top,right,bottom,left.
      if (!Array.isArray(input) || input.length !== 6 || Reflect.ownKeys(input).length !== 7) throw new RangeError('Invalid saved PDF page geometry');
      const g = [];
      for (let i = 0; i < 6; i++) {
        const field = Object.getOwnPropertyDescriptor(input, String(i));
        if (!field || !Object.hasOwn(field, 'value') || !Number.isFinite(field.value)) throw new RangeError('Invalid saved PDF page geometry');
        g.push(field.value === 0 ? 0 : field.value);
      }
      // Reproduce Rust's left-associative f32 subtractions, not just a final cast.
      const extent = (size, first, second) => Math.fround(Math.fround(Math.fround(size) - Math.fround(first)) - Math.fround(second));
      if (g[0] < 144 || g[1] < 144 || g.some(value => value < 0 || value > 14400)
          || g[0] - g[5] - g[3] < 72 || g[1] - g[2] - g[4] < 72
          || extent(g[0], g[5], g[3]) < 72 || extent(g[1], g[2], g[4]) < 72) {
        throw new RangeError('Invalid saved PDF page geometry');
      }
      if (typeof bindings.renderPdfConfiguredPage !== 'function') {
        throw Object.assign(new Error('PDF paper and margins require matching page-capable WASM bindings'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
      }
      value.pageGeometry = Object.freeze(g);
    }
    // Omitted optional fields remain omitted across JSON save/reopen cycles.
    return Object.freeze(Object.fromEntries(settingKeys.filter(key => value[key] !== undefined).map(key => [key, value[key]])));
  }
  let options = captureSettings({font: 'sans', darkMode: 'auto', fontScale: 1,
    pageNumbers: false, codeLineNumbers: false, toc: false, ...settingData(payload.options)});
  let pageGeometry = options.pageGeometry ? new Float64Array(options.pageGeometry) : null;
  const MiB = 1024 * 1024;
  function textBytes(text) {
    if (typeof text !== 'string' || text.length > 8192) throw new TypeError('Invalid image destination');
    let count = 0;
    for (const character of text) {
      const cp = character.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError('Invalid image destination Unicode');
      count += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
    }
    if (count > 8192) throw new RangeError('Image destination exceeds 8 KiB');
    return count;
  }
  function destination(value, seen) {
    if (typeof value !== 'string') throw new TypeError('Invalid image destination');
    const key = value.trim();
    const size = textBytes(key);
    if (!key || /[\u0000-\u001f\u007f-\u009f]/u.test(key) || seen.has(key)) {
      throw new TypeError('Image destinations must be nonempty, control-free and unique');
    }
    seen.add(key);
    return {key, size};
  }
  function encodedLength(text) {
    if (typeof text !== 'string' || !text.length || text.length % 4 || text.length > Math.ceil(32 * MiB / 3) * 4) {
      throw new RangeError('Invalid saved resource size');
    }
    const length = text.length / 4 * 3 - (text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0);
    if (length > 32 * MiB) throw new RangeError('Saved resource exceeds 32 MiB');
    return length;
  }
  function decode(text) {
    const binary = atob(text);
    if (binary.length !== encodedLength(text)) throw new TypeError('Invalid saved resource encoding');
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }
  function encode(bytes) {
    const chunks = [];
    // Each chunk has a multiple of three bytes, so base64 concatenates exactly.
    for (let start = 0; start < bytes.length; start += 24576) {
      chunks.push(btoa(String.fromCharCode(...bytes.subarray(start, start + 24576))));
    }
    return chunks.join('');
  }
  const slots = ['body-regular', 'body-bold', 'body-italic', 'body-bold-italic', 'mono-regular'];
  if (!Array.isArray(payload.images) || payload.images.length > 4096
      || !Array.isArray(payload.fonts) || payload.fonts.length > slots.length) {
    throw new RangeError('Workspace exceeds 4096 images or five font slots');
  }
  // Admit the saved resource totals before decoding or allocating packed ABI
  // buffers. Imported resources are subject to the same lifetime budget.
  const keys = new Set(), fontSlots = new Set();
  let nameBytes = 0, imageBytes = 0, fontTotal = 0;
  const encodedImages = payload.images.map(image => {
    const {key, size} = destination(image.destination, keys);
    nameBytes += size; imageBytes += encodedLength(image.bytes);
    return Object.freeze({destination: key, bytes: image.bytes});
  });
  for (const font of payload.fonts) {
    if (!slots.includes(font.slot) || fontSlots.has(font.slot)) throw new TypeError('Invalid or duplicate saved font slot');
    fontSlots.add(font.slot); fontTotal += encodedLength(font.bytes);
  }
  if (nameBytes > 65536 || imageBytes + fontTotal > 128 * MiB) throw new RangeError('Workspace resource budget exceeded');
  function pack(encoded, views, byteLength, names) {
    const flat = new Uint8Array(byteLength), lengths = Uint32Array.from(views, view => view.length);
    let offset = 0;
    for (const bytes of views) { flat.set(bytes, offset); offset += bytes.length; }
    return {encoded: Object.freeze(encoded), flat, lengths, names,
      destinations: encoded.map(image => image.destination)};
  }
  let images = pack(encodedImages, encodedImages.map(image => decode(image.bytes)), imageBytes, nameBytes);
  const fonts = slots.map(slot => payload.fonts.find(font => font.slot === slot));
  const fontBytes = fonts.map(font => font ? decode(font.bytes) : new Uint8Array());
  const weights = Uint32Array.from(fonts, font => font?.weight ?? 0);
  let generation = 0, publishing = false;
  function publishChange(publish, apply) {
    if (publishing) throw new Error('Workspace transaction publication is already in progress');
    publishing = true;
    try { if (publish) publish(); apply(); }
    finally { publishing = false; }
  }
  function stageSettings(patch) {
    const next = captureSettings({...options, ...settingData(patch)});
    const geometry = next.pageGeometry ? new Float64Array(next.pageGeometry) : null;
    const previous = options, previousGeometry = pageGeometry, revision = generation;
    let committed = null, ended = false, stagedDiagnostics = null, previousDiagnostics = null;
    const current = () => {
      if (ended || committed !== null || revision !== generation) throw new Error('Stale settings transaction');
    };
    return Object.freeze({
      settings: next,
      html(markdown, display) {
        current();
        const before = diagnostics;
        try {
          const html = renderHtml(markdown, display, next);
          stagedDiagnostics = diagnostics;
          return html;
        } finally { diagnostics = before; }
      },
      commit(publish) {
        current();
        publishChange(publish, () => {
          payload.options = next; options = next; pageGeometry = geometry;
          previousDiagnostics = diagnostics;
          if (stagedDiagnostics !== null) diagnostics = stagedDiagnostics;
          committed = ++generation;
        });
      },
      rollback(publish) {
        if (ended || committed === null || committed !== generation) throw new Error('Stale settings rollback');
        publishChange(publish, () => {
          payload.options = previous; options = previous; pageGeometry = previousGeometry;
          diagnostics = previousDiagnostics; generation++; ended = true;
        });
      },
    });
  }
  function stageImages(additions) {
    if (!Array.isArray(additions) || !additions.length || additions.length > 8
        || images.encoded.length + additions.length > 4096) {
      throw new RangeError('Import supports 1..8 images within the 4096-image workspace limit');
    }
    const seen = new Set(images.destinations), admitted = [];
    let batch = 0, names = images.names;
    for (const image of additions) {
      const {key, size} = destination(image.destination, seen);
      names += size;
      const value = image.bytes, isView = ArrayBuffer.isView(value);
      const buffer = isView ? value.buffer : value;
      // Intrinsic branding rejects shared memory and spoofed buffers. Respect
      // DataView, typed-array and pooled Buffer offsets, including detachment.
      const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
      const bytes = new Uint8Array(buffer, isView ? value.byteOffset : 0, isView ? value.byteLength : length);
      if (!bytes.length || bytes.length > 8 * MiB) throw new RangeError('Each imported image must contain 1 byte through 8 MiB');
      batch += bytes.length;
      if (names > 65536 || batch > 16 * MiB || images.flat.length + batch + fontTotal > 128 * MiB) {
        throw new RangeError('Imported images exceed the workspace resource budget');
      }
      admitted.push({destination: key, bytes});
    }
    const previous = images, revision = generation;
    // All admission passed. Copy once into the next ABI buffer; encoding from
    // its owned views prevents later caller mutations from changing saved data.
    const flat = new Uint8Array(previous.flat.length + batch);
    flat.set(previous.flat);
    const lengths = new Uint32Array(previous.lengths.length + admitted.length);
    lengths.set(previous.lengths);
    let offset = previous.flat.length;
    const encoded = previous.encoded.slice();
    for (let i = 0; i < admitted.length; i++) {
      const {destination, bytes} = admitted[i];
      flat.set(bytes, offset); lengths[previous.lengths.length + i] = bytes.length;
      encoded.push(Object.freeze({destination, bytes: encode(flat.subarray(offset, offset + bytes.length))}));
      offset += bytes.length;
    }
    const next = {encoded: Object.freeze(encoded), flat, lengths, names,
      destinations: encoded.map(image => image.destination)};
    let committed = null, ended = false;
    return Object.freeze({
      images: next.encoded,
      // The optional synchronous publisher writes inert JSON before the state
      // switch. If persistence throws, neither the renderer nor source changes.
      commit(publish) {
        if (ended || committed !== null || revision !== generation) throw new Error('Stale image import transaction');
        publishChange(publish, () => {
          payload.images = next.encoded; images = next; committed = ++generation;
        });
      },
      rollback(publish) {
        if (ended || committed === null || committed !== generation) throw new Error('Stale image import rollback');
        publishChange(publish, () => {
          payload.images = previous.encoded; images = previous; generation++; ended = true;
        });
      },
    });
  }
  const decoder = new TextDecoder('utf-8', {fatal: true});
  let diagnostics = [];
  function source(text) {
    const maximum = 32 * 1024 * 1024;
    if (typeof text !== 'string' || text.length > maximum) {
      throw new RangeError('Workspace source exceeds 32 MiB');
    }
    // Never let a native ABI/TextEncoder silently replace malformed editor
    // Unicode. Count before allocating and keep imported BOM/newlines exact.
    let size = 0;
    for (const character of text) {
      const cp = character.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError('Workspace source contains an unpaired surrogate');
      size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      if (size > maximum) throw new RangeError('Workspace source exceeds 32 MiB');
    }
    return text;
  }
  function take(result, mime) {
    try {
      const raw = result.bytes;
      if (result.mimeType?.split(';')[0] !== mime || !(raw instanceof Uint8Array)) {
        throw new TypeError('Native renderer returned an invalid output envelope');
      }
      if (raw.length === 0 || raw.length > 256 * 1024 * 1024) {
        throw new RangeError('Native renderer returned an invalid output size');
      }
      const bytes = raw.slice();
      const json = result.diagnosticsJson();
      const findings = json === '' ? [] : JSON.parse(json);
      if (!Array.isArray(findings)) throw new TypeError('Native renderer returned invalid diagnostics');
      diagnostics = findings;
      return bytes;
    } finally {
      if (typeof result?.free === 'function') result.free();
    }
  }
  function renderHtml(markdown, display = {}, settings = options) {
    const scale = Math.min(3, Math.max(0.5, settings.fontScale * (display.scale ?? 1)));
    const mode = display.theme === 'light' ? 'disabled'
      : display.theme === 'dark' ? 'auto' : settings.darkMode;
    const bytes = take(bindings.renderHtmlConfiguredAdvanced(
      source(markdown), settings.font, mode, settings.title, undefined, false, scale,
      ...fontBytes, weights, images.destinations, images.flat, images.lengths,
      settings.lang, settings.toc, settings.tocDepth,
    ), 'text/html');
    let html = decoder.decode(bytes);
    // Only generated styles are transformed; Markdown/code remains data.
    // Native HTML supports auto/light; a manual dark view forces its dark
    // media rules without changing the user's OS or leaking CSS into the UI.
    if (display.theme === 'dark') html = html.replace(/(<style\b[^>]*>)([\s\S]*?)(<\/style>)/gi,
      (_, open, css, close) => open + css.replace(/@media\s*\(prefers-color-scheme:\s*dark\)/g, '@media all') + close);
    // The first head item governs even resources appearing early in output.
    // Documents are safe-parsed, and the preview additionally permits no
    // scripts, forms, nested frames or external assets. Images/fonts must be
    // embedded resources. A missing asset never silently reaches the network.
    const head = /<head(?:\s[^>]*)?>/i;
    if (!head.test(html)) throw new Error('Native HTML is missing its document head');
    const policy = "default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline' data:; base-uri 'none'; form-action 'none'";
    return html.replace(head, match => match + '<meta http-equiv="Content-Security-Policy" content="' + policy + '">');
  }
  return {
    get diagnostics() { return diagnostics; },
    get settings() { return options; },
    stageSettings,
    stageImages,
    html(markdown, display) { return renderHtml(markdown, display); },
    pdf(markdown) {
      const args = [
        source(markdown), options.font, options.darkMode, options.title, options.author,
        options.metadataEpochSeconds, false, options.codeLineNumbers,
        images.destinations, images.flat, images.lengths, ...fontBytes, weights,
        undefined, undefined, undefined, options.pageNumbers,
        options.fontScale, options.lang, options.toc, options.tocDepth, undefined, false,
      ];
      const result = pageGeometry ? bindings.renderPdfConfiguredPage(...args, pageGeometry)
        : bindings.renderPdfConfiguredMulti(...args);
      const bytes = take(result, 'application/pdf');
      if (String.fromCharCode(...bytes.subarray(0, 5)) !== '%PDF-') throw new Error('Native renderer returned invalid PDF bytes');
      return bytes;
    },
  };
}

export function bootNativeWorkspace(factory, createPreview) {
  const data = document.querySelector('body > script#fmd-native-runtime[type="application/json"]');
  const payload = JSON.parse(data.textContent);
  let renderer = null, failure = null, observer = null, frame = null;
  let worker = null, previewRevision = 0, previewDiagnostics = null, suspended = false;
  const background = typeof createPreview === 'function';
  function invalidatePreview() {
    previewRevision++;
    worker?.invalidate();
  }
  function replacePreview(html, preview) {
    const next = makeFrame(html);
    preview.replaceChildren(next);
    observer?.disconnect(); observer = null; frame = next;
  }
  function savedData(patch) {
    const next = JSON.stringify({...payload, ...patch}).replace(/</g, '\\u003c')
      .replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
    if (next.length > 256 * 1024 * 1024 || new TextEncoder().encode(next).length > 256 * 1024 * 1024) {
      throw new RangeError('Saved workspace resources exceed 256 MiB');
    }
    return next;
  }
  function makeFrame(html) {
    // Render completely before replacing the old preview. Using a sandboxed
    // document preserves native CSS, MathML/SVG, highlights and font faces
    // without exposing the editor toolbar to document selectors or IDs.
    const next = document.createElement('iframe');
    next.title = 'Rust/WASM Markdown preview';
    next.setAttribute('sandbox', 'allow-same-origin');
    next.setAttribute('referrerpolicy', 'no-referrer');
    next.style.cssText = 'display:block;width:100%;border:0;min-height:320px;height:320px;background:transparent';
    next.addEventListener('load', () => {
      if (frame !== next) return;
      const doc = next.contentDocument;
      if (!doc?.body) return;
      const resize = () => {
        if (frame !== next) return;
        // Body geometry (not viewport scrollHeight) allows shrinking after
        // reflow and avoids a ResizeObserver feedback loop.
        const box = doc.body.getBoundingClientRect();
        next.style.height = Math.ceil(Math.max(320, box.height + box.top * 2)) + 'px';
      };
      resize();
      if (typeof ResizeObserver === 'function') {
        observer?.disconnect();
        observer = new ResizeObserver(resize); observer.observe(doc.body);
      }
      doc.fonts?.ready.then(resize).catch(() => {});
    });
    next.srcdoc = html;
    return next;
  }
  const engine = {
    version: 1,
    previewMode: background ? 'worker' : 'synchronous',
    invalidatePreview,
    restartPreview() {
      if (suspended) throw new Error('Preview is suspended');
      invalidatePreview();
      worker?.dispose(); worker = null;
    },
    get diagnostics() { return previewDiagnostics ?? renderer?.diagnostics ?? []; },
    get settings() { return renderer?.settings ?? null; },
    applySettings(patch, markdown, preview, display) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; retry settings after initialization.');
      const transaction = renderer.stageSettings(patch);
      // Full native HTML must succeed before changing either persistent settings
      // or the visible preview. Images/fonts are reused, not decoded or repacked.
      const next = makeFrame(transaction.html(markdown, display));
      const previousData = data.textContent, nextData = savedData({options: transaction.settings});
      const previousChildren = Array.from(preview.childNodes);
      transaction.commit(() => {
        data.textContent = nextData;
        try { preview.replaceChildren(next); }
        catch (error) {
          data.textContent = previousData;
          preview.replaceChildren(...previousChildren);
          throw error;
        }
      });
      invalidatePreview(); previewDiagnostics = null;
      observer?.disconnect(); observer = null; frame = next;
      return renderer.settings;
    },
    stageImages(additions) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; retry image insertion after initialization.');
      const transaction = renderer.stageImages(additions);
      const previous = data.textContent;
      const next = savedData({images: transaction.images});
      return Object.freeze({
        commit() {
          transaction.commit(() => { data.textContent = next; });
          invalidatePreview(); previewDiagnostics = null;
        },
        rollback() {
          transaction.rollback(() => { data.textContent = previous; });
          invalidatePreview(); previewDiagnostics = null;
        },
      });
    },
    render(markdown, preview, display, isCurrent = () => true) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; source is safe. Retry after initialization.');
      if (suspended) return false;
      if (!background) {
        replacePreview(renderer.html(markdown, display), preview);
        previewDiagnostics = null;
        return true;
      }
      // Only immutable committed snapshots enter the worker. Settings and image
      // transactions update payload in place; do not reparse/copy its WASM or
      // resources on each keystroke. The worker caches unchanged revisions.
      if (!worker) worker = createPreview(factory, payload);
      const revision = ++previewRevision, options = renderer.settings, images = payload.images;
      const current = () => !suspended && revision === previewRevision
        && options === renderer.settings && images === payload.images && isCurrent();
      return worker.render(markdown, display, {options, images}).then(result => {
        // Check editor state again *before* DOM publication, including edits
        // made without an input event and a source replacement/undo round trip.
        if (!current()) return false;
        replacePreview(result.html, preview);
        previewDiagnostics = result.diagnostics;
        return true;
      }, error => {
        if (!current() || error?.code === 'PREVIEW_SUPERSEDED') return false;
        // No automatic main-thread fallback or unbounded restart loop. Explicit
        // Restart preview disposes the failed worker and captures current state.
        throw error;
      });
    },
    html(markdown) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; retry HTML publishing after initialization.');
      // A publication is a new complete native document, not a snapshot of the
      // editor or its iframe. Use committed document settings, never view zoom
      // or a forced view theme, and retain the renderer's script-free CSP.
      previewDiagnostics = null;
      return renderer.html(markdown);
    },
    pdf(markdown) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; retry PDF export after initialization.');
      previewDiagnostics = null;
      return renderer.pdf(markdown);
    },
  };
  // Named DOM elements cannot impersonate the handoff to the app controller.
  Object.defineProperty(window, '__fmdNativeRuntime', {value: engine});
  let moduleUrl = null;
  engine.ready = (async () => {
    try {
      const binary = atob(payload.wasm), bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      moduleUrl = URL.createObjectURL(new Blob([payload.bindings], {type: 'text/javascript'}));
      const bindings = await import(moduleUrl);
      if (typeof bindings.default !== 'function') throw new Error('Native workspace requires wasm-bindgen --target web bindings');
      await bindings.default({module_or_path: bytes});
      renderer = factory(bindings, payload);
      window.dispatchEvent(new Event('fmd-native-ready'));
      return true;
    } catch (error) {
      failure = new Error('Native runtime failed: ' + String(error?.message ?? error).slice(0, 2048));
      window.dispatchEvent(new CustomEvent('fmd-native-error', {detail: failure.message}));
      return false; // Failure is visible, never an unhandled rejection or fallback.
    } finally {
      if (moduleUrl) URL.revokeObjectURL(moduleUrl);
    }
  })();
  window.addEventListener('pagehide', () => {
    suspended = true; invalidatePreview();
    worker?.dispose(); worker = null;
    observer?.disconnect(); observer = null; frame = null;
  });
  window.addEventListener('pageshow', () => { suspended = false; });
}
