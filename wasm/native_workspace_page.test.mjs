// Saved paper settings use the real shared geometry normalizer and offline
// renderer; core calls below are ABI adapters, not Rust PDF layout proof.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {pdfPageGeometry} from './pdf_page.mjs';
import {createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspaceExporterWithLoader} from './interactive_export.mjs';
const magic=Uint8Array.of(0,97,115,109,1,0,0,0);
function output(text,mimeType='text/html') {
  const bytes=new TextEncoder().encode(text);
  return {bytes,mimeType,diagnosticsJson:()=>'[]',free(){bytes.fill(0);}};
}
function adapter() {
  const calls=[];
  const engine={
    renderInteractiveHtmlConfigured(source){calls.push(['shell']);return output('<html><head></head><body><div class="fmd-content" id="fmd-content"></div>\n  </main>\n</div>\n<script type="application/json" id="fmd-raw-source">'+JSON.stringify(source)+'</script>\n<script>\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\n</script></body></html>');},
    renderHtmlConfiguredAdvanced(...args){calls.push(['html',args]);return output('<html><head></head><body>Preview</body></html>');},
    renderPdfConfiguredMulti(...args){calls.push(['default',args]);return output('%PDF-default','application/pdf');},
    renderPdfConfiguredPage(...args){calls.push(['page',args]);return output('%PDF-page','application/pdf');},
  };
  return {engine,calls};
}
async function setup() {
  const a=adapter();return {...a,exporter:await createWorkspaceExporterWithLoader({wasm:magic,bindings:'adapter'},async()=>a.engine)};
}
function payload(output) {
  const marker='<script type="application/json" id="fmd-native-runtime">';
  return JSON.parse(output.text().split(marker)[1].split('</script>')[0]);
}
const cases=[{}, {size:'letter'}, {size:'a4',orientation:'landscape',margins:{topPt:24,rightPt:36,bottomPt:48,leftPt:60}},
  {size:{widthPt:1000,heightPt:500},orientation:'portrait',margins:18}];

test('export normalizes Letter, A4, custom size, orientation and independent margins',async()=>{
  const {exporter}=await setup();
  for(const page of cases) {
    const data=payload(exporter.render('Paper',{page}));
    assert.deepEqual(data.options.pageGeometry,Array.from(pdfPageGeometry(page)));
  }
  assert.equal(Object.hasOwn(payload(exporter.render('Defaults')).options,'pageGeometry'),false);
});

test('saved PDF geometry reaches the page ABI in exact order without changing other options',async()=>{
  const {exporter,engine,calls}=await setup();
  const page=cases[2], geometry=Array.from(pdfPageGeometry(page));
  const data=payload(exporter.render('Paper',{page,author:'A',metadataEpochSeconds:0,fontScale:1.25,lang:'fr',toc:true,pageNumbers:true}));
  const renderer=createNativeWorkspaceRenderer(engine,data);calls.length=0;
  const pdf=renderer.pdf('Latest source');
  assert.equal(new TextDecoder().decode(pdf),'%PDF-page');
  assert.deepEqual(calls.map(c=>c[0]),['page']);
  const args=calls[0][1];
  assert.equal(args.length,28);assert.equal(args[0],'Latest source');assert.equal(args[4],'A');assert.equal(args[5],0);
  assert.equal(args[20],true);assert.equal(args[21],1.25);assert.equal(args[22],'fr');assert.equal(args[23],true);
  assert.ok(args[27] instanceof Float64Array);assert.deepEqual(Array.from(args[27]),geometry);
});

test('omitted geometry uses the unchanged legacy PDF ABI',async()=>{
  const {exporter,engine,calls}=await setup();delete engine.renderPdfConfiguredPage;
  const data=payload(exporter.render('Defaults'));calls.length=0;
  const bytes=createNativeWorkspaceRenderer(engine,data).pdf('Unchanged');
  assert.equal(new TextDecoder().decode(bytes),'%PDF-default');assert.equal(calls[0][0],'default');assert.equal(calls[0][1].length,27);
});

test('explicit paper fails closed before rendering on an old binary',async()=>{
  const {exporter,engine,calls}=await setup();delete engine.renderPdfConfiguredPage;
  assert.throws(()=>exporter.render('Paper',{page:{}}),{code:'UNSUPPORTED_WASM_PACKAGE'});
  assert.equal(calls.length,0);
});

test('invalid paper and accessor values fail before any asset access or native layout',async()=>{
  const {exporter,calls}=await setup();let accessed=0;
  const accessor={get size(){accessed++;return 'a4';}};
  for(const page of [null,[],{size:'a3'},{orientation:'sideways'},{margins:-1},{size:{widthPt:100,heightPt:792}},
    {size:{widthPt:Infinity,heightPt:792}},{margins:10000},{unknown:true},accessor]) {
    assert.throws(()=>exporter.render('x',{page,pdfImages:[{get bytes(){accessed++;throw Error('must not read');}}]}));
  }
  assert.equal(accessed,0);assert.equal(calls.length,0);
});

test('corrupted saved geometry is refused rather than silently using default paper',()=>{
  const {engine,calls}=adapter();
  for(const pageGeometry of [null,[],Array(6),[612,792,72,72,72],[612,792,72,72,72,72,1],
    [612,792,NaN,72,72,72],[612,792,72,72,72,'72'],[612,792,-1,72,72,72],
    [143,792,0,0,0,0],[14401,792,0,0,0,0],[612,792,72,400,72,400],
    [612,792,Infinity,72,72,72],new Float64Array([612,792,72,72,72,72])]) {
    assert.throws(()=>createNativeWorkspaceRenderer(engine,{version:1,options:{pageGeometry},images:[],fonts:[]}),/geometry/);
  }
  assert.equal(calls.length,0);
});

test('renderer snapshots geometry and later exports do not leak page settings',async()=>{
  const {exporter,engine,calls}=await setup();
  const data=payload(exporter.render('x',{page:cases[2]})), expected=data.options.pageGeometry.slice();
  const renderer=createNativeWorkspaceRenderer(engine,data);data.options.pageGeometry.fill(0);calls.length=0;
  renderer.pdf('Still owned');assert.deepEqual(Array.from(calls[0][1][27]),expected);
  const other=payload(exporter.render('other'));assert.equal(other.options.pageGeometry,undefined);
  createNativeWorkspaceRenderer(engine,other).pdf('Default');assert.equal(calls.at(-1)[0],'default');
});

test('saved validation preserves native f32 left-then-right subtraction order',async()=>{
  const {exporter,engine}=await setup();
  // Double arithmetic leaves 72 points. Native left-then-right f32 subtraction
  // also gives 72; reversing the margins incorrectly gives 71.99998474121094.
  const page={size:{widthPt:500,heightPt:500},margins:{leftPt:4/997,rightPt:428-4/997,topPt:36,bottomPt:36}};
  const data=payload(exporter.render('Rounding boundary',{page}));
  assert.deepEqual(data.options.pageGeometry,Array.from(pdfPageGeometry(page)));
  assert.equal(createNativeWorkspaceRenderer(engine,data).pdf('Boundary')[0],37);
});

test('independently imported exporter copies cannot reuse another factory runtime',async()=>{
  const firstModule=await import('./interactive_export.mjs?isolation-first');
  const secondModule=await import('./interactive_export.mjs?isolation-second');
  const wasm = answer => Uint8Array.from([...magic,1,5,1,96,0,1,127,3,2,1,0,7,10,1,6,...Buffer.from('answer'),0,0,10,6,1,4,0,65,answer,11]);
  // Real tiny WASM initialization plus explicit shell/HTML/PDF ABI adapters.
  const bindings = `let answer;
    export default async function({module_or_path}){answer=(await WebAssembly.instantiate(module_or_path)).instance.exports.answer();}
    const output=${output.toString()};
    export function renderInteractiveHtmlConfigured(source){return output('<html><head></head><body><div class="fmd-content" id="fmd-content"></div>\\n  </main>\\n</div>\\n<script type="application/json" id="fmd-raw-source">'+JSON.stringify(source)+'</script>\\n<script>\\nwindow.__fmdNativeRuntime; /* fmd-async-preview-v1 */\\n</script></body></html>');}
    export function renderHtmlConfiguredAdvanced(){return output('<html><head></head><body>Runtime '+answer+'</body></html>');}
    export function renderPdfConfiguredMulti(){return output('%PDF-adapter','application/pdf');}`;
  const a=await firstModule.createNativeWorkspaceExporter({wasm:wasm(7),bindings});
  const b=await secondModule.createNativeWorkspaceExporter({wasm:wasm(9),bindings});
  assert.ok(a.render('x').text().includes('Runtime 7'));
  assert.ok(b.render('x').text().includes('Runtime 9'));
  assert.ok(a.render('x').text().includes('Runtime 7'));
});
