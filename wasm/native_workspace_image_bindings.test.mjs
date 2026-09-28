import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {createNativeWorkspaceRenderer, bootNativeWorkspace} from './interactive_runtime.mjs';

const MiB = 1024 * 1024;
const encode = bytes => Buffer.from(bytes).toString('base64');
const add = (destination = 'figures/new.png', bytes = [8, 9]) => ({destination, bytes: Uint8Array.from(bytes)});
const unpack = images => Array.from(images, image => [image.destination, [...Buffer.from(image.bytes, 'base64')]]);
const bindings = {
  renderHtmlConfiguredAdvanced() { throw Error('Unexpected main-thread rendering'); },
  renderPdfConfiguredMulti() { throw Error('Unexpected main-thread rendering'); },
};
function fixture(make = createNativeWorkspaceRenderer) {
  const payload = {version: 1, options: {title: 'Existing document'},
    images: [{destination: 'old.png', bytes: encode([1, 2, 3])}],
    fonts: [{slot: 'body-regular', bytes: encode([4]), weight: 500}]};
  return {payload, renderer: make(bindings, payload)};
}

test('bindings append exact reviewed destinations and commit one immutable resource revision', () => {
  const {payload, renderer} = fixture(), original = JSON.stringify(payload), inventory = renderer.imageAssets;
  const options = renderer.settings, fonts = renderer.fontSlots;
  const bytes = Uint8Array.of(90, 6, 7, 91);
  const tx = renderer.stageImageBindings([{destination: ' figures/日本語.png ', bytes: new DataView(bytes.buffer, 1, 2)}, add('../shared/logo.jpg')]);
  bytes.fill(0);
  assert.equal(JSON.stringify(payload), original); assert.equal(renderer.imageAssets, inventory);
  assert.deepEqual(unpack(tx.images), [['old.png', [1, 2, 3]], ['figures/日本語.png', [6, 7]], ['../shared/logo.jpg', [8, 9]]]);
  assert.ok(Object.isFrozen(tx.images)); assert.ok(tx.images.every(Object.isFrozen));
  let writes = 0;
  assert.equal(tx.commit(() => { writes++; assert.equal(renderer.imageAssets, inventory); }), true);
  assert.equal(writes, 1); assert.equal(renderer.revision, 1);
  assert.equal(renderer.settings, options); assert.equal(renderer.fontSlots, fonts);
  assert.deepEqual(renderer.imageAssets.map(x => x.bytes), [3, 2, 2]);
  assert.throws(() => tx.commit(), /Stale/);
  tx.rollback(); assert.equal(JSON.stringify(payload), original); assert.equal(renderer.imageAssets, inventory);
  assert.equal(renderer.revision, 2); assert.throws(() => tx.rollback(), /Stale/);
});

test('addition never overwrites; replacement never inserts, including extra public arguments', () => {
  const {renderer, payload} = fixture(), original = JSON.stringify(payload);
  for (const patches of [[add('old.png')], [add(' old.png ')], [add('new.png'), add('new.png')],
    [add('new.png'), add('old.png')], [{destination: 'new.png', remove: true}],
    [{...add(), remove: false}], [{...add(), insert: true}]]) {
    assert.throws(() => renderer.stageImageBindings(patches));
  }
  assert.throws(() => renderer.stageImageChanges([add()], true), /existing destination/);
  assert.equal(JSON.stringify(payload), original); assert.equal(renderer.revision, 0);
});

test('binding records and byte views reject accessors without evaluating them', () => {
  const {renderer} = fixture(); let calls = 0;
  const record = {get destination() { calls++; return 'new.png'; }, bytes: Uint8Array.of(8)};
  const array = []; Object.defineProperty(array, 0, {get() { calls++; return add(); }});
  const inherited = Object.create({destination: 'new.png'}); inherited.bytes = Uint8Array.of(8);
  const iterator = [add()]; iterator[Symbol.iterator] = () => { calls++; throw Error('iterator'); };
  for (const value of [array, [record], [inherited], iterator, [{...add(), [Symbol('x')]: 1}], Array(1)]) {
    assert.throws(() => renderer.stageImageBindings(value));
  }
  assert.equal(calls, 0);
  const bytes = Uint8Array.of(8, 9);
  for (const key of ['buffer', 'byteOffset', 'byteLength']) Object.defineProperty(bytes, key, {get() { calls++; throw Error('shadow'); }});
  renderer.stageImageBindings([{destination: 'new.png', bytes}]).commit();
  assert.equal(calls, 0); assert.equal(renderer.imageAssets[1].bytes, 2);
});

test('cross-realm, pooled Buffer and typed array views preserve their exact owned ranges', () => {
  for (const bytes of [vm.runInNewContext('Uint8Array.of(6, 7)'), Buffer.from([90, 6, 7, 91]).subarray(1, 3),
    Uint8Array.of(90, 6, 7, 91).subarray(1, 3)]) {
    const {renderer, payload} = fixture();
    const record = Object.assign(Object.create(null), {destination: 'new.png', bytes});
    const tx = renderer.stageImageBindings([record]); bytes.fill(0); tx.commit();
    assert.deepEqual(unpack(payload.images)[1], ['new.png', [6, 7]]);
  }
});

test('empty, spoofed, shared, detached and oversized binding bytes fail before mutation', () => {
  const {renderer, payload} = fixture(), before = JSON.stringify(payload), detached = Uint8Array.of(8);
  structuredClone(detached, {transfer: [detached.buffer]});
  for (const bytes of [new Uint8Array(), [8], 'data:image/png;base64,AA==', {[Symbol.toStringTag]: 'ArrayBuffer'},
    new SharedArrayBuffer(1), new Uint8Array(new SharedArrayBuffer(1)), detached, new Uint8Array(8 * MiB + 1)]) {
    assert.throws(() => renderer.stageImageBindings([{destination: 'new.png', bytes}]));
  }
  const bytes = new Uint8Array(8 * MiB);
  assert.throws(() => renderer.stageImageBindings(['a', 'b', 'c'].map(destination => ({destination, bytes}))), /16 MiB/);
  assert.equal(JSON.stringify(payload), before); assert.equal(renderer.revision, 0);
});

test('destination, batch, count and cumulative name budgets are enforced', () => {
  const {renderer} = fixture();
  for (const destination of ['', ' \t ', 'bad\0name', '\ud800', 'x'.repeat(8193), '界'.repeat(2731)]) {
    assert.throws(() => renderer.stageImageBindings([add(destination)]));
  }
  for (const value of [[], null, {}, Array.from({length: 9}, (_, i) => add(String(i)))]) {
    assert.throws(() => renderer.stageImageBindings(value));
  }
  const full = {version: 1, options: {}, fonts: [], images: Array.from({length: 4096}, (_, i) => ({destination: String(i), bytes: 'AA=='}))};
  const atCount = createNativeWorkspaceRenderer(bindings, full);
  assert.throws(() => atCount.stageImageBindings([add()]), /4096/);
  const largeNames = {version: 1, options: {}, fonts: [], images: Array.from({length: 8}, (_, i) => ({destination: String(i) + 'x'.repeat(8191), bytes: 'AA=='}))};
  const atNames = createNativeWorkspaceRenderer(bindings, largeNames);
  assert.throws(() => atNames.stageImageBindings([add()]), /64 KiB/);
  assert.equal(atCount.revision, 0); assert.equal(atNames.revision, 0);
});

test('combined image/font budget applies to bindings and explicit removal frees capacity', () => {
  const big = Buffer.alloc(32 * MiB).toString('base64');
  const payload = {version: 1, options: {}, images: [{destination: 'large.png', bytes: big}],
    fonts: ['body-regular', 'body-bold', 'body-italic'].map(slot => ({slot, bytes: big}))};
  const renderer = createNativeWorkspaceRenderer(bindings, payload);
  assert.throws(() => renderer.stageImageBindings([add()]), /budget/);
  renderer.stageImageChanges([{destination: 'large.png', remove: true}]).commit();
  renderer.stageImageBindings([add()]).commit();
  assert.deepEqual(unpack(payload.images), [['figures/new.png', [8, 9]]]);
});

test('persistence failure is retryable without a partial append', () => {
  const {renderer, payload} = fixture(), original = JSON.stringify(payload), info = renderer.imageAssets;
  const tx = renderer.stageImageBindings([add()]);
  assert.throws(() => tx.commit(() => { throw Error('write failed'); }), /write failed/);
  assert.equal(JSON.stringify(payload), original); assert.equal(renderer.imageAssets, info); assert.equal(renderer.revision, 0);
  tx.commit(); const committed = payload.images;
  assert.throws(() => tx.rollback(() => { throw Error('rollback failed'); }), /rollback failed/);
  assert.equal(payload.images, committed); tx.rollback(); assert.equal(JSON.stringify(payload), original);
});

for (const kind of ['settings', 'fonts', 'insert', 'replace', 'bind']) {
  test(`${kind} transactions fence pending bindings and late rollback`, () => {
    const other = renderer => kind === 'settings' ? renderer.stageSettings({title: 'changed'})
      : kind === 'fonts' ? renderer.stageFonts([{slot: 'body-regular', weight: 600}])
      : kind === 'insert' ? renderer.stageImages([add('other.png')])
      : kind === 'replace' ? renderer.stageImageChanges([add('old.png')])
      : renderer.stageImageBindings([add('other.png')]);
    const f = fixture(), tx = f.renderer.stageImageBindings([add()]);
    other(f.renderer).commit(); const first = JSON.stringify(f.payload);
    assert.throws(() => tx.commit(), /Stale/); assert.equal(JSON.stringify(f.payload), first);
    const g = fixture(), committed = g.renderer.stageImageBindings([add()]);
    committed.commit(); other(g.renderer).commit(); const second = JSON.stringify(g.payload);
    assert.throws(() => committed.rollback(), /Stale/); assert.equal(JSON.stringify(g.payload), second);
  });
}

test('serialized renderer reopens bindings and forwards exact source/resources to all publication ABIs', () => {
  const make = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')', {TextDecoder, Uint8Array, Uint32Array, Float64Array, ArrayBuffer, DataView, atob, btoa});
  const {renderer, payload} = fixture(make); renderer.stageImageBindings([add()]).commit();
  const calls = [], envelope = (text, mimeType) => ({bytes: new TextEncoder().encode(text), mimeType, diagnosticsJson: () => '[]', free() {}});
  const capture = (offset, text, mime) => (...args) => {
    calls.push([args[0], [...args[offset]], [...args[offset + 1]], [...args[offset + 2]]]);
    return envelope(text, mime);
  };
  const api = {
    renderHtmlConfiguredAdvanced: capture(13, '<html><head></head><body></body></html>', 'text/html'),
    renderPdfConfiguredMulti: capture(8, '%PDF-1.7', 'application/pdf'),
    renderEpubConfiguredAdvanced: capture(9, 'PK', 'application/epub+zip'),
    renderSvgConfiguredResources: capture(5, '<svg/>', 'image/svg+xml'),
  };
  const reopened = createNativeWorkspaceRenderer(api, JSON.parse(JSON.stringify(payload)));
  const source = '\ufeff![existing](figures/new.png)\r\n\r\n![same][ref]\r\n[ref]: figures/new.png\0';
  for (const format of ['html', 'pdf', 'epub', 'svg']) reopened[format](source);
  assert.equal(calls.length, 4);
  for (const call of calls) assert.deepEqual(call, [source, ['old.png', 'figures/new.png'], [1, 2, 3, 8, 9], [3, 2]]);
});

// The production bootstrap with scoped DOM/URL and deferred transport adapters.
// These prove ownership and atomic publication, not compiled WASM image codecs.
async function runtimeFixture(run) {
  const keys = ['window', 'document', 'URL'], original = keys.map(key => Object.getOwnPropertyDescriptor(globalThis, key));
  let saved = JSON.stringify({...fixture().payload, wasm: 'AGFzbQEAAAA=', bindings: 'adapter'}), failData = false, failFrame = false;
  const data = {get textContent() { return saved; }, set textContent(value) {
    if (failData) { failData = false; throw Error('data rejected'); } saved = value;
  }};
  const preview = {childNodes: [{initial: true}], replaceChildren(...nodes) {
    if (failFrame) { failFrame = false; throw Error('frame rejected'); } this.childNodes = nodes;
  }};
  const jobs = [], events = [];
  globalThis.window = new EventTarget();
  window.addEventListener('fmd-native-images-changed', () => events.push('images'));
  globalThis.document = {querySelector: () => data, createElement: () => ({style: {}, attributes: {},
    setAttribute(key, value) { this.attributes[key] = value; }, addEventListener() {}})};
  globalThis.URL = {createObjectURL: () => 'data:text/javascript,export default async()=>{}', revokeObjectURL() {}};
  const createWorker = (_factory, payload) => {
    let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    const job = {payload, resolve, reject, disposed: 0}; jobs.push(job);
    return {render(...args) { job.args = args; return promise; }, exportDocument(...args) { job.args = args; return promise; },
      invalidate() {}, dispose() { job.disposed++; reject(Error('disposed')); }};
  };
  try {
    bootNativeWorkspace((_api, payload) => createNativeWorkspaceRenderer(bindings, payload), createWorker);
    const engine = window.__fmdNativeRuntime; assert.equal(await engine.ready, true);
    await run({engine, data, preview, jobs, events, failData() { failData = true; }, failFrame() { failFrame = true; }});
  } finally { keys.forEach((key, i) => original[i] ? Object.defineProperty(globalThis, key, original[i]) : delete globalThis[key]); }
}
const tick = () => new Promise(resolve => setImmediate(resolve));
const result = {html: '<html><head></head><body>bound</body></html>', diagnostics: []};

for (const mode of ['Bindings', 'Changes']) {
  test(`${mode} use the same atomic, operation-owned preflight path`, () => runtimeFixture(async f => {
    const before = f.data.textContent, frame = f.preview.childNodes[0], options = f.engine.settings;
    const handle = f.engine['beginImage' + mode]([add(mode === 'Bindings' ? 'new.png' : 'old.png')], '\ufeffsource\r\n', f.preview);
    assert.equal(f.engine.imagesPending, true); assert.equal(f.engine.settingsPending, true);
    assert.throws(() => f.engine.exportDocument('pdf', 's'), {code: 'EXPORT_BUSY'});
    assert.throws(() => f.engine.applyFontsAsync([{slot: 'body-regular', weight: 600}], 's', f.preview), {code: 'FONTS_BUSY'});
    await tick(); assert.equal(f.jobs.length, 1); assert.equal(f.data.textContent, before); assert.equal(f.preview.childNodes[0], frame);
    assert.equal(f.jobs[0].args[0], '\ufeffsource\r\n');
    assert.deepEqual(f.jobs[0].payload.images.map(x => x.destination), mode === 'Bindings' ? ['old.png', 'new.png'] : ['old.png']);
    f.jobs[0].resolve(result); assert.equal(await handle.promise, f.engine.imageAssets);
    assert.equal(f.engine.documentRevision, 1); assert.equal(f.engine.settings, options); assert.equal(f.engine.settingsPending, false);
    assert.equal(f.preview.childNodes[0].srcdoc, result.html); assert.equal(f.preview.childNodes[0].attributes.sandbox, 'allow-same-origin');
    assert.equal(f.jobs[0].disposed, 1); assert.deepEqual(f.events, ['images']); assert.equal(handle.cancel(), false);
  }));
}
for (const failure of ['worker', 'data', 'frame']) {
  test(`binding ${failure} failure leaves the whole original document committed`, () => runtimeFixture(async f => {
    const original = f.data.textContent, frame = f.preview.childNodes[0], inventory = f.engine.imageAssets;
    const handle = f.engine.beginImageBindings([add()], 's', f.preview); await tick();
    if (failure === 'worker') f.jobs[0].reject(Error('decode rejected'));
    else { if (failure === 'data') f.failData(); else f.failFrame(); f.jobs[0].resolve(result); }
    await assert.rejects(handle.promise, /rejected/);
    assert.equal(f.data.textContent, original); assert.equal(f.preview.childNodes[0], frame); assert.equal(f.engine.imageAssets, inventory);
    assert.equal(f.engine.documentRevision, 0); assert.equal(f.engine.settingsPending, false); assert.deepEqual(f.events, []);
  }));
}

test('binding cancellation before startup allocates no worker and can be retried', () => runtimeFixture(async f => {
  const handle = f.engine.beginImageBindings([add()], 's', f.preview);
  assert.equal(handle.cancel(), true); assert.equal(handle.cancel(), false);
  await assert.rejects(handle.promise, {code: 'IMAGES_CANCELLED'}); assert.equal(f.jobs.length, 0);
  const next = f.engine.beginImageBindings([add()], 's', f.preview); await tick(); f.jobs[0].resolve(result); await next.promise;
  assert.equal(f.engine.imageAssets.length, 2);
}));

test('retired binding handles cannot cancel or release newer font operations', () => runtimeFixture(async f => {
  const old = f.engine.beginImageBindings([add()], 's', f.preview); await tick(); old.cancel();
  const next = f.engine.applyFontsAsync([{slot: 'body-regular', weight: 600}], 's', f.preview);
  assert.equal(old.cancel(), false); await assert.rejects(old.promise, {code: 'IMAGES_CANCELLED'});
  await tick(); assert.equal(f.engine.fontsPending, true); assert.equal(f.jobs[1].disposed, 0);
  f.jobs[1].resolve(result); await next; assert.equal(f.engine.imageAssets.length, 1);
}));

test('source invalidation, resource changes and page suspension cannot publish stale bindings', () => runtimeFixture(async f => {
  let current = true;
  const old = f.engine.beginImageBindings([add()], 's', f.preview, {scale: 1}, () => current);
  await tick(); current = false; f.jobs[0].resolve(result); await assert.rejects(old.promise, {code: 'IMAGES_CANCELLED'});
  const next = f.engine.beginImageBindings([add()], 's', f.preview); await tick();
  f.engine.stageImages([add('other.png')]).commit(); await assert.rejects(next.promise, {code: 'IMAGES_CANCELLED'});
  const suspended = f.engine.beginImageBindings([add()], 's', f.preview); await tick();
  window.dispatchEvent(new Event('pagehide')); await assert.rejects(suspended.promise, {code: 'IMAGES_CANCELLED'});
  assert.throws(() => f.engine.beginImageBindings([add()], 's', f.preview), {code: 'IMAGES_SUSPENDED'});
  window.dispatchEvent(new Event('pageshow')); assert.deepEqual(f.engine.imageAssets.map(x => x.destination), ['old.png', 'other.png']);
}));
