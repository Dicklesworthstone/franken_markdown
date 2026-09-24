// Production runtime, renderer and worker transport; DOM/native-ABI adapters are
// explicit. Real worker_threads and empty WASM initialization execute below.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Worker as Thread} from 'node:worker_threads';
import {bootNativeWorkspace, createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

const bindings = `
export default async function({module_or_path}) { await WebAssembly.instantiate(module_or_path); }
function render(args, mimeType) {
  if (typeof document !== 'undefined') globalThis.settingsMainCalls.push(mimeType);
  const title = args[3];
  if (title === 'blocked') {
    if (typeof document !== 'undefined') throw Error('Unexpected main-thread render');
    globalThis.settingsStarted?.();
    while (true) {}
  }
  if (title === 'refused') throw Error('Native settings refusal');
  const text = JSON.stringify(args, (_, value) => ArrayBuffer.isView(value) ? Array.from(value) : value);
  const bytes = new TextEncoder().encode(mimeType === 'text/html'
    ? '<html><head></head><body>' + text + '</body></html>' : '%PDF-ADAPTER\\n' + text);
  return {bytes, mimeType, diagnosticsJson: () => JSON.stringify([{message: 'render:' + title}]), free() {bytes.fill(0);}};
}
export function renderHtmlConfiguredAdvanced(...args) {return render(args, 'text/html');}
export function renderPdfConfiguredMulti(...args) {return render(args, 'application/pdf');}
export const renderPdfConfiguredPage = renderPdfConfiguredMulti;
`;
const display = {scale: 1, theme: 'light'};
const rejected = (promise, code) => assert.rejects(promise, error => error.code === code);
const tick = () => new Promise(resolve => setImmediate(resolve));
async function until(check) {
  const deadline = Date.now() + 2500;
  while (!check()) {
    if (Date.now() > deadline) throw Error('Worker did not reach the expected state');
    await new Promise(resolve => setTimeout(resolve, 5));
  }
}
function realWorker(code, stats) {
  stats.started++;
  const thread = new Thread(`
    const {parentPort} = require('node:worker_threads');
    globalThis.self = globalThis;
    globalThis.Blob = class { constructor(parts) {this.text = parts.join('');} };
    URL.createObjectURL = blob => 'data:text/javascript;base64,' + Buffer.from(blob.text).toString('base64');
    URL.revokeObjectURL = () => {};
    globalThis.postMessage = (value, transfer) => parentPort.postMessage(value, transfer);
    globalThis.settingsStarted = () => parentPort.postMessage({settingsTestStarted: true});
    parentPort.on('message', data => globalThis.onmessage({data}));
    ${code}
  `, {eval: true});
  const listeners = new Map();
  return {
    addEventListener(type, fn) {
      const wrapped = value => {
        if (type === 'message' && value?.settingsTestStarted) {stats.blocked++; return;}
        fn(type === 'message' ? {data: value} : value);
      };
      listeners.set(type, wrapped); thread.on(type, wrapped);
    },
    removeEventListener(type) {const wrapped = listeners.get(type); if (wrapped) thread.off(type, wrapped); listeners.delete(type);},
    postMessage: value => thread.postMessage(value),
    terminate() {stats.stopped++; return thread.terminate();},
  };
}
async function environment(run, {controlled = false, background = true, timeoutMs = 1500, constructorFailure = false} = {}) {
  const keys = ['window', 'document', 'URL', 'settingsMainCalls'];
  const previous = keys.map(key => Object.getOwnPropertyDescriptor(globalThis, key));
  const payload = {version: 1, wasm: 'AGFzbQEAAAA=', bindings,
    options: {font: 'serif', darkMode: 'auto', fontScale: 1.25, title: 'Original', author: 'Author', lang: 'fr',
      toc: true, tocDepth: 4, pageNumbers: true, codeLineNumbers: true, metadataEpochSeconds: 0,
      pageGeometry: [792, 612, 24, 36, 48, 60]},
    images: [{destination: 'plot.png', bytes: 'AQID'}], fonts: [{slot: 'body-regular', weight: 555, bytes: 'BAUG'}]};
  const data = {textContent: JSON.stringify(payload)};
  const originalFrame = {srcdoc: 'previous preview'};
  const preview = {childNodes: [originalFrame], replaceChildren(...children) {this.childNodes = children;}};
  const stats = {started: 0, stopped: 0, blocked: 0}, clients = [], jobs = [];
  globalThis.settingsMainCalls = [];
  globalThis.window = new EventTarget();
  globalThis.document = {querySelector: () => data,
    createElement: () => ({style: {}, attributes: {}, setAttribute(key, value) {this.attributes[key] = value;}, addEventListener() {}})};
  globalThis.URL = {createObjectURL: () => 'data:text/javascript;base64,' + Buffer.from(bindings).toString('base64'), revokeObjectURL() {}};
  const makeWorker = (factory, saved) => {
    if (constructorFailure) throw Error('Worker construction failed');
    let client;
    if (controlled) {
      // Deliberately allow responses after disposal to exercise runtime guards.
      const job = {saved, disposed: 0}; jobs.push(job);
      client = {render(source, view, state) {
        Object.assign(job, {source, view, state});
        return new Promise((resolve, reject) => Object.assign(job, {resolve, reject}));
      }, dispose() {job.disposed++;}, invalidate() {}};
    } else client = createWorkspacePreviewWorker(factory, saved, {timeoutMs, workerFactory: code => realWorker(code, stats)});
    clients.push(client); return client;
  };
  try {
    bootNativeWorkspace(createNativeWorkspaceRenderer, background ? makeWorker : undefined);
    const engine = window.__fmdNativeRuntime;
    assert.throws(() => engine.applySettingsAsync({}, 'loading', preview, display), /loading/);
    assert.equal(await engine.ready, true);
    const apply = (patch, source = 'source', view = display, current) => engine.applySettingsAsync(patch, source, preview, view, current);
    await run({engine, apply, data, preview, originalFrame, stats, clients, jobs, calls: globalThis.settingsMainCalls});
  } finally {
    for (const client of clients) client.dispose();
    keys.forEach((key, i) => {if (previous[i]) Object.defineProperty(globalThis, key, previous[i]); else delete globalThis[key];});
  }
}
const result = (title = 'new') => ({html: '<html><head></head><body>' + title + '</body></html>', diagnostics: [{message: title}]});
const htmlArgs = preview => JSON.parse(preview.childNodes[0].srcdoc.split('<body>')[1].split('</body>')[0]);

test('worker preflight atomically commits exact source, settings, resources and native diagnostics without main rendering', async () => {
  await environment(async ({engine, apply, data, preview, stats, calls}) => {
    const source = '\ufeff\r\nCafé 😀\r', geometry = [612, 792, 30, 40, 50, 60];
    const value = await apply({font: 'sans', fontScale: 1.5, title: 'Changed', pageGeometry: geometry}, source, {scale: 1.2, theme: 'dark'});
    assert.equal(value, engine.settings); assert.equal(value.font, 'sans');
    assert.deepEqual(JSON.parse(data.textContent).options, value);
    assert.deepEqual(value.pageGeometry, geometry); assert.equal(value.metadataEpochSeconds, 0);
    const args = htmlArgs(preview);
    assert.equal(args[0], source); assert.equal(args[1], 'sans'); assert.equal(args[2], 'auto');
    assert.equal(args[3], 'Changed'); assert.equal(args[6], 1.5 * 1.2);
    assert.deepEqual(args[7], [4, 5, 6]); assert.equal(args[12][0], 555);
    assert.deepEqual(args[13], ['plot.png']); assert.deepEqual(args[14], [1, 2, 3]);
    assert.deepEqual(args.slice(16, 19), ['fr', true, 4]);
    assert.match(preview.childNodes[0].srcdoc, /Content-Security-Policy/);
    assert.equal(preview.childNodes[0].attributes.sandbox, 'allow-same-origin');
    assert.equal(engine.diagnostics[0].message, 'render:Changed');
    assert.deepEqual(calls, []); assert.equal(stats.started, 1); assert.equal(stats.stopped, 1);
    assert.equal(engine.settingsPending, false);
    const pdf = await engine.exportDocument('pdf', source);
    const pdfArgs = JSON.parse(new TextDecoder().decode(pdf.bytes).split('\n')[1]);
    assert.deepEqual(pdfArgs[27], geometry); assert.equal(pdfArgs[21], 1.5);
  });
});

test('staged work leaves JSON/options/preview unchanged and captures mutable caller patches and display', async () => {
  await environment(async ({engine, apply, data, preview, originalFrame, jobs}) => {
    const initial = data.textContent, original = engine.settings;
    const patch = {font: 'sans', pageGeometry: [612, 792, 20, 20, 20, 20]}, view = {scale: 1.2};
    const pending = apply(patch, 'exact', view); patch.font = 'serif'; patch.pageGeometry[0] = 999; view.scale = 2;
    await tick(); assert.equal(engine.settings, original); assert.equal(data.textContent, initial);
    assert.equal(preview.childNodes[0], originalFrame); assert.equal(engine.settingsPending, true);
    assert.equal(jobs[0].state.options.font, 'sans'); assert.equal(jobs[0].state.options.pageGeometry[0], 612);
    assert.equal(jobs[0].view.scale, 1.2); jobs[0].resolve(result()); await pending;
    assert.equal(engine.settings.font, 'sans'); assert.equal(jobs[0].disposed, 1);
  }, {controlled: true});
});

test('settings validation rejects invalid options and accessors before owning a worker', async () => {
  await environment(async ({apply, engine, stats, data}) => {
    const initial = data.textContent; let accessed = false;
    const accessor = Object.defineProperty({}, 'title', {get() {accessed = true; return 'bad';}});
    for (const patch of [{fontScale: 100}, {font: 'unknown'}, {pageGeometry: [100, 792, 0, 0, 0, 0]}, accessor]) assert.throws(() => apply(patch));
    assert.throws(() => apply({}, 'x', {scale: NaN}), error => error.code === 'SETTINGS_OPTIONS');
    assert.throws(() => apply({}, 'x', display, false), error => error.code === 'SETTINGS_OPTIONS');
    assert.equal(accessed, false); assert.equal(stats.started, 0); assert.equal(data.textContent, initial);
    assert.equal(engine.settingsPending, false);
  });
});

test('settings and exports share one slot while live preview remains independent during a hung preflight', async () => {
  await environment(async ({engine, apply, preview, stats}) => {
    const pending = rejected(apply({title: 'blocked'}), 'SETTINGS_CANCELLED');
    await until(() => stats.blocked === 1);
    assert.throws(() => apply({title: 'second'}), error => error.code === 'SETTINGS_BUSY');
    assert.throws(() => engine.exportDocument('pdf', 'x'), error => error.code === 'EXPORT_BUSY');
    assert.equal(await engine.render('still responsive', preview, display), true);
    assert.equal(htmlArgs(preview)[0], 'still responsive');
    assert.equal(stats.started, 2); engine.cancelSettings(); await pending;
    const exported = engine.exportDocument('pdf', 'old committed options');
    assert.throws(() => apply({title: 'next'}), error => error.code === 'SETTINGS_BUSY'); await exported;
    assert.equal(await engine.render('preview survived', preview, display), true);
  });
});

test('cancellation before dispatch creates no worker and permits immediate retry', async () => {
  await environment(async ({engine, apply, stats}) => {
    const pending = rejected(apply({title: 'cancel'}), 'SETTINGS_CANCELLED');
    engine.cancelSettings(); engine.cancelSettings(); await pending; assert.equal(stats.started, 0);
    await apply({title: 'retry'}); assert.equal(engine.settings.title, 'retry');
  });
});

test('actual busy computation is terminated on cancel while host timers continue and committed state survives', async () => {
  await environment(async ({engine, apply, stats, data, preview, originalFrame, calls}) => {
    const original = data.textContent; let beats = 0;
    const timer = setInterval(() => beats++, 5);
    try {
      const pending = rejected(apply({title: 'blocked'}), 'SETTINGS_CANCELLED');
      await until(() => stats.blocked === 1);
      await new Promise(resolve => setTimeout(resolve, 40)); engine.cancelSettings(); await pending;
      assert.ok(beats >= 4); assert.equal(stats.stopped, 1);
      assert.equal(data.textContent, original); assert.equal(preview.childNodes[0], originalFrame); assert.deepEqual(calls, []);
    } finally {clearInterval(timer);}
  });
});

test('late cancelled results cannot publish or clear the busy state of a newer settings operation', async () => {
  await environment(async ({engine, apply, jobs, preview}) => {
    const old = rejected(apply({title: 'old'}), 'SETTINGS_CANCELLED'); await tick(); engine.cancelSettings();
    const next = apply({title: 'next'}); await tick(); jobs[0].resolve(result('old')); await old;
    assert.equal(engine.settingsPending, true); assert.equal(engine.settings.title, 'Original');
    jobs[1].resolve(result('next')); await next;
    assert.equal(engine.settings.title, 'next'); assert.match(preview.childNodes[0].srcdoc, /next/);
  }, {controlled: true});
});

test('revision callback catches edits without input events before worker start and before publication', async () => {
  await environment(async ({apply, engine, jobs, data}) => {
    const initial = data.textContent;
    await rejected(apply({title: 'stale'}, 'x', display, () => false), 'SETTINGS_CANCELLED');
    assert.equal(jobs.length, 0);
    let current = true; const pending = rejected(apply({title: 'late'}, 'x', display, () => current), 'SETTINGS_CANCELLED');
    await tick(); current = false; jobs[0].resolve(result()); await pending;
    assert.equal(data.textContent, initial); assert.equal(engine.settings.title, 'Original');
  }, {controlled: true});
});

test('preview invalidation prevents edit/undo round trips from resurrecting settings', async () => {
  await environment(async ({engine, apply, jobs}) => {
    const pending = rejected(apply({title: 'obsolete'}), 'SETTINGS_CANCELLED'); await tick();
    engine.invalidatePreview(); engine.invalidatePreview(); jobs[0].resolve(result()); await pending;
    assert.equal(engine.settings.title, 'Original'); assert.equal(engine.settingsPending, false);
  }, {controlled: true});
});

test('successful synchronous settings changes retire staged work, but failed validation does not', async () => {
  await environment(async ({engine, apply, jobs, preview}) => {
    const pending = rejected(apply({title: 'obsolete'}), 'SETTINGS_CANCELLED'); await tick();
    assert.throws(() => engine.applySettings({fontScale: 99}, 'x', preview, display));
    assert.equal(engine.settingsPending, true);
    engine.applySettings({title: 'synchronous'}, 'x', preview, display);
    jobs[0].resolve(result()); await pending; assert.equal(engine.settings.title, 'synchronous');
  }, {controlled: true});
});

test('image commit and rollback cancel preflights without losing committed resources', async () => {
  await environment(async ({engine, apply, jobs, data}) => {
    const tx = engine.stageImages([{destination: 'new.png', bytes: Uint8Array.of(9, 8)}]);
    const first = rejected(apply({title: 'before images'}), 'SETTINGS_CANCELLED'); await tick();
    tx.commit(); jobs[0].resolve(result()); await first;
    assert.equal(JSON.parse(data.textContent).images.length, 2);
    const second = rejected(apply({title: 'before rollback'}), 'SETTINGS_CANCELLED'); await tick();
    tx.rollback(); jobs[1].resolve(result()); await second;
    assert.equal(JSON.parse(data.textContent).images.length, 1); assert.equal(engine.settings.title, 'Original');
  }, {controlled: true});
});

test('suspension stops busy settings and resume never silently reapplies an abandoned draft', async () => {
  await environment(async ({engine, apply, stats}) => {
    const pending = rejected(apply({title: 'blocked'}), 'SETTINGS_CANCELLED'); await until(() => stats.blocked === 1);
    window.dispatchEvent(new Event('pagehide')); await pending;
    assert.throws(() => apply({title: 'suspended'}), error => error.code === 'SETTINGS_SUSPENDED');
    window.dispatchEvent(new Event('pageshow')); await tick(); assert.equal(stats.started, 1);
    await apply({title: 'explicit retry'}); assert.equal(engine.settings.title, 'explicit retry');
  });
});

test('native refusal and deadlines release every worker and never fall back to main-thread rendering', async () => {
  await environment(async ({engine, apply, data, stats, calls}) => {
    const initial = data.textContent;
    await rejected(apply({title: 'refused'}), 'PREVIEW_RENDER_FAILED');
    await rejected(apply({title: 'blocked'}), 'PREVIEW_TIMEOUT');
    assert.equal(data.textContent, initial); assert.equal(engine.settings.title, 'Original');
    assert.equal(stats.started, 2); assert.equal(stats.stopped, 2); assert.deepEqual(calls, []);
    assert.equal(engine.settingsPending, false); await apply({title: 'recovered'});
  }, {timeoutMs: 500});
});

test('JSON persistence and DOM failures restore preview, settings and diagnostics atomically', async () => {
  for (const fault of ['json', 'dom']) await environment(async ({engine, apply, data, preview, jobs, originalFrame}) => {
    const initial = data.textContent, options = engine.settings, diagnostics = engine.diagnostics;
    let stored = initial, fail = true;
    if (fault === 'json') Object.defineProperty(data, 'textContent', {get: () => stored, set(value) {
      if (fail) {fail = false; throw Error('persistence refused');} stored = value;
    }});
    else preview.replaceChildren = function(...children) {
      this.childNodes = children; if (fail) {fail = false; throw Error('DOM refused');}
    };
    const pending = assert.rejects(apply({title: 'not committed'}), /refused/); await tick();
    jobs[0].resolve(result()); await pending;
    assert.equal(data.textContent, initial); assert.equal(engine.settings, options);
    assert.equal(preview.childNodes[0], originalFrame); assert.equal(engine.diagnostics, diagnostics);
    assert.equal(engine.settingsPending, false);
    const retry = apply({title: 'retry'}); await tick(); jobs[1].resolve(result()); await retry;
    assert.equal(engine.settings.title, 'retry');
  }, {controlled: true});
});

test('worker construction failure settles the operation without persistence or synchronous fallback', async () => {
  await environment(async ({apply, engine, calls, data}) => {
    const initial = data.textContent;
    await assert.rejects(apply({title: 'new'}), /construction failed/);
    assert.equal(data.textContent, initial); assert.equal(engine.settingsPending, false); assert.deepEqual(calls, []);
    await assert.rejects(apply({title: 'retry'}), /construction failed/);
  }, {constructorFailure: true});
});

test('settings publication invalidates an older preview result before it can replace the committed frame', async () => {
  await environment(async ({engine, apply, preview, jobs}) => {
    const oldPreview = engine.render('old', preview, display);
    const pending = apply({title: 'next'}, 'current'); await tick();
    jobs[1].resolve(result('new settings')); await pending;
    jobs[0].resolve(result('obsolete preview')); assert.equal(await oldPreview, false);
    assert.match(preview.childNodes[0].srcdoc, /new settings/);
  }, {controlled: true});
});

test('malformed source Unicode is rejected before worker startup and leaves settings unchanged', async () => {
  await environment(async ({apply, engine, stats}) => {
    await rejected(apply({title: 'new'}, '\ud800'), 'PREVIEW_UNICODE');
    assert.equal(stats.started, 0); assert.equal(engine.settings.title, 'Original');
    assert.equal(engine.settingsPending, false);
  });
});

test('legacy boot remains synchronously usable and does not advertise unsupported asynchronous settings', async () => {
  await environment(async ({engine, apply, preview, calls}) => {
    assert.equal(engine.settingsMode, 'synchronous');
    assert.throws(() => apply({title: 'new'}), error => error.code === 'UNSUPPORTED_WASM_PACKAGE');
    assert.equal(engine.applySettings({title: 'legacy'}, 'x', preview, {}).title, 'legacy');
    assert.deepEqual(calls, ['text/html']); assert.equal(engine.settingsPending, false);
  }, {background: false});
});

test('both serialized factories remain import-free and contain no HTML script terminators', () => {
  assert.doesNotMatch(bootNativeWorkspace.toString(), /<\/script/i);
  assert.doesNotMatch(createNativeWorkspaceRenderer.toString(), /<\/script/i);
});
