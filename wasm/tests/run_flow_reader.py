#!/usr/bin/env python3
"""Real DOM/selection tests from local modules. Sessions are explicit doubles."""
import argparse
from pathlib import Path
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("--chromium", required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
sources = {name: (root / name).read_text() for name in [
    "flow_reading.mjs", "flow-reader.js", "flow_session.mjs", "demo/flow_preview_controller.mjs",
    "demo/flow_reading_controls.mjs", "tests/flow_reading_fixtures.mjs", "tests/flow_reader.browser.mjs",
    "tests/flow_preview_reading.browser.mjs"
]}
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
    try:
        page = browser.new_page()
        page.route("**/*", lambda route: route.abort())
        page.set_content((root / "demo/flow-canvas.html").read_text().replace('<script type="module" src="./flow-canvas.js"></script>', ""))
        results = page.evaluate("""async sources => {
          const urls = new Map();
          const module = (name, replacements = {}) => {
            let text = sources[name];
            for (const [from, to] of Object.entries(replacements)) text = text.replaceAll(from, urls.get(to));
            urls.set(name, URL.createObjectURL(new Blob([text], { type: 'text/javascript' })));
          };
          try {
            module('flow_reading.mjs');
            module('flow-reader.js', {'./flow_reading.mjs': 'flow_reading.mjs'});
            module('flow_session.mjs');
            module('demo/flow_preview_controller.mjs', {'../flow_session.mjs': 'flow_session.mjs', '../flow_reading.mjs': 'flow_reading.mjs'});
            module('demo/flow_reading_controls.mjs', {'../flow-reader.js': 'flow-reader.js'});
            module('tests/flow_reading_fixtures.mjs');
            module('tests/flow_reader.browser.mjs', {'../flow-reader.js': 'flow-reader.js',
              './flow_reading_fixtures.mjs': 'tests/flow_reading_fixtures.mjs'});
            module('tests/flow_preview_reading.browser.mjs', {'../flow-reader.js': 'flow-reader.js',
              '../demo/flow_reading_controls.mjs': 'demo/flow_reading_controls.mjs',
              '../demo/flow_preview_controller.mjs': 'demo/flow_preview_controller.mjs',
              './flow_reading_fixtures.mjs': 'tests/flow_reading_fixtures.mjs'});
            return [...await (await import(urls.get('tests/flow_reader.browser.mjs'))).run(),
              ...await (await import(urls.get('tests/flow_preview_reading.browser.mjs'))).run()];
          } finally { for (const url of urls.values()) URL.revokeObjectURL(url); }
        }""", sources)
        for name in results:
            print(f"PASS: {name}")
        print(f"Passed {len(results)} tests in Chromium {browser.version}")
    finally:
        browser.close()
