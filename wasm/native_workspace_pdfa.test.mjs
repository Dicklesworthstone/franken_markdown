// Production saved-workspace factories and source codec, with explicit native
// ABI adapters. Native archival/grid byte parity is tested in Rust separately.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {Worker} from 'node:worker_threads';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';
import {createWorkspaceExporterWithLoader, createNativeSourceProject, readNativeSourceProject} from './interactive_export.mjs';
import {createNativeSourceProjectCodec} from './native_source_project.mjs';

const source = '\ufeff# Archive\r\n\r\nA café and ![Chart](chart.png)\r';
const geometry = [792, 612, 24, 36, 48, 60];
function payload(options = {}) {
  return {version: 1, options: {font: 'serif', fontScale: 1.25, title: 'Archive', ...options},
    images: [{destination: 'chart.png', bytes: 'AQID'}],
    fonts: [{slot: 'body-regular', weight: 555, bytes: 'BAUG'}]};
}
function output(text, mimeType = 'application/pdf') {
  const bytes = new TextEncoder().encode(text);
  return {bytes, mimeType, diagnosticsJson: () => '[]', free() {bytes.fill(0);}};
}
function fixture() {
  const calls = [];
  const bindings = {
    renderHtmlConfiguredAdvanced() {return output('<html><head></head><body>Preview</body></html>', 'text/html');},
    renderInteractiveHtmlConfigured(markdown) {
      return output('<html><head></head><body><div class="fmd-content" id="fmd-content"></div>\n  </main>\n</div>\n'
        + '<script type="application/json" id="fmd-raw-source">' + JSON.stringify(markdown) + '</script>\n'
        + '<script>\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\n</script></body></html>', 'text/html');
    },
  };
  for (const [name, arity] of [['renderPdfConfiguredMulti', 28], ['renderPdfConfiguredArchival', 39]]) {
    const render = (...args) => {calls.push({name, args}); return output('%PDF-ADAPTER\n' + args[0]);};
    Object.defineProperty(render, 'length', {value: arity, configurable: true}); bindings[name] = render;
  }
  return {bindings, calls};
}

test('archival workspace export preserves complete typography, paper, source and asset settings', () => {
  for (const grid of [undefined, 13.1254]) {
    const f = fixture(), p = payload({pdfA: '2b', pdfAStrict: true, pageGeometry: geometry,
      baselineGridPt: grid, typography: {homogeneous: true}, strictPagination: true, microtype: 'expansion'});
    const renderer = createNativeWorkspaceRenderer(f.bindings, p);
    const result = renderer.pdf(source), {name, args} = f.calls.at(-1);
    assert.equal(name, 'renderPdfConfiguredArchival'); assert.equal(args.length, grid ? 39 : 38);
    assert.equal(args[0], source); assert.equal(args[3], 'Archive'); assert.equal(args[21], 1.25);
    assert.deepEqual(args[8], ['chart.png']); assert.deepEqual([...args[9]], [1, 2, 3]);
    assert.deepEqual([...args[11]], [4, 5, 6]); assert.equal(args[16][0], 555);
    assert.deepEqual([...args[27]], geometry); assert.equal(args[35], 'homogeneous,strict-pagination,expansion');
    assert.equal(args[36], '2b'); assert.equal(args[37], true); assert.equal(args[38], grid);
    assert.equal(new TextDecoder().decode(result), '%PDF-ADAPTER\n' + source);
  }
});

test('ordinary workspaces keep their existing ABI, and pre-grid archival bindings remain usable', () => {
  const f = fixture();
  Object.defineProperty(f.bindings.renderPdfConfiguredArchival, 'length', {value: 38});
  createNativeWorkspaceRenderer(f.bindings, payload({pdfA: '2b'})).pdf(source);
  assert.equal(f.calls.at(-1).args.length, 38); assert.equal(f.calls.at(-1).args[37], false);
  delete f.bindings.renderPdfConfiguredArchival;
  for (const options of [{}, {pdfA: 'off'}, {pdfAStrict: false}, {pdfA: 'off', pdfAStrict: false}]) {
    createNativeWorkspaceRenderer(f.bindings, payload(options)).pdf(source);
    assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti'); assert.equal(f.calls.at(-1).args.length, 27);
  }
});

test('missing or old archival bindings reject before resources or document work', () => {
  for (const [arity, grid] of [[undefined, undefined], [37, undefined], [38, 12]]) {
    const f = fixture(), p = payload({pdfA: '2b', baselineGridPt: grid});
    if (arity === undefined) delete f.bindings.renderPdfConfiguredArchival;
    else Object.defineProperty(f.bindings.renderPdfConfiguredArchival, 'length', {value: arity});
    Object.defineProperty(p, 'images', {get() {throw Error('assets were read');}});
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, p), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    assert.equal(f.calls.length, 0);
  }
});

test('archival settings are atomic and invalid policies cannot replace committed state', () => {
  const f = fixture(), renderer = createNativeWorkspaceRenderer(f.bindings, payload());
  const before = renderer.settings;
  for (const patch of [{pdfA: '3b'}, {pdfA: null}, {pdfAStrict: 'true'}, {pdfAStrict: true},
    {pdfA: 'off', pdfAStrict: true}, {pdfA: '2b', baselineGridPt: 12, fitToPages: 2}]) {
    assert.throws(() => renderer.stageSettings(patch)); assert.equal(renderer.settings, before);
  }
  const patch = {pdfA: '2b', pdfAStrict: true}, tx = renderer.stageSettings(patch);
  patch.pdfA = 'off'; tx.html(source); renderer.pdf(source);
  assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti');
  tx.commit(); renderer.pdf(source); assert.equal(f.calls.at(-1).args[37], true);
  tx.rollback(); renderer.pdf(source); assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti');
  assert.equal(renderer.settings, before);
});

test('projects and saved HTML retain exact archival policies across reopening', async () => {
  for (const options of [{}, {pdfA: 'off'}, {pdfAStrict: false}, {pdfA: '2b'},
    {pdfA: '2b', pdfAStrict: true, baselineGridPt: 13.1254}]) {
    const first = createNativeSourceProject(source, options), project = readNativeSourceProject(first.bytes);
    assert.equal(project.source, source);
    for (const key of ['pdfA', 'pdfAStrict', 'baselineGridPt']) {
      assert.equal(Object.hasOwn(project.options, key), Object.hasOwn(options, key));
      assert.equal(project.options[key], options[key]);
    }
    assert.deepEqual(createNativeSourceProject(project.source, project.options).bytes, first.bytes);
    const f = fixture(), exporter = await createWorkspaceExporterWithLoader({wasm: Uint8Array.of(0,97,115,109,1,0,0,0),
      bindings: 'trusted adapter'}, async () => f.bindings);
    const saved = exporter.renderProject(first.bytes);
    const p = JSON.parse(saved.text().split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0]);
    createNativeWorkspaceRenderer(f.bindings, p).pdf(source);
    assert.equal(f.calls.at(-1).name, options.pdfA === '2b' ? 'renderPdfConfiguredArchival' : 'renderPdfConfiguredMulti');
  }
});

test('serialized renderer and source codec preserve admission without outer helpers', () => {
  const scope = {atob, TextEncoder, TextDecoder, Uint8Array, Uint32Array, Float64Array};
  const render = vm.runInNewContext('(' + createNativeWorkspaceRenderer.toString() + ')', scope);
  const codec = vm.runInNewContext('(' + createNativeSourceProjectCodec.toString() + ')', scope)();
  const f = fixture(), p = payload({pdfA: '2b', pdfAStrict: true, baselineGridPt: 12});
  const data = codec.decode(codec.encode({source, options: p.options, images: p.images, fonts: p.fonts}).bytes);
  render(f.bindings, {version: 1, ...data}).pdf(source); assert.equal(f.calls.at(-1).args[38], 12);
  for (const options of [{pdfA: '3b'}, {pdfAStrict: true}, {pdfA: '2b', pdfAStrict: 'true'}]) {
    assert.throws(() => codec.encode({source, options, images: [], fonts: []}), {code: 'INVALID_SOURCE_PROJECT'});
  }
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

test('background archival refusal never falls back to ordinary PDF and a later export recovers', async () => {
  const bindings = `export default async ({module_or_path})=>{await WebAssembly.instantiate(module_or_path);};
    export function renderHtmlConfiguredAdvanced(){}
    export function renderPdfConfiguredMulti(){throw Error('ordinary fallback');}
    export const renderPdfConfiguredArchival=Object.defineProperty((...a)=>{
      if(a[0]==='refuse') throw Error('pdf_a_file_uri: local file action forbidden');
      return {bytes:new TextEncoder().encode('%PDF-'+a[36]+'/'+a[37]+'/'+a[38]),mimeType:'application/pdf',diagnosticsJson:()=> '[]',free(){}};
    },'length',{value:39});`;
  const p = {...payload({pdfA: '2b', pdfAStrict: true, baselineGridPt: 12}), bindings, wasm: 'AGFzbQEAAAA='};
  const worker = createWorkspacePreviewWorker(createNativeWorkspaceRenderer, p, {workerFactory, timeoutMs: 10000});
  try {
    const state = {options: p.options, images: p.images};
    await assert.rejects(worker.exportDocument('pdf', 'refuse', state), /pdf_a_file_uri/);
    const result = await worker.exportDocument('pdf', source, state);
    assert.equal(new TextDecoder().decode(result.bytes), '%PDF-2b/true/12');
  } finally {worker.dispose();}
});
