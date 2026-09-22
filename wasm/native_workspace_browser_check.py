"""Factory-output lifecycle in Chromium. Pass a native_workspace_fixture.mjs
artifact; renderer calls are explicit ABI doubles, not proof of Rust rendering.
Uses actual exported bytes, real downloads, and fresh pages for each reopening.
Default file mode attempts real file:// navigation; content mode is explicit.
"""
import argparse
import json
from pathlib import Path
import shutil
import tempfile
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument('html', type=Path)
parser.add_argument('--chromium', default=shutil.which('chromium'))
parser.add_argument('--mode', choices=['file', 'content'], default='file')
args=parser.parse_args()
errors, requests = [], []
checks=0
retained=Path(tempfile.mkdtemp(prefix='fmd-native-factory-browser-'))

def check(ok, message):
    global checks
    assert ok, f'{message}; browser_errors={errors}'
    checks+=1

with sync_playwright() as p:
    browser=p.chromium.launch(executable_path=args.chromium,headless=True,args=['--no-sandbox'])
    context=browser.new_context(accept_downloads=True,viewport={'width':1600,'height':1000})
    context.set_default_timeout(5000)
    context.route('https://**/*',lambda route:(requests.append(route.request.url),route.abort()))
    context.route('http://**/*',lambda route:(requests.append(route.request.url),route.abort()))
    pages=[]
    def load(data):
        page=context.new_page();pages.append(page)
        page.on('pageerror',lambda error:errors.append(str(error)))
        path=retained/f'generation-{len(pages)}.html';path.write_bytes(data)
        if args.mode=='file':
            page.goto(path.as_uri(),wait_until='load')
        else:
            page.set_content(data.decode(),wait_until='load')
        return page
    def source(page):
        return page.evaluate('JSON.parse(document.querySelector(\'body > script#fmd-raw-source[type="application/json"]\').textContent)')
    def payload(page):
        return page.evaluate('JSON.parse(document.querySelector(\'body > script#fmd-native-runtime[type="application/json"]\').textContent)')
    def edit(page,text):
        page.evaluate('text=>{const e=document.querySelector("textarea#fmd-editor");e.value=text;e.dispatchEvent(new Event("input"));}',text)
    def dirty(page):
        return page.evaluate('()=>{const event=new Event("beforeunload",{cancelable:true});window.dispatchEvent(event);return event.defaultPrevented;}')
    def release(page):
        page.wait_for_function('typeof window.releaseFixtureRuntime === "function"')
        page.evaluate('window.releaseFixtureRuntime()')
        check(page.evaluate('window.__fmdNativeRuntime.ready'),'native startup failed')
        page.wait_for_function('document.querySelector("#fmd-content iframe")?.contentDocument?.querySelector("#answer")?.textContent === "7"')
    def preview(page):
        return page.frame_locator('#fmd-content iframe')
    def download(page,button):
        with page.expect_download() as pending:page.locator('#'+button).click()
        item=pending.value
        check(item.failure() is None,'download failed')
        return Path(item.path()).read_bytes(),item.suggested_filename

    page=load(args.html.read_bytes())
    original=source(page);resources=payload(page)
    check(not dirty(page),'opening dirtied the source')
    # Initialization deliberately waits; the static first preview is already a
    # complete iframe and must not fetch the old unsandboxed shell's image.
    page.wait_for_function('document.querySelector("#fmd-content iframe")?.contentDocument?.querySelector("#answer")?.textContent === "7"')
    check(page.locator('#fmd-content > img').count()==0,'initial image fragment was left outside sandbox')
    raw,_=download(page,'btn-save-markdown')
    check(raw==original.encode(),'startup Markdown download lost CRLF/null/Unicode')
    current='# During startup\n\nchart.svg\n\n# fmd-native-runtime\n\n</script><script>globalThis.sourceRan=true</script> é中😀'
    edit(page,current)
    release(page)
    check(preview(page).locator('#source').inner_text()==current,'startup reverted intervening edits')
    check(preview(page).locator('#weights').inner_text()=='555,0,0,0,0','font pins lost')
    check(preview(page).locator('#scale').inner_text()=='1.25','native type scale lost')
    image=preview(page).locator('img[alt="chart.svg"]')
    check(image.count()==1,'supplied image binding absent')
    image.evaluate('img => img.decode()')
    check(image.evaluate('img=>img.naturalWidth')==30,'embedded image failed actual browser decoding')
    check(page.evaluate('globalThis.sourceRan === undefined'),'source executed')
    check(page.locator('#fmd-content iframe').evaluate('f=>f.contentWindow.previewExecuted === undefined'),'preview scripts executed')
    check(dirty(page),'editing failed to mark source dirty')
    # Export before any queued debounce: PDF must read source, not iframe DOM.
    latest=current+'\n\nImmediate PDF revision'
    edit(page,latest)
    pdf,name=download(page,'btn-export-pdf')
    check(name.endswith('.pdf') and pdf.startswith(b'%PDF-ADAPTER\n'),'wrong export envelope')
    model=json.loads(pdf.split(b'\n',1)[1])
    check(model['source']==latest,'PDF saw a stale source revision')
    check(model['answer']==7 and model['epoch']==0,'PDF engine or metadata changed')
    geometry=[297*72/25.4,210*72/25.4,24,36,48,60]
    check(model['page']==geometry,'PDF paper orientation, dimensions or margin order changed')
    check(resources['options']['pageGeometry']==geometry,'initial artifact lost page settings')
    check(model['weights']==[555,0,0,0,0] and len(model['images'])==2,'PDF lost resource arrays')
    page.locator('#btn-zoom-in').click()
    preview(page).locator('#scale').wait_for()
    check(preview(page).locator('#scale').inner_text()=='1.375','view zoom not applied through native renderer')
    pdf,_=download(page,'btn-export-pdf')
    check(json.loads(pdf.split(b'\n',1)[1])['scale']==1.25,'view zoom incorrectly changed PDF typography')
    page.locator('#btn-theme-toggle').click()
    html,_=download(page,'btn-save-html')
    check(source(page)==original,'saving mutated live source baseline')
    second=load(html);release(second)
    check(source(second)==latest,'reopening lost saved source')
    check(payload(second)==resources,'reopening changed runtime, fonts or unused resources')
    check(not dirty(second),'reopened document is spuriously dirty')
    check(preview(second).locator('#source').inner_text()==latest,'reopening saved a stale preview')
    check(second.locator('body').evaluate('e=>e.classList.contains("theme-dark")'),'saved theme lost')
    check(preview(second).locator('#scale').inner_text()=='1.375','saved zoom lost')
    removed='Temporarily no asset references'
    edit(second,removed)
    html,_=download(second,'btn-save-html')
    third=load(html);release(third)
    check(payload(third)==resources,'unused resources disappeared between generations')
    edit(third,'Restored chart.svg and unused.svg')
    html,_=download(third,'btn-save-html')
    fourth=load(html);release(fourth)
    check(preview(fourth).locator('img[alt]').count()==2,'reinserted resource references failed')
    check(payload(fourth)==resources,'repeated save/reopen changed the runtime')
    pdf,_=download(fourth,'btn-export-pdf')
    check(json.loads(pdf.split(b'\n',1)[1])['page']==geometry,'reopened PDF lost page geometry')
    raw,_=download(fourth,'btn-save-markdown')
    check(raw==b'Restored chart.svg and unused.svg','reopened Markdown download was rewritten')
    # Corrupt only the embedded runtime data: valid static preview and source
    # stay accessible, and neither HTML nor PDF can quietly downgrade.
    broken=page.evaluate('''() => {const copy=document.documentElement.cloneNode(true);
      const data=copy.querySelector('body > script#fmd-native-runtime[type="application/json"]');
      const model=JSON.parse(data.textContent);model.wasm='AAAA';data.textContent=JSON.stringify(model).replace(/</g,'\\\\u003c');
      return '<!DOCTYPE html>\\n'+copy.outerHTML;}''').encode()
    failed=load(broken)
    failed.wait_for_function('window.__fmdNativeRuntime !== undefined')
    check(not failed.evaluate('window.__fmdNativeRuntime.ready'),'corrupt runtime unexpectedly initialized')
    check('Native runtime failed' in failed.locator('#fmd-save-status').inner_text(),'startup failure was hidden')
    downloads=[];failed.on('download',lambda item:downloads.append(item))
    failed.locator('#btn-export-pdf').click();failed.locator('#btn-save-html').click()
    failed.wait_for_timeout(100)
    check(not downloads,'failed engine exported a misleading artifact')
    raw,_=download(failed,'btn-save-markdown')
    check(raw==source(failed).encode(),'failed engine prevented source recovery')
    check(failed.locator('#fmd-content iframe').count()==1,'failure discarded last preview')
    check(not requests,f'network requests: {requests}')
    check(not errors,'uncaught runtime errors')
    browser.close()
print(json.dumps({'proof':'browser-ABI-adapter-with-real-tiny-WASM','checks_passed':checks,'navigation_mode':args.mode,
    'network_requests':requests,'runtime_errors':errors,'retained_artifacts':str(retained)}))
