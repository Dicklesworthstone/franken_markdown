// Production standalone parsing: ownership must survive collection and render.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const quote = source => source.split('\n').map(line => '> ' + line).join('\n');

test('lazy quoted paragraphs preserve reference-shaped source instead of publishing a definition', () => {
  for (const prefix of ['> ', '> > ', '> - ', '- > ', '> - > ']) {
    const source = `${prefix}first\n[not-a-definition]: /literal\n\n[not-a-definition]`;
    const html = render(source);
    assert.doesNotMatch(html, /href="\/literal"/, source);
    assert.match(html, /first\n\[not-a-definition\]: \/literal<\/p>|first\n\[not-a-definition\]: \/literal<\/li>/, source);
    assert.ok(html.indexOf('[not-a-definition]: /literal') < html.lastIndexOf('</blockquote>'), source);
    assert.ok(html.endsWith('<p>[not-a-definition]</p>\n'), source);
  }
});

test('quoted paragraphs accept real lazy continuation through nested lists and quotes', () => {
  for (const prefix of ['> ', '> > ', '> - ', '> 1. ', '- > ', '> - > ', '> > - ']) {
    const source = `${prefix}first\ncontinued`;
    const html = render(source);
    assert.match(html, /first\ncontinued/, source);
    assert.ok(html.indexOf('continued') < html.lastIndexOf('</blockquote>'), source);
  }
  assert.equal(render('> first\ncontinued\n\noutside'),
    '<blockquote>\n<p>first\ncontinued</p>\n</blockquote>\n<p>outside</p>\n');
});

test('completed and literal list blocks cannot capture dedented prose', () => {
  for (const body of [
    '# Heading', 'Title\n  ===', '```\n  code\n  ```', '~~~\n  code',
    '    indented code', '| A | B |\n  | --- | --- |\n  | a | b |',
    '[^note]: Note body', '> # Heading', '> ```\n  > code',
    '- # Heading', '- ```\n    code', '[x] # Heading', '[ ] ```\n  code',
  ]) {
    const source = `- ${body}\noutside`;
    assert.match(render(source), /<\/ul>\n<p>outside<\/p>\n$/, source);
  }
});

test('quoted nonparagraph blocks cannot capture unmarked prose', () => {
  for (const body of [
    '# Heading', 'Title\n===', '```\ncode\n```', '~~~\ncode',
    '    indented code', '| A | B |\n| --- | --- |\n| a | b |',
    '[^note]: Note body', '> # Heading', '- # Heading', '- [x] ```\n  code',
  ]) {
    const source = `${quote(body)}\noutside`;
    assert.match(render(source), /<\/blockquote>\n<p>outside<\/p>\n$/, source);
  }
});

test('real definitions after container code survive while code-shaped definitions stay literal', () => {
  for (const [prefix, indent, end] of [['> ', '> ', '</blockquote>'], ['- ', '  ', '</ul>'], ['23) ', '    ', '</ol>']]) {
    for (const fence of ['```', '~~~']) {
      const source = `${prefix}${fence}\n${indent}[fake]: /fake\n[real]: /real\n\n[fake] [real]`;
      const html = render(source);
      assert.match(html, /\[fake\]: \/fake/);
      assert.doesNotMatch(html, /href="\/fake"/);
      assert.match(html, /<a href="\/real">real<\/a>/, source);
      assert.ok(html.indexOf(end) < html.indexOf('href="/real"'), source);
    }
  }
});

test('an explicitly resumed paragraph restores laziness after a closed block or blank', () => {
  for (const source of [
    '> ```\n> code\n> ```\n> first\ncontinued',
    '> Title\n> ===\n> first\ncontinued',
    '- ```\n  code\n  ```\n  first\ncontinued',
    '- first\n\n  second\ncontinued',
    '- first\n  - child\n    text\ncontinued',
    '- ```\n  code\n  ```\t\n  first\ncontinued',
  ]) {
    const html = render(source);
    assert.ok(html.indexOf('continued') < Math.max(html.lastIndexOf('</ul>'), html.lastIndexOf('</blockquote>')), source);
    assert.ok(!html.endsWith('<p>continued</p>\n'), source);
  }
});

test('setext headings own complete paragraphs without consuming quote markers', () => {
  assert.equal(render('first\nsecond\n==='), '<h1 id="first-second">first\nsecond</h1>\n');
  assert.equal(render('> first\n> second\n> ===\noutside'),
    '<blockquote>\n<h1 id="first-second">first\nsecond</h1>\n</blockquote>\n<p>outside</p>\n');
  assert.equal(render('> first\n=\ncontinued'),
    '<blockquote>\n<p>first\n=\ncontinued</p>\n</blockquote>\n');
});

test('tab columns and literal blank lines survive list-container stripping', () => {
  assert.match(render('-\tfirst\n\t# Heading\noutside'), /<h1 id="heading">Heading<\/h1><\/li>\n<\/ul>\n<p>outside<\/p>/);
  assert.match(render('- first\n  \t- child\ncontinued'), /<li>child\ncontinued<\/li>/);
  assert.equal(render('- ```\n  code\n\noutside'),
    '<ul>\n<li><pre><code>code\n\n</code></pre></li>\n</ul>\n<p>outside</p>\n');
});

test('ending an inner quote preserves a new explicit outer paragraph and its continuation', () => {
  assert.equal(render('> > ```\n> > code\n> outside inner\noutside outer'),
    '<blockquote>\n<blockquote>\n<pre><code>code\n</code></pre>\n</blockquote>\n<p>outside inner\noutside outer</p>\n</blockquote>\n');
});

test('lazy marker-looking lines stay paragraph text after their indentation is removed', () => {
  for (const prefix of ['> ', '123. ']) {
    for (const continuation of ['=', '    - hanging', '\t- hanging']) {
      const source = `${prefix}first\n${continuation}\ncontinued`;
      const html = render(source);
      assert.match(html, /first\n(?:=|- hanging)\ncontinued/, source);
      assert.doesNotMatch(html, /<h[1-6]/, source);
      assert.doesNotMatch(html, /<ul>/, source);
    }
  }
});

test('ordered markers other than one and empty items cannot interrupt an open paragraph', () => {
  for (const source of ['first\n2. continued', 'first\n1986. continued', 'first\n+', '> first\n2. continued', '- first\n2. continued']) {
    const html = render(source);
    assert.doesNotMatch(html, /<ol/, source);
    assert.match(html, /first\n(?:2\. continued|1986\. continued|\+)/, source);
  }
  assert.match(render('first\n1. new list'), /<\/p>\n<ol>/);
  assert.match(render('first\n\n2. new list'), /<ol start="2">/);
});

test('multiline reference titles cannot manufacture a fence or heading in container ownership', () => {
  for (const prefix of ['> ', '- ']) {
    const indent = prefix === '> ' ? '> ' : '  ';
    const source = `${prefix}[ref]: /real "title\n${indent}# literal heading\n${indent}\`\`\` literal fence\n${indent}tail"\ncontinued\n\n[ref]`;
    const html = render(source);
    assert.match(html, /<a href="\/real" title="title\n# literal heading\n``` literal fence\ntail">ref<\/a>/, source);
    assert.doesNotMatch(html, /<pre>|<h[1-6]/, source);
    assert.ok(html.indexOf('continued') < Math.max(html.indexOf('</blockquote>'), html.indexOf('</ul>')), source);
  }
});

test('container collection remains bounded and state is fresh after an exhausted render', () => {
  const bounded = vm.createContext({source: '> '.repeat(200) + 'leaf'});
  vm.runInContext(code, bounded);
  assert.match(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}), /leaf/);
  const small = vm.createContext({source: '> first\n' + '> continuation\n'.repeat(2000)});
  assert.ok(small.source.length < 150000);
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 150000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("> ok\\ncontinued")', small),
    '<blockquote>\n<p>ok\ncontinued</p>\n</blockquote>\n');
});
