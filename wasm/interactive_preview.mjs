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
        } else if (request?.type === 'render' && renderer && Number.isSafeInteger(id) && id > 0) {
          if (request.state) {
            const next = {...data, options: request.state.options, images: request.state.images};
            const staged = makeRenderer(bindings, next);
            renderer = staged; data = next;
          }
          const html = renderer.html(request.markdown, request.display);
          if (typeof html !== 'string' || !html.length || html.length > 256 * 1024 * 1024) {
            throw new Error('Invalid or oversized preview HTML');
          }
          // The result owns ordinary data, not a borrowed native result handle.
          self.postMessage({type: 'result', id, html, diagnostics: renderer.diagnostics});
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
    if (failure) job.reject(failure); else job.resolve(value);
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
      worker.postMessage({type: 'render', id: active.id, markdown: active.markdown, display: active.display, state});
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
        text(result.html, 256 * MiB, 'Preview HTML');
        if (!result.html.length || !Array.isArray(result.diagnostics) || result.diagnostics.length > 4096) {
          throw error('PREVIEW_PROTOCOL', 'Invalid background preview result');
        }
        let length = 0;
        for (const item of result.diagnostics) {
          if (!item || typeof item.message !== 'string') throw error('PREVIEW_PROTOCOL', 'Invalid preview diagnostics');
          length += item.message.length;
          if (length > 1024 * 1024) throw error('PREVIEW_LIMIT', 'Preview diagnostics exceed their limit');
        }
        settle(job, {html: result.html, diagnostics: result.diagnostics});
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
          // Admit before superseding a valid request. Resource records are private
          // committed immutable snapshots supplied by the owning workspace.
          const view = {scale: display.scale, theme: display.theme};
          settle(active, null, superseded()); settle(queued, null, superseded());
          queued = {id: ++sequence, markdown, display: view, options: state.options, images: state.images, resolve, reject, settled: false};
          start(); dispatch();
        } catch (err) { reject(err); }
      });
    },
    invalidate() {
      settle(active, null, superseded()); settle(queued, null, superseded()); queued = null;
    },
    dispose() { stop(error('PREVIEW_CLOSED', 'Background preview was stopped')); },
  });
}
