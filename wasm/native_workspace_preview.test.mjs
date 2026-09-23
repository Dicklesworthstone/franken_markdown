// Production bootstrap + complete shipped editor controller. DOM, timers and
// worker transport are explicit adapters; browser tests cover real Blob workers.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {bootNativeWorkspace, createNativeWorkspaceRenderer} from './interactive_runtime.mjs';

const controller = readFileSync(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));
const bindingsSource = `
export const calls = [];
export default async ({module_or_path}) => { await WebAssembly.instantiate(module_or_path); };
export function renderHtmlConfiguredAdvanced(...args) {
  calls.push(args);
  if (args[0] === 'bad settings') throw Error('settings render failed');
  return {mimeType:'text/html',bytes:new TextEncoder().encode('<html><head></head><body>sync:'+args[0]+'</body></html>'),diagnosticsJson:()=> '[]',free(){}};
}
export function renderPdfConfiguredMulti(source) {
  return {mimeType:'application/pdf',bytes:new TextEncoder().encode('%PDF-'+source),diagnosticsJson:()=> '[]',free(){}};
}`;
let identity = 0;
class Element extends EventTarget {
  constructor(id = '') {
    super(); this.id = id; this.textContent = ''; this.innerHTML = ''; this.childNodes = [];
    this.attrs = new Map(); this.properties = new Map(); this._value = ''; this.disabled = false;
    this.style = {setProperty: (k,v) => this.properties.set(k,v), getPropertyValue: k => this.properties.get(k) || ''};
    const classes = new Set();
    this.classList = {add: x => classes.add(x),remove: x => classes.delete(x),contains: x => classes.has(x),
      toggle: x => classes.has(x) ? classes.delete(x) : classes.add(x)};
  }
  get value() { return this._value; }
  set value(value) { this._value = String(value).replace(/\r\n?/g, '\n'); }
  setAttribute(k,v) { this.attrs.set(k,v); }
  removeAttribute(k) { this.attrs.delete(k); }
  appendChild(node) { this.childNodes.push(node); node.parentNode = this; return node; }
  insertBefore(node) { return this.appendChild(node); }
  replaceChildren(...nodes) { this.childNodes = nodes; this.innerHTML = nodes.map(n => n.srcdoc || n.innerHTML || '').join(''); }
  remove() { this.removed = true; }
  click() { if (!this.disabled) this.dispatchEvent(new Event('click')); }
  focus() {}
}
async function setup({source = '# Original', background = true} = {}) {
  const ids = ['fmd-editor','fmd-content','fmd-app-body','source-line-count','stats-drawer',
    'stat-words','stat-chars','stat-read-time','stat-readability','fmd-save-status',
    'btn-save-markdown','btn-save-html','btn-export-pdf','btn-toggle-view','view-mode-icon','view-mode-label',
    'btn-zoom-in','btn-zoom-out','btn-zoom-reset','btn-theme-toggle','btn-stats-toggle','btn-stats-close'];
  const nodes = new Map(ids.map(id => [id,new Element(id)]));
  const get = id => nodes.get(id);
  const preview = get('fmd-content'); preview.innerHTML = 'initial native preview';
  get('fmd-app-body').classList.add('view-split');
  get('stats-drawer').querySelector = s => get(s.slice(1));
  const raw = Object.assign(new Element(),{textContent:JSON.stringify(source)});
  const payload = {version:1,wasm:'AGFzbQEAAAA=',bindings:bindingsSource,options:{font:'serif',fontScale:1.25},images:[],fonts:[]};
  const data = Object.assign(new Element(),{textContent:JSON.stringify(payload)});
  const header = new Element(), body = new Element(), document = new Element();
  header.appendChild(get('btn-export-pdf')); header.appendChild(get('btn-save-markdown'));
  header.querySelector = s => s.startsWith('#') ? header.childNodes.find(n => n.id === s.slice(1)) || null : null;
  Object.assign(document,{body,title:'Document',getElementById:get});
  document.querySelector = selector => ({
    'body > script#fmd-native-runtime[type="application/json"]':data,
    'body > script#fmd-raw-source[type="application/json"]':raw,
    'body > #stats-drawer':get('stats-drawer'),
    'body > .fmd-app-header':header,
  }[selector] || null);
  const root = new Element(); document.documentElement = root;
  const downloads = [], copies = [], blobs = new Map(), revoked = [];
  document.createElement = tag => {
    const node = new Element();
    if (tag === 'a') node.click = () => downloads.push({filename:node.download,blob:blobs.get(node.href)});
    return node;
  };
  root.cloneNode = () => {
    const script = Object.assign(new Element(),{textContent:raw.textContent});
    const textarea = new Element(), status = new Element(), drawer = new Element();
    const html = preview.innerHTML;
    const copy = {script,textarea,html,querySelector: selector => ({
      'body > script#fmd-raw-source[type="application/json"]':script,
      'body > #fmd-app-body > #editor-pane > textarea#fmd-editor':textarea,
      'body > #stats-drawer':drawer,
      '#editor-pane > .fmd-pane-header > #fmd-save-status':status,
    }[selector] || null),get outerHTML() { return '<html><main>'+html+'</main><script type="application/json">'+script.textContent+'</script></html>'; }};
    copies.push(copy); return copy;
  };
  const moduleUrl = 'data:text/javascript;base64,'+Buffer.from(bindingsSource).toString('base64')+'#'+ ++identity;
  let first = true, next = 0;
  const urls = {createObjectURL(blob) {
    if (first) { first = false; return moduleUrl; }
    const url = 'blob:download-'+ ++next; blobs.set(url,blob); return url;
  },revokeObjectURL(url) { revoked.push(url); }};
  const window = new Element(); window.print = () => { throw Error('native preview must not print'); };
  const globals = {document,window,URL:urls};
  const saved = Object.fromEntries(Object.keys(globals).map(k => [k,Object.getOwnPropertyDescriptor(globalThis,k)]));
  Object.assign(globalThis,globals);
  const workers = [];
  const createPreview = (factory,initial) => {
    assert.equal(factory,createNativeWorkspaceRenderer);
    const jobs = [];
    const worker = {jobs,initial,closed:false,invalidations:0,
      render(markdown,display,state) {
        return new Promise((resolve,reject) => {
          if (worker.closed) { reject(Object.assign(Error('Background preview is closed'),{code:'PREVIEW_CLOSED'})); return; }
          jobs.push({markdown,display,state,finish(html = '<html><head></head><body>'+markdown+'</body></html>',diagnostics = []) {
            resolve({html,diagnostics});
          },reject});
        });
      },invalidate() {
        worker.invalidations++;
        for (const job of jobs) job.reject(Object.assign(Error('superseded'),{code:'PREVIEW_SUPERSEDED'}));
      },dispose() { worker.invalidate(); worker.closed = true; }};
    workers.push(worker); return worker;
  };
  try {
    bootNativeWorkspace(createNativeWorkspaceRenderer,background ? createPreview : undefined);
    const engine = window.__fmdNativeRuntime;
    assert.equal(await engine.ready,true);
    const bindings = await import(moduleUrl);
    const timers = new Map(); let timerId = 0;
    // Form parsing is covered in the real-browser probe. Keep the full shipped
    // controller, but omit this one optional capability in the DOM adapter.
    const applySettings = engine.applySettings; engine.applySettings = undefined;
    const context = vm.createContext({document,window,Blob,URL:urls,Event,
      setTimeout(fn,ms) { const id=++timerId;timers.set(id,{fn,ms});return id; },
      clearTimeout(id) { timers.delete(id); },
      parseMarkdownClient(text) { return 'fallback:'+text; }});
    vm.runInContext(controller,context);
    engine.applySettings = applySettings;
    await flush();
    return {engine,bindings,nodes,workers,get,document,preview,downloads,copies,data,revoked,timers,
      click(id) { (get(id) || header.querySelector('#'+id)).click(); },
      edit(text) { get('fmd-editor').value=text;get('fmd-editor').dispatchEvent(new Event('input')); },
      fire(name) { (name.startsWith('composition') ? get('fmd-editor') : window).dispatchEvent(new Event(name)); },
      tick() { for(const [id,t] of [...timers]) if(t.ms===150){timers.delete(id);t.fn();} },
      close() { window.dispatchEvent(new Event('pagehide')); for(const [k,d] of Object.entries(saved)) d ? Object.defineProperty(globalThis,k,d) : Reflect.deleteProperty(globalThis,k); },
    };
  } catch(error) {
    for(const [k,d] of Object.entries(saved)) d ? Object.defineProperty(globalThis,k,d) : Reflect.deleteProperty(globalThis,k);
    throw error;
  }
}

test('the actual bootstrap/editor use the worker and commit diagnostics only with matching HTML', async () => {
  const s=await setup({source:'\ufeff# Exact\r\nsource\r'});
  try {
    assert.equal(s.workers.length,1); assert.equal(s.bindings.calls.length,0);
    const job=s.workers[0].jobs[0]; assert.equal(job.markdown,'\ufeff# Exact\r\nsource\r');
    assert.equal(s.preview.innerHTML,'initial native preview'); assert.equal(s.preview.attrs.get('aria-busy'),'true');
    job.finish(undefined,[{message:'native warning'}]); await flush();
    assert.match(s.preview.innerHTML,/# Exact/); assert.equal(s.preview.attrs.get('aria-busy'),'false');
    assert.match(s.get('fmd-save-status').textContent,/native warning/);
    assert.equal(s.bindings.calls.length,0);
  } finally {s.close();}
});

test('Save HTML waits for the latest preview and repeated clicks share one render/download', async () => {
  const s=await setup();
  try {
    s.edit('latest');s.click('btn-save-html');s.click('btn-save-html');
    assert.equal(s.downloads.length,0);assert.equal(s.workers[0].jobs.length,2);
    s.workers[0].jobs[1].finish();await flush();
    assert.equal(s.downloads.length,1);assert.equal(s.downloads[0].filename,'Document.html');
    assert.equal(JSON.parse(s.copies[0].script.textContent),'latest');assert.match(s.copies[0].html,/latest/);
  } finally {s.close();}
});

test('input invalidates before debounce and neither stale success nor failure can publish', async () => {
  const s=await setup();
  try {
    const old=s.workers[0].jobs[0];s.edit('newest');old.finish();await flush();
    assert.equal(s.preview.innerHTML,'initial native preview');assert.doesNotMatch(s.get('fmd-save-status').textContent,/Unable/);
    s.edit('final');s.tick();assert.equal(s.workers[0].jobs.length,2);
    s.workers[0].jobs[1].finish();await flush();assert.match(s.preview.innerHTML,/final/);
  } finally {s.close();}
});

test('an undispatched editor change is checked before publishing or downloading a pending save', async () => {
  const s=await setup();
  try {
    s.click('btn-save-html');s.get('fmd-editor').value='changed without input';
    s.workers[0].jobs[0].finish();await flush();
    assert.equal(s.downloads.length,0);assert.equal(s.preview.innerHTML,'initial native preview');
    s.click('btn-save-html');s.workers[0].jobs[1].finish();await flush();
    assert.equal(JSON.parse(s.copies[0].script.textContent),'changed without input');
  } finally {s.close();}
});

test('editing during Save HTML cancels that save; another explicit save captures current source', async () => {
  const s=await setup();
  try {
    s.click('btn-save-html');s.edit('revised');await flush();assert.equal(s.downloads.length,0);
    s.tick();s.click('btn-save-html');s.workers[0].jobs.at(-1).finish();await flush();
    assert.equal(s.downloads.length,1);assert.equal(JSON.parse(s.copies[0].script.textContent),'revised');
  } finally {s.close();}
});

test('composition suppresses intermediate previews and exports, then resumes with committed input', async () => {
  const s=await setup();
  try {
    s.fire('compositionstart');s.edit('composing');s.tick();s.click('btn-save-html');await flush();
    assert.equal(s.workers[0].jobs.length,1);assert.equal(s.downloads.length,0);
    s.fire('compositionend');s.tick();s.workers[0].jobs.at(-1).finish();await flush();assert.match(s.preview.innerHTML,/composing/);
  } finally {s.close();}
});

test('zoom and theme supersede same-source previews, and settings commits replace worker state', async () => {
  const s=await setup();
  try {
    s.click('btn-zoom-in');s.click('btn-theme-toggle');
    const w=s.workers[0],job=w.jobs.at(-1);assert.equal(job.display.scale,1.1);assert.equal(job.display.theme,'dark');
    s.engine.applySettings({title:'new title'},'# Original',s.preview,{scale:1});
    const committed=s.preview.innerHTML;job.finish();await flush();assert.equal(s.preview.innerHTML,committed);
    s.edit('new settings source');s.tick();assert.equal(w.jobs.at(-1).state.options.title,'new title');
    w.jobs.at(-1).finish();await flush();assert.match(s.preview.innerHTML,/new settings source/);
  } finally {s.close();}
});

test('image commit and rollback invalidate old previews and supply exact current resource snapshots', async () => {
  const s=await setup();
  try {
    const transaction=s.engine.stageImages([{destination:'new.png',bytes:Uint8Array.of(1,2,3)}]);
    transaction.commit();s.workers[0].jobs[0].finish();await flush();assert.equal(s.preview.innerHTML,'initial native preview');
    s.edit('with image');s.tick();const job=s.workers[0].jobs.at(-1);assert.equal(job.state.images[0].bytes,'AQID');
    transaction.rollback();job.finish();await flush();assert.equal(s.preview.innerHTML,'initial native preview');
    s.edit('without image');s.tick();assert.deepEqual(s.workers[0].jobs.at(-1).state.images,[]);
  } finally {s.close();}
});

test('worker failure preserves recovery downloads and never falls back to synchronous preview', async () => {
  const s=await setup();
  try {
    s.workers[0].closed=true;s.workers[0].jobs[0].reject(Object.assign(Error('timed out'),{code:'PREVIEW_TIMEOUT'}));await flush();
    assert.match(s.get('fmd-save-status').textContent,/Restart preview/);assert.equal(s.bindings.calls.length,0);
    s.edit('recover me');s.click('btn-save-markdown');assert.equal(await s.downloads[0].blob.text(),'recover me');
    s.click('btn-save-html');await flush();assert.equal(s.downloads.length,1);assert.equal(s.workers.length,1);
    s.click('btn-restart-preview');assert.equal(s.workers.length,2);assert.equal(s.workers[1].jobs[0].markdown,'recover me');
    s.workers[1].jobs[0].finish();await flush();assert.match(s.preview.innerHTML,/recover me/);
  } finally {s.close();}
});

test('page suspension stops work and pending saves; resumption starts one new worker with current source', async () => {
  const s=await setup();
  try {
    s.click('btn-save-html');s.fire('pagehide');s.workers[0].jobs[0].finish();await flush();
    assert.equal(s.workers[0].closed,true);assert.equal(s.downloads.length,0);assert.equal(s.preview.innerHTML,'initial native preview');
    s.fire('pageshow');assert.equal(s.workers.length,2);s.workers[1].jobs[0].finish();await flush();assert.match(s.preview.innerHTML,/# Original/);
  } finally {s.close();}
});

test('native HTML publishing and PDF export remain independent of a pending preview', async () => {
  const s=await setup();
  try {
    s.edit('export latest');s.click('btn-export-pdf');s.click('btn-publish-html');
    assert.equal(s.downloads.length,2);assert.equal(await s.downloads[0].blob.text(),'%PDF-export latest');
    assert.match(await s.downloads[1].blob.text(),/sync:export latest/);assert.equal(s.preview.innerHTML,'initial native preview');
  } finally {s.close();}
});

test('legacy synchronous bootstrap remains usable without the optional worker factory', async () => {
  const s=await setup({background:false});
  try {
    assert.equal(s.workers.length,0);assert.match(s.preview.innerHTML,/sync:# Original/);
    s.edit('legacy');s.click('btn-save-html');assert.equal(s.downloads.length,1);assert.match(s.copies[0].html,/sync:legacy/);
  } finally {s.close();}
});
