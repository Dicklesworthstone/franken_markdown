"""Offline Chromium DOM checks; not a native publication or file-URL history test.

Run with an already installed Chromium and Playwright. This never downloads a
browser, changes managed browser policies, or starts a web server. about:blank
intentionally exercises the supported opaque-origin fallback. URL/history logic
is covered separately by book_site_search_navigation.test.mjs.
"""

import argparse
import json
from pathlib import Path
import shutil

from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", default=shutil.which("chromium"))
    args = parser.parse_args()
    if not args.browser or not Path(args.browser).is_file():
        parser.error("Pass --browser with an existing Chromium executable.")
    source = Path(__file__).resolve().parents[2] / "src/book/site_search.js"
    script = source.read_text(encoding="utf-8")
    entries = [
        {"kind": "paragraph", "text": "needle current", "anchor": f"s{i}"}
        for i in range(60)
    ]
    entries.extend([
        {"kind": "heading", "text": "needle", "anchor": "old"},
        {"kind": "paragraph", "text": "obsolete", "anchor": "old"},
        {"kind": "paragraph", "text": "中文 café <svg/onload=bad>", "anchor": "unicode"},
    ])
    index = {"schema": "fmd-book-search-index-v1", "chapters": [{
        "title": '<img src=x onerror="bad"> Guide', "source": "guide/a.md", "page": "a.html",
        "index": {"schema": "fmd-search-index-v1", "entries": entries},
    }]}
    data = json.dumps(index, ensure_ascii=True).replace("<", "\\u003c").replace("&", "\\u0026")
    shell = """<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline';
style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'"></head><body>
<form id="search-form"><label for="query">Search this book</label>
<input id="query" type="search"><button type="submit">Search</button></form>
<p id="search-help"></p><p id="status" role="status"></p><ol id="results"></ol>
<button id="previous" type="button">Previous results</button>
<button id="next" type="button">Next results</button>
<script id="search-data" type="application/json">""" + data + "</script></body></html>"
    checks = []

    def check(name, value):
        if not value:
            raise AssertionError(name)
        checks.append(name)

    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=args.browser, headless=True,
                                    args=["--no-sandbox", "--disable-dev-shm-usage"])
        try:
            page = browser.new_page()
            page.set_default_timeout(5000)
            errors, requests = [], []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("request", lambda request: requests.append(request.url))
            page.set_content(shell)
            # Only expose the existing CommonJS test export; the production
            # script and mounted controller are otherwise unchanged.
            page.evaluate("window.module = {exports: {}}")
            page.add_script_tag(content=script)
            page.evaluate("window.reader = module.exports.mount(document)")
            check("one reusable permalink", page.locator("#search-permalink").count() == 1)
            check("opaque-origin permalink hidden", page.locator("#search-permalink").is_hidden())
            check("operator help wired", "path:guide/" in page.locator("#search-help").inner_text())

            def submit(query, expected):
                page.locator("#query").fill(query)
                page.locator('button[type="submit"]').click()
                page.wait_for_function("value => document.querySelector('#status').textContent.includes(value)",
                                       arg=expected)

            submit("needle path:guide/ -obsolete", "of 60 matching sections")
            check("late excluded paragraph rejects its section", page.locator("#results li").count() == 25)
            check("emitted anchor used", page.locator("#results a").first.get_attribute("href") == "./a.html#s0")
            check("chapter title remains inert", "<img" in page.locator("#results a").first.inner_text())
            check("no title image node", page.locator("#results img").count() == 0)
            check("source path rendered", "Source: guide/a.md" in page.locator("#results li").first.inner_text())
            page.locator("#next").click()
            check("second page", page.locator("#results").get_attribute("start") == "26")
            page.locator("#next").click()
            check("last page exact", page.locator("#results li").count() == 10)
            check("last page disables next", page.locator("#next").is_disabled())
            page.locator("#previous").click()
            check("previous page", page.locator("#results").get_attribute("start") == "26")
            page.locator("#query").focus()
            page.keyboard.press("Escape")
            check("real Escape clears query", page.locator("#query").input_value() == "")
            check("real Escape clears results", page.locator("#results li").count() == 0)

            page.evaluate("""() => {
              const input = document.querySelector('#query');
              input.dispatchEvent(new CompositionEvent('compositionstart', {bubbles: true}));
              input.value = '中';
              input.dispatchEvent(new InputEvent('input', {bubbles: true, isComposing: true}));
              document.querySelector('#search-form').requestSubmit();
            }""")
            page.wait_for_timeout(200)
            check("provisional composition not searched", page.locator("#results li").count() == 0)
            check("composition status", "Finish composing" in page.locator("#status").inner_text())
            prevented = page.evaluate("""() => {
              const event = new KeyboardEvent('keydown', {key:'Escape', isComposing:true,
                bubbles:true, cancelable:true});
              document.querySelector('#query').dispatchEvent(event);
              return event.defaultPrevented;
            }""")
            check("composition Escape not intercepted", not prevented)
            page.evaluate("""() => {
              const input = document.querySelector('#query');
              input.value = '中文';
              input.dispatchEvent(new CompositionEvent('compositionend', {bubbles: true}));
              input.dispatchEvent(new InputEvent('input', {bubbles: true}));
            }""")
            page.wait_for_function("() => document.querySelector('#status').textContent.includes('of 1 matching sections')")
            check("committed composition searched", page.locator("#results a").first.get_attribute("href") == "./a.html#unicode")
            submit('café', 'of 1 matching sections')
            check("markup-looking Unicode passage remains text", "<svg/onload=bad>" in page.locator("#results").inner_text())
            check("no passage svg node", page.locator("#results svg").count() == 0)
            submit('-obsolete', 'before exclusions')
            check("invalid query removes prior results", page.locator("#results li").count() == 0)
            page.evaluate("reader.dispose(); reader = module.exports.mount(document)")
            check("remount reuses link", page.locator("#search-permalink").count() == 1)
            page.evaluate("reader.dispose()")
            check("dispose hides link", page.locator("#search-permalink").is_hidden())
            check("zero page errors", not errors)
            check("zero observed requests", not requests)
            print(json.dumps({"passed": len(checks), "checks": checks,
                              "page_errors": errors, "requests": requests,
                              "scope": "production search script; real DOM; synthetic composition; opaque origin"}, indent=2))
        finally:
            browser.close()


if __name__ == "__main__":
    main()
