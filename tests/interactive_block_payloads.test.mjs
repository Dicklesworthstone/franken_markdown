// Block payloads use Markdown's LF-delimited source lines. U+2028/U+2029 in
// their text and padding on an empty list marker must not discard ownership.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = source => context.parseMarkdownClient(source);
const separators = ['\u2028', '\u2029'];

test('headings retain their complete Unicode payload and heading identity', () => {
  for (const separator of separators) {
    assert.equal(render('# one' + separator + 'two'),
      '<h1 id="one-two">one' + separator + 'two</h1>\n');
    assert.equal(render('> ## one' + separator + 'two'),
      '<blockquote>\n<h2 id="one-two">one' + separator + 'two</h2>\n</blockquote>\n');
  }
});

test('list and task payloads retain text and live inline syntax after Unicode separators', () => {
  for (const separator of separators) {
    assert.equal(render('- one' + separator + '**two**\n- [x] three' + separator + '[tail](/ok)'),
      '<ul>\n<li>one' + separator + '<strong>two</strong></li>\n' +
      '<li class="task"><input type="checkbox" disabled checked> three' + separator +
      '<a href="/ok">tail</a></li>\n</ul>\n');
  }
});

test('footnote definitions retain complete Unicode bodies and their references', () => {
  for (const separator of separators) {
    const html = render('text[^n]\n\n[^n]: first' + separator + 'last');
    assert.match(html, /href="#fn-n"/);
    assert.ok(html.includes('<li id="fn-n"><p>first' + separator + 'last</p>'));
    assert.doesNotMatch(html, /\[\^n\]:/);
  }
});

test('fenced code remains literal when its info contains a Unicode separator', () => {
  for (const separator of separators) {
    assert.equal(render('```js' + separator + 'extra\n[not a link](/outside)\n```'),
      '<pre><code class="language-js">[not a link](/outside)\n</code></pre>\n');
  }
});

test('callout titles retain their full Unicode text', () => {
  for (const separator of separators) {
    assert.equal(render('> [!NOTE] first' + separator + 'last\n> body'),
      '<aside class="callout callout-note"><p class="callout-title">first' + separator +
      'last</p>\n<p>body</p>\n</aside>\n');
  }
});

test('empty list markers use one content column regardless of trailing spaces or tabs', () => {
  for (const indent of [0, 1, 2, 3]) {
    for (const marker of ['-', '+', '*', '1.', '10.', '1)', '123456789.']) {
      for (const padding of ['', ' ', '  ', '   ', '    ', '\t', ' \t', '\t\t', '        ']) {
        const source = ' '.repeat(indent) + marker + padding + '\n' +
          ' '.repeat(indent + marker.length + 1) + 'child\n';
        const ordered = /^\d/.test(marker), start = parseInt(marker, 10);
        const tag = ordered ? 'ol' : 'ul';
        const attribute = ordered && start !== 1 ? ' start="' + start + '"' : '';
        assert.equal(render(source), '<' + tag + attribute + '>\n<li>child</li>\n</' + tag + '>\n',
          JSON.stringify(source));
      }
    }
  }
});

test('empty-marker continuation retains nested quote and list ownership', () => {
  assert.equal(render('> -   \n>   child'),
    '<blockquote>\n<ul>\n<li>child</li>\n</ul>\n</blockquote>\n');
  assert.equal(render('- -\t\n    child'),
    '<ul>\n<li><ul>\n<li>child</li>\n</ul></li>\n</ul>\n');
  assert.equal(render('- parent\n\n  +\t\n    child'),
    '<ul>\n<li><p>parent</p>\n<ul>\n<li>child</li>\n</ul></li>\n</ul>\n');
});

test('empty items cannot take content across a blank line or insufficient indentation', () => {
  for (const source of ['-   \n\n  child', '-\t\n\n  child', '-   \n child']) {
    assert.equal(render(source), '<ul>\n<li></li>\n</ul>\n<p>child</p>\n');
  }
  // Nonempty markers still use their actual content indentation.
  assert.equal(render('-   parent\n\n  child'),
    '<ul>\n<li>parent</li>\n</ul>\n<p>child</p>\n');
  // A dash immediately following a paragraph remains its setext underline.
  assert.equal(render('- parent\n  -\t\n    child'),
    '<ul>\n<li><h2 id="parent">parent</h2>\nchild</li>\n</ul>\n');
});
