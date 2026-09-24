import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeWorkspaceExporter, createWorkspaceExporterWithLoader, NATIVE_WORKSPACE_LIMITS as limits} from './interactive_export.mjs';

const magic = Uint8Array.from([0,97,115,109,1,0,0,0]);
const json = value => JSON.stringify(value).replace(/</g,'\\u003c').replace(/\u2028/g,'\\u2028').replace(/\u2029/g,'\\u2029');
const escape = value => value.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function shell(source, script = 'window.__fmdNativeRuntime; /* fmd-async-preview-v1 */') {
  return '<!DOCTYPE html>\n<html><head><title>Fixture</title></head><body>\n'
    + '<header class="fmd-app-header"></header><div class="fmd-app-body view-split" id="fmd-app-body">\n'
    + '  <section id="editor-pane"><textarea id="fmd-editor">' + escape(source) + '</textarea></section>\n'
    + '  <main id="preview-pane">\n    <div class="fmd-content" id="fmd-content"><img src="https://external.invalid/initial.png">'
    + '</div>\n  </main>\n</div>\n'
    + '<script type="application/json" id="fmd-raw-source">' + json(source) + '</script>\n'
    + '<script>\n' + script + '\n</script>\n</body>\n</html>\n';
}
function result(text, diagnostics = [], mimeType = 'text/html;charset=utf-8') {
  const bytes = new TextEncoder().encode(text);
  return {bytes, mimeType, diagnosticsJson: () => JSON.stringify(diagnostics), free(){ this.freed = (this.freed || 0) + 1; bytes.fill(0); }};
}
function adapter() {
  const calls = [], outputs = [];
  const engine = {
    renderInteractiveHtmlConfigured(...args) {calls.push(['shell',args]); const value=result(shell(args[0]));outputs.push(value);return value;},
    renderHtmlConfiguredAdvanced(...args) {calls.push(['html',args]); const value=result('<!DOCTYPE html><html><head><style>@media (prefers-color-scheme: dark) {body{color:white}}</style></head><body><h1>'+escape(args[0])+'</h1></body></html>',[{message:'Native finding'}]);outputs.push(value);return value;},
    renderPdfConfiguredMulti(){return result('%PDF-1.7\nfixture',[],'application/pdf');},
  };
  return {engine,calls,outputs};
}
async function setup(runtime = {wasm:magic,bindings:'export default async function init() {}'}) {
  const a=adapter();return {...a, exporter:await createWorkspaceExporterWithLoader(runtime,async()=>a.engine)};
}
function payload(output) {
  const open='<script type="application/json" id="fmd-native-runtime">';
  const html=output.text(); const start=html.indexOf(open)+open.length;
  assert.ok(start>=open.length);return JSON.parse(html.slice(start,html.indexOf('</script>',start)));
}
function sourceData(output) {
  const open='<script type="application/json" id="fmd-raw-source">';
  const html=output.text();const start=html.indexOf(open)+open.length;
  return JSON.parse(html.slice(start,html.indexOf('</script>',start)));
}

test('exports the complete source/runtime/options payload with a sandboxed first frame', async()=>{
  const {exporter,calls,outputs}=await setup();
  const output=exporter.render('# Native'); const html=output.text(), data=payload(output);
  assert.equal(data.version,1);assert.equal(data.wasm,Buffer.from(magic).toString('base64'));
  assert.equal(data.options.fontScale,1);assert.equal(sourceData(output),'# Native');
  assert.equal(output.sourceLength,8);assert.equal(output.format,'interactive-html');assert.equal(output.extension,'html');
  assert.ok(html.includes('sandbox="allow-same-origin"'));
  assert.match(html, /<head>\n<meta http-equiv="Content-Security-Policy"/);
  assert.ok(html.includes("'wasm-unsafe-eval'"));assert.ok(!html.includes("'unsafe-eval'"));
  assert.ok(html.includes('Content-Security-Policy'));assert.ok(html.includes('default-src &#39;none&#39;'));
  assert.ok(!html.includes('src="https://external.invalid/initial.png"'));
  assert.ok(html.indexOf('id="fmd-native-bootstrap"')<html.indexOf('<script>\nwindow.__fmdNativeRuntime'));
  assert.ok(html.includes('bootNativeWorkspace'));assert.ok(html.includes('createNativeWorkspaceRenderer'));
  assert.deepEqual(calls.map(c=>c[0]),['shell','html']);assert.ok(outputs.every(o=>o.freed===1));
  assert.equal(output.diagnostics[0].message,'Native finding');
  assert.equal(await output.blob().text(),output.text());
});

test('source, metadata and trusted binding text cannot escape inert data blocks',async()=>{
  const source='\n\r\n\0</ScRiPt><script>globalThis.sourceRan=true</script>\u2028\u2029é中😀';
  const binding='export default function(){ /* </ScRiPt><!-- </textarea> */ }\u2028';
  const {exporter}=await setup({wasm:magic,bindings:binding});
  const output=exporter.render(source,{title:'</ScRiPt><img src=x onerror=bad>',author:'  Exact author  '});
  assert.equal(sourceData(output),source);assert.equal(payload(output).bindings,binding);
  assert.equal(payload(output).options.author,'  Exact author  ');
  const start=output.text().indexOf('<script type="application/json" id="fmd-native-runtime">');
  const block=output.text().slice(start,output.text().indexOf('</script>',start));
  assert.ok(!block.includes('</ScRiPt>'));assert.ok(!block.includes('<!--'));assert.ok(!block.includes('\u2028'));
  assert.equal(output.sourceLength,new TextEncoder().encode(source).length);
});

test('runtime is captured before async loading, including exact subarray boundaries',async()=>{
  let release,received;
  const underlying=new Uint8Array([99,...magic,88]);const runtime={wasm:underlying.subarray(1,9),bindings:'old binding text'};
  const a=adapter();
  const pending=createWorkspaceExporterWithLoader(runtime,owned=>{received=owned;return new Promise(resolve=>release=()=>resolve(a.engine));});
  underlying.fill(7);runtime.bindings='changed';runtime.wasm=new Uint8Array([0]);
  assert.deepEqual([...received.wasm],[...magic]);release();
  const output=(await pending).render('Stable');assert.equal(payload(output).bindings,'old binding text');
  assert.equal(payload(output).wasm,Buffer.from(magic).toString('base64'));
});

test('resources copy exact typed/DataView byte ranges and retain unused bindings',async()=>{
  const {exporter,calls}=await setup();
  const backing=Uint8Array.from([9,1,2,3,8]);
  const output=exporter.render('No image references',{pdfImages:[{destination:' __proto__ ',bytes:new DataView(backing.buffer,1,3)}],
    fontAssets:[{slot:'mono-regular',bytes:backing.subarray(2,4),weight:550},{slot:'body-regular',bytes:backing.subarray(1,2)}]});
  backing.fill(0);
  const data=payload(output);
  assert.deepEqual(data.images,[{destination:'__proto__',bytes:'AQID'}]);
  assert.deepEqual(data.fonts,[{slot:'body-regular',bytes:'AQ=='},{slot:'mono-regular',weight:550,bytes:'AgM='}]);
  const args=calls.find(c=>c[0]==='html')[1];
  assert.deepEqual(args[13],['__proto__']);assert.deepEqual([...args[14]],[1,2,3]);assert.deepEqual([...args[15]],[3]);
  assert.deepEqual([...args[7]],[1]);assert.deepEqual([...args[11]],[2,3]);assert.deepEqual([...args[12]],[0,0,0,0,550]);
});

test('normalized settings reach the shell, preview and persisted native PDF configuration',async()=>{
  const {exporter,calls}=await setup();
  const options={font:'serif',darkMode:'disabled',fontScale:1.25,title:'  Title  ',author:'A',lang:'fr',toc:true,tocDepth:4,
    metadataEpochSeconds:0,codeLineNumbers:true,pageNumbers:true};
  const data=payload(exporter.render('Settings',options));
  assert.deepEqual(data.options,options);
  assert.deepEqual(calls[0][1],['Settings','serif','disabled','  Title  ','fr',1.25]);
  const html=calls[1][1];assert.equal(html[5],false);assert.equal(html[6],1.25);assert.equal(html[16],'fr');assert.equal(html[17],true);assert.equal(html[18],4);
});

test('stable inputs produce identical bytes and resources cannot leak across renders',async()=>{
  const {exporter}=await setup();const options={pdfImages:[{destination:'a.png',bytes:Uint8Array.of(1,2)}]};
  const first=exporter.render('Repeated',options);const second=exporter.render('Repeated',options);
  assert.deepEqual(first.bytes,second.bytes);assert.notEqual(first.bytes,second.bytes);
  assert.deepEqual(payload(exporter.render('Other')).images,[]);
  assert.equal(payload(first).images.length,1);assert.equal(sourceData(first),'Repeated');
});

test('preflight refuses malformed runtimes without invoking the loader',async()=>{
  let calls=0;
  for(const runtime of [null,{},[],{wasm:magic,bindings:''},{wasm:Uint8Array.of(1,2),bindings:'js'},
    {wasm:magic,bindings:42},{wasm:magic,bindings:'\ud800'},{wasm:'url-to-binary',bindings:'js'},
    {wasm:Uint8Array.from([0,97,115,109,2,0,0,0]),bindings:'js'}]) {
    await assert.rejects(createWorkspaceExporterWithLoader(runtime,async()=>{calls++;return adapter().engine;}));
  }
  assert.equal(calls,0);
});

test('oversized runtime views/binding text fail before load and shared/detached memory is refused',async()=>{
  let calls=0;const load=async()=>{calls++;return adapter().engine;};
  await assert.rejects(createWorkspaceExporterWithLoader({wasm:new Uint8Array(limits.wasmBytes+1),bindings:'x'},load),/limit/);
  await assert.rejects(createWorkspaceExporterWithLoader({wasm:magic,bindings:'x'.repeat(limits.bindingsBytes+1)},load),/limit/);
  if(typeof SharedArrayBuffer==='function') await assert.rejects(createWorkspaceExporterWithLoader({wasm:new Uint8Array(new SharedArrayBuffer(8)),bindings:'x'},load),/shared/);
  const detached=magic.slice();structuredClone(detached,{transfer:[detached.buffer]});
  await assert.rejects(createWorkspaceExporterWithLoader({wasm:detached,bindings:'x'},load));assert.equal(calls,0);
});

test('missing required ABI functions fail before publishing an exporter',async()=>{
  for(const key of ['renderInteractiveHtmlConfigured','renderHtmlConfiguredAdvanced','renderPdfConfiguredMulti']) {
    const a=adapter();delete a.engine[key];
    await assert.rejects(createWorkspaceExporterWithLoader({wasm:magic,bindings:'x'},async()=>a.engine),error=>error.code==='UNSUPPORTED_WASM_PACKAGE');
  }
});

test('unsupported options and invalid scalar settings cannot be silently discarded',async()=>{
  const {exporter,calls}=await setup();
  for(const options of [null,[],{allowRawHtml:true},{allowRawHtml:1},{customCss:'x'},{page:{size:'a4'}},
    {font:'system'},{darkMode:'dark'},{fontScale:NaN},{fontScale:0},{fontScale:4},{fontScale:'large'},
    {metadataEpochSeconds:-1},{metadataEpochSeconds:1.5},{metadataEpochSeconds:Number.MAX_SAFE_INTEGER+1},
    {tocDepth:0},{tocDepth:7},{tocDepth:1.5},{pageNumbers:'yes'},{codeLineNumbers:1},{toc:null},{title:42},{lang:'\ud800'}]) {
    assert.throws(()=>exporter.render('x',options));
  }
  assert.equal(calls.length,0);
});

test('source and metadata limits are byte-based and reject lossy Unicode',async()=>{
  const {exporter,calls}=await setup();
  for(const source of [42,'\ud800','x'.repeat(limits.sourceBytes+1),'é'.repeat(limits.sourceBytes/2+1)]) assert.throws(()=>exporter.render(source));
  assert.throws(()=>exporter.render('x',{title:'é'.repeat(32769)}),/limit/);
  assert.throws(()=>exporter.render('x',{lang:'x'.repeat(1025)}),/limit/);assert.equal(calls.length,0);
});

test('asset counts are checked before payload getters and all sizes before base64 allocation',async()=>{
  const {exporter,calls}=await setup();let reads=0;
  const assets=Array(limits.imageCount+1);Object.defineProperty(assets,0,{get(){reads++;throw Error('accessed');}});
  assert.throws(()=>exporter.render('x',{pdfImages:assets}),/4096/);assert.equal(reads,0);
  assert.throws(()=>exporter.render('x',{fontAssets:Array(6)}),/five/);
  assert.throws(()=>exporter.render('x',{pdfImages:[{destination:'a',bytes:new Uint8Array(limits.assetBytes+1)}]}),/limit/);
  const bytes=new Uint8Array(limits.assetBytes);
  assert.throws(()=>exporter.render('x',{pdfImages:Array.from({length:5},(_,i)=>({destination:String(i),bytes}))}),/128 MiB/);
  assert.equal(calls.length,0);
});

test('duplicate, empty, controlled and excessive asset identities are refused',async()=>{
  const {exporter,calls}=await setup();const bytes=Uint8Array.of(1);
  for(const images of [[{destination:' ',bytes}],[{destination:'a\0b',bytes}],
    [{destination:' a ',bytes},{destination:'a',bytes}],Array(2),[{destination:'x'.repeat(8193),bytes}],
    Array.from({length:9},(_,i)=>({destination:String(i)+'x'.repeat(8191),bytes}))]) assert.throws(()=>exporter.render('x',{pdfImages:images}));
  for(const fontAssets of [[{slot:'invalid',bytes}],[{slot:'mono-regular',bytes,weight:0}],[{slot:'mono-regular',bytes,weight:1001}],
    [{slot:'body-regular',bytes},{slot:'body-regular',bytes}]]) assert.throws(()=>exporter.render('x',{fontAssets}));
  assert.equal(calls.length,0);
});

test('empty, detached and shared resource byte views never reach native code',async()=>{
  const {exporter,calls}=await setup();const detached=Uint8Array.of(1);structuredClone(detached,{transfer:[detached.buffer]});
  const values=[new Uint8Array(),detached,'https://example.invalid/file',[1,2,3]];
  if(typeof SharedArrayBuffer==='function') values.push(new Uint8Array(new SharedArrayBuffer(1)));
  for(const bytes of values) assert.throws(()=>exporter.render('x',{pdfImages:[{destination:'a',bytes}]}));
  assert.equal(calls.length,0);
});

test('legacy or malformed shells fail closed and generated result objects are freed',async()=>{
  for(const transform of [s=>s.replace('__fmdNativeRuntime','oldRenderer'),s=>s.replace('id="fmd-content"','id="missing"'),
    s=>s.replace('id="fmd-raw-source"','id="missing"'),s=>s.replace('"source"','"wrong"')]) {
    const a=adapter();let emitted;
    a.engine.renderInteractiveHtmlConfigured=()=>emitted=result(transform(shell('source')));
    const exporter=await createWorkspaceExporterWithLoader({wasm:magic,bindings:'x'},async()=>a.engine);
    assert.throws(()=>exporter.render('source'));assert.equal(emitted.freed,1);
  }
});

test('invalid native envelopes, invalid UTF-8 and diagnostic failures release results',async()=>{
  for(const bad of [{bytes:new TextEncoder().encode('bad'),mimeType:'application/pdf'},
    {bytes:Uint8Array.of(255),mimeType:'text/html'}, {bytes:new Uint8Array(),mimeType:'text/html'},
    {bytes:new TextEncoder().encode(shell('x')),mimeType:'text/html',diagnosticsJson:()=>'{bad'},
    {bytes:new TextEncoder().encode(shell('x')),mimeType:'text/html',diagnosticsJson:()=>'{}'}]) {
    let freed=0;const a=adapter();
    a.engine.renderInteractiveHtmlConfigured=()=>({diagnosticsJson:()=>'[]',...bad,free(){freed++;}});
    const exporter=await createWorkspaceExporterWithLoader({wasm:magic,bindings:'x'},async()=>a.engine);
    assert.throws(()=>exporter.render('x'));assert.equal(freed,1);
  }
});

test('filename helper is bounded and cannot emit path separators or reserved device names',async()=>{
  const {exporter}=await setup();const output=exporter.render('x');
  for(const name of ['../bad\\file:xx\0','CON','aux.txt',' ... ','😀'.repeat(100)]) {
    const value=output.filename(name);assert.ok(value.endsWith('.html'));
    assert.ok(!/[<>:"/\\|?*\u0000-\u001f\u007f]/.test(value));assert.ok(Array.from(value).length<=85);
    assert.ok(!/^(con|aux)\./i.test(value));
  }
  assert.equal(output.filename(),'document.html');
});

// A real tiny WASM module, not the Rust renderer. Used only to verify module
// initialization/ownership through the public loader, separately from ABI tests.
function wasmAnswer(answer){return Uint8Array.from([...magic,1,5,1,96,0,1,127,3,2,1,0,7,10,1,6,...Buffer.from('answer'),0,0,10,6,1,4,0,65,answer,11]);}
const bindingSource = `let instance;
export default async function init({module_or_path}) { instance=(await WebAssembly.instantiate(module_or_path)).instance; }
const shell=${shell.toString()}; const escape=${escape.toString()}; const json=${json.toString()};
function output(html) { return {bytes:new TextEncoder().encode(html),mimeType:'text/html',diagnosticsJson:()=> '[]',free(){}}; }
export function renderInteractiveHtmlConfigured(source){return output(shell(source));}
export function renderHtmlConfiguredAdvanced(source){return output('<html><head></head><body><p>WASM answer '+instance.exports.answer()+'</p></body></html>');}
export function renderPdfConfiguredMulti(){return {bytes:new TextEncoder().encode('%PDF-fixture'),mimeType:'application/pdf',diagnosticsJson:()=> '[]',free(){}};}`;

test('public factory loads supplied JavaScript/WASM without a generated package or network',async()=>{
  const wasm=wasmAnswer(7);assert.ok(WebAssembly.validate(wasm));
  const exporter=await createNativeWorkspaceExporter({wasm,bindings:bindingSource});
  assert.ok(exporter.render('Public').text().includes('WASM answer 7'));
  assert.ok(exporter.render('Reuse').text().includes('WASM answer 7'));
});

test('factories using identical glue and different WASM cannot share the wrong module instance',async()=>{
  const first=await createNativeWorkspaceExporter({wasm:wasmAnswer(7),bindings:bindingSource});
  const second=await createNativeWorkspaceExporter({wasm:wasmAnswer(9),bindings:bindingSource});
  assert.ok(first.render('First').text().includes('WASM answer 7'));
  assert.ok(second.render('Second').text().includes('WASM answer 9'));
  assert.ok(first.render('Again').text().includes('WASM answer 7'));
});

test('initialization failures and incompatible binding modules reject without publishing output',async()=>{
  for(const bindings of ['export const value=1;', 'export default async function(){throw Error("init failed")}',
    'export default async function() {}','this is invalid JavaScript']) {
    await assert.rejects(createNativeWorkspaceExporter({wasm:magic,bindings}));
  }
});


test('intrinsic buffer admission rejects spoofed shared-memory brands', async()=>{
  const shared=new SharedArrayBuffer(8);
  Object.defineProperty(shared,Symbol.toStringTag,{value:'ArrayBuffer'});
  new Uint8Array(shared).set(magic);
  await assert.rejects(setup({wasm:new Uint8Array(shared),bindings:'binding'}),TypeError);
  const {exporter}=await setup();
  assert.throws(()=>exporter.render('x',{pdfImages:[{destination:'x',bytes:new DataView(shared)}]}),TypeError);
});


test('pre-worker native controllers cannot borrow the async capability from source or preview', async()=>{
  const source='fmd-async-preview-v1 window.__fmdNativeRuntime;';
  const a=adapter(); let emitted;
  a.engine.renderInteractiveHtmlConfigured=()=>emitted=result(shell(source,'window.__fmdNativeRuntime;'));
  const exporter=await createWorkspaceExporterWithLoader({wasm:magic,bindings:'fmd-async-preview-v1'},async()=>a.engine);
  assert.throws(()=>exporter.render(source),{code:'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal(emitted.freed,1);
  assert.equal(a.outputs.length,1);
  assert.equal(a.outputs[0].freed,1,'native preview released on incompatible controller');
});
