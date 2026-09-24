// Actual boot/runtime/worker functions with explicit DOM and native ABI adapters.
// worker_threads and empty WebAssembly initialization really execute.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Worker as Thread} from 'node:worker_threads';
import {bootNativeWorkspace,createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

const encode=text=>new TextEncoder().encode(text);
const bindingSource=`
export default async function({module_or_path}) { await WebAssembly.instantiate(module_or_path); }
function result(source,args,mimeType) {
  if(typeof document!=='undefined') globalThis.mainExportTestCalls.push(mimeType);
  if(source==='busy' && mimeType==='application/pdf') while(true) {}
  if(source==='throw') throw Error('native refusal');
  const serialized=JSON.stringify(args,(k,v)=>ArrayBuffer.isView(v)?Array.from(v):v);
  const text=mimeType==='text/html'?'<html><head></head><body>'+serialized+'</body></html>':'%PDF-ADAPTER\\n'+serialized;
  const bytes=new TextEncoder().encode(text);
  return {bytes,mimeType,diagnosticsJson:()=>JSON.stringify([{message:mimeType+':'+source}]),free(){bytes.fill(0);}};
}
export function renderHtmlConfiguredAdvanced(...args){return result(args[0],args,'text/html');}
export function renderPdfConfiguredMulti(...args){return result(args[0],args,'application/pdf');}
export function renderPdfConfiguredPage(...args){return result(args[0],args,'application/pdf');}
`;
function realWorker(code,stats) {
  stats.started++;
  const thread=new Thread(`
    const {parentPort}=require('node:worker_threads');
    globalThis.self=globalThis;
    globalThis.Blob=class{constructor(parts){this.text=parts.join('');}};
    URL.createObjectURL=blob=>'data:text/javascript;base64,'+Buffer.from(blob.text).toString('base64');
    URL.revokeObjectURL=()=>{};
    globalThis.postMessage=(data,transfers)=>parentPort.postMessage(data,transfers);
    parentPort.on('message',data=>globalThis.onmessage({data}));
    ${code}
  `,{eval:true});
  const listeners=new Map();
  return {addEventListener(type,fn){const wrapped=value=>fn(type==='message'?{data:value}:value);listeners.set(fn,wrapped);thread.on(type,wrapped);},
    removeEventListener(type,fn){const wrapped=listeners.get(fn);if(wrapped)thread.off(type,wrapped);listeners.delete(fn);},
    postMessage:value=>thread.postMessage(value),terminate:()=>{stats.stopped++;return thread.terminate();}};
}
async function environment(run,{timeoutMs=1500,background=true,legacyWorker=false}={}) {
  const keys=['window','document','URL','mainExportTestCalls'];
  const old=keys.map(key=>Object.getOwnPropertyDescriptor(globalThis,key));
  const stats={started:0,stopped:0,revoked:0};
  const payload={version:1,bindings:bindingSource,wasm:'AGFzbQEAAAA=',options:{font:'serif',darkMode:'auto',fontScale:1.25,
    title:'Owned title',author:'A',lang:'fr',metadataEpochSeconds:0,toc:true,tocDepth:4,pageNumbers:true,codeLineNumbers:true,
    pageGeometry:[792,612,24,36,48,60]},images:[{destination:'plot.png',bytes:'AQID'}],fonts:[{slot:'body-regular',weight:555,bytes:'BAUG'}]};
  const data={textContent:JSON.stringify(payload)};
  const preview={childNodes:[],replaceChildren(...children){this.childNodes=children;}};
  globalThis.mainExportTestCalls=[];
  globalThis.window=new EventTarget();
  globalThis.document={querySelector:()=>data,createElement:()=>({style:{},setAttribute(){},addEventListener(){}})};
  globalThis.URL={createObjectURL:()=> 'data:text/javascript;base64,'+Buffer.from(bindingSource).toString('base64'),revokeObjectURL(){stats.revoked++;}};
  const clients=[];
  const makeWorker=(factory,saved)=>{
    const client=legacyWorker?{dispose(){stats.stopped++;}}:createWorkspacePreviewWorker(factory,saved,{timeoutMs,workerFactory:code=>realWorker(code,stats)});
    clients.push(client);return client;
  };
  try {
    bootNativeWorkspace(createNativeWorkspaceRenderer,background?makeWorker:undefined);
    const engine=window.__fmdNativeRuntime;
    assert.throws(()=>engine.exportDocument('pdf','startup'),/loading/);
    assert.equal(await engine.ready,true);
    await run({engine,data,preview,stats,clients,payload,calls:globalThis.mainExportTestCalls});
  } finally {
    for(const client of clients)client.dispose();
    keys.forEach((key,i)=>{if(old[i])Object.defineProperty(globalThis,key,old[i]);else delete globalThis[key];});
  }
}
const rejected=(promise,code)=>assert.rejects(promise,error=>error.code===code);
const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const args=result=>JSON.parse(new TextDecoder().decode(result.bytes).split('\n')[1]);

test('background PDF captures exact source, page, fonts, metadata and images without main-thread rendering',async()=>{
  await environment(async({engine,calls,stats})=>{
    const source='\ufeff\r\nCafé 😀\r';
    const result=await engine.exportDocument('pdf',source);
    const a=args(result);
    assert.equal(a.length,28);assert.equal(a[0],source);assert.equal(a[1],'serif');
    assert.equal(a[3],'Owned title');assert.equal(a[4],'A');assert.equal(a[5],0);
    assert.equal(a[7],true);assert.deepEqual(a[8],['plot.png']);assert.deepEqual(a[9],[1,2,3]);
    assert.deepEqual(a[11],[4,5,6]);assert.equal(a[16][0],555);
    assert.deepEqual(a.slice(20,25),[true,1.25,'fr',true,4]);assert.deepEqual(a[27],[792,612,24,36,48,60]);
    assert.deepEqual(calls,[]);assert.equal(engine.exportPending,false);assert.equal(stats.started,1);assert.equal(stats.stopped,1);
  });
});

test('publication emits full script-restricted HTML with document rather than view settings',async()=>{
  await environment(async({engine,preview,calls})=>{
    await engine.render('preview',preview,{scale:2,theme:'dark'});
    const result=await engine.exportDocument('html','publish');
    const html=new TextDecoder().decode(result.bytes);
    assert.match(html,/Content-Security-Policy/);assert.match(html,/default-src 'none'/);
    assert.match(html,/1.25/);assert.doesNotMatch(html,/iframe|fmd-native-runtime|2.5/);assert.deepEqual(calls,[]);
  });
});

test('export findings are returned with the result without replacing current preview diagnostics',async()=>{
  await environment(async({engine,preview})=>{
    await engine.render('preview',preview,{scale:1});const before=engine.diagnostics;
    const result=await engine.exportDocument('pdf','export');
    assert.equal(result.diagnostics[0].message,'application/pdf:export');
    assert.equal(engine.diagnostics,before);assert.equal(engine.diagnostics[0].message,'text/html:preview');
  });
});

test('explicit cancellation kills only the export and allows a fresh independent retry',async()=>{
  await environment(async({engine,preview,stats})=>{
    await engine.render('preview',preview,{scale:1});
    const pending=rejected(engine.exportDocument('pdf','busy'),'EXPORT_CANCELLED');
    await wait(80);engine.cancelExport();await pending;
    assert.equal(engine.exportPending,false);assert.equal(stats.stopped,1);
    assert.equal(await engine.render('after cancel',preview,{scale:1}),true);
    assert.equal(args(await engine.exportDocument('pdf','retry'))[0],'retry');
    assert.equal(stats.started,3);assert.equal(stats.stopped,2);
  });
});

test('cancellation before the first asynchronous step never initializes an export worker',async()=>{
  await environment(async({engine,stats})=>{
    const pending=rejected(engine.exportDocument('pdf','x'),'EXPORT_CANCELLED');engine.cancelExport();await pending;
    assert.equal(stats.started,0);assert.equal(engine.exportPending,false);
  });
});

test('one active export rejects overlap without altering the pending operation',async()=>{
  await environment(async({engine})=>{
    const pending=engine.exportDocument('pdf','x');
    assert.equal(engine.exportPending,true);
    assert.throws(()=>engine.exportDocument('html','x'),error=>error.code==='EXPORT_BUSY');
    assert.equal(args(await pending)[0],'x');
  });
});

test('revision callback rejects undispatched source edits and output arriving after cancellation',async()=>{
  await environment(async({engine})=>{
    let current=true;const pending=rejected(engine.exportDocument('html','captured',()=>current),'EXPORT_CANCELLED');
    current=false;await pending;assert.equal(engine.exportPending,false);
    await rejected(engine.exportDocument('pdf','x',()=>false),'EXPORT_CANCELLED');
  });
});

test('settings commits cancel export snapshots while failed settings validation does not',async()=>{
  await environment(async({engine,preview,data})=>{
    const pending=rejected(engine.exportDocument('pdf','busy'),'EXPORT_CANCELLED');await wait(60);
    engine.applySettings({fontScale:1.75},'settings',preview,{});await pending;
    assert.equal(JSON.parse(data.textContent).options.fontScale,1.75);
    const next=engine.exportDocument('pdf','after');
    assert.throws(()=>engine.applySettings({fontScale:99},'x',preview,{}));
    assert.equal(args(await next)[21],1.75);
  });
});

test('image commit and rollback both invalidate pending export resource snapshots',async()=>{
  await environment(async({engine,data})=>{
    const tx=engine.stageImages([{destination:'new.png',bytes:Uint8Array.of(9,8)}]);
    const pending=rejected(engine.exportDocument('pdf','busy'),'EXPORT_CANCELLED');await wait(60);
    tx.commit();await pending;
    const updated=args(await engine.exportDocument('pdf','with new'));
    assert.deepEqual(updated[8],['plot.png','new.png']);assert.deepEqual(updated[9],[1,2,3,9,8]);
    const rollback=rejected(engine.exportDocument('pdf','busy'),'EXPORT_CANCELLED');await wait(60);
    tx.rollback();await rollback;
    assert.equal(JSON.parse(data.textContent).images.length,1);
    assert.deepEqual(args(await engine.exportDocument('pdf','restored'))[8],['plot.png']);
  });
});

test('page suspension cancels exports and resumption never restarts a stale request',async()=>{
  await environment(async({engine,stats})=>{
    const pending=rejected(engine.exportDocument('pdf','busy'),'EXPORT_CANCELLED');await wait(80);
    window.dispatchEvent(new Event('pagehide'));await pending;const started=stats.started;
    assert.throws(()=>engine.exportDocument('pdf','suspended'),error=>error.code==='EXPORT_SUSPENDED');
    window.dispatchEvent(new Event('pageshow'));await wait(20);assert.equal(stats.started,started);
    assert.equal(args(await engine.exportDocument('pdf','explicit retry'))[0],'explicit retry');
  });
});

test('native refusal and worker timeout release resources and leave source/settings saveable',async()=>{
  await environment(async({engine,data,stats,calls})=>{
    const initial=data.textContent;
    await rejected(engine.exportDocument('pdf','throw'),'EXPORT_RENDER_FAILED');
    await rejected(engine.exportDocument('pdf','busy'),'EXPORT_TIMEOUT');
    assert.equal(data.textContent,initial);assert.equal(engine.exportPending,false);assert.deepEqual(calls,[]);
    assert.equal(stats.started,2);assert.equal(stats.stopped,2);
    assert.equal(args(await engine.exportDocument('pdf','good'))[0],'good');
  },{timeoutMs:300});
});

test('workers missing the export method reject explicitly without calling synchronous native APIs',async()=>{
  await environment(async({engine,calls,stats})=>{
    await rejected(engine.exportDocument('pdf','x'),'UNSUPPORTED_WASM_PACKAGE');
    assert.equal(engine.exportPending,false);assert.deepEqual(calls,[]);assert.equal(stats.stopped,1);
  },{legacyWorker:true});
});

test('legacy synchronous bootstrap remains explicitly synchronous with original PDF/HTML interfaces',async()=>{
  await environment(async({engine,calls})=>{
    assert.equal(engine.exportMode,'synchronous');
    assert.throws(()=>engine.exportDocument('pdf','x'),error=>error.code==='UNSUPPORTED_WASM_PACKAGE');
    assert.ok(engine.pdf('x') instanceof Uint8Array);assert.match(engine.html('x'),/<html>/);
    assert.deepEqual(calls,['application/pdf','text/html']);
  },{background:false});
});

test('export admission rejects unsupported formats and malformed Unicode without spawning a worker',async()=>{
  await environment(async({engine,stats})=>{
    assert.throws(()=>engine.exportDocument('epub','x'),error=>error.code==='EXPORT_OPTIONS');
    await rejected(engine.exportDocument('pdf','\ud800'),'EXPORT_UNICODE');
    assert.equal(stats.started,0);assert.equal(engine.exportPending,false);
  });
});
