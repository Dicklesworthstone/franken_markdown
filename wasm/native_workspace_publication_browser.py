#!/usr/bin/env python3
"""Browser integration of production publication code with an explicit ABI double.

Run: python wasm/native_workspace_publication_browser.py --chromium /usr/bin/chromium
Requires an already installed Playwright and Chromium; performs no installation.
The small shell and native output bytes below are fixtures, not Rust rendering,
EPUB conformance, SVG typography, or production package-size evidence.
"""
from __future__ import annotations

import argparse
import json
import tempfile
import time
import xml.etree.ElementTree as ET
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "\ufeff# Report\r\n\r\n![chart](chart.png)\r\n中𝄞\r\n"
BINDINGS = r'''
export default async function init(input) {
  await WebAssembly.instantiate(input.module_or_path);
  await new Promise(resolve => setTimeout(resolve, 100));
}
function escape(text) { return text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;'); }
function render(format, args) {
  if (format !== 'html' && args[0].startsWith('SLOW')) {
    const until = performance.now() + 1500; while (performance.now() < until) { /* blocked native fixture */ }
  }
  const packet = JSON.stringify({format, args}, (_key, value) => ArrayBuffer.isView(value) ? [...value] : value);
  const text = format === 'html' ? '<html><head></head><body><p>ABI fixture: ' + escape(args[0]) + '</p></body></html>'
    : format === 'pdf' ? '%PDF-fixture\n' + packet
    : format === 'epub' ? 'PK\x03\x04' + packet
    : '<svg xmlns="http://www.w3.org/2000/svg"><metadata>' + escape(packet) + '</metadata></svg>';
  return {bytes: new TextEncoder().encode(text), mimeType: {html: 'text/html', pdf: 'application/pdf',
    epub: 'application/epub+zip', svg: 'image/svg+xml'}[format],
    diagnosticsJson: () => '[{"severity":"warning","start":0,"end":0,"scope":"document","code":"abi_fixture","message":"Explicit native ABI double"}]',
    free() {}};
}
export const renderHtmlConfiguredAdvanced = (...args) => render('html', args);
export const renderPdfConfiguredMulti = (...args) => render('pdf', args);
export const renderPdfConfiguredPage = (...args) => render('pdf', args);
export const renderEpubConfiguredAdvanced = (...args) => render('epub', args);
export const renderSvgConfiguredResources = (...args) => render('svg', args);
'''


def fixture(path: Path, *, legacy: bool = False) -> None:
    bindings = BINDINGS
    if legacy:
        bindings = bindings.replace('export const renderEpubConfiguredAdvanced', 'const renderEpubConfiguredAdvanced')
        bindings = bindings.replace('export const renderSvgConfiguredResources', 'const renderSvgConfiguredResources')
    payload = {'version': 1, 'wasm': 'AGFzbQEAAAA=', 'bindings': bindings,
               'options': {'font': 'serif', 'darkMode': 'disabled', 'fontScale': 1.25,
                           'title': 'Fixture', 'lang': 'fr', 'toc': True, 'tocDepth': 3,
                           'pageNumbers': True, 'codeLineNumbers': True, 'metadataEpochSeconds': 0},
               'images': [{'destination': 'chart.png', 'bytes': 'AQID'}],
               'fonts': [{'slot': 'body-regular', 'bytes': 'BAU=', 'weight': 650}]}
    def data(value: object) -> str:
        return json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')
    runtime = (ROOT / 'wasm/interactive_runtime.mjs').read_text().replace('export function ', 'function ')
    worker = (ROOT / 'wasm/interactive_preview.mjs').read_text().replace('export function ', 'function ')
    controller = (ROOT / 'src/interactive_controller.js').read_text()
    ids = ['btn-save-markdown', 'btn-save-html', 'btn-toggle-view', 'btn-zoom-in', 'btn-zoom-out',
           'btn-zoom-reset', 'btn-theme-toggle', 'btn-stats-toggle', 'btn-export-pdf']
    buttons = ''.join(f'<button id="{name}" type="button">{name}</button>' for name in ids)
    path.write_text('''<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Fixture</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; frame-src 'self' about:; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'">
<style>body{font:16px sans-serif;margin:16px}.fmd-app-header{display:flex;flex-wrap:wrap;gap:8px}button{padding:8px}textarea{width:90%;height:140px}#stats-drawer{display:none}</style></head><body>
<header class="fmd-app-header"><span class="fmd-title">Fixture</span>''' + buttons + '''</header>
<div id="fmd-app-body" class="view-split"><div id="editor-pane"><div class="fmd-pane-header"><span id="fmd-save-status"></span></div><textarea id="fmd-editor"></textarea><span id="source-line-count"></span></div><main id="fmd-content"></main></div>
<span id="view-mode-icon"></span><span id="view-mode-label"></span>
<div id="stats-drawer"><span id="stat-words"></span><span id="stat-chars"></span><span id="stat-read-time"></span><span id="stat-readability"></span><button id="btn-stats-close"></button></div>
<script id="fmd-raw-source" type="application/json">''' + data(SOURCE) + '''</script>
<script id="fmd-native-runtime" type="application/json">''' + data(payload) + '''</script>
<script>''' + runtime + '\n' + worker + '''
bootNativeWorkspace(createNativeWorkspaceRenderer, createWorkspacePreviewWorker);
</script><script>''' + controller + '</script></body></html>', encoding='utf-8')


def packet(path: Path, format: str) -> dict:
    content = path.read_bytes()
    if format == 'epub':
        assert content[:4] == b'PK\x03\x04'
        return json.loads(content[4:])
    if format == 'pdf':
        assert content.startswith(b'%PDF-fixture\n')
        return json.loads(content.split(b'\n', 1)[1])
    root = ET.fromstring(content)
    return json.loads(root.find('{http://www.w3.org/2000/svg}metadata').text)


def wait_check(page, expression: str) -> None:
    # Playwright's wait_for_function evaluates a string inside the page, which
    # the fixture CSP correctly refuses. CDP evaluate avoids weakening that CSP.
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if page.evaluate(expression):
            return
        page.wait_for_timeout(20)
    raise AssertionError('Browser checkpoint not reached: ' + expression)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium', help='Path to an already installed Chromium executable')
    parser.add_argument('--transport', choices=['file', 'content'], default='file',
                        help='content checks an injected document when browser policy forbids file navigation')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='fmd-publication-') as temporary, sync_playwright() as pw:
        directory = Path(temporary)
        first, old = directory / 'workspace.html', directory / 'legacy.html'
        fixture(first); fixture(old, legacy=True)
        browser = pw.chromium.launch(executable_path=args.chromium, headless=True,
                                     args=['--no-sandbox', '--disable-dev-shm-usage'])
        context = browser.new_context(accept_downloads=True)
        context.set_offline(True)
        requests, errors, downloads = [], [], []
        context.on('request', lambda request: requests.append(request.url)
                   if request.url.startswith(('http:', 'https:')) else None)
        def open_page(path: Path):
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('download', lambda download: downloads.append(download))
            if args.transport == 'file':
                page.goto(path.as_uri())
            else:
                page.set_content(path.read_text(encoding='utf-8'))
            assert page.evaluate('window.__fmdNativeRuntime.ready') is True
            wait_check(page, 'document.querySelector("#fmd-content iframe") !== null')
            return page
        page = open_page(first)
        count = 0
        def check(label: str) -> None:
            nonlocal count
            count += 1
            print(f'ok {count} - {label}', flush=True)
        def export(page, format: str, expected_name: str) -> dict:
            with page.expect_download() as event:
                page.locator('#btn-export-' + format).click()
            download = event.value
            assert download.suggested_filename == expected_name
            destination = directory / f'{len(downloads)}-{expected_name}'
            download.save_as(destination)
            return packet(destination, format)
        for format in ['epub', 'svg']:
            assert page.locator('#btn-export-' + format).count() == 1
            assert page.locator('#btn-export-' + format).is_enabled()
        check('capability-driven EPUB/SVG buttons appear after engine startup')
        epub = export(page, 'epub', 'Fixture.epub')
        assert epub['args'][:9] == [SOURCE, 'serif', 'disabled', 'Fixture', 'fr', 1.25, None, True, 3]
        assert epub['args'][9:13] == [['chart.png'], [1, 2, 3], [3], [4, 5]]
        assert epub['args'][17] == [650, 0, 0, 0, 0]
        check('EPUB click preserves exact source, navigation, image bytes and font pin')
        page.locator('#btn-zoom-in').click(); page.locator('#btn-theme-toggle').click()
        svg = export(page, 'svg', 'Fixture.svg')
        assert svg['args'][:8] == [SOURCE, 'serif', 'disabled', 1.25, None, ['chart.png'], [1, 2, 3], [3]]
        check('SVG uses document typography, not viewing zoom or theme')
        page.locator('#btn-document-settings').click()
        page.locator('[name="title"]').fill('Revised')
        page.locator('[name="fontScale"]').fill('1.5')
        page.locator('#fmd-document-settings button[type="submit"]').click()
        assert page.locator('#fmd-document-settings').is_hidden()
        epub = export(page, 'epub', 'Revised.epub')
        assert epub['args'][3] == 'Revised' and epub['args'][5] == 1.5
        check('applied document settings determine publication content and filename')
        with page.expect_download() as event:
            page.locator('#btn-save-html').click()
        saved = directory / 'reopened.html'; event.value.save_as(saved)
        reopened = open_page(saved)
        for format in ['epub', 'svg']:
            assert reopened.locator('#btn-export-' + format).count() == 1
        svg = export(reopened, 'svg', 'Revised.svg')
        assert svg['args'][0] == SOURCE and svg['args'][3] == 1.5
        assert svg['args'][6] == [1, 2, 3] and svg['args'][8] == [4, 5]
        check('Save HTML/reopen restores one working control per format and retained resources')
        for action in ['cancel', 'edit', 'silent-edit']:
            reopened.locator('#fmd-editor').fill('SLOW ' + action)
            before = len(downloads)
            reopened.locator('#btn-export-epub').click()
            wait_check(reopened, 'window.__fmdNativeRuntime.exportPending')
            for format in ['epub', 'svg', 'pdf']:
                assert not reopened.locator('#btn-export-' + format).is_enabled()
            assert not reopened.locator('#btn-publish-html').is_enabled()
            if action == 'cancel':
                reopened.locator('#btn-cancel-export').click()
            elif action == 'edit':
                reopened.locator('#fmd-editor').fill('# New revision')
            else:
                reopened.evaluate('document.querySelector("#fmd-editor").value = "# Silent newer revision"')
            wait_check(reopened, '!window.__fmdNativeRuntime.exportPending')
            wait_check(reopened, '!document.querySelector("#btn-export-epub").disabled')
            reopened.wait_for_timeout(200)
            assert len(downloads) == before
            assert 'cancel' in reopened.locator('#fmd-export-status').inner_text().lower()
            check(action + ' blocks stale downloads and restores all publication controls')
        svg = export(reopened, 'svg', 'Revised.svg')
        assert svg['args'][0] == '# Silent newer revision'
        check('a fresh SVG export succeeds after cancellation without replaying stale source')
        legacy = open_page(old)
        assert legacy.locator('#btn-export-epub').count() == 0
        assert legacy.locator('#btn-export-svg').count() == 0
        assert export(legacy, 'pdf', 'Fixture.pdf')['args'][0] == SOURCE
        check('older embedded bindings expose no unsupported buttons and retain PDF')
        assert errors == [], errors
        assert requests == [], requests
        check('offline workspace fixture uses blob workers without external requests or page errors')
        print(json.dumps({'passed': count, 'native_backend': 'explicit ABI double',
                          'browser': browser.version, 'transport': args.transport, 'external_requests': len(requests)}), flush=True)
        browser.close()


if __name__ == '__main__':
    main()
