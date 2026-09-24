#!/usr/bin/env python3
"""Production export worker/runtime/controller in real Chromium.

Explicit native-renderer/shell adapters and an empty real WASM module: this is
browser integration evidence, NOT Rust layout or valid PDF evidence. The test
fixture shortens the configurable worker deadline to 1.4 seconds. Content-mode
reopening reparses downloaded bytes, not file-navigation/bfcache acceptance.
Requires existing Node, Playwright and Chromium; downloads no dependencies.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--browser', default=shutil.which('chromium'))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--mode', choices=['content', 'file'], default='file')
    args = parser.parse_args()
    output = args.output or Path(tempfile.mkdtemp(prefix='fmd-export-browser-'))
    if args.output:
        output.mkdir(parents=True, exist_ok=False)
    output = output.resolve()
    fixtures = json.loads(subprocess.check_output(['node', '--input-type=module', '-e',
        "import {fixture,source,image} from './wasm/native_workspace_exports_fixture.mjs';console.log(JSON.stringify({normal:fixture(),unavailable:fixture({noWorker:true}),legacy:fixture({legacy:true}),source,image}));"], cwd=ROOT, text=True))
    checks, errors, requests = [], [], []
    def check(ok, label):
        assert ok, f'{label}; runtime errors={errors}'
        checks.append(label)
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=args.browser, headless=True, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width':1400,'height':900})
        context.set_default_timeout(5000)
        context.on('request', lambda req: requests.append(req.url) if req.url.startswith(('http:', 'https:')) else None)
        context.route('http://**/*', lambda route: route.abort())
        context.route('https://**/*', lambda route: route.abort())
        pages = 0
        def load(html):
            nonlocal pages
            pages += 1
            path = output / f'workspace-{pages}.html'
            path.write_text(html, encoding='utf-8')
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('dialog', lambda dialog: dialog.accept())
            if args.mode == 'file': page.goto(path.as_uri())
            else: page.set_content(html)
            ready = page.evaluate('window.__fmdNativeRuntime?.ready')
            check(ready is True, f'workspace {pages} initializes real WASM')
            return page
        def until(page, expression, timeout=5):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if page.evaluate(expression): return
                page.wait_for_timeout(25)
            raise AssertionError(f'Timed out: {expression}; status={page.locator("#fmd-export-status").all_text_contents()}; errors={errors}')
        def edit(page, source):
            page.locator('#fmd-editor').fill(source)
        def export(page, button='#btn-export-pdf', trigger=None):
            page.bring_to_front()
            with page.expect_download() as pending:
                if trigger: trigger()
                else: page.locator(button).click()
            downloaded = pending.value
            check(downloaded.failure() is None, 'download completed')
            data = Path(downloaded.path()).read_bytes()
            path = output / f'download-{len(list(output.glob("download-*")))}-{downloaded.suggested_filename}'
            path.write_bytes(data)
            return data, downloaded.suggested_filename
        def decode_pdf(data):
            check(data.startswith(b'%PDF-ADAPTER\n'), 'PDF output is labeled adapter evidence')
            return json.loads(data.split(b'\n', 1)[1])
        def busy(page, text='SLOW current source'):
            page.bring_to_front()
            edit(page, text)
            page.locator('#btn-export-pdf').click()
            until(page, 'window.__fmdNativeRuntime.exportPending')
        page = load(fixtures['normal'])
        until(page, '!!document.querySelector("#fmd-content iframe")?.contentDocument?.querySelector("#source")')
        check(not page.locator('#btn-cancel-export').is_visible(), 'Cancel stays hidden despite author button display CSS')
        data, name = export(page)
        pdf = decode_pdf(data)
        check(pdf['worker'] is True and pdf['source'] == fixtures['source'], 'PDF uses a worker and preserves exact BOM/newline source')
        check(pdf['page'] == [792,612,24,36,48,60] and pdf['epoch'] == 0, 'paper geometry and deterministic epoch reach PDF')
        check(pdf['fonts'] == [1,2,3] and pdf['weights'][0] == 550 and pdf['images'] == [['chart.png',fixtures['image']]], 'font pins and exact image bytes survive worker transfer')
        check(pdf['toc'] and pdf['depth'] == 4 and pdf['codeLineNumbers'] and pdf['pageNumbers'], 'PDF navigation and numbering settings are retained')
        check(name == 'Publication.pdf', 'PDF uses the expected download name')
        published, name = export(page, '#btn-publish-html')
        check(name == 'Publication.published.html', 'published HTML is distinct from an editable workspace')
        text = published.decode()
        check('Content-Security-Policy' in text and 'default-src' in text, 'publication retains the renderer content policy')
        check('fmd-native-runtime' not in text and 'iframe' not in text and '<textarea' not in text, 'publication does not ship the editor or runtime')
        pubpage = context.new_page(); pubpage.set_content(text)
        abi = json.loads(pubpage.locator('#abi').inner_text())
        check(abi['worker'] and abi['scale'] == 1.25, 'published HTML uses worker-owned committed typography')
        until(pubpage, 'document.querySelector("img").complete && document.querySelector("img").naturalWidth===2')
        check(True, 'transferred publication images actually decode in Chromium')
        page.locator('#btn-zoom-in').click(); page.locator('#btn-theme-toggle').click()
        pdf = decode_pdf(export(page)[0])
        check(pdf['scale'] == 1.25 and pdf['mode'] == 'disabled', 'view zoom/theme do not alter PDF settings')
        immediate = export(page, trigger=lambda: page.evaluate("() => {const e=document.getElementById('fmd-editor');e.value='Immediate export';e.dispatchEvent(new Event('input'));document.getElementById('btn-export-pdf').click();}"))[0]
        check(decode_pdf(immediate)['source'] == 'Immediate export', 'export captures current source before preview debounce')

        downloads = []
        page.on('download', lambda d: downloads.append(d))
        page.evaluate('globalThis.exportBeats=0;globalThis.exportBeatTimer=setInterval(()=>exportBeats++,10)')
        busy(page)
        check(page.locator('#btn-cancel-export').is_visible() and page.locator('#btn-export-pdf').is_disabled(), 'busy state offers Cancel and prevents duplicate button operations')
        page.evaluate("() => {for(let i=0;i<8;i++)document.dispatchEvent(new KeyboardEvent('keydown',{key:'p',ctrlKey:true}));}")
        page.locator('#btn-zoom-in').click()
        until(page, '!window.__fmdNativeRuntime.exportPending')
        page.wait_for_timeout(150)
        check(len(downloads) == 1, f'repeated keyboard requests produce only one download (observed {len(downloads)})')
        check(page.evaluate('exportBeats') > 10, 'editor event loop keeps executing during actual busy worker computation')
        check('download started' in page.locator('#fmd-export-status').inner_text(), 'export completion has its own persistent status')

        before = len(downloads)
        busy(page)
        page.locator('#btn-cancel-export').click()
        check(not page.evaluate('window.__fmdNativeRuntime.exportPending'), 'Cancel immediately releases the active export')
        page.wait_for_timeout(800)
        check(len(downloads) == before and 'cancelled' in page.locator('#fmd-export-status').inner_text(), 'cancelled worker never downloads late output')
        check(not page.locator('#btn-export-pdf').is_disabled(), 'Cancel restores export controls for retry')

        busy(page)
        edit(page, 'New revision')
        next_pdf = decode_pdf(export(page)[0])
        page.wait_for_timeout(800)
        check(next_pdf['source'] == 'New revision' and len(downloads) == before + 1, 'new source cancels old work and a fast retry cannot receive stale output')
        before = len(downloads)
        busy(page)
        page.evaluate("document.getElementById('fmd-editor').value='Undispatched edit'")
        until(page, '!window.__fmdNativeRuntime.exportPending')
        page.wait_for_timeout(100)
        check(len(downloads) == before and 'document changed' in page.locator('#fmd-export-status').inner_text(), 'final revision check catches source edits without input events')
        busy(page)
        page.evaluate("() => {const e=document.getElementById('fmd-editor');e.value='Changed';e.dispatchEvent(new Event('input'));e.value='SLOW current source';e.dispatchEvent(new Event('input'));}")
        page.wait_for_timeout(800)
        check(len(downloads) == before, 'edit/undo round trips cannot resurrect a cancelled export')
        busy(page)
        page.locator('#fmd-editor').dispatch_event('compositionstart')
        page.evaluate("document.dispatchEvent(new KeyboardEvent('keydown',{key:'p',ctrlKey:true,isComposing:true}))")
        check(not page.evaluate('window.__fmdNativeRuntime.exportPending'), 'composition cancels exports and does not launch another from the keyboard')
        page.locator('#fmd-editor').dispatch_event('compositionend')

        busy(page)
        page.evaluate("() => {globalThis.imageTransaction=window.__fmdNativeRuntime.stageImages([{destination:'extra.png',bytes:new Uint8Array([9,8])}]);imageTransaction.commit();}")
        until(page, '!window.__fmdNativeRuntime.exportPending')
        page.wait_for_timeout(50)
        check(len(downloads) == before, 'committed resources cancel the old export snapshot')
        edit(page, 'Resources updated')
        updated = decode_pdf(export(page)[0])
        check(updated['images'][-1] == ['extra.png','CQg='], 'retry exports newly committed native resources')
        page.evaluate('imageTransaction.rollback()')
        busy(page)
        page.evaluate("() => window.__fmdNativeRuntime.applySettings({fontScale:1.75},'Settings preflight',document.getElementById('fmd-content'),{})")
        until(page, '!window.__fmdNativeRuntime.exportPending')
        edit(page, 'Settings updated')
        check(decode_pdf(export(page)[0])['scale'] == 1.75, 'settings cancellation and retry preserve committed typography')

        before = len(downloads)
        busy(page)
        page.evaluate("window.dispatchEvent(new Event('pagehide'));window.dispatchEvent(new Event('pageshow'))")
        page.wait_for_timeout(800)
        check(len(downloads) == before and not page.evaluate('window.__fmdNativeRuntime.exportPending'), 'suspension cancels work without automatically restarting exports')
        busy(page, 'HANG PDF only')
        beats = page.evaluate('exportBeats')
        until(page, "document.getElementById('fmd-export-status').textContent.includes('timed out')")
        check(page.evaluate('exportBeats') > beats + 20, 'a real stuck export worker is terminated on deadline while editor remains live')
        check(not page.locator('#btn-export-pdf').is_disabled(), 'timeout releases controls without a main-thread fallback')
        edit(page, 'FAIL PDF')
        page.locator('#btn-export-pdf').click()
        until(page, "document.getElementById('fmd-export-status').textContent.includes('Deliberate renderer refusal')")
        check(len(downloads) == before, 'native export errors produce no stale downloads')
        edit(page, 'Recovered after failure')
        check(decode_pdf(export(page)[0])['source'] == 'Recovered after failure', 'an explicit retry works after a terminal worker failure')

        busy(page)
        source_copy = export(page, '#btn-save-markdown')[0]
        check(source_copy == b'SLOW current source', 'Markdown source stays downloadable during PDF computation')
        workspace = export(page, '#btn-save-html')[0].decode()
        check('id="fmd-export-controls"' not in workspace, 'saving omits transient export controls and pending operation state')
        reopened = load(workspace)
        until(reopened, '!!document.querySelector("#fmd-content iframe")')
        check(reopened.locator('#fmd-export-controls').count() == 1 and not reopened.locator('#btn-cancel-export').is_visible(), 'reopening recreates one idle export control group')
        check(not reopened.locator('#btn-export-pdf').is_disabled(), 'saving during export does not persist disabled PDF controls')
        edit(reopened, 'Second generation')
        reopened_pdf = decode_pdf(export(reopened)[0])
        check(reopened_pdf['scale'] == 1.75 and reopened_pdf['images'] == [['chart.png',fixtures['image']]], 'saved workspace preserves committed settings/resources for asynchronous exports')
        reopened2 = load(export(reopened, '#btn-save-html')[0].decode())
        check(reopened2.locator('#fmd-export-controls').count() == 1, 'second save/reopen still has one control group')
        edit(reopened2, '<script>globalThis.injected=true</script>')
        pubbytes = export(reopened2, '#btn-publish-html')[0]
        isolated = context.new_page(); isolated.set_content(pubbytes.decode())
        check(isolated.evaluate('globalThis.injected===undefined'), 'asynchronous publication source remains inert')

        unavailable = load(fixtures['unavailable'])
        unavailable.locator('#btn-export-pdf').click()
        until(unavailable, "document.getElementById('fmd-export-status').textContent.includes('Workers are unavailable')")
        check(export(unavailable, '#btn-save-markdown')[0] == fixtures['source'].encode(), 'missing Worker support never prevents source recovery')
        check(not unavailable.locator('#btn-cancel-export').is_visible(), 'worker startup failure releases busy controls')
        legacy = load(fixtures['legacy'])
        check(legacy.locator('#fmd-export-controls').count() == 0, 'legacy synchronous workspaces do not advertise cancellation they cannot provide')
        check(not requests, 'no HTTP(S) requests')
        check(not errors, 'no uncaught browser errors')
        report={'assertions':len(checks),'checks':checks,'browser':browser.version,'mode':args.mode,
                'network_requests':requests,'page_errors':errors,'proof':'production runtime/controller/real workers; explicit shell/native adapters and empty WASM',
                'rust_rendering':False,'pdf_layout':False,'bfcache':False,'deadline_ms':1400}
        (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2))
        browser.close()

if __name__ == '__main__': main()
