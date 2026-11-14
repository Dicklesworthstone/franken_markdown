// Source ownership through the actual standalone renderer, including literal
// inline bodies and paragraphs collected through nested quote/list containers.
// Run with: node --test wasm/tests/interactive_prose_ownership.test.mjs
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const escape = text => text.replace(/[&<>"]/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[ch]));

test('paragraph indentation is prose whitespace while only source breaks are hard breaks', () => {
  assert.equal(render('   alpha\n \tbeta  \n    gamma\\\n\t delta \t'),
    '<p>alpha\nbeta<br>\ngamma<br>\ndelta</p>\n');
  assert.equal(render('  a  b\tc  '), '<p>a  b\tc</p>\n');
  assert.equal(render('alpha&#32;&#32;'), '<p>alpha  </p>\n');
  assert.equal(render('alpha&#32;&#32;\n  beta&#92;\n\tgamma'),
    '<p>alpha  \nbeta\\\ngamma</p>\n');
  assert.equal(render(' \u00a0alpha\u00a0 \n \u00a0beta\u00a0 \t'),
    '<p>\u00a0alpha\u00a0\n\u00a0beta\u00a0</p>\n');
});

test('continuation whitespace remains literal inside code, math, markup and link titles', () => {
  assert.equal(render('  `first\n    second`  '), '<p><code>first     second</code></p>\n');
  assert.equal(render('text $first\n    second$ tail'),
    '<p>text $first\n    second$ tail</p>\n');
  assert.equal(render('text <span title="first\n    second"> tail'),
    '<p>text &lt;span title=&quot;first\n    second&quot;&gt; tail</p>\n');
  assert.equal(render('[label](/guide "first\n    second")'),
    '<p><a href="/guide" title="first\n    second">label</a></p>\n');
});

test('lazy marker lines retain paragraph ownership at every admitted tab width', () => {
  for (const indent of ['    ', '        ', '\t', ' \t', '  \t', '   \t']) {
    for (const marker of ['```', '~~~', '# heading', '> nested', '***', '---', '=', '2. item', '- item', '<div>']) {
      const source = '> alpha\n' + indent + marker + '\n> beta';
      assert.equal(render(source),
        '<blockquote>\n<p>alpha\n' + escape(marker) + '\nbeta</p>\n</blockquote>\n', source);
    }
  }
});

test('lazy backticks stay inline across nested containers and preserve following real links', () => {
  for (const [opening, continuation, before, after] of [
    ['alpha', '', '<p>', '</p>'],
    ['- alpha', '  ', '<ul>\n<li>', '</li>\n</ul>'],
    ['1. alpha', '   ', '<ol>\n<li>', '</li>\n</ol>'],
    ['- 1. alpha', '     ', '<ul>\n<li><ol>\n<li>', '</li>\n</ol></li>\n</ul>'],
    ['> - alpha', '>   ', '<blockquote>\n<ul>\n<li>', '</li>\n</ul>\n</blockquote>'],
    ['- > alpha', '  > ', '<ul>\n<li><blockquote>\n<p>', '</p>\n</blockquote></li>\n</ul>'],
  ]) {
    const source = '> ' + opening + '\n    ```\n> ' + continuation + 'β [literal](/hidden)\n    ```\n> '
      + continuation + '[visible](/real)';
    assert.equal(render(source), '<blockquote>\n' + before
      + 'alpha\n<code>β [literal](/hidden)</code>\n<a href="/real">visible</a>' + after + '\n</blockquote>\n', source);
  }
});

test('setext-looking lazy lines do not inject escapes into literal inline code', () => {
  for (const [opening, ending] of [['> `alpha', '> beta`'], ['> - `alpha', '>   beta`'], ['1234. `alpha', '      beta`']]) {
    for (const indent of ['', ' ', '  ', '   ']) {
      for (const marker of ['=', '==']) {
        const source = opening + '\n' + indent + marker + '\n' + ending;
        const html = render(source);
        assert.match(html, new RegExp('<code>alpha ' + marker + ' beta</code>'), source);
        assert.doesNotMatch(html, /<h[1-6]|\\/);
      }
    }
  }
});

test('lazy list markers inside code keep original punctuation with no invented escapes', () => {
  for (const opening of ['> alpha', '> - alpha', '1234. alpha']) {
    const source = opening + '\n    ```\n    - item\n    2. item\n    =\n    ```';
    const html = render(source);
    assert.match(html, /alpha\n<code>- item 2\. item =<\/code>/, source);
    assert.doesNotMatch(html, /<pre>|\\/, source);
  }
});

test('lazy block-looking source cannot manufacture a reference definition boundary', () => {
  for (const marker of ['```', '# heading', '> nested', '<div>']) {
    const source = '> alpha\n    ' + marker + '\n[ref]: /hidden\n\n[ref]';
    assert.equal(render(source), '<blockquote>\n<p>alpha\n' + escape(marker)
      + '\n[ref]: /hidden</p>\n</blockquote>\n<p>[ref]</p>\n', source);
  }
});

test('padding identity belongs to the line occurrence and does not erase matching authored code', () => {
  const source = '> alpha\n    same\n\n`literal\n    same`';
  assert.equal(render(source),
    '<blockquote>\n<p>alpha\nsame</p>\n</blockquote>\n<p><code>literal     same</code></p>\n');
  assert.equal(render('> [!NOTE]\n> `alpha\n=\n> beta`'),
    '<aside class="callout callout-note"><p class="callout-title">Note</p>\n<p><code>alpha = beta</code></p>\n</aside>\n');
});

test('lazy ownership survives Unicode and every source line ending', () => {
  const lines = ['> - 12345. café', '\t# [visible](/real)', '>          tail', '', 'outside'];
  for (const newline of ['\n', '\r\n', '\r']) {
    assert.equal(render(lines.join(newline)),
      '<blockquote>\n<ul>\n<li><ol start="12345">\n<li>café\n# <a href="/real">visible</a>\ntail</li>\n</ol></li>\n</ul>\n</blockquote>\n<p>outside</p>\n');
  }
});

test('real explicit and outside block boundaries are preserved', () => {
  assert.equal(render('> alpha\n> ```\n> literal\n> ```'),
    '<blockquote>\n<p>alpha</p>\n<pre><code>literal\n</code></pre>\n</blockquote>\n');
  assert.equal(render('> alpha\n```\nliteral\n```'),
    '<blockquote>\n<p>alpha</p>\n</blockquote>\n<pre><code>literal\n</code></pre>\n');
  assert.equal(render('1234. alpha\n      ```\n      literal\n      ```'),
    '<ol start="1234">\n<li>alpha\n<pre><code>literal\n</code></pre></li>\n</ol>\n');
  assert.equal(render('> alpha\n> # heading'),
    '<blockquote>\n<p>alpha</p>\n<h1 id="heading">heading</h1>\n</blockquote>\n');
});

test('deep lazy collection keeps literal content, bounded work and fresh render state', () => {
  const source = '> ' + '123456789. '.repeat(48) + 'alpha\n    ```\n    β [literal](/hidden)\n    ```\n    [visible](/real)';
  const html = render(source);
  assert.equal((html.match(/<ol /g) || []).length, 48);
  assert.match(html, /alpha\n<code>β \[literal\]\(\/hidden\)<\/code>\n<a href="\/real">visible<\/a>/);
  assert.doesNotMatch(html, /href="\/hidden"|<pre>/);
  const small = vm.createContext({source: '> alpha\n' + '    # text\n'.repeat(2000)});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 50000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small), error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("> fresh\\n    # literal")', small),
    '<blockquote>\n<p>fresh\n# literal</p>\n</blockquote>\n');
});

test('official paragraph and setext whitespace cases retain their unchanged expected HTML', () => {
  const examples = new Set([49, 56, 70, 82, 84, 87, 89, 113, 222, 223, 224, 226, 252, 255, 275, 276, 552, 636, 637, 645, 649]);
  const corpus = JSON.parse(readFileSync(new URL('../../tests/fixtures/commonmark/spec.json', import.meta.url), 'utf8'));
  for (const example of corpus) {
    if (!examples.has(example.example)) continue;
    // Only existing HTML serialization differences: generated heading IDs and
    // HTML5 void-element spelling. Whitespace and content remain exact.
    const html = render(example.markdown).replace(/(<h[1-6]) id="[^"]*"/g, '$1').replace(/<br>/g, '<br />');
    assert.equal(html, example.html, 'CommonMark example ' + example.example);
  }
});
