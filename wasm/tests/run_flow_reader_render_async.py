"""Run real semantic-DOM cancellation regressions with local modules in Chromium.

Requires developer-installed Playwright and Chromium. No server, network,
Markdown parser double or browser dependencies added to the production package.
Native reading pages are fixtures, not generated Rust/WASM execution.
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
    parser.error("pass --chromium with an installed Chromium path")
root = Path(__file__).resolve().parents[1]
names = ["flow_reading.mjs", "flow-reader.js", "tests/flow_reader_render_async.browser.mjs"]
modules = {name: (root / name).read_text(encoding="utf-8") for name in names}
with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=args.chromium, headless=True,
                               args=["--no-sandbox", "--disable-dev-shm-usage"])
    try:
        page = browser.new_page()
        page.route("**/*", lambda route: route.abort())
        page.set_content('<pre id="result">Running</pre>')
        page.evaluate("""async modules => {
          const urls = [], load = source => {
            const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
            urls.push(url); return url;
          };
          try {
            const reading = load(modules['flow_reading.mjs']);
            const reader = load(modules['flow-reader.js'].replaceAll('"./flow_reading.mjs"', JSON.stringify(reading)));
            const suite = load(modules['tests/flow_reader_render_async.browser.mjs']
              .replaceAll('"../flow-reader.js"', JSON.stringify(reader)));
            await import(suite);
          } finally { for (const url of urls) URL.revokeObjectURL(url); }
        }""", modules)
        result = json.loads(page.locator("#result").inner_text())
        print(json.dumps(result, indent=2), flush=True)
    finally:
        browser.close()
if result["failed"]:
    raise SystemExit(1)
