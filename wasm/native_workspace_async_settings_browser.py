#!/usr/bin/env python3
"""Exercise the production settings controller in an installed Chromium.

The native runtime is an explicit transaction/API fixture. Its settings render
uses a real CPU-blocking Blob worker; this checks controller integration, not
Rust rendering, the production transaction implementation, or WASM acceptance.
No network access or dependencies are installed. Artifacts are retained.
"""
from __future__ import annotations

import argparse
import json
import tempfile
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SOURCE = '\ufeff# Original\r\n\r\n![chart](chart.png) 中𝄞\r\n'
BOOTSTRAP = r'''
(() => {
  const data = document.querySelector('#fmd-fixture-state');
  const payload = JSON.parse(data.textContent);
  let settings = Object.freeze(payload.options), pending = null;
  const probe = window.settingsProbe = {calls: [], syncCalls: 0, commits: 0, stopped: 0, exports: [], mode: 'success'};
  const error = (code, message) => Object.assign(new Error(message), {code});
  const escape = text => String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  function preview(source, options, node) {
    const frame = document.createElement('iframe'); frame.setAttribute('sandbox', 'allow-same-origin');
    frame.srcdoc = '<html><head></head><body><h1>' + escape(options.title) + '</h1><pre>' + escape(source) + '</pre></body></html>';
    node.replaceChildren(frame);
  }
  function commit(next, source, node) {
    const oldData = data.textContent, oldChildren = [...node.childNodes];
    try {
      if (probe.mode === 'publication-failure') throw Error('Fixture publication failure');
      data.textContent = JSON.stringify({...payload, options: next}).replace(/</g, '\\u003c');
      preview(source, next, node);
    } catch (error) { data.textContent = oldData; node.replaceChildren(...oldChildren); throw error; }
    settings = Object.freeze(next); payload.options = settings; probe.commits++;
    return settings;
  }
  function cancel() {
    const job = pending;
    if (!job) return;
    pending = null; clearTimeout(job.timer); job.worker.terminate(); probe.stopped++;
    job.reject(error('SETTINGS_CANCELLED', 'Settings cancelled by fixture runtime'));
  }
  const native = {
    version: 1, ready: Promise.resolve(true), previewMode: 'worker', exportMode: 'worker', settingsMode: payload.mode,
    get settings() { return settings; }, get settingsPending() { return pending !== null; },
    get exportPending() { return false; }, get diagnostics() { return []; },
    exportFormats: ['html', 'pdf', 'epub', 'svg'], analysisFormats: ['stats'],
    cancelSettings: cancel, cancelExport() {}, cancelAnalysis() {},
    invalidatePreview: cancel, restartPreview() { cancel(); },
    render(source, node, display, current) {
      if (current()) { preview(source, settings, node); return Promise.resolve(true); }
      return Promise.resolve(false);
    },
    html() { return '<html><head></head><body>Fixture</body></html>'; },
    pdf() { return new TextEncoder().encode('%PDF-fixture'); },
    applySettings(patch, source, node, display) {
      probe.syncCalls++;
      if (payload.mode === 'worker') throw Error('Synchronous rendering called in a worker workspace');
      return commit({...settings, ...patch}, source, node);
    },
    applySettingsAsync(patch, source, node, display, current) {
      probe.calls.push({patch, source, display});
      if (probe.mode === 'throw') throw Error('Fixture synchronous admission failure');
      if (probe.mode === 'no-promise') return false;
      if (pending) throw error('SETTINGS_BUSY', 'Fixture slot occupied');
      const before = settings, next = {...settings, ...patch};
      for (const key of Object.keys(next)) if (next[key] === undefined) delete next[key];
      const workerSource = `self.onmessage = ({data}) => {
        const until = performance.now() + data.delay; while (performance.now() < until) {}
        self.postMessage(data.mode === 'fail' ? {error: 'Fixture native render failure'} : {ok: true});
      };`;
      const url = URL.createObjectURL(new Blob([workerSource], {type: 'text/javascript'}));
      const worker = new Worker(url); URL.revokeObjectURL(url);
      const mode = probe.mode;
      return new Promise((resolve, reject) => {
        const job = {worker, reject, timer: null}; pending = job;
        const settle = (failure, result) => {
          if (pending !== job) return;
          pending = null; clearTimeout(job.timer); worker.terminate(); probe.stopped++;
          if (failure) reject(failure); else resolve(result);
        };
        worker.onerror = () => settle(Error('Fixture worker failure'));
        worker.onmessage = event => {
          if (pending !== job) return;
          try {
            probe.lastCurrent = current();
            if (!probe.lastCurrent || settings !== before) throw error('SETTINGS_CANCELLED', 'Source, view or draft changed');
            if (event.data.error) throw Error(event.data.error);
            settle(null, commit(next, source, node));
          } catch (failure) { settle(failure); }
        };
        job.timer = setTimeout(() => settle(error('PREVIEW_TIMEOUT', 'Fixture worker deadline exceeded')), mode === 'timeout' ? 150 : 10000);
        worker.postMessage({mode, delay: ['slow', 'timeout'].includes(mode) ? 3000 : 100});
      });
    },
    async exportDocument(format, source, current) {
      if (pending) throw error('EXPORT_BUSY', 'Settings in progress');
      if (!current()) throw error('EXPORT_CANCELLED', 'Source changed');
      probe.exports.push({format, source, settings});
      return {format, mimeType: {pdf: 'application/pdf', html: 'text/html;charset=utf-8', epub: 'application/epub+zip', svg: 'image/svg+xml'}[format],
        bytes: new TextEncoder().encode(JSON.stringify({source, settings})), diagnostics: []};
    },
    async analyzeDocument(kind, source, current) {
      if (pending) throw error('ANALYSIS_BUSY', 'Settings in progress');
      return {kind, mimeType: 'application/json', bytes: new TextEncoder().encode('{}'), report: {
        words: 1, characters: 1, bytes: source.length, lines: 1, reading_time_secs: 1, speaking_time_secs: 1,
        flesch_reading_ease: 100, flesch_kincaid_grade: 0, reading_ease_label: 'Fixture', structure: {}, outline: [], findings: []}};
    },
  };
  if (payload.missing) delete native.applySettingsAsync;
  Object.defineProperty(window, '__fmdNativeRuntime', {value: native});
})();
'''


def fixture(controller: str, mode: str = 'worker', missing: bool = False) -> str:
    def data(value):
        return json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')
    payload = {'mode': mode, 'missing': missing, 'options': {
        'font': 'sans', 'darkMode': 'disabled', 'fontScale': 1,
        'title': 'Original', 'author': 'Exact\r\nAuthor', 'lang': 'en',
        'pageNumbers': False, 'codeLineNumbers': False, 'toc': False, 'metadataEpochSeconds': 0},
        'images': [{'destination': 'chart.png', 'bytes': 'AQID'}],
        'fonts': [{'slot': 'body-regular', 'bytes': 'BAU=', 'weight': 650}]}
    ids = ['btn-save-markdown', 'btn-save-html', 'btn-toggle-view', 'btn-zoom-in', 'btn-zoom-out',
           'btn-zoom-reset', 'btn-theme-toggle', 'btn-stats-toggle', 'btn-export-pdf']
    return '''<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Original</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' blob:; worker-src blob:; frame-src 'self' about:; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'">
<style>body{font:16px sans-serif;margin:16px}header{display:flex;flex-wrap:wrap;gap:8px}button{padding:8px}textarea{width:90%;height:160px}#stats-drawer{display:none}</style></head><body>
<header class="fmd-app-header"><span class="fmd-title">Original</span>''' + ''.join(f'<button id="{id}">{id}</button>' for id in ids) + '''</header>
<div id="fmd-app-body" class="view-split"><div id="editor-pane"><div class="fmd-pane-header"><span id="fmd-save-status"></span></div><textarea id="fmd-editor"></textarea><span id="source-line-count"></span></div><main><div id="fmd-content"></div></main></div>
<span id="view-mode-icon"></span><span id="view-mode-label"></span>
<div id="stats-drawer"><span id="stat-words"></span><span id="stat-chars"></span><span id="stat-read-time"></span><span id="stat-readability"></span><button id="btn-stats-close"></button></div>
<script id="fmd-raw-source" type="application/json">''' + data(SOURCE) + '''</script>
<script id="fmd-fixture-state" type="application/json">''' + data(payload) + '''</script>
<script>''' + BOOTSTRAP + '</script><script>' + controller + '</script></body></html>'


def wait(page, expression: str, seconds: float = 8) -> None:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if page.evaluate(expression):
            return
        page.wait_for_timeout(20)
    raise AssertionError('Checkpoint not reached: ' + expression)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium', default='/usr/bin/chromium')
    parser.add_argument('--controller', type=Path, default=ROOT / 'src/interactive_controller.js')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    directory = args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix='fmd-async-settings-'))
    if args.output:
        directory.mkdir(parents=True, exist_ok=False)
    controller = args.controller.read_text()
    checks, errors, requests = [], [], []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1440, 'height': 1000})
        context.set_offline(True)
        context.on('request', lambda r: requests.append(r.url) if r.url.startswith(('http:', 'https:')) else None)
        def open_page(mode='worker', missing=False, content=None):
            p = context.new_page()
            p.on('pageerror', lambda e: errors.append(str(e)))
            p.set_content(content or fixture(controller, mode, missing))
            wait(p, 'document.querySelector("#btn-document-settings")?.disabled === false')
            wait(p, 'document.querySelector("#btn-document-lab") !== null')
            return p
        def check(label):
            checks.append(label); print(f'ok {len(checks)} - {label}', flush=True)
        def panel(p, title=None):
            if p.locator('#fmd-document-settings').is_hidden():
                p.locator('#btn-document-settings').click()
            if title is not None:
                p.locator('#fmd-document-settings [name="title"]').fill(title)
        def apply(p):
            p.locator('#fmd-document-settings button[type="submit"]').click()
        def settled(p):
            wait(p, '!window.__fmdNativeRuntime.settingsPending')
            wait(p, 'document.querySelector("#fmd-document-settings button[type=submit]").disabled === false')
        def title(p):
            return p.evaluate('window.__fmdNativeRuntime.settings.title')
        def download(p, id):
            with p.expect_download() as event:
                p.evaluate('(id) => document.getElementById(id).click()', id)
            item = event.value
            path = directory / f'{len(list(directory.iterdir()))}-{item.suggested_filename}'
            item.save_as(path)
            return path.read_bytes()
        p = open_page()
        baseline = p.locator('#fmd-fixture-state').text_content()
        panel(p); apply(p)
        assert p.locator('#fmd-document-settings').is_hidden()
        assert p.evaluate('settingsProbe.calls.length') == 0
        assert p.locator('#fmd-fixture-state').text_content() == baseline
        check('unchanged Apply makes no native call and preserves omitted metadata/default paper')
        p.evaluate('settingsProbe.mode = "slow"')
        panel(p, 'Committed')
        p.locator('[name="paper"]').select_option('custom')
        for name, value in [('width', '700.125'), ('height', '850.25'), ('top', '21.5'), ('right', '22.5'), ('bottom', '23.5'), ('left', '24.5')]:
            p.locator(f'[name="{name}"]').fill(value)
        original_preview = p.locator('#fmd-content').inner_html()
        apply(p)
        wait(p, 'window.__fmdNativeRuntime.settingsPending')
        assert title(p) == 'Original' and p.title() == 'Original'
        assert p.locator('#fmd-content').inner_html() == original_preview
        assert p.locator('#fmd-fixture-state').text_content() == baseline
        assert p.locator('#btn-export-pdf').is_disabled() and p.locator('#btn-lab-stats').is_disabled()
        p.evaluate('window.heartbeat = false; setTimeout(() => {window.heartbeat = true}, 25)')
        wait(p, 'window.heartbeat', seconds=1)
        assert p.evaluate('window.__fmdNativeRuntime.settingsPending')
        # Repeated submissions must not replace the running operation or queue.
        p.evaluate('document.querySelector("#fmd-document-settings form").dispatchEvent(new Event("submit", {bubbles:true,cancelable:true}))')
        assert p.evaluate('settingsProbe.calls.length') == 1
        check('Apply uses the worker, keeps committed data/preview intact and leaves the UI event loop responsive')
        settled(p)
        assert p.locator('#fmd-document-settings').is_hidden()
        assert title(p) == 'Committed' and p.title() == 'Committed'
        call = p.evaluate('settingsProbe.calls[0]')
        assert call['source'] == SOURCE
        assert call['patch']['pageGeometry'] == [700.125, 850.25, 21.5, 22.5, 23.5, 24.5]
        assert 'author' not in call['patch']
        assert p.evaluate('__fmdNativeRuntime.settings.author') == 'Exact\r\nAuthor'
        assert p.evaluate('settingsProbe.syncCalls') == 0
        assert p.locator('#btn-export-pdf').is_enabled() and p.locator('#btn-lab-stats').is_enabled()
        check('successful preflight commits exact source, fractional geometry and metadata only after completion')
        committed = p.locator('#fmd-fixture-state').text_content()
        # Keyboard saves must use committed settings, not an unfinished form.
        panel(p, 'Must not be saved')
        apply(p); wait(p, '__fmdNativeRuntime.settingsPending')
        saved = download(p, 'btn-save-html')
        restored = open_page(content=saved.decode('utf-8'))
        assert title(restored) == 'Committed'
        assert restored.locator('#fmd-document-settings').is_hidden()
        assert restored.locator('#btn-document-settings').count() == 1
        assert restored.locator('#btn-export-pdf').is_enabled()
        p.locator('[data-cancel]').click(); settled(p)
        check('Save HTML during preflight retains committed settings and reopens with working controls')
        for action in ['cancel', 'escape', 'draft-edit', 'change-event', 'close', 'source-input', 'source-round-trip', 'view', 'composition', 'pagehide']:
            panel(p, 'Discard ' + action); apply(p); wait(p, '__fmdNativeRuntime.settingsPending')
            if action == 'cancel': p.locator('[data-cancel]').click()
            elif action == 'escape': p.keyboard.press('Escape')
            elif action == 'draft-edit': p.locator('[name="title"]').fill('New unapplied draft')
            elif action == 'change-event':
                p.evaluate('document.querySelector("[name=title]").dispatchEvent(new Event("change", {bubbles:true}))')
            elif action == 'close': p.evaluate('document.querySelector("#fmd-document-settings").close()')
            elif action in ('source-input', 'source-round-trip'):
                p.evaluate('''(roundtrip) => {const e=document.querySelector('#fmd-editor'), old=e.value;
                  e.value='# New source'; e.dispatchEvent(new Event('input', {bubbles:true}));
                  if(roundtrip){e.value=old;e.dispatchEvent(new Event('input', {bubbles:true}));}}''', action == 'source-round-trip')
            elif action == 'view': p.evaluate('document.querySelector("#btn-zoom-in").click()')
            elif action == 'composition':
                p.evaluate('document.querySelector("#fmd-editor").dispatchEvent(new CompositionEvent("compositionstart"))')
            else: p.evaluate('window.dispatchEvent(new PageTransitionEvent("pagehide"))')
            settled(p)
            assert title(p) == 'Committed'
            assert p.locator('#fmd-fixture-state').text_content() == committed
            assert p.locator('#btn-export-pdf').is_enabled()
            if action == 'composition': p.evaluate('document.querySelector("#fmd-editor").dispatchEvent(new CompositionEvent("compositionend"))')
            if action == 'pagehide': p.evaluate('window.dispatchEvent(new PageTransitionEvent("pageshow"))')
            if p.locator('#fmd-document-settings').is_visible(): p.locator('[data-cancel]').click()
            check(action + ' cancels preflight without committing the draft')
        for action in ['silent-source', 'silent-draft', 'silent-geometry']:
            panel(p, 'Silent rejected ' + action); apply(p); wait(p, '__fmdNativeRuntime.settingsPending')
            if action == 'silent-source': p.evaluate('document.querySelector("#fmd-editor").value = "# Silent source"')
            elif action == 'silent-draft': p.evaluate('document.querySelector("[name=title]").value = "Silent draft"')
            else: p.evaluate('document.querySelector("[name=width]").value = "900"')
            settled(p)
            assert p.evaluate('settingsProbe.lastCurrent') is False
            assert title(p) == 'Committed'
            assert 'not applied' in p.locator('[data-status]').inner_text()
            p.locator('[data-cancel]').click()
            check(action + ' is rejected by the pre-publication identity callback')
        for mode in ['fail', 'timeout', 'throw', 'no-promise', 'publication-failure']:
            p.evaluate('(mode) => {settingsProbe.mode = mode}', mode)
            panel(p, 'Retry ' + mode); apply(p); settled(p)
            assert p.locator('#fmd-document-settings').is_visible()
            assert 'not applied' in p.locator('[data-status]').inner_text()
            assert title(p) == 'Committed'
            assert p.locator('#fmd-fixture-state').text_content() == committed
            assert p.evaluate('settingsProbe.syncCalls') == 0
            assert p.locator('#btn-export-pdf').is_enabled()
            p.locator('[data-cancel]').click()
            check(mode + ' restores controls, preserves committed data and never falls back to main-thread rendering')
        p.evaluate('settingsProbe.mode = "slow"')
        panel(p, 'Old cancelled'); apply(p); wait(p, '__fmdNativeRuntime.settingsPending')
        # Exercise queued close event followed immediately by a new open/apply.
        p.evaluate('''() => {document.querySelector('[data-cancel]').click(); document.querySelector('#btn-document-settings').click();
          const input=document.querySelector('[name=title]');input.value='New surviving';input.dispatchEvent(new Event('input',{bubbles:true}));
          settingsProbe.mode='success';document.querySelector('#fmd-document-settings form').requestSubmit();}''')
        settled(p)
        assert title(p) == 'New surviving'
        assert p.locator('#fmd-document-settings').is_hidden()
        check('late completion and queued close from a cancelled job cannot close or cancel a newer settings operation')
        packet = json.loads(download(p, 'btn-export-pdf'))
        assert packet['settings']['title'] == 'New surviving'
        check('the next explicit publication uses the successfully committed settings')
        old = open_page('synchronous'); panel(old, 'Legacy works'); apply(old)
        assert title(old) == 'Legacy works' and old.evaluate('settingsProbe.syncCalls') == 1
        assert old.evaluate('settingsProbe.calls.length') == 0
        check('explicitly synchronous legacy workspaces keep their prior settings behavior')
        mismatch = open_page(missing=True); panel(mismatch, 'Unsupported'); apply(mismatch)
        assert title(mismatch) == 'Original'
        assert 'matching workspace runtime' in mismatch.locator('[data-status]').inner_text()
        assert mismatch.evaluate('settingsProbe.syncCalls') == 0
        check('worker capability mismatches fail explicitly instead of silently blocking the UI')
        assert requests == [], requests
        assert errors == [], errors
        check('controller checks complete offline without page errors or external HTTP requests')
        receipt = {'passed': len(checks), 'checks': checks, 'browser': browser.version,
                   'transport': 'content', 'native_runtime': 'explicit transaction/API fixture',
                   'worker': 'real CPU-blocking Blob worker', 'page_errors': errors, 'external_requests': requests}
        (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt), flush=True)
        browser.close()


if __name__ == '__main__':
    main()
