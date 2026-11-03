// One explicitly requested native PDF, not a print of the HTML preview. The
// snapshot provider belongs to the workspace and must not mutate its state.
// This function is also serialized into standalone files: keep it import-free.
export function createNativePdfProof(runtime, capture) {
  if (typeof runtime?.beginExportDocument !== 'function' || typeof capture !== 'function') {
    throw new TypeError('PDF proofing requires matching native export handles and a snapshot provider');
  }
  let pending = null, retained = null, revision = 0, closed = false;
  const error = (code, message) => Object.assign(new Error(message), {code});
  function snapshot() {
    const value = capture();
    if (!value || typeof value.source !== 'string' || !value.settings || value.resources === undefined) {
      throw error('PROOF_SNAPSHOT', 'Invalid PDF proof snapshot');
    }
    return {source: value.source, settings: value.settings, resources: value.resources};
  }
  function matches(value) {
    if (closed || value.revision !== revision) return false;
    const now = snapshot();
    return value.source === now.source && value.settings === now.settings && value.resources === now.resources;
  }
  function stop(message) {
    const operation = pending;
    if (!operation) return;
    pending = null;
    // The handle may cancel only its own native operation, even if another
    // export/analysis started between native completion and promise delivery.
    try { operation.handle?.cancel(); }
    finally { operation.reject(error('PROOF_CANCELLED', message)); }
  }
  function invalidate(message = 'Document changed; generate a new PDF proof') {
    revision++; retained = null; stop(message);
  }
  function current() {
    if (!retained) return null;
    try { if (matches(retained)) return retained.proof; }
    catch (reason) { invalidate(); throw reason; }
    invalidate(); return null;
  }
  function validate(result) {
    const bytes = result?.bytes;
    if (result?.format !== 'pdf' || result.mimeType !== 'application/pdf'
        || !(bytes instanceof Uint8Array) || !(bytes.buffer instanceof ArrayBuffer)
        || bytes.length < 5 || bytes.length > 256 * 1024 * 1024
        || String.fromCharCode(...bytes.subarray(0, 5)) !== '%PDF-') {
      throw error('PROOF_RESULT', 'Native export returned invalid PDF proof bytes');
    }
    if (!Array.isArray(result.diagnostics) || result.diagnostics.length > 4096) {
      throw error('PROOF_RESULT', 'Native export returned invalid PDF diagnostics');
    }
    let textSize = 0;
    const diagnostics = result.diagnostics.map(finding => {
      if (!finding || typeof finding.message !== 'string') throw error('PROOF_RESULT', 'Invalid PDF diagnostic');
      textSize += finding.message.length;
      if (textSize > 1024 * 1024) throw error('PROOF_RESULT', 'PDF diagnostics exceed their limit');
      // Display diagnostics are plain strings, not live native result objects.
      return finding.message;
    });
    // Blob copies the exact selected view, owns immutable bytes and lets the UI
    // preview and download the same PDF without re-rendering or retaining WASM.
    const blob = new Blob([bytes], {type: 'application/pdf'});
    return Object.freeze({blob, size: blob.size, diagnostics: Object.freeze(diagnostics)});
  }
  return Object.freeze({
    get pending() { return pending !== null; },
    get current() { return current(); },
    render() {
      if (closed) return Promise.reject(error('PROOF_CLOSED', 'PDF proofing is closed'));
      if (pending) return Promise.reject(error('PROOF_BUSY', 'A PDF proof is already rendering'));
      let captured;
      try {
        // Reuse the exact bytes, including the captured metadata timestamp.
        const previous = current();
        if (previous) return Promise.resolve(previous);
        captured = snapshot();
      } catch (reason) { return Promise.reject(reason); }
      return new Promise((resolve, reject) => {
        const operation = {...captured, revision, handle: null, reject};
        pending = operation;
        const owns = () => pending === operation && matches(operation);
        try {
          operation.handle = runtime.beginExportDocument('pdf', captured.source, owns);
          if (!operation.handle || typeof operation.handle.cancel !== 'function'
              || typeof operation.handle.promise?.then !== 'function') {
            throw error('PROOF_RUNTIME', 'Invalid native export handle');
          }
          Promise.resolve(operation.handle.promise).then(result => {
            if (!owns()) throw error('PROOF_CANCELLED', 'Document changed during PDF proofing');
            const proof = validate(result);
            if (!owns()) throw error('PROOF_CANCELLED', 'Document changed during PDF proofing');
            retained = {...captured, revision, proof}; pending = null; resolve(proof);
          }, reason => { throw reason; }).catch(reason => {
            if (pending === operation) { pending = null; reject(reason); }
          });
        } catch (reason) {
          if (pending === operation) { pending = null; operation.handle?.cancel?.(); reject(reason); }
        }
      });
    },
    cancel() { stop('PDF proofing cancelled; document is unchanged'); },
    invalidate,
    dispose() { if (!closed) { invalidate('PDF proofing closed'); closed = true; } },
  });
}
