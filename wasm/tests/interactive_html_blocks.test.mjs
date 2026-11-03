// HTML source ownership through the shipped lightweight renderer. Raw markup
// remains inert text; recognizing its block does not authorize HTML execution.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[ch]);
const literal = text => '<p>' + escape(text) + '</p>\n';
const markers = [
  ['<pre>', '</pre>'], ['<SCRIPT>', '</script>'], ['<style type="text/css">', '</style>'],
  ['<textarea', '</textarea>'], ['<!-- comment', '-->'], ['<?processing', '?>'],
  ['<!DOCTYPE html', '>'], ['<!lowercase declaration', '>'], ['<![CDATA[', ']]>'],
];
const endings = source => {
  let index = 0;
  return [source, source.replace(/\n/g, '\r\n'), source.replace(/\n/g, '\r'),
    source.replace(/\n/g, () => ['\r\n', '\n', '\r'][index++ % 3])];
};

test('complete tag blocks keep literal content and cannot define links or footnotes', () => {
  const body = '# Literal heading\n[secret]: /hidden\n[^ghost]: Private note\n'
    + '**stars** `code` [link](/hidden) ![image](hidden.png) [^visible]';
  for (const opening of ['<custom>', '</custom>', '<span title="quoted > marker">']) {
    const block = opening + '\n' + body + '\n</custom>\n# Still literal';
    const source = block + '\n\n# Outside\n\n[secret] [^ghost]\n\n[^visible]: Visible note';
    assert.equal(render(source), literal(block) + '<h1 id="outside">Outside</h1>\n<p>[secret] [^ghost]</p>\n', opening);
  }
});

test('types one through five own blank lines and the entire closing-marker line', () => {
  for (const [opening, closing] of markers) {
    const block = opening + '\n\n# Literal\n[secret]: /hidden\n[^ghost]: Private\n\n' + closing + ' trailing **text**';
    assert.equal(render('Before\n' + block + '\n# After\n\n[secret] [^ghost]'),
      '<p>Before</p>\n' + literal(block) + '<h1 id="after">After</h1>\n<p>[secret] [^ghost]</p>\n', opening);
  }
});

test('same-line closing markers release the next line for real definitions', () => {
  for (const [opening, closing] of markers) {
    const block = opening + ' payload ' + closing + ' trailing';
    assert.equal(render(block + '\n[r]: /right\n\n[r]'),
      literal(block) + '<p><a href="/right">r</a></p>\n', opening);
  }
});

test('any raw-text closing name ends type one, with ASCII case-insensitive matching', () => {
  for (const opening of ['<pre>', '<script>', '<style>', '<textarea>']) {
    for (const closing of ['</PRE>', '</Script>', '</sTyLe>', '</textarea>']) {
      const block = opening + '\n# Literal\n' + closing;
      assert.equal(render(block + '\n# Outside'), literal(block) + '<h1 id="outside">Outside</h1>\n');
    }
  }
  const block = '<pre>\n</pre >\n\n[secret]: /hidden\n</pre>';
  assert.equal(render(block + '\n\n[secret]'), literal(block) + '<p>[secret]</p>\n');
});

test('all type-six block names interrupt paragraphs, including incomplete tags', () => {
  const names = 'address article aside base basefont blockquote body caption center col colgroup dd details dialog dir div dl dt fieldset figcaption figure footer form frame frameset h1 h2 h3 h4 h5 h6 head header hr html iframe legend li link main menu menuitem nav noframes ol optgroup option p param search section summary table tbody td tfoot th thead title tr track ul'.split(' ');
  for (const name of names) {
    for (const opening of [`<${name}>`, `</${name.toUpperCase()}>`, `<${name}`, `<${name} a="incomplete`, `<${name}/>`]) {
      const block = opening + '\n# Literal\n[r]: /hidden';
      assert.equal(render('Before\n' + block + '\n\n[r]'), '<p>Before</p>\n' + literal(block) + '<p>[r]</p>\n', opening);
    }
  }
});

test('complete type-seven tags cannot interrupt an existing paragraph', () => {
  for (const tag of ['<custom>', '<span class=x>', '</custom>', '</pre>', '<widget data-x="a|b"/>']) {
    assert.equal(render('Before\n' + tag + '\n# Real\n[r]: /right\n\n[r]'),
      '<p>Before\n' + escape(tag) + '</p>\n<h1 id="real">Real</h1>\n<p><a href="/right">r</a></p>\n', tag);
    assert.equal(render('Before\n' + tag + '\n[r]: /literal\ncontinued\n\n[r]'),
      '<p>Before\n' + escape(tag) + '\n[r]: /literal\ncontinued</p>\n<p>[r]</p>\n', tag);
  }
});

test('types six and seven end only on a blank line, not on a closing tag', () => {
  for (const opening of ['<div>', '<custom>', '</pre>']) {
    const block = opening + '\n# Literal\n</custom>\n[r]: /hidden\n# Still literal';
    assert.equal(render(block + '\n \t \n# After\n\n[r]'),
      literal(block) + '<h1 id="after">After</h1>\n<p>[r]</p>\n');
  }
});

test('type-seven attribute grammar accepts complete ASCII names and literal quoted values', () => {
  for (const opening of [
    '<WIDGET-2/>', '<widget disabled _x :ns.x-y=ok>',
    '<widget data-x = \'café > < β\' title="quoted"/>', '<widget a=b/c d=é/>',
    '<widget a="">\t ', '<widget\ta\t=\tvalue\tb/>', '</custom-tag\t >  ',
  ]) {
    const block = opening + '\n# Literal\n[r]: /hidden';
    assert.equal(render(block + '\n\n[r]'), literal(block) + '<p>[r]</p>\n', opening);
  }
});

test('malformed tags, raw-name lookalikes and non-ASCII separators retain Markdown behavior', () => {
  for (const opening of [
    '<>', '<2tag>', '<tag.name>', '<tag❤>', '<tag a="unterminated>',
    '<tag a="x"b="y">', '<tag a= >', '<tag a=`x`>', '<tag a=x=y>',
    '<tag a=<x>>', '<tag a="x" / >', '<tag\u00a0a=x>', '<tag\fa=x>',
    '<tag a> text', '<tag><other>', '<tag>\u00a0', '</tag a=x>', '</tag/>', '<!>',
    '<pre/>', '<script/>', '<style/>', '<textarea/>', '<pre\u00a0class=x>',
    '<div.foo>', '<div/invalid>', '<search=bad>', '\ufeff<custom>',
  ]) {
    const html = render('Lead\n\n' + opening + '\n# Real\n[r]: /right\n\n[r]');
    assert.match(html, /<h1 id="real">Real<\/h1>/, opening);
    assert.match(html, /<a href="\/right">r<\/a>/, opening);
  }
});

test('unknown partial and multiline tags do not claim a later Markdown block', () => {
  for (const opening of ['<widget', '<widget a="first\nsecond">', '</widget\n>']) {
    const html = render(opening + '\n# Real\n[r]: /right\n\n[r]');
    assert.match(html, /<h1 id="real">Real<\/h1>/);
    assert.match(html, /href="\/right"/);
  }
});

test('literal HTML never authorizes raw tags, image expansion, links or heading identities', () => {
  let reads = 0;
  const assets = {has() { reads++; throw Error('literal source requested image bytes'); }};
  const block = '<custom onclick="bad()">\n<script>bad()</script>\n![private](secret.png)\n'
    + '[click](javascript:bad())\n# Visible\n[^ghost]';
  assert.equal(render(block + '\n\n# Visible', assets), literal(block) + '<h1 id="visible">Visible</h1>\n');
  assert.equal(reads, 0);
});

test('quote and list HTML blocks reject dedented lazy continuation and hidden definitions', () => {
  const contexts = [
    {first: '> ', rest: '> ', before: '<blockquote>\n', after: '</blockquote>\n'},
    {first: '- ', rest: '  ', before: '<ul>\n<li>', after: '</li>\n</ul>\n'},
    {first: '> - ', rest: '>   ', before: '<blockquote>\n<ul>\n<li>', after: '</li>\n</ul>\n</blockquote>\n'},
    {first: '- > ', rest: '  > ', before: '<ul>\n<li><blockquote>\n', after: '</blockquote></li>\n</ul>\n'},
  ];
  for (const wrapper of contexts) {
    for (const [opening, closing] of [['<custom>', '</custom>'], ['<pre>', '</pre>'], ['<!--', '-->'], ['<div>', '</div>']]) {
      const lines = [opening, '# Literal', '[r]: /hidden', closing];
      const source = wrapper.first + lines[0] + '\n' + lines.slice(1).map(line => wrapper.rest + line).join('\n')
        + '\n[r]: /right\n\n[r]';
      const html = render(source);
      assert.match(html, /<a href="\/right">r<\/a>/, source);
      assert.doesNotMatch(html, /href="\/hidden"|<h1/, source);
      assert.ok(html.startsWith(wrapper.before), source);
      assert.ok(html.includes(escape(lines.join('\n'))), source);
      assert.ok(html.indexOf(wrapper.after.trimEnd()) < html.indexOf('href="/right"'), source);
    }
  }
});

test('non-interrupting tags keep paragraph laziness in nested containers', () => {
  for (const [first, continued, end] of [
    ['> ', '> ', '</blockquote>'], ['- ', '  ', '</ul>'],
    ['> - ', '>   ', '</blockquote>'], ['- > ', '  > ', '</ul>'],
  ]) {
    const source = first + 'Before\n' + continued + '<custom>\ncontinued\n\nAfter';
    const html = render(source);
    assert.ok(html.includes('Before\n&lt;custom&gt;\ncontinued'), source);
    assert.ok(html.indexOf('continued') < html.indexOf(end), source);
    assert.ok(html.endsWith('<p>After</p>\n'), source);
  }
});

test('literal blanks and ordered-list-looking content survive inside list HTML', () => {
  for (const opening of ['<pre>', '<script>', '<!--']) {
    const closing = opening === '<!--' ? '-->' : '</pre>';
    const source = '- ' + opening + '\n  2. literal\n   \t\n  3. still literal\n  ' + closing + '\noutside';
    const html = render(source);
    assert.doesNotMatch(html, /<ol|<h[1-6]/);
    assert.match(html, /2\. literal\n  \n3\. still literal/);
    assert.ok(html.indexOf('</ul>') < html.indexOf('<p>outside</p>'), source);
  }
});

test('blank HTML termination accepts only space and tab, retaining Unicode content', () => {
  for (const blank of ['\u00a0', '\u2003', '\ufeff', '\v', '\f']) {
    const block = '<custom>\n' + blank + '\n[r]: /hidden';
    assert.equal(render(block + '\n\n[r]'), literal(block) + '<p>[r]</p>\n');
    const html = render('- <custom>\n  ' + blank + '\n  [r]: /hidden\n\n[r]');
    assert.ok(html.includes(escape(block)), JSON.stringify(blank));
    assert.doesNotMatch(html, /href="\/hidden"/);
  }
});

test('HTML leaves in footnotes preserve content without registering nested definitions', () => {
  const block = '<custom>\n# Literal\n[hidden]: /wrong\n[^ghost]: Private\n</custom>';
  const source = 'Call[^n].\n\n[^n]: ' + block.split('\n').join('\n    ') + '\n\n[hidden] [^ghost]';
  const html = render(source);
  assert.match(html, /<p>\[hidden\] \[\^ghost\]<\/p>/);
  assert.ok(html.includes(literal(block)));
  assert.doesNotMatch(html, /href="\/wrong"|<h1|id="fn-2"/);
});

test('HTML starts cannot become pipe-table cells or consume subsequent real tables', () => {
  const before = '| A | B |\n|---|---|\n';
  for (const opening of ['<custom data-v="a|b">', '<div data-v="a|b">', '<!-- a|b']) {
    const closing = opening.startsWith('<!--') ? '\n-->' : '';
    const block = opening + '\n[r]: /hidden' + closing;
    const html = render(before + block + '\n\n[r]\n\n| C | D |\n|---|---|\n| one | two |');
    assert.equal((html.match(/<table>/g) ?? []).length, 2);
    assert.equal((html.match(/<td>/g) ?? []).length, 2);
    assert.ok(html.includes(literal(block)), opening);
    assert.match(html, /<p>\[r\]<\/p>/);
  }
});

test('code and reference-title grammars retain their own HTML-looking content', () => {
  const text = '<custom>\n[r]: /hidden\n</custom>';
  for (const fenced of ['```\n' + text + '\n```', text.split('\n').map(line => '    ' + line).join('\n')]) {
    assert.equal(render(fenced + '\n\n[r]'), '<pre><code>' + escape(text) + '\n</code></pre>\n<p>[r]</p>\n');
  }
  const title = 'title\n<custom>\n# literal\n</custom>\nend';
  assert.equal(render('[r]: /right "' + title + '"\n\n[r]'),
    '<p><a href="/right" title="' + escape(title) + '">r</a></p>\n');
});

test('document-prefix and all logical newline forms preserve HTML ownership', () => {
  const body = '<custom>\n# Literal\n[secret]: /hidden\n</custom>\n\n[secret]';
  const expected = literal(body.slice(0, body.lastIndexOf('\n\n'))) + '<p>[secret]</p>\n';
  for (const input of endings(body)) {
    assert.equal(render(input), expected);
    assert.equal(render('\ufeff' + input), expected);
  }
  for (const input of endings('\ufeff---\ntitle: A document\n---\n' + body)) assert.equal(render(input), expected);
  // An interior BOM cannot turn the next line into a block opener.
  assert.match(render('Before\n\n\ufeff<custom>\n# Real'), /<h1 id="real">Real<\/h1>/);
});

test('long tags and literal bodies have bounded scanning and retain all source text', () => {
  const opening = '<widget' + ' flag'.repeat(15000) + ' title="' + 'é < >'.repeat(10000) + '">';
  const block = opening + '\n' + '[secret]: /hidden\n[^ghost]: Private\n'.repeat(3000) + '</widget>';
  const bounded = vm.createContext({source: block + '\n\n[secret] [^ghost]'});
  vm.runInContext(code, bounded);
  assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 5000}),
    literal(block) + '<p>[secret] [^ghost]</p>\n');
  const small = vm.createContext({source: '<!--\n' + 'literal\n'.repeat(3000) + '-->'});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 50000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("[secret] [^ghost]")', small), '<p>[secret] [^ghost]</p>\n');
});
