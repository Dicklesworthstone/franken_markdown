#!/usr/bin/env python3
"""Production reader in Chromium; native-wire sessions are explicit doubles."""
import argparse
from pathlib import Path
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("--chromium", required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
names = ["flow_reading.mjs", "flow-reader.js", "tests/flow_reader_lists.browser.mjs",
         "tests/flow_reading_fixtures.mjs", "tests/flow_reader.browser.mjs"]
sources = {name: (root / name).read_text(encoding="utf-8") for name in names}
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
    try:
        page = browser.new_page(user_agent="OpenAI File Downloader, XaiImageApiFetch/1.0")
        page.route("**/*", lambda route: route.abort())
        page.set_content("<!doctype html><html lang='en'><body></body></html>")
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
            module('tests/flow_reading_fixtures.mjs');
            module('tests/flow_reader.browser.mjs', {'../flow-reader.js': 'flow-reader.js',
              './flow_reading_fixtures.mjs': 'tests/flow_reading_fixtures.mjs'});
            module('tests/flow_reader_lists.browser.mjs', {'../flow-reader.js': 'flow-reader.js'});
            return [...await (await import(urls.get('tests/flow_reader.browser.mjs'))).run(),
              ...await (await import(urls.get('tests/flow_reader_lists.browser.mjs'))).run()];
          } finally { for (const url of urls.values()) URL.revokeObjectURL(url); }
        }""", sources)
        for name in results:
            print(f"PASS: {name}")
        print(f"Passed {len(results)} tests in Chromium {browser.version}")
    finally:
        browser.close()
