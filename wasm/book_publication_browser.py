#!/usr/bin/env python3
"""Offline Chromium checks of publishing-profile authoring and source/portable
round trips. Production collection/UI with an explicit source-host adapter;
no worker, generated WASM, native PDF or package-startup claim. Requires Python
Playwright and an already installed Chromium. Never downloads a browser.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re

from playwright.sync_api import sync_playwright


def fixture_url(root: Path):
    """Rewrite only static relative import URLs; leave function bodies intact."""
    pattern = re.compile(r"\b(?:from\s*|import\s*)([\"'])(\.[^\"']+)\1")
    urls, visiting, hashes = {}, set(), {}

    def visit(path):
        path = path.resolve()
        if path in visiting:
            raise ValueError(f"Cyclic offline module graph: {path}")
        if path in urls:
            return urls[path]
        if not path.is_relative_to(root):
            raise ValueError(f"Fixture dependency outside repository: {path}")
        visiting.add(path)
        raw = path.read_bytes()
        code = raw.decode("utf-8")
        hashes[str(path.relative_to(root))] = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
        for match in reversed(list(pattern.finditer(code))):
            dependency = visit(path.parent / match.group(2))
            code = code[:match.start(2)] + dependency + code[match.end(2):]
        visiting.remove(path)
        urls[path] = "data:text/javascript;base64," + base64.b64encode(code.encode()).decode()
        return urls[path]

    return visit(root / "wasm/tests/book_publication_browser_fixture.mjs"), hashes


def run(chromium: str | None, output: Path):
    root = Path(__file__).resolve().parent.parent
    url, hashes = fixture_url(root)
    output.mkdir(parents=True, exist_ok=True)
    checks, errors, requests = [], [], []
    report = {"scope": "profile UI and source/portable persistence; explicit source host; no native rendering or package startup", "checks": checks, "module_git_blobs": hashes}
    try:
        with sync_playwright() as playwright:
            kwargs = {"headless": True, "args": ["--no-sandbox"]}
            if chromium:
                kwargs["executable_path"] = chromium
            browser = playwright.chromium.launch(**kwargs)
            try:
                context = browser.new_context(viewport={"width": 1200, "height": 950})

                def block(route):
                    requests.append(route.request.url)
                    route.abort()

                context.route("**/*", block)
                page = context.new_page()
                page.on("pageerror", lambda error: errors.append(str(error)))
                # Only the insertion target and host typography are fixture HTML;
                # the production panel factory creates every authoring control.
                page.set_content('''<!doctype html><html lang="en"><meta charset="utf-8">
                  <style>body{font:16px system-ui;max-width:1000px;margin:24px auto}fieldset{margin:16px 0;padding:16px}
                  label{display:block;margin:10px 0 4px}input[type=text],select,textarea{font:inherit;padding:6px;max-width:100%}
                  textarea{width:100%}.grid{display:grid;grid-template-columns:1fr 1fr;gap:12px}button{font:inherit;margin:8px;padding:10px}
                  pre{white-space:pre-wrap;overflow-wrap:anywhere}</style><section><h2 id="publish-title">Fixture publication host</h2></section></html>''')
                page.evaluate("url => { const script=document.createElement('script'); script.type='module'; script.textContent='import {startPublicationFixture} from '+JSON.stringify(url)+'; window.h=startPublicationFixture(document);'; document.head.append(script); }", url)
                page.wait_for_function("window.h !== undefined")
                value = page.evaluate
                el = lambda name: page.locator("#book-publication-" + name)

                def check(name, condition):
                    if not condition:
                        raise AssertionError(name)
                    checks.append(name)

                def apply():
                    el("apply").click()
                    status = el("status").inner_text()
                    if not (status.startswith("Publishing profile applied") or status.startswith("Publishing profile was already current")):
                        raise AssertionError(status)

                check("authoring starts without model changes or rendering", value("h.changes===0 && h.collection.project().schemaVersion===2"))
                value("document.querySelector('#book-publication-header-image').value='assets/logo.png';h.ui.discard()")
                check("discard restores a silently edited logo selection", el("header-image").input_value() == "" and value("h.changes===0"))
                check("numeric inputs retain invalid text rather than sanitize it", el("baseFontSize").get_attribute("type") == "text")
                for key, text in {"baseFontSize": "12.5", "headingScale": "1.3", "tableFontSize": "9", "tocDepth": "3", "fitToPages": "40", "metadataEpochSeconds": "0"}.items():
                    el(key).fill(text)
                el("htmlFontFormat").select_option("woff2")
                for flag in ["homogeneous", "antiriver", "pareto", "optimal-pagination", "protrusion", "expansion"]:
                    el("type-" + flag).check()
                el("codeLineNumbers").check()
                el("header-left").fill("{title} — 日本語 😀")
                el("header-rule").check()
                el("footer-center").fill("{page}/{pages}")
                el("skipFirstPage").check()
                el("header-image").select_option("assets/logo.png")
                el("header-position").select_option("right")
                el("header-height").fill("18")
                check("draft edits cannot change saved or rendered settings", value("h.changes===0 && h.collection.options.baseFontSize===undefined && h.collection.snapshot().options.running===undefined"))
                apply()
                check("one atomic profile revision advances source-project schema", value("h.changes===1 && h.collection.project().schemaVersion===4"))
                check("all typography switches reach canonical native options", value("h.collection.options.typography==='homogeneous,antiriver,pareto,optimal-pagination,protrusion,expansion'"))
                check("sizes, TOC, font container and zero epoch reach the collection", value("h.collection.options.baseFontSize===12.5 && h.collection.options.tocDepth===3 && h.collection.options.htmlFontFormat==='woff2' && h.collection.options.metadataEpochSeconds===0"))
                check("running bands and logo geometry reach the capture", value("h.collection.snapshot().options.running.header.image.heightPt===18 && h.collection.options.running.skipFirstPage"))
                check("source bytes and resource grants are preserved", value("h.collection.files[0].source==='\\ufeff# Unicode book\\r\\n日本語 😀\\r\\n' && h.collection.images.length===1"))
                apply()
                check("unchanged profile is a no-op", value("h.changes===1"))
                el("baseFontSize").fill("0x10")
                el("apply").click()
                check("invalid numeric draft stays visible without installation", el("baseFontSize").input_value() == "0x10" and value("h.collection.options.baseFontSize===12.5"))
                el("discard").click()
                check("discard restores the applied value", el("baseFontSize").input_value() == "12.5")
                value("h.collection.setPublication({...JSON.parse(document.querySelector('#book-publication-current').textContent),typography:'pareto, homogeneous,pareto',running:{footer:{left:'A\\r\\nB\\rC'}}},h.collection.revision)")
                check("browser textarea normalizes the saved CRLF view", el("footer-left").input_value() == "A\nB\nC")
                before = value("h.changes")
                apply()
                check("unchanged view preserves exact templates and token spelling", value("h.changes") == before and value("h.collection.options.running.footer.left==='A\\r\\nB\\rC' && h.collection.options.typography==='pareto,homogeneous,pareto'"))
                el("header-image").select_option("assets/logo.png")
                apply()
                value("h.roundTrip(true)")
                check("portable restore keeps profile and explicitly authorized bytes", value("h.collection.project().schemaVersion===4 && h.collection.images.length===1 && h.collection.options.running.header.image.dest==='assets/logo.png'"))
                value("h.roundTrip(false)")
                check("source-only restore retains logo references but revokes grants", value("h.collection.images.length===0 && h.collection.options.running.header.image.dest==='assets/logo.png'"))
                check("missing reference stays visible instead of being discarded", el("header-image").input_value() == "assets/logo.png" and "not authorized" in el("resources").inner_text())
                el("baseFontSize").fill("14")
                value("h.collection.configure({...h.collection.options,title:'Concurrent title'})")
                check("configuration conflict retains draft and blocks old application", el("apply").is_disabled() and el("baseFontSize").input_value() == "14" and "STALE_SOURCE" in el("status").inner_text())
                el("discard").click()
                check("reload recovers from profile conflict", not el("apply").is_disabled())
                el("header-left").dispatch_event("compositionstart")
                check("composition blocks Apply but keeps the active field editable", el("apply").is_disabled() and not el("header-left").is_disabled())
                el("header-left").fill("日本語")
                el("header-left").dispatch_event("compositionend")
                apply()
                value("h.setBusy(true)")
                check("main source busy state blocks profile actions", el("apply").is_disabled() and el("discard").is_disabled())
                value("h.setBusy(false)")
                el("baseFontSize").fill("20")
                value("h.ui.suspend();h.ui.resume()")
                check("suspension discards drafts but retains applied settings", el("baseFontSize").input_value() == "12.5")
                value("h.collection.setPage({size:'a4',margins:54},h.collection.revision)")
                el("defaults").click()
                check("default reset is staged until Apply", value("h.collection.options.baseFontSize===12.5"))
                apply()
                check("reset preserves metadata and paper geometry", value("h.collection.options.baseFontSize===undefined && h.collection.options.title==='Concurrent title' && h.collection.options.page.margins.topPt===54 && h.collection.project().schemaVersion===3"))
                page.screenshot(path=str(output / "profile.png"), full_page=True)
                value("h.ui.dispose()")
                check("disposal disables the complete authoring panel", el("apply").is_disabled() and el("header-left").is_disabled())
                check("no page script errors", not errors)
                check("no external HTTP attempts", not requests)
                value("h.close()")
            finally:
                browser.close()
    except Exception as error:
        report["failure"] = str(error)
        raise
    finally:
        report.update({"passed": len(checks), "page_errors": errors, "external_requests": requests})
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chromium", help="Installed executable; otherwise use Playwright's installed default")
    parser.add_argument("--output", type=Path, default=Path("/tmp/fmd-publication-browser"))
    args = parser.parse_args()
    run(args.chromium, args.output)
