"""Check image edit/print/save/reopen in real Chromium using generated HTML.

Usage: python tests/support/interactive_images_browser_check.py document.html
The document must contain at least one supplied image resource. Playwright and
Chromium belong to the verification host, never to the renderer dependencies.
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
errors, requests = [], []
checks = 0

def check(ok, message):
    global checks
    assert ok, f"{message}; browser errors: {errors}"
    checks += 1

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
    context = browser.new_context(accept_downloads=True, viewport={"width": 1600, "height": 900})
    context.route("**/*", lambda route: (requests.append(route.request.url), route.abort()))

    def load(html):
        page = context.new_page()
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.set_content(html, wait_until="load")
        return page

    def manifest(page):
        return page.evaluate("JSON.parse(document.querySelector('body > script#fmd-image-assets[type=\"application/json\"]').textContent)")

    def edit(page, source):
        page.evaluate("""source => {
          const editor = document.querySelector('textarea#fmd-editor');
          editor.value = source; editor.dispatchEvent(new Event('input'));
          window.dispatchEvent(new Event('beforeprint'));
        }""", source)

    def save(page, button):
        with page.expect_download(timeout=5000) as pending:
            page.locator(button).click()
        result = pending.value
        check(result.failure() is None, "download failed")
        return Path(result.path()).read_bytes()

    page = load(args.html.read_text(encoding="utf-8"))
    assets = manifest(page)
    check(bool(assets), "fixture has no supplied images")
    key, uri = assets[0]
    original = page.locator('textarea#fmd-editor').input_value()
    initial = page.locator('main#preview-pane > #fmd-content').inner_html()
    # Escape Markdown punctuation, not the URI stored in the asset binding.
    escaped_key = ''.join('\\' + c if c in r'\[]()!*_`' else c for c in key)
    source = f'# fmd-image-assets\n\n![one](<{escaped_key}>)\n\n| H |\n|---|\n| ![two](<{escaped_key}>) |\n\nN[^n]\n\n[^n]: ![three](<{escaped_key}>)\n'
    edit(page, source)
    check(page.locator('main#preview-pane img').count() == 3, "images lost while editing")
    check(page.locator('main#preview-pane img').evaluate_all('(images) => images.every(img => img.getAttribute("src").startsWith("data:image/"))'), "a binding became a file or network URL")
    page.wait_for_function('Array.from(document.querySelectorAll("main#preview-pane img")).every(img => img.complete && img.naturalWidth > 0)')
    check(True, "embedded images decode")
    check(manifest(page) == assets, "heading impersonated or mutated the manifest")
    check(save(page, '#btn-save-markdown') == source.encode(), "Markdown source was rewritten into an asset payload")
    saved = save(page, '#btn-save-html').decode()
    second = load(saved)
    check(manifest(second) == assets, "save/reopen lost the image bindings")
    check(second.locator('main#preview-pane > #fmd-content').inner_html() == page.locator('main#preview-pane > #fmd-content').inner_html(), "save/reopen changed the preview")
    edit(second, 'Images temporarily removed')
    check(second.locator('main#preview-pane img').count() == 0, "removed images still rendered")
    # The asset remains available even after its last reference disappears.
    saved_empty = save(second, '#btn-save-html').decode()
    third = load(saved_empty)
    check(manifest(third) == assets, "unused image bindings were discarded")
    edit(third, source)
    check(third.locator('main#preview-pane img').count() == 3, "reinserted references lost their assets")
    check(third.locator('main#preview-pane img').first.get_attribute('src') == uri, "repeated save changed image bytes")
    # A directly embedded URI is usable too, but never becomes an active link.
    edit(third, f'![embedded]({uri}) [not a data link]({uri})')
    check(third.locator('main#preview-pane img').first.get_attribute('src') == uri, "embedded source image lost")
    check(third.locator('main#preview-pane a').count() == 0, "data URL became an active link")
    edit(page, original)
    check(page.locator('main#preview-pane > #fmd-content').inner_html() == initial, "exact undo failed to restore native preview")
    check(not requests, f"bound images made network requests: {requests}")
    check(not errors, f"runtime errors: {errors}")
    browser.close()
print(json.dumps({"checks_passed": checks, "network_requests": requests, "runtime_errors": errors}))
