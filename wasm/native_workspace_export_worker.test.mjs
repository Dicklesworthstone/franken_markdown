// Production worker code + real worker_threads and WASM startup. Native renderer
// calls and browser Blob module loading are explicit adapters, not Rust proof.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Worker as Thread} from 'node:worker_threads';
import vm from 'node:vm';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

const payload = {version: 1, options: {font: 'serif', fontScale: 1.25}, images: [], fonts: [],
  wasm: 'AGFzbQEAAAA=', bindings: `
let ready = false;
export default async function({module_or_path}) { await WebAssembly.instantiate(module_or_path); ready = true; }
export function run(source, options, images, display) {
  if (!ready) throw Error('WASM not initialized');
  if (source === 'throw') throw Error('Native failure');
  if (source === 'busy') while (true) {}
  return JSON.stringify({source, options, images, display});
}`};
const state = {options: payload.options, images: payload.images};
function renderer(bindings, data) {
  if (data.options.font === 'invalid') throw Error('Bad state');
  return {
    html: (source, display) => source === 'unicode' ? '\ud800' : '<html><head></head><body>' + bindings.run(source, data.options, data.images, display) + '</body></html>',
    pdf(source) {
      if (source === 'bad-pdf') return Uint8Array.of(1, 2, 3);
      // Deliberately return a selected range; adjacent pool bytes must not leak.
      const text = new TextEncoder().encode('XX%PDF-ADAPTER\n' + bindings.run(source, data.options, data.images) + 'YY');
      return text.subarray(2, text.length - 2);
    },
    diagnostics: [{message: 'native finding', code: 'fixture'}],
  };
}
function threadWorker(code, records) {
  const thread = new Thread(`
    const {parentPort} = require('node:worker_threads');
    globalThis.self = globalThis;
    globalThis.Blob = class { constructor(parts) {this.text = parts.join('');} };
    URL.createObjectURL = blob => 'data:text/javascript;base64,' + Buffer.from(blob.text).toString('base64');
    URL.revokeObjectURL = () => {};
    globalThis.postMessage = (value, transfers) => {
      parentPort.postMessage(value, transfers);
      if (transfers?.length) parentPort.postMessage({audit:true, detached:transfers.every(buffer=>buffer.byteLength===0)});
    };
    parentPort.on('message', data => globalThis.onmessage({data}));
    ${code}
  `, {eval: true});
  const wrappers = new Map();
  return {
    addEventListener(type, fn) {
      const handler = value => {
        if (value?.audit) { records.push(value); return; }
        fn(type === 'message' ? {data:value} : value);
      };
      wrappers.set(fn, handler); thread.on(type, handler);
    },
    removeEventListener(type, fn) { const handler=wrappers.get(fn); if(handler) thread.off(type,handler); wrappers.delete(fn); },
    postMessage: value => thread.postMessage(value),
    terminate: () => { records.push({terminated:true}); return thread.terminate(); },
  };
}
function real(configuration={}) {
  const records=[];
  const client=createWorkspacePreviewWorker(renderer,payload,{workerFactory:code=>threadWorker(code,records),...configuration});
  return {client,records};
}
function fake() {
  const events=new Map(),messages=[]; let terminated=0;
  const worker={addEventListener:(key,fn)=>events.set(key,fn),removeEventListener:(key)=>events.delete(key),
    postMessage:value=>messages.push(structuredClone(value)),terminate:()=>{terminated++;}};
  const client=createWorkspacePreviewWorker(renderer,payload,{workerFactory:()=>worker});
  const emit=data=>events.get('message')?.({data});
  const done=(id,format='pdf',bytes=new TextEncoder().encode('%PDF-result'))=>emit({type:'result',id,format,bytes,
    mimeType:format==='pdf'?'application/pdf':'text/html;charset=utf-8',diagnostics:[]});
  return {client,events,messages,emit,done,get terminated(){return terminated;}};
}
const reject=(promise,code)=>assert.rejects(promise,error=>error.code===code);
const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));

test('PDF runs in a real worker and transfers only owned selected bytes',async()=>{
  const {client,records}=real();
  try {
    const source='\ufeff\r\nCafé 😀\r';
    const result=await client.exportDocument('pdf',source,state);
    assert.equal(result.format,'pdf'); assert.equal(result.mimeType,'application/pdf');
    const text=new TextDecoder().decode(result.bytes);
    assert.ok(text.startsWith('%PDF-ADAPTER\n')); assert.ok(!text.endsWith('YY'));
    assert.equal(JSON.parse(text.split('\n')[1]).source,source);
    assert.equal(result.bytes.byteOffset,0); assert.equal(result.bytes.buffer.byteLength,result.bytes.byteLength);
    await wait(20); assert.ok(records.some(record=>record.detached===true),'worker transferred its owned buffer');
    assert.equal(result.diagnostics[0].code,'fixture');
  } finally {client.dispose();}
});

test('HTML publication ignores preview viewing settings and preserves committed options/resources',async()=>{
  const {client}=real();
  const next={options:{font:'sans',fontScale:1.75},images:[{destination:'chart.png',bytes:'AQI='}]};
  try {
    await client.render('view',{scale:2,theme:'dark'},state);
    const result=await client.exportDocument('html','published',next);
    const html=new TextDecoder().decode(result.bytes);
    assert.match(html,/<html><head>/); assert.match(html,/"fontScale":1.75/); assert.match(html,/chart.png/);
    assert.doesNotMatch(html,/"display"/); assert.equal(result.mimeType,'text/html;charset=utf-8');
  } finally {client.dispose();}
});

test('a busy export rejects additional work instead of superseding it or queuing arbitrarily',async()=>{
  const s=fake(); const first=s.client.exportDocument('pdf','first',state);
  await reject(s.client.exportDocument('html','second',state),'EXPORT_BUSY');
  await reject(s.client.render('preview',{scale:1},state),'EXPORT_BUSY');
  assert.equal(s.client.pendingOperations,1); assert.equal(s.messages.length,1);
  s.emit({type:'ready'}); s.done(1); await first; s.client.dispose();
});

test('an export cannot overwrite work already queued on the same client',async()=>{
  const s=fake(); const preview=s.client.render('first',{scale:1},state);
  await reject(s.client.exportDocument('pdf','second',state),'EXPORT_BUSY');
  s.emit({type:'ready'}); s.emit({type:'result',id:1,html:'preview',diagnostics:[]});
  assert.equal((await preview).html,'preview');s.client.dispose();
});

test('source and format admission precede worker startup and preserve active work',async()=>{
  const s=fake();
  for(const [format,source,code] of [['svg','x','EXPORT_OPTIONS'],['pdf','\ud800','EXPORT_UNICODE'],
    ['html','é'.repeat(16*1024*1024+1),'EXPORT_LIMIT']]) await reject(s.client.exportDocument(format,source,state),code);
  assert.equal(s.messages.length,0);
  const pending=s.client.exportDocument('html','valid',state);
  await reject(s.client.exportDocument('zip','x',state),'EXPORT_OPTIONS');
  s.emit({type:'ready'});s.done(1,'html',new TextEncoder().encode('<html/>')); await pending; s.client.dispose();
});

test('explicit invalidation terminates an export and discards all late output',async()=>{
  const s=fake();const pending=reject(s.client.exportDocument('pdf','source',state),'EXPORT_CANCELLED');
  s.emit({type:'ready'});s.client.invalidate();await pending;
  assert.equal(s.terminated,1);assert.equal(s.events.size,0);assert.equal(s.client.pendingOperations,0);
  s.done(1);s.client.dispose();assert.equal(s.terminated,1);
  await reject(s.client.exportDocument('pdf','later',state),'EXPORT_CLOSED');
});

test('disposal during startup and execution settles export consumers with export-scoped errors',async()=>{
  for(const started of [false,true]) {
    const s=fake();const pending=reject(s.client.exportDocument('pdf','source',state),'EXPORT_CLOSED');
    if(started)s.emit({type:'ready'});s.client.dispose();await pending;
    assert.equal(s.events.size,0);assert.equal(s.terminated,1);
  }
});

test('worker constructor, initialization and transport failures have no synchronous fallback',async()=>{
  const client=createWorkspacePreviewWorker(renderer,payload,{workerFactory(){throw Error('blocked');}});
  await reject(client.exportDocument('pdf','x',state),'EXPORT_START_FAILED');
  const s=fake();const pending=reject(s.client.exportDocument('pdf','x',state),'EXPORT_START_FAILED');
  s.emit({type:'error',message:'init rejected'});await pending;assert.equal(s.terminated,1);
  const t=fake();const active=reject(t.client.exportDocument('pdf','x',state),'EXPORT_WORKER_FAILED');
  t.events.get('error')({preventDefault(){}});await active;assert.equal(t.terminated,1);
});

test('a hung native export is terminated while the host event loop remains responsive',async()=>{
  const {client,records}=real({timeoutMs:350});
  try {
    await client.exportDocument('pdf','warm',state);
    let beats=0;const clock=setInterval(()=>beats++,10);
    try {await reject(client.exportDocument('pdf','busy',state),'EXPORT_TIMEOUT');}
    finally {clearInterval(clock);}
    assert.ok(beats>5);assert.equal(client.closed,true);assert.equal(client.pendingOperations,0);
    assert.ok(records.some(record=>record.terminated));
  } finally {client.dispose();}
});

test('preview and export clients are independent even when native PDF computation is blocked',async()=>{
  const preview=real(),exp=real({timeoutMs:500});
  try {
    await Promise.all([preview.client.render('warm',{scale:1},state),exp.client.exportDocument('pdf','warm',state)]);
    const pending=reject(exp.client.exportDocument('pdf','busy',state),'EXPORT_TIMEOUT');
    const result=await preview.client.render('still editable',{scale:1},state);
    assert.match(result.html,/still editable/);await pending;
    assert.equal(preview.client.closed,false);
  } finally {preview.client.dispose();exp.client.dispose();}
});

test('native errors and invalid resource revisions allow explicit retries without poisoned state',async()=>{
  const {client}=real();
  try {
    await reject(client.exportDocument('pdf','throw',state),'EXPORT_RENDER_FAILED');
    await reject(client.exportDocument('pdf','source',{options:{font:'invalid'},images:[]}),'EXPORT_RENDER_FAILED');
    const ok=await client.exportDocument('pdf','recovered',state);
    assert.match(new TextDecoder().decode(ok.bytes),/recovered/);assert.equal(client.closed,false);
  } finally {client.dispose();}
});

test('native invalid PDFs and malformed HTML Unicode cannot become downloads',async()=>{
  const {client}=real();
  try {
    await reject(client.exportDocument('pdf','bad-pdf',state),'EXPORT_RENDER_FAILED');
    await reject(client.exportDocument('html','unicode',state),'EXPORT_RENDER_FAILED');
  } finally {client.dispose();}
});

test('host verifies format, MIME, PDF signature, exact ownership and diagnostic limits',async()=>{
  const bytes=new TextEncoder().encode('%PDF-result');
  const good={type:'result',id:1,format:'pdf',mimeType:'application/pdf',bytes,diagnostics:[]};
  for(const change of [{format:'html'},{mimeType:'text/html'},{bytes:Uint8Array.of(1)},
    {bytes:new Uint8Array()},{bytes:bytes.subarray(0,5)},
    {diagnostics:[null]},{diagnostics:[{message:'x',detail:'y'.repeat(1024*1024)}]},
    {bytes:new Uint8Array(new SharedArrayBuffer(8))}]) {
    const s=fake();const pending=assert.rejects(s.client.exportDocument('pdf','x',state));
    s.emit({type:'ready'});s.emit({...good,...change});await pending;
    assert.equal(s.client.closed,true);assert.equal(s.terminated,1);
  }
});

test('committed state is resent after a rejected export revision',async()=>{
  const s=fake();const a=reject(s.client.exportDocument('pdf','bad',state),'EXPORT_RENDER_FAILED');
  s.emit({type:'ready'});s.emit({type:'error',id:1,message:'rejected'});await a;
  const b=s.client.exportDocument('pdf','retry',state);
  assert.deepEqual(s.messages.at(-1).state,state);s.done(2);await b;
  const c=s.client.exportDocument('pdf','again',state);
  assert.equal(s.messages.at(-1).state,null);s.done(3);await c;s.client.dispose();
});

test('factory including export transport stays self-contained when serialized into HTML',async()=>{
  const factory=vm.runInNewContext('('+createWorkspacePreviewWorker.toString()+')',{setTimeout,clearTimeout});
  await reject(factory(renderer,payload).exportDocument('pdf','x',state),'EXPORT_START_FAILED');
  assert.doesNotMatch(createWorkspacePreviewWorker.toString(),/<\/script/i);
});
