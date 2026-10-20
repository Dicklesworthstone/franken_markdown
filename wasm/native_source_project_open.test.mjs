import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Worker as NodeWorker} from 'node:worker_threads';
import {Script} from 'node:vm';
import {createNativeWorkspacePublisher} from './native_workspace_publisher.mjs';
import {createNativeSourceProjectOpenWorker} from './native_source_project_open_worker.mjs';
import {createNativeSourceProjectCodec} from './native_source_project.mjs';
import {projectFixtureParts, projectFixtureEngine, projectShell, projectExporterFixtureModule,
  PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS} from './native_source_project_fixture.mjs';

const parts = await projectFixtureParts();
const api = await projectExporterFixtureModule();
const project = api.createNativeSourceProject(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS());
const file = () => new Blob([project.bytes]);
function json(value) { return JSON.stringify(value).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029'); }
export const fixtureBindings = `
const json = ${json.toString()};
const projectShell = ${projectShell.toString()};
const {engine} = (${projectFixtureEngine.toString()})('selected trusted adapter');
export default async function init({module_or_path}) {
  if (!(module_or_path instanceof Uint8Array) || module_or_path.length !== 8) throw Error('fixture WASM bytes lost');
}
export const renderInteractiveHtmlConfigured = engine.renderInteractiveHtmlConfigured;
export const renderHtmlConfiguredAdvanced = engine.renderHtmlConfiguredAdvanced;
export const renderPdfConfiguredMulti = engine.renderPdfConfiguredMulti;
`;
const saved = (bindings = fixtureBindings) => JSON.stringify({version: 1,
  wasm: 'AGFzbQEAAAA=', bindings,
  options: {title: 'OLD_DOCUMENT_PRIVATE_TITLE'}, images: [{destination: 'OLD_PRIVATE_IMAGE', bytes: 'AQ=='}],
  fonts: [{slot: 'body-bold', bytes: 'Ag=='}]});
function harness() {
  const h = {urls: new Map(), timers: new Map(), workers: [], serial: 0};
  h.env = {
    URL: {createObjectURL(blob) { const key = 'blob:test-' + ++h.serial; h.urls.set(key, blob); return key; },
      revokeObjectURL(key) { h.urls.delete(key); }},
    setTimeout(fn, delay) { assert.equal(delay, 60000); const id = ++h.serial; h.timers.set(id, fn); return id; },
    clearTimeout(id) { h.timers.delete(id); },
    Worker: class {
      constructor(url) { this.code = h.urls.get(url); this.terminated = false; h.workers.push(this); h.construct?.(); }
      postMessage(request) { this.request = structuredClone(request); h.post?.(); }
      terminate() { this.terminated = true; h.terminate?.(); }
      respond(data) { this.onmessage?.({data}); }
    },
  };
  h.start = (input = file(), runtime = saved()) => createNativeSourceProjectOpenWorker(createNativeWorkspacePublisher, parts, {file: input, runtime}, h.env);
  h.run = async (worker = h.workers.at(-1)) => {
    const code = await worker.code.text();
    new Script(code);
    const native = new NodeWorker('const {parentPort}=require("node:worker_threads"); const self={postMessage:(v,t)=>parentPort.postMessage(v,t)};\n'
      + code + '\nparentPort.on("message",data=>self.onmessage?.({data}));', {eval: true});
    try {
      const result = await new Promise((resolve, reject) => {
        native.once('error', reject); native.once('message', resolve); native.postMessage(worker.request);
      });
      worker.respond(result); return result;
    } finally { await native.terminate(); }
  };
  return h;
}
function valid() {
  return {ok: true, bytes: new TextEncoder().encode('<!doctype html><p>owned</p>'), sourceLength: 0,
    filename: 'document.html', diagnostics: [], diagnosticCount: 0};
}

test('real worker rebuilds the complete project through the native workspace publisher', async () => {
  const h = harness(), operation = h.start(); await h.run(); const output = await operation.promise;
  const {engine} = projectFixtureEngine('selected trusted adapter');
  const selected = JSON.parse(saved()), data = createNativeSourceProjectCodec().decode(project.bytes);
  const expected = createNativeWorkspacePublisher(parts).render(engine, data.source,
    {version: 1, wasm: selected.wasm, bindings: selected.bindings, options: data.options, images: data.images, fonts: data.fonts.map(font => ({slot: font.slot, weight: font.weight, bytes: font.bytes}))});
  assert.deepEqual(output.bytes, expected.bytes);
  const exporter = await api.createWorkspaceExporterWithLoader({wasm: new Uint8Array([0,97,115,109,1,0,0,0]), bindings: fixtureBindings}, async () => engine);
  const direct = exporter.renderProject(project.bytes).text();
  const marker = /<script type="application\/json" id="fmd-native-runtime">([\s\S]*?)<\/script>/;
  assert.equal(new TextDecoder().decode(output.bytes).match(marker)[1], direct.match(marker)[1]);
  assert.equal(output.filename, 'Portable source.html');
  assert.equal(output.sourceLength, Buffer.byteLength(PROJECT_FIXTURE_SOURCE));
  const html = new TextDecoder().decode(output.bytes);
  assert(!html.includes('OLD_DOCUMENT_PRIVATE_TITLE')); assert(!html.includes('OLD_PRIVATE_IMAGE'));
  const source = JSON.parse(html.match(/<script type="application\/json" id="fmd-raw-source">([\s\S]*?)<\/script>/)[1]);
  const payload = JSON.parse(html.match(/<script type="application\/json" id="fmd-native-runtime">([\s\S]*?)<\/script>/)[1]);
  assert.equal(source, PROJECT_FIXTURE_SOURCE);
  assert.deepEqual(payload.options, data.options); assert.deepEqual(payload.images, data.images); assert.deepEqual(payload.fonts, data.fonts);
  assert.equal(payload.bindings, fixtureBindings); assert.equal(payload.wasm, selected.wasm);
  new Script(html.match(/<script id="fmd-native-bootstrap">([\s\S]*?)<\/script>/)[1]);
  assert.equal(h.urls.size, 0); assert.equal(h.timers.size, 0); assert(h.workers[0].terminated);
  assert.equal(operation.cancel(), false);
});

test('invalid project fails before importing even the explicitly selected trusted bindings', async () => {
  for (const text of ['{', '<html>not JSON</html>', JSON.stringify({...JSON.parse(project.text()), bindings: 'throw Error("from file")'})]) {
    const h = harness(), operation = h.start(new Blob([text]), saved('throw Error("TRUSTED_ENGINE_MUST_NOT_RUN");'));
    await h.run(); await assert.rejects(operation.promise, failure => failure.code === 'INVALID_SOURCE_PROJECT' && !failure.message.includes('TRUSTED_ENGINE'));
  }
});

test('unknown schema, noncanonical assets and invalid UTF-8 fail without loading an engine', async () => {
  const badSchema = {...JSON.parse(project.text()), schema: 'future'};
  const badAsset = JSON.parse(project.text()); badAsset.images[0].bytes = 'AB==';
  for (const [input, code] of [[JSON.stringify(badSchema), 'UNSUPPORTED_SOURCE_PROJECT'],
    [JSON.stringify(badAsset), 'INVALID_SOURCE_PROJECT'], [new Uint8Array([255]), 'INVALID_SOURCE_PROJECT']]) {
    const h = harness(), operation = h.start(new Blob([input]), saved('throw Error("must not load");'));
    await h.run(); await assert.rejects(operation.promise, {code});
  }
});

test('binding initialization and rendering failures reject the operation instead of exporting a partial workspace', async () => {
  for (const bindings of ['throw Error("init import failed");', 'export default async()=>{throw Error("initialization failed")};',
    'export default async()=>{};', fixtureBindings.replace("'selected trusted adapter'", '(()=>{throw Error("render failed")})()')]) {
    const h = harness(), operation = h.start(file(), saved(bindings)); await h.run();
    await assert.rejects(operation.promise); assert(h.workers[0].terminated); assert.equal(h.timers.size, 0);
  }
});

test('corrupt or oversized runtime metadata is rejected independently of admitted project data', async () => {
  for (const runtime of ['{', JSON.stringify({version: 2}), JSON.stringify({...JSON.parse(saved()), wasm: 'AQIDBAUGBwg='}),
    JSON.stringify({...JSON.parse(saved()), bindings: ''})]) {
    const h = harness(), operation = h.start(file(), runtime); await h.run(); await assert.rejects(operation.promise);
  }
});

test('source resembling script endings or function code stays data after reopening', async () => {
  const source = '\ufeff</script><!-- <script>globalThis.pwned=true</script>\r\n🧪';
  const encoded = api.createNativeSourceProject(source, {title: '../CON?'});
  const h = harness(), operation = h.start(new Blob([encoded.bytes])); await h.run();
  const output = await operation.promise, html = new TextDecoder().decode(output.bytes);
  assert.equal(output.filename, '.._CON_.html');
  assert(!html.includes('<script>globalThis.pwned=true'));
  assert.equal(JSON.parse(html.match(/id="fmd-raw-source">([\s\S]*?)<\/script>/)[1]), source);
  assert.equal(globalThis.pwned, undefined);
});

test('immutable Blob bytes, not overridden file properties, cross the worker boundary', async () => {
  const input = file(); let called = false;
  Object.defineProperty(input, 'size', {get() { called = true; throw Error('shadowed size'); }});
  input.arrayBuffer = () => { called = true; throw Error('host must not read'); };
  const h = harness(), operation = h.start(input); await h.run(); await operation.promise;
  assert.equal(called, false);
});

test('cancellation retires work and fences a late success', async () => {
  const h = harness(), operation = h.start(), worker = h.workers[0], late = worker.onmessage;
  assert.equal(operation.cancel(), true); assert.equal(operation.cancel(), false);
  late({data: valid()}); await assert.rejects(operation.promise, {code: 'PROJECT_OPEN_CANCELLED'});
  assert(worker.terminated); assert.equal(h.timers.size, 0); assert.equal(h.urls.size, 0);
});

test('deadline terminates a stuck read, startup or native render and never retries automatically', async () => {
  const h = harness(), operation = h.start(); [...h.timers.values()][0]();
  await assert.rejects(operation.promise, {code: 'PROJECT_OPEN_TIMEOUT'});
  assert.equal(h.workers.length, 1); assert(h.workers[0].terminated); assert.equal(h.timers.size, 0);
});

for (const kind of ['onerror', 'onmessageerror']) test(kind + ' settles and cleans up exactly once', async () => {
  const h = harness(), operation = h.start(), late = h.workers[0].onmessage;
  h.workers[0][kind]({preventDefault() {}}); late({data: valid()});
  await assert.rejects(operation.promise, {code: 'PROJECT_OPEN_FAILED'});
  assert.equal(h.timers.size, 0); assert(h.workers[0].terminated);
});

for (const fault of ['construct', 'post']) test(fault + ' failure releases worker URLs and capacity', async () => {
  const h = harness(); h[fault] = () => { throw Error('injected ' + fault); };
  const operation = h.start(); await assert.rejects(operation.promise, /injected/);
  assert.equal(h.urls.size, 0); assert.equal(h.timers.size, 0);
});

test('termination failure cannot publish retired output or mask cancellation', async () => {
  const h = harness(); h.terminate = () => { throw Error('stop failed'); };
  const operation = h.start(); operation.cancel(); await assert.rejects(operation.promise, {code: 'PROJECT_OPEN_CANCELLED'});
});

test('invalid files and a missing Worker fail before any I/O or fallback rendering', () => {
  const h = harness();
  for (const input of [null, {}, new Uint8Array([1]), new Blob()]) assert.throws(() => h.start(input));
  assert.equal(h.workers.length, 0);
  h.env.Worker = undefined; assert.throws(() => h.start(), {code: 'PROJECT_OPEN_UNAVAILABLE'});
  for (const runtime of ['', null, {}]) assert.throws(() => h.start(file(), runtime));
});

for (const [name, patch] of Object.entries({empty: {bytes: new Uint8Array()}, bytes: {bytes: [1]},
  path: {filename: '../a.html'}, extension: {filename: 'code.js'}, unicodeSize: {sourceLength: NaN},
  diagnostics: {diagnostics: [null]}, count: {diagnosticCount: -1}})) {
  test('invalid worker envelope never becomes a prepared workspace: ' + name, async () => {
    const h = harness(), operation = h.start(); h.workers[0].respond({...valid(), ...patch});
    await assert.rejects(operation.promise, /Invalid/); assert(h.workers[0].terminated);
  });
}

test('worker error codes/messages are bounded data, not executable markup', async () => {
  const h = harness(), operation = h.start();
  h.workers[0].respond({ok: false, code: '<script>bad</script>', message: 'x'.repeat(5000)});
  await assert.rejects(operation.promise, failure => failure.code === 'PROJECT_OPEN_FAILED' && failure.message.length === 2048);
});
