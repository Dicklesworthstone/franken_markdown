// Production exporter/controller/bootstrap/Worker transport, explicit recording
// native ABI and minimal shell adapters. No compiled Rust or font parser here.
import {readFile, writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {createWorkspaceExporterWithLoader} from './interactive_export.mjs';
export const source = '\ufeff# Proof\r\n\r\nText 😀\r';

export const bindings = String.raw`
export default async ({module_or_path}) => { await WebAssembly.instantiate(module_or_path); };
const encoder = new TextEncoder();
const escape = text => String(text).replace(/[&<>]/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;'}[ch]));
function envelope(text, mime, messages = []) {
  return {bytes: encoder.encode(text), mimeType: mime,
    diagnosticsJson: () => JSON.stringify(messages.map(message => ({message}))), free() {}};
}
export function renderHtmlConfiguredAdvanced(...a) {
  return envelope('<html><head></head><body><h1 id="proof">Preview</h1><pre>' + escape(a[0]) + '</pre></body></html>', 'text/html');
}
function pdf(...a) {
  if (typeof document !== 'undefined') throw Error('PDF must run in a Worker');
  if (a[0].includes('PDF_FAIL')) throw Error('fixture PDF failure');
  if (a[0].includes('PDF_SLOW')) { const end = performance.now() + 5000; while (performance.now() < end) {} }
  const content = 'BT /F1 16 Tf 50 740 Td (Native PDF adapter fixture) Tj ET';
  const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    '<< /Length ' + content.length + ' >>\nstream\n' + content + '\nendstream'];
  let text = '%PDF-1.4\n', offsets = [0];
  objects.forEach((object, i) => { offsets.push(text.length); text += (i + 1) + ' 0 obj\n' + object + '\nendobj\n'; });
  const xref = text.length;
  text += 'xref\n0 6\n0000000000 65535 f \n' + offsets.slice(1).map(n => String(n).padStart(10,'0') + ' 00000 n \n').join('');
  text += 'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n' + xref + '\n%%EOF\n';
  text += '%FMD_RECEIPT ' + JSON.stringify({source:a[0],font:a[1],mode:a[2],title:a[3],author:a[4],epoch:a[5],
    lineNumbers:a[7],destinations:a[8],images:[...a[9]],lengths:[...a[10]],
    fonts:a.slice(11,16).map(bytes=>[...bytes]),weights:[...a[16]],pageNumbers:a[20],scale:a[21],lang:a[22],
    toc:a[23],tocDepth:a[24],page:a[27] ? [...a[27]] : null}) + '\n';
  return envelope(text, 'application/pdf', ['<img src=x onerror=alert(1)> literal PDF diagnostic']);
}
export const renderPdfConfiguredMulti = pdf;
export const renderPdfConfiguredPage = pdf;
`;

export async function fixture() {
  const controller = await readFile(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
  const instrument = `
window.__proofTest = {urls:new Map(),revoked:[],requests:[],workers:0,terminated:0,violations:[]};
document.addEventListener('securitypolicyviolation', e => __proofTest.violations.push(e.violatedDirective));
const createUrl = URL.createObjectURL.bind(URL), revokeUrl = URL.revokeObjectURL.bind(URL);
URL.createObjectURL = blob => { const url=createUrl(blob); __proofTest.urls.set(url,blob); return url; };
URL.revokeObjectURL = url => { __proofTest.revoked.push(url); __proofTest.urls.delete(url); revokeUrl(url); };
const ActualWorker = Worker;
window.Worker = class extends ActualWorker {
  constructor(...args) { super(...args); __proofTest.workers++; }
  postMessage(message, ...rest) { __proofTest.requests.push({type:message.type,format:message.format,source:message.markdown}); return super.postMessage(message,...rest); }
  terminate() { __proofTest.terminated++; return super.terminate(); }
};`;
  const ids = ['toggle-view','zoom-out','zoom-reset','zoom-in','theme-toggle','stats-toggle','insert-image',
    'save-markdown','save-html','export-pdf'];
  function shell(markdown) {
    return '<!DOCTYPE html>\n<html lang="en"><head><meta charset="utf-8"><title>PDF proof test</title>'
      + '<style>body{font:16px sans-serif}button{margin:4px;padding:8px}textarea{width:90%;height:8em}#stats-drawer{display:none}dialog:not([open]){display:none}</style>'
      + '<script>' + instrument + '</script></head>\n<body>\n<header class="fmd-app-header"><span class="fmd-title">PDF proof test</span><div class="fmd-toolbar">'
      + ids.map(id=>'<button id="btn-'+id+'">'+id+'</button>').join('')
      + '<span id="view-mode-icon"></span><span id="view-mode-label"></span></div></header>\n'
      + '<div id="fmd-app-body" class="view-split"><section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status"></span></div><textarea id="fmd-editor"></textarea></section>\n'
      + '<main id="preview-pane"><div class="fmd-content" id="fmd-content"><p>Initial adapter preview</p></div>\n  </main>\n</div>\n'
      + '<div id="stats-drawer">' + ['words','chars','read-time','readability'].map(id=>'<span id="stat-'+id+'"></span>').join('') + '<button id="btn-stats-close">Close stats</button></div>\n'
      + '<script type="application/json" id="fmd-raw-source">' + JSON.stringify(markdown).replace(/</g,'\\u003c') + '</script>\n'
      + '<script>\n' + controller + '\n</script>\n</body></html>';
  }
  const engine = await import('data:text/javascript;base64,' + Buffer.from(bindings).toString('base64'));
  const exporter = await createWorkspaceExporterWithLoader({wasm:Uint8Array.of(0,97,115,109,1,0,0,0), bindings}, async()=>({
    ...engine, renderInteractiveHtmlConfigured(markdown) { return {bytes:new TextEncoder().encode(shell(markdown)), mimeType:'text/html',diagnosticsJson:()=> '[]',free(){}}; },
  }));
  return exporter.render(source, {font:'serif',fontScale:1.25,title:'PDF proof test',author:'Author',lang:'fr',
    metadataEpochSeconds:0,pageNumbers:true,codeLineNumbers:true,toc:true,tocDepth:4,
    page:{size:{widthPt:792,heightPt:612},margins:{topPt:24,rightPt:36,bottomPt:48,leftPt:60}},
    pdfImages:[{destination:'chart.png',bytes:Uint8Array.of(1,2,3)}],
    fontAssets:[{slot:'body-regular',bytes:Uint8Array.of(4,5,6),weight:555}],
  }).text();
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (!process.argv[2]) throw Error('Usage: node wasm/native_pdf_proof_fixture.mjs OUTPUT.html');
  await writeFile(process.argv[2], await fixture());
}
