// Production workspace capture, saved-project codec and PDF dispatch. The native
// bindings are explicit ABI adapters; these tests do not execute a WASM renderer
// or establish PDF page layout. No DOM, worker or filesystem is required.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createNativeSourceProjectCodec} from './native_source_project.mjs';
import {normalizePdfRunning} from './pdf_running.mjs';
import {createWorkspaceExporterWithLoader, createNativeSourceProject, readNativeSourceProject} from './interactive_export.mjs';

const source = '\ufeff# Chapter\r\n\r\nA café and ![Chart](chart.png)\r';
const geometry = [792, 612, 24, 36, 48, 60];
const bands = {
  header: {left: '{chapter}', right: '{section}', rule: true},
  footer: {center: '{page} / {pages}', rule: false},
  skipFirstPage: true,
};
const logos = {
  header: {...bands.header, image: {dest: 'chart.png', position: 'right', heightPt: 19}},
  footer: {...bands.footer, image: {dest: 'footer.png'}},
  skipFirstPage: true,
};
const plain = value => JSON.parse(JSON.stringify(value));
function payload(options = {}) {
  return {version: 1, options: {font: 'serif', fontScale: 1.25, title: 'Owned', author: 'Author',
    metadataEpochSeconds: 0, lang: 'fr', ...options},
    images: [{destination: 'chart.png', bytes: 'AQID'}, {destination: 'footer.png', bytes: 'BAUG'}],
    fonts: [{slot: 'body-regular', weight: 555, bytes: 'BwgJ'}]};
}
function output(text, mimeType = 'application/pdf') {
  const bytes = new TextEncoder().encode(text);
  return {bytes, mimeType, diagnosticsJson: () => '[]', free() {bytes.fill(0);}};
}
function fixture() {
  const calls = [];
  const bindings = {
    renderHtmlConfiguredAdvanced(...args) {
      calls.push({name: 'html', args});
      return output('<html><head></head><body>Preview</body></html>', 'text/html');
    },
    renderInteractiveHtmlConfigured(markdown) {
      calls.push({name: 'shell'});
      return output('<html><head></head><body><div class="fmd-content" id="fmd-content"></div>\n  </main>\n</div>\n'
        + '<script type="application/json" id="fmd-raw-source">' + JSON.stringify(markdown) + '</script>\n'
        + '<script>\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\n</script></body></html>', 'text/html');
    },
  };
  for (const [name, arity] of [['renderPdfConfiguredMulti', 28], ['renderPdfConfiguredPage', 29],
    ['renderPdfConfiguredRunning', 33], ['renderPdfConfiguredRunningImages', 36],
    ['renderPdfConfiguredGrid', 37], ['renderPdfConfiguredArchival', 39]]) {
    const render = (...args) => {calls.push({name, args}); return output('%PDF-ADAPTER\n' + args[0]);};
    Object.defineProperty(render, 'length', {value: arity, configurable: true}); bindings[name] = render;
  }
  return {bindings, calls};
}
function saved(rendered) {
  return JSON.parse(rendered.text().split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0]);
}
function checkCommon(args) {
  assert.equal(args[0], source); assert.equal(args[1], 'serif'); assert.equal(args[3], 'Owned');
  assert.equal(args[4], 'Author'); assert.equal(args[5], 0); assert.equal(args[21], 1.25); assert.equal(args[22], 'fr');
  assert.deepEqual(args[8], ['chart.png', 'footer.png']);
  assert.deepEqual([...args[9]], [1, 2, 3, 4, 5, 6]); assert.deepEqual([...args[10]], [3, 3]);
  assert.deepEqual([...args[11]], [7, 8, 9]); assert.equal(args[16][0], 555);
}

test('running text, images, paper, typography, grid and archival modes reach their exact native ABIs', () => {
  for (const running of [bands, logos]) for (const page of [undefined, geometry]) {
    const image = running === logos;
    for (const [options, expected, count] of [
      [{}, image ? 'renderPdfConfiguredRunningImages' : 'renderPdfConfiguredRunning', image ? 35 : 32],
      [{footnoteBacklinks: true}, image ? 'renderPdfConfiguredRunningImages' : 'renderPdfConfiguredRunning', image ? 36 : 33],
      [{baselineGridPt: 13.375}, 'renderPdfConfiguredGrid', 37],
      [{pdfA: '2b', pdfAStrict: true}, 'renderPdfConfiguredArchival', 38],
      [{pdfA: '2b', pdfAStrict: true, baselineGridPt: 13.375}, 'renderPdfConfiguredArchival', 39],
    ]) {
      const f = fixture();
      // The selected complete ABI already accepts paper; do not require a
      // separate page-only binding which is never invoked for this request.
      delete f.bindings.renderPdfConfiguredPage;
      const renderer = createNativeWorkspaceRenderer(f.bindings, payload({running, pageGeometry: page, ...options}));
      const bytes = renderer.pdf(source), {name, args} = f.calls.at(-1);
      assert.equal(name, expected); assert.equal(args.length, count); checkCommon(args);
      assert.deepEqual([...args[27]], page ?? []);
      assert.deepEqual(args[28], ['{chapter}', '', '{section}', '', '{page} / {pages}', '']);
      assert.deepEqual(args.slice(29, 32), [true, false, true]);
      if (image || options.baselineGridPt || options.pdfA) {
        assert.deepEqual(args[32], image ? ['chart.png', 'footer.png'] : ['', '']);
        assert.deepEqual([...args[33]], image ? [1, 0] : [0, 0]);
        assert.deepEqual([...args[34]], image ? [19, 0] : [0, 0]);
      }
      if (options.footnoteBacklinks) assert.equal(args[image ? 35 : 32], 'footnote-backlinks');
      if (options.pdfA) assert.deepEqual(args.slice(36), options.baselineGridPt ? ['2b', true, 13.375] : ['2b', true]);
      else if (options.baselineGridPt) assert.equal(args[36], 13.375);
      assert.equal(new TextDecoder().decode(bytes), '%PDF-ADAPTER\n' + source, 'native output survives freeing the result');
    }
  }
});

test('empty running settings preserve legacy dispatch and omission across saved generations', () => {
  for (const running of [undefined, {}, {header: {}}, {footer: {left: '', rule: false}}, {skipFirstPage: false}]) {
    const f = fixture(), p = payload(running === undefined ? {} : {running});
    delete f.bindings.renderPdfConfiguredRunning; delete f.bindings.renderPdfConfiguredRunningImages;
    const renderer = createNativeWorkspaceRenderer(f.bindings, p);
    renderer.pdf(source);
    assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti'); assert.equal(f.calls.at(-1).args.length, 27);
    assert.equal(Object.hasOwn(renderer.settings, 'running'), running !== undefined);
    const again = createNativeWorkspaceRenderer(f.bindings, JSON.parse(JSON.stringify(p)));
    assert.deepEqual(plain(again.settings), plain(renderer.settings));
  }
  for (const running of [{header: {rule: true}}, {footer: {image: {dest: 'chart.png'}}}]) {
    const f = fixture();
    createNativeWorkspaceRenderer(f.bindings, payload({running})).pdf(source);
    assert.equal(f.calls.at(-1).name, running.header ? 'renderPdfConfiguredRunning' : 'renderPdfConfiguredRunningImages');
  }
});

test('old running ABIs admit their original arguments but never discard a new request', () => {
  for (const [running, method, arity] of [[bands, 'renderPdfConfiguredRunning', 32], [logos, 'renderPdfConfiguredRunningImages', 35]]) {
    const f = fixture();
    Object.defineProperty(f.bindings[method], 'length', {value: arity});
    createNativeWorkspaceRenderer(f.bindings, payload({running})).pdf(source);
    assert.equal(f.calls.at(-1).args.length, arity);
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, payload({running, footnoteBacklinks: true})),
      {code: 'UNSUPPORTED_WASM_PACKAGE'});
  }
  for (const [options, name, arity] of [
    [{running: bands}, 'renderPdfConfiguredRunning', null],
    [{running: bands}, 'renderPdfConfiguredRunning', 31],
    [{running: logos}, 'renderPdfConfiguredRunningImages', null],
    [{running: logos}, 'renderPdfConfiguredRunningImages', 34],
    [{running: logos, baselineGridPt: 12}, 'renderPdfConfiguredGrid', 36],
    [{running: logos, pdfA: '2b'}, 'renderPdfConfiguredArchival', 37],
    [{running: logos, pdfA: '2b', baselineGridPt: 12}, 'renderPdfConfiguredArchival', 38],
  ]) {
    const f = fixture(), p = payload(options); let assets = 0;
    if (arity === null) delete f.bindings[name];
    else Object.defineProperty(f.bindings[name], 'length', {value: arity});
    Object.defineProperty(p, 'images', {get() {assets++; throw Error('Resource read before capability admission');}});
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, p), {code: 'UNSUPPORTED_WASM_PACKAGE'});
    assert.equal(assets, 0); assert.equal(f.calls.length, 0);
  }
});

test('owned running settings survive staging, commit, rollback and repeated project reopening', () => {
  const f = fixture(), initial = plain(logos), p = payload({running: initial});
  const renderer = createNativeWorkspaceRenderer(f.bindings, p);
  initial.header.left = 'caller mutation'; initial.header.image.dest = 'other.png';
  assert.equal(renderer.settings.running.header.left, '{chapter}');
  assert.equal(renderer.settings.running.header.image.dest, 'chart.png');
  for (const record of [renderer.settings.running, renderer.settings.running.header, renderer.settings.running.header.image])
    assert.ok(Object.isFrozen(record));
  const before = renderer.settings, images = renderer.imageAssets;
  const patch = {running: {header: {left: 'NEW {section}'}, footer: {center: '{pages}'}}};
  const tx = renderer.stageSettings(patch);
  patch.running.header.left = 'retarget'; tx.html(source);
  assert.equal(renderer.settings, before); renderer.pdf(source); assert.equal(f.calls.at(-1).args[28][0], '{chapter}');
  tx.commit(); renderer.pdf(source); assert.equal(f.calls.at(-1).args[28][0], 'NEW {section}');
  assert.deepEqual(renderer.imageAssets, images, 'settings do not replace image resource descriptions');
  tx.rollback(); assert.equal(renderer.settings, before);
  const codec = createNativeSourceProjectCodec();
  let data = {source, options: renderer.settings, images: p.images, fonts: p.fonts};
  for (let generation = 0; generation < 3; generation++) {
    data = codec.decode(codec.encode(data).bytes);
    assert.equal(data.source, source); assert.deepEqual(plain(data.options.running), logos);
    const reopened = createNativeWorkspaceRenderer(f.bindings, {version: 1, ...data});
    reopened.pdf(source); assert.equal(f.calls.at(-1).args[28][0], '{chapter}');
    const editable = codec.renderOptions(data);
    assert.equal(Object.isFrozen(editable.running), false);
    editable.running.header.image.dest = 'host-owned.png';
    editable.running.footer.center = 'Changed';
    assert.equal(data.options.running.header.image.dest, 'chart.png');
    assert.equal(data.options.running.footer.center, '{page} / {pages}');
  }
});

test('clearing running settings is transactional and restores default PDF dispatch', () => {
  const f = fixture(), renderer = createNativeWorkspaceRenderer(f.bindings, payload({running: logos}));
  const first = renderer.stageSettings({running: undefined}), second = renderer.stageSettings({running: bands});
  second.commit(); assert.throws(() => first.commit(), /Stale/);
  const clear = renderer.stageSettings({running: undefined});
  clear.commit(); renderer.pdf(source);
  assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredMulti'); assert.equal(f.calls.at(-1).args.length, 27);
  assert.equal(Object.hasOwn(renderer.settings, 'running'), false);
  clear.rollback(); renderer.pdf(source);
  assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredRunning');
});

test('running validation matches the shared contract and never executes nested accessors', () => {
  const codec = createNativeSourceProjectCodec();
  let reads = 0;
  const accessor = key => Object.defineProperty({}, key, {get() {reads++; return 'forbidden';}});
  const malformed = [null, [], 'header', {unknown: true}, {[Symbol('hidden')]: 1},
    accessor('header'), {header: accessor('left')}, {header: null}, {footer: {image: accessor('dest')}},
    {header: {left: false}}, {header: {rule: 'true'}}, {skipFirstPage: 'true'}, {skipFirstPage: true},
    {header: {left: ''}, skipFirstPage: true}, {header: {left: '\ud800'}}, {footer: {right: '\udfff'}},
    {header: {left: 'é'.repeat(2049)}}, {header: {left: 'a'.repeat(4097)}},
    {header: {left: 'x'.repeat(4096), center: 'x'.repeat(4096), right: 'x'.repeat(4096)},
      footer: {left: 'x'.repeat(4096), center: 'x'}},
    {header: {image: {dest: ''}}}, {header: {image: {dest: '  '}}}, {header: {image: {dest: 'x', position: 'center'}}},
    {header: {image: {dest: 'x', heightPt: 1.5}}}, {header: {image: {dest: 'x', heightPt: 0}}},
    {header: {image: {dest: 'x', heightPt: 65536}}}, {header: {image: {dest: 'x', extra: true}}},
  ];
  for (const running of malformed) {
    assert.throws(() => normalizePdfRunning(running));
    const f = fixture();
    assert.throws(() => createNativeWorkspaceRenderer(f.bindings, payload({running})));
    assert.throws(() => codec.encode({source, options: {running}, images: [], fonts: []}));
    assert.equal(f.calls.length, 0);
  }
  assert.equal(reads, 0);
  for (const running of [bands, logos, {header: {left: 'é'.repeat(2048)}},
    {header: {left: 'literal {unknown}\r\n</script>😀', image: {dest: ' chart.png ', heightPt: 65535}}},
    {header: {left: 'x'.repeat(4096), center: 'x'.repeat(4096)},
      footer: {left: 'x'.repeat(4096), center: 'x'.repeat(4096)}}]) {
    const expected = plain(normalizePdfRunning(running)), f = fixture();
    const renderer = createNativeWorkspaceRenderer(f.bindings, payload({running}));
    assert.deepEqual(plain(renderer.settings.running), expected);
    const data = codec.decode(codec.encode({source, options: {running}, images: [], fonts: []}).bytes);
    assert.deepEqual(plain(data.options.running), expected);
  }
});

test('serialized runtime and project factories carry their complete running contract without imports', () => {
  const factory = Function('return (' + createNativeWorkspaceRenderer.toString() + ');')();
  const codec = Function('return (' + createNativeSourceProjectCodec.toString() + ')();')();
  const f = fixture(), p = payload({running: logos, pageGeometry: geometry});
  const renderer = factory(f.bindings, p);
  renderer.pdf(source); assert.equal(f.calls.at(-1).name, 'renderPdfConfiguredRunningImages');
  const data = codec.decode(codec.encode({source, options: renderer.settings, images: p.images, fonts: p.fonts}).bytes);
  assert.deepEqual(plain(data.options.running), logos);
});

test('public project and workspace exporters retain running bands and assets through reopening', async () => {
  const f = fixture(), magic = Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0);
  const exporter = await createWorkspaceExporterWithLoader({wasm: magic, bindings: 'trusted adapter'}, async () => f.bindings);
  const options = {font: 'serif', fontScale: 1.25, running: plain(logos),
    page: {size: 'a4', orientation: 'landscape', margins: {topPt: 24, rightPt: 36, bottomPt: 48, leftPt: 60}},
    pdfImages: [{destination: 'chart.png', bytes: Uint8Array.of(1, 2, 3)}, {destination: 'footer.png', bytes: Uint8Array.of(4, 5, 6)}]};
  const original = createNativeSourceProject(source, options), p = saved(exporter.render(source, options));
  options.running.header.image.dest = 'later.png'; options.running.footer.center = 'later';
  assert.deepEqual(p.options.running, logos);
  const project = readNativeSourceProject(original.bytes);
  assert.deepEqual(project.options.running, logos);
  assert.deepEqual([...project.options.pdfImages[0].bytes], [1, 2, 3]);
  const fromProject = saved(exporter.renderProject(original.bytes));
  assert.deepEqual(fromProject.options.running, logos); assert.deepEqual(fromProject.images, p.images);
  for (const data of [p, fromProject]) {
    const reopened = createNativeWorkspaceRenderer(f.bindings, data);
    reopened.pdf(source);
    assert.deepEqual(f.calls.at(-1).args[28], ['{chapter}', '', '{section}', '', '{page} / {pages}', '']);
    assert.deepEqual(f.calls.at(-1).args[32], ['chart.png', 'footer.png']);
  }
});
