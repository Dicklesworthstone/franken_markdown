// Execute the shipped settings function against a stateful form adapter and the
// real saved-workspace renderer. Native outputs are explicit ABI doubles. This
// verifies controller/transaction behavior, not browser DOM or PDF rendering.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

const controller = readFileSync(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const install = controller.slice(controller.indexOf('  function installSettings() {'),
  controller.indexOf('  function installPreviewControls() {'));
function node() {
  const handlers = new Map();
  return {style: {}, disabled: false, checked: false, textContent: '', open: false,
    addEventListener(name, callback) { const list = handlers.get(name) ?? []; list.push(callback); handlers.set(name, list); },
    dispatch(name, event = {}) { for (const run of handlers.get(name) ?? []) run({preventDefault() {}, ...event}); },
    setAttribute() {}, removeAttribute() {}, focus() {},
    replaceChildren(...children) { this.children = children; },
    showModal() {this.open = true;},
    close() {this.open = false; this.dispatch('close');},
  };
}
function field(tag, attributes) {
  const result = node(), type = attributes.match(/type="([^"]+)"/)?.[1] ?? 'text';
  let value = '';
  Object.defineProperty(result, 'value', {
    get() {return value;},
    set(next) {value = String(next); if (tag === 'input' && type === 'text') value = value.replace(/[\r\n]/g, '');},
  });
  Object.defineProperty(result, 'valueAsNumber', {get() {return value === '' ? NaN : Number(value);}});
  result.valid = () => {
    if (type !== 'number') return true;
    if (value === '') return !/\brequired\b/.test(attributes);
    const n = Number(value), min = Number(attributes.match(/min="([^"]+)"/)?.[1] ?? '-Infinity');
    const max = Number(attributes.match(/max="([^"]+)"/)?.[1] ?? 'Infinity');
    return Number.isFinite(n) && n >= min && n <= max
      && (attributes.includes('step="any"') || Number.isInteger(n));
  };
  return result;
}
function setup(options = {}, {async = false, old = false} = {}) {
  const source = '# Current source', calls = [], patches = [], jobs = [];
  const bindings = {renderHtmlConfiguredAdvanced() {
    const bytes = new TextEncoder().encode('<html><head></head><body>Preview</body></html>');
    return {bytes, mimeType: 'text/html', diagnosticsJson: () => '[]', free() {}};
  }};
  for (const [name, arity] of [['renderPdfConfiguredMulti', 28], ['renderPdfConfiguredPage', 29],
    ['renderPdfConfiguredRunning', 33], ['renderPdfConfiguredRunningImages', 36]]) {
    if (old && name.includes('Running')) continue;
    const render = (...args) => {
      calls.push({name, args}); const bytes = new TextEncoder().encode('%PDF-ADAPTER');
      return {bytes, mimeType: 'application/pdf', diagnosticsJson: () => '[]', free() {}};
    };
    Object.defineProperty(render, 'length', {value: arity}); bindings[name] = render;
  }
  const payload = {version: 1, options, images: [{destination: 'logo.png', bytes: 'AQID'}], fonts: []};
  const renderer = createNativeWorkspaceRenderer(bindings, payload);
  let active = null;
  const native = {
    get settings() {return renderer.settings;}, get imageAssets() {return renderer.imageAssets;},
    get settingsPending() {return active !== null;},
    settingsMode: async ? 'worker' : 'synchronous',
    applySettings(patch) {patches.push(patch); const tx = renderer.stageSettings(patch); tx.html(source); tx.commit(); return renderer.settings;},
    applySettingsAsync(patch, _source, _preview, _display, current) {
      patches.push(patch); const tx = renderer.stageSettings(patch);
      let resolve, reject; const promise = new Promise((yes, no) => {resolve = yes; reject = no;});
      const job = {tx, current, resolve, reject, cancelled: false}; active = job; jobs.push(job); return promise;
    },
    cancelSettings() {
      if (active) {const job = active; active = null; job.cancelled = true; job.reject(Error('Cancelled settings'));}
    },
  };
  const controls = new Map(), fields = new Map();
  const exportButton = node(); controls.set('btn-export-pdf', exportButton);
  const header = node(); header.querySelector = selector => controls.get(selector.slice(1)) ?? null;
  exportButton.parentNode = {insertBefore(item) {controls.set(item.id, item);}};
  let dialog = null, form = null, status = null;
  const document = {
    title: 'Document', documentElement: {},
    body: {appendChild(item) {dialog = item;}},
    querySelector(selector) {
      if (selector === 'body > .fmd-app-header') return header;
      if (selector === 'body > dialog#fmd-document-settings') return dialog;
      return null;
    },
    createElement(tag) {
      const item = node();
      if (tag === 'dialog') {
        Object.defineProperty(item, 'innerHTML', {set(markup) {
          fields.clear();
          for (const [, kind, attributes, name] of markup.matchAll(/<(input|select)\b([^>]*\bname="([^"]+)"[^>]*)>/g))
            fields.set(name, field(kind, attributes));
          form = node(); status = node();
          const submit = node(), cancel = node(), images = node();
          form.elements = {namedItem: name => fields.get(name) ?? null};
          form.querySelector = selector => ({'[data-status]': status, 'button[type="submit"]': submit,
            '[data-cancel]': cancel, '#fmd-running-images': images})[selector] ?? null;
          form.querySelectorAll = selector => selector === '[data-page]'
            ? ['orientation', 'width', 'height', 'top', 'right', 'bottom', 'left'].map(name => fields.get(name)) : [];
          form.reportValidity = () => [...fields.values()].every(input => input.valid());
          item.querySelector = selector => selector === 'form' ? form : null;
          item.querySelectorAll = () => [];
        }});
      }
      return item;
    },
  };
  const context = Function('native', 'document', 'source', `
    let settingsBaseline = null, settingsController = null, documentLab = null, pendingExport = null;
    let previewSuspended = false, previewComposing = false, renderRevision = 0, pendingRender = null;
    let debounceTimer = null, lastRenderedSource = null, sourceAnchor = 0;
    const publicationFormats = {}, preview = {setAttribute() {}}, saveStatus = {textContent: ''};
    const currentSource = () => source, displaySettings = () => ({}), clearTimeout = () => {};
    const nativeNotice = () => '', updateStats = () => {}, renderCurrent = () => {}, invalidatePreview = () => {};
    const attempt = run => run();
    ${install}
    installSettings();
    return {get settingsController() {return settingsController;}};
  `)(native, document, source);
  return {
    renderer, payload, calls, patches, jobs, context,
    field(name) {assert.ok(fields.has(name), 'Missing field ' + name); return fields.get(name);},
    get status() {return status.textContent;}, get dialog() {return dialog;},
    open() {controls.get('btn-document-settings').dispatch('click');},
    apply() {form.dispatch('submit');},
    cancel() {form.querySelector('[data-cancel]').dispatch('click');},
    edit(name, value, {event = true} = {}) {
      const input = fields.get(name); assert.ok(input, 'Missing field ' + name);
      if (typeof value === 'boolean') input.checked = value; else input.value = value;
      if (event) form.dispatch('input');
    },
    images() {return form.querySelector('#fmd-running-images').children.map(child => child.value);},
    finish(job = active) {
      if (!job || job.cancelled) return;
      if (active === job) active = null;
      if (!job.current()) {job.reject(Error('Draft changed before publication')); return;}
      job.tx.html(source); job.tx.commit(); job.resolve(renderer.settings);
    },
  };
}

test('running settings controls preserve defaults and exact unedited templates', () => {
  for (const options of [{}, {running: {header: {left: 'Line one\r\nLine two'},
    footer: {image: {dest: 'logo.png'}, rule: false}, skipFirstPage: false}}]) {
    const f = setup(options), before = f.renderer.settings;
    f.open(); assert.deepEqual(f.images(), ['logo.png']); f.apply();
    assert.equal(f.renderer.settings, before); assert.equal(f.patches.length, 0); assert.equal(f.dialog.open, false);
    if (options.running) assert.equal(f.renderer.settings.running.header.left, 'Line one\r\nLine two');
    else assert.equal(Object.hasOwn(f.renderer.settings, 'running'), false);
  }
});

test('running authoring commits text, rules, image placement and first-page omission together', () => {
  const f = setup(); f.open();
  for (const [name, value] of Object.entries({headerLeft: '{chapter}', headerRight: '{section}', headerRule: true,
    headerImage: 'logo.png', headerImagePosition: 'right', headerImageHeight: '20',
    footerCenter: '{page} / {pages}', skipFirstPage: true})) f.edit(name, value);
  f.apply(); assert.equal(f.dialog.open, false);
  assert.deepEqual(f.renderer.settings.running, {
    header: {left: '{chapter}', right: '{section}', rule: true, image: {dest: 'logo.png', position: 'right', heightPt: 20}},
    footer: {center: '{page} / {pages}'}, skipFirstPage: true,
  });
  f.renderer.pdf('Current');
  const {name, args} = f.calls.at(-1);
  assert.equal(name, 'renderPdfConfiguredRunningImages');
  assert.deepEqual(args[28], ['{chapter}', '', '{section}', '', '{page} / {pages}', '']);
  assert.deepEqual(args.slice(29, 33), [true, false, true, ['logo.png', '']]);
  assert.deepEqual([...args[33]], [1, 0]); assert.deepEqual([...args[34]], [20, 0]);
  f.open(); assert.equal(f.field('headerLeft').value, '{chapter}'); assert.equal(f.field('headerImageHeight').value, '20');
  const saved = f.renderer.settings; f.apply(); assert.equal(f.renderer.settings, saved);
});

test('editing one band preserves unedited spelling and clearing bands returns to default dispatch', () => {
  const f = setup({running: {header: {left: '{chapter}', image: {dest: 'logo.png'}},
    footer: {right: 'Exact\r\nfooter', rule: false}, skipFirstPage: false}});
  f.open(); f.edit('headerLeft', '{section}'); f.apply();
  assert.equal(f.renderer.settings.running.footer.right, 'Exact\r\nfooter');
  assert.deepEqual(f.renderer.settings.running.header.image, {dest: 'logo.png'});
  f.open(); f.edit('headerImage', ''); f.edit('headerLeft', ''); f.edit('footerRight', ''); f.apply();
  f.renderer.pdf('Current'); assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti');
  assert.equal(Object.hasOwn(f.renderer.settings.running.header, 'image'), false);
});

test('invalid running drafts and unavailable bindings retain committed settings', () => {
  for (const edit of [f => f.edit('skipFirstPage', true), f => f.edit('headerLeft', 'é'.repeat(2049)),
    f => f.edit('headerImageHeight', '20')]) {
    const f = setup(), before = f.renderer.settings; f.open(); edit(f); f.apply();
    assert.equal(f.renderer.settings, before); assert.equal(f.dialog.open, true); assert.match(f.status, /Settings not applied/);
    f.cancel(); assert.equal(f.renderer.settings, before);
  }
  const f = setup({}, {old: true}), before = f.renderer.settings;
  f.open(); f.edit('headerLeft', '{chapter}'); f.apply();
  assert.equal(f.renderer.settings, before); assert.equal(f.dialog.open, true); assert.match(f.status, /matching WASM bindings/);
});

test('running draft changes and cancellation fence asynchronous settings publication', async () => {
  const f = setup({}, {async: true}), before = f.renderer.settings;
  f.open(); f.edit('headerLeft', 'Captured'); f.apply();
  const first = f.jobs[0]; assert.equal(f.renderer.settings, before); assert.equal(f.context.settingsController.busy, true);
  f.edit('headerLeft', 'New draft'); f.finish(first); await Promise.resolve(); await Promise.resolve();
  assert.equal(f.renderer.settings, before); assert.equal(f.dialog.open, true);
  assert.equal(f.context.settingsController.busy, false);
  f.apply(); const second = f.jobs[1];
  f.edit('footerCenter', '{page}', {event: false}); f.finish(second);
  await Promise.resolve(); await Promise.resolve();
  assert.equal(f.renderer.settings, before); assert.equal(f.dialog.open, true);
  assert.match(f.status, /Draft changed/);
  f.apply(); f.finish(); await Promise.resolve(); await Promise.resolve();
  assert.equal(f.dialog.open, false); assert.equal(f.renderer.settings.running.header.left, 'New draft');
  assert.equal(f.renderer.settings.running.footer.center, '{page}');
});
