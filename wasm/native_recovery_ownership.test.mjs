// Encoder and store are explicit host doubles. These tests exercise session
// ownership and notification ordering, not codecs or browser storage.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeRecoverySession} from './native_recovery_session.mjs';

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
async function until(predicate) {
  for (let step = 0; step < 32 && !predicate(); step++) await Promise.resolve();
  assert.ok(predicate(), 'expected recovery transition did not settle');
}
function harness() {
  const h = {source: '# Current\r\n', runtime: 'owned settings', ready: true,
    calls: [], writes: [], timers: new Map(), next: 1, observe: () => {}};
  h.session = createNativeRecoverySession({
    capture: () => ({source: h.source, runtime: h.runtime}), ready: () => h.ready,
    randomId: () => String(h.next++).padStart(32, '0'),
    notify: state => h.observe(state),
    schedule: callback => { const id = h.next++; h.timers.set(id, callback); return id; },
    unschedule: id => h.timers.delete(id),
    encoder: {encode(source, runtime) {
      const job = deferred();
      job.cancel = () => job.reject(Object.assign(Error('cancelled'), {code: 'RECOVERY_CANCELLED'}));
      h.calls.push({source, runtime, job}); return job;
    }},
    store: {write(value) {
      const job = deferred(); h.writes.push({value, job}); return job.promise;
    }},
  });
  h.encode = () => h.calls.at(-1).job.resolve({blob: Object.freeze({encoded: true}),
    title: 'Current', filename: 'Current.fmd.json'});
  h.commit = () => {
    const {value, job} = h.writes.at(-1);
    job.resolve({id: value.id, revision: value.expectedRevision + 1, updatedAt: 123});
  };
  return h;
}

for (const phase of ['idle', 'encoding', 'writing']) {
  test('observer exceptions cannot interrupt recovery in phase ' + phase, async () => {
    const h = harness();
    h.observe = state => { if (state.phase === phase) throw Error('UI observer failed'); };
    const operation = h.session.enable();
    await until(() => h.calls.length === 1);
    h.encode(); await until(() => h.writes.length === 1);
    h.commit();
    assert.equal(await operation, true);
    assert.equal(h.session.state.busy, false);
    assert.equal(h.session.state.dirty, false);
    assert.equal(h.session.state.revision, 1);
    assert.equal(h.session.state.savedAt, 123);
    assert.equal(h.timers.size, 0);
  });
}

test('an encoding observer joins the initialized shared recovery promise', async () => {
  const h = harness(); let nested;
  h.observe = state => { if (state.phase === 'encoding') nested = h.session.flush(); };
  const operation = h.session.enable();
  assert.equal(nested, operation);
  await until(() => h.calls.length === 1);
  h.encode(); await until(() => h.writes.length === 1); h.commit();
  assert.equal(await nested, true); assert.equal(await operation, true);
  assert.equal(h.calls.length, 1); assert.equal(h.writes.length, 1);
});

for (const action of ['disable', 'suspend', 'dispose', 'forget']) {
  test('revocation during the writing notification prevents an undispatched save: ' + action, async () => {
    const h = harness(); let revoked = false;
    h.observe = state => {
      if (state.phase !== 'writing' || revoked) return;
      revoked = true;
      if (action === 'forget') h.session.forget(state.id);
      else h.session[action]();
    };
    const operation = h.session.enable();
    await until(() => h.calls.length === 1); h.encode();
    await until(() => revoked);
    assert.equal(h.writes.length, 0);
    assert.equal(await operation, false);
    assert.equal(h.session.state.enabled, false);
    assert.equal(h.session.state.busy, false);
    assert.equal(h.session.state.savedAt, null);
    assert.equal(h.timers.size, 0);
    if (action === 'forget') assert.equal(h.session.state.id, null);
  });
}

for (const key of ['source', 'runtime']) {
  test('a notification edit discards the old encoding and schedules current ' + key, async () => {
    const h = harness(); let changed = false;
    h.observe = state => {
      if (state.phase !== 'writing' || changed) return;
      changed = true; h[key] += ' newest';
      h.session.changed();
    };
    const operation = h.session.enable();
    await until(() => h.calls.length === 1); h.encode();
    await until(() => changed);
    assert.equal(h.writes.length, 0);
    assert.equal(await operation, false);
    assert.equal(h.session.state.dirty, true);
    assert.equal(h.session.state.enabled, true); assert.equal(h.timers.size, 1);
    const [id, callback] = h.timers.entries().next().value;
    h.timers.delete(id); callback();
    await until(() => h.calls.length === 2);
    assert.equal(h.calls[1][key], h[key]);
    h.encode(); await until(() => h.writes.length === 1); h.commit();
    await until(() => !h.session.state.busy);
    assert.equal(h.session.state.dirty, false);
  });
}

test('programmatic mutation and composition during notification cannot grant stale storage authority', async () => {
  for (const action of ['source', 'runtime', 'composition']) {
    const h = harness(); let changed = false;
    h.observe = state => {
      if (state.phase !== 'writing' || changed) return;
      changed = true;
      if (action === 'composition') h.ready = false;
      else h[action] += ' without an event';
    };
    const operation = h.session.enable();
    await until(() => h.calls.length === 1); h.encode();
    await until(() => changed);
    assert.equal(h.writes.length, 0);
    assert.equal(await operation, false);
    assert.equal(h.session.state.dirty, true); assert.equal(h.session.state.busy, false);
    assert.equal(h.timers.size, action === 'composition' ? 0 : 1);
    h.session.disable();
  }
});

test('throwing completion observers do not suppress a newer revision or its CAS receipt', async () => {
  const h = harness();
  h.observe = state => { if (!state.busy && state.savedAt !== null) throw Error('Status panel unavailable'); };
  const first = h.session.enable();
  await until(() => h.calls.length === 1); h.encode();
  await until(() => h.writes.length === 1);
  h.source = '# Newer source'; h.session.changed(); h.commit();
  assert.equal(await first, true);
  assert.equal(h.session.state.revision, 1); assert.equal(h.session.state.dirty, true);
  assert.equal(h.timers.size, 1);
  const next = h.session.flush();
  await until(() => h.calls.length === 2);
  assert.equal(h.calls[1].source, h.source);
  h.encode(); await until(() => h.writes.length === 2);
  assert.equal(h.writes[1].value.id, h.writes[0].value.id);
  assert.equal(h.writes[1].value.expectedRevision, 1);
  h.commit(); assert.equal(await next, true);
  assert.equal(h.session.state.revision, 2); assert.equal(h.session.state.dirty, false);
});

for (const callback of ['observer', 'cancel']) {
  test('dispose is terminal before reentrant ' + callback + ' callbacks can re-enable recovery', async () => {
    const h = harness();
    const operation = h.session.enable();
    await until(() => h.calls.length === 1);
    if (callback === 'observer') {
      h.observe = state => { if (!state.enabled) h.session.enable(); };
    } else {
      const cancel = h.calls[0].job.cancel;
      h.calls[0].job.cancel = () => { h.session.enable(); cancel(); };
    }
    h.session.dispose();
    assert.equal(await operation, false);
    assert.equal(h.session.state.enabled, false); assert.equal(h.session.state.busy, false);
    assert.equal(h.calls.length, 1); assert.equal(h.writes.length, 0);
    assert.equal(h.timers.size, 0); assert.equal(await h.session.enable(), false);
    h.session.dispose();
  });
}
