// This factory is embedded in standalone HTML. Keep it import-free. Project
// validation belongs to the shared source-project codec, never to IndexedDB.
export function createNativeRecoveryStore({indexedDB = globalThis.indexedDB,
  name = 'fmd-native-recovery-v1', timeoutMs = 10000} = {}) {
  const maximum = 256 * 1024 * 1024, capacity = 8, active = new Set();
  const fail = (code, message) => Object.assign(new Error(message), {code});
  const invalid = message => fail('RECOVERY_INVALID', message);
  function identity(id, revision) {
    if (typeof id !== 'string' || !/^[a-f0-9]{32}$/.test(id)
        || !Number.isSafeInteger(revision) || revision < 0) throw invalid('Invalid recovery identity or revision');
  }
  function metadata(value) {
    if (!value || typeof value !== 'object') throw invalid('Damaged recovery metadata');
    const {id, revision, updatedAt, title, filename, size} = value;
    identity(id, revision);
    if (!revision || !Number.isSafeInteger(updatedAt) || updatedAt < 0
        || typeof title !== 'string' || title.length > 512
        || typeof filename !== 'string' || filename.length > 200 || !filename.endsWith('.fmd.json')
        || /[<>:"/\\|?*\u0000-\u001f\u007f]/.test(filename)
        || !Number.isSafeInteger(size) || size < 1 || size > maximum) throw invalid('Damaged recovery metadata');
    return Object.freeze({id, revision, updatedAt, title, filename, size});
  }
  function blobSize(blob) {
    let size;
    try { size = Object.getOwnPropertyDescriptor(Blob.prototype, 'size').get.call(blob); }
    catch { throw invalid('Recovery requires a data-only project Blob'); }
    if (!size || size > maximum) throw invalid('Recovery project exceeds 256 MiB');
    return size;
  }
  function run(mode, work) {
    return new Promise((resolve, reject) => {
      let db, transaction, done = false, output, request;
      const finish = (error, value) => {
        if (done) return;
        done = true; clearTimeout(timer); active.delete(cancel);
        if (error && transaction) { try { transaction.abort(); } catch { /* Already settled. */ } }
        db?.close();
        if (error) reject(error); else resolve(value);
      };
      const cancel = () => finish(fail('RECOVERY_CANCELLED', 'Recovery storage operation cancelled'));
      const timer = setTimeout(() => finish(fail('RECOVERY_TIMEOUT', 'Recovery storage timed out; keep a downloaded backup')), timeoutMs);
      active.add(cancel);
      try {
        if (!indexedDB || typeof indexedDB.open !== 'function') throw fail('RECOVERY_UNAVAILABLE', 'Local recovery storage is unavailable in this browser or origin');
        request = indexedDB.open(name, 1);
        request.onblocked = () => finish(fail('RECOVERY_BLOCKED', 'Another page is blocking recovery storage; close it and retry'));
        request.onerror = () => finish(fail('RECOVERY_STORAGE', request.error?.message || 'Unable to open recovery storage'));
        request.onupgradeneeded = () => {
          if (done) { request.transaction.abort(); return; }
          const database = request.result;
          database.createObjectStore('records', {keyPath: 'id'});
          database.createObjectStore('projects');
        };
        request.onsuccess = () => {
          db = request.result;
          if (done) { db.close(); return; }
          db.onversionchange = cancel;
          try {
            transaction = db.transaction(['records', 'projects'], mode);
            transaction.oncomplete = () => finish(null, output);
            transaction.onabort = () => finish(fail('RECOVERY_STORAGE', transaction.error?.message || 'Recovery transaction aborted'));
            // Request success is NOT a receipt: only oncomplete publishes one.
            work(transaction, value => { output = value; }, error => finish(error));
          } catch (error) { finish(error); }
        };
      } catch (error) { finish(error); }
    });
  }
  function records(values) {
    if (!Array.isArray(values) || values.length > capacity) throw invalid('Recovery catalog exceeds eight entries');
    const result = values.map(metadata);
    if (result.reduce((total, item) => total + item.size, 0) > maximum) throw invalid('Recovery catalog exceeds its 256 MiB budget');
    return result;
  }
  return Object.freeze({
    limits: Object.freeze({entries: capacity, totalBytes: maximum}),
    list() {
      return run('readonly', (tx, publish, reject) => {
        const request = tx.objectStore('records').getAll();
        request.onsuccess = () => {
          try { publish(Object.freeze(records(request.result).sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id)))); }
          catch (error) { reject(error); }
        };
      });
    },
    read(id, revision) {
      identity(id, revision);
      return run('readonly', (tx, publish, reject) => {
        const request = tx.objectStore('records').get(id);
        request.onsuccess = () => {
          try {
            if (!request.result || request.result.revision !== revision) throw fail('RECOVERY_CONFLICT', 'This recovery copy changed or was deleted; refresh the list');
            const record = metadata(request.result), project = tx.objectStore('projects').get(id);
            project.onsuccess = () => {
              try {
                if (blobSize(project.result) !== record.size) throw invalid('Recovery project and metadata do not match');
                publish(Object.freeze({record, blob: project.result}));
              } catch (error) { reject(error); }
            };
          } catch (error) { reject(error); }
        };
      });
    },
    write({id, expectedRevision, title, filename, blob}) {
      identity(id, expectedRevision);
      const record = metadata({id, revision: expectedRevision + 1, updatedAt: Date.now(), title, filename, size: blobSize(blob)});
      return run('readwrite', (tx, publish, reject) => {
        const catalog = tx.objectStore('records'), request = catalog.getAll();
        request.onsuccess = () => {
          try {
            const all = records(request.result), previous = all.find(item => item.id === id);
            if ((previous?.revision ?? 0) !== expectedRevision) throw fail('RECOVERY_CONFLICT', 'Recovery copy changed or was deleted elsewhere; autosave stopped');
            if (!previous && all.length >= capacity) throw fail('RECOVERY_FULL', 'Eight recovery copies are already stored; explicitly delete an old copy before enabling another');
            if (all.reduce((total, item) => total + (item.id === id ? 0 : item.size), record.size) > maximum) {
              throw fail('RECOVERY_FULL', 'Local recovery exceeds 256 MiB; download and explicitly delete an old copy');
            }
            catalog.put(record); tx.objectStore('projects').put(blob, id); publish(record);
          } catch (error) { reject(error); }
        };
      });
    },
    remove(id, revision) {
      identity(id, revision);
      return run('readwrite', (tx, publish, reject) => {
        const catalog = tx.objectStore('records'), request = catalog.get(id);
        request.onsuccess = () => {
          if (!request.result || request.result.revision !== revision) {
            reject(fail('RECOVERY_CONFLICT', 'Recovery copy changed or was deleted; refresh before deleting')); return;
          }
          catalog.delete(id); tx.objectStore('projects').delete(id); publish(true);
        };
      });
    },
    // Does not delete durable copies. Later explicit operations may reopen DB.
    close() { for (const cancel of [...active]) cancel(); },
  });
}
