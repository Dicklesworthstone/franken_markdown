"""Exercise production workspace bootstrap, transactions and real Worker transport.

The embedded ABI adapter records arguments and injects errors/delays; it does not
compile WASM or validate/shape real fonts. No installed font file is used.
Run: CHROMIUM=/usr/bin/chromium python wasm/native_workspace_fonts_browser.py
"""
import base64
import json
import os
from pathlib import Path
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = (ROOT / 'wasm/interactive_runtime.mjs').read_text().replace('export function ', 'function ')
WORKER = (ROOT / 'wasm/interactive_preview.mjs').read_text().replace('export function ', 'function ')
BINDINGS = r'''
export default async function initialize() {}
function envelope(mimeType, text) {
  return {mimeType, bytes: new TextEncoder().encode(text), diagnosticsJson: () => '[]', free() {}};
}
function receipt(args, index) {
  if (typeof window !== 'undefined') window.mainRenderCalls = (window.mainRenderCalls || 0) + 1;
  const fonts = args.slice(index, index + 5).map(bytes => Array.from(bytes));
  if (fonts.some(bytes => bytes[0] === 255)) throw Error('Native font validation rejected these bytes');
  if (args[0].includes('SLOW')) {
    const until = performance.now() + 500;
    while (performance.now() < until) {}
  }
  return {source: args[0], fonts, weights: Array.from(args[index + 5]),
    title: args[3], font: args[1], scale: args[6]};
}
function escape(text) { return text.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
export function renderHtmlConfiguredAdvanced(...args) {
  return envelope('text/html','<!doctype html><html><head></head><body><pre id="receipt">'
    +escape(JSON.stringify(receipt(args,7)))+'</pre></body></html>');
}
export function renderPdfConfiguredMulti(...args) {
  return envelope('application/pdf','%PDF-\n'+JSON.stringify(receipt(args,11)));
}
export function renderPdfConfiguredPage(...args) { return renderPdfConfiguredMulti(...args); }
export function renderEpubConfiguredAdvanced(...args) {
  return envelope('application/epub+zip','PK\x03\x04\n'+JSON.stringify(receipt(args,12)));
}
export function renderSvgConfiguredResources(...args) {
  return envelope('image/svg+xml','<svg xmlns="http://www.w3.org/2000/svg"><desc>'
    +escape(JSON.stringify(receipt(args,8)))+'</desc></svg>');
}
export function accessibilityAudit() {return JSON.stringify({schema_version:'1',target:'pdf',findings:[]});}
'''
# These deliberately small envelopes model two distinguishable selected faces.
# The recording adapter is not an SFNT/TTC parser; real extraction is exercised
# separately by the native fixture tests and generated-WASM collection smoke.
COLLECTION = b'ttcf\x00\x01\x00\x00\x00\x00\x00\x02\x00'
SLOW_COLLECTION = COLLECTION[:-1] + b'\xfe'
COLLECTION_FACES = (
    b'\x00\x01\x00\x00\x00\x01\x00\x10\x00\x00\x00\x00\x0b\x0c',
    b'OTTO\x00\x01\x00\x10\x00\x00\x00\x00\x15\x16',
)
LEGACY_BINDINGS = BINDINGS
BINDINGS += r'''
export function extractFontCollectionFace(bytes, faceIndex) {
  if (typeof window !== 'undefined') {
    window.mainCollectionCalls = (window.mainCollectionCalls || 0) + 1;
    throw Error('Collection extraction ran on the UI thread');
  }
  if (bytes[0] !== 0x74 || bytes[1] !== 0x74 || bytes[2] !== 0x63 || bytes[3] !== 0x66) {
    throw 'FONT_COLLECTION_NOT_COLLECTION: Expected the recording TTC fixture';
  }
  if (!Number.isSafeInteger(faceIndex) || faceIndex < 0 || faceIndex >= 2) {
    throw 'FONT_COLLECTION_FACE_INDEX: The recording collection has two faces, indexed 0 and 1';
  }
  if (bytes[bytes.length - 1] === 254) {
    const until = performance.now() + 500;
    while (performance.now() < until) {}
  }
  return new Uint8Array(faceIndex === 0
    ? [0,1,0,0,0,1,0,16,0,0,0,0,11,12]
    : [79,84,84,79,0,1,0,16,0,0,0,0,21,22]);
}
'''
INITIAL = {'version': 1, 'wasm': 'AA==', 'bindings': BINDINGS,
           'options': {'font': 'serif', 'darkMode': 'auto', 'fontScale': 1.25,
                       'title': 'Font document', 'pageNumbers': True, 'codeLineNumbers': False, 'toc': False},
           'images': [{'destination': 'plot.png', 'bytes': 'BAUG'}],
           'fonts': [{'slot': 'body-regular', 'bytes': 'AQID', 'weight': 450},
                     {'slot': 'mono-regular', 'bytes': 'BwgJ'}]}
INSTRUMENT = '''
window.mainRenderCalls = 0; window.fontEvents = 0; window.workersCreated = 0;
window.workersActive = 0; window.workersMaximum = 0;
window.mainCollectionCalls = 0; window.collectionRequests = [];
window.addEventListener('fmd-native-fonts-changed',()=>window.fontEvents++);
const OriginalWorker = window.Worker;
window.Worker = class extends OriginalWorker {
  constructor(...args) {
    super(...args); window.workersCreated++; window.workersActive++;
    window.workersMaximum = Math.max(window.workersMaximum, window.workersActive);
  }
  terminate() {
    if (!this.stopped) {this.stopped=true;window.workersActive--;}
    return super.terminate();
  }
  postMessage(message, transfer) {
    if (message?.type === 'font-collection') {
      window.collectionRequests.push({faceIndex:message.faceIndex, bytes:Array.from(message.bytes)});
    }
    return super.postMessage(message, transfer);
  }
};
'''

def fixture(payload=None):
    encoded = json.dumps(payload or INITIAL).replace('<', '\\u003c')
    return ('<!doctype html><html><head><meta charset="utf-8">'
            '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'unsafe-inline\' blob:; '
            'worker-src blob:; style-src \'unsafe-inline\'; frame-src \'self\' about:; base-uri \'none\'">'
            '</head><body><textarea id="source">Document</textarea><div id="preview"><p>Original preview</p></div>'
            '<script type="application/json" id="fmd-native-runtime">' + encoded + '</script>'
            '<script>' + INSTRUMENT + RUNTIME + '\n' + WORKER + '\n'
            'bootNativeWorkspace(createNativeWorkspaceRenderer, createWorkspacePreviewWorker);\n'
            'window.native = window.__fmdNativeRuntime; window.preview=document.querySelector("#preview");'
            'window.data=document.querySelector("#fmd-native-runtime");'
            'window.apply=(bytes=[11,12],source="Document",current=()=>true)=>native.applyFontsAsync('
            '[{slot:"body-regular",bytes:new Uint8Array(bytes),weight:625}],source,preview,{scale:1},current);'
            '</script></body></html>')


def main():
    checks, requests, errors = [], [], []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=os.environ.get('CHROMIUM', '/usr/bin/chromium'),
                                     headless=True, args=['--no-sandbox'])
        page = browser.new_page()
        page.set_default_timeout(5000)
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.route('**/*', lambda route: (requests.append(route.request.url), route.abort()))

        def boot(payload=None):
            page.goto('about:blank'); page.set_content(fixture(payload))
            assert page.evaluate('native.ready') is True

        def check(name):
            checks.append(name)

        def state():
            return page.evaluate('({data:data.textContent,html:preview.innerHTML,slots:native.fontSlots,events:fontEvents})')

        def receipt():
            return page.evaluate('JSON.parse(new DOMParser().parseFromString(preview.firstElementChild.srcdoc,"text/html").querySelector("#receipt").textContent)')

        def start(source='SLOW', bytes=None):
            page.evaluate('''({source,bytes})=>{
              window.settled=null;
              window.job=apply(bytes||[11,12],source).then(value=>{settled={ok:true,value}},error=>{settled={ok:false,code:error.code,message:error.message}});
            }''', {'source': source, 'bytes': bytes})

        def start_collection(data=COLLECTION, index=1):
            page.evaluate('''({bytes,index})=>{
              window.settled=null;
              window.job=native.extractFontCollectionFaceAsync(new Uint8Array(bytes),index)
                .then(value=>{settled={ok:true,bytes:Array.from(value)}},
                      error=>{settled={ok:false,code:error.code,message:error.message}});
            }''', {'bytes': list(data), 'index': index})

        def finish():
            page.wait_for_function('() => settled !== null'); return page.evaluate('settled')

        boot()
        assert page.evaluate('native.fontMode') == 'worker'
        page.evaluate('native.render("before",preview,{scale:1})')
        assert receipt()['fonts'][0] == [1, 2, 3]
        old_data = page.evaluate('JSON.parse(data.textContent)')
        page.evaluate('apply()')
        assert receipt()['fonts'] == [[11, 12], [], [], [], [7, 8, 9]]
        assert receipt()['weights'] == [625, 0, 0, 0, 0]
        updated = page.evaluate('JSON.parse(data.textContent)')
        assert updated['images'] == old_data['images'] and updated['options'] == old_data['options']
        assert updated['fonts'][0] == {'slot': 'body-regular', 'bytes': 'Cww=', 'weight': 625}
        assert page.evaluate('mainRenderCalls') == 0
        assert page.evaluate('workersActive') == 0
        assert page.evaluate('workersMaximum') <= 2
        assert page.evaluate('fontEvents') == 1
        check('font preflight runs in a real worker, atomically updates preview/data, and preserves source-independent settings/images')

        page.evaluate('native.render("after",preview,{scale:1})')
        assert receipt()['fonts'][0] == [11, 12]
        assert receipt()['source'] == 'after'
        assert page.evaluate('workersActive') == 1
        page.evaluate('native.render("another edit",preview,{scale:1.2})')
        assert receipt()['fonts'][0] == [11, 12]
        check('cached pre-change preview worker is retired; subsequent edits keep the newly committed fonts')

        for kind in ['html', 'pdf', 'epub', 'svg']:
            result = page.evaluate('''async kind=>{
              const result=await native.exportDocument(kind,"published");
              const text=new TextDecoder().decode(result.bytes);
              if(kind==='html') return JSON.parse(new DOMParser().parseFromString(text,'text/html').querySelector('#receipt').textContent);
              if(kind==='svg') return JSON.parse(new DOMParser().parseFromString(text,'image/svg+xml').querySelector('desc').textContent);
              return JSON.parse(text.slice(text.indexOf('\\n')+1));
            }''', kind)
            assert result['fonts'] == [[11, 12], [], [], [], [7, 8, 9]], kind
            assert result['weights'] == [625, 0, 0, 0, 0], kind
            assert result['source'] == 'published'
        assert page.evaluate('mainRenderCalls') == 0
        assert page.evaluate('workersActive') == 1
        check('HTML/PDF/EPUB/SVG publication workers all receive exact updated bytes and weights with no UI-thread render')

        saved = page.evaluate('JSON.parse(data.textContent)')
        boot(saved)
        page.evaluate('native.render("reopened",preview,{scale:1})')
        assert receipt()['fonts'][0] == [11, 12]
        assert page.evaluate('native.fontSlots[0].weight') == 625
        check('saved inert runtime payload reopens with the same font slot bytes and weight pins')

        page.evaluate('native.applyFontsAsync([{slot:"body-regular",clear:true}],"cleared",preview,{scale:1})')
        assert receipt()['fonts'][0] == [] and receipt()['weights'][0] == 0
        assert receipt()['fonts'][4] == [7, 8, 9]
        assert page.evaluate('native.fontSlots[0].bytes') == 0
        check('clearing a slot restores its native default without dropping other embedded faces')

        boot(); before = state(); created = page.evaluate('workersCreated')
        page.evaluate('native.applyFontsAsync([{slot:"body-regular",weight:450}],"Document",preview,{scale:1})')
        assert state() == before and page.evaluate('workersCreated') == created
        check('identical font updates are no-ops with no worker, persistence, preview or event changes')

        boot(); before = state()
        start('Document', [255]); result = finish()
        assert result['ok'] is False and 'Native font validation rejected' in result['message']
        assert state() == before and page.evaluate('!native.fontsPending && workersActive===0')
        page.evaluate('apply()')
        assert receipt()['fonts'][0] == [11, 12]
        check('native font rejection preserves old resources and preview, releases the worker, and allows retry')

        boot(); before = state(); start()
        assert page.evaluate('native.fontsPending && native.settingsPending')
        for expression in [
            'apply()', 'native.exportDocument("pdf","Document")',
            'native.applySettingsAsync({fontScale:2},"Document",preview,{scale:1})',
            'native.analyzeDocument("accessibility","Document")']:
            result = page.evaluate("() => {try{" + expression + ";return 'accepted'}catch(error){return error.code}}")
            assert result.endswith('_BUSY'), result
        page.evaluate('native.cancelFonts()'); assert finish()['code'] == 'FONTS_CANCELLED'
        assert state() == before
        check('font changes share the bounded document-operation slot with exports, settings and analysis; cancellation leaves all state intact')

        boot(); before = state()
        page.evaluate('''()=>{
          window.settled=null;
          window.job=apply().then(()=>settled={ok:true},error=>settled={code:error.code});
          native.cancelFonts();
        }''')
        assert finish()['code'] == 'FONTS_CANCELLED'
        assert state() == before and page.evaluate('workersCreated') == 0
        check('cancellation before the promise boundary does not initialize any worker')

        boot(); before = state(); start(); page.evaluate('native.invalidatePreview()')
        assert finish()['code'] == 'FONTS_CANCELLED' and state() == before
        check('ordinary source/view invalidation cancels a staged font preflight')

        boot(); before = state()
        page.evaluate('''()=>{
          window.current=true;window.settled=null;
          window.job=apply([11,12],'SLOW',()=>current).then(()=>settled={ok:true},error=>settled={code:error.code});
          current=false;
        }''')
        assert finish()['code'] == 'FONTS_CANCELLED' and state() == before
        check('an explicit current-revision guard rejects source changes even without an input event')

        boot(); before = state(); start()
        page.evaluate('window.dispatchEvent(new Event("pagehide"))')
        assert finish()['code'] == 'FONTS_CANCELLED' and state() == before
        assert page.evaluate('workersActive') == 0
        assert page.evaluate('''()=>{try{apply();return false}catch(e){return e.code}}''') == 'FONTS_SUSPENDED'
        page.evaluate('window.dispatchEvent(new Event("pageshow"))'); page.evaluate('apply()')
        assert receipt()['fonts'][0] == [11, 12]
        check('page suspension terminates preflight, blocks new changes and safely resumes')

        boot(); start()
        page.evaluate('''()=>{
          window.old=job; native.cancelFonts();
          window.newResult=null;
          window.newJob=apply([21,22],'SLOW new').then(()=>newResult={ok:true},e=>newResult={code:e.code});
        }''')
        assert finish()['code'] == 'FONTS_CANCELLED'
        assert page.evaluate('native.fontsPending')
        page.wait_for_function('() => newResult !== null')
        assert page.evaluate('newResult.ok') is True and receipt()['fonts'][0] == [21, 22]
        check('late completion of a cancelled transaction cannot release or overwrite a newer font operation')

        boot(); before = state(); start()
        page.evaluate('native.stageImages([{destination:"new.png",bytes:new Uint8Array([42])}]).commit()')
        assert finish()['code'] == 'FONTS_CANCELLED'
        assert page.evaluate('JSON.parse(data.textContent).fonts') == json.loads(before['data'])['fonts']
        assert page.evaluate('JSON.parse(data.textContent).images.length') == 2
        check('an intervening image transaction cancels old font work and retains the newer resource update')

        boot(); before = state(); start()
        page.evaluate('native.applySettings({fontScale:1.5},"settings",preview,{scale:1})')
        assert finish()['code'] == 'FONTS_CANCELLED'
        assert page.evaluate('native.settings.fontScale') == 1.5
        assert page.evaluate('JSON.parse(data.textContent).fonts') == json.loads(before['data'])['fonts']
        check('legacy synchronous settings publication cancels staged fonts without losing the committed settings')

        for target in ['preview', 'data']:
            boot(); before = state()
            if target == 'preview':
                page.evaluate('''()=>{const real=preview.replaceChildren.bind(preview);let once=true;
                  preview.replaceChildren=(...args)=>{if(once){once=false;throw Error('preview refused')}return real(...args)};
                }''')
            else:
                page.evaluate('''()=>{const property=Object.getOwnPropertyDescriptor(Node.prototype,'textContent');let once=true;
                  Object.defineProperty(data,'textContent',{get(){return property.get.call(this)},set(v){if(once){once=false;throw Error('data refused')}property.set.call(this,v)}});
                }''')
            start('Document'); result = finish()
            assert result['ok'] is False and target + ' refused' in result['message']
            assert state() == before and page.evaluate('!native.fontsPending && workersActive===0')
        check('preview or inert-JSON publication failure restores both old DOM and old data before live fonts switch')

        boot(); before = state()
        page.evaluate('''()=>{
          const Native=window.Worker;
          window.Worker=class {constructor(){throw Error('blocked worker')}};
        }''')
        start('Document'); result = finish()
        assert not result['ok'] and state() == before and page.evaluate('mainRenderCalls') == 0
        check('worker startup failure never falls back to rendering fonts on the UI thread')

        boot(); before = state()
        for params in [None, [], [{'slot': 'bad', 'clear': True}], [{'slot': 'body-regular', 'weight': 0}]]:
            result = page.evaluate('''patches=>{try{native.applyFontsAsync(patches,'Document',preview,{scale:1});return true}catch(e){return false}}''', params)
            assert result is False
        assert state() == before and page.evaluate('workersCreated') == 0
        check('invalid patch admission happens before worker allocation or mutation')

        boot(); before = state()
        selected = page.evaluate('''async bytes=>{
          const backing=new Uint8Array(bytes.length+2);backing.set(bytes,1);
          const view=new DataView(backing.buffer,1,bytes.length);
          const pending=native.extractFontCollectionFaceAsync(view,1);
          backing.fill(255);
          window.selectedFace=await pending;
          return {bytes:Array.from(selectedFace),offset:selectedFace.byteOffset,
            length:selectedFace.byteLength,bufferLength:selectedFace.buffer.byteLength};
        }''', list(COLLECTION))
        assert selected == {'bytes': list(COLLECTION_FACES[1]), 'offset': 0,
                            'length': len(COLLECTION_FACES[1]), 'bufferLength': len(COLLECTION_FACES[1])}
        assert state() == before
        assert page.evaluate('collectionRequests') == [{'faceIndex': 1, 'bytes': list(COLLECTION)}]
        assert page.evaluate('mainCollectionCalls===0 && mainRenderCalls===0 && workersActive===0 && !native.fontsPending')
        check('collection selection snapshots only the supplied view before await, returns owned standalone bytes, and changes no live or saved font state')

        page.evaluate('native.applyFontsAsync([{slot:"body-regular",bytes:selectedFace,weight:650}],"collection preview",preview,{scale:1})')
        expected_fonts = [list(COLLECTION_FACES[1]), [], [], [], [7, 8, 9]]
        assert receipt()['fonts'] == expected_fonts
        saved = page.evaluate('JSON.parse(data.textContent)')
        assert saved['fonts'][0] == {'slot': 'body-regular',
                                    'bytes': base64.b64encode(COLLECTION_FACES[1]).decode(), 'weight': 650}
        assert saved['images'] == INITIAL['images'] and saved['options'] == INITIAL['options']
        for kind in ['html', 'pdf', 'epub', 'svg']:
            published = page.evaluate('''async kind=>{
              const result=await native.exportDocument(kind,'selected face publication');
              const text=new TextDecoder().decode(result.bytes);
              if(kind==='html') return JSON.parse(new DOMParser().parseFromString(text,'text/html').querySelector('#receipt').textContent);
              if(kind==='svg') return JSON.parse(new DOMParser().parseFromString(text,'image/svg+xml').querySelector('desc').textContent);
              return JSON.parse(text.slice(text.indexOf('\\n')+1));
            }''', kind)
            assert published['fonts'] == expected_fonts, kind
            assert published['weights'] == [650, 0, 0, 0, 0], kind
            assert published['source'] == 'selected face publication', kind
        assert page.evaluate('collectionRequests.length') == 1
        assert page.evaluate('mainCollectionCalls===0 && mainRenderCalls===0 && workersActive===0')
        boot(saved)
        page.evaluate('native.render("reopened collection face",preview,{scale:1})')
        assert receipt()['fonts'] == expected_fonts and receipt()['weights'][0] == 650
        assert page.evaluate('collectionRequests.length') == 0
        check('selected standalone bytes and weight persist and reopen unchanged; HTML/PDF/EPUB/SVG workers receive the same selected face without re-extracting the collection')

        boot(); before = state(); start_collection(index=2)
        result = finish()
        assert result['ok'] is False and result['code'] == 'FONT_COLLECTION_FACE_INDEX'
        assert 'two faces' in result['message']
        assert state() == before and page.evaluate('!native.fontsPending && workersActive===0')
        start_collection(index=0)
        assert finish()['bytes'] == list(COLLECTION_FACES[0])
        assert state() == before and page.evaluate('mainCollectionCalls') == 0
        check('an unavailable collection face retains its typed native error and all old state; explicit face zero remains independently selectable')

        boot(); before = state()
        for index in [-1, 0.5, 1024, '1', None]:
            result = page.evaluate('''async ({bytes,index})=>{
              try {await native.extractFontCollectionFaceAsync(new Uint8Array(bytes),index);return {accepted:true};}
              catch(error){return {accepted:false,message:error.message};}
            }''', {'bytes': list(COLLECTION), 'index': index})
            assert result['accepted'] is False and 'integer from 0 through 1023' in result['message']
        assert state() == before and page.evaluate('workersCreated===0 && !native.fontsPending')
        check('collection face admission rejects missing, fractional, coerced and out-of-bound indices before starting a worker')

        boot(); before = state(); start_collection(SLOW_COLLECTION)
        page.wait_for_function('()=>collectionRequests.length===1')
        assert page.evaluate('native.fontsPending && native.settingsPending')
        page.evaluate('native.cancelFonts()')
        assert finish()['code'] == 'FONTS_CANCELLED'
        assert state() == before and page.evaluate('workersActive===0 && !native.fontsPending')
        check('cancelling a dispatched collection selection terminates its real worker and leaves preview, saved bytes and events intact')

        boot(); before = state()
        page.evaluate('''bytes=>{
          window.current=true;window.settled=null;
          window.job=native.extractFontCollectionFaceAsync(new Uint8Array(bytes),1,()=>current)
            .then(value=>{settled={ok:true}},error=>{settled={code:error.code}});
        }''', list(SLOW_COLLECTION))
        page.wait_for_function('()=>collectionRequests.length===1')
        page.evaluate('current=false')
        assert finish()['code'] == 'FONTS_CANCELLED'
        assert state() == before and page.evaluate('workersActive===0 && !native.fontsPending')
        check('a selection whose revision changes while its worker is running cannot publish or return stale face bytes')

        boot(); before = state(); start_collection(SLOW_COLLECTION)
        page.wait_for_function('()=>collectionRequests.length===1')
        page.evaluate('''bytes=>{
          native.cancelFonts();window.newResult=null;
          window.newJob=native.extractFontCollectionFaceAsync(new Uint8Array(bytes),0)
            .then(value=>{newResult={ok:true,bytes:Array.from(value)}},error=>{newResult={code:error.code}});
        }''', list(SLOW_COLLECTION))
        assert finish()['code'] == 'FONTS_CANCELLED'
        page.wait_for_function('()=>newResult!==null')
        assert page.evaluate('newResult') == {'ok': True, 'bytes': list(COLLECTION_FACES[0])}
        assert state() == before and page.evaluate('workersActive===0 && !native.fontsPending')
        check('completion of a cancelled collection job cannot cancel or overwrite a later explicit face selection')

        boot({**INITIAL, 'bindings': LEGACY_BINDINGS}); before = state()
        start_collection(); result = finish()
        assert result['ok'] is False and result['code'] == 'UNSUPPORTED_WASM_PACKAGE'
        assert 'matching WASM bindings with extractFontCollectionFace' in result['message']
        assert state() == before and page.evaluate('workersActive===0 && !native.fontsPending')
        page.evaluate('apply()')
        assert receipt()['fonts'][0] == [11, 12]
        assert page.evaluate('mainCollectionCalls===0 && mainRenderCalls===0')
        check('older generated bindings clearly refuse collection selection while existing standalone font preflight remains available')

        assert requests == [], requests
        assert errors == [], errors
        check('all scenarios finish without HTTP requests or page errors')
        output = {'scope': 'production bootstrap/resource transactions/Worker transport with an explicit recording ABI adapter, not compiled WASM or font shaping',
                  'browser': browser.version, 'passed': len(checks), 'checks': checks,
                  'httpRequests': len(requests), 'pageErrors': errors}
        browser.close()
    print(json.dumps(output, indent=2))

if __name__ == '__main__':
    main()
