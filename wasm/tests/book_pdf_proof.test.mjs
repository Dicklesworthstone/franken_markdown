import assert from 'node:assert/strict';
import test from 'node:test';
import { createBookPdfProof } from '../book_pdf_proof.mjs';
import { pdfResult, proofHost } from './book_pdf_proof_helpers.mjs';
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
function fixture(options = {}) {
  const host = proofHost();
  return { ...host, host, session: createBookPdfProof({ ...host, ...options }) };
}
async function finish(f, result = pdfResult()) {
  const promise = f.session.render();
  f.jobs.at(-1).resolve(result);
  return promise;
}

test('captures complete ordered source, includes, settings, image views and supplied font roles', async () => {
  const f = fixture();
  const proof = await finish(f);
  assert.deepEqual(f.jobs[0].files, f.model.files);
  assert.deepEqual(f.jobs[0].options, f.model.options);
  assert.equal(f.jobs[0].format, 'pdf');
  assert.equal(proof.chapters, 2);
  assert.equal(proof.filename, 'A-proof-for-a-book.pdf');
  assert.equal(proof.revision, 0);
  assert.equal(proof.blob.type, 'application/pdf');
  assert(Object.isFrozen(proof));
  assert.equal(proof.sourceLength, 29);
  assert(!Object.hasOwn(proof, 'files') && !Object.hasOwn(proof, 'options'));
  assert.notEqual(f.jobs[0].options.images[0].bytes, f.model.options.images[0].bytes);
});

test('copies only the returned byte view and reuses one immutable PDF without a second render', async () => {
  const f = fixture();
  const wanted = pdfResult().bytes;
  const buffer = new Uint8Array(wanted.length + 10).fill(255);
  buffer.set(wanted, 3);
  const output = pdfResult(buffer.subarray(3, 3 + wanted.length));
  output.blob = () => { throw Error('must not use a mutable result closure'); };
  const proof = await finish(f, output);
  buffer.fill(0);
  assert.deepEqual(new Uint8Array(await proof.blob.arrayBuffer()), wanted);
  assert.equal(proof.size, wanted.length);
  assert.equal(f.session.current, proof);
  assert.equal(await f.session.render(), proof);
  assert.equal(await f.session.render(), proof);
  assert.equal(f.jobs.length, 1);
});

test('captures unflushed text before starting, including synchronous collection notifications', async () => {
  const f = fixture();
  f.host.rawEdit('\ufeff# Edited\r\n\r\nnew');
  const proof = await finish(f);
  assert.equal(f.jobs[0].files[0].source, '\ufeff# Edited\r\n\r\nnew');
  assert.equal(proof.revision, 1);
  assert.equal(f.session.current, proof);
});

test('reserves admission before editor capture and refuses a reentrant or parallel proof', async () => {
  const f = fixture();
  const capture = f.controls.captureProject;
  let reentry;
  f.controls.captureProject = () => { reentry = f.session.render(); return capture(); };
  const pending = f.session.render();
  await assert.rejects(reentry, { code: 'BOOK_BUSY' });
  await assert.rejects(f.session.render(), { code: 'BOOK_BUSY' });
  assert.equal(f.jobs.length, 1);
  f.jobs[0].resolve(pdfResult());
  await pending;
});

test('ordinary source/settings/order/image/font/include changes retire pending and retained proofs', async () => {
  for (const mutate of [
    state => state.files.reverse(),
    state => state.options.title = 'new',
    state => state.options.fontScale = 2,
    state => state.options.images = [],
    state => state.options.fontAssets = [],
    state => state.options.includeSources[0].source = 'changed include',
  ]) {
    const f = fixture();
    const pending = f.session.render();
    f.change(mutate);
    await assert.rejects(pending, { code: 'STALE_SOURCE' });
    assert(f.jobs[0].signal.aborted);
    f.jobs[0].resolve(pdfResult());
    await tick();
    assert.equal(f.session.current, null);
    await finish(f);
    f.change(mutate);
    assert.equal(f.session.current, null);
  }
});

test('silent DOM changes and source selection are checked before retention and activation', async () => {
  for (const change of [f => f.host.rawEdit('silent'), f => f.host.select(1)]) {
    const f = fixture();
    const pending = f.session.render();
    change(f);
    f.jobs[0].resolve(pdfResult());
    await assert.rejects(pending, { code: 'STALE_SOURCE' });
    assert.equal(f.session.current, null);
    await finish(f);
    f.host.rawEdit('silent again');
    assert.equal(f.session.current, null);
  }
});

test('snapshot-time changes and explicit capture cancellation never send stale input to a worker', async () => {
  for (const action of ['snapshot', 'capture']) {
    const f = fixture();
    if (action === 'snapshot') {
      const snapshot = f.collection.snapshot;
      f.collection.snapshot = () => { const value = snapshot(); f.change(); return value; };
    } else f.controls.captureProject = () => f.session.cancel();
    await assert.rejects(f.session.render(), error => ['STALE_SOURCE', 'EXPORT_CANCELLED'].includes(error.code));
    assert.equal(f.jobs.length, 0);
  }
});

test('cancellation settles promptly even if the endpoint ignores abort; late results cannot cancel a newer job', async () => {
  const f = fixture();
  const old = f.session.render();
  f.session.cancel();
  await assert.rejects(old, { code: 'EXPORT_CANCELLED' });
  assert.equal(f.session.pending, false);
  const next = f.session.render();
  f.jobs[0].reject(Error('late worker failure'));
  await tick();
  assert.equal(f.jobs[1].signal.aborted, false);
  assert.equal(f.session.pending, true);
  f.jobs[1].resolve(pdfResult());
  assert.equal(f.session.current, null);
  const proof = await next;
  assert.equal(f.session.current, proof);
});

test('external abort is operation-scoped, pre-abort starts no work and listeners are removed', async () => {
  const f = fixture(), early = new AbortController();
  early.abort();
  await assert.rejects(f.session.render({ signal: early.signal }), { code: 'EXPORT_CANCELLED' });
  assert.equal(f.jobs.length, 0);
  const abort = new AbortController();
  let adds = 0, removes = 0;
  const signal = { get aborted() { return abort.signal.aborted; },
    addEventListener(...args) { adds++; abort.signal.addEventListener(...args); },
    removeEventListener(...args) { removes++; abort.signal.removeEventListener(...args); } };
  const old = f.session.render({ signal });
  abort.abort();
  await assert.rejects(old, { code: 'EXPORT_CANCELLED' });
  assert(f.jobs[0].signal.aborted);
  assert.equal(adds, removes);
  const proof = await finish(f);
  abort.abort();
  assert.equal(f.session.current, proof);
  await assert.rejects(f.session.render({ signal: {} }), { code: 'INVALID_OPTIONS' });
});

test('import/composition, suspension and disposal block admission, clear bytes and never rebuild automatically', async () => {
  const f = fixture();
  f.host.busy(true);
  await assert.rejects(f.session.render(), { code: 'BOOK_BUSY' });
  f.host.busy(false);
  const pending = f.session.render();
  f.host.busy(true);
  await assert.rejects(pending, { code: 'STALE_SOURCE' });
  f.host.busy(false);
  assert.equal(f.jobs.length, 1);
  await finish(f);
  f.session.suspend();
  assert.equal(f.session.current, null);
  await assert.rejects(f.session.render(), { code: 'PDF_PROOF_CLOSED' });
  f.session.resume();
  assert.equal(f.jobs.length, 2);
  const final = f.session.render();
  f.session.dispose();
  await assert.rejects(final, { code: 'PDF_PROOF_CLOSED' });
  f.session.dispose();
  assert.equal(f.host.disposed, 1);
  assert.equal(f.host.listenerCount, 0);
  await assert.rejects(f.session.render(), { code: 'PDF_PROOF_CLOSED' });
});

test('renderer errors including falsy rejections free the slot and permit explicit retry', async () => {
  for (const reason of [Error('failed'), undefined, null, false, '']) {
    const f = fixture();
    const pending = f.session.render();
    f.jobs[0].reject(reason);
    const observed = await pending.then(() => ({ resolved: true }), error => ({ error }));
    assert.deepEqual(observed, { error: reason });
    assert.equal(f.session.pending, false);
    assert.equal(f.session.current, null);
    await finish(f);
  }
  const f = fixture();
  f.worker.render = () => { throw Object.assign(Error('worker busy'), { code: 'BOOK_BUSY' }); };
  await assert.rejects(f.session.render(), { code: 'BOOK_BUSY' });
  assert.equal(f.session.pending, false);
});

test('invalid and over-budget PDF results never become current', async () => {
  const cases = [
    output => output.format = 'book-site', output => output.mimeType = 'text/html',
    output => output.extension = 'html', output => output.bytes = new Uint8Array(),
    output => output.bytes = new TextEncoder().encode('<html>oops'),
    output => output.bytes = new TextEncoder().encode('%PDF-x.y'),
    output => output.bytes = new DataView(new ArrayBuffer(12)),
    output => output.bytes = new Uint8Array(new SharedArrayBuffer(12)),
    output => output.sourceLength = -1, output => output.sourceLength = NaN,
    output => output.sourceLength = 64 * 1024 * 1024 + 1,
  ];
  for (const mutate of cases) {
    const f = fixture(), output = pdfResult(); mutate(output);
    await assert.rejects(finish(f, output), { code: 'INVALID_BOOK_PDF' });
    assert.equal(f.session.current, null);
  }
  const f = fixture({ maxOutputBytes: 8 });
  await assert.rejects(finish(f), { code: 'INVALID_BOOK_PDF' });
  assert.throws(() => fixture({ maxOutputBytes: 129 * 1024 * 1024 }), TypeError);
});

test('empty books and invalid capture leave the original source intact', async () => {
  const f = fixture();
  f.model.files = [];
  await assert.rejects(f.session.render(), { code: 'EMPTY_BOOK' });
  assert.equal(f.jobs.length, 0);
  assert.equal(f.session.pending, false);
  const g = fixture(), before = structuredClone(g.model);
  g.controls.captureProject = () => { throw Object.assign(Error('invalid typed path'), { code: 'INVALID_PATH' }); };
  await assert.rejects(g.session.render(), { code: 'INVALID_PATH' });
  assert.deepEqual(g.model, before);
});

test('observers cannot prevent settlement and reentrant invalidation cannot publish a retired proof', async () => {
  const f = fixture();
  const remove = f.session.subscribe(() => { throw Error('observer'); });
  await finish(f);
  remove();
  f.session.invalidate();
  f.session.subscribe(() => { if (f.session.pending) f.session.cancel(); });
  await assert.rejects(f.session.render(), { code: 'EXPORT_CANCELLED' });
  assert.equal(f.jobs.length, 1);
  assert.equal(f.session.current, null);
});
