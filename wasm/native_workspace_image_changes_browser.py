"""Image-resource authoring acceptance with production JS and real Chromium APIs.
The HTML shell and native ABI are explicit adapters, not compiled Rust/WASM.
Run: CHROMIUM=/usr/bin/chromium python wasm/native_workspace_image_changes_browser.py
"""
from pathlib import Path
import base64
import io
import json
import os
import tempfile
import time
from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(tempfile.mkdtemp(prefix='fmd-image-changes-'))
RUNTIME = (ROOT / 'wasm/interactive_runtime.mjs').read_text().replace('export function ', 'function ')
WORKER = (ROOT / 'wasm/interactive_preview.mjs').read_text().replace('export function ', 'function ')
CONTROLS = (ROOT / 'src/interactive_import.js').read_text()
SOURCE = '\ufeff# Images\r\n\r\n![First](a.png) and ![again](a.png)\r'

def png(width, height, color):
    output = io.BytesIO()
    Image.new('RGB', (width, height), color).save(output, format='PNG')
    return output.getvalue()

FIRST = png(2, 2, 'red')
SECOND = png(3, 4, 'blue')
THIRD = png(5, 6, 'green')
_jpeg = io.BytesIO()
Image.new('RGB', (7, 8), 'orange').save(_jpeg, format='JPEG')
JPEG = _jpeg.getvalue()
BINDINGS = r'''
export default async ({module_or_path}) => { await WebAssembly.instantiate(module_or_path); };
const enc = new TextEncoder();
const escape = text => String(text).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function result(text, mimeType = 'text/html', messages = []) {
  return {bytes: enc.encode(text), mimeType, diagnosticsJson: () => JSON.stringify(messages.map(message => ({message}))), free() {}};
}
export function renderHtmlConfiguredAdvanced(...a) {
  if (typeof document !== 'undefined') throw Error('Image preflight must run in a Worker');
  if (a[0].includes('IMAGE_FAIL')) throw Error('native adapter rejected proposed images');
  if (a[0].includes('IMAGE_SLOW')) { const end = performance.now()+5000; while(performance.now()<end) {} }
  let offset = 0;
  const images = a[13].map((name, i) => {
    const bytes = a[14].subarray(offset, offset+a[15][i]); offset += a[15][i];
    const mime = bytes[0] === 255 ? 'image/jpeg' : 'image/png';
    return '<img alt="'+escape(name)+'" src="data:'+mime+';base64,'+btoa(String.fromCharCode(...bytes))+'">';
  }).join('');
  return result('<html><head></head><body>'+images+'<pre>'+escape(a[0])+'</pre></body></html>');
}
export function renderPdfConfiguredMulti() { return result('%PDF-test-only', 'application/pdf'); }
'''
INSTRUMENT = r'''
window.__imageTest = {workers:0, stopped:0, renders:0, violations:[], changes:0};
document.addEventListener('securitypolicyviolation', e => __imageTest.violations.push(e.violatedDirective));
window.addEventListener('fmd-native-images-changed', () => __imageTest.changes++);
const OriginalWorker = Worker;
window.Worker = class extends OriginalWorker {
  constructor(...args) { super(...args); __imageTest.workers++; }
  postMessage(message, ...args) { if (message.type === 'render') __imageTest.renders++; return super.postMessage(message,...args); }
  terminate() { __imageTest.stopped++; return super.terminate(); }
};
'''

def data(value):
    return json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')


def fixture(legacy=False):
    payload = {'version':1, 'options':{'font':'serif','fontScale':1.25}, 'fonts':[],
               'images':[{'destination':name,'bytes':base64.b64encode(FIRST).decode()}
                         for name in ['a.png','unused.png','<img src=x onerror=alert(1)>']],
               'wasm':'AGFzbQEAAAA=', 'bindings':BINDINGS}
    policy = "default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; frame-src 'self' about: blob:; object-src blob:; base-uri 'none'; form-action 'none'"
    return '<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="'+policy+'">' \
      + '<title>Image resource controls</title><style>body{font:16px sans-serif}button{padding:8px;margin:4px}textarea{width:90%;height:8em}</style></head><body>' \
      + '<header class="fmd-app-header"><button id="btn-save-html">Save HTML</button><button id="btn-insert-image">Insert image</button><input id="fmd-image-picker" type="file" multiple hidden></header>' \
      + '<div id="fmd-app-body" class="view-split"><section id="editor-pane"><div class="fmd-pane-header"><span id="fmd-save-status"></span></div><textarea id="fmd-editor"></textarea></section>' \
      + '<main id="preview-pane"><div id="fmd-content"></div></main></div>' \
      + '<script type="application/json" id="fmd-raw-source">'+data(SOURCE)+'</script>' \
      + '<script type="application/json" id="fmd-native-runtime">'+data(payload)+'</script>' \
      + '<script>'+INSTRUMENT+RUNTIME+WORKER+'\nbootNativeWorkspace(createNativeWorkspaceRenderer,createWorkspacePreviewWorker);' \
      + ("delete window.__fmdNativeRuntime.beginImageChanges;" if legacy else '') \
      + "document.querySelector('#fmd-editor').value=JSON.parse(document.querySelector('#fmd-raw-source').textContent);\n" \
      + CONTROLS+'</script></body></html>'

checks = []
errors = []
network = []

def check(condition, name):
    if not condition:
        raise AssertionError(name)
    checks.append(name)


def wait(page, expression, seconds=10):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if page.evaluate(expression):
            return
        time.sleep(.02)
    raise AssertionError('Timeout: '+expression+'\n'+page.locator('body').inner_text()[-1600:])


def selected_file(page, value=SECOND):
    page.locator('#fmd-image-replacement').set_input_files({'name':'replacement.png','mimeType':'image/png','buffer':value})


def state(page):
    return page.evaluate("({saved:document.querySelector('#fmd-native-runtime').textContent, source:document.querySelector('#fmd-editor').value, raw:document.querySelector('#fmd-raw-source').textContent, revision:__fmdNativeRuntime.documentRevision})")


def source(page, text):
    page.evaluate("text=>{const e=document.querySelector('#fmd-editor');e.value=text;e.dispatchEvent(new Event('input',{bubbles:true}));}", text)


def settled(page):
    wait(page, "document.querySelector('#fmd-document-images').getAttribute('aria-busy')==='false'")


def replace(page, value=SECOND):
    selected_file(page, value)
    page.locator('#btn-replace-document-image').click()
    settled(page)


with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=os.environ.get('CHROMIUM','/usr/bin/chromium'), headless=True, args=['--no-sandbox'])
    context = browser.new_context(viewport={'width':1280,'height':960})
    context.route('http://**/*', lambda route: (network.append(route.request.url), route.abort()))
    context.route('https://**/*', lambda route: (network.append(route.request.url), route.abort()))
    pages = []

    def open_page(html=None):
        page = context.new_page()
        page.set_default_timeout(5000)
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.set_content(html or fixture(), wait_until='domcontentloaded')
        wait(page, "__fmdNativeRuntime.settings && document.querySelector('#btn-document-images') && !document.querySelector('#btn-document-images').disabled")
        pages.append(page)
        return page

    page = open_page()
    page.evaluate("async()=>{await __fmdNativeRuntime.render(document.querySelector('#fmd-editor').value,document.querySelector('#fmd-content'),{scale:1});}")
    page.locator('#btn-document-images').click()
    baseline = state(page)
    check(page.locator('#fmd-image-selection option').count()==3, 'inventory lists all embedded bindings without scanning source')
    check(page.locator('#fmd-document-images img').count()==0, 'image destinations remain literal UI text, not HTML')
    check(page.locator('#btn-replace-document-image').is_disabled(), 'replacement requires an explicitly selected local file')
    replace(page)
    updated = state(page)
    saved = json.loads(updated['saved'])
    check(saved['images'][0]['destination']=='a.png' and base64.b64decode(saved['images'][0]['bytes'])==SECOND,
          'replacement preserves destination and stores exact bytes read from the selected local PNG')
    check(updated['source']==baseline['source'] and updated['raw']==baseline['raw'], 'resource editing leaves textarea and lossless source data untouched')
    check(saved['options']==json.loads(baseline['saved'])['options'] and saved['fonts']==[], 'image edits preserve document settings and fonts')
    wait(page, "document.querySelector('#fmd-content iframe').contentDocument?.images[0]?.naturalWidth===3")
    check(page.evaluate("document.querySelector('#fmd-content iframe').contentDocument.images[0].naturalHeight")==4,
          'real browser preview decodes the replacement dimensions after worker publication')
    count = page.evaluate('__imageTest.workers')
    revision = updated['revision']
    replace(page)
    check(state(page)['revision']==revision and page.evaluate('__imageTest.workers')==count, 'same-byte replacement is a no-op without another worker')
    check('already matches' in page.locator('#fmd-images-status').inner_text(), 'no-op feedback distinguishes unchanged resources')
    replace(page, JPEG)
    wait(page, "document.querySelector('#fmd-content iframe').contentDocument?.images[0]?.naturalWidth===7")
    check(base64.b64decode(json.loads(state(page)['saved'])['images'][0]['bytes'])==JPEG, 'local JPEG replacements use the same stable destination and real browser decoder')
    replace(page, SECOND)
    count = page.evaluate('__imageTest.workers')
    stable = state(page)
    replace(page, b'not a PNG or JPEG')
    check(state(page)==stable and page.evaluate('__imageTest.workers')==count, 'invalid image bytes are rejected before native preflight')
    replace(page, SECOND[:-12])
    check(state(page)==stable and page.evaluate('__imageTest.workers')==count, 'truncated PNG containers cannot replace saved resources')
    oversized = bytearray(SECOND)
    oversized[16:20] = (16385).to_bytes(4, 'big')
    replace(page, bytes(oversized))
    check(state(page)==stable and page.evaluate('__imageTest.workers')==count, 'oversized image dimensions are rejected before browser decode and worker admission')
    source(page, 'IMAGE_FAIL')
    failed = state(page)
    replace(page, THIRD)
    check(state(page)==failed and 'native adapter rejected' in page.locator('#fmd-images-status').inner_text(), 'worker failure is visible and leaves resources unchanged')

    source(page, 'IMAGE_SLOW')
    slow = state(page)
    selected_file(page, THIRD)
    page.locator('#btn-replace-document-image').click()
    wait(page, '__fmdNativeRuntime.imagesPending')
    page.locator('#btn-cancel-image-change').click()
    wait(page, '!__fmdNativeRuntime.imagesPending')
    check(state(page)==slow and page.locator('#fmd-document-images').get_attribute('aria-busy')=='false', 'cancellation terminates real native preflight without publication')
    selected_file(page, THIRD)
    page.locator('#btn-replace-document-image').click()
    wait(page, '__fmdNativeRuntime.imagesPending')
    source(page, 'New source during image preflight')
    settled(page)
    check(state(page)['saved']==slow['saved'] and not page.evaluate('__fmdNativeRuntime.imagesPending'), 'source edits cancel and fence in-flight replacement')
    source(page, 'IMAGE_SLOW')
    selected_file(page, THIRD)
    page.locator('#btn-replace-document-image').click()
    wait(page, '__fmdNativeRuntime.imagesPending')
    page.evaluate("document.documentElement.style.setProperty('--fmd-base','24px')")
    settled(page)
    check(state(page)['saved']==slow['saved'], 'view changes cancel a preflight whose display snapshot is obsolete')
    source(page, 'File-read cancellation')
    selected_file(page, THIRD)
    before = state(page)
    page.evaluate("()=>{document.querySelector('#btn-replace-document-image').click();document.querySelector('#btn-cancel-image-change').click();}")
    settled(page)
    time.sleep(.1)
    check(state(page)==before, 'cancel during real FileReader admission prevents later decode or publication')
    page.evaluate("document.querySelector('#fmd-editor').dispatchEvent(new CompositionEvent('compositionstart'))")
    check(page.locator('#btn-remove-document-image').is_disabled(), 'text composition pauses resource edits')
    page.evaluate("document.querySelector('#fmd-editor').dispatchEvent(new CompositionEvent('compositionend'))")
    page.evaluate("document.querySelector('#fmd-editor').readOnly=true")
    wait(page, "document.querySelector('#btn-remove-document-image').disabled")
    check(page.locator('#btn-replace-document-image').is_disabled(), 'read-only source state cannot mutate embedded images')
    page.evaluate("document.querySelector('#fmd-editor').readOnly=false")
    wait(page, "!document.querySelector('#btn-remove-document-image').disabled")

    # Declined removal and confirmed removal both use the actual browser dialog.
    page.locator('#fmd-image-selection').select_option('unused.png')
    before = state(page)
    page.once('dialog', lambda dialog: dialog.dismiss())
    page.locator('#btn-remove-document-image').click()
    settled(page)
    check(state(page)==before, 'declining removal preserves source and resources')
    page.once('dialog', lambda dialog: dialog.accept())
    page.locator('#btn-remove-document-image').click()
    settled(page)
    check([x['destination'] for x in json.loads(state(page)['saved'])['images']]==['a.png','<img src=x onerror=alert(1)>'], 'confirmed removal reclaims a binding and preserves survivor order')
    check(state(page)['source']==before['source'], 'removal does not silently rewrite existing Markdown references')
    page.locator('#fmd-image-selection').select_option('a.png')
    replace(page, THIRD)
    page.screenshot(path=str(OUT/'document-images.png'))

    # Reparse a DOM snapshot with an open dialog: boot must not trust saved UI,
    # file selections, disabled attributes or messages as resource authority.
    selected_file(page, SECOND)
    snapshot = page.evaluate("'<!doctype html>\\n'+document.documentElement.cloneNode(true).outerHTML")
    reopened = open_page(snapshot)
    check(reopened.locator('#btn-document-images').count()==1 and reopened.locator('#fmd-document-images').count()==1,
          'reparsed saved shells rebuild a single fresh image manager')
    check(not reopened.locator('#fmd-document-images').evaluate('(node)=>node.open')
          and reopened.locator('#fmd-image-replacement').evaluate('(node)=>node.files.length')==0,
          'reopened manager discards open state and local-file selections')
    check(json.loads(state(reopened)['saved'])['images']==json.loads(state(page)['saved'])['images'], 'reopened runtime retains committed image replacements and removals')
    check(not reopened.evaluate("()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented;}"), 'reopening establishes a clean resource-change baseline')
    reopened.locator('#btn-document-images').click()
    replace(reopened, SECOND)
    check(base64.b64decode(json.loads(state(reopened)['saved'])['images'][0]['bytes'])==SECOND, 'reopened manager remains functional with the serialized production runtime')
    check(reopened.evaluate("()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented;}"), 'resource-only edits warn before unloading even without a source edit')

    source(page, 'IMAGE_SLOW')
    selected_file(page, SECOND)
    page.locator('#btn-replace-document-image').click()
    wait(page, '__fmdNativeRuntime.imagesPending')
    page.evaluate("window.dispatchEvent(new Event('pagehide'))")
    wait(page, '!__fmdNativeRuntime.imagesPending')
    check(page.locator('#btn-remove-document-image').is_disabled(), 'page suspension cancels worker and disables resource mutation')
    count = page.evaluate('__imageTest.workers')
    page.evaluate("window.dispatchEvent(new Event('pageshow'))")
    check(page.evaluate('__imageTest.workers')==count, 'page restoration never retries image mutations implicitly')
    page.keyboard.press('Escape')
    check(not page.locator('#fmd-document-images').evaluate('(node)=>node.open'), 'Escape dismisses the manager')
    page.locator('#btn-document-images').click()
    source(page, 'Remove all')
    while page.locator('#fmd-image-selection option').count():
        page.once('dialog', lambda dialog: dialog.accept())
        page.locator('#btn-remove-document-image').click()
        settled(page)
    check(page.evaluate('__fmdNativeRuntime.imageAssets.length')==0 and page.locator('#btn-remove-document-image').is_disabled(), 'empty resource sets remain usable and cannot remove a nonexistent selection')
    page.locator('#btn-close-document-images').click()
    page.locator('#fmd-image-picker').set_input_files({'name':'insert.png','mimeType':'image/png','buffer':FIRST})
    wait(page, '__fmdNativeRuntime.imageAssets.length===1')
    check('fmd-import/' in state(page)['source'], 'existing Insert image workflow remains compatible after replacements and removals')
    page.locator('#btn-document-images').click()
    check(page.locator('#fmd-image-selection option').count()==1, 'inventory refreshes after the existing insertion workflow commits')

    legacy = context.new_page()
    legacy.on('pageerror', lambda error: errors.append(str(error)))
    legacy.set_content(fixture(legacy=True), wait_until='domcontentloaded')
    wait(legacy, '__fmdNativeRuntime.settings')
    check(legacy.locator('#btn-document-images').count()==0, 'older runtimes do not expose unsupported image editing controls')
    violations = [value for opened in pages for value in opened.evaluate('__imageTest.violations')]
    check(not errors, 'no browser page errors')
    check(not violations, 'no Content Security Policy violations')
    check(not network, 'no HTTP or HTTPS requests')
    report = {'checks':len(checks),'passed':checks,'page_errors':errors,'csp_violations':violations,'network_requests':network,
              'scope':'Production image controls, resource runtime and worker transport; real Chromium FileReader/PNG decoding/Workers. Explicit shell and native ABI adapters; no compiled Rust/WASM rendering or full controller Save HTML validation.',
              'artifacts':str(OUT)}
    (OUT/'report.json').write_text(json.dumps(report,indent=2))
    print(json.dumps(report,indent=2))
    browser.close()
