// Actual fallback execution, including asset identity and URL admission.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
const context = vm.createContext({Map});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = (source, entries = []) => context.parseMarkdownClient(source, new Map(entries));
const png = 'data:image/png;base64,iVBORw0KGgo=';
const svg = 'data:image/svg+xml;base64,' + Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"/>').toString('base64');

test('supplied image bytes replace the exact Markdown destination', () => {
  assert.match(render('![plot](plot.png)', [['plot.png', png]]), new RegExp('src="' + png + '"'));
  assert.doesNotMatch(render('![plot](plot.png)', [['plot.png', png]]), /src="plot.png"/);
});
test('embedded image syntax survives edits even without a manifest', () => {
  for (const uri of [png, svg]) assert.ok(render('![plot](' + uri + ')').includes('src="' + uri + '"'));
});
test('bindings also work in forward references, lists, tables and footnotes', () => {
  const source = '- ![one][pic]\n\n| H |\n|---|\n| ![two](plot.png) |\n\nNote[^n]\n\n[^n]: ![three][pic]\n\n[pic]: plot.png "A title"';
  const html = render(source, [['plot.png', png]]);
  assert.equal(html.split('src="' + png + '"').length - 1, 3);
  assert.match(html, /title="A title"/);
});
test('keys are decoded as Markdown once before exact matching', () => {
  for (const [syntax, key] of [['a&amp;b.png', 'a&b.png'], ['x\\(1\\).png', 'x(1).png'], ['<my file.png>', 'my file.png']]) {
    assert.ok(render('![x](' + syntax + ')', [[key, png]]).includes(png));
  }
});
test('binding a remote image does not fetch that URL or rewrite surrounding links', () => {
  const url = 'https://example.invalid/photo.png';
  const html = render('![image](' + url + ') [link](' + url + ')', [[url, png]]);
  assert.ok(html.includes('src="' + png + '"'));
  assert.ok(html.includes('href="' + url + '"'));
});
test('opaque Map keys do not access object prototypes', () => {
  for (const key of ['__proto__', 'constructor', 'toString']) {
    assert.ok(render('![x](' + key + ')', [[key, svg]]).includes(svg));
    assert.ok(render('![x](' + key + ')').includes('src="' + key + '"'));
  }
});
test('unsafe or damaged binding values stay inert and never fall back to a remote URL', () => {
  for (const bad of [null, 42, {}, '', 'https://example.invalid/other', 'javascript:alert(1)',
    'data:text/html;base64,PHNjcmlwdD4=', 'data:image/svg+xml,<svg onload="alert(1)"/>',
    'data:image/png;base64,AAAA" onerror="bad', 'data:image/png;base64,A===']) {
    assert.equal(render('![kept](https://example.invalid/x)', [['https://example.invalid/x', bad]]), '<p>kept</p>\n');
  }
});
test('the data-image allowlist never expands link or autolink schemes', () => {
  for (const uri of [png, svg]) {
    assert.doesNotMatch(render('[x](' + uri + ') <' + uri + '>'), /href=/);
  }
});
test('invalid embedded types and malformed base64 are refused', () => {
  for (const uri of ['data:text/html;base64,AAAA', 'data:image/png;base64,', 'data:image/png;base64,A',
    'data:image/png;base64,AAAA=AAA', 'data:image/png;base64,AA AA', 'data:image/svg+xml;base64,%%==',
    'data:image/png;base64,AAAA\n', 'data:image/png;foo;base64,AAAA']) assert.equal(context.fmdEmbeddedImageUrl(uri), false, uri);
});
test('image resources are document-local and source plus assets remain unchanged', () => {
  const source = '![x](plot.png)', entries = [['plot.png', png]];
  const before = JSON.stringify(entries);
  assert.ok(render(source, entries).includes(png));
  assert.doesNotMatch(render(source), /data:image/);
  assert.equal(JSON.stringify(entries), before);
  assert.equal(source, '![x](plot.png)');
});
test('alt/title escaping and image link nesting survive resource resolution', () => {
  const html = render('[![**A & B**](plot.png "quotes &amp; stuff")](https://example.org)', [['plot.png', png]]);
  assert.match(html, /alt="A &amp; B"/);
  assert.match(html, /title="quotes &amp; stuff"/);
  assert.match(html, /<a href="https:\/\/example.org"><img /);
  assert.ok(html.includes(png));
});
test('old callers and unmatched image destinations retain their existing behavior', () => {
  assert.equal(context.parseMarkdownClient('![x](relative.png)'), '<p><img src="relative.png" alt="x"></p>\n');
  assert.equal(render('![x](javascript:alert(1))'), '<p>x</p>\n');
});
