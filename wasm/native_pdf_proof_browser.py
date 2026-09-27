"""PDF proof authoring acceptance: production controller/exporter/runtime and
real browser Workers; explicit shell/native ABI adapters, not compiled Rust.
Run: CHROMIUM=/usr/bin/chromium python wasm/native_pdf_proof_browser.py
"""
from pathlib import Path
import json
import os
import subprocess
import time
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'validation' / 'pdf-proof-browser'
OUT.mkdir(parents=True, exist_ok=True)
subprocess.run(['node', str(ROOT / 'wasm/native_pdf_proof_fixture.mjs'), str(OUT / 'workspace.html')], check=True)
HTML = (OUT / 'workspace.html').read_text()
SOURCE = '\ufeff# Proof\r\n\r\nText 😀\r'
checks, errors, requests, violations = [], [], [], []

def check(condition, name):
    if not condition:
        raise AssertionError(name)
    checks.append(name)

def wait(page, expression, seconds=10):
    # CDP evaluate, not wait_for_function's page eval under the production CSP.
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if page.evaluate(expression):
            return
        time.sleep(.025)
    raise AssertionError('Timed out: ' + expression + '\n' + page.locator('body').inner_text()[-1500:])

def click(page, selector):
    page.locator(selector).click(timeout=5000)

def count_exports(page):
    return page.evaluate("__proofTest.requests.filter(x=>x.type==='export'&&x.format==='pdf').length")

def open_proof(page):
    click(page, '#btn-pdf-proof')

def generate(page):
    click(page, '#btn-generate-pdf-proof')
    wait(page, "document.querySelector('#fmd-proof-status').textContent.startsWith('PDF generated:')")

def download(page, selector, name):
    with page.expect_download(timeout=10000) as event:
        click(page, selector)
    event.value.save_as(str(OUT / name))
    return (OUT / name).read_bytes()

def receipt(data):
    return json.loads(data.split(b'%FMD_RECEIPT ', 1)[1])

def source(page, text, event=True):
    page.evaluate("([text,event])=>{const e=document.querySelector('#fmd-editor');e.value=text;if(event)e.dispatchEvent(new Event('input',{bubbles:true}));}", [text, event])

def viewer_url(page):
    return page.locator('#fmd-proof-viewer').get_attribute('data')

def pdf_blobs(page):
    return page.evaluate("[...__proofTest.urls.values()].filter(x=>x.type==='application/pdf').length")

with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=os.environ.get('CHROMIUM', '/usr/bin/chromium'),
                                        headless=True, args=['--no-sandbox'])
    context = browser.new_context(accept_downloads=True, viewport={'width':1280, 'height':960})
    context.route('http://**/*', lambda route: (requests.append(route.request.url), route.abort()))
    context.route('https://**/*', lambda route: (requests.append(route.request.url), route.abort()))
    pages = []
    def open_page(text=HTML):
        page = context.new_page()
        page.set_default_timeout(5000)
        page.on('pageerror', lambda error: errors.append(str(error)))
        # about:blank also exercises portable/opaque-origin Blob worker startup.
        page.set_content(text, wait_until='domcontentloaded')
        wait(page, "window.__fmdNativeRuntime?.settings && document.querySelector('#btn-pdf-proof') && !document.querySelector('#btn-pdf-proof').disabled")
        pages.append(page)
        return page

    page = open_page()
    check(page.locator('#btn-pdf-proof').count() == 1 and page.locator('#fmd-pdf-proof').count() == 1,
          'fresh single proof control installs after parsing')
    open_proof(page)
    check(count_exports(page) == 0 and page.locator('#btn-download-pdf-proof').is_disabled(),
          'opening the panel does not render or offer nonexistent output')
    generate(page)
    first_url = viewer_url(page)
    proof_bytes = page.evaluate("async url=>Array.from(new Uint8Array(await __proofTest.urls.get(url).arrayBuffer()))", first_url)
    first = download(page, '#btn-download-pdf-proof', 'initial.pdf')
    r = receipt(first)
    check(first == bytes(proof_bytes) and count_exports(page) == 1,
          'viewer and download receive the exact same native bytes without a second render')
    check(r['source'] == SOURCE and r['font'] == 'serif' and r['scale'] == 1.25 and r['epoch'] == 0
          and r['weights'] == [555,0,0,0,0] and r['fonts'] == [[4,5,6],[],[],[],[]]
          and r['images'] == [1,2,3] and r['destinations'] == ['chart.png'] and r['page'] == [792,612,24,36,48,60]
          and r['toc'] and r['tocDepth'] == 4 and r['pageNumbers'] and r['lineNumbers'] and r['lang'] == 'fr',
          'full controller supplies exact BOM/CRLF source and committed PDF resource/typography/paper ABI')
    check(page.locator('#fmd-proof-diagnostics img').count() == 0
          and '<img src=x' in page.locator('#fmd-proof-diagnostics').inner_text(), 'PDF diagnostics remain literal text')
    second = download(page, '#btn-download-pdf-proof', 'repeat.pdf')
    generate(page)
    check(first == second and count_exports(page) == 1 and first_url in page.evaluate('__proofTest.revoked'),
          'repeat proof reuses immutable bytes and retires previous viewer URL')
    check(page.evaluate('__fmdNativeRuntime.diagnostics.length') == 0,
          'PDF diagnostics do not replace live HTML preview diagnostics')
    page.screenshot(path=str(OUT / 'proof-viewer.png'))
    # Save while the dialog is open: actual controller keyboard shortcut, not a
    # test serializer. A saved workspace must carry neither viewers nor URLs.
    with page.expect_download(timeout=10000) as event:
        page.keyboard.press('Control+Shift+S')
    event.value.save_as(str(OUT / 'saved.html'))
    saved = (OUT / 'saved.html').read_text()
    check('blob:null/' not in saved and '<dialog id="fmd-pdf-proof"' not in saved
          and '<object' not in saved and 'id="btn-pdf-proof"' not in saved,
          'Save HTML strips transient proof controls, viewer URLs and rendered bytes')
    reopened = open_page(saved)
    open_proof(reopened); generate(reopened)
    saved_pdf = download(reopened, '#btn-download-pdf-proof', 'reopened.pdf')
    check(saved_pdf == first and reopened.locator('#fmd-pdf-proof').count() == 1,
          'Save HTML/reopen retains exact source/resources and installs fresh functional proof controls')
    page.bring_to_front()
    current_url = viewer_url(page)
    click(page, '#btn-close-pdf-proof')
    check(page.locator('#fmd-proof-viewer').count() == 0 and current_url in page.evaluate('__proofTest.revoked'),
          'closing removes the viewer and revokes its Blob URL')

    # Changes made before the 150 ms preview debounce still enter the PDF now.
    source(page, 'Latest edit 😀\n')
    open_proof(page); generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'latest.pdf'))['source'] == 'Latest edit 😀\n',
          'proof uses newest editor source without waiting for HTML preview debounce')
    old_url = viewer_url(page)
    source(page, 'Silent change', event=False)
    click(page, '#btn-download-pdf-proof')
    check(page.locator('#btn-download-pdf-proof').is_disabled() and page.locator('#fmd-proof-viewer').count() == 0
          and old_url in page.evaluate('__proofTest.revoked'), 'silent source mutation blocks stale downloads and releases stale viewer')

    source(page, 'PDF_SLOW pending')
    before = count_exports(page)
    click(page, '#btn-generate-pdf-proof')
    wait(page, "__fmdNativeRuntime.exportPending")
    wait(page, f"__proofTest.requests.filter(x=>x.type==='export'&&x.format==='pdf').length>{before}")
    click(page, '#btn-cancel-pdf-proof')
    wait(page, '!__fmdNativeRuntime.exportPending')
    check(page.locator('#fmd-pdf-proof').get_attribute('aria-busy') == 'false'
          and page.locator('#btn-download-pdf-proof').is_disabled(), 'cancel stops an in-flight real PDF worker without publishing')
    source(page, 'Replacement after cancel')
    generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'after-cancel.pdf'))['source'] == 'Replacement after cancel',
          'explicit retry after cancellation starts a fresh worker for the current source')
    source(page, 'PDF_FAIL')
    click(page, '#btn-generate-pdf-proof')
    wait(page, "document.querySelector('#fmd-proof-status').textContent.includes('fixture PDF failure')")
    failed_count = count_exports(page)
    time.sleep(.2)
    check(count_exports(page) == failed_count and page.locator('#btn-download-pdf-proof').is_disabled(),
          'native failure is visible with no automatic retry, fallback or stale download')

    source(page, 'Resource proof')
    generate(page)
    page.evaluate("__fmdNativeRuntime.stageImages([{destination:'added.png',bytes:Uint8Array.of(9)}]).commit()")
    wait(page, "document.querySelector('#btn-download-pdf-proof').disabled && !document.querySelector('#fmd-proof-viewer')")
    generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'images.pdf'))['images'] == [1,2,3,9],
          'committed image changes retire proofs and reach the next native export')
    page.evaluate("__fmdNativeRuntime.applySettings({fontScale:1.75}, 'Resource proof', document.querySelector('#fmd-content'), {scale:1})")
    wait(page, "document.querySelector('#btn-download-pdf-proof').disabled")
    generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'settings.pdf'))['scale'] == 1.75,
          'committed settings changes retire proofs and reach the next native export')
    page.evaluate("async ()=>{await __fmdNativeRuntime.applyFontsAsync([{slot:'body-regular',bytes:Uint8Array.of(7,8),weight:450}], 'Resource proof',document.querySelector('#fmd-content'),{scale:1});}")
    wait(page, "document.querySelector('#btn-download-pdf-proof').disabled")
    generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'fonts.pdf'))['fonts'][0] == [7,8],
          'real worker font transaction retires proofs and propagates committed font bytes')

    page.evaluate("document.querySelector('#fmd-editor').dispatchEvent(new CompositionEvent('compositionstart'))")
    check(page.locator('#btn-generate-pdf-proof').is_disabled() and page.locator('#fmd-proof-viewer').count() == 0,
          'IME composition pauses proof admission and releases stale viewing state')
    page.evaluate("document.querySelector('#fmd-editor').dispatchEvent(new CompositionEvent('compositionend'))")
    generate(page)
    old_url = viewer_url(page)
    page.evaluate("window.dispatchEvent(new Event('pagehide'))")
    check(page.locator('#fmd-proof-viewer').count() == 0 and old_url in page.evaluate('__proofTest.revoked')
          and page.locator('#btn-generate-pdf-proof').is_disabled(), 'page suspension releases PDF memory and disables admission')
    count = count_exports(page)
    page.evaluate("window.dispatchEvent(new Event('pageshow'))")
    check(count_exports(page) == count and not page.locator('#btn-generate-pdf-proof').is_disabled(),
          'page restoration permits explicit retry without silently regenerating a PDF')

    # Capability fallback still downloads the actual worker bytes.
    page.evaluate("Object.defineProperty(navigator,'pdfViewerEnabled',{value:false,configurable:true})")
    generate(page)
    fallback = download(page, '#btn-download-pdf-proof', 'fallback.pdf')
    check(page.locator('#fmd-proof-viewer').count() == 0 and receipt(fallback)['source'] == 'Resource proof',
          'browsers without an embedded viewer retain exact native PDF download')
    click(page, '#btn-close-pdf-proof')

    # Real file import uses the production lossless source anchor, including Undo.
    imported = '\ufeff# Imported\r\n\r\nnew text\r'
    page.on('dialog', lambda dialog: dialog.accept())
    with page.expect_file_chooser() as event:
        click(page, '#btn-open-markdown')
    event.value.set_files({'name':'import.md','mimeType':'text/markdown','buffer':imported.encode()})
    wait(page, "document.querySelector('#fmd-save-status').textContent.includes('Markdown source opened')")
    open_proof(page); generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'imported.pdf'))['source'] == imported,
          'real local-file import preserves BOM/CRLF in proof despite textarea normalization')
    click(page, '#btn-close-pdf-proof'); click(page, '#btn-undo-source-open')
    open_proof(page); generate(page)
    check(receipt(download(page, '#btn-download-pdf-proof', 'undone.pdf'))['source'] == 'Resource proof',
          'production Undo source replacement changes the proof source back exactly')
    page.keyboard.press('Escape')
    check(not page.locator('#fmd-pdf-proof').evaluate('(node)=>node.open'), 'Escape closes proofing and restores ordinary workspace navigation')

    for opened in pages:
        violations += opened.evaluate('__proofTest.violations')
    check(not errors, 'no browser page errors')
    check(not violations, 'production proof/frame/script CSP produces no violations')
    check(not requests, 'no HTTP or HTTPS requests')
    report = {'checks':len(checks), 'passed':checks, 'page_errors':errors, 'csp_violations':violations,
              'network_requests':requests,
              'scope':'Production JS controller/exporter/bootstrap and real Chromium Workers; explicit native ABI and shell adapters. No compiled Rust/WASM PDF or visual viewer certification.'}
    (OUT / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    browser.close()
