"""Real Chromium pixels and cooperative scheduling; synthetic native/font data.

Requires developer-installed Playwright and Chromium. Loads local module bytes
as Blob modules, without a web server or network. Does not build generated WASM.
"""
import argparse
import json
import shutil
from pathlib import Path
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--chromium", default=shutil.which("chromium") or shutil.which("google-chrome"))
args = parser.parse_args()
if not args.chromium:
    parser.error("pass --chromium with the path to an installed Chromium browser")
root = Path(__file__).resolve().parents[1]
names = ["flow_session.mjs", "flow_outlines.mjs", "flow-canvas.js",
         "tests/flow_canvas_viewport_fixtures.mjs", "tests/flow_canvas_viewport.browser.mjs"]
modules = {name: (root / name).read_text() for name in names}
with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=args.chromium, headless=True,
                               args=["--no-sandbox", "--disable-dev-shm-usage"])
    try:
        page = browser.new_page()
        page.set_content('<pre id="result" data-status="running">Running</pre>')
        page.evaluate("""async modules => {
          const urls = [], load = source => {
            const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
            urls.push(url); return url;
          };
          try {
            const session = load(modules['flow_session.mjs']);
            const outlines = load(modules['flow_outlines.mjs'].replaceAll('"./flow_session.mjs"', JSON.stringify(session)));
            const canvas = load(modules['flow-canvas.js'].replaceAll('"./flow_session.mjs"', JSON.stringify(session))
              .replaceAll('"./flow_outlines.mjs"', JSON.stringify(outlines)));
            const fixtures = load(modules['tests/flow_canvas_viewport_fixtures.mjs']);
            const suite = load(modules['tests/flow_canvas_viewport.browser.mjs']
              .replaceAll('"../flow-canvas.js"', JSON.stringify(canvas))
              .replaceAll('"./flow_canvas_viewport_fixtures.mjs"', JSON.stringify(fixtures)));
            await import(suite);
          } finally { for (const url of urls) URL.revokeObjectURL(url); }
        }""", modules)
        report = json.loads(page.locator("#result").inner_text())
        print(json.dumps(report, indent=2), flush=True)
    finally:
        browser.close()
if report["failed"]:
    raise SystemExit(1)
