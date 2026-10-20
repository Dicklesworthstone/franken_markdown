#!/usr/bin/env python3
"""Reopen native source projects with real Chromium Blob workers and downloads.

The renderer/bindings/source-host are explicit adapters, not compiled Rust/WASM.
No tools, browsers or dependencies are downloaded. Artifacts are retained.
"""
from __future__ import annotations
import argparse
import base64
import json
import pathlib
import re
import shutil
import subprocess
import tempfile
from playwright.sync_api import sync_playwright, expect


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=pathlib.Path)
    parser.add_argument('--browser', default=shutil.which('chromium'))
    parser.add_argument('--mode', choices=['file', 'content'], default='file')
    args = parser.parse_args()
    if not args.browser:
        parser.error('Supply an already installed Chromium with --browser PATH')
    if args.output:
        args.output.mkdir(parents=True, exist_ok=False)
        output = args.output.resolve()
    else:
        output = pathlib.Path(tempfile.mkdtemp(prefix='fmd-project-open-'))
    fixture_module = pathlib.Path(__file__).with_name('native_source_project_fixture.mjs').as_uri()
    script = ('import {writeFile} from "node:fs/promises"; import {nativeSourceProjectFixture, projectFixtureBindings} from '
              + json.dumps(fixture_module) + '; await writeFile(process.argv[1], await nativeSourceProjectFixture(projectFixtureBindings()), {flag:"wx"});')
    workspace = output / 'original-workspace.html'
    subprocess.run(['node', '--input-type=module', '-e', script, str(workspace)], check=True)
    source = '\ufeff# Imported project\r\n\r\nExact naïve 🧪 source.\r\n\r\n</script><script>globalThis.projectRan=true</script>\r\n'
    svg = base64.b64encode(b'<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"/>').decode()
    project = {'schema': 'fmd-native-source-project-v1', 'source': source,
               'options': {'font': 'serif', 'darkMode': 'disabled', 'fontScale': 1.25, 'title': 'Imported project',
                           'author': 'Importer', 'lang': 'nl', 'metadataEpochSeconds': 0, 'pageNumbers': True,
                           'codeLineNumbers': True, 'toc': True, 'tocDepth': 4, 'pageGeometry': [612.125, 792.25, 32, 33, 34, 35]},
               'images': [{'destination': 'imported.svg', 'bytes': svg}, {'destination': 'unused-import.svg', 'bytes': svg}],
               'fonts': [{'slot': 'body-bold', 'bytes': 'BwgJ', 'weight': 625}]}
    selected = output / 'input.fmd.json'
    selected.write_text(json.dumps(project, ensure_ascii=False), encoding='utf-8')
    invalid = output / 'invalid.fmd.json'
    invalid.write_text(json.dumps({**project, 'bindings': 'throw Error("untrusted project code");'}), encoding='utf-8')
    checks: list[str] = []
    errors: list[str] = []
    network: list[str] = []
    workers: list[str] = []
    downloads: list[str] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=args.browser, headless=True, args=['--no-sandbox'])
        def new_page(path: pathlib.Path):
            context = browser.new_context(accept_downloads=True, viewport={'width': 1100, 'height': 820})
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('request', lambda request: network.append(request.url) if request.url.startswith(('http:', 'https:')) else None)
            page.on('worker', lambda worker: workers.append(worker.url))
            page.on('download', lambda download: downloads.append(download.suggested_filename))
            if args.mode == 'file':
                page.goto(path.as_uri(), wait_until='load')
            else:
                page.set_content(path.read_text(encoding='utf-8'), wait_until='load')
            page.locator('#btn-open-source-project').wait_for()
            return context, page
        context, page = new_page(workspace)
        page.locator('#fmd-editor').fill('CURRENT PRIVATE UNSAVED TEXT\nDo not replace me.')
        original_text = page.locator('#fmd-editor').input_value()
        original_runtime = page.locator('#fmd-native-runtime').text_content()
        assert page.evaluate('window.__fmdNativeRuntime.settings') is None
        page.locator('#btn-open-source-project').click()
        page.locator('#fmd-project-open-file').set_input_files(str(selected))
        assert not workers
        assert page.locator('#btn-download-opened-project').is_disabled()
        checks.append('File selection is explicit, creates no worker, and works despite fixture native-startup failure')
        def build() -> None:
            page.locator('#btn-build-source-project').click()
            expect(page.locator('#fmd-project-open-status')).to_contain_text('Workspace ready:', timeout=15000)
        def download(name: str) -> bytes:
            with page.expect_download() as event:
                page.locator('#btn-download-opened-project').click()
            item = event.value
            assert item.suggested_filename == 'Imported project.html'
            destination = output / name
            item.save_as(destination)
            return destination.read_bytes()
        build()
        assert not downloads
        checks.append('Real Blob worker imports selected trusted bindings and prepares a workspace without auto-download')
        first = download('opened-workspace.html')
        text = first.decode('utf-8')
        raw_source = json.loads(re.search(r'id="fmd-raw-source">([\s\S]*?)</script>', text)[1])
        payload = json.loads(re.search(r'id="fmd-native-runtime">([\s\S]*?)</script>', text)[1])
        assert raw_source == source
        for key in ['options', 'images', 'fonts']:
            assert payload[key] == project[key], key
        assert payload['bindings'] == json.loads(original_runtime)['bindings']
        assert payload['wasm'] == json.loads(original_runtime)['wasm']
        assert 'CURRENT PRIVATE UNSAVED TEXT' not in text
        assert 'unused.png' not in json.dumps(payload['images'])
        assert page.locator('#fmd-editor').input_value() == original_text
        assert page.locator('#fmd-native-runtime').text_content() == original_runtime
        checks.append('Downloaded workspace preserves complete imported data, excludes old resources and leaves unsaved source untouched')
        before = len(workers)
        assert download('opened-repeat.html') == first
        assert len(workers) == before
        checks.append('Repeated download uses identical prepared bytes without another worker or render')
        page.evaluate('document.getElementById("btn-build-source-project").click(); document.getElementById("btn-cancel-open-project").click();')
        expect(page.locator('#fmd-project-open-status')).to_contain_text('cancelled')
        assert page.locator('#btn-download-opened-project').is_disabled()
        checks.append('Immediate cancellation retires an actual worker and prevents publication')
        page.locator('#fmd-project-open-file').set_input_files(str(selected))
        page.evaluate('document.getElementById("btn-build-source-project").click(); const input=document.getElementById("fmd-project-open-file"); input.value=""; input.dispatchEvent(new Event("change"));')
        expect(page.locator('#fmd-project-open-status')).to_contain_text('Selection changed')
        assert page.locator('#btn-download-opened-project').is_disabled()
        checks.append('Changing file selection invalidates pending opening')
        page.locator('#fmd-project-open-file').set_input_files(str(invalid))
        page.locator('#btn-build-source-project').click()
        expect(page.locator('#fmd-project-open-status')).to_contain_text('Project opening failed:', timeout=10000)
        assert page.locator('#btn-download-opened-project').is_disabled()
        assert page.evaluate('globalThis.projectRan') is None
        assert page.locator('#fmd-editor').input_value() == original_text
        checks.append('Executable fields in a project are rejected as data; no imported source code runs')
        page.locator('#fmd-project-open-file').set_input_files(str(selected))
        page.evaluate('(bindings) => { const node=document.getElementById("fmd-native-runtime"); const p=JSON.parse(node.textContent); p.bindings=bindings; node.textContent=JSON.stringify(p); }',
                      'export default async()=>{throw Error("explicit startup failure")};')
        build_button = page.locator('#btn-build-source-project')
        build_button.click()
        expect(page.locator('#fmd-project-open-status')).to_contain_text('explicit startup failure', timeout=10000)
        assert page.locator('#btn-download-opened-project').is_disabled()
        page.evaluate('(text)=>{document.getElementById("fmd-native-runtime").textContent=text}', original_runtime)
        checks.append('Trusted-engine startup failure has no partial output and leaves the current document unchanged')
        page.evaluate('window.dispatchEvent(new PageTransitionEvent("pagehide", {persisted:true})); window.dispatchEvent(new PageTransitionEvent("pageshow", {persisted:true}));')
        assert not page.locator('#fmd-open-source-project').is_visible()
        assert page.locator('#fmd-project-open-file').evaluate('(node)=>node.files.length') == 0
        checks.append('Synthetic suspension clears local file selection and requires explicit reopening')
        context.close()
        context, page = new_page(output / 'opened-workspace.html')
        assert json.loads(page.locator('#fmd-raw-source').text_content()) == source
        assert page.locator('#btn-open-source-project').count() == 1
        with page.expect_download() as event:
            page.locator('#btn-save-source-project').click()
        saved = output / 'round-trip.fmd.json'
        event.value.save_as(saved)
        assert json.loads(saved.read_text(encoding='utf-8')) == project
        checks.append('A fresh browser context opens the rebuilt workspace and saves an exact complete source project')
        page.locator('#btn-open-source-project').click()
        page.locator('#fmd-project-open-file').set_input_files(str(selected))
        build()
        assert download('second-generation.html') == first
        checks.append('The reopened workspace can reopen projects itself with byte-identical output and no bootstrap growth')
        page.set_viewport_size({'width': 390, 'height': 820})
        page.screenshot(path=str(output / 'open-controls-390.png'), full_page=True)
        for selector in ['#fmd-open-source-project', '#fmd-project-open-file', '#btn-download-opened-project']:
            assert page.locator(selector).is_visible()
            assert page.locator(selector).evaluate('(node)=>node.getBoundingClientRect().right <= innerWidth')
        checks.append('Open controls and the selected file remain visible and contained at a 390-pixel viewport')
        assert not network, network
        assert not errors, errors
        context.close()
        browser.close()
    report = {'mode': args.mode, 'passed': len(checks), 'checks': checks, 'workers_observed': len(workers),
              'downloads_observed': len(downloads), 'page_errors': errors, 'network_requests': network,
              'native_engine': 'explicit adapter, not Rust/WASM', 'lossless_host': 'explicit adapter, not the production controller'}
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, indent=2))
    print('Retained artifacts:', output)


if __name__ == '__main__':
    main()
