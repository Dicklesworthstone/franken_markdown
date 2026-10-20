// Data-only worker shared by automatic snapshots and recovery downloads. This
// factory and the existing codec are serialized, not any document-supplied code.
export function createNativeRecoveryEncoder(createCodec, environment = globalThis) {
  const maximum = 256 * 1024 * 1024;
  function workerMain(createCodec) {
    const codec = createCodec();
    self.onmessage = async event => {
      self.onmessage = null;
      try {
        const request = event.data;
        let data;
        if (request.kind === 'encode') {
          const saved = JSON.parse(request.runtime);
          if (saved.version !== 1) throw Error('Unsupported workspace version');
          data = {source: request.source, options: saved.options, images: saved.images, fonts: saved.fonts};
        } else if (request.kind === 'validate') {
          if (!(request.blob instanceof Blob) || !request.blob.size || request.blob.size > codec.limits.outputBytes) throw Error('Invalid recovery project size');
          const saved = codec.decode(await request.blob.arrayBuffer());
          data = {source: saved.source, options: saved.options, images: saved.images, fonts: saved.fonts};
        } else throw Error('Unsupported recovery operation');
        const output = codec.encode(data);
        const title = Array.from(data.options.title || 'Untitled document').slice(0, 120).join('');
        self.postMessage({ok: true, bytes: output.bytes, title, filename: output.filename(data.options.title || 'document')}, [output.bytes.buffer]);
      } catch (error) {
        self.postMessage({ok: false, message: String(error?.message || 'Project validation failed').slice(0, 512)});
      }
    };
  }
  function begin(request) {
    let worker, timer, url, settled = false, rejectJob;
    function retire() {
      if (timer !== undefined) environment.clearTimeout(timer);
      if (url !== undefined) { environment.URL.revokeObjectURL(url); url = undefined; }
      if (worker) {
        worker.onmessage = worker.onerror = worker.onmessageerror = null;
        try { const result = worker.terminate(); result?.catch?.(() => {}); } catch { /* Retired. */ }
      }
    }
    const promise = new Promise((resolve, reject) => {
      rejectJob = error => { if (settled) return; settled = true; retire(); reject(error); };
      try {
        if (request.kind === 'encode' && (typeof request.source !== 'string' || request.source.length > 32 * 1024 * 1024
            || typeof request.runtime !== 'string' || request.runtime.length > maximum)) throw Error('Source or workspace exceeds recovery limits');
        if (request.kind === 'validate' && (!(request.blob instanceof Blob) || !request.blob.size || request.blob.size > maximum)) throw Error('Invalid recovery project size');
        if (typeof environment.Worker !== 'function') throw Error('Local recovery requires a data-encoding Worker');
        url = environment.URL.createObjectURL(new Blob(['(' + workerMain.toString() + ')(' + createCodec.toString() + ');'], {type: 'text/javascript'}));
        worker = new environment.Worker(url);
        environment.URL.revokeObjectURL(url); url = undefined;
        worker.onmessage = event => {
          if (settled) return;
          try {
            const value = event.data;
            if (!value || value.ok !== true) throw Error(String(value?.message || 'Recovery worker failed').slice(0, 512));
            if (!(value.bytes instanceof Uint8Array) || !value.bytes.length || value.bytes.length > maximum
                || typeof value.title !== 'string' || value.title.length > 512
                || typeof value.filename !== 'string' || value.filename.length > 200
                || !value.filename.endsWith('.fmd.json') || /[<>:"/\\|?*\u0000-\u001f\u007f]/.test(value.filename)) throw Error('Invalid recovery worker output');
            const result = Object.freeze({blob: new Blob([value.bytes], {type: 'application/json'}), title: value.title, filename: value.filename});
            settled = true; retire(); resolve(result);
          } catch (error) { rejectJob(error); }
        };
        worker.onerror = event => { event.preventDefault?.(); rejectJob(Error('Recovery worker failed; source is unchanged')); };
        worker.onmessageerror = () => rejectJob(Error('Recovery worker response could not be decoded'));
        timer = environment.setTimeout(() => rejectJob(Error('Recovery encoding timed out; keep a downloaded backup')), 30000);
        worker.postMessage(request);
      } catch (error) { rejectJob(error); }
    });
    return Object.freeze({promise, cancel() {
      rejectJob(Object.assign(Error('Recovery encoding cancelled'), {code: 'RECOVERY_CANCELLED'}));
    }});
  }
  return Object.freeze({encode: (source, runtime) => begin({kind: 'encode', source, runtime}),
    validate: blob => begin({kind: 'validate', blob})});
}
