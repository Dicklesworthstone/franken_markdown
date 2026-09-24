#!/usr/bin/env python3
"""Real dedicated-worker integration with production editor/exporter/runtime.

The shell and Rust ABI are explicit test adapters and WASM is an actual empty
module. No Rust compilation, PDF quality, real bfcache or file:// compatibility
is implied by --mode content. All artifacts are retained; no cleanup is run.
"""
import argparse
import asyncio
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parent.parent
SOURCE = '\ufeff# Preview\r\n\r\nStart 😀\r'
READ = """() => { const f=document.querySelector('#fmd-content iframe');
const p=f?.contentDocument?.querySelector('#result'); return p ? JSON.parse(p.textContent) : null; }"""

async def run(args):
    out = Path(args.output) if args.output else Path(tempfile.mkdtemp(prefix='fmd-preview-browser-'))
    out.mkdir(parents=True, exist_ok=True)
    out = out.resolve()
    fixture = out / 'fixture.html'
    subprocess.run(['node', str(ROOT / 'wasm/native_workspace_preview_fixture.mjs'), str(fixture)], check=True, cwd=ROOT)
    html = fixture.read_text()
    checks, errors, network, messages, workers = [], [], [], [], []
    report = {'mode': args.mode, 'fixture': str(fixture), 'checks': checks,
              'limitations': ['Empty WASM and explicit shell/native ABI adapters; no rebuilt Rust renderer.',
                              'Lifecycle events are synthetic, not real bfcache.',
                              'Content-mode reopening is not file navigation.']}
    def check(condition, name):
        if not condition:
            raise AssertionError(name)
        checks.append(name)
    async with async_playwright() as pw:
        browser = await pw.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox'])
        report['browser'] = browser.version
        context = await browser.new_context(accept_downloads=True)
        context.on('request', lambda req: network.append(req.url) if req.url.startswith(('https:', 'http:')) else None)
        async def opened(content=html, injection=''):
            page = await context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('console', lambda message: messages.append(message.text))
            page.on('worker', lambda worker: workers.append(worker))
            if injection:
                # Only test instrumentation; the production CSP is not removed,
                # weakened or bypassed. Deadline tests explicitly alter host time.
                content = content.replace('<head>', '<head><script>' + injection + '</script>', 1)
            if args.mode == 'file':
                path = out / f'page-{len(context.pages)}.html'
                path.write_text(content)
                await page.goto(path.as_uri())
            else:
                await page.set_content(content)
            return page
        async def result(page, source, worker=True):
            await page.wait_for_function("""({source,worker}) => {
const p=document.querySelector('#fmd-content iframe')?.contentDocument?.querySelector('#result');
if(!p) return false; const value=JSON.parse(p.textContent); return value.source===source && value.worker===worker; }""",
                arg={'source': source, 'worker': worker}, timeout=10000)
            return await page.evaluate(READ)
        async def download(page, selector, name):
            async with page.expect_download() as event:
                await page.locator(selector).click()
            item = await event.value
            target = out / name
            await item.save_as(target)
            return target.read_bytes()
        async def wait_message(text, start):
            end = time.monotonic() + 5
            while text not in messages[start:]:
                if time.monotonic() > end:
                    raise AssertionError('Missing browser console marker: ' + text)
                await asyncio.sleep(.01)
        try:
            page = await opened()
            first = await result(page, SOURCE)
            check(len(workers) == 1, 'Startup creates one real dedicated Blob worker')
            check(await page.evaluate('() => (window.__fixtureCoreCalls ?? []).length') == 0,
                  'Initial live preview never invokes the main-thread HTML ABI')
            check(first['font'] == 'serif' and first['scale'] == 1.25 and first['fontBytes'] == [4, 5],
                  'Worker receives committed typography and exact font views')
            check(first['destinations'] == ['fixture.png'] and first['images'] == [1, 2, 3],
                  'Worker receives explicit image bindings')
            check(await page.locator('#fmd-content iframe').get_attribute('sandbox') == 'allow-same-origin',
                  'Preview retains its script-free sandbox')
            check('worker-src blob:' in html, 'Exported CSP explicitly permits local Blob workers')
            check(await page.locator('#btn-restart-preview').is_enabled(), 'Explicit restart control is available after initialization')
            editor = page.locator('#fmd-editor')
            await page.evaluate("""() => { window.__presented=[]; new MutationObserver(() => {
const f=document.querySelector('#fmd-content iframe'); window.__presented.push(f?.srcdoc ?? '');
}).observe(document.getElementById('fmd-content'), {childList:true}); }""")
            start = len(messages)
            await editor.fill('slow: obsolete')
            await wait_message('fixture-render-start', start)
            await editor.fill('newest while busy 😀')
            check('fixture-render-end' not in messages[start:], 'Typing completes before the busy worker returns')
            check(await editor.input_value() == 'newest while busy 😀', 'Editor retains input while worker is blocked')
            await result(page, 'newest while busy 😀')
            check(not any('slow: obsolete' in value for value in await page.evaluate('window.__presented')),
                  'Superseded slow results are never installed')
            check(await page.evaluate('() => (window.__fixtureCoreCalls ?? []).length') == 0,
                  'Live editing stays off the main-thread HTML ABI')

            start = len(messages)
            await editor.fill('slow: direct mutation')
            await wait_message('fixture-render-start', start)
            await page.evaluate("document.getElementById('fmd-editor').value='undispatched mutation'")
            await wait_message('fixture-render-end', start)
            await page.wait_for_timeout(100)
            check((await page.evaluate(READ))['source'] == 'newest while busy 😀',
                  'An undispatched source change prevents stale publication')
            await editor.dispatch_event('input')
            await result(page, 'undispatched mutation')

            await editor.dispatch_event('compositionstart')
            await editor.fill('composing 中')
            await page.wait_for_timeout(220)
            check((await page.evaluate(READ))['source'] == 'undispatched mutation',
                  'IME composition suppresses intermediate preview publication')
            await editor.dispatch_event('compositionend')
            await result(page, 'composing 中')
            await page.locator('#btn-zoom-in').click()
            await page.wait_for_function("() => JSON.parse(document.querySelector('#fmd-content iframe').contentDocument.querySelector('#result').textContent).scale > 1.3")
            check((await page.evaluate(READ))['scale'] == 1.375, 'Zoom reflows unchanged source in the worker')
            await page.locator('#btn-theme-toggle').click()
            await page.locator('#btn-theme-toggle').click()
            await page.wait_for_function("() => JSON.parse(document.querySelector('#fmd-content iframe').contentDocument.querySelector('#result').textContent).mode === 'disabled'")
            check((await page.evaluate(READ))['mode'] == 'disabled', 'Theme changes supersede older view requests')

            await editor.fill('worker-fail')
            await page.wait_for_function("() => document.getElementById('fmd-save-status').textContent.includes('Deliberate worker-only failure')")
            check('Deliberate worker-only failure' in await page.locator('#fmd-save-status').inner_text(),
                  'Worker failure is visible and retryable')
            check((await page.evaluate(READ))['source'] == 'composing 中', 'Failed worker leaves the last successful preview intact')
            markdown = await download(page, '#btn-save-markdown', 'failure-source.md')
            check(markdown == b'worker-fail', 'Source remains downloadable during preview failure')
            await editor.fill('recovered')
            await result(page, 'recovered')
            check('Unable' not in await page.locator('#fmd-save-status').inner_text(), 'New input recovers a nonterminal render failure without synchronous fallback')

            start = len(messages)
            await editor.fill('slow: workspace save')
            await wait_message('fixture-render-start', start)
            saved = await download(page, '#btn-save-html', 'saved-workspace.html')
            check((await page.evaluate(READ))['worker'] is True, 'Save HTML awaits a matching worker-rendered preview')
            check((await page.evaluate(READ))['source'] == 'slow: workspace save', 'Saved workspace uses current source, not the previous preview')
            check(b'id="btn-retry-preview"' not in saved, 'Transient restart controls are not serialized into saved markup')
            await wait_message('fixture-render-end', start)
            await page.wait_for_timeout(100)
            check((await page.evaluate(READ))['worker'] is True, 'Save HTML never invokes a synchronous render fallback')
            seen = []
            page.on('download', lambda event: seen.append(event.suggested_filename))
            start = len(messages)
            await editor.fill('slow: superseded save')
            await wait_message('fixture-render-start', start)
            await page.locator('#btn-save-html').click()
            await editor.fill('newer than pending save')
            await result(page, 'newer than pending save')
            check(seen == [], 'An edit cancels the superseded pending workspace download')
            check(await page.evaluate('() => (window.__fixtureCoreCalls ?? []).length') == 0,
                  'Asynchronous Save HTML never invokes the main-thread HTML ABI')
            reopened = await opened(saved.decode())
            await result(reopened, 'slow: workspace save')
            await reopened.locator('#fmd-editor').fill('fresh reopened revision')
            await result(reopened, 'fresh reopened revision')
            check(await reopened.evaluate('() => (window.__fixtureCoreCalls ?? []).length') == 0,
                  'Reopened saved workspace initializes its own worker and continues background editing')
            check(await reopened.locator('#btn-restart-preview').count() == 1, 'Reopening installs exactly one restart control')
            published = await download(reopened, '#btn-publish-html', 'published.html')
            check(b'fresh reopened revision' in published and b'fmd-native-runtime' not in published,
                  'Publishing remains a separate current-source document without workspace payload')

            before_workers = len(workers)
            start = len(messages)
            await editor.fill('blocked')
            await wait_message('fixture-render-blocked', start)
            await page.evaluate("window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true}))")
            await editor.fill('retained after suspension')
            await page.wait_for_timeout(200)
            check(len(workers) == before_workers, 'Suspension does not restart a worker for hidden-page edits')
            await page.evaluate("window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}))")
            await result(page, 'retained after suspension')
            check(len(workers) == before_workers + 1, 'Resume replaces the terminated stuck worker and renders current source')

            unavailable = await opened(injection='window.__ActualWorker=window.Worker;window.Worker=undefined;')
            await unavailable.wait_for_function("() => document.getElementById('fmd-save-status').textContent.includes('Workers are unavailable')")
            check(await unavailable.evaluate('() => (window.__fixtureCoreCalls ?? []).length') == 0,
                  'Missing Worker support never starts a synchronous live preview')
            check((await unavailable.evaluate(READ))['source'] == SOURCE, 'Unavailable worker preserves the pre-rendered document')
            await unavailable.evaluate('() => { window.Worker=window.__ActualWorker; }')
            await unavailable.locator('#btn-restart-preview').click()
            await result(unavailable, SOURCE)
            check((await unavailable.evaluate(READ))['worker'], 'Explicit restart creates a working worker after capability recovery')

            # Advance only the host's 30-second timers. Rendering still blocks a
            # real worker; the production deadline handler performs termination.
            timed = await opened(injection='const clock=window.setTimeout;window.setTimeout=(fn,ms,...args)=>clock(fn,ms===30000?1000:ms,...args);')
            await result(timed, SOURCE)
            await timed.locator('#fmd-editor').fill('blocked')
            await timed.wait_for_function("() => document.getElementById('fmd-save-status').textContent.includes('timed out')",timeout=10000)
            check('timed out' in await timed.locator('#fmd-save-status').inner_text(), 'Accelerated host deadline terminates a genuinely blocked browser worker')
            check(await timed.evaluate('() => (window.__fixtureCoreCalls ?? []).length') == 0,
                  'A timeout cannot fall back to the UI-thread renderer')
            await timed.locator('#fmd-editor').fill('after timeout')
            await timed.locator('#btn-restart-preview').click()
            await result(timed, 'after timeout')
            check((await timed.evaluate(READ))['worker'] is True, 'Explicit restart recovers a terminated worker using current source')
            check(network == [], 'No HTTP or HTTPS requests')
            check(errors == [], 'No uncaught page errors')
            report['passed'] = len(checks)
            report['status'] = 'passed'
            await page.screenshot(path=str(out / 'preview.png'))
        except Exception as error:
            report['status'] = 'failed'
            report['failure'] = str(error)
            raise
        finally:
            report['http_requests'] = network
            report['page_errors'] = errors
            report['worker_count'] = len(workers)
            (out / 'report.json').write_text(json.dumps(report, indent=2, ensure_ascii=False))
            await browser.close()
    print(json.dumps({'passed': len(checks), 'report': str(out / 'report.json')}))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output')
    parser.add_argument('--mode', choices=['file', 'content'], default='file')
    parser.add_argument('--chromium', default=os.environ.get('CHROMIUM', '/usr/bin/chromium'))
    asyncio.run(run(parser.parse_args()))
