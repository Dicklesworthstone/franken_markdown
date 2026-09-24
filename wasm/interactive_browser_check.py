"""Browser/adapter probe for the standalone native workspace exporter.

Run: python wasm/interactive_browser_check.py
Requires Node, Playwright and Chromium (CHROMIUM may select its executable).
Uses the real public exporter, serialized bootstrap and controller. Generated
bindings/initial Rust output are explicit doubles; an empty WASM module really
instantiates under CSP. Downloaded bytes are reparsed in fresh pages, not opened
via file://. This is not Rust rendering, PDF quality or local-file acceptance.
The lightweight parser is a rejecting stub: a native fallback is a test failure.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
import html
import json
import os
import re
import shutil
import subprocess
import time
from playwright.sync_api import sync_playwright

REPO = Path(__file__).resolve().parents[1]
BINDINGS = r"""// Explicit browser-only binding fixture. This is NOT the Rust renderer.
const encoder = new TextEncoder();
const escape = text => String(text).replace(/[&<>"']/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
export default async function init({module_or_path}) {
  await new Promise(resolve=>setTimeout(resolve,300));
  await WebAssembly.instantiate(module_or_path);
  globalThis.fixtureWasmInitialized = true;
}
function output(text,mimeType) {
  return {bytes:encoder.encode(text),mimeType,diagnosticsJson:()=> '[]',free(){globalThis.fixtureFrees=(globalThis.fixtureFrees||0)+1;}};
}
export function renderHtmlConfiguredAdvanced(...a) {
  globalThis.fixtureHtmlCalls=(globalThis.fixtureHtmlCalls||0)+1;
  const image='data:image/png;base64,'+btoa(String.fromCharCode(...a[14].subarray(0,a[15][0])));
  return output('<!DOCTYPE html><html><head><style>body { margin:16px; font-size:'+16*a[6]+'px; } #source {white-space:pre-wrap;} @media (prefers-color-scheme: dark) { body {background:rgb(20,20,20);color:white;} }</style></head><body><h1 id="fmd-raw-source">Complete native document</h1><p>Binding fixture — not Rust-rendered output</p><pre id="source">'+escape(a[0])+'</pre><img id="plot" src="'+image+'"><div id="font" data-bytes="'+[...a[7]].join(',')+'" data-weight="'+a[12][0]+'">Font bytes retained</div><svg width="60" height="30"><rect width="60" height="30" fill="green"/></svg></body></html>', 'text/html; charset=utf-8');
}
export function renderPdfConfiguredMulti(...a) {
  return output('%PDF-1.7\n'+JSON.stringify({source:a[0],images:[...a[9]],fonts:[...a[11]],weight:a[16][0],author:a[4],epoch:a[5],scale:a[21]}), 'application/pdf');
}
"""

BUILD = r"""// Browser fixture: production exporter/bootstrap/controller, explicit fake core.
import {readFileSync,writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
const here=fileURLToPath(new URL('.',import.meta.url));
writeFileSync(here+'package.json','{"type":"module"}');

writeFileSync(here+'franken_markdown.js',`
import {readFileSync} from 'node:fs';
export async function init(bytes){ await WebAssembly.instantiate(bytes); }
export async function renderInteractiveHtml(source){
 const html=readFileSync(new URL('./shell.html',import.meta.url),'utf8');
 return {format:'interactive-html',mimeType:'text/html; charset=utf-8',extension:'html',bytes:new TextEncoder().encode(html),sourceLength:new TextEncoder().encode(source).length,diagnostics:[],text:()=>html,blob:()=>new Blob([html]),filename:(base='document')=>base+'.html'};
}`);
const {renderOfflineWorkspace}=await import('./interactive.js');
const bindings=readFileSync(new URL('./fake_bindings.mjs',import.meta.url),'utf8');
const source=JSON.parse(readFileSync(here+'source.json','utf8'));
const runtime={bindings,wasm:Uint8Array.of(0,97,115,109,1,0,0,0)};
const options={title:'Native workspace',font:'serif',fontScale:1.125,lang:'de',author:'Author',metadataEpochSeconds:0,
 pdfImages:[{destination:'plot.png',bytes:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAEUlEQVR4nGP4z8Dwn4GB4T8AEfcD/fvWtu0AAAAASUVORK5CYII=','base64')}],
 fontAssets:[{slot:'body-regular',weight:550,bytes:Uint8Array.of(1,2,3)}]};
writeFileSync(here+'workspace.html',(await renderOfflineWorkspace(source,runtime,options)).text());
writeFileSync(here+'failed.html',(await renderOfflineWorkspace(source,{...runtime,bindings:bindings.replace('export function renderPdfConfiguredMulti','export function missingPdf')},options)).text());
console.log('Wrote browser fixtures using production exporter; fake bindings + empty WASM module, not Rust output');
"""

def build_fixture(root):
    node = shutil.which('node')
    if not node:
        raise RuntimeError('Node is required for the actual exporter fixture')
    template = (REPO / 'src/interactive.rs').read_text()
    body = template[template.index('pub fn render_interactive_html('):template.index('const INTERACTIVE_CSS')]
    css = template.split('const INTERACTIVE_CSS: &str = r#"', 1)[1].split('"#;', 1)[0]
    js = "function parseMarkdownClient() { throw new Error('Native preview used fallback'); }\n" + (REPO / 'src/interactive_controller.js').read_text()
    source = '\r\n# Native document\r\n\r\nOriginal source.\r'
    values = {'INTERACTIVE_CSS': css, 'INTERACTIVE_JS': js,
              '&initial_rendered': '<p>Initial Rust preview stand-in</p>'}
    parts = []
    pattern = r'out\.push_str\(\s*(r#"[\s\S]*?"#|"(?:\\.|[^"\\])*"|&?\w+)\s*,?\s*\);|escape_html_to\(([^;]+);|push_source_json\(([^;]+);'
    for match in re.finditer(pattern, body):
        arg = match.group(1)
        if arg:
            parts.append(arg[3:-2] if arg.startswith('r#"') else json.loads(arg) if arg.startswith('"') else values[arg])
        elif match.group(2):
            arg = match.group(2).split(', &mut')[0]
            parts.append(html.escape('en' if arg.startswith('opts.lang') else 'Native workspace' if arg == 'title' else source, quote=True))
        else:
            parts.append(json.dumps(source, ensure_ascii=False).replace('<', '\\u003c'))
    shell = ''.join(parts)
    assert shell.endswith('\n</script>\n</body>\n</html>\n')
    for name in ['interactive.js', 'interactive_runtime.mjs', 'interactive_preview.mjs']:
        shutil.copyfile(REPO / 'wasm' / name, root / name)
    (root / 'shell.html').write_text(shell)
    (root / 'source.json').write_text(json.dumps(source))
    (root / 'fake_bindings.mjs').write_text(BINDINGS)
    (root / 'build.mjs').write_text(BUILD)
    subprocess.run([node, str(root / 'build.mjs')], check=True, capture_output=True, text=True, timeout=15)

def probe(root, chromium):
    checks = []
    def check(condition, label):
        assert condition, label
        checks.append(label)

    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=chromium, headless=True, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width':1440,'height':960}, color_scheme='light')
        requests, errors = [], []
        context.on('request', lambda request: requests.append(request.url) if request.url.startswith(('http:', 'https:')) else None)
        context.on('page', lambda page: page.on('pageerror', lambda error: errors.append(str(error))))
        def open_bytes(html):
            page = context.new_page()
            page.set_default_timeout(7000)
            page.on('dialog', lambda dialog: dialog.accept())
            page.set_content(html)
            return page
        def wait_condition(page, expression):
            # Playwright wait_for_function internally evaluates strings in-page,
            # correctly blocked by our CSP. Poll via DevTools without weakening it.
            deadline=time.monotonic()+7
            while time.monotonic()<deadline:
                if page.evaluate('Boolean('+expression+')'): return
                page.wait_for_timeout(30)
            raise AssertionError('Condition did not become true: '+expression)
        def ready(page):
            check(page.evaluate('window.__fmdNativeRuntime.ready'), 'runtime ready')
            wait_condition(page, 'document.querySelector("#fmd-content iframe")?.contentDocument?.querySelector("#source")')
            return page.frame_locator('#fmd-content iframe')
        def edit(page, source):
            page.locator('#fmd-editor').fill(source)
        def download(page, button):
            with page.expect_download() as event:
                page.locator(button).click()
            dl = event.value
            return Path(dl.path()).read_bytes(), dl.suggested_filename
        original = json.loads((root/'source.json').read_text())
        page = open_bytes((root/'workspace.html').read_text())
        edit(page,'Typed while WASM initializes')
        frame = ready(page)
        check(frame.locator('#source').inner_text() == 'Typed while WASM initializes','latest startup source used')
        check(page.evaluate('globalThis.fixtureWasmInitialized') is True,'actual WASM instantiated under CSP')
        check(frame.locator('#plot').evaluate('(img)=>img.complete && img.naturalWidth === 2'),'embedded PNG decodes')
        check(frame.locator('#font').get_attribute('data-bytes') == '1,2,3','exact font bytes reach ABI')
        check(frame.locator('#font').get_attribute('data-weight') == '550','font weight reaches ABI')
        check(page.locator('body > script#fmd-raw-source').count()==1,'document IDs isolated from controller')
        check(page.locator('#fmd-content iframe').get_attribute('sandbox')=='allow-same-origin','preview disallows scripts/forms/top navigation')
        check(page.evaluate('globalThis.fixtureHtmlCalls || 0') == 0,'live preview renders in worker, not the UI thread')
        payload = page.locator('body > script#fmd-native-runtime').text_content()
        hostile = '\n# fmd-raw-source\n\n</ScRiPt><script>globalThis.injected=true</script>\n<!--<script>-->\nCafé 中 😀\u2028\u2029'
        edit(page,hostile)
        data,name = download(page,'#btn-export-pdf')
        result = json.loads(data.decode().split('\n',1)[1])
        check(name.endswith('.pdf') and data.startswith(b'%PDF-'),'toolbar downloads PDF envelope')
        check(result['source']==hostile,'PDF uses latest edit before debounce')
        check(result['fonts']==[1,2,3] and result['weight']==550,'PDF fonts and weight preserved')
        check(result['epoch']==0 and result['author']=='Author','PDF metadata preserved')
        check(result['scale']==1.125,'PDF typography independent of view zoom')
        page.locator('#btn-theme-toggle').click()
        wait_condition(page, 'document.querySelector("#fmd-content iframe")?.contentDocument?.querySelector("#source")?.textContent.includes("Café")')
        check(frame.locator('body').evaluate('(body)=>getComputedStyle(body).backgroundColor')=='rgb(20, 20, 20)','manual dark overrides light OS')
        page.locator('#btn-zoom-in').click()
        wait_condition(page, '(() => { const body=document.querySelector("#fmd-content iframe")?.contentDocument?.body; return body && getComputedStyle(body).fontSize === "19.8px"; })()')
        check(frame.locator('body').evaluate('(body)=>getComputedStyle(body).fontSize')=='19.8px','view zoom reaches native HTML')
        check(page.evaluate('globalThis.injected === undefined'),'hostile source never executed')
        saved,name = download(page,'#btn-save-html')
        check(name.endswith('.html') and saved.startswith(b'<!DOCTYPE html>'),'HTML workspace downloaded')
        for generation in range(2):
            reopened=open_bytes(saved.decode())
            ready(reopened)
            check(reopened.locator('body > script#fmd-native-runtime').text_content()==payload,'runtime/assets byte-exact on reopen')
            check(reopened.locator('#fmd-editor').input_value()==hostile,'source survives save/reopen')
            check(reopened.frame_locator('#fmd-content iframe').locator('#plot').evaluate('(img)=>img.complete && img.naturalWidth===2'),'reopened embedded image decodes')
            check(reopened.locator('#btn-zoom-reset').inner_text()=='110%','saved view scale restored')
            check(reopened.evaluate('globalThis.injected === undefined'),'reopened source remains inert')
            hostile += '\nGeneration '+str(generation)
            edit(reopened,hostile)
            with reopened.expect_download() as event:
                reopened.locator('#fmd-editor').press('Control+p')
            pdf=Path(event.value.path()).read_bytes()
            check(json.loads(pdf.decode().split('\n',1)[1])['source']==hostile,'reopened keyboard PDF uses current source')
            saved,_=download(reopened,'#btn-save-html')
            check(saved.decode().count('id="fmd-native-runtime"')==1,'runtime payload never duplicates')
            reopened.close()
        untouched=open_bytes((root/'workspace.html').read_text());ready(untouched)
        data,_=download(untouched,'#btn-save-markdown')
        check(data==original.encode(),'untouched CRLF/CR Markdown remains byte-exact')
        bad=open_bytes((root/'failed.html').read_text())
        check(bad.evaluate('window.__fmdNativeRuntime.ready') is False,'missing ABI fails initialization')
        check('renderPdfConfiguredMulti' in bad.locator('#fmd-save-status').text_content(),'incompatible runtime error visible')
        check(bad.locator('#fmd-content iframe').count()==0,'failure preserves original preview, never fallback success')
        edit(bad,'source remains recoverable')
        bad.locator('#btn-export-pdf').click()
        check('failed' in bad.locator('#fmd-save-status').text_content(),'failed runtime cannot export stale PDF')
        data,_=download(bad,'#btn-save-markdown')
        check(data==b'source remains recoverable','Markdown downloadable after native failure')
        check(not requests,'no HTTP(S) requests during runtime/edit/export/reopen')
        check(not errors,'no uncaught browser runtime errors')
        browser.close()
    print(json.dumps({'checks_passed':len(checks),'checks':checks,'network_requests':requests,'runtime_errors':errors,
     'proof':'Chromium browser/adapter with explicit HTML/PDF binding doubles and actual empty WASM instantiation; downloaded bytes reparsed in fresh pages, not file navigation or Rust/WASM parity.'},indent=2))

if __name__ == '__main__':
    chromium = os.environ.get('CHROMIUM') or shutil.which('chromium') or shutil.which('chromium-browser')
    if not chromium:
        raise RuntimeError('Chromium is required; set CHROMIUM to its executable')
    with TemporaryDirectory(prefix='fmd-native-browser-') as directory:
        root = Path(directory)
        build_fixture(root)
        probe(root, chromium)
