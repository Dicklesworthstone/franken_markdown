// Public exporter contract with isolated existing-wrapper bindings. No Rust or
// rebuilt WASM is implied by these adapter tests; browser boot has its own probe.
import assert from 'node:assert/strict';
import {test, after} from 'node:test';
import {mkdtemp, copyFile, writeFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
const dir = await mkdtemp(join(tmpdir(),'fmd-offline-contract-'));
after(() => rm(dir,{recursive:true,force:true}));
await writeFile(join(dir,'package.json'),'{"type":"module"}');
for(const file of ['interactive.js','interactive_runtime.mjs','interactive_preview.mjs']) await copyFile(new URL(file,import.meta.url),join(dir,file));
await writeFile(join(dir,'franken_markdown.js'),`
export const state={calls:[], gate:null,legacy:false,preWorker:false};
export async function init(bytes) { state.calls.push({kind:'init',bytes}); if(state.gate) await state.gate; }
export async function renderInteractiveHtml(source,options) {
  state.calls.push({kind:'render',source,options});
  const text='<!DOCTYPE html>\\n<html><head></head><body><script type="application/json" id="fmd-raw-source">'
    +JSON.stringify(source).replace(/</g,'\\\\u003c')+'</script>\\n<script>\\n'
    +(state.legacy?'/* old app */':"window.addEventListener('fmd-native-ready', () => {});" + (state.preWorker ? '' : '/* fmd-async-preview-v1 */'))
    +'\\n</script>\\n</body>\\n</html>\\n';
  return {format:'interactive-html',mimeType:'text/html; charset=utf-8',extension:'html',
    bytes:new TextEncoder().encode(text),sourceLength:0,diagnostics:[{message:'initial diagnostic'}],
    text:()=>text,blob:()=>new Blob([text]),filename:(name='document')=>name+'.html'};
}
`);
const {renderOfflineWorkspace}=await import(pathToFileURL(join(dir,'interactive.js')));
const {state}=await import(pathToFileURL(join(dir,'franken_markdown.js')));
const wasm=()=>Uint8Array.of(0,97,115,109,1,0,0,0);
const runtime=()=>({bindings:'export default async function() {}',wasm:wasm()});
function payload(output) {
  return JSON.parse(output.text().split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0]);
}
function reset() {state.calls=[];state.gate=null;state.legacy=false;state.preWorker=false;}

test('public output contains runtime/assets, enforces offline CSP and preserves result helpers',async()=>{
  reset();
  const result=await renderOfflineWorkspace('Café 中 😀',runtime(),{title:'Title',font:'serif',toc:true,pdfImages:[{destination:'a.png',bytes:Uint8Array.of(3,4)}]});
  const p=payload(result);
  assert.equal(result.sourceLength,Buffer.byteLength('Café 中 😀'));
  assert.equal(p.options.title,'Title'); assert.equal(p.options.font,'serif'); assert.equal(p.options.toc,true);
  assert.equal(p.images[0].bytes,'AwQ='); assert.deepEqual([...Buffer.from(p.wasm,'base64')],[...wasm()]);
  assert.match(result.text(),/<head>\n<meta http-equiv="Content-Security-Policy"/);
  assert.match(result.text(),/'wasm-unsafe-eval'/); assert.doesNotMatch(result.text(),/'unsafe-eval'/);
  assert.ok(result.text().indexOf('bootNativeWorkspace') < result.text().lastIndexOf("window.addEventListener('fmd-native-ready'"));
  assert.equal(await result.blob().text(),result.text()); assert.equal(result.blob().type,'text/html; charset=utf-8');
  assert.equal(result.filename('named'),'named.html'); assert.equal(result.diagnostics[0].message,'initial diagnostic');
  assert.ok(Object.isFrozen(result)); assert.equal(state.calls[0].kind,'init');
  assert.deepEqual([...state.calls[0].bytes],[...wasm()]);
});

test('capture owns source, scalar settings and exact image/font/runtime views before asynchronous init',async()=>{
  reset(); let release;
  state.gate=new Promise(resolve=>{release=resolve;});
  const backing=Uint8Array.of(99,1,2,3,88), font=Uint8Array.of(6,7), module=wasm();
  let source='# Before';
  const options={title:'before',fontScale:1.25,pdfImages:[{destination:' before.png ',bytes:new DataView(backing.buffer,1,3)}],fontAssets:[{slot:'body-regular',weight:555,bytes:font}]};
  const supplied={bindings:'// original\nexport default async function() {}',wasm:module};
  const pending=renderOfflineWorkspace({toString:()=>source},supplied,options);
  backing.fill(0);font.fill(0);module.fill(0);source='# After';
  options.title='after';options.fontScale=2;options.pdfImages[0].destination='after'; options.pdfImages.length=0;
  options.fontAssets[0].slot='mono-regular';options.fontAssets[0].weight=999;supplied.bindings='changed';
  release(); const result=await pending; const p=payload(result);
  assert.deepEqual([...Buffer.from(p.wasm,'base64')],[...wasm()]);
  assert.equal(p.bindings,'// original\nexport default async function() {}');
  assert.equal(p.options.title,'before');assert.equal(p.options.fontScale,1.25);
  assert.deepEqual(p.images,[{destination:'before.png',bytes:'AQID'}]);
  assert.deepEqual(p.fonts,[{slot:'body-regular',weight:555,bytes:'Bgc='}]);
  assert.equal(state.calls[1].source,'# Before');assert.deepEqual([...state.calls[0].bytes],[...wasm()]);
});

test('base64 chunk boundaries preserve exact byte views, including pooled Buffer and typed-array slices',async()=>{
  for(const size of [1,2,3,24575,24576,24577,49157]) {
    reset(); const backing=Buffer.alloc(size+10); for(let i=0;i<backing.length;i++) backing[i]=i%251;
    const bytes=backing.subarray(3,size+3);
    const p=payload(await renderOfflineWorkspace('x',runtime(),{pdfImages:[{destination:'x.png',bytes}]}));
    assert.deepEqual(Buffer.from(p.images[0].bytes,'base64'),bytes);
  }
});

test('script terminators and tokenizer-state attacks remain inert JSON, including binding source',async()=>{
  reset(); const hostile='</ScRiPt><script>alert(1)</script>\n<!--<script>--></script>\u2028\u2029';
  const r=runtime();r.bindings='// '+hostile;
  const result=await renderOfflineWorkspace(hostile,r,{title:hostile,pdfImages:[{destination:'</script>.png',bytes:Uint8Array.of(1)}]});
  const data=result.text().split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0];
  assert.doesNotMatch(data,/[<\u2028\u2029]/);
  assert.equal(JSON.parse(data).bindings,r.bindings);
  assert.equal((result.text().match(/<script(?:\s|>)/g)||[]).length,4);
});

test('old compiled controllers fail explicitly even when Markdown contains the new protocol text',async()=>{
  reset();state.legacy=true;
  await assert.rejects(renderOfflineWorkspace("window.addEventListener('fmd-native-ready'",runtime()), {code:'UNSUPPORTED_WASM_PACKAGE'});
});

test('counts reject sparse/oversized arrays before reading their entries or initializing WASM',async()=>{
  for(const key of ['pdfImages','fontAssets']) {
    reset(); let reads=0; const list=new Array(key==='pdfImages'?4097:6);
    Object.defineProperty(list,0,{get(){reads++;throw Error('entry read');}});
    await assert.rejects(renderOfflineWorkspace('x',runtime(),{[key]:list}),/at most/);
    assert.equal(reads,0);assert.equal(state.calls.length,0);
  }
});

test('invalid options, duplicates, malformed strings and binary inputs fail before init',async()=>{
  const cases=[
    [{font:'unknown'},/font/], [{darkMode:'dark'},/darkMode/], [{fontScale:NaN},/fontScale/],
    [{fontScale:4},/fontScale/], [{toc:'yes'},/boolean/], [{tocDepth:7},/tocDepth/],
    [{metadataEpochSeconds:-1},/metadata/], [{title:'\ud800'},/surrogate/],
    [{allowRawHtml:true},/Unsupported/], [{customCss:'body{}'},/Unsupported/],
    [{pdfImages:[{destination:'x',bytes:Uint8Array.of(1)},{destination:' x ',bytes:Uint8Array.of(2)}]},/unique/],
    [{pdfImages:[{destination:'a\n.png',bytes:Uint8Array.of(1)}]},/control/],
    [{fontAssets:[{slot:'bad',bytes:Uint8Array.of(1)}]},/slot/],
    [{fontAssets:[{slot:'body-bold',weight:1001,bytes:Uint8Array.of(1)}]},/weight/],
    [{fontAssets:[{slot:'body-bold',bytes:Uint8Array.of(1)},{slot:'body-bold',bytes:Uint8Array.of(2)}]},/unique/],
    [{pdfImages:[{destination:'x',bytes:new Uint8Array()}]},/length/],
    [{pdfImages:[{destination:'x',bytes:new Uint8Array(new SharedArrayBuffer(1))}]},TypeError],
  ];
  for(const [options,pattern] of cases) {
    reset();await assert.rejects(renderOfflineWorkspace('x',runtime(),options),pattern);assert.equal(state.calls.length,0);
  }
  reset();const detached=Uint8Array.of(1);structuredClone(detached.buffer,{transfer:[detached.buffer]});
  await assert.rejects(renderOfflineWorkspace('x',runtime(),{pdfImages:[{destination:'x',bytes:detached}]}),TypeError);
  await assert.rejects(renderOfflineWorkspace('x',{bindings:'a',wasm:Uint8Array.of(1)}),/WebAssembly/);
  await assert.rejects(renderOfflineWorkspace('\udc00',runtime()),/surrogate/);
  assert.equal(state.calls.length,0);
});

test('aggregate limits reject before initialization and separate calls never share captured resources',async()=>{
  reset();const bytes=new Uint8Array(32*1024*1024);
  await assert.rejects(renderOfflineWorkspace('x',runtime(),{pdfImages:Array.from({length:5},(_,i)=>({destination:String(i),bytes}))}),/combined/);
  assert.equal(state.calls.length,0);
  const outputs=await Promise.all([1,2].map(value=>renderOfflineWorkspace(String(value),runtime(),{pdfImages:[{destination:'x',bytes:Uint8Array.of(value)}]})));
  assert.equal(payload(outputs[0]).images[0].bytes,'AQ=='); assert.equal(payload(outputs[1]).images[0].bytes,'Ag==');
});


test('native but pre-worker controllers are refused even when source advertises the marker', async()=>{
  reset(); state.preWorker=true;
  const source="fmd-async-preview-v1 window.addEventListener('fmd-native-ready'";
  await assert.rejects(renderOfflineWorkspace(source,runtime()), {code:'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal(state.calls.filter(call=>call.kind==='render').length,1);
  reset();
});
