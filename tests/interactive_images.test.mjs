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

const imageFixtures = [
  ['image/png', Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAE0lEQVR4nGP4z8DwHwyBNIhgAAA/0gX7yxemzgAAAABJRU5ErkJggg==', 'base64')],
  ['image/jpeg', Buffer.from(readFileSync(new URL('fixtures/jpeg-quadrants.hex', import.meta.url), 'utf8').replace(/\s/g, ''), 'hex')],
  ['image/gif', Buffer.from(readFileSync(new URL('fixtures/gif/animated-colors.base64', import.meta.url), 'utf8').trim(), 'base64')],
  ['image/svg+xml', Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><text>é中😀</text></svg>')],
];
const percentBytes = bytes => Array.from(bytes, value => '%' + value.toString(16).padStart(2, '0')).join('');

test('source image encodings normalize for display without changing original bytes', () => {
  for (const [mime, bytes] of imageFixtures) {
    const canonical = 'data:' + mime + ';base64,' + bytes.toString('base64');
    for (const destination of [
      'data:' + mime + ',' + percentBytes(bytes),
      'DATA:' + mime.toUpperCase() + ';CHARSET=UTF-8;BASE64,' + bytes.toString('base64'),
      'data:' + mime + ';base64,' + encodeURIComponent(bytes.toString('base64')),
    ]) {
      const source = 'Changed paragraph.\n\n![Retained figure](' + destination + ')';
      const html = render(source);
      assert.ok(html.includes('src="' + canonical + '"'), mime);
      assert.equal(html.match(/<img /g)?.length, 1);
      assert.ok(source.includes(destination), 'normalization must only affect output');
    }
  }
});

test('percent data and literal Unicode share byte-exact UTF-8 image encoding', () => {
  const original = '<svg xmlns="http://www.w3.org/2000/svg"><text>é中😀 &amp; detail</text></svg>';
  const destination = 'data:image/svg+xml,' + encodeURIComponent(original)
    .replace('%C3%A9%E4%B8%AD%F0%9F%98%80', 'é中😀');
  const expected = 'data:image/svg+xml;base64,' + Buffer.from(original).toString('base64');
  assert.ok(render('![Unicode](' + destination + ')').includes('src="' + expected + '"'));
});

test('normalized images remain usable in references, lists, tables and footnotes', () => {
  const bytes = imageFixtures[2][1], destination = 'data:image/gif,' + percentBytes(bytes);
  const source = '# Figures\n\n- ![List][image]\n\n| Figure |\n|---|\n| ![Cell](' + destination
    + ') |\n\nNote[^n].\n\n[^n]: ![Note][image]\n\n[image]: ' + destination + ' "Animation"';
  const html = render(source), expected = 'data:image/gif;base64,' + bytes.toString('base64');
  assert.equal(html.split('src="' + expected + '"').length - 1, 3);
  assert.equal(html.match(/title="Animation"/g).length, 2);
});

test('explicit data-image bindings own replacement and rejection before source decoding', () => {
  const destination = 'data:image/png;charset=bad;base64,not-an-image';
  assert.ok(render('![Replacement](' + destination + ')', [[destination, svg]]).includes(svg));
  for (const [mime, bytes] of imageFixtures) {
    const target = 'data:' + mime + ',' + percentBytes(bytes);
    for (const rejected of [null, '', 'data:image/png;base64,AB==']) {
      assert.equal(render('![Kept alt](' + target + ')', [[target, rejected]]), '<p>Kept alt</p>\n');
    }
  }
});

test('image normalization cannot turn data links or non-image mappings into active content', () => {
  for (const [mime, bytes] of imageFixtures) {
    const target = 'data:' + mime + ',' + percentBytes(bytes);
    assert.doesNotMatch(render('[Link](' + target + ') <' + target + '>'), /href=/);
  }
  for (const target of ['data:text/html;base64,PHN2Zy8+', 'data:;base64,PHN2Zy8+',
    'data:application/xhtml+xml;base64,PHN2Zy8+', 'data:image/tiff;base64,AAAA']) {
    assert.equal(render('![Kept alt](' + target + ')', [[target, svg]]), '<p>Kept alt</p>\n');
  }
  const markup = '<svg xmlns="http://www.w3.org/2000/svg" onload="bad()"><script>bad()</script></svg>';
  const html = render('![Image](data:image/svg+xml,' + encodeURIComponent(markup) + ')');
  assert.ok(html.includes('src="data:image/svg+xml;base64,' + Buffer.from(markup).toString('base64') + '"'));
  assert.doesNotMatch(html, /<svg|<script|onload=|onerror=/);
});

test('bounded headers and strict percent/base64 syntax reject malformed source images', () => {
  for (const destination of [
    'data:image/png,%', 'data:image/png,%0', 'data:image/png,%0g',
    'data:image/png;base64,%3DAAA', 'data:image/png;base64,AB%3D%3D',
    'data:image/png;base64,AAB%3D', 'data:image/png;base64,AA==AAAA',
    'data:image/png;base64,AA%20AA', 'data:image/png;charset=latin1,ABC',
    'data:image/png;base64;charset=utf-8,AAAA', 'data:image/png;charset=utf-8;charset=utf-8,ABC',
    'data:image/png;' + 'x'.repeat(256) + ',ABC', 'data:image/png,',
  ]) assert.equal(render('![Kept alt](' + destination + ')'), '<p>Kept alt</p>\n', destination);
});

test('normalizer enforces output and work limits before publishing encoded data', () => {
  const source = 'data:image/png,' + '%41'.repeat(128), expected = 'data:image/png;base64,' + Buffer.alloc(128, 65).toString('base64');
  let charged = 0;
  const charge = count => { charged += count; }, fail = kind => { throw Object.assign(Error(kind), {code: 'FMD_RENDER_LIMIT', limit: kind}); };
  assert.equal(context.fmdNormalizeSourceImage(source, charge, fail, expected.length), expected);
  assert.ok(charged >= 128 * 3 + 128);
  assert.throws(() => context.fmdNormalizeSourceImage(source, charge, fail, expected.length - 1),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'output');
  let remaining = 32;
  assert.throws(() => context.fmdNormalizeSourceImage(source, count => {
    if (count > remaining) fail('work'); remaining -= count;
  }, fail, expected.length), error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
});
