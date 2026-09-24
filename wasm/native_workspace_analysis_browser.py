#!/usr/bin/env python3
"""Check native Document Lab plumbing with actual Chromium and explicit ABI fixtures.

Uses installed Playwright/Chromium only. No packages or browser binaries are
installed. The shell and native report/render values are explicit test doubles;
this is not evidence of Rust parsing, PDF/UA or rebuilt-WASM conformance.
Artifacts are retained in a new directory (never deleted or overwritten).
"""
from __future__ import annotations

import argparse
import json
import tempfile
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SOURCE = '\ufeff# First\r\n\r\n### Second 中𝄞\r\n\r\n![chart](chart.png)\r\n'
BINDINGS = r'''
export default async function init(input) {
  await WebAssembly.instantiate(input.module_or_path);
  await new Promise(resolve => setTimeout(resolve, 80));
}
const escape = text => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
function result(format, source) {
  const packet = JSON.stringify({format, source});
  const html = '<html><head></head><body><h1 id="first">First</h1><div style="height:900px"></div>'
    + '<h3 id="constructor">Second 中𝄞</h3><h2 id="btn-export-pdf">App ID collision</h2>'
    + '<pre>' + escape(source) + '</pre></body></html>';
  return {mimeType: {html: 'text/html', pdf: 'application/pdf', epub: 'application/epub+zip', svg: 'image/svg+xml'}[format],
    bytes: new TextEncoder().encode(format === 'html' ? html : format === 'pdf' ? '%PDF-fixture\n' + packet
      : format === 'epub' ? 'PK\x03\x04' + packet : '<svg xmlns="http://www.w3.org/2000/svg"><metadata>' + escape(packet) + '</metadata></svg>'),
    diagnosticsJson: () => '[{"message":"Preview fixture diagnostic"}]', free() {}};
}
export const renderHtmlConfiguredAdvanced = source => result('html', source);
export const renderPdfConfiguredMulti = source => result('pdf', source);
export const renderPdfConfiguredPage = source => result('pdf', source);
export const renderEpubConfiguredAdvanced = source => result('epub', source);
export const renderSvgConfiguredResources = source => result('svg', source);
export function documentStats(source) {
  if (source.startsWith('SLOW')) {
    const until = performance.now() + 1800; while (performance.now() < until) { /* native CPU fixture */ }
  }
  if (source === 'ERROR') throw Error('Deliberate native analysis failure');
  const outline = [{level: 1, text: 'First', slug: 'first'},
    {level: 3, text: 'Second 中𝄞 <img src=x onerror=alert(1)>', slug: 'constructor'},
    {level: 2, text: 'App ID collision', slug: 'btn-export-pdf'}];
  const findings = [{severity: 'warning', code: 'broken_internal_anchor', message: '#missing <script>alert(1)</script>'},
    {severity: 'info', code: 'heading_skip', message: 'H1 to H3: fixture finding'}];
  if (source === 'MANY') {
    while (outline.length < 205) outline.push({level: 2, text: 'Extra', slug: 'extra'});
    while (findings.length < 205) findings.push({severity: 'info', code: 'extra', message: 'Extra'});
  }
  return JSON.stringify({schema: 'fmd-document-stats-v1', bytes: new TextEncoder().encode(source).length,
    lines: 5, words: 123, characters: 456, sentences: 7, syllables: 8, reading_time_secs: 34, speaking_time_secs: 57,
    flesch_reading_ease: 78.25, flesch_kincaid_grade: 4.5, reading_ease_label: 'Fairly easy',
    structure: {paragraphs: 3, headings_total: outline.length, code_blocks: 2, tables: 1, lists: 2, images: 1, links_total: 4},
    outline, findings, fixture_source: source, additive_native_field: {preserved: true}}, null, 2);
}
export function accessibilityAudit(source) {
  if (source === 'ERROR') throw Error('Deliberate native audit failure');
  return JSON.stringify({schema_version: '1', target: 'pdf', page_count: 1,
    findings: [{severity: 'warning', code: 'missing_alt_text', detail: '<img src=x onerror=alert(1)>'}], fixture_source: source});
}
'''


def fixture(path: Path, mode: str = 'full') -> None:
    bindings = BINDINGS
    if mode in ('legacy', 'audit-only'):
        bindings = bindings.replace('export function documentStats', 'function documentStats')
    if mode == 'legacy':
        bindings = bindings.replace('export function accessibilityAudit', 'function accessibilityAudit')
    payload = {'version': 1, 'wasm': 'AGFzbQEAAAA=', 'bindings': bindings,
               'options': {'font': 'serif', 'darkMode': 'disabled', 'fontScale': 1.25, 'title': 'Fixture',
                           'lang': 'fr', 'toc': True, 'tocDepth': 3, 'pageNumbers': True, 'codeLineNumbers': True},
               'images': [{'destination': 'chart.png', 'bytes': 'AQID'}],
               'fonts': [{'slot': 'body-regular', 'bytes': 'BAU=', 'weight': 650}]}
    def data(value):
        return json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')
    runtime = (ROOT / 'wasm/interactive_runtime.mjs').read_text().replace('export function ', 'function ')
    worker = (ROOT / 'wasm/interactive_preview.mjs').read_text().replace('export function ', 'function ')
    controller = (ROOT / 'src/interactive_controller.js').read_text()
    ids = ['btn-save-markdown', 'btn-save-html', 'btn-toggle-view', 'btn-zoom-in', 'btn-zoom-out',
           'btn-zoom-reset', 'btn-theme-toggle', 'btn-stats-toggle', 'btn-export-pdf']
    buttons = ''.join(f'<button id="{name}" type="button">{name}</button>' for name in ids)
    shell = '''<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Fixture</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; frame-src 'self' about:; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'">
<style>body{font:16px sans-serif;margin:16px}.fmd-app-header{display:flex;flex-wrap:wrap;gap:8px}button{padding:8px}textarea{width:90%;height:140px}#stats-drawer{display:none}dt{font-weight:bold}dd{margin-bottom:8px}</style></head><body>
<header class="fmd-app-header"><span class="fmd-title">Fixture</span>''' + buttons + '''</header>
<div id="fmd-app-body" class="view-split"><div id="editor-pane"><div class="fmd-pane-header"><span id="fmd-save-status"></span></div><textarea id="fmd-editor"></textarea><span id="source-line-count"></span></div><main id="fmd-content"></main></div>
<span id="view-mode-icon"></span><span id="view-mode-label"></span>
<div id="stats-drawer"><span id="stat-words"></span><span id="stat-chars"></span><span id="stat-read-time"></span><span id="stat-readability"></span><button id="btn-stats-close"></button></div>
<script id="fmd-raw-source" type="application/json">''' + data(SOURCE) + '''</script>
<script id="fmd-native-runtime" type="application/json">''' + data(payload) + '''</script>
<script>''' + runtime + '\n' + worker + '''
bootNativeWorkspace(createNativeWorkspaceRenderer, createWorkspacePreviewWorker);
</script><script>''' + controller + '</script></body></html>'
    with path.open('x', encoding='utf-8') as stream:
        stream.write(shell)


def wait_check(page, expression: str, seconds: float = 10) -> None:
    # Avoid wait_for_function's string eval: the production CSP forbids unsafe-eval.
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if page.evaluate(expression):
            return
        page.wait_for_timeout(20)
    raise AssertionError('Browser checkpoint not reached: ' + expression)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium', default='/usr/bin/chromium')
    parser.add_argument('--transport', choices=['file', 'content'], default='file')
    parser.add_argument('--output', type=Path, help='A new, nonexisting directory for retained artifacts')
    args = parser.parse_args()
    if args.output:
        directory = args.output.resolve()
        directory.mkdir(parents=True, exist_ok=False)
    else:
        directory = Path(tempfile.mkdtemp(prefix='fmd-document-lab-'))
    first, old, partial = [directory / name for name in ['workspace.html', 'legacy.html', 'audit-only.html']]
    fixture(first); fixture(old, 'legacy'); fixture(partial, 'audit-only')
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=args.chromium, headless=True,
                                     args=['--no-sandbox', '--disable-dev-shm-usage'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1440, 'height': 1000})
        context.set_offline(True)
        requests, errors, downloads, checks = [], [], [], []
        context.on('request', lambda request: requests.append(request.url)
                   if request.url.startswith(('http:', 'https:')) else None)
        def open_page(path):
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('download', lambda item: downloads.append(item))
            if args.transport == 'file':
                page.goto(path.as_uri())
            else:
                page.set_content(path.read_text(encoding='utf-8'))
            assert page.evaluate('window.__fmdNativeRuntime.ready') is True
            wait_check(page, 'document.querySelector("#fmd-content iframe") !== null')
            return page
        def check(label):
            checks.append(label); print(f'ok {len(checks)} - {label}', flush=True)
        def stats(page):
            page.locator('#btn-lab-stats').click()
            wait_check(page, 'document.querySelector("#fmd-lab-status").textContent.startsWith("Analysis complete")')
        def report(page, name):
            with page.expect_download() as event:
                page.locator('#btn-lab-download').click()
            item = event.value
            assert item.suggested_filename == name
            path = directory / f'{len(downloads)}-{name}'
            item.save_as(path)
            return json.loads(path.read_bytes())
        page = open_page(first)
        initial_payload = page.locator('#fmd-native-runtime').text_content()
        wait_check(page, 'document.querySelector("#btn-document-lab") !== null')
        assert page.locator('#fmd-document-lab').is_hidden()
        assert not page.evaluate('window.__fmdNativeRuntime.analysisPending')
        page.locator('#btn-document-lab').click()
        assert page.locator('#fmd-document-lab').is_visible()
        assert page.locator('#btn-lab-download').is_disabled()
        check('native capabilities expose a modeless, explicit Document Lab')
        stats(page)
        assert page.locator('#fmd-lab-output dd').first.inner_text() == '123'
        assert '2 code blocks' in page.locator('#fmd-lab-output').inner_text()
        assert 'broken_internal_anchor' in page.locator('#fmd-lab-findings').inner_text()
        assert page.locator('#fmd-lab-output img,#fmd-lab-output script').count() == 0
        assert '<script>alert(1)</script>' in page.locator('#fmd-lab-findings').inner_text()
        packet = report(page, 'Fixture.stats.json')
        assert packet['fixture_source'] == SOURCE
        assert packet['bytes'] == len(SOURCE.encode('utf-8'))
        assert packet['additive_native_field'] == {'preserved': True}
        check('native metrics, safe findings and complete JSON preserve BOM, CRLF and non-BMP source')
        page.locator('#fmd-lab-outline button').nth(1).click()
        wait_check(page, 'document.querySelector("#fmd-content iframe").contentDocument.activeElement.id === "constructor"')
        before = len(downloads)
        page.locator('#fmd-lab-outline button').nth(2).click()
        wait_check(page, 'document.querySelector("#fmd-content iframe").contentDocument.activeElement.id === "btn-export-pdf"')
        assert len(downloads) == before
        check('outline navigation uses native IDs only inside the current preview, including app-ID collisions')
        page.locator('#btn-lab-audit').click()
        wait_check(page, 'document.querySelector("#fmd-lab-status").textContent.startsWith("Analysis complete")')
        assert 'engine defaults' in page.locator('#fmd-lab-output h3').inner_text()
        assert 'not this workspace' in page.locator('#fmd-lab-scope').inner_text()
        assert 'missing_alt_text' in page.locator('#fmd-lab-findings').inner_text()
        assert report(page, 'Fixture.accessibility.json')['fixture_source'] == SOURCE
        assert page.locator('#fmd-native-runtime').text_content() == initial_payload
        check('accessibility audit is explicitly scoped to engine defaults and leaves saved data unchanged')
        # Panel is modeless: source editing remains usable during native CPU work.
        for action in ['cancel', 'edit', 'close', 'escape', 'silent-edit', 'round-trip']:
            if page.locator('#fmd-document-lab').is_hidden():
                page.locator('#btn-document-lab').click()
            page.locator('#fmd-editor').fill('SLOW ' + action)
            before = len(downloads)
            page.locator('#btn-lab-stats').click()
            wait_check(page, 'window.__fmdNativeRuntime.analysisPending')
            assert page.locator('#btn-export-pdf').is_disabled()
            assert page.locator('#btn-publish-html').is_disabled()
            assert page.locator('#btn-document-settings').is_disabled()
            assert page.evaluate('''() => {try {window.__fmdNativeRuntime.exportDocument('pdf', '# Busy');}
              catch (error) {return error.code;} }''') == 'EXPORT_BUSY'
            page.evaluate('window.labHeartbeat = false; setTimeout(() => {window.labHeartbeat = true}, 30)')
            wait_check(page, 'window.labHeartbeat', seconds=1)
            if action == 'cancel':
                page.locator('#btn-lab-cancel').click()
            elif action == 'edit':
                page.locator('#fmd-editor').fill('# New revision')
            elif action == 'close':
                page.locator('#btn-lab-close').click()
            elif action == 'escape':
                page.locator('#btn-lab-close').focus(); page.keyboard.press('Escape')
            elif action == 'silent-edit':
                page.evaluate('document.querySelector("#fmd-editor").value = "# Silent newer revision"')
            else:
                page.evaluate('''() => {const e = document.querySelector('#fmd-editor'), before = e.value;
                  e.value = '# Intermediate'; e.dispatchEvent(new Event('input', {bubbles: true}));
                  e.value = before; e.dispatchEvent(new Event('input', {bubbles: true}));}''')
            wait_check(page, '!window.__fmdNativeRuntime.analysisPending')
            wait_check(page, '!document.querySelector("#btn-export-pdf").disabled')
            assert page.locator('#btn-lab-download').is_disabled()
            assert page.locator('#fmd-lab-output').inner_text() == ''
            assert len(downloads) == before
            check(action + ' cancels or rejects stale analysis without blocking the editor or exports')
        if page.locator('#fmd-document-lab').is_hidden():
            page.locator('#btn-document-lab').click()
        page.locator('#fmd-editor').fill('ERROR')
        page.locator('#btn-lab-stats').click()
        wait_check(page, 'document.querySelector("#fmd-lab-status").textContent.startsWith("Analysis failed")')
        assert page.locator('#btn-lab-stats').is_enabled()
        assert page.locator('#fmd-native-runtime').text_content() == initial_payload
        page.locator('#fmd-editor').fill('# Explicit retry')
        stats(page)
        assert report(page, 'Fixture.stats.json')['fixture_source'] == '# Explicit retry'
        check('native report failure is visible, preserves the workspace and permits an explicit retry')
        page.evaluate('document.querySelector("#fmd-editor").value = "# Changed without input event"')
        before = len(downloads)
        page.locator('#btn-lab-download').click()
        assert len(downloads) == before
        assert 'analyze again' in page.locator('#fmd-lab-status').inner_text()
        check('JSON download rechecks silent edits after analysis completion')
        page.locator('#fmd-editor').fill('MANY'); stats(page)
        assert page.locator('#fmd-lab-outline li').count() == 200
        assert page.locator('#fmd-lab-findings li').count() == 200
        packet = report(page, 'Fixture.stats.json')
        assert len(packet['outline']) == 205 and len(packet['findings']) == 205
        assert 'first 200' in page.locator('#fmd-lab-output').inner_text()
        check('large reports bound visible rows without silently truncating downloadable JSON')
        # Source-file import uses the controller's lossless source anchor.
        imported = '\ufeff# Imported\r\n\r\n中𝄞\r\n'
        page.on('dialog', lambda dialog: dialog.accept())
        # Intercept the real chooser before opening it. An unhandled headless
        # chooser is cancelled by Chromium before a later input-file assignment.
        with page.expect_file_chooser() as selected:
            page.locator('#btn-open-markdown').click()
        selected.value.set_files({'name': 'import.md', 'mimeType': 'text/markdown', 'buffer': imported.encode('utf-8')})
        wait_check(page, 'document.querySelector("#fmd-editor").value.includes("Imported")')
        assert page.locator('#btn-lab-download').is_disabled()
        stats(page)
        assert report(page, 'Fixture.stats.json')['fixture_source'] == imported
        check('source replacement invalidates prior reports and analyzes the exact imported bytes')
        # Saving never persists derived reports or their transient controls.
        with page.expect_download() as event:
            page.locator('#btn-save-html').click()
        saved = directory / 'saved-workspace.html'; event.value.save_as(saved)
        reopened = open_page(saved)
        wait_check(reopened, 'document.querySelector("#btn-document-lab") !== null')
        assert reopened.locator('#btn-document-lab').count() == 1
        assert reopened.locator('#fmd-document-lab').count() == 1
        assert reopened.locator('#fmd-document-lab').is_hidden()
        assert reopened.locator('#fmd-lab-output').inner_text() == ''
        reopened.locator('#btn-document-lab').click(); stats(reopened)
        assert report(reopened, 'Fixture.stats.json')['fixture_source'] == imported
        check('Save HTML/reopen restores one functional Document Lab without saved reports or jobs')
        # A saved file during a busy report must not retain disabled export buttons.
        reopened.locator('#fmd-editor').fill('SLOW save')
        reopened.locator('#btn-lab-stats').click()
        with reopened.expect_download() as event:
            reopened.locator('#btn-save-html').click()
        busy_saved = directory / 'saved-during-analysis.html'; event.value.save_as(busy_saved)
        restored = open_page(busy_saved)
        assert restored.locator('#btn-export-pdf').is_enabled()
        assert not restored.evaluate('window.__fmdNativeRuntime.analysisPending')
        check('saving during analysis does not persist a pending job or disabled publication controls')
        legacy = open_page(old)
        assert legacy.locator('#btn-document-lab').count() == 0
        with legacy.expect_download() as event:
            legacy.locator('#btn-export-pdf').click()
        path = directory / 'legacy.pdf'; event.value.save_as(path)
        assert path.read_bytes().startswith(b'%PDF-fixture')
        only_audit = open_page(partial)
        only_audit.locator('#btn-document-lab').click()
        assert only_audit.locator('#btn-lab-stats').is_hidden()
        assert only_audit.locator('#btn-lab-audit').is_enabled()
        check('old and partial native capabilities remain usable without unsupported report actions')
        # Exercise the four existing publication paths as a regression check.
        reopened.locator('#btn-lab-close').click()
        reopened.locator('#fmd-editor').fill('# Publication regression')
        for id in ['btn-export-pdf', 'btn-export-epub', 'btn-export-svg', 'btn-publish-html']:
            with reopened.expect_download() as event:
                reopened.locator('#' + id).click()
            item = event.value; item.save_as(directory / f'{len(downloads)}-{item.suggested_filename}')
        check('HTML/PDF/EPUB/SVG publications still download after document analysis')
        # These synthetic lifecycle/IME events verify controller cancellation,
        # not operating-system IME behavior or browser back-forward-cache entry.
        page.locator('#fmd-editor').fill('SLOW composition')
        page.locator('#btn-lab-stats').click()
        wait_check(page, 'window.__fmdNativeRuntime.analysisPending')
        page.evaluate('document.querySelector("#fmd-editor").dispatchEvent(new CompositionEvent("compositionstart"))')
        wait_check(page, '!window.__fmdNativeRuntime.analysisPending')
        assert page.locator('#btn-lab-stats').is_disabled()
        assert page.locator('#btn-lab-download').is_disabled()
        page.evaluate('document.querySelector("#fmd-editor").dispatchEvent(new CompositionEvent("compositionend"))')
        assert page.locator('#btn-lab-stats').is_enabled()
        check('composition invalidates analysis and pauses report actions until composition ends')
        page.locator('#fmd-editor').fill('SLOW lifecycle')
        page.locator('#btn-lab-stats').click()
        wait_check(page, 'window.__fmdNativeRuntime.analysisPending')
        page.evaluate('window.dispatchEvent(new PageTransitionEvent("pagehide"))')
        assert not page.evaluate('window.__fmdNativeRuntime.analysisPending')
        assert page.locator('#btn-lab-stats').is_disabled()
        assert page.locator('#fmd-lab-output').inner_text() == ''
        page.evaluate('window.dispatchEvent(new PageTransitionEvent("pageshow"))')
        page.locator('#fmd-editor').fill('# Restored document'); stats(page)
        assert report(page, 'Fixture.stats.json')['fixture_source'] == '# Restored document'
        check('page suspension cancels analysis and restoration allows a fresh report')
        before_source = page.locator('#fmd-editor').input_value()
        page.locator('#btn-document-settings').click()
        page.locator('#fmd-document-settings [name="title"]').fill('Revised')
        page.locator('#fmd-document-settings button[type="submit"]').click()
        assert page.locator('#fmd-document-settings').is_hidden()
        assert page.locator('#btn-lab-download').is_disabled()
        assert page.locator('#fmd-lab-output').inner_text() == ''
        assert page.locator('#fmd-editor').input_value() == before_source
        stats(page)
        assert report(page, 'Revised.stats.json')['fixture_source'] == before_source
        check('applied settings retire the old report without changing source and update the JSON filename')
        assert errors == [], errors
        assert requests == [], requests
        check('production blob workers operate offline without page errors or external requests')
        page.screenshot(path=str(directory / 'document-lab.png'), full_page=False)
        receipt = {'passed': len(checks), 'checks': checks, 'browser': browser.version,
                   'transport': args.transport, 'native_backend': 'explicit ABI fixture',
                   'external_requests': requests, 'page_errors': errors, 'artifacts': str(directory)}
        (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt), flush=True)
        browser.close()


if __name__ == '__main__':
    main()
