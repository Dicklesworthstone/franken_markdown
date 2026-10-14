#!/usr/bin/env python3
"""Chromium proof of selected-chapter UI/Worker/isolation, with explicit native ABI
and editor fixtures. Uses a test-only classic-worker ESM bootstrap in an offline
page; does not verify the normal module-worker entrypoint or package startup.
No generated WASM, Markdown rendering or visual parity claim.
Run: python wasm/book_chapter_browser.py --chromium /usr/bin/chromium --output /tmp/fmd-chapter
Requires Python Playwright and an installed Chromium; never downloads a browser.
"""
import argparse
import json
from pathlib import Path
import re

from playwright.sync_api import sync_playwright


def modules(root: Path):
    """Materialize the fixture's acyclic static relative-import graph offline.

    Only module URL strings are rewritten. Function bodies remain production
    source, shared by host and real Worker; no alternate implementation is used.
    Fail on missing files/cycles rather than silently stubbing dependencies.
    """
    pattern = re.compile(r"\b(?:from\s*|import\s*)([\"'])(\.[^\"']+)\1")
    ordered, visiting, complete = [], set(), set()
    def visit(path):
        path = path.resolve()
        if path in visiting:
            raise ValueError(f"Cyclic fixture module graph: {path}")
        if path in complete:
            return
        if not path.is_relative_to(root):
            raise ValueError(f"Fixture dependency outside repository: {path}")
        visiting.add(path)
        code = path.read_text(encoding="utf-8")
        links = []
        for match in pattern.finditer(code):
            dependency = (path.parent / match.group(2)).resolve()
            visit(dependency)
            # JavaScript slices UTF-16 code units, not Python Unicode points.
            units = lambda offset: len(code[:offset].encode("utf-16-le")) // 2
            links.append({"start": units(match.start(2)), "end": units(match.end(2)),
                          "path": str(dependency.relative_to(root))})
        visiting.remove(path)
        complete.add(path)
        ordered.append({"path": str(path.relative_to(root)), "code": code, "links": links})
    visit(root / "wasm/tests/book_chapter_browser_fixture.mjs")
    return ordered


def run(chromium: str | None, output: Path):
    output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parent.parent
    graph = modules(root)
    html = (root / "wasm/demo/book.html").read_text(encoding="utf-8")
    original = '<script type="module" src="./book.js"></script>'
    if html.count(original) != 1:
        raise ValueError("Cannot identify the real workbench entrypoint")
    html = html.replace(original, "")
    checks, errors, outbound, block_events = [], [], [], []
    report = {"scope": "production JS and Chromium DOM/CSP; real Worker via test-only classic bootstrap; explicit native ABI/editor fixtures, not package startup", "checks": checks}
    try:
        with sync_playwright() as playwright:
            launch = {"headless": True, "args": ["--no-sandbox"]}
            if chromium:
                launch["executable_path"] = chromium
            browser = playwright.chromium.launch(**launch)
            try:
                context = browser.new_context(viewport={"width": 1200, "height": 1000})
                # Prevent any accidental external request; record attempts rather
                # than letting a blocked DNS lookup masquerade as CSP enforcement.
                def route(request):
                    url = request.request.url
                    outbound.append(url)
                    request.abort()
                context.route("**/*", route)
                page = context.new_page()
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.on("console", lambda event: block_events.append(event.text) if event.text.startswith("FMD_CHAPTER_FIXTURE_BLOCK_") else None)
                page.set_content(html)
                page.evaluate("""graph => {
                  const urls = new Map();
                  for (const module of graph) {
                    let code = module.code;
                    for (const link of [...module.links].reverse()) {
                      const url = urls.get(link.path);
                      if (!url) throw new Error('Missing offline dependency ' + link.path);
                      code = code.slice(0, link.start) + url + code.slice(link.end);
                    }
                    const url = 'data:text/javascript;base64,' + btoa(unescape(encodeURIComponent(code)));
                    urls.set(module.path, url);
                  }
                  const script = document.createElement('script'); script.type = 'module';
                  script.src = urls.get('wasm/tests/book_chapter_browser_fixture.mjs');
                  document.head.append(script);
                  window.releaseFixtureModules = () => { for (const url of urls.values()) if (url.startsWith('blob:')) URL.revokeObjectURL(url); };
                }""", graph)
                page.wait_for_function("window.chapterFixture !== undefined")
                def check(name, condition):
                    if not condition:
                        raise AssertionError(name)
                    checks.append(name)
                def value(expression):
                    return page.evaluate(expression)
                def ready(index):
                    page.wait_for_function("i => { const status=document.querySelector('#preview-status').textContent; return (status.startsWith('Reviewing ') && document.querySelector('#preview-pages').value===String(i)) || /^[A-Z_]+:/.test(status); }", arg=index, timeout=15000)
                    status = value("document.querySelector('#preview-status').textContent")
                    if not status.startswith("Reviewing "):
                        raise AssertionError(status)
                def frame():
                    return page.query_selector("#preview-frame").content_frame()
                def metrics():
                    return json.loads(frame().locator("#metrics").inner_text())
                def build(index):
                    page.click("#preview-build")
                    ready(index)
                def blank():
                    return value("!document.querySelector('#preview-frame').srcdoc.includes('script nonce')")
                check("no automatic render at startup", value("chapterFixture.counters.requests.length===0"))
                build(0)
                check("selected chapter rendered without site ZIP work", metrics()["renders"] == 1 and metrics()["sites"] == 0)
                check("real isolated reader displays fixture content", frame().locator("#content").inner_text() == "First chapter")
                check("navigation includes all chapters", page.locator("#preview-pages option").count() == 2)
                check("opaque origin and no same-origin sandbox grant", value("document.querySelector('#preview-frame').contentDocument===null") and page.locator("#preview-frame").get_attribute("sandbox") == "allow-scripts")
                check("native language preserved", frame().locator("html").get_attribute("lang") == "ja")
                check("active content and external image sources removed", frame().locator("script,iframe,form,[onerror]").count() == 0 and frame().locator("#remote").get_attribute("src") is None)
                check("fixture scripts never executed", frame().evaluate("window.__fixturePwned !== true") and value("window.__fixturePwned !== true"))
                # A violation event proves CSP enforcement independently of the
                # managed browser's ambient outbound-network restrictions.
                for kind in ["style", "img", "script"]:
                    blocked = frame().evaluate("""kind => new Promise(resolve => {
                      const node = document.createElement(kind);
                      const done = value => { clearTimeout(timer); document.removeEventListener('securitypolicyviolation', listener); node.remove(); resolve(value); };
                      const listener = event => {
                        if (event.effectiveDirective.startsWith(kind + '-src')) done({directive:event.effectiveDirective, blocked:event.blockedURI});
                      };
                      const timer = setTimeout(() => done(null), 2000);
                      document.addEventListener('securitypolicyviolation', listener);
                      if (kind === 'style') node.textContent = '@import url("https://blocked.invalid/csp-probe");';
                      else if (kind === 'img') node.src = 'https://blocked.invalid/csp-probe';
                      else node.textContent = 'window.__fixtureScriptProbe=true';
                      document.body.append(node);
                    })""", kind)
                    check(f"CSP explicitly blocks unauthorized {kind}", blocked is not None)
                check("CSP script probe did not execute", frame().evaluate("window.__fixtureScriptProbe !== true"))
                frame().wait_for_function("document.querySelector('#embedded').complete")
                check("embedded PNG remains loadable", frame().locator("#embedded").evaluate("image=>image.naturalWidth===1"))
                frame().locator("#same").click()
                ready(0)
                page.wait_for_function("document.querySelector('#preview-frame').srcdoc.includes('target')")
                frame().wait_for_function("document.activeElement?.id==='target'")
                check("same-page fragment focuses without worker render", value("chapterFixture.counters.requests.length===1"))
                frame().locator("#cross").click()
                ready(1)
                frame().wait_for_function("document.activeElement?.id==='target'")
                check("cross-chapter link renders and focuses destination", frame().locator("#content").inner_text() == "Second chapter")
                check("navigation reuses native book without resource snapshot or transfer", metrics()["creates"] == 1 and value("chapterFixture.counters.snapshots===1 && chapterFixture.counters.requests[1].assetBytes===0 && chapterFixture.counters.requests[1].sources.length===0"))
                count = value("chapterFixture.counters.requests.length")
                frame().locator("#external").click()
                page.wait_for_function("document.querySelector('#preview-status').textContent.includes('No navigation')")
                check("external link cannot navigate reader or host", value("chapterFixture.counters.requests.length") == count and page.url == "about:blank")
                value("chapterFixture.edit(1,'Changed 😀'); chapterFixture.edit(2,'New shared')")
                check("edits immediately clear displayed content", blank())
                build(1)
                work = metrics()
                check("source edits reuse book and update selected content", work["source"] == "Changed 😀" and work["includes"][0]["source"] == "New shared" and work["creates"] == 1 and work["updates"] == 1)
                check("source edits transfer only changed paths", value("JSON.stringify(chapterFixture.counters.requests.at(-1).sources)") == '["two.md","note.md"]' and value("chapterFixture.counters.requests.at(-1).assetBytes===0 && chapterFixture.counters.snapshots===1"))
                value("chapterFixture.revoke()")
                check("asset revocation clears idle worker and frame", blank() and value("!chapterFixture.worker.hasRetainedBook && chapterFixture.counters.terminations===1"))
                build(1)
                check("revocation recaptures without unauthorized assets", value("chapterFixture.counters.workers===2 && chapterFixture.counters.snapshots===2 && chapterFixture.counters.requests.at(-1).assetBytes===0"))
                value("chapterFixture.edit(1,'BLOCK')")
                page.click("#preview-build")
                # Await an actual log from inside the fixture's synchronous native
                # call, not merely a request posted before worker startup.
                for _ in range(100):
                    if "FMD_CHAPTER_FIXTURE_BLOCK_STARTED" in block_events:
                        break
                    page.wait_for_timeout(20)
                check("synchronous worker work actually started", "FMD_CHAPTER_FIXTURE_BLOCK_STARTED" in block_events)
                value("chapterFixture.edit(1,'After cancellation')")
                build(1)
                check("editing cancels synchronous worker and permits new render", frame().locator("#content").inner_text() == "After cancellation" and "FMD_CHAPTER_FIXTURE_BLOCK_FINISHED" not in block_events)
                value("document.querySelector('#chapter-source').value='silent edit'")
                frame().locator("#cross").click()
                page.wait_for_function("document.querySelector('#preview-status').textContent.includes('STALE_SOURCE')")
                check("silent source edit fences navigation", blank())
                build(1)
                page.screenshot(path=str(output / "reader.png"))
                value("chapterFixture.preview.suspend()")
                check("suspension releases content and retained worker", blank() and value("!chapterFixture.worker.hasRetainedBook"))
                value("chapterFixture.close(); releaseFixtureModules()")
                check("no page script errors", not errors)
                check("no attempted external HTTP requests", not outbound)
                report.update({"passed": len(checks), "page_errors": errors, "external_requests": outbound})
            finally:
                try:
                    report["fixture_counters"] = page.evaluate("window.chapterFixture?.counters")
                except Exception:
                    pass
                browser.close()
    except Exception as error:
        report.update({"failure": str(error), "passed": len(checks), "page_errors": errors, "external_requests": outbound})
        raise
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chromium", help="Installed Chromium executable; omit for Playwright's installed default")
    parser.add_argument("--output", type=Path, default=Path("/tmp/fmd-chapter-browser"))
    args = parser.parse_args()
    run(args.chromium, args.output)
