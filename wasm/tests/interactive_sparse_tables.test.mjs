// Exercise the real standalone renderer, with the native table ownership rules.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const header = '| Name | Value |\n| --- | --- |\n';

test('unchanged GFM example 202 keeps the unpiped row inside the table', () => {
  const html = render('| abc | def |\n| --- | --- |\n| bar | baz |\nbar\n\nbar\n');
  assert.match(html, /<tr><td>bar<\/td><td>baz<\/td><\/tr>\n<tr><td>bar<\/td><td><\/td><\/tr>/);
  assert.equal((html.match(/<p>bar<\/p>/g) || []).length, 1);
});

test('short, complete and overflowing rows retain one table and normal cell padding', () => {
  const html = render(header + 'first\nleft | right\nlast\n| a | b | ignored |\n');
  assert.equal((html.match(/<table>/g) || []).length, 1);
  assert.match(html, /<td>first<\/td><td><\/td>/);
  assert.match(html, /<td>left<\/td><td>right<\/td>/);
  assert.match(html, /<td>last<\/td><td><\/td>/);
  assert.match(html, /<td>a<\/td><td>b<\/td>/);
  assert.doesNotMatch(html, /ignored|<p>/);
});

test('single-column rows retain links images code formatting and reference identity', () => {
  const source = '| Status |\n| --- |\nQueued\n[guide](/guide)\n![badge](badge.png)\n`a  b`\n**ready**\n[known]\n\n[known]: /resolved\n';
  const html = render(source);
  for (const cell of ['Queued', '<a href="/guide">guide</a>', '<img src="badge.png" alt="badge">',
    '<code>a  b</code>', '<strong>ready</strong>', '<a href="/resolved">known</a>']) {
    assert.ok(html.includes(`<tr><td>${cell}</td></tr>`), cell);
  }
  assert.doesNotMatch(html, /<p>/);
  assert.equal(render(source), html, 'saving and reopening the same source retains identical ownership');
});

test('a pipe in either header or delimiter disambiguates a one-column table from setext', () => {
  for (const source of ['Status\n| --- |\nready\nqueued', '| Status |\n---\nready\nqueued']) {
    const html = render(source);
    assert.match(html, /<th>Status<\/th>/);
    assert.match(html, /<td>ready<\/td>/);
    assert.match(html, /<td>queued<\/td>/);
    assert.doesNotMatch(html, /<p>/);
  }
  assert.match(render('Status\n---\nready'), /^<h2[^>]*>Status<\/h2>\n<p>ready<\/p>/);
  for (const source of ['Status\n| nope |\nready', 'Status\n| --- | --- |\nready', 'Status\n| :-x |\nready']) {
    assert.doesNotMatch(render(source), /<table>/, source);
  }
});

test('short table rows cannot authorize a dedented lazy quote or list continuation', () => {
  for (const source of [
    '> | Name | Value |\n> | --- | --- |\n> short\noutside\n',
    '- | Name | Value |\n  | --- | --- |\n  short\noutside\n',
    '> Status\n> | --- |\n> short\noutside\n',
    '- Status\n  | --- |\n  short\noutside\n',
  ]) {
    const html = render(source);
    assert.match(html, /<td>short<\/td>/);
    assert.match(html, /<\/(?:blockquote|ul)>\n<p>outside<\/p>\n$/);
  }
});

test('nested quotes lists and footnotes retain the same short table rows', () => {
  for (const source of [
    '> - Status\n>   | --- |\n>   [guide](/guide)\noutside\n',
    '- > Status\n  > | --- |\n  > [guide](/guide)\noutside\n',
    'Use[^note].\n\n[^note]: Status\n    | --- |\n    [guide](/guide)\noutside\n',
  ]) {
    const html = render(source);
    assert.match(html, /<td><a href="\/guide">guide<\/a><\/td>/);
    assert.match(html, /<p>outside<\/p>/);
  }
});

test('supported block openers still terminate a table after its short rows', () => {
  for (const [suffix, expected] of [
    ['# Heading', '<h1'], ['---', '<hr>'], ['- item', '<ul>'], ['2. item', '<ol start="2">'],
    ['> quote', '<blockquote>'], ['```md\n[ref]: /hidden\n```', '<pre><code'],
    ['    [ref]: /hidden', '<pre><code'], ['<custom-tag>\n[ref]: /hidden\n</custom-tag>', '<p>&lt;custom-tag&gt;'],
  ]) {
    const html = render(header + 'short\n' + suffix + '\n\n[ref]\n');
    assert.ok(html.includes('</table></div>\n' + expected), suffix + ': ' + html);
    assert.match(html, /<td>short<\/td><td><\/td>/);
    assert.doesNotMatch(html, /href="\/hidden"/);
  }
});

test('definitions after sparse tables retain authority and separate following source', () => {
  const html = render('Status\n| --- |\nready\n[ref]: /target\nplain after\n\nUse [ref].\n');
  assert.match(html, /<td>ready<\/td>/);
  assert.match(html, /<\/table><\/div>\n<p>plain after<\/p>/);
  assert.match(html, /Use <a href="\/target">ref<\/a>\./);
  const adjacent = render(header + 'short\n[ref]: /target\nStatus\n| --- |\nlast\n\n[ref]\n');
  assert.equal((adjacent.match(/<table>/g) || []).length, 2);
  assert.match(adjacent, /<td>last<\/td>/);
});

test('multiline definitions after short table rows retain label destination and title', () => {
  const source = '> Status\n> | --- |\n> ready\n> [two\n> words]:\n> /target\n> "long\n> title"\n>\n> [two words]\n';
  const html = render(source);
  assert.match(html, /<td>ready<\/td>/);
  assert.match(html, /href="\/target" title="long\ntitle">two words<\/a>/);
});

test('pipe-bearing definition shapes remain cells without creating reference authority', () => {
  const html = render(header + 'short\n[ref]: /hidden|right\n[not](a b)\n[closed] junk\n\n[ref]\n');
  assert.match(html, /<td>\[ref\]: \/hidden<\/td><td>right<\/td>/);
  assert.match(html, /<td>\[not\]\(a b\)<\/td><td><\/td>/);
  assert.match(html, /<td>\[closed\] junk<\/td><td><\/td>/);
  assert.match(html, /<p>\[ref\]<\/p>\n$/);
  assert.doesNotMatch(html, /href=/);
});

test('Unicode BOM and mixed line endings do not change table ownership', () => {
  const source = '\ufeffÉtat\r\n| --- |\r😀 [go](<dir/图.png>)\r\n![徽章](image.png)\r# After\n';
  const html = render(source);
  assert.match(html, /<th>État<\/th>/);
  assert.match(html, /<td>😀 <a href="dir\/图.png">go<\/a><\/td>/);
  assert.match(html, /<td><img src="image.png" alt="徽章"><\/td>/);
  assert.match(html, /<\/table><\/div>\n<h1[^>]*>After<\/h1>/);
});

test('many sparse rows remain bounded and preserve a late reference definition', () => {
  const source = header + 'short\n'.repeat(4096) + '[ref]: /target\n\n[ref]\n';
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000});
  assert.equal((html.match(/<td>short<\/td>/g) || []).length, 4096);
  assert.match(html, /<p><a href="\/target">ref<\/a><\/p>\n$/);
});

test('wide sparse padding falls back losslessly without granting reference authority', () => {
  const columns = 1025;
  const prefix = '|' + 'H|'.repeat(columns) + '\n|' + '---|'.repeat(columns) + '\n'
    + 'short\n'.repeat(512) + '|' + 'full|'.repeat(columns) + '\n';
  const fallback = '[hidden]: /wrong|right\n[also]: /wrong-too\n';
  const source = prefix + fallback + '\n[real]: /target\n\n[hidden] [also] [real]\n';
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 15000});
  assert.equal((html.match(/<td>short<\/td>/g) || []).length, 512);
  assert.equal((html.match(/<td>full<\/td>/g) || []).length, columns);
  assert.equal((html.match(/<td><\/td>/g) || []).length, 512 * 1024);
  assert.ok(html.includes('</table></div>\n<p>' + fallback.trimEnd() + '</p>'));
  assert.match(html, /<p>\[hidden\] \[also\] <a href="\/target">real<\/a><\/p>/);
  assert.doesNotMatch(html, /href="\/wrong/);
});

test('container fallback decisions stay identical at the padding boundary', () => {
  // Exercise the same production paths with a small private budget, keeping
  // the container test cheap; the preceding test pins the real 512K boundary.
  const bounded = vm.createContext({});
  vm.runInContext(code.replace('let left = 512 * 1024;', 'let left = 2;'), bounded);
  const body = header + 'one\ntwo\n[hidden]: /wrong|right\nthree\n';
  for (const source of [body.trimEnd().split('\n').map(line => '> ' + line).join('\n') + '\noutside [hidden]\n',
    '- ' + body.trimEnd().replace(/\n/g, '\n  ') + '\noutside [hidden]\n']) {
    const html = bounded.parseMarkdownClient(source);
    assert.match(html, /<td>one<\/td><td><\/td>/);
    assert.match(html, /<td>two<\/td><td><\/td>/);
    // The full-width reference-shaped row consumes no padding and remains a
    // table cell; the following short row opens a real lazy paragraph.
    assert.match(html, /<td>\[hidden\]: \/wrong<\/td><td>right<\/td>/);
    assert.ok(html.includes('three\noutside [hidden]'));
    assert.doesNotMatch(html, /href="\/wrong"/);
  }
});

test('the first padding fallback row cannot close a nonexistent setext paragraph', () => {
  const bounded = vm.createContext({});
  vm.runInContext(code.replace('let left = 512 * 1024;', 'let left = 2;'), bounded);
  const prefix = 'Name | Value | Third\n--- | --- | ---\nshort\n';
  const html = bounded.parseMarkdownClient(prefix + '===\n[hidden]: /wrong\n\n[hidden]\n');
  assert.match(html, /<\/table><\/div>\n<p>===\n\[hidden\]: \/wrong<\/p>\n<p>\[hidden\]<\/p>/);
  assert.doesNotMatch(html, /href="\/wrong"/);
  const control = bounded.parseMarkdownClient(prefix + 'first\n===\n[visible]: /right\n\n[visible]\n');
  assert.match(control, /<h1[^>]*>first<\/h1>/);
  assert.match(control, /<a href="\/right">visible<\/a>/);
});
