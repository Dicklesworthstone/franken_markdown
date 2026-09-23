#!/usr/bin/env python3
"""Plain HTML publishing through the shipped bootstrap, renderer adapter and UI.

Uses the retained source-control fixture shell, production controller/runtime,
and an actual empty WASM module. Native ABI functions are explicit adapters,
not Rust parsing, font embedding, PDF layout or native/WASM parity evidence.
Requires existing Node, Playwright and Chromium; installs/downloads nothing.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
import tempfile

from playwright.sync_api import sync_playwright
from native_workspace_source_browser import fixture as source_fixture, ORIGINAL

ROOT = pathlib.Path(__file__).resolve().parents[1]
PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6SAAAAABJRU5ErkJggg=='
IMPORTED = '\ufeff\r\n# Published é中😀\r\n![chart](chart.png)\r\n<script>globalThis.sourceRan=true</script>\r\n'


def fixture() -> str:
    functions = json.loads(subprocess.check_output(['node', '--input-type=module', '-e',
        "import {bootNativeWorkspace as boot,createNativeWorkspaceRenderer as factory} from './wasm/interactive_runtime.mjs';console.log(JSON.stringify({boot:boot.toString(),factory:factory.toString()}));"], cwd=ROOT, text=True))
    bindings = r'''
export default async ({module_or_path}) => {
  await WebAssembly.instantiate(module_or_path);
  await new Promise(resolve => { globalThis.releasePublisherFixture=resolve; });
};
const esc = text => String(text).replace(/[&<>"']/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
export function renderHtmlConfiguredAdvanced(...a) {
  if(globalThis.failNextPublish) { globalThis.failNextPublish=false; throw Error('Injected native publication failure'); }
  globalThis.nativeHtmlCalls=(globalThis.nativeHtmlCalls||0)+1;
  const abi={font:a[1],mode:a[2],title:a[3],scale:a[6],lang:a[16],toc:a[17],depth:a[18],weights:Array.from(a[12])};
  let offset=0;
  const images=a[13].map((key,i)=>{const bytes=a[14].subarray(offset,offset+a[15][i]);offset+=a[15][i];
    return a[0].includes(key)?'<img alt="'+esc(key)+'" src="data:image/png;base64,'+btoa(String.fromCharCode(...bytes))+'">':'';}).join('');
  const html='<!DOCTYPE html><html lang="'+esc(a[16])+'"><head><meta charset="utf-8"><title>'+esc(a[3])+'</title>'
    +'<style>@media (prefers-color-scheme: dark) { body { color: white; background: black; } }</style></head><body><main>'
    +'<pre id="abi">'+esc(JSON.stringify(abi))+'</pre><pre id="published-source">'+esc(a[0])+'</pre>'+images+'</main></body></html>';
  const bytes=new TextEncoder().encode(html);
  return {bytes,mimeType:'text/html',diagnosticsJson:()=>JSON.stringify(globalThis.publishWarning?[{message:'Adapter warning visible to author'}]:[]),free(){bytes.fill(0);}};
}
export function renderPdfConfiguredMulti(){throw Error('PDF not part of this probe');}
'''
    payload = {'version': 1, 'wasm': 'AGFzbQEAAAA=', 'bindings': bindings,
               'options': {'font': 'serif', 'fontScale': 1.25, 'darkMode': 'auto', 'title': 'Initial title',
                           'lang': 'fr', 'toc': True, 'tocDepth': 3, 'metadataEpochSeconds': 0},
               'images': [{'destination': 'chart.png', 'bytes': PNG},
                          {'destination': 'NEVER_PUBLISH_UNUSED_RESOURCE', 'bytes': 'AQIDBA=='}],
               'fonts': [{'slot': 'body-regular', 'bytes': 'BQYH', 'weight': 555}]}
    data = json.dumps(payload).replace('<', '\\u003c')
    boot = '<script id="fmd-native-runtime" type="application/json">' + data + '</script><script>;(' + functions['boot'] + ')(' + functions['factory'] + ');</script>'
    shell = source_fixture(False)
    # Use the native workspace's existing offline script/iframe capabilities,
    # not a server, CSP bypass, external module, or browser-security switch.
    shell = shell.replace("script-src 'unsafe-inline';", "script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; font-src data:; frame-src 'self' about:;")
    return shell.replace('<script>', boot + '<script>', 1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=['file', 'content'], default='file')
    parser.add_argument('--browser', default=shutil.which('chromium'))
    parser.add_argument('--output', type=pathlib.Path)
    args = parser.parse_args()
    output = args.output or pathlib.Path(tempfile.mkdtemp(prefix='fmd-publishing-'))
    if args.output:
        output.mkdir(parents=True, exist_ok=False)
    output = output.resolve()
    (output / 'article.md').write_bytes(IMPORTED.encode())
    checks, requests, errors = [], [], []
    def check(condition: bool, message: str) -> None:
        if not condition:
            raise AssertionError(message)
        checks.append(message)

    with sync_playwright() as pw:
        browser = pw.chromium.launch(headless=True, executable_path=args.browser, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1280, 'height': 900})
        context.on('request', lambda req: requests.append(req.url) if req.url.startswith(('http:', 'https:')) else None)
        context.route('http://**/*', lambda route: route.abort())
        context.route('https://**/*', lambda route: route.abort())
        def open_page(content: str, name: str, *, workspace: bool = True, release: bool = True):
            path = output / name
            with path.open('x', encoding='utf-8') as stream:
                stream.write(content)
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('dialog', lambda dialog: dialog.accept())
            if args.mode == 'file':
                page.goto(path.as_uri())
            else:
                page.set_content(content)
            if workspace:
                page.wait_for_function('() => typeof releasePublisherFixture === "function"')
                if release:
                    page.evaluate('releasePublisherFixture()')
                    page.wait_for_function('() => !document.getElementById("btn-publish-html").disabled')
            return page

        serial = 0
        def download(page, trigger, suffix: str):
            nonlocal serial
            serial += 1
            with page.expect_download() as pending:
                trigger()
            dest = output / f'download-{serial}.{suffix}'
            pending.value.save_as(dest)
            check(pending.value.failure() is None, f'download {serial} completed')
            return dest.read_bytes(), pending.value.suggested_filename

        def publish(page):
            data, name = download(page, lambda: page.locator('#btn-publish-html').click(), 'published.html')
            check(name.endswith('.published.html'), 'plain publication has a distinct filename suffix')
            return data.decode()

        def dirty(page):
            return page.evaluate("() => {const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented;}")

        page = open_page(fixture(), 'workspace.html', release=False)
        check(page.locator('#btn-publish-html').is_disabled(), 'publishing disabled until real WASM bootstrap finishes')
        page.evaluate('releasePublisherFixture()')
        page.wait_for_function('() => !document.getElementById("btn-publish-html").disabled')
        check(page.locator('#btn-publish-html').count() == 1, 'one native Publish HTML control')
        with page.expect_file_chooser() as chooser:
            page.locator('#btn-open-markdown').click()
        chooser.value.set_files(output / 'article.md')
        page.wait_for_function('() => !document.getElementById("btn-open-markdown").disabled')
        dialog = page.locator('#fmd-document-settings')
        page.locator('#btn-document-settings').click()
        dialog.locator('[name=font]').select_option('sans')
        dialog.locator('[name=fontScale]').fill('1.75')
        dialog.locator('[name=title]').fill('Published <literal> & title')
        dialog.locator('[name=lang]').fill('de')
        dialog.locator('[name=tocDepth]').fill('5')
        dialog.locator('[type=submit]').click()
        page.locator('#btn-zoom-in').click()
        page.locator('#btn-theme-toggle').click()
        page.evaluate('window.beforePublishFrame=document.querySelector("#fmd-content > iframe");window.beforePublishData=document.getElementById("fmd-native-runtime").textContent;')
        text = publish(page)
        check(page.evaluate('beforePublishFrame===document.querySelector("#fmd-content > iframe")'), 'publishing leaves the live preview frame unchanged')
        check(page.evaluate('beforePublishData===document.getElementById("fmd-native-runtime").textContent'), 'publishing leaves saved runtime JSON unchanged')
        check(dirty(page) and page.locator('#btn-undo-source-open').is_enabled(), 'publishing neither marks work saved nor consumes replacement undo')
        for marker in ['fmd-native-runtime', 'fmd-raw-source', 'fmd-editor', 'wasm-unsafe-eval', 'releasePublisherFixture', 'NEVER_PUBLISH_UNUSED_RESOURCE']:
            check(marker not in text, 'publication excludes ' + marker)
        check('<script' not in text.lower() and '<iframe' not in text.lower(), 'plain document has no scripts or editor frame')
        check("default-src 'none'" in text and "img-src data:" in text, 'publication retains native offline resource policy')
        check('@media (prefers-color-scheme: dark)' in text and '@media all' not in text, 'view theme does not rewrite published document theme')
        published = open_page(text, 'publication.html', workspace=False)
        abi = json.loads(published.locator('#abi').inner_text())
        check(abi == {'font': 'sans', 'mode': 'auto', 'title': 'Published <literal> & title', 'scale': 1.75, 'lang': 'de', 'toc': True, 'depth': 5, 'weights': [555, 0, 0, 0, 0]}, 'published native ABI receives committed typography/title/language/TOC and font pins, not zoom')
        check(published.title() == 'Published <literal> & title', 'metadata remains literal text in reopened publication')
        check(published.locator('img').count() == 1, 'only the used embedded image travels in native adapter output')
        published.wait_for_function('() => [...document.images].every(image=>image.complete)')
        check(published.locator('img').evaluate('(image)=>image.naturalWidth===1'), 'published embedded PNG loads without external access')
        check(published.evaluate('!globalThis.sourceRan && !globalThis.__fmdNativeRuntime'), 'publication runs no source script or editing engine')
        check(published.locator('#published-source').inner_text() == IMPORTED.replace('\r\n', '\n'), 'published rendered content reflects selected source')
        published.screenshot(path=str(output / 'plain-publication.png'), full_page=True)
        # Execute a hostile inline-script insertion under the exported policy:
        # this is a policy regression, not an attempt to sanitize trusted bindings.
        blocked = open_page(text.replace('</body>', '<script>globalThis.unexpectedScript=true</script></body>'), 'policy-check.html', workspace=False)
        check(blocked.evaluate('!globalThis.unexpectedScript'), 'exported CSP blocks subsequently inserted inline scripts')
        immediate, _ = download(page, lambda: page.evaluate("() => {const editor=document.getElementById('fmd-editor');editor.value='Newest before preview debounce';editor.dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('btn-publish-html').click();}"), 'immediate.html')
        check('Newest before preview debounce' in immediate.decode(), 'publication captures edits before preview debounce')
        page.wait_for_timeout(200)
        page.locator('#btn-document-settings').click()
        dialog.locator('[name=fontScale]').fill('2.5')
        # Modal draft remains unapplied even when an application caller requests
        # publication; this test does not claim the blocked toolbar is clickable.
        draft_bytes, _ = download(page, lambda: page.evaluate("document.getElementById('btn-publish-html').click()"), 'draft-check.html')
        draft = open_page(draft_bytes.decode(), 'draft-publication.html', workspace=False)
        check(json.loads(draft.locator('#abi').inner_text())['scale'] == 1.75, 'unapplied settings draft is not published')
        dialog.locator('[data-cancel]').click()
        captured = []
        page.on('download', lambda download: captured.append(download))
        page.evaluate('globalThis.failNextPublish=true')
        page.locator('#btn-publish-html').click()
        page.wait_for_timeout(50)
        check(not captured and 'Injected native publication failure' in page.locator('#fmd-save-status').inner_text(), 'native failure produces an error rather than a stale download')
        page.evaluate('globalThis.publishWarning=true')
        recovered = publish(page)
        check('Adapter warning visible to author' in page.locator('#fmd-save-status').inner_text(), 'native diagnostics remain visible to the author after download')
        saved, name = download(page, lambda: page.locator('#btn-save-html').click(), 'workspace.html')
        check(not name.endswith('.published.html'), 'editable workspace retains a distinct ordinary HTML filename')
        reopened = open_page(saved.decode(), 'workspace-reopened.html')
        check(reopened.locator('#btn-publish-html').count() == 1 and reopened.locator('#fmd-source-controls').count() == 1, 'save/reopen recreates one publication and source toolbar')
        check(not dirty(reopened), 'reopened workspace starts with a clean saved baseline')
        check(publish(reopened) == recovered, 'publication remains byte-identical across saved-workspace reopening with fixed source/settings')
        check(not dirty(reopened), 'publishing a clean reopened document keeps it clean')
        second, _ = download(reopened, lambda: reopened.locator('#btn-save-html').click(), 'workspace.html')
        reopened_twice = open_page(second.decode(), 'workspace-reopened-twice.html')
        check(reopened_twice.locator('#btn-publish-html').count() == 1, 'second reopening does not duplicate publication controls')
        lightweight = open_page(source_fixture(False), 'lightweight.html', workspace=False)
        check(lightweight.locator('#btn-publish-html').count() == 0, 'lightweight workspace does not advertise unavailable native publishing')
        check(not requests, 'zero HTTP(S) requests')
        check(not errors, 'zero uncaught page errors')
        report = {'proof': 'production-bootstrap-runtime-controller-with-explicit-native-ABI-and-shell-adapters',
                  'mode': args.mode, 'browser': browser.version, 'assertions': len(checks), 'checks': checks,
                  'network_requests': requests, 'page_errors': errors, 'rust_rendering': False,
                  'font_embedding': False, 'pdf_layout': False, 'artifacts': str(output)}
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
        browser.close()


if __name__ == '__main__':
    main()
