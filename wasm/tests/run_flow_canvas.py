"""Real Chromium Canvas pixel tests. Requires developer-installed Playwright.

Loads local module bytes as Blob ES modules; no web server or network required.
The test suite uses synthetic glyph fixtures, not a generated WASM renderer.
"""
import argparse
import json
import shutil
from pathlib import Path
from playwright.sync_api import sync_playwright
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--chromium', default=shutil.which('chromium') or shutil.which('google-chrome'))
args = parser.parse_args()
if not args.chromium:
    parser.error('pass --chromium with the path to an installed Chromium browser')
root = Path(__file__).resolve().parents[1]
names=['flow_session.mjs','flow_outlines.mjs','flow-canvas.js','tests/flow_canvas.browser.mjs']
modules={name:(root/name).read_text() for name in names}
with sync_playwright() as p:
 browser=p.chromium.launch(executable_path=args.chromium,headless=True,timeout=10000,args=['--no-sandbox','--disable-dev-shm-usage'])
 page=browser.new_page()
 page.on('pageerror',lambda err:print('PAGE ERROR:',err,flush=True))
 page.set_content('<pre id="result" data-status="running">Running</pre>')
 page.evaluate('''async modules => {
   const urls=[];
   const load=source=>{const url=URL.createObjectURL(new Blob([source],{type:"text/javascript"}));urls.push(url);return url;};
   try {
     const session=load(modules['flow_session.mjs']);
     const outlines=load(modules['flow_outlines.mjs'].replaceAll('"./flow_session.mjs"',JSON.stringify(session)));
     const canvas=load(modules['flow-canvas.js'].replaceAll('"./flow_session.mjs"',JSON.stringify(session)).replaceAll('"./flow_outlines.mjs"',JSON.stringify(outlines)));
     const suite=load(modules['tests/flow_canvas.browser.mjs'].replaceAll('"../flow-canvas.js"',JSON.stringify(canvas)));
     await import(suite);
   } finally {for (const url of urls)URL.revokeObjectURL(url);}
 }''',modules)
 r=json.loads(page.locator('#result').inner_text())
 print(json.dumps(r,indent=2),flush=True)
 browser.close()
if r['failed']:raise SystemExit(1)
