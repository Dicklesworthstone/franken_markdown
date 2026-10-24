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
    "flow_raster.mjs", "flow-assets.js", "flow_session.mjs", "pdf_page.mjs", "pdf_running.mjs",
    "render_typography.mjs", "flow_export.mjs", "flow_reading.mjs", "demo/flow_preview_export.mjs",
    "demo/flow_preview_controller.mjs", "demo/flow_export_controls.mjs",
    "tests/flow_image_fixtures.mjs", "tests/flow_assets.browser.mjs"
]}
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
    try:
        page = browser.new_page(user_agent="franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)")
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
            for (const name of ['flow_session.mjs','pdf_page.mjs','pdf_running.mjs','render_typography.mjs','flow_reading.mjs']) module(name);
            module('flow_export.mjs', {'./flow_session.mjs':'flow_session.mjs','./pdf_page.mjs':'pdf_page.mjs',
              './pdf_running.mjs':'pdf_running.mjs','./render_typography.mjs':'render_typography.mjs'});
            module('demo/flow_preview_export.mjs', {'../flow_session.mjs':'flow_session.mjs','../flow_export.mjs':'flow_export.mjs'});
            module('demo/flow_preview_controller.mjs', {'../flow_reading.mjs':'flow_reading.mjs','../flow_session.mjs':'flow_session.mjs',
              './flow_preview_export.mjs':'demo/flow_preview_export.mjs'});
            module('demo/flow_export_controls.mjs', {'../flow_session.mjs':'flow_session.mjs','../flow_export.mjs':'flow_export.mjs'});
            module('tests/flow_assets.browser.mjs', {'../flow-assets.js': 'flow-assets.js',
              './flow_image_fixtures.mjs': 'tests/flow_image_fixtures.mjs','../demo/flow_preview_controller.mjs':'demo/flow_preview_controller.mjs',
              '../demo/flow_export_controls.mjs':'demo/flow_export_controls.mjs','../flow_export.mjs':'flow_export.mjs'});
            return await (await import(urls.get('tests/flow_assets.browser.mjs'))).run();
          } finally { for (const url of urls.values()) URL.revokeObjectURL(url); }
        }""", sources)
        for name in results:
            print(f"PASS: {name}")
        print(json.dumps({"passed": len(results), "browser": browser.version}))
    finally:
        browser.close()
