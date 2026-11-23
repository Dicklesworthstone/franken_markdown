// Execute the renderer embedded in editable HTML, including source ownership
// and exact resource bindings. These are output regressions, not source checks.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = (source, images) => context.parseMarkdownClient(source, images);

test('spaced destinations remain clickable for inline and all reference forms', () => {
  for (const source of [
    '[guide](<docs/my guide.md>)',
    '[guide][doc]\n\n[doc]: <docs/my guide.md>',
    '[guide][]\n\n[guide]: <docs/my guide.md>',
    '[guide]\n\n[guide]: <docs/my guide.md>',
    '[guide]\n\n[guide]:\n  <docs/my guide.md>',
  ]) {
    assert.equal(render(source), '<p><a href="docs/my%20guide.md">guide</a></p>\n', source);
  }
});

test('space encoding preserves existing escapes, query delimiters, Unicode and titles', () => {
  assert.equal(render('[read](<docs/Café notes%20v2.md?x=one two&next=%25#part one> "A &amp; B")'),
    '<p><a href="docs/Café%20notes%20v2.md?x=one%20two&amp;next=%25#part%20one" title="A &amp; B">read</a></p>\n');
  assert.equal(render('[read](docs/one&#32;two.md)'),
    '<p><a href="docs/one%20two.md">read</a></p>\n');
  assert.equal(render('[email](<mailto:reader@example.com?subject=Read this>)'),
    '<p><a href="mailto:reader@example.com?subject=Read%20this">email</a></p>\n');
});

test('spaced image destinations retain exact binding identity before URL encoding', () => {
  const png = 'data:image/png;base64,AAAA';
  const images = new Map([['plots/my plot.png', png], ['plots/my%20plot.png', null]]);
  assert.equal(render('![plot](<plots/my plot.png>)', images),
    '<p><img src="' + png + '" alt="plot"></p>\n');
  assert.equal(render('![plot](plots/my%20plot.png)', images), '<p>plot</p>\n');
  assert.equal(render('![plot](<plots/my plot.png>)'),
    '<p><img src="plots/my%20plot.png" alt="plot"></p>\n');
  assert.equal(render('![plot][image]\n\n[image]: <plots/my plot.png>', images),
    '<p><img src="' + png + '" alt="plot"></p>\n');
  assert.equal(render('![plot](<plots/my plot.png>)', new Map([['plots/my plot.png', null]])),
    '<p>plot</p>\n');
});

test('accepting path spaces does not admit obfuscated schemes, controls or backslashes', () => {
  for (const destination of [
    'java script:alert(1)', 'java&#32;script:alert(1)',
    'java\tscript:alert(1)', 'java&#9;script:alert(1)',
    'javascript:alert(1)', 'data:text/html,unsafe', 'file:///secret',
    '\\\\example.test/path', '/\\example.test/path',
    'docs/one\u0001two.md', 'docs/one\u007ftwo.md',
  ]) {
    assert.doesNotMatch(render('[read](<' + destination + '>)'), /<a\b/, destination);
    assert.doesNotMatch(render('![plot](<' + destination + '>)'), /<img\b/, destination);
  }
  assert.equal(render('[read](<docs/a b:notes.md>)'),
    '<p><a href="docs/a%20b:notes.md">read</a></p>\n');
  for (const destination of ['notes(edition:2026).md', '(edition:2026).md', 'notes)edition:2026.md']) {
    assert.equal(render('[read](<' + destination + '>)'),
      '<p><a href="' + destination + '">read</a></p>\n');
  }
});

test('a bare reference destination cannot consume an unescaped opening angle bracket', () => {
  for (const destination of ['foo<bar', 'foo<bar>', '/docs<other.md', 'foo<bar> "Title"']) {
    const source = '[r]: ' + destination + '\n\n[r]';
    const html = render(source);
    assert.ok(html.includes('[r]:'), source);
    assert.ok(html.includes('<p>[r]</p>'), source);
    assert.doesNotMatch(html, /<a\b/, source);
  }
});

test('rejected references cannot shadow later valid definitions in nested containers', () => {
  for (const malformed of ['[r]: foo<bar', '> [r]: foo<bar', '- [r]: foo<bar']) {
    const html = render(malformed + '\n\n[r]: /real\n\n[r]');
    assert.match(html, /foo&lt;bar/);
    assert.match(html, /<a href="\/real">r<\/a>/);
    assert.doesNotMatch(html, /href="foo/);
  }
});

test('escaped opening angles and angle-delimited destinations retain valid references', () => {
  for (const destination of [String.raw`foo\<bar`, '<foo bar>']) {
    const html = render('[r]: ' + destination + '\n\n[r]');
    assert.doesNotMatch(html, /\[r\]:/);
    assert.match(html, /<a href="foo(?:&lt;|%20)bar">r<\/a>/);
  }
  assert.equal(render('`[r]: foo<bar`\n\n[r]'),
    '<p><code>[r]: foo&lt;bar</code></p>\n<p>[r]</p>\n');
});
