#!/usr/bin/env python3
"""Retained browser proof for the shipped native settings/controller code.

Uses explicit renderer/shell adapters and a real empty WASM module, NOT Rust
rendering or valid PDF layout. --mode content reparses downloaded HTML in fresh
pages; it is not evidence of file:// navigation compatibility. No server or
network asset is required. Requires existing Node, Playwright and Chromium.
"""
from __future__ import annotations

import argparse
import html
import json
import pathlib
import shutil
import subprocess
import tempfile

from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = '\r\n# Settings\r\n\r\n![Chart](chart.png)\r\n\0</script><script>globalThis.sourceRan=true</script> é中😀\r\n'
PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6SAAAAABJRU5ErkJggg=='


def inert(value: object) -> str:
    return json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')


def fixture(functions: dict[str, str], *, page_capable: bool = True, fail_start: bool = False, native: bool = True) -> str:
    controller = (ROOT / 'src/interactive_controller.js').read_text()
    bindings = r'''
export default async function init({module_or_path}) {
  await WebAssembly.instantiate(module_or_path);
  await new Promise(resolve => { globalThis.releaseSettingsFixture = resolve; });
  STARTUP_FAILURE
}
const esc = text => String(text).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function result(text,mimeType) { const bytes = new TextEncoder().encode(text); return {bytes,mimeType,diagnosticsJson:()=> '[]',free(){bytes.fill(0);}}; }
function images(keys,bytes,lengths) { let offset=0; return keys.map((key,i)=>{ const data=bytes.slice(offset,offset+lengths[i]);offset+=lengths[i]; return [key,btoa(String.fromCharCode(...data))];}); }
export function renderHtmlConfiguredAdvanced(...a) {
  if (globalThis.failNextSettingsRender) { globalThis.failNextSettingsRender=false; throw Error('Injected native render failure'); }
  const abi={font:a[1],mode:a[2],title:a[3],scale:a[6],weights:Array.from(a[12]),lang:a[16],toc:a[17],depth:a[18]};
  return result('<!DOCTYPE html><html><head></head><body><pre id="abi">'+esc(JSON.stringify(abi))+'</pre><pre id="source">'+esc(a[0])+'</pre>'
    +images(a[13],a[14],a[15]).filter(([key])=>a[0].includes(key)).map(([key,data])=>'<img alt="'+esc(key)+'" src="data:image/png;base64,'+data+'">').join('')+'</body></html>','text/html');
}
function pdf(a,page) { return result('%PDF-ADAPTER\n'+JSON.stringify({source:a[0],font:a[1],mode:a[2],title:a[3],author:a[4],epoch:a[5],codeLineNumbers:a[7],images:images(a[8],a[9],a[10]),weights:Array.from(a[16]),pageNumbers:a[20],scale:a[21],lang:a[22],toc:a[23],depth:a[24],page}), 'application/pdf'); }
export function renderPdfConfiguredMulti(...a) { return pdf(a,null); }
PAGE_EXPORT
'''.replace('STARTUP_FAILURE', "throw Error('Injected startup failure');" if fail_start else '')
    bindings = bindings.replace('PAGE_EXPORT', 'export function renderPdfConfiguredPage(...a) { return pdf(a,Array.from(a.pop())); }' if page_capable else '')
    payload = {'version': 1, 'wasm': 'AGFzbQEAAAA=', 'bindings': bindings,
               'options': {'font': 'serif', 'fontScale': 1.25, 'darkMode': 'auto', 'title': 'Original',
                           'author': 'Author', 'lang': 'fr', 'metadataEpochSeconds': 0,
                           'pageNumbers': False, 'codeLineNumbers': False, 'toc': False},
               'images': [{'destination': 'chart.png', 'bytes': PNG}],
               'fonts': [{'slot': 'body-regular', 'bytes': 'AQID', 'weight': 555}]}
    buttons = ['toggle-view', 'zoom-out', 'zoom-reset', 'zoom-in', 'theme-toggle', 'stats-toggle', 'save-markdown', 'save-html', 'export-pdf']
    policy = "default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; frame-src 'self' about:; base-uri 'none'; form-action 'none'"
    runtime = '' if not native else ('<script id="fmd-native-runtime" type="application/json">' + inert(payload) + '</script>'
                                   '<script>;(' + functions['boot'] + ')(' + functions['factory'] + ');</script>')
    return ('<!DOCTYPE html><html lang="fr"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="' + policy + '">'
            '<title>Original</title><style>body{font-family:system-ui;margin:16px}button{padding:8px}textarea{width:95%;height:110px}.view-read #editor-pane{display:none}</style></head><body>'
            '<header class="fmd-app-header"><span class="fmd-title">Original</span><div class="fmd-toolbar">'
            + ''.join('<button id="btn-' + name + '">' + name + '</button>' for name in buttons)
            + '<span id="view-mode-icon"></span><span id="view-mode-label"></span></div></header>'
            '<div class="view-split" id="fmd-app-body"><section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status" role="status"></span></div><textarea id="fmd-editor">'
            + html.escape(SOURCE) + '</textarea></section><main id="preview-pane"><div id="fmd-content">Initial adapter preview</div></main></div>'
            '<div id="stats-drawer">' + ''.join('<span id="stat-' + name + '"></span>' for name in ['words', 'chars', 'read-time', 'readability'])
            + '<button id="btn-stats-close">close</button></div><script type="application/json" id="fmd-raw-source">' + inert(SOURCE) + '</script>'
            + runtime + '<script>function parseMarkdownClient(s){globalThis.lightweightCalls=(globalThis.lightweightCalls||0)+1;return "<p>Lightweight adapter</p>";}\n'
            + controller + '</script></body></html>')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=['file', 'content'], default='file')
    parser.add_argument('--browser', default=shutil.which('chromium'))
    parser.add_argument('--output', type=pathlib.Path, help='New artifact directory; never overwritten')
    args = parser.parse_args()
    output = args.output or pathlib.Path(tempfile.mkdtemp(prefix='fmd-settings-'))
    if args.output:
        output.mkdir(parents=True, exist_ok=False)
    output = output.resolve()
    functions = json.loads(subprocess.check_output(['node', '--input-type=module', '-e',
        "import {bootNativeWorkspace as boot,createNativeWorkspaceRenderer as factory} from './wasm/interactive_runtime.mjs';console.log(JSON.stringify({boot:boot.toString(),factory:factory.toString()}));"], cwd=ROOT, text=True))
    checks: list[str] = []
    requests: list[str] = []
    errors: list[str] = []
    def check(condition: bool, message: str) -> None:
        if not condition:
            raise AssertionError(message)
        checks.append(message)

    with sync_playwright() as pw:
        browser = pw.chromium.launch(headless=True, executable_path=args.browser, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1100, 'height': 950})
        context.on('request', lambda req: requests.append(req.url) if req.url.startswith(('http:', 'https:')) else None)
        context.route('http://**/*', lambda route: route.abort())
        context.route('https://**/*', lambda route: route.abort())
        serial = 0
        def open_page(content: str, name: str, *, release: bool = True, native: bool = True):
            path = output / name
            with path.open('x', encoding='utf-8') as stream:
                stream.write(content)
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            if args.mode == 'file':
                page.goto(path.as_uri())
            else:
                page.set_content(content)
            if native:
                page.wait_for_function('() => typeof releaseSettingsFixture === "function"')
                if release:
                    page.evaluate('releaseSettingsFixture()')
                    page.wait_for_function('() => window.__fmdNativeRuntime.settings !== null')
            return page

        def download(page, trigger, suffix: str) -> bytes:
            nonlocal serial
            serial += 1
            with page.expect_download() as pending:
                trigger()
            dest = output / f'download-{serial}.{suffix}'
            pending.value.save_as(dest)
            check(pending.value.failure() is None, f'download {serial} completed')
            return dest.read_bytes()

        def dirty(page) -> bool:
            return page.evaluate("() => { const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented; }")

        def settings(page):
            return page.evaluate('window.__fmdNativeRuntime.settings')

        def open_settings(page):
            page.locator('#btn-document-settings').click()
            return page.locator('body > #fmd-document-settings')

        def pdf(page):
            data = download(page, lambda: page.locator('#btn-export-pdf').click(), 'adapter.pdf')
            check(data.startswith(b'%PDF-ADAPTER\n'), 'PDF download is explicitly an ABI adapter')
            return json.loads(data.split(b'\n', 1)[1])

        page = open_page(fixture(functions), 'initial.html', release=False)
        check(page.locator('#btn-document-settings').is_disabled(), 'settings disabled while native runtime initializes')
        page.evaluate('releaseSettingsFixture()')
        page.wait_for_function('() => window.__fmdNativeRuntime.settings !== null')
        check(page.locator('#btn-document-settings').is_enabled(), 'settings enabled after runtime readiness')
        check(not dirty(page), 'untouched workspace does not warn about modifications')
        baseline = settings(page)
        before = page.locator('#fmd-native-runtime').text_content()
        dialog = open_settings(page)
        check(dialog.is_visible() and page.locator('dialog:modal').count() == 1, 'real native dialog opens modally')
        check(dialog.locator('[name=fontScale]').input_value() == '1.25', 'dialog reflects export typography')
        check(dialog.locator('[name=width]').is_disabled(), 'default paper is not silently made explicit')
        dialog.locator('[name=title]').fill('Unapplied')
        dialog.locator('[data-cancel]').click()
        check(settings(page) == baseline and page.locator('#fmd-native-runtime').text_content() == before, 'Cancel preserves settings and saved bytes')
        dialog = open_settings(page)
        check(dialog.locator('[name=title]').input_value() == 'Original', 'reopening discards unapplied form values')
        dialog.locator('[name=fontScale]').fill('2.5')
        page.keyboard.press('Escape')
        check(not dialog.is_visible() and not dirty(page), 'Escape cancels without dirtying source or settings')
        dialog = open_settings(page)
        dialog.locator('[type=submit]').click()
        check(settings(page) == baseline and not dirty(page), 'unchanged Apply preserves omitted TOC depth and default paper')
        dialog = open_settings(page)
        title = 'Revised <script>globalThis.metadataRan=true</script> &'
        dialog.locator('[name=title]').fill(title)
        dialog.locator('[name=author]').fill('New author')
        dialog.locator('[name=lang]').fill('de')
        dialog.locator('[name=font]').select_option('sans')
        dialog.locator('[name=fontScale]').fill('1.75')
        dialog.locator('[name=darkMode]').select_option('disabled')
        dialog.locator('[name=toc]').check()
        dialog.locator('[name=tocDepth]').fill('5')
        dialog.locator('[name=pageNumbers]').check()
        dialog.locator('[name=codeLineNumbers]').check()
        dialog.locator('[name=paper]').select_option('a4')
        dialog.locator('[name=orientation]').select_option('landscape')
        for name, value in zip(['top', 'right', 'bottom', 'left'], [24, 36, 48, 60]):
            dialog.locator(f'[name={name}]').fill(str(value))
        page.screenshot(path=str(output / 'settings-panel.png'), full_page=True)
        dialog.locator('[type=submit]').click()
        geometry = [297 * 72 / 25.4, 210 * 72 / 25.4, 24, 36, 48, 60]
        check(not dialog.is_visible() and settings(page)['pageGeometry'] == geometry, 'A4 landscape and each margin apply exactly')
        check(page.title() == title and page.locator('.fmd-title').inner_text() == title, 'document title is updated as literal text')
        check(page.locator('html').get_attribute('lang') == 'de', 'document language is preserved in its shell')
        check(page.evaluate('!globalThis.sourceRan && !globalThis.metadataRan'), 'source and metadata remain data rather than executable scripts')
        check(dirty(page), 'settings-only edits activate the unload warning')
        frame = page.frame_locator('#fmd-content > iframe')
        abi = json.loads(frame.locator('#abi').inner_text())
        check(abi['font'] == 'sans' and abi['scale'] == 1.75 and abi['toc'] and abi['depth'] == 5, 'native preview receives applied typography and TOC')
        check(frame.locator('img').count() == 1, 'existing authorized image remains in the preview')
        first_pdf = pdf(page)
        check(first_pdf['source'] == SOURCE, 'settings do not normalize untouched source bytes')
        check(first_pdf['page'] == geometry and first_pdf['epoch'] == 0, 'PDF receives exact geometry and deterministic metadata epoch')
        check(first_pdf['title'] == title and first_pdf['author'] == 'New author' and first_pdf['lang'] == 'de', 'PDF receives all edited metadata')
        check(first_pdf['pageNumbers'] and first_pdf['codeLineNumbers'] and first_pdf['toc'] and first_pdf['depth'] == 5, 'PDF receives all navigation and numbering flags')
        check(first_pdf['weights'][0] == 555 and first_pdf['images'] == [['chart.png', PNG]], 'settings do not replace fonts or image bytes')
        page.locator('#btn-zoom-in').click()
        zoom_pdf = pdf(page)
        check(zoom_pdf['scale'] == 1.75 and zoom_pdf['page'] == geometry, 'view zoom does not change PDF scale or paper')
        abi = json.loads(page.frame_locator('#fmd-content > iframe').locator('#abi').inner_text())
        check(abs(abi['scale'] - 1.925) < 1e-12, 'view zoom multiplies the new preview typography')
        immediate = download(page, lambda: page.evaluate("() => {const e=document.getElementById('fmd-editor');e.value='Immediate source';e.dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('btn-export-pdf').click();}"), 'adapter.pdf')
        check(json.loads(immediate.split(b'\n', 1)[1])['source'] == 'Immediate source', 'PDF reads current source before the preview debounce')
        page.locator('#fmd-editor').fill(SOURCE)
        check(dirty(page) and 'Modified' in page.locator('#fmd-save-status').inner_text(), 'restoring source does not clear settings-only modification state')
        markdown = download(page, lambda: page.locator('#btn-save-markdown').click(), 'md')
        check(markdown == SOURCE.encode(), 'Save Markdown retains exact untouched source independently of settings')
        dialog = open_settings(page)
        dialog.locator('[name=fontScale]').fill('2.5')
        saved = download(page, lambda: page.keyboard.press('Control+Shift+S'), 'html').decode()
        check(settings(page)['fontScale'] == 1.75, 'saving an open dialog does not apply pending fields')
        reopened = open_page(saved, 'reopened-1.html')
        check(reopened.locator('#btn-document-settings').count() == 1 and reopened.locator('body > dialog#fmd-document-settings').count() == 1, 'reopening creates no duplicate controls')
        check(not reopened.locator('#fmd-document-settings').is_visible() and not dirty(reopened), 'reopened workspace is clean with its dialog closed')
        check(settings(reopened) == settings(page), 'all committed settings survive Save HTML/reopen')
        check(pdf(reopened)['images'] == [['chart.png', PNG]], 'reopened renderer retains exact image resources')
        reopened2 = open_page(download(reopened, lambda: reopened.locator('#btn-save-html').click(), 'html').decode(), 'reopened-2.html')
        check(settings(reopened2) == settings(reopened) and not dirty(reopened2), 'second save/reopen generation preserves settings and clean baseline')
        check(reopened2.locator('#btn-document-settings').count() == 1, 'second reopening still has one settings button')
        dialog = open_settings(reopened2)
        old_settings = settings(reopened2)
        old_data = reopened2.locator('#fmd-native-runtime').text_content()
        reopened2.evaluate('window.savedSettingsFrame=document.querySelector("#fmd-content > iframe")')
        dialog.locator('[name=width]').fill('144')
        dialog.locator('[name=height]').fill('144')
        for name in ['top', 'right', 'bottom', 'left']:
            dialog.locator(f'[name={name}]').fill('72')
        dialog.locator('[type=submit]').click()
        check(dialog.is_visible() and 'geometry' in dialog.locator('[data-status]').inner_text(), 'invalid content rectangle stays visible as an actionable error')
        check(settings(reopened2) == old_settings and reopened2.locator('#fmd-native-runtime').text_content() == old_data, 'invalid paper leaves committed and saved settings unchanged')
        check(reopened2.evaluate('savedSettingsFrame===document.querySelector("#fmd-content > iframe")'), 'failed Apply preserves the existing preview frame')
        dialog.locator('[data-cancel]').click()
        dialog = open_settings(reopened2)
        dialog.locator('[name=fontScale]').fill('2')
        reopened2.evaluate('globalThis.failNextSettingsRender=true')
        dialog.locator('[type=submit]').click()
        check('Injected native render failure' in dialog.locator('[data-status]').inner_text(), 'native rendering failure is reported without parser fallback')
        check(settings(reopened2) == old_settings and reopened2.locator('#fmd-native-runtime').text_content() == old_data, 'native failure leaves settings and saved bytes intact')
        dialog.locator('[type=submit]').click()
        check(not dialog.is_visible() and settings(reopened2)['fontScale'] == 2, 'retry applies after the renderer recovers')
        dialog = open_settings(reopened2)
        dialog.locator('[name=paper]').select_option('default')
        dialog.locator('[type=submit]').click()
        check('pageGeometry' not in settings(reopened2) and pdf(reopened2)['page'] is None, 'default paper restores legacy PDF dispatch')
        old = open_page(fixture(functions, page_capable=False), 'old-package.html')
        dialog = open_settings(old); dialog.locator('[name=fontScale]').fill('1.5'); dialog.locator('[type=submit]').click()
        check(settings(old)['fontScale'] == 1.5, 'typography editing works with an older non-page-capable package')
        dialog = open_settings(old); dialog.locator('[name=paper]').select_option('a4'); dialog.locator('[type=submit]').click()
        check('matching page-capable WASM' in dialog.locator('[data-status]').inner_text() and 'pageGeometry' not in settings(old), 'old-package paper requests fail explicitly without discarding typography')
        dialog.locator('[data-cancel]').click()
        dialog = open_settings(old); dialog.locator('[name=fontScale]').fill('1.25'); dialog.locator('[type=submit]').click()
        check(not dirty(old), 'restoring saved settings clears a settings-only unload warning')
        dialog = open_settings(old); dialog.locator('[name=fontScale]').fill('1.7')
        old.evaluate("() => window.__fmdNativeRuntime.applySettings({fontScale:1.6},JSON.parse(document.getElementById('fmd-raw-source').textContent),document.getElementById('fmd-content'),{})")
        dialog.locator('[type=submit]').click()
        check('changed while this panel was open' in dialog.locator('[data-status]').inner_text(), 'a stale form cannot overwrite newer applied settings')
        check(settings(old)['fontScale'] == 1.6, 'stale-form rejection preserves the newer settings revision')
        failed = open_page(fixture(functions, fail_start=True), 'startup-failure.html', release=False)
        failed.evaluate('releaseSettingsFixture()')
        failed.wait_for_function('() => document.getElementById("fmd-save-status").textContent.includes("Injected startup failure")')
        check(failed.locator('#btn-document-settings').is_disabled(), 'startup failure leaves settings unavailable instead of falling back')
        check(download(failed, lambda: failed.locator('#btn-save-markdown').click(), 'md') == SOURCE.encode(), 'source remains recoverable after startup failure')
        lightweight = open_page(fixture(functions, native=False), 'lightweight.html', native=False)
        check(lightweight.locator('#btn-document-settings').count() == 0, 'lightweight workspaces do not offer unsupported native settings')
        lightweight.locator('#fmd-editor').fill('changed')
        lightweight.wait_for_function('() => globalThis.lightweightCalls===1')
        check(lightweight.locator('#fmd-content').inner_text() == 'Lightweight adapter', 'lightweight editing still uses its established renderer')
        reopened2.set_viewport_size({'width': 320, 'height': 700})
        dialog = open_settings(reopened2)
        check(dialog.evaluate('(d)=>d.scrollWidth<=d.clientWidth+1'), 'settings remain usable without horizontal overflow at a narrow viewport')
        reopened2.screenshot(path=str(output / 'settings-narrow.png'), full_page=True)
        check(not requests, 'no HTTP(S) requests')
        check(not errors, 'no uncaught browser errors')
        report = {'proof': 'shipped-controller-runtime-with-explicit-renderer-and-shell-adapters', 'browser': browser.version,
                  'mode': args.mode, 'assertions': len(checks), 'checks': checks, 'network_requests': requests,
                  'page_errors': errors, 'rust_rendering': False, 'pdf_layout': False, 'artifacts': str(output)}
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
        browser.close()


if __name__ == '__main__':
    main()
