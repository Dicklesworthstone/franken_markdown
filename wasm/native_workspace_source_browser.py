#!/usr/bin/env python3
"""Exercise source-file authoring in the shipped standalone controller.

FileReader, file chooser, textarea, dialogs and downloads are real Chromium.
The initial HTML shell and rendering functions are explicit adapters, not Rust
rendering or PDF conformance. Content mode reparses saved bytes in fresh pages;
it is not proof of file-navigation support. No browser/dependencies are installed.
"""
from __future__ import annotations

import argparse
import html
import json
import pathlib
import shutil
import tempfile

from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
ORIGINAL = '\r\n# Original\r\n\r\n![retained](chart.png)\r\n'
IMPORTED = '\ufeff\r\n# Imported é中😀\rline\n\0</script><script>globalThis.sourceRan=true</script>\r\n'


def fixture(native: bool) -> str:
    script = (ROOT / 'src/interactive_controller.js').read_text()
    assert '</script' not in script.lower(), 'Embedded controller must not terminate its script'
    data = json.dumps(ORIGINAL).replace('<', '\\u003c')
    setup = '''
      globalThis.calls=[];
      function parseMarkdownClient(source) { calls.push(['lightweight',source]); return '<p>Lightweight renderer adapter</p>'; }
    '''
    if native:
        setup += '''
          Object.defineProperty(window,'__fmdNativeRuntime',{value:{version:1,diagnostics:[],
            settings:Object.freeze({font:'serif',fontScale:1.25,title:'Existing metadata',toc:true}),
            render(source,preview,display){if(globalThis.failSourcePreview)throw Error('Native preview unavailable');calls.push(['html',source,display]); const pre=document.createElement('pre');pre.textContent=source;preview.replaceChildren(pre);},
            pdf(source){calls.push(['pdf',source]);return new TextEncoder().encode('%PDF-ADAPTER\\n'+JSON.stringify({source,settings:this.settings,images:['chart.png']}));}
          }});
        '''
    buttons = ['toggle-view', 'zoom-out', 'zoom-reset', 'zoom-in', 'theme-toggle', 'stats-toggle', 'save-markdown', 'save-html', 'export-pdf']
    return ('<!DOCTYPE html><html><head><meta charset="utf-8"><title>Source fixture</title>'
            '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'unsafe-inline\'; style-src \'unsafe-inline\'; img-src data:; base-uri \'none\'; form-action \'none\'">'
            '<style>body{font-family:system-ui;margin:16px}button{padding:8px}.fmd-toolbar{display:flex;gap:8px;overflow-x:auto}textarea{width:95%;height:200px}.view-read #editor-pane{display:none}pre{white-space:pre-wrap}</style>'
            '</head><body><header class="fmd-app-header"><div class="fmd-toolbar">'
            + ''.join('<button id="btn-' + name + '">' + name + '</button>' for name in buttons)
            + '<span id="view-mode-icon"></span><span id="view-mode-label"></span></div></header>'
            '<div id="fmd-app-body" class="view-split"><section id="editor-pane"><div class="fmd-pane-header"><span id="source-line-count"></span><span id="fmd-save-status" role="status"></span></div><textarea id="fmd-editor">'
            + html.escape(ORIGINAL) + '</textarea></section><main><div id="fmd-content">Original native adapter preview</div></main></div>'
            '<div id="stats-drawer">' + ''.join('<span id="stat-' + name + '"></span>' for name in ['words', 'chars', 'read-time', 'readability'])
            + '<button id="btn-stats-close">close</button></div><script type="application/json" id="fmd-raw-source">' + data + '</script>'
            '<script type="application/json" id="fmd-image-assets">[["chart.png","data:image/png;base64,AQID"]]</script>'
            '<script>' + setup + '\n' + script + '</script></body></html>')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=['file', 'content'], default='file')
    parser.add_argument('--browser', default=shutil.which('chromium'))
    parser.add_argument('--output', type=pathlib.Path)
    args = parser.parse_args()
    output = args.output or pathlib.Path(tempfile.mkdtemp(prefix='fmd-source-'))
    if args.output:
        output.mkdir(parents=True, exist_ok=False)
    output = output.resolve()
    cases = {'import.md': IMPORTED.encode(), 'second.markdown': b'# Second\r\n', 'empty.txt': b'',
             'bad.md': bytes([0xc3, 0x28]), 'wrong.html': b'<script>bad()</script>'}
    for name, data in cases.items():
        (output / name).write_bytes(data)
    checks, errors, requests, dialogs = [], [], [], []
    def check(condition: bool, message: str) -> None:
        if not condition:
            raise AssertionError(message)
        checks.append(message)

    with sync_playwright() as pw:
        browser = pw.chromium.launch(headless=True, executable_path=args.browser, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1200, 'height': 850})
        context.on('request', lambda req: requests.append(req.url) if req.url.startswith(('http:', 'https:')) else None)
        context.route('http://**/*', lambda route: route.abort())
        context.route('https://**/*', lambda route: route.abort())
        serial = 0
        decisions = []
        def handle_dialog(dialog):
            dialogs.append({'type': dialog.type, 'message': dialog.message})
            check(dialog.type == 'confirm', 'only source-replacement confirmations appear')
            if decisions and not decisions.pop(0):
                dialog.dismiss()
            else:
                dialog.accept()

        def open_page(content: str, name: str):
            path = output / name
            with path.open('x', encoding='utf-8') as stream:
                stream.write(content)
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('dialog', handle_dialog)
            if args.mode == 'file':
                page.goto(path.as_uri())
            else:
                page.set_content(content)
            return page

        def download(page, trigger, suffix='md') -> bytes:
            nonlocal serial
            serial += 1
            with page.expect_download() as pending:
                trigger()
            dest = output / f'download-{serial}.{suffix}'
            pending.value.save_as(dest)
            check(pending.value.failure() is None, f'download {serial} completed')
            return dest.read_bytes()

        def markdown(page):
            return download(page, lambda: page.locator('#btn-save-markdown').click())

        def open_source(page, name, accept=True):
            if name not in ['bad.md', 'wrong.html']:
                decisions.append(accept)
            with page.expect_file_chooser() as chooser:
                page.locator('#btn-open-markdown').click()
            chooser.value.set_files(output / name)
            page.wait_for_function('() => !document.getElementById("btn-open-markdown").disabled')

        def dirty(page):
            return page.evaluate("() => {const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented;}")

        for native in [True, False]:
            mode = 'native' if native else 'lightweight'
            page = open_page(fixture(native), mode + '.html')
            check(page.locator('#btn-open-markdown').count() == 1, mode + ': one source opener')
            check(not dirty(page), mode + ': initial workspace is clean')
            check(markdown(page) == ORIGINAL.encode(), mode + ': initial exact bytes download')
            page.evaluate('window.originalEditor=document.getElementById("fmd-editor")')
            page.locator('#btn-toggle-view').click()
            open_source(page, 'import.md')
            check(page.locator('#fmd-editor').is_visible(), mode + ': opening returns to editing view')
            check(page.evaluate('originalEditor===document.getElementById("fmd-editor")'), mode + ': source replacement retains the editor element')
            check(page.locator('#fmd-editor').input_value() == IMPORTED.replace('\r\n', '\n').replace('\r', '\n'), mode + ': textarea displays normalized editing view')
            check(markdown(page) == IMPORTED.encode(), mode + ': BOM, mixed newlines, NUL and Unicode round-trip exactly')
            check(dirty(page), mode + ': imported source remains unsaved')
            check(page.evaluate('!globalThis.sourceRan'), mode + ': imported script syntax remains data')
            check(page.locator('#btn-undo-source-open').is_enabled(), mode + ': one-step source recovery is available')
            if native:
                pdf = download(page, lambda: page.locator('#btn-export-pdf').click(), 'adapter.pdf')
                body = json.loads(pdf.split(b'\n', 1)[1])
                check(body['source'] == IMPORTED and body['settings']['fontScale'] == 1.25 and body['images'] == ['chart.png'], 'native PDF adapter gets exact imported source and retained configuration')
            open_source(page, 'second.markdown', accept=False)
            check(markdown(page) == IMPORTED.encode(), mode + ': declining replacement keeps current source')
            page.locator('#btn-undo-source-open').click()
            check(markdown(page) == ORIGINAL.encode() and not dirty(page), mode + ': Undo restores initial exact bytes and clean state')
            open_source(page, 'empty.txt')
            check(markdown(page) == b'', mode + ': empty files are intentional source replacements')
            page.locator('#btn-undo-source-open').click()
            open_source(page, 'bad.md')
            check('Source not opened' in page.locator('#fmd-save-status').inner_text(), mode + ': corrupt UTF-8 is refused visibly')
            check(markdown(page) == ORIGINAL.encode(), mode + ': corrupt-file failure preserves source')
            open_source(page, 'wrong.html')
            check('Source not opened' in page.locator('#fmd-save-status').inner_text(), mode + ': HTML files are not executed or accepted as Markdown')
            open_source(page, 'import.md')
            page.locator('#fmd-editor').fill('new typing')
            check(page.locator('#btn-undo-source-open').is_disabled(), mode + ': typing retires replacement undo')
            page.locator('#fmd-editor').fill(IMPORTED)
            check(markdown(page) == IMPORTED.encode(), mode + ': returning to the imported view recovers its original encoding')
            saved = download(page, lambda: page.locator('#btn-save-html').click(), 'html').decode()
            reopened = open_page(saved, mode + '-reopened.html')
            check(reopened.locator('#fmd-source-controls').count() == 1, mode + ': saved controls are not duplicated on reopening')
            check(reopened.locator('#btn-undo-source-open').is_disabled(), mode + ': source history is not serialized')
            check(markdown(reopened) == IMPORTED.encode() and not dirty(reopened), mode + ': Save HTML preserves exact imported source as the new clean baseline')
            again = open_page(download(reopened, lambda: reopened.locator('#btn-save-html').click(), 'html').decode(), mode + '-reopened-again.html')
            check(markdown(again) == IMPORTED.encode() and again.locator('#fmd-source-controls').count() == 1, mode + ': repeated save/reopen remains lossless with one toolbar')
            if native:
                again.evaluate('globalThis.failSourcePreview=true')
                open_source(again, 'second.markdown')
                again.wait_for_function('() => document.getElementById("fmd-save-status").textContent.includes("Native preview unavailable")')
                check(markdown(again) == cases['second.markdown'], 'renderer failure does not prevent opening or recovering source')
                again.evaluate('globalThis.failSourcePreview=false')
            open_source(again, 'empty.txt')
            again.evaluate("window.dispatchEvent(new Event('pagehide'));window.dispatchEvent(new Event('pageshow'))")
            check(again.locator('#btn-open-markdown').is_enabled() and again.locator('#btn-undo-source-open').is_disabled(), mode + ': simulated suspension clears history and re-enables new file selection')
            again.screenshot(path=str(output / (mode + '-source-controls.png')), full_page=True)
        check(not decisions, 'all expected confirmation decisions were consumed')
        check(not requests, 'no HTTP(S) requests')
        check(not errors, 'no uncaught browser errors')
        report = {'proof': 'production-controller-with-shell-and-renderer-adapters', 'mode': args.mode,
                  'browser': browser.version, 'assertions': len(checks), 'checks': checks,
                  'network_requests': requests, 'page_errors': errors, 'dialogs': dialogs,
                  'rust_rendering': False, 'pdf_layout': False, 'real_bfcache': False, 'artifacts': str(output)}
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
        browser.close()


if __name__ == '__main__':
    main()
