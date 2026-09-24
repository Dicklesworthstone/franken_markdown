// Explicit shell/native-ABI fixture; production bootstrap/controller/worker.
// Empty WASM really initializes. PDF bytes are adapter data, not PDF layout proof.
import {readFileSync,writeFileSync} from 'node:fs';
import {bootNativeWorkspace,createNativeWorkspaceRenderer} from './interactive_runtime.mjs';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';

export const source='\ufeff\r\n# Export source\r\n\r\n![chart](chart.png)\r\nCafé 😀\r';
export const image='iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAEUlEQVR4nGP4z8Dwn4GB4T8AEfcD/fvWtu0AAAAASUVORK5CYII=';
const json=value=>JSON.stringify(value).replace(/</g,'\\u003c').replace(/\u2028/g,'\\u2028').replace(/\u2029/g,'\\u2029');
const escape=text=>text.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const bindings=`
export default async function({module_or_path}) { await WebAssembly.instantiate(module_or_path); }
const escape=${escape.toString()};
function output(text,mimeType,source) {const bytes=new TextEncoder().encode(text);return {bytes,mimeType,diagnosticsJson:()=>JSON.stringify([{message:mimeType+':'+source.slice(0,30)}]),free(){bytes.fill(0);}};}
function images(names,bytes,lengths) {let at=0;return names.map((name,i)=>{const data=bytes.slice(at,at+lengths[i]);at+=lengths[i];return [name,btoa(String.fromCharCode(...data))];});}
function block(source,kind) {
  if(source.startsWith('FAIL') && (kind==='pdf' || source.startsWith('FAIL_HTML'))) throw Error('Deliberate renderer refusal');
  if((kind==='pdf'&&source.startsWith('HANG')) || (kind==='html'&&source.startsWith('HTML_HANG'))) while(true) {}
  if((kind==='pdf'&&source.startsWith('SLOW')) || (kind==='html'&&source.startsWith('HTML_SLOW'))) {const end=Date.now()+700;while(Date.now()<end){}}
}
export function renderHtmlConfiguredAdvanced(...a) {
  block(a[0],'html');
  const worker=typeof document==='undefined';
  if(!worker)globalThis.fixtureMainHtml=(globalThis.fixtureMainHtml||0)+1;
  const abi={source:a[0],font:a[1],mode:a[2],title:a[3],scale:a[6],fontBytes:[...a[7]],weights:[...a[12]],images:images(a[13],a[14],a[15]),lang:a[16],toc:a[17],depth:a[18],worker};
  return output('<!DOCTYPE html><html><head><meta charset="utf-8"><title>'+escape(a[3]||'')+'</title></head><body><pre id="source">'+escape(a[0])+'</pre><pre id="abi">'+escape(JSON.stringify(abi))+'</pre>'
    +abi.images.filter(([key])=>a[0].includes(key)).map(([key,data])=>'<img alt="'+escape(key)+'" src="data:image/png;base64,'+data+'">').join('')+'</body></html>','text/html',a[0]);
}
function pdf(a,page) {
  if(typeof document!=='undefined')throw Error('PDF must not render on the editor thread');
  block(a[0],'pdf');
  return output('%PDF-ADAPTER\\n'+JSON.stringify({source:a[0],font:a[1],mode:a[2],title:a[3],author:a[4],epoch:a[5],codeLineNumbers:a[7],images:images(a[8],a[9],a[10]),fonts:[...a[11]],weights:[...a[16]],pageNumbers:a[20],scale:a[21],lang:a[22],toc:a[23],depth:a[24],page,worker:typeof document==='undefined'}),'application/pdf',a[0]);
}
export function renderPdfConfiguredMulti(...a){return pdf(a,null);}
export function renderPdfConfiguredPage(...a){return pdf(a,[...a[27]]);}
`;
export function fixture({legacy=false,noWorker=false}={}) {
  const options={font:'serif',darkMode:'disabled',fontScale:1.25,title:'Publication',author:'A',lang:'fr',metadataEpochSeconds:0,
    toc:true,tocDepth:4,pageNumbers:true,codeLineNumbers:true,pageGeometry:[792,612,24,36,48,60]};
  const payload={version:1,wasm:'AGFzbQEAAAA=',bindings,options,images:[{destination:'chart.png',bytes:image}],fonts:[{slot:'body-regular',bytes:'AQID',weight:550}]};
  // Shorten only the configurable worker deadline for this retained test fixture.
  const factory='function(factory,payload){return ('+createWorkspacePreviewWorker.toString()+')(factory,payload,{timeoutMs:1400});}';
  const bootstrap='('+bootNativeWorkspace.toString()+')('+createNativeWorkspaceRenderer.toString()+(legacy?'':','+factory)+');';
  const controls=['toggle-view','zoom-out','zoom-reset','zoom-in','theme-toggle','stats-toggle','save-markdown','save-html','export-pdf'];
  const policy="default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; frame-src 'self' about:; base-uri 'none'; form-action 'none'";
  const instrumentation=`const ActualWorker=window.Worker;window.workerLog=[];window.Worker=class extends ActualWorker{constructor(...args){super(...args);workerLog.push({event:'start'});}postMessage(...args){workerLog.push({event:args[0].type,format:args[0].format});return super.postMessage(...args);}terminate(){workerLog.push({event:'stop'});return super.terminate();}};`;
  return '<!DOCTYPE html><html lang="fr"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="'+policy+'"><title>Publication</title><style>body{font:16px system-ui;margin:16px}.fmd-toolbar{display:flex;gap:8px;flex-wrap:wrap}.fmd-btn{display:inline-flex;padding:6px}textarea{width:90%;height:120px}.view-read #editor-pane{display:none}</style></head><body>'
    +'<header class="fmd-app-header"><span class="fmd-title">Publication</span><div class="fmd-toolbar">'+controls.map(name=>'<button class="fmd-btn" id="btn-'+name+'">'+name+'</button>').join('')+'<span id="view-mode-icon"></span><span id="view-mode-label"></span></div></header>'
    +'<div id="fmd-app-body" class="view-split"><section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status"></span></div><textarea id="fmd-editor">'+escape(source)+'</textarea></section><main id="preview-pane"><div id="fmd-content">Initial fixture preview</div></main></div>'
    +'<div id="stats-drawer">'+['words','chars','read-time','readability'].map(name=>'<span id="stat-'+name+'"></span>').join('')+'<button id="btn-stats-close">Close</button></div>'
    +'<script type="application/json" id="fmd-raw-source">'+json(source)+'</script><script type="application/json" id="fmd-native-runtime">'+json(payload)+'</script>'
    +'<script>'+instrumentation+(noWorker?'window.Worker=undefined;':'')+bootstrap+'</script><script>function parseMarkdownClient(){throw Error("Unexpected lightweight fallback");}\n'
    +readFileSync(new URL('../src/interactive_controller.js',import.meta.url),'utf8')+'</script></body></html>';
}
if(process.argv[1]===new URL(import.meta.url).pathname) {
  const path=process.argv[2];if(!path)throw Error('Pass a new output HTML path');
  writeFileSync(path,fixture(),{flag:'wx'});
}
