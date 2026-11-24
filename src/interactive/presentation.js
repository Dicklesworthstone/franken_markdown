// Included only for configured lightweight workspaces. The native WASM editor
// owns its own preview and never installs this adapter. Source remains data.
function fmdCreateStyledPresentation(config) {
  if (config?.version !== 1 || typeof config.css !== 'string'
      || config.css.length > 192 * 1024 * 1024 || typeof config.title !== 'string'
      || typeof config.lang !== 'string' || typeof config.toc !== 'boolean'
      || !Number.isInteger(config.tocDepth) || config.tocDepth < 1 || config.tocDepth > 6) {
    throw Error('Invalid saved document presentation');
  }
  const escape = value => String(value).replace(/[&<>"']/g, ch =>
    ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[ch]));
  // Navigation belongs to the document parser: it knows authored marker
  // placement, real heading labels/IDs and which headings live in notes.
  // Capture settings once so edited and printed fragments use the same policy.
  const contents = Object.freeze({toc: config.toc, tocDepth: config.tocDepth});
  function render(parser, source, assets, profile) {
    return parser(source, assets, profile, contents);
  }
  function documentHtml(fragment, display = {}) {
    if (typeof fragment !== 'string' || fragment.length > 32 * 1024 * 1024) throw Error('Preview exceeds its output limit');
    let css = config.css;
    if (display.theme === 'dark') css = css.replace(/@media\s*\(prefers-color-scheme:\s*dark\)/g, '@media all');
    if (display.theme === 'light') css = css.replace(/@media\s*\(prefers-color-scheme:\s*dark\)/g, '@media not all');
    const scale = Number.isFinite(display.scale) ? Math.min(2, Math.max(0.7, display.scale)) : 1;
    // Escape raw-text closers again when opening a manually edited saved file.
    css = css.replace(/<\/style/gi, '<\\/style');
    const policy = "default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline' data:; base-uri 'none'; form-action 'none'";
    return '<!DOCTYPE html><html lang="' + escape(config.lang) + '"><head><meta charset="utf-8">'
      + '<meta http-equiv="Content-Security-Policy" content="' + escape(policy) + '"><title>'
      + escape(config.title) + '</title><style>' + css + '</style></head><body style="zoom:' + scale
      + '"><main class="fmd">' + fragment + '</main></body></html>';
  }
  const displayKey = (display = {}) => JSON.stringify([
    Number.isFinite(display.scale) ? Math.min(2, Math.max(0.7, display.scale)) : 1,
    display.theme === 'dark' || display.theme === 'light' ? display.theme : null,
  ]);
  function frame(fragment, display) {
    return '<iframe data-fmd-styled-preview title="Markdown preview" sandbox="allow-same-origin allow-modals" '
      + 'data-fmd-display="' + escape(displayKey(display)) + '" '
      + 'referrerpolicy="no-referrer" style="display:block;width:100%;border:0;min-height:320px;height:100vh" srcdoc="'
      + escape(documentHtml(fragment, display)) + '"></iframe>';
  }
  return Object.freeze({render, documentHtml, displayKey, frame});
}

(function() {
  const candidate = Object.getOwnPropertyDescriptor(window, '__fmdNativeRuntime')?.value;
  if (document.querySelector('body > script#fmd-native-runtime[type="application/json"]')
      || (candidate?.version === 1 && typeof candidate.render === 'function'
        && typeof candidate.pdf === 'function')) return;
  const data = document.querySelector('body > script#fmd-presentation[type="application/json"]');
  const styled = fmdCreateStyledPresentation(JSON.parse(data.textContent));
  const preview = document.getElementById('fmd-content'), editor = document.getElementById('fmd-editor');
  const template = document.createElement('template');
  template.innerHTML = preview.querySelector('iframe[data-fmd-styled-preview]').getAttribute('srcdoc');
  const initialFragment = template.content.querySelector('main.fmd').innerHTML;
  const initialSource = JSON.parse(document.querySelector('body > script#fmd-raw-source[type="application/json"]').textContent);
  const originalParse = parseMarkdownClient;
  const authoringProfile = (() => {
    try { return Object.freeze({value: fmdReadAuthoringProfile(document), error: null}); }
    catch (error) { return Object.freeze({value: undefined, error}); }
  })();
  let fragment = initialFragment, observedFrame = null, resize = null, printRevision = 0;
  let composing = false, suspended = false;
  const display = () => ({scale: parseFloat(document.documentElement.style.getPropertyValue('--fmd-base')) / 16,
    theme: document.body.classList.contains('theme-dark') ? 'dark'
      : document.body.classList.contains('theme-light') ? 'light' : undefined});
  parseMarkdownClient = function(source, assets, profile) {
    fragment = styled.render(originalParse, source, assets, profile);
    return styled.frame(fragment, display());
  };
  function measure(frame) {
    if (frame !== observedFrame) return;
    const body = frame.contentDocument?.body;
    if (body) frame.style.height = Math.min(1000000, Math.max(320, body.scrollHeight,
      frame.contentDocument.documentElement.scrollHeight)) + 'px';
  }
  function observeFrame() {
    const frame = preview.querySelector('iframe[data-fmd-styled-preview]');
    if (!frame || frame === observedFrame) return;
    resize?.disconnect(); observedFrame = frame;
    // Exact undo restores the controller's original native frame. Its content
    // stays intact, but the reader's current view must still apply to it.
    const currentDisplay = display(), key = styled.displayKey(currentDisplay);
    if (frame.getAttribute('data-fmd-display') !== key) {
      const t = document.createElement('template'); t.innerHTML = frame.getAttribute('srcdoc');
      frame.setAttribute('srcdoc', styled.documentHtml(t.content.querySelector('main.fmd').innerHTML, currentDisplay));
      frame.setAttribute('data-fmd-display', key);
    }
    let navigationDocument = null;
    const loaded = () => {
      if (frame !== observedFrame) return;
      measure(frame);
      const content = frame.contentDocument;
      if (content && content !== navigationDocument) {
        navigationDocument = content;
        // A srcdoc's relative URLs inherit the outer file's base URL. Handle
        // document-local anchors ourselves so TOC/footnote links cannot replace
        // the preview with a second copy of the surrounding workspace.
        content.addEventListener('click', event => {
          const anchor = event.target?.closest?.('a[href]');
          const href = anchor?.getAttribute('href');
          if (!href?.startsWith('#')) return;
          event.preventDefault();
          let id = href.slice(1);
          try { id = decodeURIComponent(id); } catch { /* A literal percent is legal in an ID. */ }
          const target = id ? content.getElementById(id) : content.documentElement;
          target?.scrollIntoView({block: 'start'});
          target?.focus({preventScroll: true});
        });
      }
      if (typeof ResizeObserver === 'function' && frame.contentDocument?.body) {
        resize = new ResizeObserver(() => measure(frame)); resize.observe(frame.contentDocument.body);
      }
      frame.contentDocument?.fonts?.ready.then(() => measure(frame));
    };
    frame.addEventListener('load', loaded); loaded();
  }
  const changes = new MutationObserver(observeFrame);
  changes.observe(preview, {childList: true}); observeFrame();
  const view = new MutationObserver(() => {
    // Rebuild from the current frame's body: exact-undo uses the native initial
    // fragment, which may contain diagrams outside the lightweight parser.
    const frame = preview.querySelector('iframe[data-fmd-styled-preview]');
    if (!frame) return;
    const t = document.createElement('template'); t.innerHTML = frame.getAttribute('srcdoc');
    fragment = t.content.querySelector('main.fmd').innerHTML;
    preview.innerHTML = styled.frame(fragment, display());
  });
  view.observe(document.documentElement, {attributes: true, attributeFilter: ['style']});
  view.observe(document.body, {attributes: true, attributeFilter: ['class']});
  function report(error) {
    document.getElementById('fmd-save-status').textContent = 'Unable to print: ' + String(error?.message ?? error).slice(0, 2048);
  }
  async function printCurrent() {
    if (composing || suspended) throw Error('Finish editing before printing');
    if (authoringProfile.error) throw authoringProfile.error;
    const source = editor.value, revision = ++printRevision;
    // Flush the controller's pending debounce before preparing a print frame.
    // Otherwise its later render can replace this frame while fonts load.
    window.dispatchEvent(new Event('beforeprint'));
    if (source === initialSource.replace(/\r\n?/g, '\n')) fragment = initialFragment;
    else {
      const assets = new Map();
      const imageData = document.querySelector('body > script#fmd-image-assets[type="application/json"]');
      for (const [key, bytes] of imageData ? JSON.parse(imageData.textContent) : []) if (!assets.has(key)) assets.set(key, bytes);
      fragment = styled.render(originalParse, source, assets, authoringProfile.value);
    }
    preview.innerHTML = styled.frame(fragment, display()); observeFrame();
    const frame = observedFrame;
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(Error('Preview is still loading; retry print')), 10000);
      frame.addEventListener('load', () => {clearTimeout(timer); resolve();}, {once: true});
    });
    await frame.contentDocument?.fonts?.ready;
    if (revision !== printRevision || composing || suspended || editor.value !== source || frame !== observedFrame) throw Error('Document changed; retry print');
    frame.contentWindow.focus(); frame.contentWindow.print();
  }
  document.getElementById('btn-export-pdf').addEventListener('click', event => {
    event.preventDefault(); event.stopImmediatePropagation(); printCurrent().catch(report);
  }, true);
  document.addEventListener('keydown', event => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey || event.isComposing || composing || event.key.toLowerCase() !== 'p') return;
    event.preventDefault(); event.stopImmediatePropagation();
    if (!event.repeat) printCurrent().catch(report);
  }, true);
  editor.addEventListener('input', () => {printRevision++;});
  editor.addEventListener('compositionstart', () => {composing = true; printRevision++;});
  editor.addEventListener('compositionend', () => {composing = false;});
  window.addEventListener('pagehide', () => {suspended = true; printRevision++; resize?.disconnect();});
  window.addEventListener('pageshow', () => {suspended = false; composing = false;});
})();
