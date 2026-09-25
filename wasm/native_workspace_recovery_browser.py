#!/usr/bin/env python3
"""Exercise recovery with the production controller and explicit failing runtimes.

Runs installed Chromium/Playwright offline via content injection. It proves
controller serialization, downloads and reopening, not native Rust rendering,
WASM compilation, filesystem navigation or repair of a damaged embedded engine.
"""
from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path

from playwright.sync_api import sync_playwright

from native_workspace_async_settings_browser import BOOTSTRAP, ROOT, SOURCE, fixture, wait

PAYLOAD = {'version': 1, 'wasm': 'AGFzbQEAAAA=', 'bindings': 'export default async function() {}',
           'options': {'title': 'Original', 'font': 'serif', 'fontScale': 1.25,
                       'pageGeometry': [700.125, 850.25, 21.5, 22.5, 23.5, 24.5]},
           'images': [{'destination': 'chart.png', 'bytes': 'AQID'}],
           'fonts': [{'slot': 'body-regular', 'bytes': 'BAU=', 'weight': 650}]}


def document(controller: str, failure='fail', payload=True):
    html = fixture(controller)
    if payload:
        data = json.dumps(PAYLOAD).replace('<', '\\u003c')
        html = html.replace('<script>' + BOOTSTRAP, '<script id="fmd-native-runtime" type="application/json">'
                            + data + '</script><script>' + BOOTSTRAP)
    if failure in ['absent', 'lightweight']:
        html = html.replace('<script>' + BOOTSTRAP + '</script>', '<script>window.settingsProbe = {};</script>')
    extra = r'''
window.settingsProbe.renderCalls = 0; window.settingsProbe.printCalls = 0; window.settingsProbe.parserCalls = 0;
window.print = () => {settingsProbe.printCalls++};
window.parseMarkdownClient = source => {settingsProbe.parserCalls++; return '<p>Lightweight fixture</p>'};
const testNative = Object.getOwnPropertyDescriptor(window, '__fmdNativeRuntime')?.value;
if (testNative) {
  const render = testNative.render.bind(testNative);
  settingsProbe.restore = () => {testNative.render = render};
  settingsProbe.resolve = [];
  testNative.render = (...args) => {
    settingsProbe.renderCalls++;
    if (MODE === 'healthy') return render(...args);
    if (MODE === 'pending') return new Promise(resolve => settingsProbe.resolve.push(resolve));
    return Promise.reject(new Error('Fixture render failure'));
  };
  if (MODE === 'not-ready') {
    testNative.ready = new Promise(() => {});
    Object.defineProperty(testNative, 'settings', {get: () => null});
  }
}
'''.replace('MODE', json.dumps(failure))
    return html.replace('<script>' + controller, '<script>' + extra + '</script><script>' + controller)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium', default='/usr/bin/chromium')
    parser.add_argument('--controller', type=Path, default=ROOT / 'src/interactive_controller.js')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    directory = args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix='fmd-recovery-'))
    if args.output:
        directory.mkdir(parents=True, exist_ok=False)
    controller = args.controller.read_text()
    checks, errors, requests, downloads = [], [], [], []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True)
        context.set_offline(True)
        context.on('request', lambda r: requests.append(r.url) if r.url.startswith(('http:', 'https:')) else None)
        def open_page(mode='fail', payload=True, content=None):
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('download', lambda item: downloads.append(item))
            page.set_content(content or document(controller, mode, payload))
            return page
        def check(label):
            checks.append(label); print(f'ok {len(checks)} - {label}', flush=True)
        def save(page, id='btn-save-recovery'):
            with page.expect_download() as event:
                page.evaluate('(id) => document.getElementById(id).click()', id)
            item = event.value
            if id == 'btn-save-recovery': assert item.suggested_filename.endswith('.recovery.html')
            path = directory / f'{len(downloads)}-{item.suggested_filename}'; item.save_as(path)
            return path.read_bytes()
        def raw_source(page):
            return json.loads(page.locator('body > #fmd-raw-source').text_content())
        page = open_page()
        assert page.locator('#btn-save-recovery').count() == 1
        wait(page, 'document.querySelector("#fmd-save-status").textContent.includes("Unable to complete")')
        assert 'Save recovery copy' in page.locator('#fmd-save-status').inner_text()
        calls = page.evaluate('settingsProbe.renderCalls')
        original_preview = page.locator('#fmd-content').inner_html()
        native_text = page.locator('#fmd-native-runtime').text_content()
        recovered = save(page)
        assert page.evaluate('settingsProbe.renderCalls') == calls
        assert page.locator('#fmd-content').inner_html() == original_preview
        assert page.locator('#fmd-native-runtime').text_content() == native_text
        reopened = open_page(content=recovered.decode('utf-8'))
        assert raw_source(reopened) == SOURCE
        assert reopened.locator('#fmd-native-runtime').text_content() == native_text
        assert reopened.locator('#fmd-content iframe').count() == 0
        assert 'preview intentionally omitted' in reopened.locator('#fmd-content').inner_text()
        assert reopened.locator('html').get_attribute('data-fmd-recovery') == '1'
        assert reopened.locator('#btn-save-recovery').count() == 1
        assert save(reopened, 'btn-save-markdown') == SOURCE.encode('utf-8')
        check('render failure still permits recovery of exact BOM/CRLF source, settings, images, fonts and runtime without another render')
        second = save(reopened)
        twice = open_page(content=second.decode('utf-8'))
        assert raw_source(twice) == SOURCE
        assert json.loads(twice.locator('#fmd-native-runtime').text_content()) == PAYLOAD
        assert twice.locator('#btn-save-recovery').count() == 1
        check('repeated recovery/reopen retains resource bytes without duplicate controls or a misleading preview')
        not_ready = open_page('not-ready')
        assert not_ready.locator('#btn-save-recovery').is_enabled()
        ready_copy = save(not_ready)
        assert not_ready.evaluate('settingsProbe.renderCalls') == 0
        assert b'chart.png' in ready_copy
        check('recovery is available before an unresolved native initialization promise')
        absent = open_page('absent')
        absent.locator('#fmd-editor').fill('# Edited despite bootstrap failure')
        absent.wait_for_timeout(200)
        absent.locator('#btn-export-pdf').click()
        assert absent.evaluate('settingsProbe.parserCalls') == 0
        assert absent.evaluate('settingsProbe.printCalls') == 0
        assert absent.evaluate('''() => {const event = new KeyboardEvent('keydown', {key:'p',ctrlKey:true,cancelable:true});
          document.dispatchEvent(event); return event.defaultPrevented;}''')
        absent_copy = save(absent)
        absent_again = open_page(content=absent_copy.decode('utf-8'))
        assert raw_source(absent_again) == '# Edited despite bootstrap failure'
        check('missing native handoff never silently selects the reduced parser or browser PDF printing and remains recoverable')
        malicious = '</textarea><script>window.sourceInjected=true</script><!--\u2028\u2029 中𝄞'
        raw_payload = dict(PAYLOAD, bindings='</script><script>window.runtimeInjected=true</script><!--<script>')
        page.evaluate('(text) => {document.querySelector("#fmd-editor").value = text}', malicious)
        page.evaluate('(text) => {document.querySelector("#fmd-native-runtime").textContent = text}', json.dumps(raw_payload, ensure_ascii=False))
        safe = save(page)
        safe_page = open_page(content=safe.decode('utf-8'))
        assert raw_source(safe_page) == malicious
        assert json.loads(safe_page.locator('#fmd-native-runtime').text_content()) == raw_payload
        assert safe_page.evaluate('window.sourceInjected === undefined && window.runtimeInjected === undefined')
        check('host-supplied unescaped runtime JSON and script-like source remain inert while preserving decoded values')
        broken = '{"version":1,'
        page.evaluate('(text) => {document.querySelector("#fmd-native-runtime").textContent = text}', broken)
        damaged = save(page)
        damaged_page = open_page(content=damaged.decode('utf-8'))
        assert damaged_page.locator('#fmd-native-runtime').text_content() == broken
        assert raw_source(damaged_page) == malicious
        check('damaged runtime JSON is retained without requiring parsing or pretending to repair the embedded engine')
        pending = open_page('pending')
        wait(pending, 'settingsProbe.renderCalls > 0')
        count = len(downloads)
        pending.locator('#btn-save-html').click(); pending.wait_for_timeout(100)
        assert len(downloads) == count
        waiting_copy = save(pending)
        assert len(downloads) == count + 1
        pending.evaluate('settingsProbe.resolve.forEach(resolve => resolve(true))')
        pending.wait_for_timeout(150)
        assert len(downloads) == count + 1
        assert b'preview intentionally omitted' in waiting_copy
        check('recovery completes while a render hangs and supersedes a waiting Save HTML so late completion causes no surprise download')
        healthy = open_page('healthy')
        wait(healthy, 'document.querySelector("#fmd-content iframe") !== null')
        healthy_copy = save(healthy)
        regenerated = open_page(content=healthy_copy.decode('utf-8'))
        wait(regenerated, 'document.querySelector("#fmd-content iframe") !== null')
        assert regenerated.locator('html').get_attribute('data-fmd-recovery') is None
        assert raw_source(regenerated) == SOURCE
        ordinary = save(regenerated, 'btn-save-html')
        regular_page = open_page(content=ordinary.decode('utf-8'))
        assert regular_page.locator('html').get_attribute('data-fmd-recovery') is None
        check('a healthy engine regenerates the recovered preview and normal Save HTML resumes without a recovery marker')
        healthy.evaluate('settingsProbe.mode = "slow"')
        healthy.locator('#btn-document-settings').click()
        healthy.locator('[name="title"]').fill('Unapplied recovery draft')
        healthy.locator('#fmd-document-settings button[type="submit"]').click()
        wait(healthy, '__fmdNativeRuntime.settingsPending')
        staged = save(healthy)
        assert healthy.evaluate('__fmdNativeRuntime.settingsPending')
        staged_page = open_page(content=staged.decode('utf-8'))
        assert staged_page.evaluate('__fmdNativeRuntime.settings.title') == 'Original'
        assert staged_page.locator('#fmd-document-settings').is_hidden()
        assert staged_page.locator('#btn-export-pdf').is_enabled()
        assert json.loads(staged_page.locator('#fmd-native-runtime').text_content()) == PAYLOAD
        healthy.locator('[data-cancel]').click()
        check('recovery during settings preflight saves committed data without invoking or cancelling the renderer')
        # Supply an oversized source through a scoped value getter: laying out a
        # real 33 MiB textarea tests Chromium's text layout rather than this
        # serializer. The Node suite independently counts full-size real strings.
        # The production controller must refuse this value before DOM cloning.
        page.evaluate('''() => {
          window.originalClone = document.documentElement.cloneNode;
          window.cloneCalls = 0;
          document.documentElement.cloneNode = function(...args) {window.cloneCalls++; return originalClone.apply(this,args)};
          const oversized = 'x'.repeat(32 * 1024 * 1024 + 1);
          Object.defineProperty(document.querySelector('#fmd-editor'), 'value', {configurable:true, get:() => oversized});
        }''')
        count = len(downloads)
        page.locator('#btn-save-recovery').click()
        assert len(downloads) == count and page.evaluate('cloneCalls') == 0
        assert 'byte limit' in page.locator('#fmd-save-status').inner_text()
        page.evaluate('delete document.querySelector("#fmd-editor").value; document.querySelector("#fmd-editor").value = "\\ud800"')
        page.locator('#btn-save-recovery').click()
        assert len(downloads) == count and page.evaluate('cloneCalls') == 0
        assert 'invalid Unicode' in page.locator('#fmd-save-status').inner_text()
        page.evaluate('document.documentElement.cloneNode = originalClone; document.querySelector("#fmd-editor").value = ""')
        empty = save(page)
        empty_page = open_page(content=empty.decode('utf-8'))
        assert raw_source(empty_page) == ''
        check('oversized or malformed source is refused before DOM cloning; an empty source recovers correctly')
        page.evaluate('''() => {const base=document.documentElement.cloneNode;
          document.documentElement.cloneNode = function(...args) {
            const copy=base.apply(this,args); document.querySelector('#fmd-editor').value='# Reentrant change'; return copy;
          };}''')
        count = len(downloads)
        page.locator('#btn-save-recovery').click()
        assert len(downloads) == count
        assert 'changed during serialization' in page.locator('#fmd-save-status').inner_text()
        check('serialization rechecks document identity instead of saving an unexpectedly changed source')
        light = open_page('lightweight', payload=False)
        assert light.locator('#btn-save-recovery').count() == 0
        light.locator('#fmd-editor').fill('# Lightweight edit'); light.wait_for_timeout(200)
        assert light.evaluate('settingsProbe.parserCalls') > 0
        normal_light = save(light, 'btn-save-html')
        assert b'Lightweight fixture' in normal_light
        check('ordinary lightweight workspaces keep their existing parser and save path without native recovery controls')
        assert requests == [], requests
        assert errors == [], errors
        check('all recovery checks run offline without external HTTP requests or page errors')
        receipt = {'passed': len(checks), 'checks': checks, 'browser': browser.version, 'transport': 'content',
                   'native_runtime': 'explicit failure/API fixtures', 'external_requests': requests, 'page_errors': errors}
        (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt), flush=True)
        browser.close()


if __name__ == '__main__':
    main()
