"""Independent tokenization and app-runtime checks for interactive HTML output.

Invoked by interactive_html_test.rs with byte-framed source and HTML on stdin.
Uses only Python/Node standard libraries; never executes source as JavaScript.
"""

from html.parser import HTMLParser
import json
import shutil
import subprocess
import sys


class Document(HTMLParser):
    def __init__(self, html):
        super().__init__(convert_charrefs=True)
        self.elements = []
        self.scripts = []
        self.script = None
        self.feed(html)
        self.close()

    def handle_starttag(self, tag, attrs):
        self.elements.append((tag, dict(attrs)))
        if tag == "script":
            assert self.script is None
            self.script = {"attrs": dict(attrs), "text": ""}
            self.scripts.append(self.script)

    def handle_endtag(self, tag):
        if tag == "script":
            self.script = None

    def handle_data(self, data):
        if self.script is not None:
            self.script["text"] += data

    def assert_safe_attributes(self):
        for tag, attrs in self.elements:
            assert not any(name.startswith("on") for name in attrs), (tag, attrs)
            assert attrs.get("id") != "injected", (tag, attrs)


source_len = int(sys.stdin.buffer.readline())
source = sys.stdin.buffer.read(source_len).decode("utf-8")
html = sys.stdin.buffer.read().decode("utf-8")
doc = Document(html)
doc.assert_safe_attributes()
assert len(doc.scripts) == 2, "Markdown introduced or swallowed a script element"
source_script, app_script = doc.scripts
assert source_script["attrs"] == {
    "type": "application/json",
    "id": "fmd-raw-source",
}
assert not app_script["attrs"], "The only executable script must be the bundled app"
assert "<" not in source_script["text"]
assert json.loads(source_script["text"]) == source, "Source did not round trip"
assert next(attrs for tag, attrs in doc.elements if tag == "html") == {
    "lang": 'en" onmouseover="alert(1)&<>'
}

node = shutil.which("node")
if node is None:
    print("PASS HTML tokenization + source round trip; SKIP JS runtime: Node unavailable")
    sys.exit(0)

# The fake DOM lets us run the emitted app code without a browser install. The
# resulting live-preview markup goes back through the independent HTML parser.
runtime = r"""
const assert = require('node:assert/strict');
const vm = require('node:vm');
const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const elements = new Map();
function element() {
  const classes = new Set();
  return {
    value: 'HTML parser normalized initial value', textContent: '',
    innerHTML: '<p>Initial Rust preview</p>', handlers: {},
    addEventListener(name, handler) { this.handlers[name] = handler; },
    classList: {
      add(name) { classes.add(name); },
      remove(name) { classes.delete(name); },
      contains(name) { return classes.has(name); },
      toggle(name) { classes.has(name) ? classes.delete(name) : classes.add(name); }
    }
  };
}
for (const id of input.ids) elements.set(id, element());
elements.get('fmd-raw-source').textContent = input.data;
elements.get('stats-drawer').querySelector = selector => elements.get(selector.slice(1));
const styles = new Map();
const timers = new Map();
let nextTimer = 0;
let prints = 0;
const documentEvents = {}, windowEvents = {};
const document = {
  addEventListener(name, handler) { documentEvents[name] = handler; },
  querySelector(selector) {
    const ids = {
      'body > script#fmd-raw-source[type="application/json"]': 'fmd-raw-source',
      'body > #stats-drawer': 'stats-drawer'
    };
    assert.ok(ids[selector], `Unexpected selector: ${selector}`);
    return elements.get(ids[selector]);
  },
  getElementById(id) {
    assert.ok(elements.has(id), `Missing DOM element: ${id}`);
    return elements.get(id);
  },
  body: element(),
  documentElement: {style: {setProperty(key, value) { styles.set(key, value); }, getPropertyValue(key) { return styles.get(key) || ''; }}}
};
const context = vm.createContext({
  document, window: {print() { prints++; }, addEventListener(name, handler) { windowEvents[name] = handler; }},
  setTimeout(fn, delay) { assert.equal(delay, 150); const id = ++nextTimer; timers.set(id, fn); return id; },
  clearTimeout(id) { timers.delete(id); }
});
vm.runInContext(input.app, context, {timeout: 2000});
assert.equal(context.fmdInjected, undefined, 'Markdown executed as JavaScript');
assert.equal(elements.get('fmd-editor').value, input.source);
assert.equal(elements.get('source-line-count').textContent, `Lines: ${input.source.split('\n').length}`);
assert.equal(elements.get('fmd-content').innerHTML, '<p>Initial Rust preview</p>', 'Startup replaced the full Rust render');
function click(id) { elements.get(id).handlers.click(); }
function edit(source) {
  elements.get('fmd-editor').value = source;
  elements.get('fmd-editor').handlers.input();
  assert.equal(timers.size, 1, 'Editor debounce did not schedule one render');
}
edit('Superseded input');
edit('# Edited\n\n**strong** and *emphasis* with [link](https://example.com).\n- item\n```rust\nlet x = "<safe>";\n```');
for (const fn of timers.values()) fn();
timers.clear();
const normal = elements.get('fmd-content').innerHTML;
assert.ok(normal.includes('<h1 id="edited">Edited</h1>'));
assert.ok(normal.includes('<strong>strong</strong>'));
assert.ok(normal.includes('<em>emphasis</em>'));
assert.ok(normal.includes('<a href="https://example.com">link</a>'));
assert.ok(normal.includes('<li>item</li>'));
assert.ok(normal.includes('class="language-rust"'));
assert.ok(normal.includes('&lt;safe&gt;'));

const hostileLanguage = 'rust" onmouseover="globalThis.fmdInjected=true" data-x="<img id=\'injected\' src=x onerror=\'alert(1)\'>&';
edit('```' + hostileLanguage + '\n<script>globalThis.fmdInjected=true</script>\n```');
for (const fn of timers.values()) fn();
timers.clear();
const hostile = elements.get('fmd-content').innerHTML;
assert.equal(context.fmdInjected, undefined);
click('btn-toggle-view');
assert.ok(elements.get('fmd-app-body').classList.contains('view-read'));
click('btn-toggle-view');
assert.ok(elements.get('fmd-app-body').classList.contains('view-split'));
click('btn-zoom-in');
assert.equal(elements.get('btn-zoom-reset').textContent, '110%');
click('btn-zoom-reset');
assert.equal(styles.get('--fmd-base'), '16px');
click('btn-stats-toggle');
assert.ok(elements.get('stats-drawer').classList.contains('open'));
click('btn-stats-close');
assert.ok(!elements.get('stats-drawer').classList.contains('open'));
click('btn-theme-toggle');
assert.ok(document.body.classList.contains('theme-dark'));
click('btn-export-pdf');
assert.equal(prints, 1);
process.stdout.write(JSON.stringify({normal, hostile, hostileLanguage}));
"""
ids = [attrs["id"] for _, attrs in doc.elements if "id" in attrs]
assert len(set(ids)) == len(ids), "Markdown introduced duplicate application ids"
result = subprocess.run(
    [node, "-e", runtime],
    input=json.dumps({
        "ids": ids,
        "data": source_script["text"],
        "app": app_script["text"],
        "source": source,
    }),
    encoding="utf-8",
    capture_output=True,
    timeout=10,
    check=True,
)
rendered = json.loads(result.stdout)
for key in ["normal", "hostile"]:
    parsed = Document(rendered[key])
    parsed.assert_safe_attributes()
    assert not parsed.scripts, "Live preview emitted an executable script"
code_attrs = [attrs for tag, attrs in Document(rendered["hostile"]).elements if tag == "code"]
assert code_attrs == [{"class": "language-" + rendered["hostileLanguage"].split()[0]}]
print("PASS HTML tokenization + lossless source + JS editor/toolbar + safe live code attributes")
