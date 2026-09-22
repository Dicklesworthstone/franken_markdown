import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeWorkspaceRenderer, bootNativeWorkspace} from './interactive_runtime.mjs';
import vm from 'node:vm';
const utf8 = new TextEncoder();
const encode = bytes => Buffer.from(bytes).toString('base64');
function setup() {
  const payload = {version: 1, options: {font:'serif', darkMode:'auto', title:' T ', fontScale:1.25,
    lang:'fr', toc:true, tocDepth:3, author:' A ', metadataEpochSeconds:0, pageNumbers:true, codeLineNumbers:true},
    images:[{destination:'pic.png',bytes:encode([1,2,3])},{destination:'other.svg',bytes:encode([4,5])}],
    fonts:[{slot:'mono-regular',weight:430,bytes:encode([6,7])},{slot:'body-regular',weight:580,bytes:encode([8,9])}]};
  const calls = [], freed = [], outputs = [];
  const bindings = {};
  for (const [method, mimeType] of [['renderHtmlConfiguredAdvanced','text/html; charset=utf-8'], ['renderPdfConfiguredMulti','application/pdf']]) {
    bindings[method] = (...args) => {
      calls.push({method,args});
      const output = {mimeType, bytes:utf8.encode(method.includes('Html')
        ? '<!DOCTYPE html><html><head><style>@media (prefers-color-scheme: dark) { body { color: white; } }</style></head><body><main>native</main></body></html>'
        : '%PDF-1.7\nNative core bytes'),
      diagnosticsJson:() => JSON.stringify([{severity:'warning',message:'supplied diagnostic',code:'example'}]),
      free() { freed.push(method); }};
      outputs.push(output); return output;
    };
  }
  return {payload, bindings, calls, freed, outputs, renderer:createNativeWorkspaceRenderer(bindings,payload)};
}

test('HTML uses the advanced native ABI with exact images, ordered font slots and weight pins', () => {
  const s = setup(); const html = s.renderer.html('Current\r\nsource', {scale:1.2,theme:'light'});
  const a = s.calls[0].args;
  assert.equal(s.calls[0].method,'renderHtmlConfiguredAdvanced');
  assert.equal(a.length,19);
  assert.deepEqual(a.slice(0,7),['Current\r\nsource','serif','disabled',' T ',undefined,false,1.5]);
  assert.deepEqual(a.slice(7,12).map(bytes => Array.from(bytes)),[[8,9],[],[],[],[6,7]]);
  assert.ok(a[12] instanceof Uint32Array);
  assert.deepEqual([...a[12]],[580,0,0,0,430]);
  assert.deepEqual(a[13],['pic.png','other.svg']); assert.deepEqual([...a[14]],[1,2,3,4,5]);
  assert.deepEqual([...a[15]],[3,2]); assert.deepEqual(a.slice(16),['fr',true,3]);
  assert.match(html, /<head><meta http-equiv="Content-Security-Policy"/);
  assert.match(html, /default-src 'none'/); assert.match(html, /img-src data:/);
  assert.equal(s.freed.length,1); assert.equal(s.renderer.diagnostics[0].code,'example');
});

test('PDF uses the complete existing multi-resource ABI, with export typography independent of view zoom', () => {
  const s = setup(); const bytes = s.renderer.pdf('Latest PDF source');
  const a = s.calls[0].args;
  assert.equal(a.length,27);
  assert.deepEqual(a.slice(0,8),['Latest PDF source','serif','auto',' T ',' A ',0,false,true]);
  assert.deepEqual(a[8],['pic.png','other.svg']); assert.deepEqual([...a[9]],[1,2,3,4,5]);
  assert.deepEqual([...a[10]],[3,2]); assert.deepEqual(a.slice(11,16).map(bytes=>[...bytes]),[[8,9],[],[],[],[6,7]]);
  assert.deepEqual([...a[16]],[580,0,0,0,430]);
  assert.deepEqual(a.slice(17),[undefined,undefined,undefined,true,1.25,'fr',true,3,undefined,false]);
  assert.equal(new TextDecoder().decode(bytes),'%PDF-1.7\nNative core bytes');
  s.outputs[0].bytes.fill(0); assert.equal(bytes[0],37,'Output must own its bytes before free');
  assert.equal(s.freed.length,1);
});

test('missing font slots pass empty slices, and absent metadata stays absent', () => {
  const s = setup(); s.payload.fonts = []; s.payload.images = []; s.payload.options = {fontScale:1};
  const renderer = createNativeWorkspaceRenderer(s.bindings,s.payload); renderer.html('empty assets');
  const a = s.calls[0].args;
  for (let i=7;i<12;i++) assert.deepEqual([...a[i]],[]);
  assert.deepEqual([...a[12]],[0,0,0,0,0]); assert.deepEqual(a[13],[]);
  assert.deepEqual([...a[14]],[]); assert.deepEqual([...a[15]],[]);
});

test('manual dark transforms generated styles, not literal source text', () => {
  const s=setup();
  s.bindings.renderHtmlConfiguredAdvanced=()=>({mimeType:'text/html',bytes:utf8.encode('<html><head><style>@media (prefers-color-scheme: dark) {}</style></head><body><code>@media (prefers-color-scheme: dark)</code></body></html>'),diagnosticsJson:()=>'',free(){}});
  const html=s.renderer.html('css example',{theme:'dark'});
  assert.match(html, /<style>@media all/); assert.match(html, /<code>@media \(prefers-color-scheme: dark\)/);
});

test('generated results are freed on every validation and decoding failure', () => {
  for (const kind of ['mime','type','empty','diagnostics','utf8','head','pdf']) {
    const s=setup(); let frees=0;
    const result={mimeType:kind==='pdf'?'application/pdf':'text/html',bytes:utf8.encode('<html><head></head></html>'),diagnosticsJson:()=> '[]',free(){frees++;}};
    if(kind==='mime') result.mimeType='bad';
    if(kind==='type') result.bytes=[];
    if(kind==='empty') result.bytes=new Uint8Array();
    if(kind==='diagnostics') result.diagnosticsJson=()=> '{}';
    if(kind==='utf8') result.bytes=Uint8Array.of(255);
    if(kind==='head') result.bytes=utf8.encode('<p>Not a document</p>');
    s.bindings.renderHtmlConfiguredAdvanced=s.bindings.renderPdfConfiguredMulti=()=>result;
    assert.throws(()=>kind==='pdf'?s.renderer.pdf('x'):s.renderer.html('x'),undefined,kind);
    assert.equal(frees,1,kind);
  }
});

test('native exceptions propagate without a reduced JavaScript fallback', () => {
  const s=setup(); s.bindings.renderHtmlConfiguredAdvanced=()=>{throw 'native failure';};
  assert.throws(()=>s.renderer.html('x'), value=>value==='native failure');
  assert.equal(s.calls.length,0);
});

test('matching native exports and serialization protocol are mandatory', () => {
  const s=setup(); delete s.bindings.renderPdfConfiguredMulti;
  assert.throws(()=>createNativeWorkspaceRenderer(s.bindings,s.payload),/renderPdfConfiguredMulti/);
  s.payload.version=2; s.bindings.renderPdfConfiguredMulti=()=>{};
  assert.throws(()=>createNativeWorkspaceRenderer(s.bindings,s.payload),/version/);
});

test('serialized factory executes without module imports or outer lexical dependencies', () => {
  const s=setup();
  const fn=vm.runInNewContext('('+createNativeWorkspaceRenderer.toString()+')', {atob,Uint8Array,Uint32Array,TextEncoder,TextDecoder});
  const renderer=fn(s.bindings,s.payload);
  assert.match(renderer.html('source'),/native/); assert.equal(renderer.pdf('source')[0],37);
  assert.doesNotMatch(bootNativeWorkspace.toString()+createNativeWorkspaceRenderer.toString(),/<\/script/i);
});
