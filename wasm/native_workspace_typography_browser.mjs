// Real-browser acceptance for the shipped settings UI, renderer factory,
// saved-document boot and disposable worker. Native calls are explicit ABI
// adapters and instantiate an empty WASM module: this is not PDF layout proof.
// FMD_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs FMD_CHROMIUM_BIN=/path/to/chromium
// node wasm/native_workspace_typography_browser.mjs
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {bootNativeWorkspace, createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

const {chromium} = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : 'playwright');
const controller = await readFile(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const source = '\ufeff# Typesetting\r\n\r\nExact café 😀 and ![Chart](chart.png)\r\n';
const inert = value => JSON.stringify(value).replace(/</g, '\\u003c');
const bindings = `
export default async function init({module_or_path}) { await WebAssembly.instantiate(module_or_path); }
function result(text,mimeType) { const bytes=new TextEncoder().encode(text);
  return {bytes,mimeType,diagnosticsJson:()=> '[]',free(){bytes.fill(0);}}; }
function escape(text) { return text.replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c])); }
export function renderHtmlConfiguredAdvanced(...a) {
  if(typeof document!=='undefined') throw Error('Rendering escaped the background worker');
  if(a[3]==='Slow settings') {const until=Date.now()+900;while(Date.now()<until){}}
  return result('<!doctype html><html><head></head><body><pre>'+escape(a[0])+'</pre></body></html>','text/html');
}
function pdf(a,name) {
  if(typeof document!=='undefined') throw Error('PDF escaped the background worker');
  if(a[0].includes('Refuse exact') && a.some(value=>typeof value==='string' && value.includes('strict-pagination')))
    throw JSON.stringify({code:'PAGINATION_INFEASIBLE',reason:'infeasible',message:'The caption cannot fit on one page'});
  return result('%PDF-ADAPTER\\n'+JSON.stringify({name,args:a.map(value=>ArrayBuffer.isView(value)?Array.from(value):value)}),'application/pdf');
}
export const renderPdfConfiguredMulti=Object.defineProperty((...a)=>pdf(a,'multi'),'length',{value:28});
export const renderPdfConfiguredPage=Object.defineProperty((...a)=>pdf(a,'page'),'length',{value:29});
export const renderPdfConfiguredGrid=Object.defineProperty((...a)=>pdf(a,'grid'),'length',{value:37});
`;
function fixture({options = {}, old = false, markdown = source} = {}) {
  const payload = {version: 1, wasm: 'AGFzbQEAAAA=',
    bindings: old ? bindings.replace('value:28', 'value:27').replace('value:29', 'value:28').replace('value:37', 'value:36') : bindings,
    options: {font: 'serif', darkMode: 'disabled', fontScale: 1.25, title: 'Original',
      author: 'Exact\r\nAuthor', lang: 'fr', metadataEpochSeconds: 0, ...options},
    images: [{destination: 'chart.png', bytes: 'AQID'}],
    fonts: [{slot: 'body-regular', bytes: 'BAUG', weight: 555}]};
  const ids = ['toggle-view', 'zoom-out', 'zoom-reset', 'zoom-in', 'theme-toggle',
    'stats-toggle', 'save-markdown', 'save-html', 'export-pdf'];
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><title>Original</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; frame-src 'self' about:; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'">
<style>body{font:16px sans-serif;margin:16px}button{padding:8px}textarea{width:90%;height:100px}</style></head><body>
<header class="fmd-app-header"><span class="fmd-title">Original</span><div class="fmd-toolbar">
${ids.map(id => `<button id="btn-${id}">${id}</button>`).join('')}
<span id="view-mode-icon"></span><span id="view-mode-label"></span></div></header>
<div id="fmd-app-body" class="view-split"><section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status"></span></div><textarea id="fmd-editor"></textarea></section><main><div id="fmd-content">Initial</div></main></div>
<div id="stats-drawer">${['words','chars','read-time','readability'].map(name => `<span id="stat-${name}"></span>`).join('')}<button id="btn-stats-close">Close</button></div>
<script id="fmd-raw-source" type="application/json">${inert(markdown)}</script>
<script id="fmd-native-runtime" type="application/json">${inert(payload)}</script>
<script>;(${bootNativeWorkspace.toString()})(${createNativeWorkspaceRenderer.toString()},${createWorkspacePreviewWorker.toString()});</script>
<script>function parseMarkdownClient(){throw Error('Unexpected reduced parser');}
${controller}</script></body></html>`;
}

const browser = await chromium.launch({headless: true, executablePath: process.env.FMD_CHROMIUM_BIN || undefined,
  args: ['--no-sandbox', '--disable-dev-shm-usage']});
const context = await browser.newContext({acceptDownloads: true, viewport: {width: 1300, height: 1000}});
context.setDefaultTimeout(15000);
await context.setOffline(true);
const requests = [], errors = [], checks = [];
context.on('request', request => {if (/^https?:/.test(request.url())) requests.push(request.url());});
const check = label => {checks.push(label); console.log(`ok ${checks.length} - ${label}`);};
const settings = page => page.evaluate(() => window.__fmdNativeRuntime.settings);
const savedPayload = page => page.locator('#fmd-native-runtime').textContent();
const field = (page, name) => page.locator(`#fmd-document-settings [name="${name}"]`);
const panel = async page => {await page.locator('#btn-document-settings').click();};
const cancel = page => page.locator('#fmd-document-settings [data-cancel]').click();
const apply = page => page.locator('#fmd-document-settings button[type="submit"]').click();
const settled = page => page.waitForFunction(() => !window.__fmdNativeRuntime.settingsPending
  && !document.querySelector('#fmd-document-settings button[type="submit"]').disabled);
const dirty = page => page.evaluate(() => {
  const event = new Event('beforeunload', {cancelable: true}); window.dispatchEvent(event); return event.defaultPrevented;
});
async function load(html) {
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(String(error)));
  await page.setContent(html);
  await page.waitForFunction(() => document.querySelector('#btn-document-settings')?.disabled === false
    && document.querySelector('#fmd-content > iframe') && document.querySelector('#fmd-content').getAttribute('aria-busy') !== 'true');
  return page;
}
async function download(page, id) {
  const pending = page.waitForEvent('download');
  // Keyboard Save remains available while a modal draft is open.
  await page.evaluate(id => document.getElementById(id).click(), id);
  const item = await pending; assert.equal(await item.failure(), null);
  return readFile(await item.path(), 'utf8');
}
async function pdf(page) {
  const data = await download(page, 'btn-export-pdf');
  assert(data.startsWith('%PDF-ADAPTER\n')); return JSON.parse(data.slice('%PDF-ADAPTER\n'.length));
}

try {
  const page = await load(fixture()), baseline = await settings(page), rawBaseline = await savedPayload(page);
  assert.equal(await dirty(page), false);
  await panel(page);
  for (const name of ['homogeneous', 'antiriver', 'pareto']) assert.equal(await field(page, name).isChecked(), false);
  assert.equal(await field(page, 'pagination').inputValue(), 'default');
  assert.equal(await field(page, 'microtype').inputValue(), 'disabled');
  assert.equal(await field(page, 'baselineGridPt').inputValue(), '');
  assert.equal(await field(page, 'fitToPages').inputValue(), '');
  await apply(page); await settled(page);
  assert.deepEqual(await settings(page), baseline); assert.equal(await savedPayload(page), rawBaseline);
  assert.equal(await dirty(page), false);
  check('default controls and unchanged Apply preserve omitted settings, saved payload and clean state');

  await panel(page);
  for (const name of ['homogeneous', 'antiriver', 'pareto']) await field(page, name).check();
  await field(page, 'pagination').selectOption('strict'); await field(page, 'microtype').selectOption('expansion');
  await field(page, 'baselineGridPt').fill('13.375'); await apply(page); await settled(page);
  assert.equal(await page.locator('#fmd-document-settings').isVisible(), false);
  const configured = await settings(page);
  assert.deepEqual(configured.typography, {homogeneous: true, antiriver: true, pareto: true});
  assert.equal(configured.optimalPagination, true); assert.equal(configured.strictPagination, true);
  assert.equal(configured.baselineGridPt, 13.375); assert.equal(configured.microtype, 'expansion');
  assert.equal(await dirty(page), true); assert.equal(configured.author, 'Exact\r\nAuthor');
  const output = await pdf(page);
  assert.equal(output.name, 'grid'); assert.equal(output.args.length, 37); assert.equal(output.args[0], source);
  assert.equal(output.args[35], 'homogeneous,antiriver,pareto,optimal-pagination,strict-pagination,expansion');
  assert.equal(output.args[36], 13.375); assert.deepEqual(output.args[8], ['chart.png']);
  assert.deepEqual(output.args[9], [1,2,3]); assert.deepEqual(output.args[11], [4,5,6]); assert.equal(output.args[16][0], 555);
  check('quality controls commit atomically and reach the complete PDF worker ABI with exact source, fonts and images');

  const saved = await download(page, 'btn-save-html'), reopened = await load(saved);
  assert.deepEqual(await settings(reopened), configured);
  assert.equal(await dirty(reopened), false);
  await panel(reopened); assert.equal(await field(reopened, 'pagination').inputValue(), 'strict');
  assert.equal(await field(reopened, 'baselineGridPt').inputValue(), '13.375');
  for (const name of ['homogeneous', 'antiriver', 'pareto']) assert.equal(await field(reopened, name).isChecked(), true);
  await apply(reopened); await settled(reopened); assert.deepEqual(await settings(reopened), configured);
  assert.deepEqual((await pdf(reopened)).args, output.args);
  check('Save HTML and a fresh browser page retain committed typesetting and reconstruct working controls');

  await panel(reopened);
  for (const name of ['homogeneous', 'antiriver', 'pareto']) await field(reopened, name).uncheck();
  await field(reopened, 'baselineGridPt').fill(''); await field(reopened, 'fitToPages').fill('3');
  await field(reopened, 'pagination').selectOption('optimal'); await field(reopened, 'microtype').selectOption('protrusion');
  await apply(reopened); await settled(reopened);
  const fitted = await pdf(reopened);
  assert.equal(fitted.name, 'multi'); assert.equal(fitted.args.length, 28);
  assert.equal(fitted.args[25], 3); assert.equal(fitted.args[26], true); assert.equal(fitted.args[27], 'optimal-pagination');
  assert.equal((await settings(reopened)).strictPagination, false);
  assert.equal(Object.hasOwn(await settings(reopened), 'baselineGridPt'), false);
  check('authors can switch from fixed grids and exact layout to adaptive fitting and optical margins');

  await panel(reopened); await field(reopened, 'fitToPages').fill('');
  await field(reopened, 'pagination').selectOption('default'); await field(reopened, 'microtype').selectOption('disabled');
  await apply(reopened); await settled(reopened);
  assert.equal((await pdf(reopened)).args.length, 27);
  assert.equal(Object.hasOwn(await settings(reopened), 'fitToPages'), false);
  check('clearing optional targets and disabling quality choices restores the original PDF call shape');

  const beforeConflict = await savedPayload(reopened), preview = await reopened.locator('#fmd-content').innerHTML();
  await panel(reopened); await field(reopened, 'baselineGridPt').fill('12'); await field(reopened, 'fitToPages').fill('2');
  await apply(reopened); await settled(reopened);
  assert.match(await reopened.locator('[data-status]').textContent(), /cannot be combined/);
  assert.equal(await savedPayload(reopened), beforeConflict); assert.equal(await reopened.locator('#fmd-content').innerHTML(), preview);
  assert.equal(await reopened.locator('#fmd-document-settings').isVisible(), true); await cancel(reopened);
  check('incompatible grid and fitting requests remain an editable draft without changing saved settings or preview');

  for (const [name, value] of [['baselineGridPt','0.5'],['baselineGridPt','145'],['fitToPages','0'],['fitToPages','1.5'],['fitToPages','4294967296']]) {
    await panel(reopened); await field(reopened, name).fill(value); await apply(reopened);
    assert.equal(await field(reopened, name).evaluate(node => node.validity.valid), false);
    assert.equal(await savedPayload(reopened), beforeConflict); await cancel(reopened);
  }
  check('number controls enforce the native grid and unsigned integer page-target bounds');

  const aliasPage = await load(fixture({options: {typography: {pareto: false}, microtype: 'expansion',
    microtypeProtrusion: true, strictPagination: true, optimalPagination: false, baselineGridPt: 0}}));
  const aliasBefore = await savedPayload(aliasPage);
  await panel(aliasPage); assert.equal(await field(aliasPage, 'microtype').inputValue(), 'expansion');
  assert.equal(await field(aliasPage, 'pagination').inputValue(), 'strict'); await apply(aliasPage); await settled(aliasPage);
  assert.equal(await savedPayload(aliasPage), aliasBefore);
  await panel(aliasPage); await field(aliasPage, 'homogeneous').check(); await field(aliasPage, 'microtype').selectOption('disabled');
  await apply(aliasPage); await settled(aliasPage);
  const aliases = await settings(aliasPage);
  assert.equal(aliases.microtypeProtrusion, false); assert.equal(aliases.microtype, 'disabled');
  assert.deepEqual(aliases.typography, {pareto: false, homogeneous: true});
  assert.equal(aliases.optimalPagination, false); assert.equal(aliases.strictPagination, true); assert.equal(aliases.baselineGridPt, 0);
  const protrusionPage = await load(fixture({options: {microtype: 'disabled', microtypeProtrusion: true}}));
  await panel(protrusionPage); assert.equal(await field(protrusionPage, 'microtype').inputValue(), 'protrusion');
  await field(protrusionPage, 'microtype').selectOption('disabled'); await apply(protrusionPage); await settled(protrusionPage);
  assert.equal((await pdf(protrusionPage)).args[26], false);
  check('effective saved aliases display correctly; explicit changes clear protrusion without rewriting unedited nested options');

  const oldPage = await load(fixture({old: true})), oldBefore = await savedPayload(oldPage);
  for (const configure of [() => field(oldPage, 'pagination').selectOption('strict'),
    () => field(oldPage, 'pareto').check(), () => field(oldPage, 'baselineGridPt').fill('12')]) {
    await panel(oldPage); await configure(); await apply(oldPage); await settled(oldPage);
    assert.match(await oldPage.locator('[data-status]').textContent(), /matching WASM bindings/);
    assert.equal(await savedPayload(oldPage), oldBefore); await cancel(oldPage);
  }
  check('older binaries refuse newly selected PDF capabilities before committing the draft');

  const racePage = await load(fixture()), raceBefore = await savedPayload(racePage);
  for (const name of ['homogeneous', 'antiriver', 'pareto', 'pagination', 'microtype', 'baselineGridPt', 'fitToPages']) {
    await panel(racePage); await field(racePage, 'title').fill('Slow settings'); await apply(racePage);
    await racePage.waitForFunction(() => window.__fmdNativeRuntime.settingsPending);
    // No input/change event: the final draft-identity comparison itself must
    // include every added control, not just rely on event cancellation.
    await field(racePage, name).evaluate(node => {
      if (node.type === 'checkbox') node.checked = true;
      else node.value = node.name === 'pagination' ? 'strict' : node.name === 'microtype' ? 'expansion' : '12';
    });
    await settled(racePage);
    assert.equal(await savedPayload(racePage), raceBefore);
    assert.match(await racePage.locator('[data-status]').textContent(), /not applied/); await cancel(racePage);
  }
  check('silent changes to each new control invalidate an in-flight settings result before publication');

  await panel(racePage); await field(racePage, 'title').fill('Slow settings'); await field(racePage, 'pareto').check();
  await apply(racePage); await racePage.waitForFunction(() => window.__fmdNativeRuntime.settingsPending);
  await cancel(racePage); await settled(racePage); assert.equal(await savedPayload(racePage), raceBefore);
  await panel(racePage); await field(racePage, 'title').fill('Slow settings'); await field(racePage, 'microtype').selectOption('expansion');
  await apply(racePage); await racePage.waitForFunction(() => window.__fmdNativeRuntime.settingsPending);
  const pendingCopy = await download(racePage, 'btn-save-html'), pendingReopened = await load(pendingCopy);
  assert.deepEqual(await settings(pendingReopened), baseline);
  if (await racePage.locator('#fmd-document-settings').isVisible()) await cancel(racePage);
  await settled(racePage); assert.equal(await savedPayload(racePage), raceBefore);
  check('cancellation and Save HTML during preflight preserve committed settings instead of the unfinished quality draft');

  const refusePage = await load(fixture({markdown: '# Refuse exact', options: {strictPagination: true}}));
  let downloads = 0; refusePage.on('download', () => downloads++);
  await refusePage.locator('#btn-export-pdf').click();
  await refusePage.waitForFunction(() => document.querySelector('#fmd-export-status').textContent.includes('caption cannot fit'));
  assert.equal(downloads, 0);
  await panel(refusePage); await field(refusePage, 'pagination').selectOption('default'); await apply(refusePage); await settled(refusePage);
  assert.equal((await pdf(refusePage)).args.length, 27); assert.equal(downloads, 1);
  check('a strict-pagination refusal downloads no substitute PDF and changing page policy lets the next export recover');

  assert.deepEqual(requests, []); assert.deepEqual(errors, []);
  check('complete browser workflow makes no external requests and reports no page runtime errors');
  console.log(JSON.stringify({passed: checks.length, checks, browser: browser.version(),
    native: 'explicit ABI adapters and an empty WASM module; not native PDF layout',
    controller: 'production', runtime: 'production', worker: 'production disposable Blob workers',
    externalRequests: requests, pageErrors: errors}, null, 2));
} finally {await browser.close();}
