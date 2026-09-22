"""Optional real-Chromium checks for a generated interactive HTML document.

Usage: python tests/support/interactive_browser_check.py document.html
Requires Playwright and Chromium on the verification host, not in the renderer.
Loads supplied HTML bytes in fresh browser pages and exercises real Blob
navigation/downloads. It does not assert native/WASM parity or file:// policy.
"""
import argparse
import json
from pathlib import Path
import shutil

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("html", type=Path)
parser.add_argument("--chromium", default=shutil.which("chromium"))
args = parser.parse_args()
errors = []
requests = []
checks = 0

def check(condition, message):
    global checks
    assert condition, f"{message}; browser_errors={errors}"
    checks += 1

def load(context, html):
    page = context.new_page()
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("request", lambda request: requests.append(request.url))
    page.set_content(html, wait_until="load")
    return page

def raw_source(page):
    return page.evaluate('JSON.parse(document.querySelector(\'body > script#fmd-raw-source[type="application/json"]\').textContent)')

def preview(page):
    return page.locator('main#preview-pane > #fmd-content').inner_html()

def download(page, button):
    with page.expect_download(timeout=5000) as pending:
        page.locator(button).click()
    result = pending.value
    check(result.failure() is None, "Browser rejected download")
    return Path(result.path()).read_bytes(), result.suggested_filename

def dirty(page):
    return page.evaluate("""() => {
      const event = new Event('beforeunload', {cancelable: true});
      window.dispatchEvent(event); return event.defaultPrevented;
    }""")

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
    context = browser.new_context(accept_downloads=True, viewport={"width": 1440, "height": 900})
    page = load(context, args.html.read_text(encoding="utf-8"))
    original = raw_source(page)
    original_preview = preview(page)
    check(not dirty(page), "Opening a document dirtied it")
    raw, name = download(page, '#btn-save-markdown')
    check(raw == original.encode("utf-8"), "Untouched source bytes changed on download")
    check(name.endswith('.md'), "Wrong Markdown extension")
    untouched, _ = download(page, '#btn-save-html')
    clean = load(context, untouched.decode("utf-8"))
    check(raw_source(clean) == original, "Untouched workspace changed source")
    check(preview(clean) == original_preview, "Unchanged workspace lost native content")
    check(not dirty(clean), "Reopened untouched workspace was dirty")

    # Includes IDs that look like the surrounding application, code literal
    # injection attempts, tables, task/ordered lists and repeated/nested notes.
    edited = '''\n# fmd-raw-source

# stats-drawer

# stat-words

# btn-stats-close

## Revised

| Left | Right |
| :-- | --: |
| alpha | 42 |

3. First
   - [x] Nested task
4. Second

Body[^one] and again[^one].

[^one]: Note with [guide](/guide) and another[^two].
[^two]: Nested note.

```html
</ScRiPt><script>globalThis.fmdInjected=true</script>
</textarea><img src=x onerror="globalThis.fmdInjected=true">
```

Unicode é中😀\u2028\u2029 and null \0.
'''
    page.evaluate("""source => {
      const editor = document.querySelector('textarea#fmd-editor');
      editor.value = source;
      editor.dispatchEvent(new Event('input'));
      window.print = () => { window.printedHtml = document.querySelector('main#preview-pane > #fmd-content').innerHTML; };
      document.getElementById('btn-export-pdf').click();
    }""", edited)
    printed = page.evaluate('window.printedHtml')
    check('<table>' in printed and 'Revised' in printed, "Print saw a stale preview")
    check('<ol start="3">' in printed, "Ordered list lost its start number")
    check('checked' in printed and 'Nested task' in printed, "Task list was lost")
    check('footnotes' in printed and 'fnref-1-2' in printed, "Repeated notes lost their backlinks")
    check('Nested note.' in printed, "Nested footnote was lost")
    check(page.evaluate('globalThis.fmdInjected === undefined'), "Source executed in editor")
    check(dirty(page), "Changed source did not activate leave warning")
    markdown, _ = download(page, '#btn-save-markdown')
    check(markdown == edited.encode('utf-8'), "Edited Markdown download lost bytes")
    check(dirty(page), "Download initiation falsely marked edits saved")
    page.locator('#btn-zoom-in').click()
    page.locator('#btn-stats-toggle').click()
    workspace, _ = download(page, '#btn-save-html')
    check(raw_source(page) == original, "Saving mutated the live source baseline")
    second = load(context, workspace.decode('utf-8'))
    check(raw_source(second) == edited, "Workspace JSON did not round-trip source")
    check(second.locator('textarea#fmd-editor').input_value() == edited, "Reopened editor lost source")
    check(preview(second) == preview(page), "Reopened workspace changed preview")
    check(second.locator('body > #stats-drawer').evaluate("e => !e.classList.contains('open')"), "Export retained transient drawer state")
    second.locator('#btn-stats-toggle').click()
    second.locator('#stats-drawer > .fmd-stats-header > #btn-stats-close').click()
    check(second.locator('body > #stats-drawer').evaluate("e => !e.classList.contains('open')"), "A document heading intercepted the drawer close action")
    check(not dirty(second), "New source baseline not established on reopening")
    check(second.evaluate('document.scripts.length') == 2, "Injected or missing executable script")
    check(second.evaluate('globalThis.fmdInjected === undefined'), "Saved source executed when reopening")
    second.locator('#btn-zoom-in').click()
    check(second.locator('#btn-zoom-reset').inner_text() == '120%', "Saved zoom control state diverged")

    # Saving an exported workspace must remain self-hosting for another cycle.
    next_text = edited + '\n## Third generation\n'
    second.locator('textarea#fmd-editor').fill(next_text)
    another, _ = download(second, '#btn-save-html')
    third = load(context, another.decode('utf-8'))
    check(raw_source(third) == next_text, "Second export generation lost source")
    check('Third generation' in preview(third), "Second export generation saved a stale preview")
    check(third.evaluate('globalThis.fmdInjected === undefined'), "Second export generation executed source")

    # Restoring the normalized editing view must recover byte-exact source and
    # the original native markup, not a fallback approximation of it.
    page.evaluate("""source => {
      const editor = document.querySelector('textarea#fmd-editor');
      editor.value = source;
      editor.dispatchEvent(new Event('input'));
      window.dispatchEvent(new Event('beforeprint'));
    }""", original)
    check(preview(page) == original_preview, "Exact undo did not restore native preview")
    restored, _ = download(page, '#btn-save-markdown')
    check(restored == original.encode('utf-8'), "Undo failed to restore original source bytes")
    check(not dirty(page), "Undo failed to clear leave warning")
    check(not requests, f"Standalone fixture made network requests: {requests}")
    check(not errors, f"Browser runtime errors: {errors}")
    browser.close()
print(json.dumps({"checks_passed": checks, "browser": "Chromium", "network_requests": len(requests), "runtime_errors": errors}))
