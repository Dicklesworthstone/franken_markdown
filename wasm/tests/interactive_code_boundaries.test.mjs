// Code bytes and end-of-input ownership through the production renderer.
// Run with: node --test wasm/tests/interactive_code_boundaries.test.mjs
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);

test('four-column code admission accepts mixed spaces and tabs without converting content tabs', () => {
  for (const indent of ['    ', '\t', ' \t', '  \t', '   \t']) {
    assert.equal(render(indent + 'alpha\tβ'), '<pre><code>alpha\tβ\n</code></pre>\n', indent);
    assert.equal(render('> ' + indent + 'alpha\tβ'),
      '<blockquote>\n<pre><code>alpha\tβ\n</code></pre>\n</blockquote>\n', indent);
  }
  for (const [source, content] of [
    ['\t\talpha', '\talpha\n'],
    ['    \talpha', '\talpha\n'],
    ['  \t  alpha', '  alpha\n'],
    ['   \t \talpha\tβ', ' \talpha\tβ\n'],
  ]) assert.equal(render(source), '<pre><code>' + content + '</code></pre>\n', source);
});

test('short blank indentation is removed while literal padding beyond four columns survives', () => {
  for (const blank of ['', ' ', '  ', '   ', '\t', ' \t', '  \t', '   \t', '    ']) {
    assert.equal(render('    first\n' + blank + '\n    last'),
      '<pre><code>first\n\nlast\n</code></pre>\n', JSON.stringify(blank));
  }
  assert.equal(render('    first\n     \n    \t\n    last'),
    '<pre><code>first\n \n\t\nlast\n</code></pre>\n');
  assert.equal(render('    first\n \t \n  \tlast\n\n\n'),
    '<pre><code>first\n \nlast\n</code></pre>\n');
});

test('code content cannot publish links, images, references or footnotes after mixed tab indentation', () => {
  const assets = {has() { assert.fail('indented code must not look up image assets'); }};
  const source = ' \t![image](image.png) [link](/hidden) [^n]\n  \t[ref]: /hidden\n\n[ref]';
  assert.equal(context.parseMarkdownClient(source, assets),
    '<pre><code>![image](image.png) [link](/hidden) [^n]\n[ref]: /hidden\n</code></pre>\n<p>[ref]</p>\n');
});

test('hanging paragraph indentation and real code boundaries stay distinct', () => {
  for (const indent of ['    ', '\t', ' \t', '  \t', '   \t']) {
    assert.equal(render('first\n' + indent + '[live](/real)'),
      '<p>first\n<a href="/real">live</a></p>\n');
    assert.equal(render('first\n\n' + indent + '[literal](/hidden)'),
      '<p>first</p>\n<pre><code>[literal](/hidden)\n</code></pre>\n');
  }
});

test('unterminated fences have no phantom EOF line and retain every authored blank line', () => {
  for (const fence of ['```', '~~~~~', '  ```lang']) {
    const attr = fence.includes('lang') ? ' class="language-lang"' : '';
    for (const [tail, expected] of [
      ['', ''], ['\n', ''], ['\nalpha', 'alpha\n'], ['\nalpha\n', 'alpha\n'],
      ['\nalpha\n\n', 'alpha\n\n'], ['\nalpha\n\n\n', 'alpha\n\n\n'],
      ['\nalpha\n  ', fence.startsWith(' ') ? 'alpha\n\n' : 'alpha\n  \n'],
      ['\nalpha\n\t', 'alpha\n\t\n'],
    ]) {
      assert.equal(render(fence + tail), '<pre><code' + attr + '>' + expected + '</code></pre>\n', fence + JSON.stringify(tail));
    }
  }
});

test('EOF identity survives nested list collection without dropping a real terminal blank', () => {
  for (const [opening, continuation, before, after] of [
    ['- ```', '  ', '<ul>\n<li>', '</li>\n</ul>\n'],
    ['1234. ```', '      ', '<ol start="1234">\n<li>', '</li>\n</ol>\n'],
    ['- - ```', '    ', '<ul>\n<li><ul>\n<li>', '</li>\n</ul></li>\n</ul>\n'],
  ]) {
    for (const newline of ['\n', '\r\n', '\r']) {
      for (let blanks = 0; blanks < 3; blanks++) {
        const source = opening + newline + continuation + 'alpha' + newline.repeat(blanks + 1);
        assert.equal(render(source), before + '<pre><code>alpha' + '\n'.repeat(blanks + 1)
          + '</code></pre>' + after, JSON.stringify(source));
      }
    }
  }
});

test('explicit quote blanks and list-exit blanks are actual code content', () => {
  assert.equal(render('> ```\n> alpha\n>\n'),
    '<blockquote>\n<pre><code>alpha\n\n</code></pre>\n</blockquote>\n');
  assert.equal(render('- > ```\n  > alpha\n  >\n'),
    '<ul>\n<li><blockquote>\n<pre><code>alpha\n\n</code></pre>\n</blockquote></li>\n</ul>\n');
  assert.equal(render('- ```\n  alpha\n\noutside'),
    '<ul>\n<li><pre><code>alpha\n\n</code></pre></li>\n</ul>\n<p>outside</p>\n');
  assert.equal(render('> ```\n> alpha\n\noutside'),
    '<blockquote>\n<pre><code>alpha\n</code></pre>\n</blockquote>\n<p>outside</p>\n');
});

test('closed fences and literal HTML and mathematics retain their source-ending policy', () => {
  assert.equal(render('```\nalpha\n\n```\n'), '<pre><code>alpha\n\n</code></pre>\n');
  assert.equal(render('```\nalpha\n\n```\n\n'), '<pre><code>alpha\n\n</code></pre>\n');
  assert.equal(render('$$\nalpha\n'), '<pre class="math-source">$$\nalpha\n</pre>\n');
  assert.equal(render('<script>\nalpha\n'), '<p>&lt;script&gt;\nalpha\n</p>\n');
  assert.equal(render('`alpha\n  beta`'), '<p><code>alpha   beta</code></p>\n');
});

test('many code lines remain bounded and a refused render does not retain EOF state', () => {
  const source = ' \talpha\tβ\n'.repeat(3000);
  assert.equal(render(source), '<pre><code>' + 'alpha\tβ\n'.repeat(3000) + '</code></pre>\n');
  const small = vm.createContext({source});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 50000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("```\\nfresh\\n")', small),
    '<pre><code>fresh\n</code></pre>\n');
});

test('official code-block examples use their exact unchanged expected HTML', () => {
  const examples = new Set([2, 111, 126, 127, 137, 139, 237]);
  const corpus = JSON.parse(readFileSync(new URL('../../tests/fixtures/commonmark/spec.json', import.meta.url), 'utf8'));
  for (const example of corpus) {
    if (examples.has(example.example)) assert.equal(render(example.markdown), example.html, 'CommonMark example ' + example.example);
  }
});
