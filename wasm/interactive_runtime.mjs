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
  const options = payload.options;
  // Page settings were normalized by the host's shared PDF geometry contract.
  // Revalidate the saved six-number representation before allocation/dispatch.
  let pageGeometry = null;
  if (options.pageGeometry !== undefined) {
    const input = options.pageGeometry;
    const g = Array.isArray(input) && input.length === 6 ? Array.from(input) : null;
    const extent = (size, first, second) => Math.fround(Math.fround(Math.fround(size) - Math.fround(first)) - Math.fround(second));
    if (!g || !g.every(Number.isFinite)
        || g[0] < 144 || g[1] < 144 || g.some(value => value < 0 || value > 14400)
        || g[0] - g[5] - g[3] < 72 || g[1] - g[2] - g[4] < 72
        || extent(g[0], g[5], g[3]) < 72 || extent(g[1], g[2], g[4]) < 72) {
      throw new RangeError('Invalid saved PDF page geometry');
    }
    if (typeof bindings.renderPdfConfiguredPage !== 'function') {
      throw Object.assign(new Error('PDF paper and margins require matching page-capable WASM bindings'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    }
    pageGeometry = new Float64Array(g);
  }
  function decode(text) {
    const binary = atob(text);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }
  const images = payload.images.map(image => ({destination: image.destination, bytes: decode(image.bytes)}));
  const lengths = Uint32Array.from(images, image => image.bytes.length);
  const flat = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
  let offset = 0;
  for (const image of images) { flat.set(image.bytes, offset); offset += image.bytes.length; }
  const slots = ['body-regular', 'body-bold', 'body-italic', 'body-bold-italic', 'mono-regular'];
  const fonts = slots.map(slot => payload.fonts.find(font => font.slot === slot));
  const fontBytes = fonts.map(font => font ? decode(font.bytes) : new Uint8Array());
  const weights = Uint32Array.from(fonts, font => font?.weight ?? 0);
  const destinations = images.map(image => image.destination);
  const decoder = new TextDecoder('utf-8', {fatal: true});
  let diagnostics = [];
  function source(text) {
    if (typeof text !== 'string' || text.length > 32 * 1024 * 1024
        || new TextEncoder().encode(text).length > 32 * 1024 * 1024) {
      throw new RangeError('Workspace source exceeds 32 MiB');
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
  return {
    get diagnostics() { return diagnostics; },
    html(markdown, display = {}) {
      const scale = Math.min(3, Math.max(0.5, options.fontScale * (display.scale ?? 1)));
      const mode = display.theme === 'light' ? 'disabled'
        : display.theme === 'dark' ? 'auto' : options.darkMode;
      const bytes = take(bindings.renderHtmlConfiguredAdvanced(
        source(markdown), options.font, mode, options.title, undefined, false, scale,
        ...fontBytes, weights, destinations, flat, lengths,
        options.lang, options.toc, options.tocDepth,
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
    },
    pdf(markdown) {
      const args = [
        source(markdown), options.font, options.darkMode, options.title, options.author,
        options.metadataEpochSeconds, false, options.codeLineNumbers,
        destinations, flat, lengths, ...fontBytes, weights,
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

export function bootNativeWorkspace(factory) {
  const data = document.querySelector('body > script#fmd-native-runtime[type="application/json"]');
  const payload = JSON.parse(data.textContent);
  let renderer = null, failure = null, observer = null, frame = null;
  const engine = {
    version: 1,
    get diagnostics() { return renderer?.diagnostics ?? []; },
    render(markdown, preview, display) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; source is safe. Retry after initialization.');
      const html = renderer.html(markdown, display);
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
      observer?.disconnect(); observer = null;
      frame = next;
      preview.replaceChildren(next);
    },
    pdf(markdown) {
      if (!renderer) throw failure ?? new Error('Native renderer is loading; retry PDF export after initialization.');
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
  window.addEventListener('pagehide', () => observer?.disconnect());
}
