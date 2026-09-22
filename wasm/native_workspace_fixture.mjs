// Browser/ABI adapter fixture, NOT Rust rendering. The public exporter, boot
// runtime and controller are real; only the three core renderer calls are doubles.
// The module actually instantiates WASM and returns its answer in preview/PDF.
import {readFile, writeFile} from 'node:fs/promises';
import {createNativeWorkspaceExporter} from './native_workspace.js';
const outputPath = process.argv[2];
if (!outputPath) throw new Error('Usage: node wasm/native_workspace_fixture.mjs NEW_OUTPUT.html');
const controller = await readFile(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const escape = text => text.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const json = value => JSON.stringify(value).replace(/</g,'\\u003c').replace(/\u2028/g,'\\u2028').replace(/\u2029/g,'\\u2029');
function shell(source) {
  const buttons = ['toggle-view','zoom-out','zoom-reset','zoom-in','theme-toggle','stats-toggle','save-markdown','save-html','export-pdf'];
  return '<!DOCTYPE html>\n<html><head><title>Native factory adapter</title><style>body{margin:8px}button{padding:8px}textarea{width:95%;height:100px}.view-read #editor-pane{display:none}</style></head><body>\n'
    + '<header class="fmd-app-header">'+buttons.map(id=>'<button id="btn-'+id+'">'+id+'</button>').join('')+'<span id="view-mode-icon"></span><span id="view-mode-label"></span></header>\n'
    + '<div class="fmd-app-body view-split" id="fmd-app-body">\n  <section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status" role="status"></span></div><textarea id="fmd-editor">'+escape(source)+'</textarea></section>\n'
    + '  <main id="preview-pane">\n    <div class="fmd-content" id="fmd-content"><img src="https://external.invalid/unsandboxed-first-frame">'
    + '</div>\n  </main>\n</div>\n'
    + '<div id="stats-drawer">'+['words','chars','read-time','readability'].map(id=>'<span id="stat-'+id+'"></span>').join('')+'<button id="btn-stats-close">close</button></div>\n'
    + '<script type="application/json" id="fmd-raw-source">'+json(source)+'</script>\n<script>\n'
    + 'function parseMarkdownClient(){throw Error("Unexpected reduced parser fallback");}\n'+controller+'\n</script>\n</body>\n</html>\n';
}
const wasm=Uint8Array.from([0,97,115,109,1,0,0,0,1,5,1,96,0,1,127,3,2,1,0,7,10,1,6,...Buffer.from('answer'),0,0,10,6,1,4,0,65,7,11]);
const bindings = `let instance;
export default async function init({module_or_path}) {
  instance=(await WebAssembly.instantiate(module_or_path)).instance;
  if(typeof document !== 'undefined') await new Promise(resolve => { globalThis.releaseFixtureRuntime=resolve; });
}
const controller=${JSON.stringify(controller)}, escape=${escape.toString()}, json=${json.toString()}, shell=${shell.toString()};
function result(text,mimeType='text/html') { const bytes=new TextEncoder().encode(text);return {bytes,mimeType,diagnosticsJson:()=> '[]',free(){bytes.fill(0);}}; }
function images(keys,flat,lengths) { let offset=0; return keys.map((key,i)=>{const bytes=flat.slice(offset,offset+lengths[i]); offset+=lengths[i]; return [key,btoa(String.fromCharCode(...bytes))];}); }
export function renderInteractiveHtmlConfigured(source){return result(shell(source));}
export function renderHtmlConfiguredAdvanced(...a){
  const resources=images(a[13],a[14],a[15]);
  return result('<!DOCTYPE html><html><head><style>@media (prefers-color-scheme: dark) {body{background:#111}}</style></head><body>'
    +'<output id="answer">'+instance.exports.answer()+'</output><pre id="source">'+escape(a[0])+'</pre>'
    +'<output id="scale">'+a[6]+'</output><output id="weights">'+Array.from(a[12]).join(',')+'</output>'
    +resources.filter(([key])=>a[0].includes(key)).map(([key,data])=>'<img alt="'+escape(key)+'" src="data:image/svg+xml;base64,'+data+'">').join('')
    +'<img id="blocked" src="https://external.invalid/preview-resource"><script>globalThis.previewExecuted=true<'+ '/script>'
    +'</body></html>');
}
function pdfResult(a, page){
  return result('%PDF-ADAPTER\\n'+JSON.stringify({answer:instance.exports.answer(),source:a[0],font:a[1],epoch:a[5],images:images(a[8],a[9],a[10]),weights:Array.from(a[16]),scale:a[21],page}),'application/pdf');
}
export function renderPdfConfiguredMulti(...a){return pdfResult(a,null);}
export function renderPdfConfiguredPage(...a){return pdfResult(a,Array.from(a.pop()));}
`;
const source = '\r\n# Native factory\r\n\r\n![Chart](chart.svg)\r\n\0</script><script>globalThis.sourceRan=true</script> é中😀\r\n';
const svg = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="30" height="10"><rect width="30" height="10"/></svg>');
const exporter = await createNativeWorkspaceExporter({wasm,bindings});
const output=exporter.render(source,{font:'serif',fontScale:1.25,metadataEpochSeconds:0,
  page:{size:'a4',orientation:'landscape',margins:{topPt:24,rightPt:36,bottomPt:48,leftPt:60}},
  pdfImages:[{destination:'chart.svg',bytes:svg},{destination:'unused.svg',bytes:svg}],
  fontAssets:[{slot:'body-regular',bytes:Uint8Array.of(1,2,3),weight:555}]});
await writeFile(outputPath,output.bytes,{flag:'wx'});
console.log(JSON.stringify({proof:'browser-ABI-adapter-with-real-tiny-WASM',output:outputPath,bytes:output.bytes.length}));
