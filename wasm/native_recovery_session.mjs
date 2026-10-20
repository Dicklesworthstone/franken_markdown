// One opt-in writer. No renderer, browser storage or source reads are touched
// until enable/save. Keep this factory import-free for standalone embedding.
export function createNativeRecoverySession({store, encoder, capture, ready = () => true,
  notify = () => {}, randomId = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, '0')).join(''),
  schedule = setTimeout, unschedule = clearTimeout, delay = 1500}) {
  let enabled = false, disposed = false, dirty = true, epoch = 0, timer = null, pending = null;
  let target = null, savedAt = null, message = 'Local recovery is off. Downloads remain your portable backup.';
  function state() {
    return Object.freeze({enabled, dirty, busy: pending !== null, phase: pending?.phase ?? 'idle',
      id: target?.id ?? null, revision: target?.revision ?? 0, savedAt, message});
  }
  function publish() { if (!disposed) notify(state()); }
  function usable() { try { return !disposed && ready() === true; } catch { return false; } }
  function clearTimer() { if (timer !== null) unschedule(timer); timer = null; }
  function queue() {
    clearTimer();
    if (enabled && dirty && !pending && usable()) timer = schedule(() => { timer = null; flush(); }, delay);
  }
  function same(job) {
    if (!enabled || !usable() || epoch !== job.epoch) return false;
    try { const now = capture(); return now.source === job.snapshot.source && now.runtime === job.snapshot.runtime; }
    catch { return false; }
  }
  function stop(reason) {
    enabled = false; epoch++; clearTimer();
    if (pending?.phase === 'encoding') pending.operation?.cancel();
    message = reason; publish();
  }
  function flush() {
    clearTimer();
    if (pending) return pending.promise;
    if (!enabled || !usable() || !dirty) return Promise.resolve(false);
    const job = {phase: 'encoding', epoch, snapshot: null, operation: null, promise: null};
    pending = job; message = 'Preparing local recovery copy…'; publish();
    job.promise = Promise.resolve().then(() => {
      if (!enabled || !usable() || epoch !== job.epoch) return null;
      if (!target) target = {id: randomId(), revision: 0};
      job.target = target;
      job.snapshot = capture();
      job.operation = encoder.encode(job.snapshot.source, job.snapshot.runtime);
      return job.operation.promise;
    }).then(project => {
      if (!project || !same(job)) return null;
      job.phase = 'writing'; message = 'Writing local recovery copy…'; publish();
      // The store compares revisions inside the same read/write transaction.
      return store.write({...project, id: job.target.id, expectedRevision: job.target.revision});
    }).then(receipt => {
      if (!receipt) return false;
      if (target === job.target) { target = {id: receipt.id, revision: receipt.revision}; savedAt = receipt.updatedAt; }
      dirty = !same(job);
      message = enabled ? (dirty ? 'An earlier revision is stored; current changes are pending.' : 'Current project stored locally. Keep a downloaded backup too.')
        : 'Local recovery is off; the last started transaction completed.';
      return true;
    }).catch(error => {
      if (error?.code === 'RECOVERY_CANCELLED') return false;
      enabled = false; dirty = true; target = null; clearTimer();
      message = 'Local recovery stopped: ' + String(error?.message ?? error).slice(0, 512) + '. Download a source project to keep your work.';
      return false;
    }).finally(() => {
      if (pending === job) pending = null;
      publish(); queue();
    });
    return job.promise;
  }
  return Object.freeze({
    get state() { return state(); },
    enable() {
      if (!usable()) { message = 'Finish text composition or resume the workspace before enabling recovery.'; publish(); return Promise.resolve(false); }
      enabled = true; dirty = true; publish(); return flush();
    },
    disable() { stop('Local recovery is off. Existing copies are retained; an already-started transaction may still complete.'); },
    changed() {
      epoch++; dirty = true; clearTimer();
      if (pending?.phase === 'encoding') pending.operation?.cancel();
      if (enabled) message = 'Changes are not yet stored locally.';
      publish(); queue();
    },
    flush,
    forget(id) {
      if (target?.id === id) { stop('Recovery target detached; explicitly enable to create a new copy.'); target = null; savedAt = null; publish(); }
    },
    suspend() { stop('Workspace suspended; enable local recovery again after returning.'); },
    dispose() { stop('Local recovery is off.'); disposed = true; },
  });
}
