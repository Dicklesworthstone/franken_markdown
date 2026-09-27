import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

const MiB = 1024 * 1024;
const encoded = value => Buffer.from(value).toString('base64');
const image = (destination, bytes) => ({destination, bytes: encoded(bytes)});
const bindings = {
  renderHtmlConfiguredAdvanced() { throw Error('Unexpected native call in resource transaction'); },
  renderPdfConfiguredMulti() { throw Error('Unexpected native call in resource transaction'); },
};
function fixture(make = createNativeWorkspaceRenderer) {
  const payload = {version: 1, options: {}, images: [image('a.png', [1, 2]), image('b.jpg', [3, 4, 5]), image('c.png', [6])],
    fonts: [{slot: 'body-regular', bytes: encoded([7]), weight: 500}]};
  const renderer = make(bindings, payload);
  return {payload, renderer};
}
const patch = (destination, values) => ({destination, bytes: Uint8Array.from(values)});
const read = entries => Array.from(entries, ({destination, bytes}) => [destination, [...Buffer.from(bytes, 'base64')]]);

test('inventory is stable, immutable and exposes sizes, not resource buffers', () => {
  const {renderer} = fixture();
  assert.equal(renderer.imageAssets, renderer.imageAssets);
  assert.deepEqual(renderer.imageAssets, [{destination: 'a.png', bytes: 2}, {destination: 'b.jpg', bytes: 3}, {destination: 'c.png', bytes: 1}]);
  assert.ok(Object.isFrozen(renderer.imageAssets));
  assert.ok(renderer.imageAssets.every(Object.isFrozen));
  assert.throws(() => { renderer.imageAssets[0].bytes = 100; }, TypeError);
  assert.equal(renderer.revision, 0);
});

test('mixed replacement/removal preserves surviving identities and commits the batch once', () => {
  const {renderer, payload} = fixture(), original = JSON.stringify(payload), options = renderer.settings, fonts = renderer.fontSlots;
  const info = renderer.imageAssets;
  const tx = renderer.stageImageChanges([{destination: ' b.jpg ', remove: true}, patch('a.png', [8, 9, 10])]);
  assert.equal(tx.changed, true); assert.equal(JSON.stringify(payload), original); assert.equal(renderer.imageAssets, info);
  assert.deepEqual(read(tx.images), [['a.png', [8, 9, 10]], ['c.png', [6]]]);
  assert.ok(Object.isFrozen(tx.images)); assert.ok(tx.images.every(Object.isFrozen));
  let publications = 0;
  assert.equal(tx.commit(() => { publications++; assert.equal(renderer.imageAssets, info); }), true);
  assert.equal(publications, 1); assert.equal(renderer.revision, 1);
  assert.deepEqual(read(payload.images), [['a.png', [8, 9, 10]], ['c.png', [6]]]);
  assert.deepEqual(renderer.imageAssets.map(x => x.bytes), [3, 1]);
  assert.equal(renderer.settings, options); assert.equal(renderer.fontSlots, fonts);
  assert.throws(() => tx.commit(), /Stale/);
});

test('exact typed/DataView ranges are owned before returning a staged edit', () => {
  for (const kind of ['typed', 'data', 'buffer']) {
    const {renderer, payload} = fixture(), backing = Uint8Array.from([90, 8, 9, 91]);
    const bytes = kind === 'typed' ? backing.subarray(1, 3) : kind === 'data'
      ? new DataView(backing.buffer, 1, 2) : Buffer.from(backing.buffer, 1, 2);
    const tx = renderer.stageImageChanges([{destination: 'a.png', bytes}]);
    backing.fill(0); tx.commit();
    assert.deepEqual(read(payload.images)[0], ['a.png', [8, 9]], kind);
  }
});

test('shadowed view getters cannot substitute byte ranges or run during admission', () => {
  const {renderer, payload} = fixture(), bytes = Uint8Array.of(8, 9);
  for (const key of ['buffer', 'byteOffset', 'byteLength']) {
    Object.defineProperty(bytes, key, {get() { throw Error('Shadowed getter ran'); }});
  }
  renderer.stageImageChanges([{destination: 'a.png', bytes}]).commit();
  assert.deepEqual(read(payload.images)[0][1], [8, 9]);
});

test('cross-realm views and null-prototype data records retain their exact values', () => {
  const {renderer, payload} = fixture();
  const record = Object.assign(Object.create(null), {destination: 'a.png', bytes: vm.runInNewContext('Uint8Array.of(8, 9)')});
  renderer.stageImageChanges([record]).commit();
  assert.deepEqual(read(payload.images)[0][1], [8, 9]);
});

test('unknown keys, duplicate normalized names and ambiguous edits fail without mutation', () => {
  const {renderer, payload} = fixture(), before = JSON.stringify(payload), info = renderer.imageAssets;
  for (const value of [null, {}, [], Array(1), Array(9), [null], [7],
    [{destination: 'missing.png', remove: true}], [patch('new.png', [8])],
    [{destination: 'a.png'}], [{destination: 'a.png', remove: false}],
    [{destination: 'a.png', remove: true, bytes: Uint8Array.of(8)}],
    [{destination: 'a.png', remove: true, extra: 1}],
    [{destination: 'a.png', remove: true}, patch(' a.png ', [8])],
    [patch('', [8])], [patch('a\0.png', [8])], [patch('\ud800', [8])]]) {
    assert.throws(() => renderer.stageImageChanges(value));
  }
  assert.equal(JSON.stringify(payload), before); assert.equal(renderer.imageAssets, info); assert.equal(renderer.revision, 0);
});

test('accessors, sparse arrays, inherited records and symbols are not evaluated', () => {
  const {renderer} = fixture(); let reads = 0;
  const accessor = {get destination() { reads++; return 'a.png'; }, remove: true};
  const array = []; Object.defineProperty(array, 0, {get() { reads++; return accessor; }});
  const inherited = Object.create({destination: 'a.png'}); inherited.remove = true;
  const symbol = {destination: 'a.png', remove: true, [Symbol('hidden')]: true};
  const iterator = [patch('a.png', [8])]; iterator[Symbol.iterator] = () => { reads++; throw Error('iterator'); };
  for (const value of [[accessor], array, [inherited], [symbol], iterator]) assert.throws(() => renderer.stageImageChanges(value));
  assert.equal(reads, 0);
});

test('empty, shared, spoofed and detached buffers cannot enter the resource store', () => {
  const {renderer} = fixture(), detached = Uint8Array.of(8);
  structuredClone(detached, {transfer: [detached.buffer]});
  for (const bytes of [new Uint8Array(), new SharedArrayBuffer(1), new Uint8Array(new SharedArrayBuffer(1)),
    {[Symbol.toStringTag]: 'ArrayBuffer'}, detached, [8], 'https://example.invalid/image.png']) {
    assert.throws(() => renderer.stageImageChanges([{destination: 'a.png', bytes}]));
  }
});

test('replacement byte limits admit a whole batch before publishing anything', () => {
  const {renderer, payload} = fixture(), before = JSON.stringify(payload);
  assert.throws(() => renderer.stageImageChanges([{destination: 'a.png', bytes: new Uint8Array(8 * MiB + 1)}]), /8 MiB/);
  const large = new Uint8Array(8 * MiB);
  assert.throws(() => renderer.stageImageChanges(['a.png', 'b.jpg', 'c.png'].map(destination => ({destination, bytes: large}))), /16 MiB/);
  assert.equal(JSON.stringify(payload), before); assert.equal(renderer.revision, 0);
});

test('no-op replacement does not change revision, inventory, saved data or invoke publication', () => {
  const {renderer, payload} = fixture(), images = payload.images, info = renderer.imageAssets;
  const tx = renderer.stageImageChanges([patch('a.png', [1, 2])]);
  assert.equal(tx.changed, false);
  assert.equal(tx.commit(() => { throw Error('No-op publisher must not run'); }), false);
  assert.equal(payload.images, images); assert.equal(renderer.imageAssets, info); assert.equal(renderer.revision, 0);
  assert.throws(() => tx.commit(), /Stale/); assert.throws(() => tx.rollback(), /Stale/);
});

test('failed persistence leaves the original buffers and revision intact; retry and rollback are exact', () => {
  const {renderer, payload} = fixture(), original = JSON.stringify(payload), info = renderer.imageAssets;
  const tx = renderer.stageImageChanges([patch('a.png', [9]), {destination: 'b.jpg', remove: true}]);
  assert.throws(() => tx.commit(() => { throw Error('write failure'); }), /write failure/);
  assert.equal(JSON.stringify(payload), original); assert.equal(renderer.imageAssets, info); assert.equal(renderer.revision, 0);
  tx.commit(); const committed = payload.images;
  assert.throws(() => tx.rollback(() => { throw Error('rollback failure'); }), /rollback failure/);
  assert.equal(payload.images, committed); assert.equal(renderer.revision, 1);
  tx.rollback(); assert.equal(JSON.stringify(payload), original); assert.equal(renderer.imageAssets, info); assert.equal(renderer.revision, 2);
  assert.throws(() => tx.rollback(), /Stale/);
});

for (const kind of ['settings', 'fonts', 'insert', 'images']) {
  test(`${kind} commits fence stale image commits and stale rollback`, () => {
    const other = renderer => kind === 'settings' ? renderer.stageSettings({title: 'new'})
      : kind === 'fonts' ? renderer.stageFonts([{slot: 'body-regular', weight: 600}])
      : kind === 'insert' ? renderer.stageImages([patch('new.png', [9])])
      : renderer.stageImageChanges([patch('c.png', [9])]);
    const f = fixture(), tx = f.renderer.stageImageChanges([patch('a.png', [8])]);
    other(f.renderer).commit(); const saved = JSON.stringify(f.payload);
    assert.throws(() => tx.commit(), /Stale/); assert.equal(JSON.stringify(f.payload), saved);
    const g = fixture(), committed = g.renderer.stageImageChanges([patch('a.png', [8])]);
    committed.commit(); other(g.renderer).commit(); const next = JSON.stringify(g.payload);
    assert.throws(() => committed.rollback(), /Stale/); assert.equal(JSON.stringify(g.payload), next);
  });
}

test('removing every binding yields a usable empty inventory and frees names for explicit reinsertion', () => {
  const {renderer, payload} = fixture();
  renderer.stageImageChanges(renderer.imageAssets.map(({destination}) => ({destination, remove: true}))).commit();
  assert.deepEqual(payload.images, []); assert.deepEqual(renderer.imageAssets, []);
  renderer.stageImages([patch('a.png', [9])]).commit();
  assert.deepEqual(read(payload.images), [['a.png', [9]]]);
});

test('final-set budgeting allows replacement at 128 MiB and removal reclaims font capacity', () => {
  const big = Buffer.alloc(32 * MiB).toString('base64');
  const payload = {version: 1, options: {}, images: [{destination: 'big.png', bytes: big}],
    fonts: ['body-regular', 'body-bold', 'body-italic'].map(slot => ({slot, bytes: big}))};
  const renderer = createNativeWorkspaceRenderer(bindings, payload);
  assert.throws(() => renderer.stageImages([patch('extra.png', [1])]), /budget/);
  renderer.stageImageChanges([patch('big.png', [1])]).commit();
  assert.equal(renderer.imageAssets[0].bytes, 1);
  const more = {slot: 'mono-regular', bytes: new Uint8Array(32 * MiB)};
  assert.throws(() => renderer.stageFonts([more]), /budget/);
  renderer.stageImageChanges([{destination: 'big.png', remove: true}]).commit();
  renderer.stageFonts([more]).commit();
  assert.equal(renderer.fontSlots.find(x => x.slot === 'mono-regular').bytes, 32 * MiB);
});

test('serialized factory remains import-free and saved edits reopen identically', () => {
  const make = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')', {TextDecoder, Uint8Array, Uint32Array, Float64Array, ArrayBuffer, DataView, atob, btoa});
  const {renderer, payload} = fixture(make);
  renderer.stageImageChanges([patch('b.jpg', [8]), {destination: 'a.png', remove: true}]).commit();
  const reopened = createNativeWorkspaceRenderer(bindings, JSON.parse(JSON.stringify(payload)));
  assert.equal(JSON.stringify(reopened.imageAssets), JSON.stringify(renderer.imageAssets));
  assert.deepEqual(read(payload.images), [['b.jpg', [8]], ['c.png', [6]]]);
});

test('committed packed image bytes reach all four production publication adapters', () => {
  const calls = [], envelope = (text, mimeType) => ({bytes: new TextEncoder().encode(text), mimeType, diagnosticsJson: () => '[]', free() {}});
  const record = (format, offset, output, mime) => (...args) => {
    calls.push({format, source: args[0], names: [...args[offset]], bytes: [...args[offset + 1]], lengths: [...args[offset + 2]]});
    return envelope(output, mime);
  };
  const api = {
    renderHtmlConfiguredAdvanced: record('html', 13, '<html><head></head><body></body></html>', 'text/html'),
    renderPdfConfiguredMulti: record('pdf', 8, '%PDF-1.7', 'application/pdf'),
    renderEpubConfiguredAdvanced: record('epub', 9, 'PK', 'application/epub+zip'),
    renderSvgConfiguredResources: record('svg', 5, '<svg/>', 'image/svg+xml'),
  };
  const f = fixture(), renderer = createNativeWorkspaceRenderer(api, f.payload), source = '\ufeff![a](a.png)\r\n![c](c.png)\r';
  renderer.stageImageChanges([patch('a.png', [9, 8, 7, 6]), {destination: 'b.jpg', remove: true}]).commit();
  for (const format of ['html', 'pdf', 'epub', 'svg']) renderer[format](source);
  assert.equal(calls.length, 4);
  for (const call of calls) {
    assert.equal(call.source, source); assert.deepEqual(call.names, ['a.png', 'c.png']);
    assert.deepEqual(call.bytes, [9, 8, 7, 6, 6]); assert.deepEqual(call.lengths, [4, 1]);
  }
});

// Execute the production bootstrap. These narrowly scoped DOM/URL and deferred
// transport adapters test transaction ownership, not native image decoding.
async function runtimeFixture(run) {
  const names = ['window', 'document', 'URL'];
  const before = names.map(name => Object.getOwnPropertyDescriptor(globalThis, name));
  const payload = {...fixture().payload, wasm: 'AGFzbQEAAAA=', bindings: 'test-only native ABI adapter'};
  let persisted = JSON.stringify(payload), failData = false, failPreview = false;
  const data = {get textContent() { return persisted; }, set textContent(value) {
    if (failData) { failData = false; throw Error('persistence rejected'); } persisted = value;
  }};
  const preview = {childNodes: [{initial: true}], replaceChildren(...nodes) {
    if (failPreview) { failPreview = false; throw Error('preview rejected'); } this.childNodes = nodes;
  }};
  const workers = [], events = [];
  globalThis.window = new EventTarget();
  window.addEventListener('fmd-native-images-changed', () => events.push('images'));
  globalThis.document = {querySelector: () => data,
    createElement: tag => ({tag, style: {}, attributes: {}, setAttribute(key, value) { this.attributes[key] = value; }, addEventListener() {}})};
  globalThis.URL = {createObjectURL: () => 'data:text/javascript,export default async()=>{}', revokeObjectURL() {}};
  function createWorker(_factory, saved) {
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    const job = {saved, resolve, reject, disposed: 0}; workers.push(job);
    return {render(...args) { job.args = args; return promise; },
      exportDocument(...args) { job.args = args; return promise; },
      invalidate() {}, dispose() { job.disposed++; reject(Error('disposed')); }};
  }
  const {bootNativeWorkspace} = await import('./interactive_runtime.mjs');
  try {
    bootNativeWorkspace((_bindings, saved) => createNativeWorkspaceRenderer(bindings, saved), createWorker);
    const engine = window.__fmdNativeRuntime;
    assert.equal(await engine.ready, true);
    await run({engine, data, preview, workers, events,
      failData() { failData = true; }, failPreview() { failPreview = true; }});
  } finally {
    names.forEach((name, i) => before[i] ? Object.defineProperty(globalThis, name, before[i]) : delete globalThis[name]);
  }
}
const tick = () => new Promise(resolve => setImmediate(resolve));
const renderResult = {html: '<html><head></head><body>changed</body></html>', diagnostics: [{message: 'image change'}]};

test('background image preflight publishes resource JSON and iframe together in the shared slot', () => runtimeFixture(async f => {
  const original = f.data.textContent, oldFrame = f.preview.childNodes[0], options = f.engine.settings;
  const handle = f.engine.beginImageChanges([patch('a.png', [9])], '\ufeffsource\r\n', f.preview, {scale: 1.2});
  assert.ok(Object.isFrozen(handle)); assert.equal(f.engine.imagesPending, true); assert.equal(f.engine.settingsPending, true);
  assert.throws(() => f.engine.exportDocument('pdf', 's'), {code: 'EXPORT_BUSY'});
  assert.throws(() => f.engine.applyFontsAsync([{slot: 'body-regular', weight: 600}], 's', f.preview), {code: 'FONTS_BUSY'});
  await tick(); assert.equal(f.workers.length, 1);
  assert.equal(f.data.textContent, original); assert.equal(f.preview.childNodes[0], oldFrame);
  assert.equal(f.workers[0].args[0], '\ufeffsource\r\n');
  assert.equal(f.workers[0].saved.images[0].bytes, encoded([9]));
  f.workers[0].resolve(renderResult); const inventory = await handle.promise;
  assert.equal(inventory, f.engine.imageAssets); assert.equal(inventory[0].bytes, 1);
  assert.deepEqual(read(JSON.parse(f.data.textContent).images)[0], ['a.png', [9]]);
  assert.equal(f.preview.childNodes[0].srcdoc, renderResult.html);
  assert.equal(f.preview.childNodes[0].attributes.sandbox, 'allow-same-origin');
  assert.equal(f.engine.settings, options); assert.equal(f.engine.documentRevision, 1);
  assert.equal(f.engine.settingsPending, false); assert.equal(f.workers[0].disposed, 1);
  assert.deepEqual(f.events, ['images']); assert.equal(handle.cancel(), false);
}));

test('no-op image changes allocate no worker and leave persistence/preview/revision untouched', () => runtimeFixture(async f => {
  const previous = f.data.textContent, frame = f.preview.childNodes[0], info = f.engine.imageAssets;
  const handle = f.engine.beginImageChanges([patch('a.png', [1, 2])], 's', f.preview);
  assert.equal(await handle.promise, info); assert.equal(f.workers.length, 0);
  assert.equal(f.data.textContent, previous); assert.equal(f.preview.childNodes[0], frame);
  assert.equal(f.engine.documentRevision, 0); assert.deepEqual(f.events, []);
}));

for (const failure of ['worker', 'data', 'preview']) {
  test(`${failure} failure cannot partially publish image changes`, () => runtimeFixture(async f => {
    const previous = f.data.textContent, frame = f.preview.childNodes[0], info = f.engine.imageAssets;
    const handle = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview);
    await tick();
    if (failure === 'worker') f.workers[0].reject(Error('native image decode failed'));
    else { if (failure === 'data') f.failData(); else f.failPreview(); f.workers[0].resolve(renderResult); }
    await assert.rejects(handle.promise, /failed|rejected/);
    assert.equal(f.data.textContent, previous); assert.equal(f.preview.childNodes[0], frame);
    assert.equal(f.engine.imageAssets, info); assert.equal(f.engine.documentRevision, 0);
    assert.equal(f.engine.settingsPending, false); assert.deepEqual(f.events, []);
  }));
}

test('cancel-before-start releases admission and creates no worker', () => runtimeFixture(async f => {
  const previous = f.data.textContent;
  const handle = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview);
  assert.equal(handle.cancel(), true); assert.equal(handle.cancel(), false);
  await assert.rejects(handle.promise, {code: 'IMAGES_CANCELLED'});
  assert.equal(f.workers.length, 0); assert.equal(f.data.textContent, previous);
}));

test('late cancelled image completion cannot cancel or release a newer settings preflight', () => runtimeFixture(async f => {
  const old = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview);
  await tick(); old.cancel();
  const newer = f.engine.applySettingsAsync({title: 'new'}, 's', f.preview);
  assert.equal(old.cancel(), false); await assert.rejects(old.promise, {code: 'IMAGES_CANCELLED'});
  await tick(); assert.equal(f.engine.settingsPending, true); assert.equal(f.workers[1].disposed, 0);
  f.workers[0].resolve(renderResult); f.workers[1].resolve(renderResult); await newer;
  assert.equal(f.engine.settings.title, 'new'); assert.equal(f.engine.imageAssets[0].bytes, 2);
}));

test('silent source changes and resource commits reject the old image preflight', () => runtimeFixture(async f => {
  let current = true;
  const old = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview, {scale: 1}, () => current);
  await tick(); current = false; f.workers[0].resolve(renderResult);
  await assert.rejects(old.promise, {code: 'IMAGES_CANCELLED'});
  const newer = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview);
  await tick(); f.engine.stageImages([patch('new.png', [8])]).commit();
  await assert.rejects(newer.promise, {code: 'IMAGES_CANCELLED'});
  assert.deepEqual(f.engine.imageAssets.map(x => x.destination), ['a.png', 'b.jpg', 'c.png', 'new.png']);
  assert.equal(f.engine.imageAssets[0].bytes, 2);
}));

test('page suspension cancels image work; restore admits only an explicit new request', () => runtimeFixture(async f => {
  const old = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview);
  await tick(); window.dispatchEvent(new Event('pagehide'));
  await assert.rejects(old.promise, {code: 'IMAGES_CANCELLED'});
  assert.throws(() => f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview), {code: 'IMAGES_SUSPENDED'});
  window.dispatchEvent(new Event('pageshow')); await tick(); assert.equal(f.workers.length, 1);
  const next = f.engine.beginImageChanges([patch('a.png', [9])], 's', f.preview);
  await tick(); f.workers[1].resolve(renderResult); await next.promise;
  assert.equal(f.engine.imageAssets[0].bytes, 1);
}));
