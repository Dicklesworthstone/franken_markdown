"""Production font controls, file reads, native bootstrap and real Workers.

Uses the shared recording ABI adapter and explicit source/save host adapters,
not compiled WASM, actual font shaping, or the full workspace controller.
No installed fonts are read. Run with Python Playwright and CHROMIUM.
"""
import json
import os
from pathlib import Path
from playwright.sync_api import sync_playwright
from native_workspace_fonts_browser import RUNTIME, WORKER, INITIAL, INSTRUMENT

ROOT = Path(__file__).resolve().parents[1]
CONTROLS = (ROOT / 'src/interactive_fonts.js').read_text()
CSS = (ROOT / 'src/interactive.rs').read_text().split('const INTERACTIVE_CSS: &str = r#"', 1)[1].split('"#;', 1)[0]
ORIGINAL = '\ufeff# Document\r\n\r\nOriginal source.\0\r'


def fixture(payload=None, source=ORIGINAL, native=True):
    def data(value):
        return json.dumps(value).replace('<', '\\u003c')
    boot = ('bootNativeWorkspace(createNativeWorkspaceRenderer,createWorkspacePreviewWorker);'
            'window.native=window.__fmdNativeRuntime;' if native else '')
    return ('<!doctype html><html><head><meta charset="utf-8">'
            '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'unsafe-inline\' blob:; '
            'worker-src blob:; style-src \'unsafe-inline\'; frame-src \'self\' about:; base-uri \'none\'; form-action \'none\'">'
            '<style>' + CSS + '</style></head><body>'
            '<header class="fmd-app-header"><div class="fmd-toolbar"><button id="btn-save-html">Save HTML</button></div></header>'
            '<div id="fmd-app-body" class="fmd-app-body view-split"><section id="editor-pane" class="fmd-editor-pane">'
            '<span id="fmd-save-status"></span><textarea id="fmd-editor"></textarea></section>'
            '<main id="preview-pane" class="fmd-preview-pane"><div id="fmd-content"><p>Original preview</p></div></main></div>'
            '<script type="application/json" id="fmd-raw-source">' + data(source) + '</script>'
            '<script type="application/json" id="fmd-native-runtime">' + data(payload or INITIAL) + '</script>'
            '<script>' + INSTRUMENT + RUNTIME + '\n' + WORKER + '\n' + boot + '''
              window.editor=document.querySelector('#fmd-editor');
              window.preview=document.querySelector('#fmd-content');
              window.data=document.querySelector('#fmd-native-runtime');
              window.original=JSON.parse(document.querySelector('#fmd-raw-source').textContent);
              editor.value=original; window.originalView=editor.value; window.edits=0;
              window.currentSource=()=>editor.value===originalView?original:editor.value;
              editor.addEventListener('input',()=>{edits++;window.native?.invalidatePreview()});
              window.savedShell=()=>{
                const copy=document.documentElement.cloneNode(true);
                copy.querySelector('#fmd-raw-source').textContent=JSON.stringify(currentSource()).replace(/</g,'\\\\u003c');
                return '<!doctype html>'+copy.outerHTML;
              };
              window.unloadBlocked=()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented;};
            ''' + CONTROLS + '</script></body></html>')


def main():
    checks, errors, requests = [], [], []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=os.environ.get('CHROMIUM', '/usr/bin/chromium'),
                                     headless=True, args=['--no-sandbox'])
        page = browser.new_page()
        page.set_default_timeout(5000)
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.route('**/*', lambda route: (requests.append(route.request.url), route.abort()))
        page.on('dialog', lambda dialog: dialog.accept() if dialog.type == 'beforeunload' else dialog.dismiss())

        def boot(payload=None, source=ORIGINAL):
            page.goto('about:blank'); page.set_content(fixture(payload, source))
            assert page.evaluate('native.ready')
            page.locator('#btn-document-fonts').click()
            assert page.locator('#fmd-document-fonts').is_visible()

        def action(slot, value):
            page.locator('#fmd-font-action-' + slot).select_option(value)

        def select(slot='body-regular', data=b'\x0b\x0c', name='Local.ttf'):
            action(slot, 'file')
            page.locator('#fmd-font-file-' + slot).set_input_files({'name': name, 'mimeType': 'font/ttf', 'buffer': data})

        def weight(slot, value):
            page.locator('#fmd-font-weight-' + slot).fill(value)

        def apply():
            page.locator('#btn-apply-fonts').click()

        def wait(message):
            page.wait_for_function('(message)=>document.querySelector("#fmd-fonts-status").textContent.includes(message)', arg=message)

        def status():
            return page.locator('#fmd-fonts-status').inner_text()

        def state():
            return page.evaluate('({data:data.textContent,source:currentSource(),view:editor.value,html:preview.innerHTML,events:fontEvents})')

        def receipt():
            return page.evaluate('JSON.parse(new DOMParser().parseFromString(preview.firstElementChild.srcdoc,"text/html").querySelector("#receipt").textContent)')

        def check(name):
            checks.append(name)

        def source_change(text, event=True):
            page.evaluate('''({text,event})=>{editor.value=text;if(event)editor.dispatchEvent(new Event('input',{bubbles:true}));}''', {'text': text, 'event': event})

        def delay_reads():
            page.evaluate('''()=>{
              window.readers=[];window.aborts=0;
              window.FileReader=class {
                constructor(){this.readyState=0;readers.push(this)}
                readAsArrayBuffer(file){this.readyState=1;this.file=file;}
                abort(){this.readyState=2;aborts++;this.onabort?.();}
              };
            }''')

        boot()
        assert page.locator('#fmd-document-fonts fieldset').count() == 5
        assert '450' in page.locator('#fmd-font-info-body-regular').inner_text()
        assert page.locator('#fmd-font-info-body-bold').inner_text() == 'Native default'
        assert page.evaluate('!unloadBlocked()')
        select(); weight('body-regular', '625')
        select('body-bold', b'\x16', 'Bold.TTF'); weight('body-bold', '750')
        action('mono-regular', 'clear')
        apply(); wait('Fonts applied')
        r = receipt()
        assert r['fonts'] == [[11, 12], [22], [], [], []]
        assert r['weights'] == [625, 750, 0, 0, 0]
        assert page.evaluate('currentSource()') == ORIGINAL
        assert page.evaluate('edits') == 0 and page.evaluate('fontEvents') == 1
        assert page.evaluate('mainRenderCalls') == 0 and page.evaluate('workersActive') == 0
        assert page.evaluate('unloadBlocked()')
        assert 'Fonts modified' in page.locator('#fmd-save-status').inner_text()
        assert page.locator('#fmd-font-file-body-regular').evaluate('(e)=>e.files.length') == 0
        check('three-slot file/weight/clear batch commits once through real workers, preserving lossless source without an input event')

        shell = page.evaluate('savedShell()')
        page.goto('about:blank'); page.set_content(shell)
        assert page.evaluate('native.ready')
        assert not page.locator('#fmd-document-fonts').is_visible()
        page.locator('#btn-document-fonts').click()
        assert page.locator('#btn-document-fonts').count() == 1
        assert page.evaluate('native.fontSlots[0].weight') == 625
        assert page.evaluate('currentSource()') == ORIGINAL
        assert page.locator('#fmd-font-file-body-regular').evaluate('(e)=>e.files.length') == 0
        assert page.locator('#fmd-font-action-body-regular').input_value() == 'keep'
        assert page.evaluate('!unloadBlocked()')
        page.evaluate('fmdInstallDocumentFonts()')
        assert page.locator('#btn-document-fonts').count() == 1
        check('serialized workspace restores committed fonts and exact source while rebuilding a fresh, idempotent font dialog')

        action('body-regular', 'weight'); weight('body-regular', '')
        apply(); wait('Fonts applied')
        assert receipt()['fonts'][0] == [11, 12] and receipt()['weights'][0] == 0
        check('clearing only the weight pin retains the exact embedded font bytes')

        boot(); before = state(); action('body-regular', 'weight')
        apply(); wait('already match')
        assert state() == before and page.evaluate('workersCreated') == 0
        assert page.evaluate('!unloadBlocked()')
        apply(); wait('No font changes selected')
        check('identical weight and empty drafts emit no events, render no preview, and leave unload state clean')

        boot(); before = state(); select(); select('body-bold', b'\xff')
        apply(); wait('Fonts not applied')
        assert 'Native font validation rejected' in status()
        assert state() == before and page.evaluate('!unloadBlocked()')
        select('body-bold', b'\x22'); apply(); wait('Fonts applied')
        assert receipt()['fonts'][0] == [11, 12] and receipt()['fonts'][1] == [34]
        check('a rejected second font preserves the entire prior resource set and preview; correcting it permits one atomic retry')

        boot(); before = state()
        page.evaluate('''()=>{window.readCount=0;const Original=FileReader;
          window.FileReader=class extends Original {constructor(){super();readCount++}};}''')
        select(); select('body-bold', b'abc', 'bad.woff2')
        apply(); wait('Choose a local .ttf')
        assert page.evaluate('readCount') == 0 and state() == before
        check('every selected file is admitted before the first read; an unsupported second filename prevents partial reading or import')

        boot(); before = state(); action('body-bold', 'weight'); weight('body-bold', '700')
        apply(); wait('require an embedded font')
        assert state() == before
        action('body-regular', 'weight'); action('body-bold', 'keep'); weight('body-regular', '1001')
        apply(); wait('Weight pins must be integers')
        assert state() == before and page.evaluate('workersCreated') == 0
        check('invalid pins and weight-only edits of default slots reject before reading or allocating workers')

        boot(); before = state()
        for slot in ['body-regular','body-bold','body-italic']:
            action(slot, 'file')
            page.locator('#fmd-font-file-' + slot).evaluate('''e=>{
              Object.defineProperty(e,'files',{configurable:true,value:[{name:'large.ttf',size:32*1024*1024}]});
            }''')
        page.evaluate('''()=>{window.readCount=0;window.FileReader=class {constructor(){readCount++;throw Error('must not read')}};}''')
        apply(); wait('64 MiB batch limit')
        assert page.evaluate('readCount') == 0 and state() == before
        check('aggregate byte limits reject all selected files before allocating read buffers')

        boot(); before = state(); select(); delay_reads(); apply()
        page.wait_for_function('()=>readers.length===1')
        page.locator('#btn-cancel-fonts').click()
        assert page.evaluate('aborts') == 1 and page.evaluate('workersCreated') == 0
        assert state() == before and page.locator('#btn-apply-fonts').is_enabled()
        check('cancel aborts an in-flight local file read and releases the controls without native work')

        boot(); before = state(); select(); delay_reads(); apply()
        page.wait_for_function('()=>readers.length===1')
        page.locator('#btn-close-fonts').click()
        page.locator('#btn-document-fonts').click()
        assert page.evaluate('aborts') == 1
        assert page.locator('#fmd-font-file-body-regular').evaluate('(e)=>e.files.length') == 0
        assert state() == before
        check('closing and reopening discards selected file handles and old asynchronous operation state')

        boot(); before = state(); select(); delay_reads(); apply()
        page.wait_for_function('()=>readers.length===1')
        weight('body-regular', '500')
        assert page.evaluate('aborts') == 1 and state() == before
        check('editing a weight draft while a file is loading aborts the old batch without clearing the new draft')

        boot(); before = state(); select(); delay_reads(); apply()
        page.wait_for_function('()=>readers.length===1')
        page.evaluate('''()=>{const reader=readers[0];reader.result=new Uint8Array([1]).buffer;reader.readyState=2;reader.onload();}''')
        wait('admitted file size')
        assert state() == before and page.evaluate('workersCreated') == 0
        check('a size-mismatched file read never reaches native validation or alters saved fonts')

        boot(); before = state(); select(); delay_reads()
        page.evaluate('''()=>{const real=setTimeout;window.setTimeout=(fn,delay,...rest)=>real(fn,delay===10000?10:delay,...rest);}''')
        apply(); wait('ten seconds')
        assert state() == before and page.evaluate('aborts') == 1
        check('the local-read deadline aborts hanging reads and leaves committed fonts recoverable')

        boot(source='SLOW current'); before = state(); select(); apply()
        page.wait_for_function('()=>native.fontsPending')
        page.locator('#btn-cancel-fonts').click()
        assert state() == before and page.evaluate('workersActive') == 0
        assert page.locator('#btn-apply-fonts').is_enabled()
        check('cancelling native preflight terminates the worker without changing source, payload or preview')

        boot(source='SLOW current'); before = state(); select(); apply()
        page.wait_for_function('()=>native.fontsPending')
        source_change('newer source')
        page.wait_for_function('()=>!native.fontsPending')
        now = state(); assert now['data'] == before['data'] and now['html'] == before['html']
        assert now['source'] == 'newer source' and now['events'] == 0
        check('a source input during native font validation retires the old batch and retains the newer source')

        boot(source='SLOW current'); before = state(); select(); apply()
        page.wait_for_function('()=>native.fontsPending')
        source_change('silently newer', event=False)
        wait('Fonts not applied')
        assert state()['data'] == before['data'] and state()['html'] == before['html']
        check('the pre-publication snapshot check catches source changes without input events')

        boot(source='SLOW current'); before = state(); select(); apply()
        page.wait_for_function('()=>native.fontsPending')
        page.evaluate('''()=>{document.body.classList.add('theme-dark');document.body.classList.remove('theme-dark');}''')
        page.wait_for_function('()=>!native.fontsPending')
        assert state() == before
        check('view mutation records cancel even a theme change-and-revert before observer delivery')

        boot(source='SLOW current'); before = state(); select(); apply()
        page.wait_for_function('()=>native.fontsPending')
        page.locator('#fmd-font-weight-body-regular').dispatch_event('compositionstart')
        assert page.locator('#btn-apply-fonts').is_disabled() and not page.evaluate('native.fontsPending')
        page.locator('#fmd-font-weight-body-regular').dispatch_event('compositionend')
        assert page.locator('#btn-apply-fonts').is_enabled() and state() == before
        check('input-method composition pauses apply and cancels a previously captured font draft')

        boot(source='SLOW current'); before = state(); select(); apply()
        page.wait_for_function('()=>native.fontsPending')
        page.evaluate('window.dispatchEvent(new Event("pagehide"))')
        assert page.locator('#btn-apply-fonts').is_disabled() and page.evaluate('workersActive') == 0
        page.evaluate('window.dispatchEvent(new Event("pageshow"))')
        assert page.locator('#btn-apply-fonts').is_enabled() and state() == before
        assert page.locator('#fmd-font-file-body-regular').evaluate('(e)=>e.files.length') == 0
        check('suspension aborts pending native work and clears file handles; resuming never applies stale drafts')

        boot(source='SLOW current'); select(); apply()
        page.wait_for_function('()=>native.fontsPending'); page.locator('#btn-cancel-fonts').click()
        select(data=b'\x29'); apply()
        wait('Fonts applied')
        assert receipt()['fonts'][0] == [41] and page.evaluate('fontEvents') == 1
        check('a cancelled operation cannot reset controls or overwrite a newer successful apply')

        boot(); before = state(); select()
        page.evaluate('''()=>{window.job=native.exportDocument('pdf','SLOW export');}''')
        apply(); wait('current document operation')
        page.evaluate('job')
        assert state() == before
        apply(); wait('Fonts applied')
        check('font authoring respects the export operation slot and remains usable once that export finishes')

        boot(); before = state(); select()
        page.evaluate('''()=>{const real=preview.replaceChildren.bind(preview);let once=true;
          preview.replaceChildren=(...nodes)=>{if(once){once=false;throw Error('host refused preview')}return real(...nodes)};}''')
        apply(); wait('Fonts not applied')
        assert state() == before and page.evaluate('!unloadBlocked()')
        check('atomic publication failure leaves the authoring dialog retryable without dirtying the document')

        boot(); select()
        page.keyboard.press('Escape')
        assert not page.locator('#fmd-document-fonts').is_visible()
        page.locator('#btn-document-fonts').click()
        assert page.locator('#fmd-font-file-body-regular').evaluate('(e)=>e.files.length') == 0
        page.emulate_media(media='print')
        assert not page.locator('#fmd-document-fonts').is_visible()
        page.emulate_media(media='screen')
        assert page.locator('#fmd-document-fonts').is_visible()
        check('Escape releases the file draft and production print CSS excludes the modal without changing screen state')

        page.goto('about:blank'); page.set_content(fixture(native=False))
        assert page.locator('#btn-document-fonts').count() == 0
        check('lightweight or legacy runtimes are not offered nonfunctional native font controls')

        assert errors == [], errors
        assert requests == [], requests
        check('all authoring scenarios finish with zero page errors and zero HTTP requests')
        output = {'scope': 'production font controls, local FileReader, bootstrap and real Workers with recording ABI and source/save host adapters; not full controller or compiled WASM',
                  'browser': browser.version, 'passed': len(checks), 'checks': checks, 'pageErrors': errors, 'httpRequests': len(requests)}
        browser.close()
    print(json.dumps(output, indent=2))


if __name__ == '__main__':
    main()
