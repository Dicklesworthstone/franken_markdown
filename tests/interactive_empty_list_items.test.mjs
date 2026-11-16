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
