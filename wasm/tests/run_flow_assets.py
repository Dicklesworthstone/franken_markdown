#!/usr/bin/env python3
"""Run real browser raster tests from local source bytes; no server or network."""
import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("--chromium", required=True, help="Installed Chromium executable")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
sources = {name: (root / name).read_text() for name in [
    "flow_raster.mjs", "flow-assets.js", "tests/flow_image_fixtures.mjs", "tests/flow_assets.browser.mjs"
]}
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
    try:
        page = browser.new_page()
        page.route("**/*", lambda route: route.abort())
        page.set_content("<!doctype html><meta charset='utf-8'><title>Flow image tests</title>")
        results = page.evaluate("""async sources => {
          const urls = new Map();
          const module = (name, replacements = {}) => {
            let text = sources[name];
            for (const [from, to] of Object.entries(replacements)) text = text.replaceAll(from, urls.get(to));
            const url = URL.createObjectURL(new Blob([text], {type: 'text/javascript'}));
            urls.set(name, url);
          };
          try {
            module('flow_raster.mjs');
            module('flow-assets.js', {'./flow_raster.mjs': 'flow_raster.mjs'});
            module('tests/flow_image_fixtures.mjs');
            module('tests/flow_assets.browser.mjs', {'../flow-assets.js': 'flow-assets.js',
              './flow_image_fixtures.mjs': 'tests/flow_image_fixtures.mjs'});
            return await (await import(urls.get('tests/flow_assets.browser.mjs'))).run();
          } finally { for (const url of urls.values()) URL.revokeObjectURL(url); }
        }""", sources)
        for name in results:
            print(f"PASS: {name}")
        print(json.dumps({"passed": len(results), "browser": browser.version}))
    finally:
        browser.close()
