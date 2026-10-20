import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeRecoverySession} from './native_recovery_session.mjs';

function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return {promise, resolve, reject}; }
const tick = () => new Promise(resolve => setImmediate(resolve));
function harness() {
  const h = {source: '\ufeff# Original\r\n', runtime: 'resources', ready: true, calls: [], writes: [], notices: [], timers: new Map(), next: 1};
  const encoder = {encode(source, runtime) {
    const job = deferred(); job.cancel = () => { job.cancelled = true; job.reject(Object.assign(Error('cancelled'), {code: 'RECOVERY_CANCELLED'})); };
    h.calls.push({source, runtime, job}); return job;
  }};
  const store = {write(value) { const result = deferred(); h.writes.push({value, result}); return result.promise; }};
  h.session = createNativeRecoverySession({store, encoder, capture: () => ({source: h.source, runtime: h.runtime}), ready: () => h.ready,
    randomId: () => String(h.next++).padStart(32, '0'), notify: value => h.notices.push(value),
    schedule: fn => { const id = h.next++; h.timers.set(id, fn); return id; }, unschedule: id => h.timers.delete(id)});
  h.encode = async (index = h.calls.length - 1) => { h.calls[index].job.resolve({blob: new Blob(['project']), title: 'Example', filename: 'Example.fmd.json'}); await tick(); };
  h.commit = async (index = h.writes.length - 1) => { const call = h.writes[index]; call.result.resolve({id: call.value.id, revision: call.value.expectedRevision + 1, updatedAt: 123}); await tick(); };
  h.runTimer = async () => { const [id, fn] = h.timers.entries().next().value; h.timers.delete(id); fn(); await tick(); };
  return h;
}

test('opt-in: construction and edits do not capture, encode or write', async () => {
  const h = harness(); h.session.changed(); await h.session.flush();
  assert.equal(h.session.state.enabled, false); assert.equal(h.calls.length, 0); assert.equal(h.writes.length, 0); assert.equal(h.timers.size, 0);
});
test('only transaction completion marks an exact complete snapshot saved', async () => {
  const h = harness(); const operation = h.session.enable(); await tick();
  assert.deepEqual([h.calls[0].source, h.calls[0].runtime], [h.source, h.runtime]);
  assert.equal(h.session.state.phase, 'encoding'); assert.equal(h.session.state.savedAt, null);
  await h.encode(); assert.equal(h.session.state.phase, 'writing'); assert.equal(h.session.state.dirty, true);
  await h.commit(); assert.equal(await operation, true); assert.equal(h.session.state.dirty, false); assert.equal(h.session.state.savedAt, 123);
  assert.equal(h.session.state.revision, 1); assert.equal(h.writes[0].value.expectedRevision, 0);
});
test('edits coalesce, superseded encoders cannot save, and latest revision is captured', async () => {
  const h = harness(); const first = h.session.enable(); await tick();
  h.source = 'new'; h.session.changed(); h.source = 'newest'; h.session.changed();
  assert.equal(await first, false); assert.equal(h.calls[0].job.cancelled, true); assert.equal(h.writes.length, 0);
  assert.equal(h.timers.size, 1); await h.runTimer(); assert.equal(h.calls[1].source, 'newest');
  await h.encode(); await h.commit(); assert.equal(h.session.state.dirty, false);
});
test('resource and settings changes are fenced without any source edit', async () => {
  const h = harness(); const first = h.session.enable(); await tick();
  h.runtime = 'new assets'; h.session.changed(); await first; await h.runTimer();
  assert.equal(h.calls[1].runtime, 'new assets'); await h.encode(); await h.commit();
});
test('programmatic source or runtime changes without events cannot publish an old encoding', async () => {
  for (const key of ['source', 'runtime']) {
    const h = harness(); const first = h.session.enable(); await tick(); h[key] += ' changed';
    await h.encode(); assert.equal(await first, false); assert.equal(h.writes.length, 0); assert.equal(h.timers.size, 1);
    h.session.disable();
  }
});
test('edits during a transaction retain its receipt but schedule the newer revision with CAS', async () => {
  const h = harness(); const first = h.session.enable(); await tick(); await h.encode();
  h.source = 'after transaction started'; h.session.changed(); await h.commit(); await first;
  assert.equal(h.session.state.dirty, true); assert.equal(h.session.state.revision, 1); assert.equal(h.timers.size, 1);
  await h.runTimer(); await h.encode(); assert.equal(h.writes[1].value.expectedRevision, 1);
  assert.equal(h.writes[1].value.id, h.writes[0].value.id); await h.commit(); assert.equal(h.session.state.dirty, false);
});
test('programmatic edits while writing also remain dirty', async () => {
  const h = harness(); h.session.enable(); await tick(); await h.encode(); h.source = 'without event'; await h.commit();
  assert.equal(h.session.state.dirty, true); h.session.disable();
});
test('pause cancels encoding and timers but never deletes a recovery copy', async () => {
  const h = harness(); const first = h.session.enable(); await tick(); h.session.disable();
  assert.equal(await first, false); assert.equal(h.writes.length, 0); assert.equal(h.timers.size, 0);
});
test('pause while writing does not falsely promise that the transaction was cancelled', async () => {
  const h = harness(); const first = h.session.enable(); await tick(); await h.encode(); h.session.disable(); await h.commit();
  assert.equal(await first, true); assert.equal(h.session.state.enabled, false); assert.equal(h.session.state.dirty, true);
  assert.match(h.session.state.message, /last started transaction completed/); assert.equal(h.timers.size, 0);
});
test('quota/conflict failures disarm autosave; explicit retry uses a new identity', async () => {
  for (const code of ['QuotaExceededError', 'RECOVERY_CONFLICT']) {
    const h = harness(); const first = h.session.enable(); await tick(); await h.encode();
    const oldId = h.writes[0].value.id; h.writes[0].result.reject(Object.assign(Error(code), {code})); await first;
    assert.equal(h.session.state.enabled, false); assert.equal(h.session.state.id, null); assert.match(h.session.state.message, /Download/);
    const second = h.session.enable(); await tick(); await h.encode(); assert.notEqual(h.writes[1].value.id, oldId); await h.commit(); await second;
  }
});
test('a failed codec leaves editor source and resources unchanged and performs no storage write', async () => {
  const h = harness(); const source = h.source, runtime = h.runtime;
  const first = h.session.enable(); await tick(); h.calls[0].job.reject(Error('Invalid project')); await first;
  assert.equal(h.source, source); assert.equal(h.runtime, runtime); assert.equal(h.writes.length, 0); assert.equal(h.session.state.enabled, false);
});
test('composition blocks work and ending composition reschedules it', async () => {
  const h = harness(); h.session.enable(); await tick(); h.ready = false; h.session.changed(); await tick();
  assert.equal(h.timers.size, 0); h.ready = true; h.source = '完成'; h.session.changed(); await h.runTimer();
  assert.equal(h.calls[1].source, '完成'); await h.encode(); await h.commit();
});
test('enable is refused while the host is not ready', async () => {
  const h = harness(); h.ready = false; assert.equal(await h.session.enable(), false); assert.equal(h.calls.length, 0);
});
test('suspension turns autosave off and never replays it automatically', async () => {
  const h = harness(); const first = h.session.enable(); await tick(); h.session.suspend(); await first;
  h.session.changed(); await h.session.flush(); assert.equal(h.calls.length, 1); assert.equal(h.session.state.enabled, false);
});
test('detaching a target during an in-flight write never reattaches it', async () => {
  const h = harness(); h.session.enable(); await tick(); await h.encode(); const id = h.session.state.id;
  h.session.forget(id); await h.commit(); assert.equal(h.session.state.id, null); assert.equal(h.session.state.enabled, false);
});
test('duplicate flushes share one job and never queue concurrent writers', async () => {
  const h = harness(); const first = h.session.enable(); assert.equal(h.session.flush(), first); await tick(); await h.encode();
  assert.equal(h.session.flush(), first); assert.equal(h.calls.length, 1); assert.equal(h.writes.length, 1); await h.commit(); await first;
});
test('dispose cancels queued work and ignores late notifications', async () => {
  const h = harness(); const first = h.session.enable(); await tick(); h.session.dispose(); const count = h.notices.length;
  await first; assert.equal(h.notices.length, count); assert.equal(h.timers.size, 0); assert.equal(await h.session.enable(), false);
});
test('independent tabs get independent identities, never overwrite by title or source', async () => {
  const first = harness(), second = harness(); second.next = 99;
  first.session.enable(); second.session.enable(); await tick(); await first.encode(); await second.encode();
  assert.notEqual(first.writes[0].value.id, second.writes[0].value.id); await first.commit(); await second.commit();
});
