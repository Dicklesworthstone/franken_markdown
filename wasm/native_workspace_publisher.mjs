// The same native shell/preview assembler serves ordinary exports and project
// reopening in a disposable worker. This factory is embedded in its own output;
// keep it import-free and keep every dependency in the trusted parts object.
export function createNativeWorkspacePublisher(dependencies) {
  const names = ['bootNativeWorkspace', 'createNativeWorkspaceRenderer',
    'createWorkspacePreviewWorker', 'createNativePdfProof', 'registerNativePdfProof',
    'installNativeSourceProjectDownload', 'createNativeSourceProjectCodec'];
  const optional = ['createNativeSourceProjectOpenWorker', 'installNativeSourceProjectOpen'];
  const parts = Object.freeze(Object.fromEntries([...names, ...optional]
    .filter(name => dependencies[name] !== undefined).map(name => [name, dependencies[name]])));
  for (const name of names) if (typeof parts[name] !== 'function') throw new TypeError('Missing workspace implementation: ' + name);
  for (const name of optional) if (parts[name] !== undefined && typeof parts[name] !== 'function') throw new TypeError('Invalid workspace implementation: ' + name);
  const {createNativeWorkspaceRenderer} = parts;
  const NATIVE_WORKSPACE_LIMITS = {sourceBytes: 32 * 1024 * 1024, outputBytes: 256 * 1024 * 1024};
  function serializedParts() {
    return '{' + Object.entries(parts).map(([name, value]) => JSON.stringify(name) + ':'
      + Function.prototype.toString.call(value)).join(',') + '}';
  }
  function startNativeWorkspace(createPublisher, parts) {
    parts.bootNativeWorkspace(parts.createNativeWorkspaceRenderer, parts.createWorkspacePreviewWorker);
    parts.registerNativePdfProof(parts.createNativePdfProof, window.__fmdNativeRuntime, function(host) {
      parts.installNativeSourceProjectDownload(parts.createNativeSourceProjectCodec, window.__fmdNativeRuntime, host);
      parts.installNativeSourceProjectOpen?.(createPublisher, parts, window.__fmdNativeRuntime, host);
    });
  }
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


  function jsonData(value) {
    return JSON.stringify(value).replace(/</g, '\\u003c')
      .replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
  }

  function attribute(text) {
    return text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
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
    const scriptTag = '<' + 'script', scriptEndTag = '<' + '/script>';
    const sourceOpen = scriptTag + ' type="application/json" id="fmd-raw-source">';
    const sourceStart = shell.indexOf(sourceOpen);
    const sourceEnd = shell.indexOf(scriptEndTag, sourceStart + sourceOpen.length);
    const appStart = '<div class="fmd-content" id="fmd-content">';
    const appEnd = '</div>\n  </main>\n</div>\n';
    const previewStart = shell.indexOf(appStart), previewEnd = shell.lastIndexOf(appEnd, sourceStart);
    // The controller must await Promise-returning previews before Save HTML.
    // A marker appearing in Markdown/initial output cannot supply that capability.
    const scriptStart = shell.indexOf(scriptTag + '>\n', sourceEnd + 9);
    const scriptEnd = shell.indexOf(scriptEndTag, scriptStart + 9);
    if (sourceStart < 0 || sourceEnd < 0 || previewStart < 0 || previewEnd < previewStart
        || scriptStart < sourceEnd || scriptEnd < scriptStart
        || !shell.slice(scriptStart, scriptEnd).includes('__fmdNativeRuntime')
        || !shell.slice(scriptStart, scriptEnd).includes('fmd-async-preview-v1')
        || JSON.parse(shell.slice(sourceStart + sourceOpen.length, sourceEnd)) !== source) {
      throw Object.assign(new Error('Native workspace requires a rebuilt matching WASM package with the background preview editor controller'), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    }
    // Replace, do not append to, the initial unsandboxed fragment. Even before
    // initialization the complete native preview gets the iframe resource policy.
    const frame = '<iframe title="Rust/WASM Markdown preview" sandbox="allow-same-origin" referrerpolicy="no-referrer" '
      + 'style="display:block;width:100%;border:0;min-height:320px;height:320px" srcdoc="' + attribute(preview) + '"></iframe>';
    const bootstrap = scriptTag + ' type="application/json" id="fmd-native-runtime">' + jsonData(payload) + scriptEndTag + '\n'
      + scriptTag + ' id="fmd-native-bootstrap">\n;(' + startNativeWorkspace.toString() + ')('
      + createNativeWorkspacePublisher.toString() + ', ' + serializedParts() + ');\n' + scriptEndTag + '\n';
    // These offsets refer to the original shell. Assembly never searches the
    // newly injected source, binary, binding text or rendered document for tags.
    const output = shell.slice(0, previewStart + appStart.length) + frame
      + shell.slice(previewEnd, scriptStart) + bootstrap + shell.slice(scriptStart);
    const head = /<head(?:\s[^>]*)?>/i;
    if (!head.test(output)) throw new Error('Native workspace shell is missing its document head');
    // Allow only the embedded application, Blob module and WASM compilation.
    // Browser PDF viewers may use an internal Blob frame for the native proof.
    // Permit only local Blob objects/frames here. The Markdown iframe retains its
    // separate stricter policy: no scripts, objects, frames or remote resources.
    const policy = "default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; frame-src 'self' about: blob:; object-src blob:; base-uri 'none'; form-action 'none'";
    return output.replace(head, match => match + '\n<meta http-equiv="Content-Security-Policy" content="' + policy + '">');
  }

  function filename(value = 'document') {
    let name = Array.from(String(value)).slice(0, 80).join('')
      .replace(/[<>:"/\\|?*\u0000-\u001f\u007f]/g, '_').trim().replace(/[. ]+$/, '');
    if (!name || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name)) name = 'document';
    return name + '.html';
  }


  function render(engine, markdown, payload) {
    const sourceLength = textSize(markdown, NATIVE_WORKSPACE_LIMITS.sourceBytes, 'workspace source');
    for (const key of ['renderInteractiveHtmlConfigured', 'renderHtmlConfiguredAdvanced', 'renderPdfConfiguredMulti']) {
      if (typeof engine?.[key] !== 'function') throw Object.assign(new Error(`Matching WASM bindings required: ${key}`), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    }
    const opts = payload.options;
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
  }
  return Object.freeze({render});
}
