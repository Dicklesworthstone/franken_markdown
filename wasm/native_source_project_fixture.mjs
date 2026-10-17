// Explicit native engine / initial-shell adapters for project transport tests.
// The exporter, codec, page normalization, PDF registration and project UI are
// unchanged production modules. This fixture is NOT Rust/WASM render evidence.
import {readFile, writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';

const dataModule = source => 'data:text/javascript;base64,' + Buffer.from(source).toString('base64');
const runtimeAdapter = dataModule(`
export function createNativeWorkspaceRenderer(engine, payload) {
  return {diagnostics: [], html(source) { return engine.renderHtmlConfiguredAdvanced(source, payload); }};
}
export function bootNativeWorkspace() {
  // Deliberately fail native startup: source-project saving must need no engine.
  window.__fmdNativeRuntime = {settings: null, settingsPending: false,
    beginExportDocument() { throw Error('Explicit fixture: no Rust renderer'); },
    ready: Promise.reject(Error('Explicit fixture: native engine unavailable'))};
  window.__fmdNativeRuntime.ready.catch(() => {});
}
`);
const previewAdapter = dataModule('export function createWorkspacePreviewWorker() { throw Error("Explicit fixture: no preview renderer"); }');
const proofAdapter = dataModule(`export function createNativePdfProof() {
  return {current: null, cancel() {}, invalidate() {}, render() { throw Error('Explicit fixture: no PDF renderer'); }};
}`);
let modulePromise;
export async function projectExporterFixtureModule() {
  if (!modulePromise) {
    const text = await readFile(new URL('./interactive_export.mjs', import.meta.url), 'utf8');
    const doubles = {'./interactive_runtime.mjs': runtimeAdapter, './interactive_preview.mjs': previewAdapter, './native_pdf_proof.mjs': proofAdapter};
    const adjusted = text.replace(/from\s+(['"])(\.\/[^'"]+)\1/g, (_, quote, name) =>
      'from ' + JSON.stringify(doubles[name] ?? new URL(name, import.meta.url).href));
    modulePromise = import(dataModule(adjusted));
  }
  return modulePromise;
}
function json(value) { return JSON.stringify(value).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029'); }
export function projectShell(source) {
  return '<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Source project fixture</title>'
    + '<meta name="viewport" content="width=device-width, initial-scale=1"><style>body{font:16px sans-serif;margin:12px}header{display:flex;flex-wrap:wrap;gap:10px}textarea{width:95%;height:200px;margin-top:20px}button{padding:8px}iframe{max-width:100%}</style></head><body>'
    + '<header class="fmd-app-header"><button id="btn-save-html" type="button">Save HTML (fixture)</button></header>'
    + '<div id="fmd-app-body"><div id="editor-pane"><textarea id="fmd-editor"></textarea></div><main>'
    + '<div class="fmd-content" id="fmd-content">Initial fixture preview</div>\n  </main>\n</div>\n'
    + '<script type="application/json" id="fmd-raw-source">' + json(source) + '</script>\n'
    + '<script>\n// fmd-async-preview-v1; fmd-pdf-proof-host-v1; explicit lossless-host adapter\n'
    + 'const editor = document.getElementById("fmd-editor");\n'
    + 'let anchor = JSON.parse(document.getElementById("fmd-raw-source").textContent);\n'
    + 'editor.value = anchor; let view = editor.value, composing = false;\n'
    + 'function currentSource() { return editor.value === view ? anchor : editor.value; }\n'
    + 'editor.addEventListener("input", () => { anchor = currentSource(); view = editor.value; });\n'
    + 'editor.addEventListener("compositionstart", () => { composing = true; });\n'
    + 'editor.addEventListener("compositionend", () => { composing = false; });\n'
    + 'window.__fmdNativeRuntime.installPdfProof({source: currentSource, ready: () => !composing, download() { throw Error("No fixture PDF"); }});\n'
    + '</script>\n</body></html>\n';
}
export function projectFixtureEngine(label = 'fixture') {
  const captures = [];
  const engine = {
    renderInteractiveHtmlConfigured(source) {
      return {mimeType: 'text/html', bytes: new TextEncoder().encode(projectShell(source)), diagnosticsJson: () => '[]', free() {}};
    },
    renderPdfConfiguredMulti() { throw Error('Explicit fixture: no PDF render'); },
    renderHtmlConfiguredAdvanced(source, payload) {
      captures.push(structuredClone({source, payload}));
      return '<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; img-src data:; font-src data:;"><p>'
        + label + ': ' + source.replace(/[<&]/g, c => c === '<' ? '&lt;' : '&amp;') + '</p>';
    },
  };
  return {engine, captures};
}
export const PROJECT_FIXTURE_SOURCE = '\ufeff# Project transport\r\n\r\nPreserve 😀, naïve, and e\u0301.\r\n![Chart](chart.svg)\r\n';
export const PROJECT_FIXTURE_OPTIONS = () => ({font: 'serif', fontScale: 1.125, metadataEpochSeconds: 0,
  title: 'Portable source', author: 'Author', lang: 'de', toc: true, tocDepth: 3, pageNumbers: true,
  page: {size: {widthPt: 600.125, heightPt: 800.25}, margins: {topPt: 35, rightPt: 40, bottomPt: 45, leftPt: 50}},
  pdfImages: [{destination: 'chart.svg', bytes: new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12"><rect width="12" height="12" fill="black"/></svg>')},
    {destination: 'unused.png', bytes: new Uint8Array([1, 2, 3])}],
  // Explicit data fixture, not a usable TrueType font or glyph-shaping proof.
  fontAssets: [{slot: 'body-regular', bytes: new Uint8Array([0, 1, 2, 3]), weight: 525}],
});
export async function nativeSourceProjectFixture() {
  const module = await projectExporterFixtureModule(), {engine} = projectFixtureEngine();
  const exporter = await module.createWorkspaceExporterWithLoader({wasm: new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]), bindings: '// trusted fixture'}, async () => engine);
  return exporter.render(PROJECT_FIXTURE_SOURCE, PROJECT_FIXTURE_OPTIONS()).text();
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.length !== 3) throw Error('Usage: node wasm/native_source_project_fixture.mjs NEW_OUTPUT.html');
  await writeFile(process.argv[2], await nativeSourceProjectFixture(), {flag: 'wx'});
  console.log('Retained explicit-adapter source-project fixture:', process.argv[2]);
}
