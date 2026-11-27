import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);

test('an empty list item cannot absorb its first paragraph after a blank line', () => {
  assert.equal(render('-\n\n  outside\n'), '<ul>\n<li></li>\n</ul>\n<p>outside</p>\n');
  assert.equal(render('- first\n-\n\n  outside\n'),
    '<ul>\n<li>first</li>\n<li></li>\n</ul>\n<p>outside</p>\n');
  assert.equal(render('1. first\n2.\n\n   outside\n'),
    '<ol>\n<li>first</li>\n<li></li>\n</ol>\n<p>outside</p>\n');
});

test('a blank after an empty item may separate real same-list siblings', () => {
  for (let indent = 0; indent <= 3; indent++) {
    const pad = ' '.repeat(indent);
    assert.equal(render('-\n\n' + pad + '- next\n'),
      '<ul>\n<li></li>\n<li><p>next</p></li>\n</ul>\n');
    assert.equal(render('- first\n-\n\n' + pad + '- next\n'),
      '<ul>\n<li><p>first</p></li>\n<li></li>\n<li><p>next</p></li>\n</ul>\n');
    assert.equal(render('1. first\n2.\n\n' + pad + '3. next\n'),
      '<ol>\n<li><p>first</p></li>\n<li></li>\n<li><p>next</p></li>\n</ol>\n');
  }
});

test('empty-item boundaries preserve outside indented code and inert definitions', () => {
  for (const prefix of ['-\n', '- first\n-\n', '1. first\n2.\n']) {
    const html = render(prefix + '\n    [r]: /literal\n\n[r]\n');
    assert.match(html, /<\/ul>\n<pre><code>|<\/ol>\n<pre><code>/);
    assert.match(html, /<pre><code>\[r\]: \/literal\n<\/code><\/pre>/);
    assert.match(html, /<p>\[r\]<\/p>/);
    assert.doesNotMatch(html, /href="\/literal"/);
  }
  assert.equal(render('- first\n-\n\n    - literal\n'),
    '<ul>\n<li>first</li>\n<li></li>\n</ul>\n<pre><code>- literal\n</code></pre>\n');
});

test('changed markers and thematic breaks remain outside the preceding list', () => {
  assert.equal(render('- first\n-\n\n+ next\n'),
    '<ul>\n<li>first</li>\n<li></li>\n</ul>\n<ul>\n<li>next</li>\n</ul>\n');
  assert.equal(render('* first\n*\n\n* * *\n'),
    '<ul>\n<li>first</li>\n<li></li>\n</ul>\n<hr>\n');
});

test('immediate empty-item content and nonempty paragraph continuations retain their ownership', () => {
  assert.equal(render('- first\n-\n  second\n'),
    '<ul>\n<li>first</li>\n<li>second</li>\n</ul>\n');
  assert.equal(render('- first\n- second\n\n  tail\n'),
    '<ul>\n<li><p>first</p></li>\n<li><p>second</p>\n<p>tail</p></li>\n</ul>\n');
});

test('nested containers keep the outside block in its own parent scope', () => {
  assert.equal(render('> - first\n> -\n>\n>   outside\n'),
    '<blockquote>\n<ul>\n<li>first</li>\n<li></li>\n</ul>\n<p>outside</p>\n</blockquote>\n');
  assert.equal(render('- outer\n  - first\n  -\n\n    outside\n'),
    '<ul>\n<li><p>outer</p>\n<ul>\n<li>first</li>\n<li></li>\n</ul>\n<p>outside</p></li>\n</ul>\n');
});

test('multiple blank lines keep the same sibling and parent ownership', () => {
  for (let count = 2; count <= 4; count++) {
    const gap = '\n'.repeat(count);
    assert.equal(render('- parent\n  - first\n  -\n' + gap + '  - next\n'),
      '<ul>\n<li>parent\n<ul>\n<li><p>first</p></li>\n<li></li>\n<li><p>next</p></li>\n</ul></li>\n</ul>\n');
    assert.equal(render('- outer\n  - first\n  -\n' + gap + '    outside\n'),
      '<ul>\n<li><p>outer</p>\n<ul>\n<li>first</li>\n<li></li>\n</ul>\n<p>outside</p></li>\n</ul>\n');
  }
});

test('task markers and definitions remain content rather than empty markers', () => {
  assert.equal(render('- [x]\n\n  tail\n'),
    '<ul>\n<li class="task"><p><input type="checkbox" disabled checked> tail</p></li>\n</ul>\n');
  assert.equal(render('- [r]: /real\n\n  [r]\n'),
    '<ul>\n<li><p><a href="/real">r</a></p></li>\n</ul>\n');
});

test('task separators preserve code indentation and only real definitions own links', () => {
  for (const marker of ['-', '1.', '12)']) {
    for (const checkbox of ['[x]', '[X]', '[ ]']) {
      for (const gap of ['     ', '\t    ', ' \t', '\t\t']) {
        const source = marker + ' ' + checkbox + gap + '[guide]: /literal\noutside\n\n'
          + '[guide]: /real\n\n[guide]';
        const html = render(source);
        assert.match(html, /<li class="task"><input type="checkbox" disabled(?: checked)?> <pre><code>\[guide\]: \/literal\n<\/code><\/pre><\/li>/);
        assert.match(html, /<\/(?:ul|ol)>\n<p>outside<\/p>\n<p><a href="\/real">guide<\/a><\/p>/);
        assert.doesNotMatch(html, /href="\/literal"/);
      }
    }
  }
});

test('code in nested task items cannot create citations or absorb outer paragraphs', () => {
  for (const source of [
    '> - [x]     [^n]: sample\noutside\n\nUse[^n].',
    '- - [x]     [^n]: sample\noutside\n\nUse[^n].',
    '> 3) [X]     [^n]: sample\noutside\n\nUse[^n].',
  ]) {
    const html = render(source);
    assert.match(html, /<pre><code>\[\^n\]: sample\n<\/code><\/pre>/);
    assert.match(html, /<p>outside<\/p>\n<p>Use\[\^n\]\.<\/p>/);
    assert.doesNotMatch(html, /class="footnotes"/);
  }
});

test('only an ASCII space or tab separates the checkbox from live item content', () => {
  for (const separator of ['\u00a0', '\u2003', '\u2028', '\u2029', '\uFEFF', '\v', '\f']) {
    const source = '- [x]' + separator + '[r]: /sample\n\n[r]';
    assert.equal(render(source), '<ul>\n<li>[x]' + separator + '[r]: /sample</li>\n</ul>\n<p>[r]</p>\n');
  }
  assert.match(render('- [X]\t[r]: /real\n\n[r]'), /<a href="\/real">r<\/a>/);
  assert.match(render('- [ ]    **text**'), /disabled> <strong>text<\/strong>/);
});
