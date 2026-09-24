// Test-only shell/native-ABI adapters. All editor, packaging, worker and runtime
// code comes from production modules. The embedded eight-byte WASM is real but
// empty: this fixture does not claim Rust rendering or native/WASM byte parity.
import {readFile, writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {createWorkspaceExporterWithLoader} from './interactive_export.mjs';

export const fixtureSource = '\ufeff# Preview\r\n\r\nStart 😀\r';
export const fixtureBindings = `
let initialized = false;
export default async function({module_or_path}) {
  await WebAssembly.instantiate(module_or_path); initialized = true;
}
const escape = text => String(text).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function result(mimeType, text) {
  return {mimeType, bytes:new TextEncoder().encode(text),
    diagnosticsJson:()=>JSON.stringify([{message:'Fixture renderer diagnostic'}]), free(){}};
}
export function renderHtmlConfiguredAdvanced(...args) {
  if (!initialized) throw new Error('WASM initialization missing');
  const worker = typeof WorkerGlobalScope === 'function' && globalThis instanceof WorkerGlobalScope;
  const source = args[0];
  if (!worker) (globalThis.__fixtureCoreCalls ??= []).push(source);
  if (worker && source.startsWith('slow:')) {
    console.log('fixture-render-start');
    const end = Date.now() + 1500; while (Date.now() < end) {}
    console.log('fixture-render-end');
  }
  if (worker && source === 'blocked') { console.log('fixture-render-blocked'); while (true) {} }
  if (worker && source === 'worker-fail') throw new Error('Deliberate worker-only failure');
  const value = {source,worker,font:args[1],mode:args[2],title:args[3],scale:args[6],
    fontBytes:Array.from(args[7]),weights:Array.from(args[12]),destinations:args[13],
    images:Array.from(args[14]),lang:args[16],toc:args[17]};
  return result('text/html', '<!DOCTYPE html><html><head><title>Fixture</title></head><body><pre id="result">'
    + escape(JSON.stringify(value)) + '</pre></body></html>');
}
export function renderPdfConfiguredMulti(source) {
  if (!initialized) throw new Error('WASM initialization missing');
  return result('application/pdf', '%PDF-fixture\\n' + source);
}
export const renderPdfConfiguredPage = renderPdfConfiguredMulti;
`;
const escape = text => String(text).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
export async function fixtureShell(source = fixtureSource) {
  const controller = await readFile(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
  const controls = [
    ['btn-toggle-view','<span id="view-mode-icon">Read</span><span id="view-mode-label">Read Mode</span>'],
    ['btn-zoom-in','Zoom in'],['btn-zoom-out','Zoom out'],['btn-zoom-reset','100%'],
    ['btn-theme-toggle','Theme'],['btn-stats-toggle','Stats'],['btn-save-markdown','Save Markdown'],
    ['btn-save-html','Save HTML'],['btn-export-pdf','Export PDF'],
  ].map(([id,label])=>`<button type="button" class="fmd-btn" id="${id}">${label}</button>`).join('');
  // Delimiters intentionally match the Rust shell ABI consumed by the exporter.
  return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>Preview fixture</title>
<style>body{font:16px sans-serif}.fmd-app-header,.fmd-toolbar{display:flex;flex-wrap:wrap;gap:5px}.fmd-btn{display:flex;padding:5px}textarea{width:90%;height:100px}#stats-drawer{display:none}</style></head><body>
<header class="fmd-app-header"><span class="fmd-title">Preview fixture</span><div class="fmd-toolbar">${controls}</div></header>
<div class="fmd-app-body view-split" id="fmd-app-body"><section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status" role="status"></span></div><textarea id="fmd-editor">${escape(source)}</textarea></section>
  <main><div class="fmd-content" id="fmd-content">Initial fixture</div>
  </main>
</div>
<div id="stats-drawer"><button id="btn-stats-close">Close</button><span id="stat-words"></span><span id="stat-chars"></span><span id="stat-read-time"></span><span id="stat-readability"></span></div>
<script type="application/json" id="fmd-raw-source">${JSON.stringify(source).replace(/</g,'\\u003c')}</script>
<script type="application/json" id="fmd-image-assets">[]</script>
<script>
${controller}
</script>
</body>
</html>
`;
}
export async function makePreviewFixture(source = fixtureSource) {
  const wasm = Uint8Array.of(0,97,115,109,1,0,0,0);
  const module = await import('data:text/javascript;base64,' + Buffer.from(fixtureBindings).toString('base64'));
  await module.default({module_or_path:wasm});
  const shell = await fixtureShell(source);
  const exporter = await createWorkspaceExporterWithLoader({wasm,bindings:fixtureBindings}, async()=>({
    ...module,
    renderInteractiveHtmlConfigured:()=>({mimeType:'text/html',bytes:new TextEncoder().encode(shell),diagnosticsJson:()=> '[]',free(){}}),
  }));
  return exporter.render(source, {font:'serif',fontScale:1.25,title:'Preview fixture',lang:'en',
    pdfImages:[{destination:'fixture.png',bytes:Uint8Array.of(1,2,3)}],
    fontAssets:[{slot:'body-regular',weight:555,bytes:Uint8Array.of(4,5)}]}).text();
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (!process.argv[2]) throw new Error('Supply a retained output HTML path');
  await writeFile(process.argv[2], await makePreviewFixture());
}
