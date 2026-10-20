// Explicit IndexedDB transaction model, NOT a substitute for the native browser
// gate. It exercises the production store's requests, commit receipts and aborts.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeRecoveryStore} from './native_recovery_store.mjs';
const later = fn => setTimeout(fn, 0);

function model() {
  const h = {opens: 0, closed: 0, state: new Map(), queue: [], active: null,
    failPut: false, abortAfterSuccess: false, blocked: false, holdCommit: false};
  function next() {
    if (h.active || !h.queue.length) return;
    const tx = h.queue.shift(); h.active = tx;
    tx.data = new Map([...h.state].map(([name, data]) => [name, new Map(data)]));
    pump(tx);
  }
  function finish(tx, error) {
    if (tx.done) return;
    tx.done = true; tx.error = error || null;
    if (!error && tx.mode === 'readwrite') h.state = tx.data;
    if (h.active === tx) h.active = null;
    else h.queue = h.queue.filter(item => item !== tx);
    later(() => { if (error) tx.onabort?.(); else tx.oncomplete?.(); next(); });
  }
  function pump(tx) {
    if (tx.done || tx !== h.active || tx.pumping) return;
    tx.pumping = true;
    later(() => {
      tx.pumping = false;
      if (tx.done) return;
      const operation = tx.requests.shift();
      if (!operation) { if (!h.holdCommit) finish(tx); return; }
      const {request, action, name, write} = operation;
      try {
        if (write && name === 'projects' && h.failPut) throw Error('QuotaExceededError');
        request.result = action(tx.data.get(name)); request.onsuccess?.();
        if (write && name === 'projects' && h.abortAfterSuccess) tx.abort();
      } catch (error) { request.error = error; request.onerror?.(); finish(tx, error); }
      pump(tx);
    });
  }
  const indexedDB = {open() {
    h.opens++;
    const request = {};
    later(() => {
      let stopped = false;
      const connection = {
        close() { h.closed++; },
        createObjectStore(name) { h.state.set(name, new Map()); },
        transaction(names, mode) {
          const tx = {mode, requests: [], done: false, pumping: false,
            abort() { finish(tx, Error('AbortError')); },
            objectStore(name) {
              assert.ok(names.includes(name));
              function enqueue(action, write = false) {
                const request = {}; tx.requests.push({request, action, name, write}); pump(tx); return request;
              }
              return {getAll: () => enqueue(data => [...data.values()].map(value => structuredClone(value))),
                get: id => enqueue(data => structuredClone(data.get(id))),
                put: (value, key = value.id) => enqueue(data => { data.set(key, structuredClone(value)); return key; }, true),
                delete: id => enqueue(data => data.delete(id), true)};
            }};
          h.queue.push(tx); later(next); return tx;
        }};
      request.result = connection;
      request.transaction = {abort() { stopped = true; }};
      if (h.blocked) { request.onblocked?.(); later(() => request.onsuccess?.()); return; }
      if (!h.state.size) request.onupgradeneeded?.();
      if (stopped) { request.error = Error('AbortError'); request.onerror?.(); }
      else request.onsuccess?.();
    });
    return request;
  }};
  h.store = createNativeRecoveryStore({indexedDB, timeoutMs: 1000});
  h.other = () => createNativeRecoveryStore({indexedDB, timeoutMs: 1000});
  h.release = () => { h.holdCommit = false; if (h.active) pump(h.active); };
  return h;
}
const id = n => n.toString(16).padStart(32, '0');
const project = (n = 1, expectedRevision = 0) => ({id: id(n), expectedRevision, title: 'same title',
  filename: 'document.fmd.json', blob: new Blob(['{"data":"inert"}'], {type: 'application/json'})});
const tick = () => new Promise(resolve => setTimeout(resolve, 25));

test('construction is inert; list explicitly opens and returns metadata only', async () => {
  const h = model(); assert.equal(h.opens, 0); assert.deepEqual(await h.store.list(), []);
  await h.store.write(project()); const rows = await h.store.list(); assert.equal(rows.length, 1); assert.ok(!('blob' in rows[0]));
  assert.equal(rows[0].size, project().blob.size); h.store.close();
});
test('write does not resolve on request success before transaction completion', async () => {
  const h = model(); await h.store.list(); h.holdCommit = true;
  let settled = false; const pending = h.store.write(project()).then(value => { settled = true; return value; });
  await tick(); assert.equal(settled, false); assert.equal(h.state.get('records').size, 0);
  h.release(); const receipt = await pending; assert.equal(receipt.revision, 1); assert.equal(h.state.get('records').size, 1);
});
test('metadata and project bytes commit atomically and retain exact blob contents', async () => {
  const h = model(); const record = await h.store.write(project()); const saved = await h.store.read(record.id, record.revision);
  assert.deepEqual(saved.record, record); assert.equal(await saved.blob.text(), await project().blob.text());
});
test('two independent writers using the same revision have exactly one winner', async () => {
  const h = model(), other = h.other(); await h.store.write(project());
  const results = await Promise.allSettled([h.store.write(project(1, 1)), other.write(project(1, 1))]);
  assert.equal(results.filter(result => result.status === 'fulfilled').length, 1);
  assert.equal(results.find(result => result.status === 'rejected').reason.code, 'RECOVERY_CONFLICT');
  assert.equal((await h.store.list())[0].revision, 2);
});
test('request success followed by abort cannot publish receipt or partial metadata', async () => {
  const h = model(); await h.store.write(project()); h.abortAfterSuccess = true;
  await assert.rejects(h.store.write(project(1, 1)), error => error.code === 'RECOVERY_STORAGE');
  h.abortAfterSuccess = false; assert.equal((await h.store.read(id(1), 1)).record.revision, 1);
});
test('quota error rolls back metadata and preserves the old document', async () => {
  const h = model(); await h.store.write(project()); h.failPut = true;
  await assert.rejects(h.store.write({...project(1, 1), title: 'must not commit'}), /QuotaExceededError/);
  h.failPut = false; const saved = await h.store.read(id(1), 1); assert.equal(saved.record.title, 'same title');
});
test('stale read/delete fail; deletion removes metadata and bytes together', async () => {
  const h = model(); await h.store.write(project()); await h.store.write(project(1, 1));
  await assert.rejects(h.store.read(id(1), 1), error => error.code === 'RECOVERY_CONFLICT');
  await assert.rejects(h.store.remove(id(1), 1), error => error.code === 'RECOVERY_CONFLICT');
  assert.equal(await h.store.remove(id(1), 2), true); assert.deepEqual(await h.store.list(), []);
  assert.equal(h.state.get('projects').size, 0);
});
test('deleted target cannot be resurrected by its old writer', async () => {
  const h = model(); await h.store.write(project()); await h.store.remove(id(1), 1);
  await assert.rejects(h.store.write(project(1, 1)), error => error.code === 'RECOVERY_CONFLICT'); assert.deepEqual(await h.store.list(), []);
});
test('capacity is eight independent identities; no title-based overwrite or eviction', async () => {
  const h = model(); for (let n = 1; n <= 8; n++) await h.store.write(project(n));
  await assert.rejects(h.store.write(project(9)), error => error.code === 'RECOVERY_FULL'); assert.equal((await h.store.list()).length, 8);
  await h.store.remove(id(8), 1); await h.store.write(project(9)); assert.equal((await h.store.list()).length, 8);
});
test('aggregate budget refuses growth and permits an in-budget replacement', async () => {
  const h = model(), unit = new Blob([new Uint8Array(1024 * 1024)]);
  const large = new Blob(Array(200).fill(unit)), excess = new Blob(Array(80).fill(unit));
  await h.store.write({...project(), blob: large});
  await assert.rejects(h.store.write({...project(2), blob: excess}), error => error.code === 'RECOVERY_FULL');
  await h.store.write({...project(1, 1), blob: excess}); await h.store.write({...project(2), blob: excess});
  assert.equal((await h.store.list()).length, 2);
});
test('close cancels pending opening and a held transaction, leaving storage intact', async () => {
  const h = model(); const opening = h.store.list(); h.store.close();
  await assert.rejects(opening, error => error.code === 'RECOVERY_CANCELLED'); await tick();
  await h.store.write(project()); h.holdCommit = true; const write = h.store.write(project(1, 1)); await tick(); h.store.close();
  await assert.rejects(write, error => error.code === 'RECOVERY_CANCELLED'); h.release();
  assert.equal((await h.store.read(id(1), 1)).record.revision, 1);
});
test('blocked open fails explicitly and closes the eventual late connection', async () => {
  const h = model(); h.blocked = true;
  await assert.rejects(h.store.list(), error => error.code === 'RECOVERY_BLOCKED'); await tick(); assert.ok(h.closed > 0);
});
test('unavailable and never-settling storage reject, rather than claiming persistence', async () => {
  await assert.rejects(createNativeRecoveryStore({indexedDB: null}).list(), error => error.code === 'RECOVERY_UNAVAILABLE');
  const store = createNativeRecoveryStore({indexedDB: {open: () => ({})}, timeoutMs: 5});
  await assert.rejects(store.list(), error => error.code === 'RECOVERY_TIMEOUT'); store.close();
});
test('invalid identities, revisions, filenames and blobs fail before opening storage', () => {
  const h = model();
  for (const patch of [{id: 'unsafe'}, {expectedRevision: -1}, {expectedRevision: Number.MAX_SAFE_INTEGER},
    {filename: '../bad.fmd.json'}, {title: 'x'.repeat(513)}, {blob: new Blob()}, {blob: {size: 12}}]) {
    assert.throws(() => h.store.write({...project(), ...patch}));
  }
  assert.equal(h.opens, 0);
});
test('damaged metadata or mismatched blob size cannot be downloaded silently', async () => {
  const h = model(); await h.store.write(project()); h.state.get('projects').set(id(1), new Blob(['short']));
  await assert.rejects(h.store.read(id(1), 1), error => error.code === 'RECOVERY_INVALID');
  h.state.get('records').get(id(1)).filename = '../../payload.html';
  await assert.rejects(h.store.list(), error => error.code === 'RECOVERY_INVALID');
});
