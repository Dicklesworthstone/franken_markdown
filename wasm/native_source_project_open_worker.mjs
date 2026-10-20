// One disposable worker owns file reading, strict project admission, native
// startup, rendering and workspace assembly. Only trusted application functions
// become worker code. The chosen file and saved runtime cross as inert data.
export function createNativeSourceProjectOpenWorker(createPublisher, parts, request, env = globalThis) {
  const maximum = 256 * 1024 * 1024;
  const error = (code, message) => Object.assign(new Error(message), {code});
  let size;
  try { size = Object.getOwnPropertyDescriptor(Blob.prototype, 'size').get.call(request?.file); }
  catch { throw error('INVALID_SOURCE_PROJECT', 'Choose a local source-project file'); }
  if (!size || size > maximum) throw error('SOURCE_PROJECT_LIMIT', 'Project file must be nonempty and no larger than 256 MiB');
  if (typeof request.runtime !== 'string' || !request.runtime.length || request.runtime.length > maximum)
    throw error('INVALID_SOURCE_PROJECT', 'Saved workspace runtime is unavailable or exceeds its limit');
  if (typeof env.Worker !== 'function') throw error('PROJECT_OPEN_UNAVAILABLE', 'This browser cannot create a project-opening worker');

  function run(createPublisher, parts) {
    const codec = parts.createNativeSourceProjectCodec();
    const publisher = createPublisher(parts);
    function admitText(value, maximum, label) {
      if (typeof value !== 'string' || !value.length || value.length > maximum) throw Error('Invalid ' + label);
      let size = 0;
      for (const character of value) {
        const cp = character.codePointAt(0);
        if (cp >= 0xd800 && cp <= 0xdfff) throw Error('Invalid Unicode in ' + label);
        size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
        if (size > maximum) throw Error(label + ' exceeds its byte limit');
      }
    }
    async function load(runtime) {
      // The project has already been admitted. Code comes only from the current
      // trusted workspace, never from fields in the selected project file.
      if (!runtime || runtime.version !== 1) throw Error('Unsupported saved workspace runtime');
      admitText(runtime.bindings, 4 * 1024 * 1024, 'runtime bindings');
      if (typeof runtime.wasm !== 'string' || !runtime.wasm.length || runtime.wasm.length % 4
          || runtime.wasm.length > Math.ceil(64 * 1024 * 1024 / 3) * 4) throw Error('Invalid runtime WASM size');
      const binary = atob(runtime.wasm);
      if (binary.length < 8 || binary.length > 64 * 1024 * 1024
          || ![0, 97, 115, 109, 1, 0, 0, 0].every((value, i) => binary.charCodeAt(i) === value)) throw Error('Invalid runtime WASM header');
      const wasm = Uint8Array.from(binary, character => character.charCodeAt(0));
      let url, revoke = false;
      try {
        if (typeof process !== 'undefined' && process.versions?.node) {
          // Node-worker qualification uses an isolated data module; browsers
          // always import a Blob. Both consume exactly the selected binding text.
          const identity = URL.createObjectURL(new Blob()); URL.revokeObjectURL(identity);
          const bytes = new TextEncoder().encode(runtime.bindings), chunks = [];
          for (let start = 0; start < bytes.length; start += 24576)
            chunks.push(btoa(String.fromCharCode(...bytes.subarray(start, start + 24576))));
          url = 'data:text/javascript;base64,' + chunks.join('') + '#' + identity;
        } else {
          url = URL.createObjectURL(new Blob([runtime.bindings], {type: 'text/javascript'})); revoke = true;
        }
        const engine = await import(url);
        if (typeof engine.default !== 'function') throw Error('Matching wasm-bindgen web bindings required');
        await engine.default({module_or_path: wasm});
        return engine;
      } finally { if (revoke) URL.revokeObjectURL(url); }
    }
    self.onmessage = async event => {
      self.onmessage = null;
      try {
        const request = event.data;
        if (!(request.file instanceof Blob) || !request.file.size || request.file.size > codec.limits.outputBytes)
          throw Error('Invalid project file size');
        // This happens before importing or initializing even the trusted engine.
        const project = codec.decode(await request.file.arrayBuffer());
        admitText(request.runtime, 256 * 1024 * 1024, 'saved runtime');
        const runtime = JSON.parse(request.runtime);
        const engine = await load(runtime);
        const payload = {version: 1, wasm: runtime.wasm, bindings: runtime.bindings,
          options: project.options, images: project.images,
          // Match the ordinary exporter's field order without decoding and
          // re-encoding every resource just to package it again.
          fonts: project.fonts.map(font => ({slot: font.slot, weight: font.weight, bytes: font.bytes}))};
        const output = publisher.render(engine, project.source, payload);
        self.postMessage({ok: true, bytes: output.bytes, sourceLength: output.sourceLength,
          filename: output.filename(project.options.title || 'document'),
          diagnostics: output.diagnostics.slice(0, 100).map(item => String(item?.message ?? item).slice(0, 2000)),
          diagnosticCount: output.diagnostics.length}, [output.bytes.buffer]);
      } catch (failure) {
        self.postMessage({ok: false, code: typeof failure?.code === 'string' ? failure.code.slice(0, 80) : 'PROJECT_OPEN_FAILED',
          message: String(failure?.message ?? failure).slice(0, 2048)});
      }
    };
  }

  let worker = null, url = null, timer = null, settled = false, resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  // Suspension can cancel before a host attaches its rejection handler.
  promise.catch(() => {});
  function cleanup() {
    if (timer !== null) env.clearTimeout(timer); timer = null;
    if (url !== null) env.URL.revokeObjectURL(url); url = null;
    if (worker) {
      worker.onmessage = worker.onerror = worker.onmessageerror = null;
      try {
        const stopped = worker.terminate();
        if (stopped && typeof stopped.catch === 'function') stopped.catch(() => {});
      } catch { /* Retired work cannot publish, even if termination throws. */ }
    }
  }
  function finish(failure, value) {
    if (settled) return false;
    settled = true; cleanup();
    if (failure) reject(failure); else resolve(value);
    return true;
  }
  try {
    const functions = '{' + Object.entries(parts).map(([name, value]) => JSON.stringify(name) + ':'
      + Function.prototype.toString.call(value)).join(',') + '}';
    const code = ';(' + run.toString() + ')(' + createPublisher.toString() + ',' + functions + ');';
    url = env.URL.createObjectURL(new Blob([code], {type: 'text/javascript'}));
    worker = new env.Worker(url);
    env.URL.revokeObjectURL(url); url = null;
    worker.onmessage = event => {
      if (settled) return;
      try {
        const value = event.data;
        if (!value || typeof value.ok !== 'boolean') throw Error('Invalid project-opening response');
        if (!value.ok) throw error(typeof value.code === 'string' && /^[A-Z_]{1,80}$/.test(value.code) ? value.code : 'PROJECT_OPEN_FAILED',
          typeof value.message === 'string' ? value.message.slice(0, 2048) : 'Project opening failed');
        if (!(value.bytes instanceof Uint8Array) || !value.bytes.length || value.bytes.length > maximum
            || !Number.isSafeInteger(value.sourceLength) || value.sourceLength < 0 || value.sourceLength > 32 * 1024 * 1024
            || typeof value.filename !== 'string' || value.filename.length > 200 || !value.filename.endsWith('.html')
            || /[<>:"/\\|?*\u0000-\u001f\u007f]/.test(value.filename)
            || !Array.isArray(value.diagnostics) || value.diagnostics.length > 100
            || value.diagnostics.some(item => typeof item !== 'string' || item.length > 2000)
            || !Number.isSafeInteger(value.diagnosticCount) || value.diagnosticCount < value.diagnostics.length)
          throw Error('Invalid opened workspace output');
        finish(null, Object.freeze(value));
      } catch (failure) { finish(failure); }
    };
    worker.onerror = event => { event.preventDefault?.(); finish(error('PROJECT_OPEN_FAILED', 'Project worker failed; retry explicitly')); };
    worker.onmessageerror = () => finish(error('PROJECT_OPEN_FAILED', 'Project response could not be decoded'));
    timer = env.setTimeout(() => finish(error('PROJECT_OPEN_TIMEOUT', 'Project opening timed out; retry explicitly')), 60000);
    worker.postMessage({file: request.file, runtime: request.runtime});
  } catch (failure) { finish(failure); }
  return Object.freeze({promise, cancel() {
    return finish(error('PROJECT_OPEN_CANCELLED', 'Project opening cancelled; current document unchanged'));
  }});
}
