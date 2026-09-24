// Serialized into portable workspaces. Keep this function import-free; renderer
// and bindings are trusted application code, never Markdown-controlled imports.
export function createWorkspacePreviewWorker(factory, payload, configuration = {}) {
  const MiB = 1024 * 1024;
  const timeoutMs = configuration.timeoutMs ?? 30000;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000) {
    throw new RangeError('Preview timeout must be 1..120000 milliseconds');
  }
  const error = (code, message) => Object.assign(new Error(message), {code});
  function text(value, limit, name) {
    if (typeof value !== 'string' || value.length > limit) throw error('PREVIEW_LIMIT', name + ' exceeds its byte limit');
    let length = 0;
    for (const character of value) {
      const cp = character.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) throw error('PREVIEW_UNICODE', 'Invalid Unicode in ' + name);
      length += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      if (length > limit) throw error('PREVIEW_LIMIT', name + ' exceeds its byte limit');
    }
  }
  // Worker-owned WASM and resource cache. No URLs or functions arrive in render
  // requests. A state revision replaces the renderer only after validation.
  async function serve(makeRenderer) {
    let bindings = null, data = null, renderer = null, initialized = false;
    const report = (id, err) => self.postMessage({type: 'error', id,
      message: String(err?.message ?? err).slice(0, 2048)});
    self.onmessage = async ({data: request}) => {
      const id = request?.id;
      try {
        if (request?.type === 'init' && !initialized) {
          initialized = true;
          data = request.payload;
          const binary = atob(data.wasm), bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
          const url = URL.createObjectURL(new Blob([data.bindings], {type: 'text/javascript'}));
          try {
            bindings = await import(url);
            if (typeof bindings.default !== 'function') throw new Error('Missing WASM initialization export');
            await bindings.default({module_or_path: bytes});
            renderer = makeRenderer(bindings, data);
          } finally { URL.revokeObjectURL(url); }
          self.postMessage({type: 'ready'});
        } else if (['render', 'export'].includes(request?.type) && renderer && Number.isSafeInteger(id) && id > 0) {
          if (request.state) {
            const next = {...data, options: request.state.options, images: request.state.images};
            const staged = makeRenderer(bindings, next);
            renderer = staged; data = next;
          }
          if (request.type === 'export') {
            const format = request.format;
            let bytes;
            if (format === 'html') {
              // Publication uses committed document settings, never viewing zoom.
              const html = renderer.html(request.markdown);
              if (typeof html !== 'string' || !html.length || html.length > 256 * 1024 * 1024) {
                throw new Error('Invalid or oversized publication HTML');
              }
              let length = 0;
              for (const character of html) {
                const cp = character.codePointAt(0);
                if (cp >= 0xd800 && cp <= 0xdfff) throw new Error('Invalid publication Unicode');
                length += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
                if (length > 256 * 1024 * 1024) throw new Error('Publication HTML exceeds its byte limit');
              }
              bytes = new TextEncoder().encode(html);
            } else if (format === 'pdf') {
              const value = renderer.pdf(request.markdown);
              if (!(value instanceof Uint8Array) || !value.length || value.length > 256 * 1024 * 1024) {
                throw new Error('Invalid or oversized PDF bytes');
              }
              // Never transfer a borrowed/pool/WASM buffer. Own the exact view.
              bytes = value.slice();
              if (String.fromCharCode(...bytes.subarray(0, 5)) !== '%PDF-') throw new Error('Invalid PDF signature');
            } else throw new Error('Unsupported export format');
            const findings = renderer.diagnostics;
            if (!Array.isArray(findings) || findings.length > 4096
                || findings.some(item => !item || typeof item.message !== 'string')) {
              throw new Error('Invalid export diagnostics');
            }
            const json = JSON.stringify(findings);
            if (json.length > 1024 * 1024 || new TextEncoder().encode(json).length > 1024 * 1024) {
              throw new Error('Export diagnostics exceed their byte limit');
            }
            self.postMessage({type: 'result', id, format, bytes,
              mimeType: format === 'pdf' ? 'application/pdf' : 'text/html;charset=utf-8',
              diagnostics: JSON.parse(json)}, [bytes.buffer]);
          } else {
            const html = renderer.html(request.markdown, request.display);
            if (typeof html !== 'string' || !html.length || html.length > 256 * 1024 * 1024) {
              throw new Error('Invalid or oversized preview HTML');
            }
            // The result owns ordinary data, not a borrowed native result handle.
            self.postMessage({type: 'result', id, html, diagnostics: renderer.diagnostics});
          }
        } else throw new Error('Invalid preview worker request');
      } catch (err) { report(id, err); }
    };
  }
  const source = `(${serve.toString()})(${factory.toString()});`;
  let worker = null, moduleUrl = null, timer = null, started = false, closed = false;
  let active = null, queued = null, sequence = 0;
  let lastOptions = null, lastImages = null;
  const superseded = () => error('PREVIEW_SUPERSEDED', 'A newer edit superseded this preview');
  function settle(job, value, failure) {
    if (!job || job.settled) return;
    job.settled = true;
    if (failure) job.reject(job.format ? exportError(failure) : failure); else job.resolve(value);
  }
  function exportError(reason) {
    return error(String(reason?.code || 'EXPORT_FAILED').replace(/^PREVIEW_/, 'EXPORT_'),
      String(reason?.message || reason).replace(/preview/gi, 'export').slice(0, 2048));
  }
  function revoke() {
    if (moduleUrl !== null) { URL.revokeObjectURL(moduleUrl); moduleUrl = null; }
  }
  function stop(reason) {
    if (closed) return;
    closed = true;
    clearTimeout(timer); timer = null;
    const owned = worker; worker = null;
    if (owned) {
      owned.removeEventListener('message', receive);
      owned.removeEventListener('error', failed);
      owned.removeEventListener('messageerror', failed);
      try { owned.terminate()?.catch?.(() => {}); } catch { /* already stopped */ }
    }
    revoke();
    settle(active, null, reason); settle(queued, null, reason);
    active = null; queued = null;
  }
  function deadline() {
    clearTimeout(timer);
    timer = setTimeout(() => stop(error('PREVIEW_TIMEOUT', 'Background preview timed out; source is unchanged')), timeoutMs);
  }
  function failed(event) {
    event?.preventDefault?.();
    stop(error('PREVIEW_WORKER_FAILED', 'Background preview worker failed; source is unchanged'));
  }
  function dispatch() {
    if (!started || closed || active || !queued) return;
    active = queued; queued = null;
    const state = active.options === lastOptions && active.images === lastImages
      ? null : {options: active.options, images: active.images};
    deadline();
    try {
      worker.postMessage({type: active.format ? 'export' : 'render', format: active.format,
        id: active.id, markdown: active.markdown, display: active.display, state});
      lastOptions = active.options; lastImages = active.images;
    } catch (err) { stop(error('PREVIEW_SEND_FAILED', String(err?.message ?? err).slice(0, 2048))); }
  }
  function receive(event) {
    if (closed) return;
    const result = event.data;
    if (!started && result?.type === 'ready') {
      started = true; clearTimeout(timer); timer = null; revoke(); dispatch(); return;
    }
    if (result?.type === 'error' && !started) {
      stop(error('PREVIEW_START_FAILED', String(result.message).slice(0, 2048))); return;
    }
    if (!started || !active || result?.id !== active.id || !['result', 'error'].includes(result?.type)) {
      stop(error('PREVIEW_PROTOCOL', 'Unexpected background preview response')); return;
    }
    clearTimeout(timer); timer = null;
    const job = active; active = null;
    if (result.type === 'error') {
      // Do not reuse a resource cache whose attempted revision was rejected.
      lastOptions = null; lastImages = null;
      settle(job, null, error('PREVIEW_RENDER_FAILED', String(result.message).slice(0, 2048)));
    } else if (!job.settled) {
      try {
        if (job.format) {
          const mime = job.format === 'pdf' ? 'application/pdf' : 'text/html;charset=utf-8';
          if (result.format !== job.format || result.mimeType !== mime
              || !(result.bytes instanceof Uint8Array) || !result.bytes.length || result.bytes.length > 256 * MiB
              || !(result.bytes.buffer instanceof ArrayBuffer)
              || result.bytes.byteOffset !== 0 || result.bytes.byteLength !== result.bytes.buffer.byteLength) {
            throw error('EXPORT_PROTOCOL', 'Invalid background export result');
          }
          if (job.format === 'pdf' && String.fromCharCode(...result.bytes.subarray(0, 5)) !== '%PDF-') {
            throw error('EXPORT_PROTOCOL', 'Invalid background PDF signature');
          }
          text(JSON.stringify(result.diagnostics), MiB, 'Export diagnostics');
        } else {
          text(result.html, 256 * MiB, 'Preview HTML');
          if (!result.html.length) throw error('PREVIEW_PROTOCOL', 'Empty background preview result');
        }
        if (!Array.isArray(result.diagnostics) || result.diagnostics.length > 4096) {
          throw error('PREVIEW_PROTOCOL', 'Invalid background preview result');
        }
        let length = 0;
        for (const item of result.diagnostics) {
          if (!item || typeof item.message !== 'string') throw error('PREVIEW_PROTOCOL', 'Invalid preview diagnostics');
          length += item.message.length;
          if (length > 1024 * 1024) throw error('PREVIEW_LIMIT', 'Preview diagnostics exceed their limit');
        }
        settle(job, job.format ? {format: job.format, bytes: result.bytes, mimeType: result.mimeType,
          diagnostics: result.diagnostics} : {html: result.html, diagnostics: result.diagnostics});
      } catch (err) { settle(job, null, err); stop(err); return; }
    }
    dispatch();
  }
  function start() {
    if (worker || closed) return;
    try {
      if (configuration.workerFactory) worker = configuration.workerFactory(source);
      else {
        if (typeof Worker !== 'function') throw new Error('Workers are unavailable');
        moduleUrl = URL.createObjectURL(new Blob([source], {type: 'text/javascript'}));
        // The bootstrap has no static imports. A classic worker can dynamically
        // import the embedded bindings too, and works in portable/opaque-origin
        // contexts where module-worker startup is refused by some browsers.
        worker = new Worker(moduleUrl, {name: 'franken-markdown-preview'});
      }
      worker.addEventListener('message', receive);
      worker.addEventListener('error', failed);
      worker.addEventListener('messageerror', failed);
      deadline();
      worker.postMessage({type: 'init', payload});
    } catch (err) { stop(error('PREVIEW_START_FAILED', String(err?.message ?? err).slice(0, 2048))); }
  }
  return Object.freeze({
    get closed() { return closed; },
    get pendingOperations() { return Number(Boolean(active)) + Number(Boolean(queued)); },
    render(markdown, display, state) {
      return new Promise((resolve, reject) => {
        if (closed) { reject(error('PREVIEW_CLOSED', 'Background preview is closed')); return; }
        try {
          text(markdown, 32 * MiB, 'Workspace source');
          if (!display || !Number.isFinite(display.scale) || display.scale < 0.7 || display.scale > 2
              || ![undefined, 'light', 'dark'].includes(display.theme) || !state?.options || !Array.isArray(state.images)) {
            throw error('PREVIEW_OPTIONS', 'Invalid background preview options');
          }
          if (active?.format || queued?.format) throw error('EXPORT_BUSY', 'A document export is already running');
          // Admit before superseding a valid request. Resource records are private
          // committed immutable snapshots supplied by the owning workspace.
          const view = {scale: display.scale, theme: display.theme};
          settle(active, null, superseded()); settle(queued, null, superseded());
          queued = {id: ++sequence, markdown, display: view, options: state.options, images: state.images, resolve, reject, settled: false};
          start(); dispatch();
        } catch (err) { reject(err); }
      });
    },
    // Use a separate client for exports so editing cannot starve an explicit
    // export. This lane has no queue: a second operation is refused, not hidden.
    exportDocument(format, markdown, state) {
      return new Promise((resolve, reject) => {
        try {
          if (closed) throw error('EXPORT_CLOSED', 'Background export is closed');
          if (!['html', 'pdf'].includes(format) || !state?.options || !Array.isArray(state.images)) {
            throw error('EXPORT_OPTIONS', 'Invalid background export options');
          }
          text(markdown, 32 * MiB, 'Workspace source');
          if (active || queued) throw error('EXPORT_BUSY', 'A document operation is already running');
          queued = {id: ++sequence, format, markdown, options: state.options, images: state.images,
            resolve, reject, settled: false};
          start(); dispatch();
        } catch (err) { reject(exportError(err)); }
      });
    },
    invalidate() {
      if (active?.format || queued?.format) {
        stop(error('EXPORT_CANCELLED', 'Background export was cancelled')); return;
      }
      settle(active, null, superseded()); settle(queued, null, superseded()); queued = null;
    },
    dispose() { stop(error('PREVIEW_CLOSED', 'Background preview was stopped')); },
  });
}
