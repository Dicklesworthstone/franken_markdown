#!/usr/bin/env python3
"""Actual live-page HTML, entrypoint and navigation controls in Chromium.

Native hit/reading/preview and unrelated page controllers are explicit doubles.
No generated WASM, glyph hit accuracy, OS interaction or accessibility
conformance is claimed. All requests are intercepted; external I/O is blocked.
Requires Python Playwright and an installed Chromium, matching other DOM lanes.
"""
import argparse
import json
from pathlib import Path
import re
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
CONTROLLER = r'''
const h = window.navHarness = { calls: [], delay: false, target: '#last-section', inputEvents: 0 };
document.querySelector('#source').addEventListener('input', () => h.inputEvents++);
export function createPreviewController({ painter, onState }) {
  let state = { status: 'idle', document: null, frame: null }, source = null, doc = null, intent = null;
  const controller = {
    disposed: false, get state() { return state; },
    update(next) {
      if (controller.disposed || JSON.stringify(next) === JSON.stringify(intent)) return;
      intent = { ...next };
      if (next.source !== source || !doc) {
        source = next.source;
        const start = source.indexOf('\n\n') + 2;
        let end = source.indexOf('\n\n', start); if (end < 0) end = source.length;
        const bytes = value => new TextEncoder().encode(value).length;
        const span = { startByte: bytes(source.slice(0, start)), endByte: bytes(source.slice(0, end)) };
        const token = { revision: '1', layoutRevision: '1' };
        h.range = { start, end }; h.source = source;
        doc = { token, nodes: [{ enclosingSourceSpan: span },
          { enclosingSourceSpan: { startByte: 0, endByte: bytes(source.split('\n')[0]) } }],
          assertCurrent() { if (controller.disposed || state.document !== this) throw new Error('stale document'); },
          locateFragment(target) { this.assertCurrent(); return target === '#last-section' ? { ...token, nodeIndex: 1 } : null; },
        };
      }
      const frame = Object.freeze({ ...doc.token, width: next.width, height: next.height,
        pixelRatio: next.pixelRatio, scrollX: 0, scrollY: next.scrollY, glyphs: 1,
        totalBounds: { x: 0, y: 0, width: next.width, height: 2400 } });
      painter.frame = frame;
      state = { status: 'ready', frame, document: doc, readingPending: false, readingError: null, images: null };
      h.controller = controller; h.painter = painter;
      h.response = () => ({ schemaVersion: 1, ...frame, linkTarget: h.target,
        hit: { itemIndex: 900, byteOffset: 999, utf16Offset: 999, enclosingSourceSpan: doc.nodes[0].enclosingSourceSpan } });
      onState(state);
    },
    locateReading(index, snapshot, input) {
      if (controller.disposed || state.status !== 'ready' || snapshot !== doc || source !== input)
        throw Object.assign(new Error('wrong source'), { code: 'STALE_REVISION' });
      snapshot.assertCurrent();
      return { ...doc.token, nodeIndex: index, enclosingSourceSpan: doc.nodes[index].enclosingSourceSpan,
        sourceRange: index ? { start: 0, end: source.split('\n')[0].length } : { ...h.range },
        bounds: { x: 0, y: index ? 900 : 50, width: 100, height: 20 } };
    },
    restart() { intent = null; doc = null; painter.frame = null; state = { status: 'idle' }; onState(state); },
    dispose() { controller.disposed = true; painter.dispose(); state = { status: 'disposed' }; onState(state); },
  };
  return controller;
}
'''
PAINTER = r'''
export class FlowCanvasRenderer {
  frame = null; disposed = false;
  hitTest(x, y) {
    const h = window.navHarness, response = h.response();
    h.calls.push([x, y]);
    if (!h.delay) return Promise.resolve(response);
    return new Promise((resolve, reject) => { h.resolve = () => resolve(response); h.reject = () => reject(new Error('late worker')); });
  }
  clear() { this.frame = null; }
  dispose() { this.disposed = true; this.clear(); }
}
'''
STUBS = {
    "/wasm/demo/flow_preview_controller.mjs": CONTROLLER,
    "/wasm/flow-canvas.js": PAINTER,
    "/wasm/flow-assets.js": "export class FlowImageAssets {}",
    "/wasm/flow-reader.js": "export const readFlowDocument = () => {};",
    "/wasm/flow-worker.js": "export const createWorkerFlowSession = () => {};",
    "/wasm/demo/flow-source.js": "// Unrelated source/persistence UI explicitly excluded.",
    "/wasm/demo/flow_export_controls.mjs": "export const createExportControls = () => ({update(){},dispose(){},invalidate(){}});",
    "/wasm/demo/flow_reading_controls.mjs": "export const createReadingControls = () => ({update(){},dispose(){}});",
    "/wasm/demo/flow_render_settings.mjs": "export const createRenderSettingsControls = () => ({settings:{values:{},preview:{}},dispose(){},exportOptions(){return {}}});",
    "/wasm/demo/local_image_sources.mjs": "export const createLocalImageSources = () => ({count:0}); export const createDirectoryImageControls = () => ({clear(){},dispose(){},suspend(){},resume(){}});",
}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    args = parser.parse_args()
    results = []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
        def check(name, action):
            context = browser.new_context(viewport={"width": 1400, "height": 1000}, device_scale_factor=2)
            unexpected, errors = [], []
            context.route("**/*", lambda route: (unexpected.append(route.request.url), route.abort()))
            page = context.new_page(); page.on("pageerror", lambda error: errors.append(str(error)))
            # Use an in-memory document: navigation/network can be forbidden by
            # browser policy. Preserve the real HTML, leaving its module tags
            # inert; only import specifiers are relinked to local Blob modules.
            html = (ROOT / "wasm/demo/flow-canvas.html").read_text()
            html = re.sub(r'(<script type="module") src=', r'\1 data-test-source=', html)
            page.set_content(html)
            modules = dict(STUBS)
            modules["/wasm/demo/flow_canvas_navigation.mjs"] = (ROOT / "wasm/demo/flow_canvas_navigation.mjs").read_text()
            entry = (ROOT / "wasm/demo/flow-canvas.js").read_text()
            page.evaluate(r"""async ({modules, entry}) => {
                const urls = new Map();
                for (const [path, source] of Object.entries(modules))
                    urls.set(path, URL.createObjectURL(new Blob([source], {type:'text/javascript'})));
                const linked = entry.replace(/(from\s+)["']([^"']+)["']/g, (match, prefix, specifier) => {
                    const path = new URL(specifier, 'http://localhost/wasm/demo/flow-canvas.js').pathname;
                    if (!urls.has(path)) throw new Error('unmapped module '+path);
                    return prefix + JSON.stringify(urls.get(path));
                });
                const url = URL.createObjectURL(new Blob([linked], {type:'text/javascript'}));
                try { await import(url); }
                finally { URL.revokeObjectURL(url); for (const value of urls.values()) URL.revokeObjectURL(value); }
            }""", {"modules": modules, "entry": entry})
            page.wait_for_function("window.navHarness?.controller?.state.status === 'ready'")
            action(page)
            assert not errors, errors
            assert not unexpected, unexpected
            results.append(name)
            context.close()
        def inspect(page):
            page.locator('#preview').click(position={"x": 30, "y": 25})
            page.wait_for_function("!document.querySelector('#canvas-source').disabled")
        def source_action(page):
            inspect(page)
            page.locator('#canvas-source').focus(); page.keyboard.press('Enter')
            value = page.evaluate("""() => {const s=document.querySelector('#source'), h=navHarness;
                return {start:s.selectionStart,end:s.selectionEnd,range:h.range,same:s.value===h.source,events:h.inputEvents,focus:document.activeElement===s,calls:h.calls};} """)
            assert (value['start'], value['end']) == (value['range']['start'], value['range']['end'])
            assert value['same'] and value['focus'] and value['events'] == 0
            assert len(value['calls']) == 1
            assert abs(value['calls'][0][0] - 30) < 1 and abs(value['calls'][0][1] - 25) < 1
        check('live entrypoint and native Enter source selection preserve Markdown', source_action)
        def follow(page):
            inspect(page); before = page.url
            assert page.locator('#viewport').evaluate('(v)=>v.scrollTop') == 0
            page.locator('#canvas-follow').focus(); page.keyboard.press('Enter')
            page.wait_for_function("document.querySelector('#viewport').scrollTop === 884")
            assert page.url == before and page.evaluate('location.hash') == ''
            assert page.locator('#canvas-source').is_disabled()
            assert page.locator('#source').input_value() == page.evaluate('navHarness.source')
        check('native keyboard Follow scrolls internally without browser navigation', follow)
        def external(page):
            page.evaluate("navHarness.target='https://example.com/<script>'")
            inspect(page)
            assert page.locator('#canvas-follow').is_disabled()
            assert page.locator('script').count() == 2
            assert 'external' in page.locator('#hit').inner_text()
        check('external targets stay inert without DOM injection or requests', external)
        def modifiers(page):
            page.locator('#preview').click(position={"x": 30, "y": 25}, modifiers=['Control'])
            page.locator('#preview').click(position={"x": 30, "y": 25}, button='middle')
            assert page.evaluate('navHarness.calls.length') == 0
        check('modified and middle clicks do not issue hit requests', modifiers)
        def unsent(page):
            inspect(page)
            page.evaluate("document.querySelector('#source').value += ' unsent'")
            page.locator('#canvas-source').click()
            assert page.locator('#canvas-source').is_disabled()
            assert 'STALE' in page.locator('#hit').inner_text() or 'NAVIGATION' in page.locator('#hit').inner_text()
        check('unsubmitted programmatic source changes revoke navigation', unsent)
        def late(page):
            page.evaluate('navHarness.delay=true')
            page.locator('#preview').click(position={"x": 30, "y": 25})
            page.wait_for_function('navHarness.calls.length === 1')
            page.evaluate("""() => {const s=document.querySelector('#source');s.value+=' edited';s.dispatchEvent(new Event('input',{bubbles:true}));} """)
            page.evaluate('navHarness.resolve()')
            page.wait_for_timeout(50)
            assert page.locator('#canvas-source').is_disabled()
            assert page.evaluate('navHarness.calls.length') == 1
        check('late hit after source input cannot publish an old source range', late)
        def restart(page):
            page.evaluate('navHarness.delay=true')
            page.locator('#preview').click(position={"x": 30, "y": 25})
            page.wait_for_function('navHarness.calls.length === 1')
            page.locator('#restart').click()
            page.wait_for_function("navHarness.controller.state.status === 'ready'")
            page.locator('#preview').click(position={"x": 30, "y": 25})
            assert page.evaluate('navHarness.calls.length') == 1
            page.evaluate('navHarness.reject();navHarness.delay=false')
            page.wait_for_timeout(50); inspect(page)
            assert page.evaluate('navHarness.calls.length') == 2
        check('restart retains one physical hit slot and recovers from late rejection', restart)
        def focus_change(page):
            inspect(page)
            page.evaluate("""() => {document.querySelector('#source').addEventListener('focus', e=>{
                e.target.value='replacement'; e.target.setSelectionRange(0,0);
                e.target.dispatchEvent(new Event('input',{bubbles:true}));
            }, {once:true});} """)
            page.locator('#canvas-source').click()
            assert page.locator('#source').input_value() == 'replacement'
            assert page.locator('#source').evaluate('(s)=>s.selectionEnd') == 0
        check('focus-triggered replacement is fenced before textarea selection', focus_change)
        def same_replacement(page):
            inspect(page)
            page.evaluate("document.querySelector('#source').dispatchEvent(new Event('fmd-document-replaced'))")
            assert page.locator('#canvas-source').is_disabled()
        check('same-text file replacement revokes old Canvas authority', same_replacement)
        def suspension(page):
            page.evaluate('navHarness.delay=true')
            page.locator('#preview').click(position={"x": 30, "y": 25})
            page.wait_for_function('navHarness.calls.length === 1')
            page.evaluate("window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true}))")
            assert page.locator('#canvas-source').is_disabled()
            page.evaluate("window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}))")
            page.wait_for_function("navHarness.controller.state.status === 'ready'")
            page.locator('#preview').click(position={"x": 30, "y": 25})
            assert page.evaluate('navHarness.calls.length') == 1
            page.evaluate('navHarness.resolve();navHarness.delay=false')
            page.wait_for_timeout(50); inspect(page)
            assert page.evaluate('navHarness.calls.length') == 2
        check('persisted page suspension never revives hits or duplicates listeners', suspension)
        browser.close()
    print(json.dumps({"passed": len(results), "checks": results, "unexpected_requests": 0,
        "scope": "actual HTML/entrypoint/DOM; explicit native, painter and unrelated-controller doubles"}, indent=2))

if __name__ == '__main__':
    main()
