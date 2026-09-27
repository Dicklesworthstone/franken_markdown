// Explicit source/editor and deferred-worker doubles. These tests execute the
// production proof session/controller; they do not simulate PDF typesetting.
export function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
export function pdfResult(bytes = new TextEncoder().encode('%PDF-1.7\nretained test bytes\n%%EOF\n')) {
  return { format: 'book-pdf', mimeType: 'application/pdf', extension: 'pdf', bytes, sourceLength: 29 };
}
export function proofHost() {
  const changes = new Set(), sourceChanges = new Set();
  const model = {
    files: [{ path: 'second.md', source: '\ufeff# Second\r\n' }, { path: 'first.md', source: '# First\r' }],
    options: {
      title: 'A proof / for a book', font: 'serif', fontScale: 1.25, toc: true, pageNumbers: true,
      images: [{ destination: 'image.png', bytes: new Uint8Array([1, 2, 3]) }],
      fontAssets: [{ slot: 'body-regular', bytes: new Uint8Array([4, 5, 6]), weight: 450 }],
      includeSources: [{ path: 'parts/note.md', source: 'included\r\n' }],
    },
  };
  let revision = 0, busy = false, view = model.files[0].source, active = 0, disposed = 0;
  const change = action => { action?.(model); revision++; for (const listener of changes) listener(); };
  const collection = {
    get revision() { return revision; },
    get files() { return structuredClone(model.files); },
    snapshot() {
      if (!model.files.length) throw Object.assign(Error('Add a chapter first'), { code: 'EMPTY_BOOK' });
      return structuredClone(model);
    },
    subscribe(listener) { changes.add(listener); return () => changes.delete(listener); },
  };
  const controls = {
    get sourceBusy() { return busy; },
    checkpoint() { return JSON.stringify([revision, active, view]); },
    captureProject() {
      if (model.files.length && model.files[active].source !== view)
        change(state => { state.files[active].source = view; });
      return structuredClone(model);
    },
    subscribeSourceState(listener) { sourceChanges.add(listener); return () => sourceChanges.delete(listener); },
  };
  const jobs = [];
  const worker = {
    render(files, format, options, { signal }) {
      const job = { ...deferred(), files, format, options, signal };
      jobs.push(job);
      return job.promise;
    },
    dispose() { disposed++; },
  };
  return { collection, controls, worker, jobs, model, change,
    get disposed() { return disposed; },
    get listenerCount() { return changes.size + sourceChanges.size; },
    rawEdit(source) { view = source; },
    edit(source) { view = source; change(state => { state.files[active].source = source; }); },
    select(index) { active = index; view = model.files[index].source; },
    busy(value) { busy = value; for (const listener of sourceChanges) listener(); },
  };
}
