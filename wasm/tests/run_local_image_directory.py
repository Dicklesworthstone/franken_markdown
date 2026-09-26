#!/usr/bin/env python3
"""Actual Chromium FileList/DOM tests; no Rust/WASM or native picker-dialog claim.
Requires Playwright Python plus Chromium. All HTTP requests are blocked.
"""
import argparse
import base64
from pathlib import Path
import tempfile

from playwright.sync_api import sync_playwright


def module_url(source: str) -> str:
    return "data:text/javascript;base64," + base64.b64encode(source.encode()).decode()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    args = parser.parse_args()
    wasm = Path(__file__).resolve().parents[1]
    raster = module_url((wasm / "flow_raster.mjs").read_text())
    source = (wasm / "demo/local_image_sources.mjs").read_text()
    source = source.replace('"../flow_raster.mjs"', repr(raster))
    module = module_url(source)
    # Only owned, newly created test files. Leave the scratch directory intact.
    root = Path(tempfile.mkdtemp(prefix="fmd-image-directory-")) / "project"
    for name, payload in [("docs/guide.md", b"# Private source"),
                          ("images/plot.png", b"first image bytes"),
                          ("images/a[b](c).jpg", b"second image bytes")]:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)
    count = 0
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=args.chromium, headless=True,
                                             args=["--no-sandbox"])
        page = browser.new_page()
        requests = []
        page.route("**/*", lambda route: (requests.append(route.request.url), route.abort()))
        page.set_content('<section id="images"></section><p id="image-status"></p><textarea id="source">unchanged</textarea>')
        page.evaluate("""async (url) => {
          const m = await import(url);
          globalThis.grants = [];
          globalThis.ui = m.createDirectoryImageControls({
            container: document.querySelector('#images'), status: document.querySelector('#image-status'),
            onChange(next) { grants.push(next); globalThis.active = next; }
          });
          globalThis.read = async (url) => {
            const bytes = await active.load({url}, {signal: new AbortController().signal, maxBytes: 8388608});
            return bytes === null ? null : new TextDecoder().decode(bytes);
          };
        }""", module)

        def check(condition: bool, label: str) -> None:
            nonlocal count
            if not condition:
                raise AssertionError(label)
            count += 1
            print("PASS", label)

        page.locator("summary").click()
        check(page.locator('#image-folder').evaluate('(e) => e.webkitdirectory === true'), "native directory input")
        page.locator('#image-folder').set_input_files(str(root))
        check(page.locator('#image-folder').evaluate('(e) => [...e.files].every(f => f.webkitRelativePath.startsWith("project/"))'), "real picker-relative paths")
        check(page.evaluate('grants.length === 0'), "selection does not grant before Apply")
        page.locator('#image-document-path').fill('docs/guide.md')
        page.locator('#apply-image-folder').click()
        check(page.evaluate('active.count === 2'), "only images admitted")
        check(page.evaluate('read("../images/plot.png")') == 'first image bytes', "actual nested file contents")
        check(page.evaluate('read("guide.md")') is None, "Markdown contents never returned")
        check(page.evaluate('read("../../project/images/plot.png")') is None, "root escape refused")
        check(page.evaluate('read("../images/a%5Bb%5D%28c%29.jpg")') == 'second image bytes', "escaped filename read")
        check(page.evaluate('active.references.includes("![a\\\\[b\\\\](c).jpg](../images/a%5Bb%5D%28c%29.jpg)")'), "escaped insertion reference")
        page.locator('#image-document-path').fill('../invalid.md')
        page.locator('#apply-image-folder').click()
        check(page.evaluate('grants.length === 1'), "invalid rebase preserves prior grant")
        page.evaluate('ui.suspend()')
        check(page.locator('#image-folder').evaluate('(e) => e.files.length === 0 && e.disabled'), "suspension drops native FileList")
        page.evaluate('ui.resume()')
        check(page.locator('#apply-image-folder').is_disabled(), "resume requires a fresh selection")
        check(page.locator('#source').input_value() == 'unchanged', "source unchanged")
        page.evaluate('ui.dispose(); ui.dispose()')
        check(page.locator('#image-folder').count() == 0, "owned DOM removed on disposal")
        check(not requests, "no network requests")
        browser.close()
    print(f"{count} Chromium checks passed; scratch fixtures: {root}")


if __name__ == "__main__":
    main()
