// Exercise the real saved-workspace factories, codec and disposable worker.
// Native calls are explicit ABI adapters: these tests do not claim PDF layout
// or a generated WASM package was executed.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {Worker} from 'node:worker_threads';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';
import {createWorkspaceExporterWithLoader, createNativeSourceProject, readNativeSourceProject} from './interactive_export.mjs';
import {createNativeSourceProjectCodec} from './native_source_project.mjs';

const encoder = new TextEncoder();
const plain = value => JSON.parse(JSON.stringify(value));
const source = '\ufeff# Exact pages\r\n\r\nMeasured café 😀\r';
const all = {typography: {homogeneous: true, antiriver: true, pareto: true},
  optimalPagination: true, strictPagination: true, microtype: 'expansion', baselineGridPt: 13.375};
const tokens = 'homogeneous,antiriver,pareto,optimal-pagination,strict-pagination,expansion';
const geometry = [792, 612, 24, 36, 48, 60];
function payload(options = {}) {
  return {version: 1, options: {font: 'serif', fontScale: 1.25, title: 'Owned', ...options},
    images: [{destination: 'chart.png', bytes: 'AQID'}],
    fonts: [{slot: 'body-regular', weight: 555, bytes: 'BAUG'}]};
}
function output(text, mimeType = 'application/pdf') {
  const bytes = encoder.encode(text);
  return {bytes, mimeType, diagnosticsJson: () => '[]', free() {bytes.fill(0);}};
}
function fixture() {
  const calls = [];
  const bindings = {
    renderHtmlConfiguredAdvanced(...args) {
      calls.push(['html', args]); return output('<html><head></head><body>Preview</body></html>', 'text/html');
    },
    renderInteractiveHtmlConfigured(markdown) {
      calls.push(['shell']); return output('<html><head></head><body><div class="fmd-content" id="fmd-content"></div>\n  </main>\n</div>\n'
        + '<script type="application/json" id="fmd-raw-source">' + JSON.stringify(markdown) + '</script>\n'
        + '<script>\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\n</script></body></html>', 'text/html');
    },
  };
  for (const [name, count] of [['renderPdfConfiguredMulti', 28], ['renderPdfConfiguredPage', 29], ['renderPdfConfiguredGrid', 37]]) {
    const binding = (...args) => {calls.push([name, args]); return output('%PDF-adapter\n' + args[0]);};
    // Match generated wasm-bindgen positional arity, including optional args.
    Object.defineProperty(binding, 'length', {value: count}); bindings[name] = binding;
  }
  return {bindings, calls};
}
function saved(output) {
  return JSON.parse(output.text().split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0]);
}

test('grid and exact pagination reach the complete native ABI with paper and owned resources', () => {
  const f = fixture(), p = payload({...all, pageGeometry: geometry});
  const renderer = createNativeWorkspaceRenderer(f.bindings, p);
  const bytes = renderer.pdf(source), [name, args] = f.calls.at(-1);
  assert.equal(name, 'renderPdfConfiguredGrid'); assert.equal(args.length, 37);
  assert.equal(args[0], source); assert.equal(args[3], 'Owned'); assert.equal(args[21], 1.25);
  assert.deepEqual(args[8], ['chart.png']); assert.deepEqual([...args[9]], [1, 2, 3]);
  assert.deepEqual([...args[11]], [4, 5, 6]); assert.equal(args[16][0], 555);
  assert.deepEqual([...args[27]], geometry);
  assert.deepEqual(args.slice(28, 35).map(value => ArrayBuffer.isView(value) ? [...value] : value), [[], false, false, false, [], [], []]);
  assert.equal(args[35], tokens); assert.equal(args[36], 13.375);
  assert.equal(new TextDecoder().decode(bytes), '%PDF-adapter\n' + source);
});

test('typography selects the trailing multi/page tokens while explicit disabled settings preserve old ABI shapes', () => {
  for (const pageGeometry of [undefined, geometry]) {
    const f = fixture(), options = {...all, baselineGridPt: 0, pageGeometry};
    createNativeWorkspaceRenderer(f.bindings, payload(options)).pdf(source);
    const [name, args] = f.calls.at(-1);
    assert.equal(name, pageGeometry ? 'renderPdfConfiguredPage' : 'renderPdfConfiguredMulti');
    assert.equal(args.length, pageGeometry ? 29 : 28); assert.equal(args.at(-1), tokens);
    createNativeWorkspaceRenderer(f.bindings, payload({pageGeometry, baselineGridPt: 0,
      typography: {homogeneous: false}, strictPagination: false, optimalPagination: false,
      microtype: 'disabled', microtypeProtrusion: false})).pdf(source);
    assert.equal(f.calls.at(-1)[1].length, pageGeometry ? 28 : 27);
  }
});

test('page fitting and optical protrusion use existing scalar positions without requiring new bindings', () => {
  const f = fixture(); Object.defineProperty(f.bindings.renderPdfConfiguredMulti, 'length', {value: 27});
  for (const settings of [{fitToPages: 4, microtype: 'protrusion'}, {fitToPages: 1, microtypeProtrusion: true}]) {
    createNativeWorkspaceRenderer(f.bindings, payload(settings)).pdf(source);
    const args = f.calls.at(-1)[1]; assert.equal(args.length, 27);
    assert.equal(args[25], settings.fitToPages); assert.equal(args[26], true);
  }
});

test('unsupported grid/token bindings refuse before assets are decoded or a document is rendered', () => {
  for (const [settings, key, arity] of [
    [{baselineGridPt: 12}, 'renderPdfConfiguredGrid', null],
    [{baselineGridPt: 12}, 'renderPdfConfiguredGrid', 36],
    [{strictPagination: true}, 'renderPdfConfiguredMulti', 27],
    [{typography: {pareto: true}, pageGeometry: geometry}, 'renderPdfConfiguredPage', 28],
    [{microtype: 'expansion'}, 'renderPdfConfiguredMulti', 27],
  ]) {
    const f = fixture();
    if (arity === null) delete f.bindings[key]; else Object.defineProperty(f.bindings[key], 'length', {value: arity});
    const p = payload(settings); Object.defineProperty(p, 'images', {get() {throw Error('assets were read');}});
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, p), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    assert.equal(f.calls.length, 0);
  }
});

test('settings transactions deeply own typography and preserve committed exports through rollback', () => {
  const f = fixture(), p = payload(), renderer = createNativeWorkspaceRenderer(f.bindings, p);
  const patch = plain(all), tx = renderer.stageSettings(patch); patch.typography.pareto = false;
  assert.equal(tx.settings.typography.pareto, true); assert(Object.isFrozen(tx.settings.typography));
  renderer.pdf('before'); assert.equal(f.calls.at(-1)[1].length, 27);
  tx.html('pending'); assert.equal(f.calls.at(-1)[0], 'html');
  tx.commit(); renderer.pdf('after'); assert.equal(f.calls.at(-1)[1][35], tokens);
  tx.rollback(); renderer.pdf('restored'); assert.equal(f.calls.at(-1)[1].length, 27);
  assert.equal(p.options.baselineGridPt, undefined);
});

test('invalid typography and fixed-grid/fitting conflicts cannot mutate settings or invoke getters', () => {
  const f = fixture(), renderer = createNativeWorkspaceRenderer(f.bindings, payload()); let reads = 0;
  const getter = {}; Object.defineProperty(getter, 'pareto', {enumerable: true, get() {reads++; return true;}});
  const invalid = [{typography: getter}, {typography: {unknown: true}}, {typography: []},
    {typography: {pareto: 1}}, {strictPagination: 1}, {optimalPagination: 'yes'}, {microtypeProtrusion: 0},
    {microtype: 'unknown'}, {baselineGridPt: 0.5}, {baselineGridPt: 145}, {baselineGridPt: NaN},
    {fitToPages: 0}, {fitToPages: 1.5}, {fitToPages: 0x100000000}, {fitToPages: 2, baselineGridPt: 12}];
  const before = renderer.settings;
  for (const patch of invalid) assert.throws(() => renderer.stageSettings(patch));
  assert.equal(renderer.settings, before); assert.equal(f.calls.length, 0); assert.equal(reads, 0);
});

test('native project round trips retain PDF settings, omission and editable owned nested options', () => {
  const first = createNativeSourceProject(source, all), restored = readNativeSourceProject(first.bytes);
  for (const [key, value] of Object.entries(all)) assert.deepEqual(restored.options[key], value);
  const second = createNativeSourceProject(restored.source, restored.options); assert.deepEqual(second.bytes, first.bytes);
  restored.options.typography.pareto = false;
  assert.equal(readNativeSourceProject(first.bytes).options.typography.pareto, true);
  const empty = readNativeSourceProject(createNativeSourceProject('x').bytes);
  for (const key of Object.keys(all)) assert(!Object.hasOwn(empty.options, key));
});

test('saved codec independently refuses malformed typesetting and never executes option accessors', () => {
  const codec = createNativeSourceProjectCodec(); let reads = 0;
  const getter = {}; Object.defineProperty(getter, 'antiriver', {enumerable: true, get() {reads++; return true;}});
  for (const options of [{typography: getter}, {strictPagination: 'true'}, {microtype: 'invalid'},
    {baselineGridPt: -1}, {fitToPages: 0}, {baselineGridPt: 12, fitToPages: 1}]) {
    const p = payload(options); assert.throws(() => codec.encode({source, options: p.options, images: p.images, fonts: p.fonts}),
      {code: 'INVALID_SOURCE_PROJECT'});
  }
  assert.equal(reads, 0);
});

test('exported and reopened native HTML preserves the complete typesetting request', async () => {
  const f = fixture();
  const exporter = await createWorkspaceExporterWithLoader({wasm: Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0), bindings: 'trusted fixture'}, async () => f.bindings);
  const result = exporter.render(source, {...all, page: {size: 'a4', margins: 24}});
  const p = saved(result); assert.deepEqual(p.options.typography, all.typography);
  const reopened = createNativeWorkspaceRenderer(f.bindings, JSON.parse(JSON.stringify(p)));
  reopened.pdf('revised'); assert.equal(f.calls.at(-1)[1][35], tokens); assert.equal(f.calls.at(-1)[1][36], 13.375);
  const project = createNativeSourceProject(source, all);
  assert.deepEqual(saved(exporter.renderProject(project.bytes)).options.typography, all.typography);
});

test('serialized renderer and codec retain typography without imports or outer lexical helpers', () => {
  const scope = {atob, Uint8Array, Uint32Array, Float64Array, TextEncoder, TextDecoder};
  const render = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')', scope);
  const codec = vm.runInNewContext('(' + createNativeSourceProjectCodec.toString() + ')', scope)();
  const f = fixture(), p = payload({...all, pageGeometry: geometry});
  const data = codec.decode(codec.encode({source, options: p.options, images: p.images, fonts: p.fonts}).bytes);
  render(f.bindings, {version: 1, ...data}).pdf(source);
  assert.equal(f.calls.at(-1)[1][35], tokens);
});

test('typed native pagination refusals remain coded and never produce default PDF output', () => {
  const f = fixture(); let renders = 0;
  const refusal = {code: 'PAGINATION_INFEASIBLE', reason: 'infeasible', message: 'Caption cannot fit'};
  f.bindings.renderPdfConfiguredMulti = function(a,b,c,d,e,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z,A,B,C) {
    renders++; throw JSON.stringify(refusal);
  };
  const renderer = createNativeWorkspaceRenderer(f.bindings, payload({strictPagination: true}));
  assert.throws(() => renderer.pdf(source), error => error instanceof Error
    && error.code === refusal.code && error.reason === refusal.reason && error.message === refusal.message);
  assert.equal(renders, 1);
});

function workerFactory(code) {
  const thread = new Worker(`const {parentPort}=require('node:worker_threads');
    globalThis.self=globalThis; globalThis.Blob=class {constructor(parts){this.text=parts.join('');}};
    URL.createObjectURL=blob=>'data:text/javascript;base64,'+Buffer.from(blob.text).toString('base64'); URL.revokeObjectURL=()=>{};
    globalThis.postMessage=(data,transfer)=>parentPort.postMessage(data,transfer);
    parentPort.on('message',data=>globalThis.onmessage({data})); ${code}`, {eval: true});
  const listeners = new Map();
  return {addEventListener(type, fn) {const cb = data => fn(type === 'message' ? {data} : data); listeners.set(type, cb); thread.on(type, cb);},
    removeEventListener(type) {const cb = listeners.get(type); if (cb) thread.off(type, cb); listeners.delete(type);},
    postMessage: data => thread.postMessage(data), terminate: () => thread.terminate()};
}

test('real disposable workers retain strict pagination reason codes and can recover for the next export', async () => {
  const bindings = `export default async ({module_or_path}) => {await WebAssembly.instantiate(module_or_path);};
    export function renderHtmlConfiguredAdvanced(){}
    export function renderPdfConfiguredMulti(a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z,A,B) {
      if (a === 'refuse') throw JSON.stringify({code:'PAGINATION_INFEASIBLE',reason:'infeasible',message:'caption cannot fit'});
      return {bytes:new TextEncoder().encode('%PDF-'+B),mimeType:'application/pdf',diagnosticsJson:()=> '[]',free(){}};
    }`;
  const p = {...payload({strictPagination: true}), bindings, wasm: 'AGFzbQEAAAA='};
  const worker = createWorkspacePreviewWorker(createNativeWorkspaceRenderer, p, {workerFactory, timeoutMs: 10000});
  try {
    const state = {options: p.options, images: p.images};
    await assert.rejects(worker.exportDocument('pdf', 'refuse', state), {code: 'PAGINATION_INFEASIBLE', reason: 'infeasible'});
    const result = await worker.exportDocument('pdf', source, state);
    assert.equal(new TextDecoder().decode(result.bytes), '%PDF-strict-pagination');
  } finally {worker.dispose();}
});
