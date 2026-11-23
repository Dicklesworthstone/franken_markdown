// A reference-only prefix still owns its paragraph. Execute the actual
// editable-document renderer and check visible source and destination identity.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = source => context.parseMarkdownClient(source);
const quote = text => text.split('\n').map(line => '> ' + line).join('\n');
const list = (text, marker = '- ') => marker + text.replace(/\n/g, '\n' + ' '.repeat(marker.length));
const containers = [
  ['root', text => text],
  ['quote', quote],
  ['list', list],
  ['ordered list', text => list(text, '12. ')],
  ['quoted list', text => quote(list(text))],
  ['list with quote', text => list(quote(text))],
];

test('hanging prose after a reference prefix cannot become code or publish later definitions', () => {
  for (const indent of ['    ', '\t', '        ']) {
    assert.equal(render('[a]: /real\n' + indent + 'code\n[b]: /ghost\nend\n\n[a] [b]'),
      '<p>code\n[b]: /ghost\nend</p>\n<p><a href="/real">a</a> [b]</p>\n', JSON.stringify(indent));
  }
});

test('non-1 and empty list markers remain visible in the paragraph begun by a definition', () => {
  for (const marker of ['2. text', '0) text', '12. text', '-', '+', '*', '2.']) {
    assert.equal(render('[a]: /real\n' + marker + '\n[b]: /ghost\n\n[a] [b]'),
      '<p>' + marker + '\n[b]: /ghost</p>\n<p><a href="/real">a</a> [b]</p>\n', marker);
  }
});

test('quoted non-1 reference-shaped items agree with native paragraph ownership', () => {
  for (const title of ['', '\n> "title for a"']) {
    assert.equal(render('> [a]: /x' + title + '\n> 2. [b]: /y\n\n[a] and [b]'),
      '<blockquote>\n<p>2. [b]: /y</p>\n</blockquote>\n<p><a href="/x"'
        + (title ? ' title="title for a"' : '') + '>a</a> and [b]</p>\n');
  }
});

test('nested containers preserve hanging source without donating reference authority', () => {
  for (const [name, wrap] of containers) {
    const html = render('[a] [b]\n\n' + wrap('[a]: /real\n    prose\n[b]: /ghost'));
    assert.match(html, /<a href="\/real">a<\/a> \[b\]/, name);
    assert.match(html, /prose\n\[b\]: \/ghost/, name);
    assert.doesNotMatch(html, /href="\/ghost"|<pre>/, name);
  }
});

test('consecutive hanging definitions remain eligible and preserve first-definition wins', () => {
  for (const [name, wrap] of containers) {
    const html = render(wrap('[a]: /first\n    [b]: /second\n\t[a]: /shadow\n2. visible') + '\n\n[a] [b]');
    assert.match(html, /2\. visible/, name);
    assert.match(html, /<a href="\/first">a<\/a> <a href="\/second">b<\/a>/, name);
    assert.doesNotMatch(html, /shadow|\[b\]:|<pre>/, name);
  }
});

test('multiline hanging definitions keep titles and do not reopen a block boundary', () => {
  assert.equal(render('[a]: /first\n    [b\n     c]:\n      /second\n      "Read"\n2. visible\n\n[a] [b c]'),
    '<p>2. visible</p>\n<p><a href="/first">a</a> <a href="/second" title="Read">b c</a></p>\n');
});

test('a later real definition can resolve forward links without being shadowed by paragraph text', () => {
  assert.equal(render('[b]\n\n[a]: /first\n2. [b]: /ghost\n\n[b]: /real\n\n[a] [b]'),
    '<p><a href="/real">b</a></p>\n<p>2. <a href="/real">b</a>: /ghost</p>\n'
      + '<p><a href="/first">a</a> <a href="/real">b</a></p>\n');
});

test('blank lines and interrupting blocks restore ordinary reference boundaries', () => {
  assert.equal(render('> [a]: /x\n>\n> 2. [b]: /y\n\n[a] and [b]'),
    '<blockquote>\n<ol start="2">\n<li></li>\n</ol>\n</blockquote>\n'
      + '<p><a href="/x">a</a> and <a href="/y">b</a></p>\n');
  for (const boundary of ['\n', '# Heading\n', '---\n', '```\ncode\n```\n', '> quote\n']) {
    const html = render('[a]: /x\n' + boundary + '2. [b]: /y\n\n[a] [b]');
    assert.match(html, /<ol start="2">/, boundary);
    assert.match(html, /<a href="\/x">a<\/a> <a href="\/y">b<\/a>/, boundary);
  }
});

test('blank-separated indentation still owns code instead of defining a reference', () => {
  assert.equal(render('[a]: /real\n\n    [b]: /ghost\n\n[a] [b]'),
    '<pre><code>[b]: /ghost\n</code></pre>\n<p><a href="/real">a</a> [b]</p>\n');
});

test('tables and footnotes retain their native exceptions after a definition-only prefix', () => {
  const table = render('> [a]: /x\n> | h |\n> | - |\n> 2. [b]: /y\n\n[a] [b]');
  assert.match(table, /<th>h<\/th>/);
  assert.match(table, /<ol start="2">/);
  assert.match(table, /<a href="\/x">a<\/a> <a href="\/y">b<\/a>/);
  const footnote = render('[a]: /x\n[^n]: note [a]\n\n[a] note[^n]');
  assert.match(footnote, /id="fn-n"><p>note <a href="\/x">a<\/a><\/p>/);
  assert.match(footnote, /href="#fn-n" role="doc-noteref"/);
  assert.equal(render('[a]: /x\n    text\n[^n]: ghost\n\n[^n]'),
    '<p>text\n[^n]: ghost</p>\n<p>[^n]</p>\n');
});

test('a malformed next-line title stays visible and cannot authorize following definitions', () => {
  assert.equal(render('[a]: /x\n    "title" trailing\n[b]: /y\n\n[a] [b]'),
    '<p>&quot;title&quot; trailing\n[b]: /y</p>\n<p><a href="/x">a</a> [b]</p>\n');
});

test('noninterrupting HTML and display dollars retain the reference paragraph', () => {
  assert.equal(render('[a]: /x\n<custom>\n[b]: /y\n\n[a] [b]'),
    '<p>&lt;custom&gt;\n[b]: /y</p>\n<p><a href="/x">a</a> [b]</p>\n');
  assert.equal(render('[a]: /x\n$$\n[b]: /y\n$$\n\n[a] [b]'),
    '<p>$$\n[b]: /y\n$$</p>\n<p><a href="/x">a</a> [b]</p>\n');
});

test('setext headings use visible continuation text without promoting a removed definition', () => {
  assert.equal(render('[a]: /x\n    Title\n===\n[a]'),
    '<h1 id="title">Title</h1>\n<p><a href="/x">a</a></p>\n');
  assert.equal(render('[a]: /x\n===\n[a]'), '<p>===\n<a href="/x">a</a></p>\n');
});
