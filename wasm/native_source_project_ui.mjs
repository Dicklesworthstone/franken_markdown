// Embedded alongside the native editor. The existing controller supplies exact
// source through its lossless anchor; never read/reinterpret textarea.value.
export function installNativeSourceProjectDownload(createCodec, runtime, host, doc = document) {
  if (!runtime || typeof host?.source !== 'function' || typeof host.ready !== 'function') return null;
  let controls = null, disposed = false;
  function install() {
    if (disposed || controls) return;
    const win = doc.defaultView;
    const header = doc.querySelector('body > .fmd-app-header');
    const saveHtml = header?.querySelector('#btn-save-html');
    const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
    const payload = doc.querySelector('body > script#fmd-native-runtime[type="application/json"]');
    if (!saveHtml || !editor || !payload) return;
    header.querySelector('#fmd-source-project-controls')?.remove();
    const box = doc.createElement('span'); box.id = 'fmd-source-project-controls';
    box.style.cssText = 'display:inline-flex;flex-wrap:wrap;align-items:center;gap:6px;max-width:100%';
    const make = (tag, id, text) => {
      const node = doc.createElement(tag); node.id = id; node.textContent = text; box.appendChild(node); return node;
    };
    const save = make('button', 'btn-save-source-project', 'Save source project');
    const cancel = make('button', 'btn-cancel-source-project', 'Cancel project save');
    for (const button of [save, cancel]) { button.type = 'button'; button.className = 'fmd-btn'; }
    save.title = 'Save exact Markdown, committed settings, images and fonts without the embedded engine';
    const status = make('span', 'fmd-source-project-status', '');
    status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
    status.style.cssText = 'font-size:12px;max-width:18rem;overflow-wrap:anywhere';
    saveHtml.parentNode.insertBefore(box, saveHtml);
    const listeners = [];
    const listen = (target, kind, handler) => { target.addEventListener(kind, handler); listeners.push([target, kind, handler]); };
    let alive = true, suspended = false, composing = false, pending = null, downloadUrl = null;
    function ready() {
      try { return alive && !suspended && !composing && host.ready(); }
      catch { return false; }
    }
    function refresh() {
      save.disabled = !ready() || pending !== null;
      cancel.hidden = pending === null; cancel.disabled = pending === null;
      box.setAttribute('aria-busy', String(pending !== null));
    }
    function revokeDownload() {
      if (downloadUrl !== null) win.URL.revokeObjectURL(downloadUrl);
      downloadUrl = null;
    }
    function retire(job) {
      if (!job) return;
      if (job.timer !== null) win.clearTimeout(job.timer);
      if (job.worker) {
        job.worker.onmessage = job.worker.onerror = job.worker.onmessageerror = null;
        try {
          const stopped = job.worker.terminate();
          if (stopped && typeof stopped.catch === 'function') stopped.catch(() => {});
        } catch { /* The retired operation can never publish again. */ }
      }
    }
    function invalidate(message) {
      const job = pending; pending = null;
      retire(job); revokeDownload();
      if (job && message) status.textContent = message;
      refresh();
    }
    function current(job) {
      try { return pending === job && ready() && host.source() === job.source && payload.textContent === job.runtime; }
      catch { return false; }
    }
    function finish(job, message) {
      if (pending !== job) return;
      pending = null; retire(job); status.textContent = message; refresh();
    }
    function saveProject() {
      if (!ready() || pending) return false;
      revokeDownload();
      let job;
      try { job = {worker: null, timer: null, source: host.source(), runtime: payload.textContent}; }
      catch (error) { status.textContent = 'Unable to capture source project: ' + String(error?.message ?? error).slice(0, 512); return false; }
      pending = job; status.textContent = 'Preparing data-only source project…'; refresh();
      let url = null;
      try {
        if (typeof win.Worker !== 'function') throw Error('This browser cannot create the project-encoding worker');
        if (typeof job.source !== 'string' || job.source.length > 32 * 1024 * 1024
            || typeof job.runtime !== 'string' || job.runtime.length > 256 * 1024 * 1024) throw Error('Source or saved resources exceed the project limit');
        // Only this trusted function becomes worker code. The runtime JSON and
        // Markdown cross as inert structured-clone data, never script fragments.
        const code = 'const codec = (' + createCodec.toString() + ')();\n'
          + 'self.onmessage = function(event) { self.onmessage = null; try {\n'
          + ' const request = event.data; const saved = JSON.parse(request.runtime);\n'
          + ' if (saved.version !== 1) throw Error("Unsupported saved workspace version");\n'
          + ' const output = codec.encode({source: request.source, options: saved.options, images: saved.images, fonts: saved.fonts});\n'
          + ' self.postMessage({ok:true, bytes:output.bytes, filename:output.filename(saved.options.title || "document")}, [output.bytes.buffer]);\n'
          + '} catch(error) { self.postMessage({ok:false, message:String(error && error.message || "Project encoding failed").slice(0,512)}); } };';
        url = win.URL.createObjectURL(new Blob([code], {type: 'text/javascript'}));
        job.worker = new win.Worker(url);
        win.URL.revokeObjectURL(url); url = null;
        job.worker.onmessage = event => {
          if (pending !== job) return;
          if (!current(job)) { invalidate('Document changed; save the current source project again.'); return; }
          try {
            const value = event.data;
            if (!value || typeof value.ok !== 'boolean') throw Error('Invalid project worker response');
            if (!value.ok) throw Error(typeof value.message === 'string' ? value.message.slice(0, 512) : 'Project encoding failed');
            if (!(value.bytes instanceof Uint8Array) || !value.bytes.length || value.bytes.length > 256 * 1024 * 1024
                || typeof value.filename !== 'string' || value.filename.length > 200
                || !value.filename.endsWith('.fmd.json') || /[<>:"/\\|?*\u0000-\u001f\u007f]/.test(value.filename)) throw Error('Invalid project worker output');
            const blob = new Blob([value.bytes], {type: 'application/json'});
            const download = win.URL.createObjectURL(blob);
            if (!current(job)) { win.URL.revokeObjectURL(download); invalidate('Document changed; save again.'); return; }
            downloadUrl = download;
            const link = doc.createElement('a'); link.href = download; link.download = value.filename;
            try {
              doc.body.appendChild(link);
              if (!current(job)) { invalidate('Document changed; save again.'); return; }
              link.click();
            } finally { link.remove(); }
            finish(job, 'Source-project download requested; check your downloads.');
          } catch (error) {
            revokeDownload();
            finish(job, 'Project save failed: ' + String(error?.message ?? error).slice(0, 512));
          }
        };
        job.worker.onerror = event => { event.preventDefault?.(); finish(job, 'Project worker failed; document unchanged. Retry explicitly.'); };
        job.worker.onmessageerror = () => finish(job, 'Project response could not be decoded; document unchanged.');
        job.timer = win.setTimeout(() => finish(job, 'Project encoding timed out; document unchanged. Retry explicitly.'), 30000);
        if (!current(job)) { invalidate('Document changed; save again.'); return false; }
        job.worker.postMessage({source: job.source, runtime: job.runtime});
        return true;
      } catch (error) {
        if (url !== null) win.URL.revokeObjectURL(url);
        finish(job, 'Project save failed: ' + String(error?.message ?? error).slice(0, 512));
        return false;
      }
    }
    listen(save, 'click', saveProject);
    listen(cancel, 'click', () => invalidate('Project save cancelled; document unchanged.'));
    listen(editor, 'input', () => invalidate('Source changed; save the current project again.'));
    listen(editor, 'compositionstart', () => { composing = true; invalidate('Project save cancelled for text composition.'); });
    listen(editor, 'compositionend', () => { composing = false; refresh(); });
    listen(win, 'pagehide', () => { suspended = true; composing = false; invalidate('Workspace suspended; project save cancelled.'); });
    listen(win, 'pageshow', () => { suspended = false; composing = false; refresh(); });
    listen(win, 'fmd-native-ready', refresh);
    listen(box, 'focusin', refresh);
    const observer = new win.MutationObserver(() => invalidate('Committed settings or resources changed; save again.'));
    observer.observe(payload, {childList: true, characterData: true, subtree: true});
    controls = {save: saveProject, cancel: () => invalidate('Project save cancelled; document unchanged.'), dispose() {
      alive = false; invalidate(); observer.disconnect();
      for (const [target, kind, handler] of listeners) target.removeEventListener(kind, handler);
    }};
    runtime.ready?.then(refresh, refresh);
    refresh();
  }
  if (doc.readyState === 'loading') doc.addEventListener('DOMContentLoaded', install, {once: true});
  else install();
  return Object.freeze({save: () => controls?.save() ?? false, cancel: () => controls?.cancel(), dispose() {
    disposed = true; doc.removeEventListener('DOMContentLoaded', install); controls?.dispose();
  }});
}
