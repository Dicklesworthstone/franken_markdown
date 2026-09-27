"""Real Chromium tests of the production find/replace UI and renderer.
This host adapts input/save/preview events, not the full native controller or WASM.
Run: python tests/interactive_search_browser.py (CHROMIUM overrides the browser path)
"""
from pathlib import Path
import json
import os
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SEARCH = (ROOT / 'src/interactive_search.js').read_text()
RENDERER = (ROOT / 'src/interactive_renderer.js').read_text()
CSS = (ROOT / 'src/interactive.rs').read_text().split('const INTERACTIVE_CSS: &str = r#"', 1)[1].split('"#;', 1)[0]
HOST = '''<!doctype html><html><head><meta charset="utf-8"></head><body>
<header class="fmd-app-header"><div><button id="btn-toggle-view">Toggle view</button><button id="btn-save-markdown">Save Markdown</button></div></header>
<div id="fmd-app-body" class="fmd-app-body view-split"><section id="editor-pane" class="fmd-editor-pane"><textarea id="fmd-editor"></textarea></section><main id="preview" class="fmd-preview-pane"><div id="fmd-content"></div></main></div>
<div id="other-controls"><input id="other-input"></div><output id="render-error"></output></body></html>'''
HOST = HOST.replace('</head>', '<style>' + CSS + '</style></head>')
HOST_JS = '''source => {
  const editor = document.querySelector('#fmd-editor');
  editor.value = source;
  window.anchor = {source, view: editor.value};
  window.sourceValue = () => editor.value === anchor.view ? anchor.source : editor.value;
  window.edits = [];
  window.saved = [];
  window.renderSource = () => {
    try {
      document.querySelector('#fmd-content').innerHTML = parseMarkdownClient(sourceValue());
      document.querySelector('#render-error').textContent = '';
    } catch (error) { document.querySelector('#render-error').textContent = error.message; }
  };
  editor.addEventListener('input', () => { edits.push(sourceValue()); renderSource(); });
  document.querySelector('#btn-save-markdown').onclick = () => saved.push(sourceValue());
  document.querySelector('#btn-toggle-view').onclick = () => {
    const body = document.querySelector('#fmd-app-body');
    body.classList.toggle('view-read'); body.classList.toggle('view-split');
  };
  renderSource();
}'''

checks = []
with sync_playwright() as pw:
    browser = pw.chromium.launch(executable_path=os.environ.get('CHROMIUM', '/usr/bin/chromium'), headless=True, args=['--no-sandbox'])
    page = browser.new_page()
    page.set_default_timeout(4000)
    requests, errors = [], []
    page.route('**/*', lambda route: (requests.append(route.request.url), route.abort()))
    page.on('pageerror', lambda error: errors.append(str(error)))
    def boot(source):
        page.goto('about:blank')
        page.set_content(HOST)
        page.add_script_tag(content=RENDERER)
        page.evaluate(HOST_JS, source)
        page.add_script_tag(content=SEARCH)
        page.locator('#btn-source-search').click()
    def find(query, case=True):
        page.locator('#fmd-find-query').fill(query)
        page.locator('#fmd-find-case').set_checked(case)
        page.locator('#btn-find-source').click()
    def replace(query, text, all=True, case=True):
        find(query, case)
        page.locator('#fmd-find-replacement').fill(text)
        page.locator('#btn-replace-all-source' if all else '#btn-replace-source').click()
    def text(): return page.locator('#fmd-editor').input_value()
    def count(): return page.evaluate('edits.length')
    def status(): return page.locator('#fmd-source-search-status').inner_text()
    def select(): return page.locator('#fmd-editor').evaluate('(e)=>[e.selectionStart,e.selectionEnd,e.selectionDirection]')
    def disabled(id): return page.locator('#'+id).is_disabled()
    def check(name): checks.append(name)

    boot('# Topic\n\nfoo *foo* `foo`\n\n[foo](foo)')
    before = text()
    find('foo')
    assert 'of 5' in status(), status()
    assert select()[:2] == [9, 12], select()
    page.locator('#btn-find-next').click(); assert select()[:2] == [14, 17]
    page.locator('#btn-find-previous').click(); assert select()[:2] == [9, 12]
    page.locator('#fmd-find-replacement').fill('bar')
    page.locator('#btn-replace-all-source').click()
    assert text() == before.replace('foo', 'bar') and count() == 1
    assert 'Replaced 5 matches' in status() and not disabled('btn-undo-source-replace')
    assert 'bar' in page.locator('#fmd-content').inner_text()
    page.locator('#btn-undo-source-replace').click()
    assert text() == before and count() == 2 and select()[:2] == [9,12]
    assert disabled('btn-undo-source-replace')
    check('all five Markdown/code/link matches replace atomically, preview updates, one-step undo restores source and selection')

    boot('one x two x three')
    find('x'); page.locator('#btn-find-next').click()
    page.locator('#fmd-find-replacement').fill('xx')
    page.locator('#btn-replace-source').click()
    assert text() == 'one x two xx three' and count() == 1
    page.locator('#btn-find-source').click()
    assert 'of 3' in status()
    check('single replacement changes only the selected original range; later explicit search sees replacement-introduced matches')

    boot('😀 İ K K k\nline\nline')
    replace('k', 'λ', case=False)
    assert text() == '😀 İ λ λ λ\nline\nline'
    page.locator('#btn-undo-source-replace').click()
    replace('line\nline', '$&\n<unsafe>')
    assert text().endswith('$&\n<unsafe>')
    assert page.locator('#fmd-content unsafe').count() == 0
    check('Unicode-insensitive offsets, multiline queries and literal dollar/HTML replacements remain correct and inert')

    boot('foo foo')
    replace('foo', 'foo')
    assert text() == 'foo foo' and count() == 0 and disabled('btn-undo-source-replace')
    replace('foo', '')
    assert text() == ' ' and count() == 1
    check('identical replacements do not emit input; empty replacements delete all matched text')

    boot('foo foo')
    find('foo'); page.locator('#fmd-find-replacement').fill('bar')
    page.locator('#fmd-editor').fill('foo foo newer')
    assert disabled('btn-replace-all-source') and disabled('btn-undo-source-replace')
    find('foo')
    page.locator('#fmd-editor').evaluate('(e)=>{e.value="silent newer foo"}')
    page.locator('#btn-replace-all-source').click()
    assert text() == 'silent newer foo' and 'choose Find again' in status()
    check('event-driven and silent source changes both reject stale replacement sets')

    boot('foo foo')
    find('foo'); page.locator('#fmd-find-replacement').fill('bar')
    page.locator('#fmd-editor').evaluate('(e)=>e.setSelectionRange(0,0)')
    page.locator('#btn-replace-source').click()
    assert text() == 'foo foo' and 'Select a match' in status()
    page.locator('#btn-find-next').click(); page.locator('#btn-replace-source').click()
    assert text() == 'foo bar'
    page.locator('#fmd-editor').fill('newer')
    assert disabled('btn-undo-source-replace')
    check('single replace requires the displayed match selection; a later user edit invalidates undo')

    boot('foo')
    replace('foo', 'bar')
    page.locator('#fmd-editor').evaluate('(e)=>{e.value="silent new"}')
    page.locator('#btn-undo-source-replace').click()
    assert text() == 'silent new' and 'discard newer' in status()
    check('undo checks exact current source even without an input event')

    boot('foo foo')
    find('foo')
    page.locator('#fmd-find-query').fill('bar'); assert disabled('btn-replace-all-source')
    find('foo')
    page.locator('#fmd-find-case').uncheck(); assert disabled('btn-replace-all-source')
    check('query and case-mode changes retire the old result set')

    boot('foo foo')
    find('foo')
    page.locator('#fmd-find-replacement').fill('x')
    page.locator('#fmd-editor').dispatch_event('compositionstart')
    assert disabled('btn-replace-all-source') and disabled('btn-find-source')
    page.locator('#fmd-editor').dispatch_event('compositionend')
    assert not disabled('btn-find-source') and disabled('btn-replace-all-source')
    find('foo'); page.locator('#btn-replace-all-source').click()
    assert text() == 'x x'
    page.evaluate('window.dispatchEvent(new Event("pagehide"))')
    assert disabled('btn-undo-source-replace') and disabled('btn-find-source')
    page.evaluate('window.dispatchEvent(new Event("pageshow"))')
    assert not disabled('btn-find-source') and disabled('btn-undo-source-replace')
    check('composition and pagehide fence mutations, release retained undo, and resume without stale matches')

    boot('foo')
    page.evaluate('''() => {
      let used = false;
      document.querySelector('#fmd-editor').addEventListener('input', event => {
        if (used) return; used = true;
        const editor = event.target;
        editor.value += '!'; editor.dispatchEvent(new Event('input', {bubbles:true}));
        editor.value = editor.value.slice(0,-1); editor.dispatchEvent(new Event('input', {bubbles:true}));
      });
    }''')
    replace('foo', 'bar')
    assert text() == 'bar' and disabled('btn-undo-source-replace') and 'Further source activity' in status()
    check('synchronous edit-and-revert ABA input cannot leave an older undo transaction authorized')

    boot('foo foo')
    find('foo')
    page.locator('#fmd-find-replacement').fill('x')
    page.locator('#fmd-editor').evaluate('''e => {
      const real = e.setRangeText.bind(e);
      e.setRangeText = (...args) => { real(...args); e.value = e.value.slice(0,-1); };
    }''')
    page.locator('#btn-replace-all-source').click()
    assert text() == 'foo foo' and count() == 0 and 'complete replacement' in status()
    check('incomplete textarea acceptance rolls back before publishing any input event')

    boot('foo foo')
    replace('foo', 'bar')
    find('bar'); page.locator('#fmd-find-replacement').fill('baz')
    page.locator('#fmd-editor').evaluate('''e => {
      const real = e.setRangeText.bind(e); let once = true;
      e.setRangeText = (...args) => { real(...args); if (once) { once = false; throw Error('write refused'); } };
    }''')
    page.locator('#btn-replace-all-source').click()
    assert text() == 'bar bar' and count() == 1 and not disabled('btn-undo-source-replace')
    page.locator('#btn-undo-source-replace').click()
    assert text() == 'foo foo' and count() == 2
    check('a refused second batch retains the prior successful batch undo without publishing a partial edit')

    boot('x' * 10001)
    find('x')
    assert 'No partial' in status() and disabled('btn-replace-all-source') and count() == 0
    page.locator('#fmd-editor').fill('x' * 10000)
    find('x'); page.locator('#fmd-find-replacement').fill('y' * 65536)
    old_count = count()
    page.locator('#btn-replace-all-source').click()
    assert '32 MiB' in status() and text() == 'x'*10000 and count() == old_count
    check('match overflow and oversized batch expansion leave all source untouched and never offer truncated replace-all')

    boot('foo')
    page.locator('#btn-close-source-search').click()
    page.locator('#fmd-editor').focus(); page.keyboard.press('Control+f')
    assert page.locator('#fmd-source-search').is_visible() and page.locator('#fmd-find-query').evaluate('(e)=>document.activeElement===e')
    page.keyboard.press('Control+h')
    assert page.locator('#fmd-find-replacement').evaluate('(e)=>document.activeElement===e')
    page.keyboard.press('Escape'); assert not page.locator('#fmd-source-search').is_visible()
    assert page.locator('#fmd-editor').evaluate('(e)=>document.activeElement===e')
    assert page.locator('#other-input').evaluate('''e => {
      const key = new KeyboardEvent('keydown',{key:'f',ctrlKey:true,bubbles:true,cancelable:true});
      e.dispatchEvent(key); return !key.defaultPrevented;
    }''')
    check('source-scoped keyboard shortcuts work without hijacking Find in unrelated controls')

    boot('foo')
    page.locator('#btn-close-source-search').click()
    page.locator('#btn-toggle-view').click()
    page.locator('#btn-source-search').click()
    assert page.locator('#fmd-app-body').evaluate('(e)=>e.classList.contains("view-split")')
    page.evaluate('fmdInstallSourceSearch()')
    assert page.locator('#btn-source-search').count() == 1 and page.locator('#fmd-source-search').count() == 1
    check('opening search exits reading mode; installation is idempotent')

    boot('\ufefffoo\r\nfoo\0\r')
    original = page.evaluate('sourceValue()')
    replace('foo', 'bar')
    page.locator('#btn-save-markdown').click()
    assert page.evaluate('saved.at(-1)') == '\ufeffbar\nbar\0\n'
    page.locator('#btn-undo-source-replace').click()
    page.locator('#btn-save-markdown').click()
    assert page.evaluate('saved.at(-1)') == original
    check('input integration retains editor-normalized edits and restores the lossless CRLF/BOM/control source anchor on undo')

    boot('safe')
    # The 64 KiB replacement ceiling remains below the renderer's source limit;
    # several expanded matches can still intentionally reach a parser budget.
    page.locator('#fmd-editor').fill('x x x x x')
    replace('x', '[' * 60000)
    assert 'limit' in page.locator('#render-error').inner_text() and not disabled('btn-undo-source-replace')
    page.locator('#btn-save-markdown').click()
    assert len(page.evaluate('saved.at(-1)')) == 300004
    page.locator('#btn-undo-source-replace').click()
    assert text() == 'x x x x x' and page.locator('#render-error').inner_text() == ''
    check('renderer budget failure does not roll back source, disable Markdown saving, or prevent replacement undo')

    boot('foo')
    replace('foo', 'bar')
    # Values are runtime state, not default textarea markup. Reopening the saved
    # shell must not retain queries, old result sets, enabled undo, or handlers.
    shell = page.evaluate('''() => { const copy = document.documentElement.cloneNode(true); for (const script of copy.querySelectorAll('script')) script.remove(); return copy.outerHTML; }''')
    page.goto('about:blank'); page.set_content(shell)
    page.add_script_tag(content=RENDERER); page.evaluate(HOST_JS, 'bar')
    page.add_script_tag(content=SEARCH)
    assert page.locator('#fmd-find-query').input_value() == ''
    assert page.locator('#fmd-find-replacement').input_value() == ''
    assert disabled('btn-undo-source-replace') and disabled('btn-replace-all-source')
    assert page.locator('#btn-source-search').count() == 1
    check('a serialized/reopened UI shell starts with fresh query, matches and undo state')

    boot('line\n' * 3000 + 'needle')
    page.locator('#fmd-editor').evaluate('(e)=>{e.style.height="100px";e.style.width="400px";e.setSelectionRange(0,0);e.scrollTop=0}')
    find('needle')
    position = page.locator('#fmd-editor').evaluate('(e)=>({top:e.scrollTop,height:e.scrollHeight,visible:e.clientHeight})')
    assert position['top'] > position['height'] - position['visible'] * 2, position
    assert page.locator('body > div[aria-hidden="true"]').count() == 0
    check('navigation reveals off-screen matches and synchronously removes its inert measurement mirror')

    boot(('long wrapped words ' * 3000) + 'needle')
    page.locator('#fmd-editor').evaluate('(e)=>{e.style.height="100px";e.style.width="250px";e.setSelectionRange(0,0);e.scrollTop=0}')
    find('needle')
    position = page.locator('#fmd-editor').evaluate('(e)=>({top:e.scrollTop,height:e.scrollHeight,visible:e.clientHeight})')
    assert position['top'] > position['height'] - position['visible'] * 2, position
    check('wrapped single-line source uses actual textarea metrics instead of line-count approximations')

    boot('foo foo')
    find('foo'); page.locator('#fmd-find-replacement').fill('bar')
    page.locator('#fmd-find-replacement').dispatch_event('compositionstart')
    assert disabled('btn-replace-all-source') and disabled('btn-find-source')
    page.locator('#fmd-find-replacement').dispatch_event('compositionend')
    assert not disabled('btn-find-source') and disabled('btn-replace-all-source')
    assert text() == 'foo foo' and count() == 0
    check('query/replacement input-method composition also pauses source mutation')

    # The host above is deliberately minimal; apply the exact production CSS
    # here to verify a visible find panel never leaks into browser print output.
    template = (ROOT / 'src/interactive.rs').read_text()
    css = template.split('const INTERACTIVE_CSS: &str = r#"', 1)[1].split('"#;', 1)[0]
    page.add_style_tag(content=css)
    page.evaluate('fmdInstallSourceSearch().open()')
    page.emulate_media(media='print')
    assert not page.locator('#fmd-source-search').is_visible()
    page.emulate_media(media='screen')
    assert page.locator('#fmd-source-search').is_visible()
    check('production print CSS excludes the source-search panel while preserving its screen state')

    assert errors == [], errors
    assert requests == [], requests
    check('all browser checks finish with zero page errors and zero HTTP requests')
    output = {'scope': 'production source-search UI and renderer in a DOM host with input/save adapters; not full native/WASM controller',
              'browser': browser.version, 'passed': len(checks), 'checks': checks,
              'httpRequests': len(requests), 'pageErrors': errors}
    browser.close()
print(json.dumps(output, ensure_ascii=False, indent=2))
