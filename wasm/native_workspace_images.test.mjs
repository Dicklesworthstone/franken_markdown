// Actual runtime/import code with explicit native ABI and DOM adapters. These
// tests do not claim Rust decoding/layout or browser undo/download execution.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
const encoder = new TextEncoder();
const b64 = bytes => Buffer.from(bytes).toString('base64');
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8DwHwQBEPgD/U6VwW8AAAAASUVORK5CYII=', 'base64');
const source = readFileSync(new URL('../src/interactive_import.js', import.meta.url), 'utf8');
function setup(extra = {}) {
  const payload = {version: 1, options: {font: 'serif', fontScale: 1.25, pageNumbers: true,
    pageGeometry: [842, 595, 36, 40, 44, 48]}, images: [{destination:'old.png', bytes:b64([1,2])}],
    fonts:[{slot:'body-regular', bytes:b64([7,8]), weight:450}], ...extra};
  const calls = [];
  let freed = 0;
  const bindings = {};
  for (const method of ['renderHtmlConfiguredAdvanced', 'renderPdfConfiguredMulti', 'renderPdfConfiguredPage']) {
    bindings[method] = (...args) => {
      calls.push({method, args});
      return {mimeType:method.includes('Html') ? 'text/html' : 'application/pdf',
        bytes:encoder.encode(method.includes('Html') ? '<html><head></head><body>ABI adapter</body></html>' : '%PDF-1.7\nABI adapter, not a PDF file'),
        diagnosticsJson:()=>'[]', free(){freed++;}};
    };
  }
  const renderer = createNativeWorkspaceRenderer(bindings, payload);
  const assets = (kind = 'pdf') => {
    renderer[kind]('latest source'); const {args} = calls.at(-1), i = kind === 'html' ? 13 : 8;
    let offset = 0;
    return args[i].map((key, n) => {
      const bytes = args[i + 1].slice(offset, offset + args[i + 2][n]); offset += bytes.length;
      return {destination:key, bytes:[...bytes]};
    });
  };
  return {payload, bindings, renderer, calls, assets, get freed(){return freed;}};
}
const image = (destination = 'new.png', bytes = png) => ({destination, bytes});
const file = (name='picture.png', bytes=png) => ({name, size:bytes.length,
  async arrayBuffer(){return Uint8Array.from(bytes).buffer;}});
const pure = vm.runInNewContext(source+'\n({fmdReadImageImport, fmdPrepareImageImport, fmdNativeImageImport});',
  {Uint8Array, ArrayBuffer, btoa, crypto:webcrypto});
const read = files => pure.fmdReadImageImport(files, async()=>{}, ()=>true);

test('staged images are invisible until committed, then reach HTML and paged PDF with untouched fonts', () => {
  const s=setup(), t=s.renderer.stageImages([image()]);
  assert.deepEqual(s.assets(),[{destination:'old.png',bytes:[1,2]}]);
  t.commit(); const expected=[{destination:'old.png',bytes:[1,2]},{destination:'new.png',bytes:[...png]}];
  assert.deepEqual(s.assets('html'), expected); assert.deepEqual(s.assets(),expected);
  const {method,args}=s.calls.at(-1); assert.equal(method,'renderPdfConfiguredPage');
  assert.deepEqual([...args[27]],[842,595,36,40,44,48]); assert.deepEqual([...args[11]],[7,8]);
  assert.deepEqual([...args[16]],[450,0,0,0,0]); assert.equal(args[21],1.25); assert.equal(s.freed,3);
  t.rollback(); assert.deepEqual(s.assets(),[{destination:'old.png',bytes:[1,2]}]);
});

test('exact DataView, typed-array and pooled Buffer views are snapshotted before caller mutation', () => {
  const s=setup(), raw=Buffer.from([99,4,5,6,88]);
  const t=s.renderer.stageImages([image('one',new DataView(raw.buffer,raw.byteOffset+1,3)),
    image('two',raw.subarray(2,4)),image('three',Uint16Array.of(0x1234))]);
  raw.fill(0);t.commit();assert.deepEqual(s.assets().slice(1,3),[
    {destination:'one',bytes:[4,5,6]},{destination:'two',bytes:[5,6]}]);
  assert.deepEqual(Buffer.from(s.payload.images[1].bytes,'base64'),Buffer.from([4,5,6]));
});

test('publisher errors leave both commit and rollback retryable without a partial renderer switch', () => {
  const s=setup(), before=JSON.stringify(s.payload), t=s.renderer.stageImages([image()]);
  assert.throws(()=>t.commit(()=>{throw Error('cannot persist');}),/cannot persist/);
  assert.equal(JSON.stringify(s.payload),before);assert.equal(s.assets().length,1);
  t.commit();assert.equal(s.assets().length,2);
  assert.throws(()=>t.rollback(()=>{throw Error('cannot restore');}),/cannot restore/);
  assert.equal(s.assets().length,2);t.rollback();assert.equal(s.assets().length,1);
});

test('stale commits, repeated commits and old rollbacks cannot replace newer resources', () => {
  const s=setup(), a=s.renderer.stageImages([image('a')]), b=s.renderer.stageImages([image('b')]);
  a.commit();assert.throws(()=>b.commit(),/Stale/);assert.throws(()=>a.commit(),/Stale/);
  const c=s.renderer.stageImages([image('c')]);c.commit();assert.throws(()=>a.rollback(),/Stale/);
  assert.deepEqual(s.assets().map(a=>a.destination),['old.png','a','c']);
  c.rollback();assert.throws(()=>c.rollback(),/Stale/);assert.throws(()=>b.commit(),/Stale/);
});

test('reopening serialized images preserves unused resources and allows subsequent independent imports', () => {
  const a=setup();a.renderer.stageImages([image()]).commit();
  const b=setup(JSON.parse(JSON.stringify(a.payload)));assert.deepEqual(b.assets(),a.assets());
  b.renderer.stageImages([image('another')]).commit();assert.equal(a.assets().length,2);assert.equal(b.assets().length,3);
  assert.equal(b.payload.images[0].destination,'old.png');
});

test('immutable transaction manifests cannot be changed independently of packed native bytes', () => {
  const s=setup(), t=s.renderer.stageImages([image()]);
  assert.ok(Object.isFrozen(t));assert.ok(Object.isFrozen(t.images));assert.ok(Object.isFrozen(t.images[1]));
  assert.throws(()=>{t.images[1].destination='other';});t.commit();assert.equal(s.assets()[1].destination,'new.png');
});

test('batch/count admission precedes entry getters including sparse arrays and full workspaces', () => {
  const s=setup();let reads=0;const many=new Array(9);Object.defineProperty(many,0,{get(){reads++;throw Error('read');}});
  assert.throws(()=>s.renderer.stageImages(many),/1..8/);assert.equal(reads,0);
  const full=setup({images:Array.from({length:4096},(_,i)=>({destination:String(i),bytes:'AQ=='}))});
  const one=new Array(1);Object.defineProperty(one,0,{get(){reads++;throw Error('read');}});
  assert.throws(()=>full.renderer.stageImages(one),/4096/);assert.equal(reads,0);
});

test('duplicate, control-bearing, empty and lossy destination names are rejected without side effects', () => {
  const s=setup();
  for (const list of [[image(' old.png ')],[image('a'),image(' a ')],[image('')],[image('a\nb')],
    [image('\ud800')],[image('é'.repeat(4097))],[image(null)]]) assert.throws(()=>s.renderer.stageImages(list));
  assert.equal(s.assets().length,1);assert.equal(s.payload.images.length,1);
});

test('cumulative destination budget remains enforced across successful imports and reopenings', () => {
  const s=setup({images:Array.from({length:8},(_,i)=>({destination:String(i)+'x'.repeat(8191),bytes:'AQ=='}))});
  assert.throws(()=>s.renderer.stageImages([image()]),/budget/);assert.equal(s.assets().length,8);
});

test('empty, shared, detached, oversized and over-batch binary inputs cannot enter the native store', () => {
  const s=setup();const detached=Uint8Array.of(1);structuredClone(detached.buffer,{transfer:[detached.buffer]});
  for (const bytes of [new Uint8Array(),new SharedArrayBuffer(1),new Uint8Array(new SharedArrayBuffer(1)),
    detached,{byteLength:1},new Uint8Array(8*1024*1024+1)]) assert.throws(()=>s.renderer.stageImages([image('bad',bytes)]));
  const big=new Uint8Array(8*1024*1024);
  assert.throws(()=>s.renderer.stageImages([image('a',big),image('b',big),image('c',Uint8Array.of(1))]),/budget/);
  assert.equal(s.assets().length,1);
});

test('saved resource counts and aggregate image/font bytes are rejected before decoding', () => {
  const s=setup();let decoded=0;
  const factory=vm.runInNewContext('('+createNativeWorkspaceRenderer.toString()+')',{
    atob(){decoded++;throw Error('decoded too early');},Uint8Array,Uint32Array,Float64Array,TextEncoder,TextDecoder});
  const block='A'.repeat(Math.ceil(32*1024*1024/3)*4-1)+'=';
  const p={...s.payload,images:[{destination:'x',bytes:'AQ=='}],
    fonts:['body-regular','body-bold','body-italic','body-bold-italic'].map(slot=>({slot,bytes:block}))};
  assert.throws(()=>factory(s.bindings,p),/budget/);assert.equal(decoded,0);
  p.images=new Array(4097);assert.throws(()=>factory(s.bindings,p),/4096/);assert.equal(decoded,0);
});

test('base64 chunk boundaries remain exact in saved and live native resource copies', () => {
  for(const length of [1,2,3,24575,24576,24577,49155]) {
    const s=setup(), bytes=Uint8Array.from({length},(_,i)=>i%251);
    s.renderer.stageImages([image('x',bytes)]).commit();
    assert.deepEqual(Buffer.from(s.payload.images[1].bytes,'base64'),Buffer.from(bytes));
    assert.deepEqual(s.assets()[1].bytes,[...bytes]);
  }
});

test('serialized runtime stages resources without external lexical dependencies', () => {
  const s=setup();const factory=vm.runInNewContext('('+createNativeWorkspaceRenderer.toString()+')',
    {atob,btoa,Uint8Array,Uint32Array,Float64Array,TextEncoder,TextDecoder,ArrayBuffer});
  const renderer=factory(s.bindings,s.payload);renderer.stageImages([image()]).commit();renderer.pdf('x');
  assert.equal(s.calls.at(-1).args[8][1],'new.png');
});

test('lightweight preparation keeps exact data-URI Markdown; native preparation uses distinct short stable references', async () => {
  const f=file('literal[<>&.png');const lightweight=await pure.fmdPrepareImageImport([f],async()=>{},()=>true);
  assert.equal(lightweight,'![literal\\[\\<\\>&amp;.png](data:image/png;base64,'+png.toString('base64')+')');
  const images=await read([f,f]), s=setup(), pending=pure.fmdNativeImageImport(images,s.renderer);
  assert.doesNotMatch(pending.markdown,/data:/);assert.match(pending.markdown,/fmd-import\/[a-f0-9]{32}\.png/);
  pending.transaction.commit();const added=s.assets().slice(1);
  assert.notEqual(added[0].destination,added[1].destination);assert.deepEqual(added[0].bytes,[...png]);
});

test('file bytes, size and alt are captured before asynchronous decoding and native publication', async () => {
  const bytes=Uint8Array.from(png);const f={name:'before.png',size:bytes.length,async arrayBuffer(){return bytes.buffer;}};
  const images=await pure.fmdReadImageImport([f],async()=>{bytes.fill(0);f.name='after';f.size=1;},()=>true);
  assert.equal(images[0].alt,'before.png');assert.deepEqual([...images[0].bytes],[...png]);
});

test('read/verify errors and stale revisions return no usable import batch', async () => {
  let verified=0;
  const f=file();f.arrayBuffer=async()=>new SharedArrayBuffer(f.size);
  await assert.rejects(read([f]));
  const other=file();other.size=1;await assert.rejects(read([other]),/size changed/);
  await assert.rejects(pure.fmdReadImageImport([file(),file()],async()=>{if(++verified===2) throw Error('decode');},()=>true),/decode/);
  await assert.rejects(pure.fmdReadImageImport([file()],async()=>{},()=>false),/document changed/);
  let current=true;await assert.rejects(pure.fmdReadImageImport([file()],async()=>{current=false;},()=>current),/document changed/);
});

function target() {
  const listeners=new Map();
  return {value:'',disabled:false,addEventListener(type,fn){if(!listeners.has(type)) listeners.set(type,[]);listeners.get(type).push(fn);},
    dispatchEvent(event){for(const fn of listeners.get(event.type)||[]) fn(event);},
    async fire(type, event={}){await Promise.all((listeners.get(type)||[]).map(fn=>fn({type,...event})));},click(){this.dispatchEvent({type:'click'});}};
}
function dom({native=true, mode='command', decode=async()=>{}, initial='# Before'}={}) {
  const s=setup(), editor=target(), button=target(), picker=target(), status=target(), window=target();
  editor.value=initial;editor.selectionStart=editor.selectionEnd=initial.length;
  editor.focus=()=>{};editor.setSelectionRange=(start,end)=>{editor.selectionStart=start;editor.selectionEnd=end;};
  editor.setRangeText=(text,start,end)=>{if(mode==='failure') throw Error('editing unavailable');
    editor.value=editor.value.slice(0,start)+text+editor.value.slice(end);};
  const app={classList:{contains:()=>false}}, nodes=new Map([
    ['body > #fmd-app-body > #editor-pane > textarea#fmd-editor',editor],
    ['body > .fmd-app-header #btn-insert-image',button],['body > .fmd-app-header #fmd-image-picker',picker],
    ['#editor-pane > .fmd-pane-header > #fmd-save-status',status],['body > #fmd-app-body',app]]);
  const document={querySelector:selector=>nodes.get(selector),execCommand(_,__,text){
    if(mode==='fallback' || mode==='failure') throw Error('command unavailable');
    editor.setRangeText(mode==='partial'?'partial '+text:text,editor.selectionStart,editor.selectionEnd);
    editor.dispatchEvent({type:'input'});return true;
  }};
  if(native) Object.defineProperty(window,'__fmdNativeRuntime',{value:{version:1,
    render:(...a)=>s.renderer.html(...a),pdf:(...a)=>s.renderer.pdf(...a),stageImages:(...a)=>s.renderer.stageImages(...a)}});
  const context=vm.createContext({document,window,Uint8Array,ArrayBuffer,TextEncoder,btoa,crypto:webcrypto,Event,setTimeout,clearTimeout,
    Image:class {naturalWidth=2;naturalHeight=1;async decode(){await decode();}removeAttribute(){}}});
  vm.runInContext(source,context);
  return {...s,editor,button,picker,status,window,context,
    async choose(files=[file()]){button.click();picker.files=files;await picker.fire('change');}};
}

test('shipped picker publishes resources before synchronous input/PDF, without data-URI source bloat', async () => {
  const s=dom();let inputAssets;
  s.editor.addEventListener('input',()=>{inputAssets=s.assets();});await s.choose();
  assert.equal(inputAssets.length,2);assert.match(s.editor.value,/fmd-import\//);assert.doesNotMatch(s.editor.value,/data:/);
  assert.match(s.status.textContent,/Save HTML.*Markdown alone/);assert.equal(s.button.disabled,false);assert.equal(s.picker.disabled,false);
});

test('a failed multi-file browser decode leaves source and native resources untouched', async () => {
  let count=0;const s=dom({decode:async()=>{if(++count===2) throw Error('decoder refused');}});await s.choose([file(),file()]);
  assert.equal(s.editor.value,'# Before');assert.equal(s.assets().length,1);assert.match(s.status.textContent,/decoder refused/);
});

test('missing import support and unavailable native initialization never silently use lightweight rendering', async () => {
  for(const kind of ['old','loading']) {
    const s=dom(), engine=s.window.__fmdNativeRuntime;
    if(kind==='old') delete engine.stageImages;else engine.stageImages=()=>{throw Error('Native loading');};
    await s.choose();assert.equal(s.editor.value,'# Before');assert.equal(s.assets().length,1);
    assert.match(s.status.textContent,kind==='old'?/Rebuild/:/Native loading/);
  }
});

test('editing-command exceptions fall back safely; complete editing failures roll back resource publication', async () => {
  const fallback=dom({mode:'fallback'});await fallback.choose();assert.equal(fallback.assets().length,2);
  const failed=dom({mode:'failure'}), before=JSON.stringify(failed.payload);await failed.choose();
  assert.equal(failed.editor.value,'# Before');assert.equal(JSON.stringify(failed.payload),before);
  assert.equal(failed.assets().length,1);assert.match(failed.status.textContent,/editing unavailable/);
});

test('unexpected partial host edits retain resources instead of leaving newly inserted references broken', async () => {
  const s=dom({mode:'partial'});await s.choose();assert.match(s.status.textContent,/inspect the document/);
  assert.match(s.editor.value,/fmd-import\//);assert.equal(s.assets().length,2);
});

test('typing then undoing during asynchronous decode is still a stale revision, not permission to replace a selection', async () => {
  let release;const gate=new Promise(resolve=>{release=resolve;});const s=dom({decode:()=>gate});
  const pending=s.choose();await new Promise(resolve=>setImmediate(resolve));
  s.editor.value='edited';s.editor.dispatchEvent({type:'input'});s.editor.value='# Before';s.editor.dispatchEvent({type:'input'});
  release();await pending;assert.equal(s.assets().length,1);assert.equal(s.editor.value,'# Before');assert.match(s.status.textContent,/document changed/);
});

test('leaving and restoring a page invalidates outstanding imports', async () => {
  let release;const gate=new Promise(resolve=>{release=resolve;});const s=dom({decode:()=>gate});
  const pending=s.choose();await new Promise(resolve=>setImmediate(resolve));
  await s.window.fire('pagehide');await s.window.fire('pageshow');release();await pending;
  assert.equal(s.assets().length,1);assert.equal(s.editor.value,'# Before');assert.match(s.status.textContent,/document changed/);
});

test('read-only caret movement during decoding does not retarget the original selection', async () => {
  let release;const gate=new Promise(resolve=>{release=resolve;});const s=dom({decode:()=>gate,initial:'first last'});
  s.editor.selectionStart=0;s.editor.selectionEnd=5;const pending=s.choose();await new Promise(resolve=>setImmediate(resolve));
  s.editor.setSelectionRange(10,10);release();await pending;assert.doesNotMatch(s.editor.value,/first/);assert.match(s.editor.value,/last$/);
});

test('native imports enforce UTF-8 bytes, not only the textarea UTF-16 length', async () => {
  const initial='é'.repeat(16*1024*1024), s=dom({initial});await s.choose();
  assert.equal(s.editor.value,initial);assert.equal(s.assets().length,1);assert.match(s.status.textContent,/UTF-8 source limit/);
});

test('the lightweight picker remains portable without a native engine', async () => {
  const s=dom({native:false});await s.choose();assert.match(s.editor.value,/data:image\/png;base64,/);
  assert.equal(s.assets().length,1);assert.match(s.status.textContent,/download to keep changes/);
});


test('clipboard-delivered files use the same verified native resources without ambient clipboard reads', async () => {
  const s=dom();let prevented=false;
  await s.editor.fire('paste',{clipboardData:{files:[file()]},preventDefault(){prevented=true;}});
  assert.equal(prevented,true);assert.equal(s.assets().length,2);assert.match(s.editor.value,/fmd-import\//);
});

test('file drag/drop targets the editor selection and prevents file navigation', async () => {
  const s=dom({initial:'replace keep'});s.editor.setSelectionRange(0,7);
  let dragged=false,dropped=false;const dataTransfer={files:[file()],types:['Files'],dropEffect:'none'};
  await s.editor.fire('dragover',{dataTransfer,preventDefault(){dragged=true;}});
  assert.equal(dataTransfer.dropEffect,'copy');dataTransfer.dropEffect='none';
  await s.editor.fire('drop',{dataTransfer,preventDefault(){dropped=true;}});
  assert.equal(dragged,true);assert.equal(dataTransfer.dropEffect,'copy');assert.equal(dropped,true);
  assert.equal(s.assets().length,2);assert.doesNotMatch(s.editor.value,/replace/);assert.match(s.editor.value,/keep$/);
});

test('ordinary text, HTML-only pastes and URL drags keep their browser defaults and never fetch resources', async () => {
  const s=dom();let prevented=0,reads=0;
  const data={files:[],types:['text/plain','text/html','text/uri-list'],getData(){reads++;throw Error('must not read URLs');}};
  for(const type of ['paste','dragover','drop']) await s.editor.fire(type,{clipboardData:data,dataTransfer:data,preventDefault(){prevented++;}});
  assert.equal(prevented,0);assert.equal(reads,0);assert.equal(s.editor.value,'# Before');assert.equal(s.assets().length,1);
});

test('oversized paste batches are refused before file reads and invalid drops remain source-preserving', async () => {
  const s=dom();let reads=0;const f=file();f.arrayBuffer=async()=>{reads++;return png.buffer;};
  await s.editor.fire('paste',{clipboardData:{files:Array(9).fill(f)},preventDefault(){}});
  assert.equal(reads,0);assert.match(s.status.textContent,/between 1 and 8/);
  let prevented=false;
  await s.editor.fire('drop',{dataTransfer:{files:[file('claimed.png',Buffer.from('<svg/>'))]},preventDefault(){prevented=true;}});
  assert.equal(prevented,true);assert.equal(s.editor.value,'# Before');assert.equal(s.assets().length,1);
});

test('picker, paste and drop share one in-flight import, not competing source/resource transactions', async () => {
  let release;const gate=new Promise(resolve=>{release=resolve;});const s=dom({decode:()=>gate});
  const pending=s.choose();await new Promise(resolve=>setImmediate(resolve));let reads=0;
  const f=file();f.arrayBuffer=async()=>{reads++;throw Error('competing import');};
  for(const type of ['paste','drop']) await s.editor.fire(type,{clipboardData:{files:[f]},dataTransfer:{files:[f]},preventDefault(){}});
  assert.equal(reads,0);assert.match(s.status.textContent,/already in progress/);
  release();await pending;assert.equal(s.assets().length,2);assert.equal((s.editor.value.match(/fmd-import\//g)||[]).length,1);
});

test('lightweight paste and drop preserve their portable data-URI Markdown behavior', async () => {
  for(const type of ['paste','drop']) {
    const s=dom({native:false});await s.editor.fire(type,{clipboardData:{files:[file()]},dataTransfer:{files:[file()]},preventDefault(){}});
    assert.match(s.editor.value,/data:image\/png;base64,/);assert.equal(s.assets().length,1);
  }
});
