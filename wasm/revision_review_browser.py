#!/usr/bin/env python3
"""Browser integration of the revision workbench and production module worker.

All application routes are fulfilled from this checkout. Only the native renderer
module is replaced by an explicit fixture, so this is UI/worker evidence, not
Rust semantic correctness or rebuilt-WASM acceptance. The default origin transport exercises the shipped module entry; content transport
resolves imports to Blob URLs and starts it through a classic-worker bootstrap.
No installation, uploads, ambient file lookup or external network is performed. Artifacts are retained.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
import time
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
ORIGIN = 'https://fmd-review.test'
FIXTURE = r'''
const utf8 = text => new TextEncoder().encode(text);
export async function createRenderer() {
  return {
    async semanticDiff(old, next, options) {
      if (old.startsWith('SLOW')) {
        const until = performance.now() + 1200; while (performance.now() < until) { /* native CPU double */ }
      }
      if (old === 'ERROR') throw new Error('Deliberate renderer failure');
      return {schema: 'fmd-diff-v1', old_name: options.oldName ?? 'Before', new_name: options.newName ?? 'Current',
        stats: {unchanged_blocks: 1, inserted_blocks: 2, deleted_blocks: 3, modified_blocks: 4,
          words_inserted: 5, words_deleted: 6, similarity_ratio: 0.125},
        fixture: {old, next, options}, additional_native_field: {kept: true}};
    },
    renderSemanticDiff(old, next, options) {
      if (old === 'HTML_ERROR') throw new Error('HTML generation failed');
      const escape = s => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;');
      const html = '<!doctype html><html><head><title>Comparison fixture</title><style>body{font:16px sans-serif}ins{background:#dfd}</style></head><body>'
        + '<h1>' + escape(options.oldName ?? '') + '</h1><del>' + escape(old) + '</del><ins>' + escape(next) + '</ins>'
        + '<img src="https://external.invalid/track"><script>window.parent.reviewExecuted=true;<\/script></body></html>';
      return {format:'diff-html', mimeType:'text/html; charset=utf-8', extension:'html',
        sourceLength: utf8(old).length + utf8(next).length, bytes:utf8(html),
        diagnostics:[{severity:'warning', start:0, end:0, scope:'document', code:'fixture', message:'Explicit renderer double'}]};
    }
  };
}
'''


def wait(page, expression, seconds=8):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if page.evaluate(expression):
            return
        page.wait_for_timeout(15)
    raise AssertionError('Checkpoint not reached: ' + expression + '; status=' + page.locator('#review-status').inner_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium', default='/usr/bin/chromium')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--transport', choices=['origin', 'content'], default='origin',
                        help='content uses explicit blob-resolved modules on hosts blocking test-origin navigation')
    args = parser.parse_args()
    if args.output:
        out = args.output.resolve(); out.mkdir(parents=True, exist_ok=False)
    else:
        out = Path(tempfile.mkdtemp(prefix='fmd-revision-review-'))
    checks, errors, external, downloads, workers = [], [], [], [], []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=args.chromium, headless=True,
                                     args=['--no-sandbox', '--disable-dev-shm-usage'])
        context = browser.new_context(accept_downloads=True, viewport={'width': 1440, 'height': 1000})
        context.set_offline(True)
        # Explicit allowlist; no path traversal or accidental access to unrelated
        # repository files, system paths, credentials or network destinations.
        allowed = {f'/wasm/{name}' for name in ['demo/review.html', 'demo/revision_review.mjs',
                   'document_worker.mjs', 'document_worker_entry.js', 'worker_transport.mjs', 'pdf_page.mjs']}
        def route(request):
            path = urlparse(request.request.url).path
            if request.request.url == ORIGIN + '/wasm/franken_markdown.js':
                request.fulfill(status=200, content_type='text/javascript', body=FIXTURE)
            elif request.request.url.startswith(ORIGIN + '/') and path in allowed:
                location = ROOT / path.lstrip('/')
                request.fulfill(status=200, content_type='text/javascript' if path.endswith(('.mjs', '.js')) else 'text/html',
                                body=location.read_bytes())
            else:
                external.append(request.request.url); request.abort()
        context.route('**/*', route)
        page = context.new_page()
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.on('worker', lambda worker: workers.append(worker.url))
        page.on('download', lambda download: downloads.append(download))
        accept_dialog = lambda dialog: dialog.accept()
        page.on('dialog', accept_dialog)
        if args.transport == 'origin':
            page.goto(ORIGIN + '/wasm/demo/review.html')
        else:
            # Explicit alternate transport, not file/HTTP package acceptance.
            # Resolve production relative imports to private Blob module URLs;
            # only renderer construction is injected through its existing seam.
            html = (ROOT/'wasm/demo/review.html').read_text()
            html = re.sub(r'<script type="module">[\s\S]*?</script>', '', html)
            page.set_content(html)
            modules = {name: (ROOT/'wasm'/name).read_text() for name in [
                'worker_transport.mjs', 'pdf_page.mjs', 'document_worker.mjs',
                'document_worker_entry.js', 'demo/revision_review.mjs']}
            modules['franken_markdown.js'] = FIXTURE
            page.evaluate("""async modules => {
                const urls = {};
                for (const name of ['worker_transport.mjs','pdf_page.mjs','franken_markdown.js',
                    'document_worker.mjs','document_worker_entry.js','demo/revision_review.mjs']) {
                    let code = modules[name];
                    for (const [path, url] of Object.entries(urls)) {
                        for (const prefix of ['./','../']) {
                            code = code.replaceAll('"' + prefix + path + '"', '"' + url + '"')
                                .replaceAll("'" + prefix + path + "'", "'" + url + "'");
                        }
                    }
                    urls[name] = URL.createObjectURL(new Blob([code], {type:'text/javascript'}));
                }
                // Classic-worker startup is supported on opaque content origins
                // where Chromium refuses a module-worker constructor. The
                // production module entry still imports normally; buffer only
                // startup messages until its listener has been installed.
                const start = `const held=[];const hold=e=>held.push(e.data);
                    addEventListener('message',hold);
                    import(${JSON.stringify(urls['document_worker_entry.js'])}).then(()=>{
                      removeEventListener('message',hold);
                      for(const data of held) dispatchEvent(new MessageEvent('message',{data}));
                    });`;
                const workerUrl=URL.createObjectURL(new Blob([start],{type:'text/javascript'}));
                const {createWorkerRenderer} = await import(urls['document_worker.mjs']);
                const {installRevisionReview} = await import(urls['demo/revision_review.mjs']);
                window.reviewTest = installRevisionReview(document, {rendererFactory: () =>
                    createWorkerRenderer({workerFactory: () => new Worker(workerUrl)})});
            }""", modules)
        def check(label):
            checks.append(label); print(f'ok {len(checks)} - {label}', flush=True)
        def source(side, text):
            page.locator('#' + side + '-source').fill(text)
        def compare():
            page.locator('#compare').click()
            wait(page, 'document.querySelector("#review-status").textContent.startsWith("Comparison ready")')
        def save(button, filename):
            with page.expect_download() as event:
                page.locator('#' + button).click()
            item = event.value
            assert item.suggested_filename == filename, item.suggested_filename
            destination = out / f'{len(downloads)}-{filename}'; item.save_as(destination)
            return destination.read_bytes()
        wait(page, '!document.querySelector("#compare").disabled')
        assert workers == []
        assert page.locator('#download-review-json').is_disabled()
        check('loading the workbench starts no renderer or worker')
        # Both sources absent is a valid comparison, not an error or missing input.
        compare()
        packet = json.loads(save('download-review-json', 'revision-comparison.json'))
        assert packet['fixture']['old'] == packet['fixture']['next'] == ''
        assert page.locator('#review-summary dd').all_text_contents() == ['1', '2', '3', '4', '5', '6', '12.5%']
        check('empty revisions compare successfully and expose the native structural metrics')
        before = '\ufeff# Old\r\n\r\n中𝄞\r\n'
        after = '\ufeff# New\r\n\r\n**Changed**\r\n'
        for side, text in [('before', before), ('after', after)]:
            page.locator('#' + side + '-file').set_input_files({'name': side + '.md', 'mimeType': 'text/markdown', 'buffer': text.encode('utf-8')})
            wait(page, f'document.querySelector("#review-status").textContent.startsWith("Opened {side}")')
        compare()
        packet = json.loads(save('download-review-json', 'revision-comparison.json'))
        assert packet['fixture'] == {'old': before, 'next': after, 'options': {'oldName': 'before.md', 'newName': 'after.md'}}
        assert packet['additional_native_field'] == {'kept': True}
        assert save('save-before', 'before.md.md') == before.encode('utf-8')
        assert save('save-after', 'after.md.md') == after.encode('utf-8')
        check('file imports and source/report downloads preserve exact BOM, CRLF and non-BMP text')
        before_dom = page.locator('#before-source').input_value()
        # Invalid file should not overwrite the prior revision or start WASM.
        page.locator('#before-file').set_input_files({'name': 'corrupt.md', 'mimeType': 'text/markdown', 'buffer': b'\xff\xfe'})
        wait(page, 'document.querySelector("#review-status").textContent.startsWith("File not opened")')
        assert page.locator('#before-source').input_value() == before_dom
        assert page.locator('#download-review-json').is_disabled()
        check('invalid UTF-8 import fails visibly without replacing source')
        page.locator('#before-name').fill('<img src=x onerror=alert(1)>')
        compare()
        frame = page.frame_locator('#review-preview')
        assert frame.locator('h1').inner_text() == '<img src=x onerror=alert(1)>'
        assert not page.evaluate('window.reviewExecuted === true')
        assert page.locator('#review-preview').get_attribute('sandbox') == ''
        guarded = save('download-review-html', 'revision-comparison.html').decode('utf-8')
        assert guarded.index('Content-Security-Policy') < guarded.index('<title>')
        assert "default-src 'none'" in guarded
        assert external == [], external
        check('preview and HTML download carry a resource-denying CSP and cannot execute document script')
        page.locator('#swap-revisions').click()
        compare()
        packet = json.loads(save('download-review-json', 'revision-comparison.json'))
        assert packet['fixture']['old'] == after and packet['fixture']['next'] == before
        assert packet['fixture']['options']['newName'] == '<img src=x onerror=alert(1)>'
        check('swap exchanges exact revisions and labels, not normalized textarea copies')
        page.remove_listener('dialog', accept_dialog)
        page.once('dialog', lambda dialog: dialog.dismiss())
        page.locator('#before-file').set_input_files({'name':'declined.md', 'mimeType':'text/markdown', 'buffer':b'# Declined'})
        wait(page, 'document.querySelector("#review-status").textContent.startsWith("File opening declined")')
        assert save('save-before', 'after.md.md') == after.encode('utf-8')
        page.on('dialog', accept_dialog)
        check('declining a file replacement preserves the prior lossless source')
        # Real blocking computation happens in the actual production module
        # worker. The only fixture is its imported native-renderer implementation.
        for action in ['cancel', 'edit', 'silent-edit', 'round-trip', 'label', 'composition', 'pagehide']:
            source('before', 'SLOW ' + action); source('after', '# After')
            count = len(downloads)
            page.locator('#compare').click()
            wait(page, 'document.querySelector("#review-status").textContent.includes("native worker")')
            page.evaluate('window.reviewHeartbeat = false; setTimeout(() => {window.reviewHeartbeat=true}, 30)')
            wait(page, 'window.reviewHeartbeat', seconds=1)
            if action == 'cancel': page.locator('#cancel-review').click()
            elif action == 'edit': source('after', '# Newer revision')
            elif action == 'silent-edit': page.evaluate('document.querySelector("#after-source").value = "# Silent edit"')
            elif action == 'round-trip':
                page.evaluate('''() => {const e=document.querySelector('#after-source'), s=e.value;
                  e.value='# Temporary';e.dispatchEvent(new Event('input'));e.value=s;e.dispatchEvent(new Event('input'));}''')
            elif action == 'label': page.locator('#after-name').fill('New label')
            elif action == 'composition':
                page.evaluate('document.querySelector("#before-source").dispatchEvent(new CompositionEvent("compositionstart"))')
                assert page.locator('#compare').is_disabled()
                page.evaluate('document.querySelector("#before-source").dispatchEvent(new CompositionEvent("compositionend"))')
            else:
                page.evaluate('window.dispatchEvent(new PageTransitionEvent("pagehide"))')
                assert page.locator('#compare').is_disabled()
                page.evaluate('window.dispatchEvent(new PageTransitionEvent("pageshow"))')
            wait(page, '!document.querySelector("#compare").disabled')
            assert page.locator('#download-review-json').is_disabled()
            assert page.locator('#review-results').is_hidden()
            assert len(downloads) == count
            check(action + ' retires unfinished work without blocking source editing or publishing stale output')
        source('before', 'ERROR'); source('after', '# Safe')
        page.locator('#compare').click()
        wait(page, 'document.querySelector("#review-status").textContent.startsWith("Comparison failed")')
        assert save('save-before', 'after.md.md') == b'ERROR'
        assert page.locator('#review-results').is_hidden()
        source('before', '# Retry'); compare()
        assert json.loads(save('download-review-json', 'revision-comparison.json'))['fixture']['old'] == '# Retry'
        check('native failure retains sources and an explicit retry creates a working fresh renderer')
        # A completed result is also guarded, not only an in-flight response.
        page.evaluate('document.querySelector("#after-source").value = "# Silently changed after success"')
        count = len(downloads); page.locator('#download-review-json').click()
        assert len(downloads) == count
        assert page.locator('#review-results').is_hidden()
        check('download activation independently rejects edits made after the result completed')
        # Hold a real FileReader completion while editing source. Late callbacks
        # must not replace a new revision or clear a newer import's ownership.
        page.evaluate('''() => {
          const original=FileReader.prototype.readAsArrayBuffer;
          window.restoreReader=()=>{FileReader.prototype.readAsArrayBuffer=original;};
          FileReader.prototype.readAsArrayBuffer=function(file){window.releaseRead=()=>original.call(this,file);};
        }''')
        page.locator('#before-file').set_input_files({'name':'late.md','mimeType':'text/markdown','buffer':b'# Late file'})
        source('before', '# Edit during file read')
        page.evaluate('window.releaseRead();window.restoreReader();')
        page.wait_for_timeout(200)
        assert page.locator('#before-source').input_value() == '# Edit during file read'
        check('a stale file-read completion cannot overwrite intervening source edits')
        page.evaluate('''() => {
          const original=FileReader.prototype.readAsArrayBuffer;
          window.restoreReader=()=>{FileReader.prototype.readAsArrayBuffer=original;};
          FileReader.prototype.readAsArrayBuffer=function(file){window.releaseRead=()=>original.call(this,file);};
        }''')
        page.locator('#after-file').set_input_files({'name':'silent-read.md','mimeType':'text/markdown','buffer':b'# Late file'})
        page.evaluate('document.querySelector("#after-source").value = "# Silent edit during read";window.releaseRead();window.restoreReader();')
        wait(page, 'document.querySelector("#review-status").textContent.startsWith("File opening discarded")')
        assert page.locator('#after-source').input_value() == '# Silent edit during read'
        assert page.locator('#compare').is_enabled()
        check('silent edits during a file read are reported without replacing source')
        # Use a scoped value getter: isolates controller admission from the
        # unrelated cost of laying out a four-megabyte browser textarea.
        page.evaluate('''() => {const e=document.querySelector('#before-source');
          Object.defineProperty(e,'value',{configurable:true,get(){return 'x'.repeat(4*1024*1024+1)}});}''')
        prior_workers = len(workers); page.locator('#compare').click()
        wait(page, 'document.querySelector("#review-status").textContent.includes("UTF-8 byte limit")')
        assert len(workers) == prior_workers
        page.evaluate('delete document.querySelector("#before-source").value')
        check('oversized source fails before worker startup')
        assert errors == [], errors
        assert external == [], external
        source('before', '# Screenshot before'); source('after', '# Screenshot after'); compare()
        assert page.frame_locator('#review-preview').locator('ins').inner_text() == '# Screenshot after'
        page.screenshot(path=str(out / 'review.png'), full_page=True)
        page.set_viewport_size({'width':390, 'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
        assert page.locator('#compare').is_visible()
        page.screenshot(path=str(out / 'review-mobile.png'), full_page=True)
        check('the two-source workbench remains usable without horizontal overflow on a narrow viewport')
        receipt = {'passed':len(checks), 'checks':checks, 'browser':browser.version,
                   'native_backend':'explicit renderer fixture', 'transport':args.transport,
                   'workers_started':len(workers), 'external_requests':external, 'page_errors':errors,
                   'source_hashes':{str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest()
                                    for path in [ROOT/'wasm/demo/revision_review.mjs', ROOT/'wasm/document_worker.mjs']}}
        (out/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
        print(json.dumps({'passed':len(checks), 'artifacts':str(out)}), flush=True)
        browser.close()


if __name__ == '__main__':
    main()
