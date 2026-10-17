#!/usr/bin/env python3
"""Retained source-project UI/worker checks in an existing local Chromium.

Uses the actual exporter, codec and controls with explicit native renderer and
lossless-host fixtures. Does not claim Rust/WASM rendering, PDF conformance, or
the production controller's complete undo/IME lifecycle. Downloads no tools.
"""
from __future__ import annotations
import argparse
import base64
import json
import pathlib
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
        parser.error('An already installed Chromium is required; supply --browser PATH')
    root = pathlib.Path(__file__).resolve().parent
    if args.output:
        args.output.mkdir(parents=True, exist_ok=False)
        output = args.output.resolve()
    else:
        output = pathlib.Path(tempfile.mkdtemp(prefix='fmd-source-project-'))
    fixture = output / 'workspace.html'
    subprocess.run(['node', str(root / 'native_source_project_fixture.mjs'), str(fixture)], check=True)
    checks: list[str] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=args.browser, headless=True, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1100, 'height': 820})
        page = context.new_page()
        errors: list[str] = []
        network: list[str] = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.on('request', lambda request: network.append(request.url) if request.url.startswith(('http:', 'https:')) else None)
        def load() -> None:
            if args.mode == 'file':
                page.goto(fixture.as_uri(), wait_until='load')
            else:
                page.set_content(fixture.read_text(), wait_until='load')
            page.locator('#btn-save-source-project').wait_for()
        load()
        expected = '\ufeff# Project transport\r\n\r\nPreserve 😀, naïve, and e\u0301.\r\n![Chart](chart.svg)\r\n'
        assert page.evaluate('window.__fmdNativeRuntime.settings') is None
        assert page.locator('#btn-save-source-project').is_enabled()
        checks.append('Controls work after explicit native startup failure without creating an engine')
        def save(name: str) -> tuple[bytes, dict]:
            with page.expect_download() as event:
                page.locator('#btn-save-source-project').click()
            download = event.value
            path = output / name
            download.save_as(path)
            content = path.read_bytes()
            assert download.suggested_filename.endswith('.fmd.json')
            assert page.locator('#fmd-source-project-status').inner_text().startswith('Source-project download requested')
            return content, json.loads(content)
        first, project = save('first.fmd.json')
        assert project['source'] == expected
        assert set(project) == {'schema', 'source', 'options', 'images', 'fonts'}
        assert project['schema'] == 'fmd-native-source-project-v1'
        assert project['options']['metadataEpochSeconds'] == 0
        assert project['options']['pageGeometry'] == [600.125, 800.25, 35, 40, 45, 50]
        assert [image['destination'] for image in project['images']] == ['chart.svg', 'unused.png']
        assert base64.b64decode(project['images'][1]['bytes']) == bytes([1, 2, 3])
        assert project['fonts'] == [{'slot': 'body-regular', 'bytes': 'AAECAw==', 'weight': 525}]
        checks.append('Real Blob-worker download preserves exact source, resources, epoch and fractional geometry')
        second, _ = save('repeat.fmd.json')
        assert first == second
        checks.append('Repeated explicit saves are byte-identical')
        page.evaluate('document.getElementById("btn-save-source-project").click(); document.getElementById("btn-cancel-source-project").click()')
        assert 'cancelled' in page.locator('#fmd-source-project-status').inner_text()
        assert page.locator('#btn-save-source-project').is_enabled()
        checks.append('Cancellation terminates the job and returns controls without altering source')
        page.evaluate('document.getElementById("btn-save-source-project").click(); document.getElementById("fmd-editor").value += "changed without input"')
        expect(page.locator('#fmd-source-project-status')).to_contain_text('changed')
        checks.append('Programmatic source changes without input events refuse stale delivery')
        page.evaluate('document.getElementById("btn-save-source-project").click(); const n=document.getElementById("fmd-native-runtime"); const p=JSON.parse(n.textContent); p.options.title="Changed"; n.textContent=JSON.stringify(p);')
        expect(page.locator('#fmd-source-project-status')).to_contain_text('changed')
        checks.append('Committed resource/settings changes invalidate pending output')
        page.evaluate('document.getElementById("fmd-editor").dispatchEvent(new CompositionEvent("compositionstart"))')
        assert page.locator('#btn-save-source-project').is_disabled()
        page.evaluate('document.getElementById("fmd-editor").dispatchEvent(new CompositionEvent("compositionend"))')
        assert page.locator('#btn-save-source-project').is_enabled()
        checks.append('Composition events block and then reenable explicit saves (synthetic event check)')
        page.evaluate('binding => { const n=document.getElementById("fmd-native-runtime"); const p=JSON.parse(n.textContent); p.bindings=binding; n.textContent=JSON.stringify(p); }', 'throw Error("must not execute");')
        page.wait_for_timeout(25)
        _, after = save('after-edit.fmd.json')
        assert after['source'].endswith('changed without input')
        assert 'bindings' not in after and after['options']['title'] == 'Changed'
        checks.append('Subsequent saves use current data and never execute serialized binding text')
        # Reparse a saved shell with transient project controls. This exercises
        # reconstruction, not the native controller's full Save HTML workflow.
        reopened = output / 'reopened-fixture.html'
        reopened.write_text(page.content())
        # A fresh document realm is essential in content mode: set_content on
        # the old page does not reset lexical bindings or old event listeners.
        page.close()
        page = context.new_page()
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.on('request', lambda request: network.append(request.url) if request.url.startswith(('http:', 'https:')) else None)
        if args.mode == 'file':
            page.goto(reopened.as_uri(), wait_until='load')
        else:
            page.set_content(reopened.read_text(), wait_until='load')
        assert page.locator('#fmd-source-project-controls').count() == 1
        assert page.locator('#btn-save-source-project').is_enabled()
        checks.append('Reparsed HTML rebuilds one control set without replaying prior jobs')
        page.set_viewport_size({'width': 390, 'height': 820})
        page.screenshot(path=str(output / 'controls-390.png'), full_page=True)
        assert page.locator('#btn-save-source-project').is_visible()
        assert page.locator('#fmd-source-project-controls').evaluate('(n)=>n.getBoundingClientRect().right <= innerWidth')
        checks.append('Project controls remain visible and contained at a 390-pixel viewport')
        assert not network, network
        assert not errors, errors
        browser.close()
    report = {'mode': args.mode, 'checks': checks, 'passed': len(checks), 'native_renderer': 'explicit adapter, not Rust/WASM',
        'lossless_host': 'explicit fixture, not the full production controller', 'network_requests': network, 'page_errors': errors}
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    print('Retained artifacts:', output)


if __name__ == '__main__':
    main()
