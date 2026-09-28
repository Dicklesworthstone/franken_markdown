#!/usr/bin/env python3
"""Real browser interaction with explicit source/preflight adapters, not WASM.

Uses Playwright and Chromium; retains its output in a fresh temporary directory.
Test .ttf bytes are protocol fixtures, not valid fonts or shared font artifacts.
"""
import json
import os
from pathlib import Path
import tempfile
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
MODULES = ["demo/book_font_assets.mjs", "demo/book_font_authoring.mjs",
           "demo/book_font_controls.mjs", "tests/book_font_test_support.mjs",
           "tests/book_font_browser_fixture.mjs"]
OUTPUT = Path(tempfile.mkdtemp(prefix="fmd-book-font-browser-"))
checks = []
def check(name, value):
    checks.append({"check": name, "passed": bool(value)})
    assert value, name

def choose(page, slot, name="font.ttf", weight="", data=b"\x00\x01\x00\x00\x05\x06\x07\xff"):
    page.locator(f"#book-font-file-{slot}").set_input_files({"name": name, "mimeType": "font/ttf", "buffer": data})
    page.locator(f"#book-font-weight-{slot}").fill(weight)

def fonts(page):
    return page.evaluate("fixture.book.collection.fonts")

def settled(page):
    page.wait_for_function("() => document.querySelector('#book-font-panel').getAttribute('aria-busy') === 'false'")

with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=os.environ.get("CHROMIUM", "/usr/bin/chromium"),
                                         headless=True, args=["--no-sandbox"])
    page = browser.new_page(viewport={"width": 1000, "height": 850})
    errors, network = [], []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("request", lambda request: network.append(request.url) if request.url.startswith(("http:", "https:")) else None)
    page.set_content('''<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src blob:; worker-src blob:; style-src 'unsafe-inline'; connect-src 'none'; font-src 'none'; object-src 'none'">
<title>Book font authoring — interaction fixture</title>
<style>body{font:16px/1.45 sans-serif;max-width:900px;margin:16px auto;padding:12px}label{display:block;margin:8px 0}input,button,textarea{font:inherit;max-width:100%;box-sizing:border-box}button{padding:8px;margin:4px}input{padding:6px}section{max-width:100%}p{overflow-wrap:anywhere}textarea{width:100%}</style>
</head><body><h1>Book publisher font authoring</h1><textarea id="chapter-source" aria-label="Book source"></textarea>
<section><h2 id="publish-title">Prepare and download</h2></section></body></html>''')
    page.evaluate("window.cspViolations=[]; addEventListener('securitypolicyviolation', e => cspViolations.push(e.violatedDirective))")
    # Relocate import specifiers only; production functions execute unmodified.
    page.evaluate('''async sources => {
      const urls = {};
      for (const [path, original] of Object.entries(sources)) {
        const source = original.replace(/(from\\s*["'])(\\.[^"']+)(["'])/g, (_, before, spec, after) => {
          const parts = path.split('/'); parts.pop();
          for (const part of spec.split('/')) { if (part === '..') parts.pop(); else if (part !== '.') parts.push(part); }
          const url = urls[parts.join('/')]; if (!url) throw Error('unstaged import '+spec);
          return before+url+after;
        });
        urls[path] = URL.createObjectURL(new Blob([source], {type:'text/javascript'}));
      }
      window.moduleUrls = urls;
      const fixture = await import(urls['tests/book_font_browser_fixture.mjs']); fixture.mount();
    }''', {name: (ROOT / name).read_text() for name in MODULES})
    check("five labelled font-role file inputs", page.locator('input[type=file]').count() == 5)
    check("no font assignment on initialization", fonts(page) == [] and page.evaluate("fixture.stats.started") == 0)
    check("apply disabled without selected files", page.locator("#book-font-assign").is_disabled())
    choose(page, "body-regular", "variable.ttf", "450")
    choose(page, "body-bold", "<bold&font>.ttf", "700")
    page.locator("#book-font-assign").click(); settled(page)
    check("two roles installed atomically after one real worker", len(fonts(page)) == 2 and page.evaluate("fixture.stats.started") == 1)
    check("per-role weight pins retained", [entry["weight"] for entry in fonts(page)] == [450, 700])
    check("hostile filename remains plain text", "<bold&font>.ttf" in page.locator("#book-font-current-body-bold").inner_text() and page.locator('bold').count() == 0)
    check("selected file handles released after assignment", page.evaluate("[...document.querySelectorAll('input[type=file]')].every(x=>!x.files.length)"))
    check("exact supplied bytes retained", page.evaluate("[...fixture.book.store.snapshot()[0].bytes]") == [0,1,0,0,5,6,7,255])
    check("original BOM and CRLF source preserved", page.evaluate("fixture.book.collection.files[0].source") == "\ufeff# Book\r\n\nOriginal source")
    choose(page, "body-regular", "bad-weight.ttf", "1e2")
    page.locator("#book-font-assign").click()
    page.wait_for_function("() => document.querySelector('#book-font-status').textContent.includes('INVALID_FONT_WEIGHT')")
    check("invalid pin never starts a worker", page.evaluate("fixture.stats.started") == 1)
    page.locator("#book-font-discard").click()
    before = fonts(page)
    page.evaluate("fixture.mode.reject=true")
    choose(page, "body-regular", "reject.ttf"); choose(page, "mono-regular", "mono.ttf")
    page.locator("#book-font-assign").click(); settled(page)
    check("preflight failure keeps every prior role", fonts(page) == before)
    check("failed selection remains available for correction", page.locator("#book-font-file-mono-regular").evaluate("x=>x.files.length") == 1)
    page.locator("#book-font-discard").click(); page.evaluate("fixture.mode.reject=false; fixture.mode.delay=300")
    choose(page, "body-regular", "cancel.ttf", "600"); page.locator("#book-font-assign").click()
    page.wait_for_function("() => fixture.stats.active===1")
    page.locator("#book-font-cancel").click(); settled(page)
    check("cancel terminates the owned real worker", page.evaluate("fixture.stats.active") == 0 and fonts(page) == before)
    page.locator("#book-font-assign").click(); page.wait_for_function("() => fixture.stats.active===1")
    page.evaluate("document.querySelector('#book-font-weight-body-regular').value='800'")
    settled(page)
    check("silent weight changes fence installation", fonts(page) == before)
    page.locator("#book-font-assign").click(); page.wait_for_function("() => fixture.stats.active===1")
    page.locator("#chapter-source").fill("# New source\n")
    settled(page)
    check("source edits cancel pending font work", page.evaluate("fixture.stats.active") == 0 and fonts(page) == before)
    page.locator("#book-font-discard").click()
    page.once("dialog", lambda dialog: dialog.dismiss())
    page.locator("#book-font-remove-body-bold").click(); settled(page)
    check("declining actual browser confirmation preserves role", fonts(page) == before)
    page.once("dialog", lambda dialog: dialog.accept())
    page.locator("#book-font-remove-body-bold").click(); settled(page)
    check("confirming actual browser removal removes only chosen role", [entry["slot"] for entry in fonts(page)] == ["body-regular"])
    page.evaluate("fixture.mode.delay=0")
    choose(page, "mono-regular", "mono.ttf"); page.locator("#book-font-assign").click(); settled(page)
    check("explicit retry assigns additional role", [entry["slot"] for entry in fonts(page)] == ["body-regular", "mono-regular"])
    choose(page, "body-italic", "uncommitted.ttf")
    page.evaluate("dispatchEvent(new PageTransitionEvent('pagehide', {persisted:true}))")
    check("suspension releases installed and selected font grants", fonts(page) == [] and page.evaluate("[...document.querySelectorAll('input[type=file]')].every(x=>!x.files.length)"))
    started = page.evaluate("fixture.stats.started")
    page.evaluate("dispatchEvent(new PageTransitionEvent('pageshow', {persisted:true}))")
    check("resume does not reload fonts or regenerate work", page.evaluate("fixture.stats.started") == started and not page.locator("#book-font-file-body-regular").is_disabled())
    page.set_viewport_size({"width": 390, "height": 844})
    check("font panel fits a narrow viewport", page.evaluate("document.documentElement.scrollWidth <= innerWidth"))
    page.screenshot(path=str(OUTPUT / "font-controls.png"), full_page=True)
    page.evaluate("fixture.ui.dispose(); fixture.ui.dispose()")
    check("dispose releases one dedicated client", page.evaluate("fixture.stats.disposed===1 && fixture.stats.active===0"))
    check("no browser page errors", not errors)
    check("no content-security violations", page.evaluate("cspViolations.length") == 0)
    check("no HTTP or HTTPS requests", not network)
    browser.close()
report = {"scope": "production JS and font store; explicit source host and protocol-only worker adapter; no Rust/WASM font parsing", "checks": checks, "errors": errors, "network": network, "artifacts": str(OUTPUT)}
(OUTPUT / "results.json").write_text(json.dumps(report, indent=2)+"\n")
print(json.dumps(report, indent=2))
