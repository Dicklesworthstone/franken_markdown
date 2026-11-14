// Execute the parser actually embedded in saved interactive HTML documents.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);

test('sibling indentation follows the previous item content width, not marker alignment', () => {
  for (const [marker, start, width] of [['1.', 1, 3], ['23)', 23, 4]]) {
    for (let indent = 0; indent < width; indent++) {
      const source = `${marker} first\n${' '.repeat(indent)}42${marker.at(-1)} second`;
      const opening = start === 1 ? '<ol>' : '<ol start="23">';
      assert.equal(render(source), `${opening}\n<li>first</li>\n<li>second</li>\n</ol>\n`, source);
      assert.equal(render(source.replace('\n', '\n\n')),
        `${opening}\n<li><p>first</p></li>\n<li><p>second</p></li>\n</ol>\n`, source);
    }
  }
  assert.equal(render('- first\n - second\n  - third\n   - fourth'),
    '<ul>\n<li>first</li>\n<li>second</li>\n<li>third</li>\n<li>fourth</li>\n</ul>\n');
});

test('changed marker types and empty siblings end the old list for every newline form', () => {
  for (const [source, expected] of [
    ['- first\n2. second\n', '<ul>\n<li>first</li>\n</ul>\n<ol start="2">\n<li>second</li>\n</ol>\n'],
    ['- first\n 0) second\n', '<ul>\n<li>first</li>\n</ul>\n<ol start="0">\n<li>second</li>\n</ol>\n'],
    ['1. first\n-\n', '<ol>\n<li>first</li>\n</ol>\n<ul>\n<li></li>\n</ul>\n'],
    ['- first\n+\n', '<ul>\n<li>first</li>\n</ul>\n<ul>\n<li></li>\n</ul>\n'],
    ['1. first\n 2) second\n', '<ol>\n<li>first</li>\n</ol>\n<ol start="2">\n<li>second</li>\n</ol>\n'],
    ['> first\n-\n', '<blockquote>\n<p>first</p>\n</blockquote>\n<ul>\n<li></li>\n</ul>\n'],
  ]) {
    for (const newline of ['\n', '\r\n', '\r']) {
      assert.equal(render(source.replaceAll('\n', newline)), expected, source);
    }
  }
});

test('non-1 markers keep real paragraph boundaries and existing ordered child lists', () => {
  for (const source of ['- first\n  2. second', '1. first\n   2. second']) {
    const tag = source.startsWith('-') ? 'ul' : 'ol';
    assert.equal(render(source), `<${tag}>\n<li>first\n2. second</li>\n</${tag}>\n`, source);
  }
  for (const marker of ['1234567890. second', '2.second']) {
    assert.equal(render(`- first\n${marker}`), `<ul>\n<li>first\n${marker}</li>\n</ul>\n`);
  }
  assert.equal(render('- 2. child\n     wrapped\n  3. second'),
    '<ul>\n<li><ol start="2">\n<li>child\nwrapped</li>\n<li>second</li>\n</ol></li>\n</ul>\n');
  for (const source of ['> first\n> 2. second', '> first\n    2. second']) {
    assert.equal(render(source), '<blockquote>\n<p>first\n2. second</p>\n</blockquote>\n', source);
  }
  for (const marker of ['0.', '1.', '2.', '1986)', '999999999)']) {
    assert.equal(render(`> first\n    ${marker} second`),
      `<blockquote>\n<p>first\n${marker} second</p>\n</blockquote>\n`, marker);
    assert.equal(render(`123. first\n    ${marker} second`),
      `<ol start="123">\n<li>first\n${marker} second</li>\n</ol>\n`, marker);
  }
});

test('blank separators inside a child list loosen only that child', () => {
  for (const blank of ['\n', '\n\n', '\t\n']) {
    assert.equal(render(`- parent\n  - first\n${blank}  - second`),
      '<ul>\n<li>parent\n<ul>\n<li><p>first</p></li>\n<li><p>second</p></li>\n</ul></li>\n</ul>\n');
    assert.equal(render(`- parent\n  - child\n${blank}    tail`),
      '<ul>\n<li>parent\n<ul>\n<li><p>child</p>\n<p>tail</p></li>\n</ul></li>\n</ul>\n');
  }
  assert.equal(render('- [x] parent\n  - first\n\n  - second'),
    '<ul>\n<li class="task"><input type="checkbox" disabled checked> parent\n<ul>\n<li><p>first</p></li>\n<li><p>second</p></li>\n</ul></li>\n</ul>\n');
});

test('dedented paragraphs and differently marked children give the blank back to the parent', () => {
  assert.equal(render('- parent\n  - child\n\n  tail'),
    '<ul>\n<li><p>parent</p>\n<ul>\n<li>child</li>\n</ul>\n<p>tail</p></li>\n</ul>\n');
  assert.equal(render('- parent\n  - first\n\n  + second'),
    '<ul>\n<li><p>parent</p>\n<ul>\n<li>first</li>\n</ul>\n<ul>\n<li>second</li>\n</ul></li>\n</ul>\n');
  for (let markerIndent = 2; markerIndent <= 5; markerIndent++) {
    const childContent = markerIndent + 2;
    for (let nextIndent = 2; nextIndent <= childContent + 2; nextIndent++) {
      const source = `- parent\n${' '.repeat(markerIndent)}- child\n\n${' '.repeat(nextIndent)}tail`;
      const html = render(source);
      if (nextIndent < childContent) {
        assert.ok(html.startsWith('<ul>\n<li><p>parent</p>\n<ul>\n<li>child</li>'), source);
        assert.ok(html.includes(nextIndent >= 6 ? '<pre><code>tail\n</code></pre>' : '<p>' + ' '.repeat(nextIndent - 2) + 'tail</p>'), source);
      } else {
        assert.ok(html.startsWith('<ul>\n<li>parent\n<ul>\n<li><p>child</p>'), source);
        assert.equal((html.match(/<p>/g) || []).length, 2, source);
        assert.match(html, /<p> *tail<\/p><\/li>\n<\/ul><\/li>\n<\/ul>\n$/, source);
      }
    }
  }
});

test('sibling boundaries preserve Unicode references and nested quote ownership', () => {
  for (const newline of ['\n', '\r\n', '\r']) {
    assert.equal(render('- café\n2. [réf]: /target\n3. [réf]\n'.replaceAll('\n', newline)),
      '<ul>\n<li>café</li>\n</ul>\n<ol start="2">\n<li></li>\n<li><a href="/target">réf</a></li>\n</ol>\n');
  }
  assert.equal(render('> - first\n2. second'),
    '<blockquote>\n<ul>\n<li>first</li>\n</ul>\n</blockquote>\n<ol start="2">\n<li>second</li>\n</ol>\n');
  assert.equal(render('> - parent\n>   - first\n>\n>   - second'),
    '<blockquote>\n<ul>\n<li>parent\n<ul>\n<li><p>first</p></li>\n<li><p>second</p></li>\n</ul></li>\n</ul>\n</blockquote>\n');
});

test('thematic breaks keep precedence over sibling markers and own only their actual gaps', () => {
  for (const marker of ['-', '*']) {
    for (const separator of [' ', '\t']) {
      const rule = Array(3).fill(marker).join(separator);
      for (const indent of ['', ' ']) {
        for (const blank of ['', '\n']) {
          const source = `${marker} first\n${blank}${indent}${rule}`;
          assert.equal(render(source), '<ul>\n<li>first</li>\n</ul>\n<hr>\n', source);
        }
      }
      assert.equal(render(`- parent\n  ${marker} child\n\n  ${rule}`),
        '<ul>\n<li><p>parent</p>\n<ul>\n<li>child</li>\n</ul>\n<hr></li>\n</ul>\n');
      assert.equal(render(`- parent\n  ${marker} child\n\n    ${rule}`),
        '<ul>\n<li>parent\n<ul>\n<li><p>child</p>\n<hr></li>\n</ul></li>\n</ul>\n');
    }
  }
  // Two bullet markers form a real nested item, so the precedence check must
  // not reject marker-looking source unless it satisfies the rule grammar.
  assert.equal(render('- first\n - -'),
    '<ul>\n<li>first</li>\n<li><ul>\n<li></li>\n</ul></li>\n</ul>\n');
});

test('official sibling and loose-list examples keep their expected structure and text', () => {
  const corpus = JSON.parse(readFileSync(new URL('../../tests/fixtures/commonmark/spec.json', import.meta.url), 'utf8'));
  const examples = [11, 60, 295, 296, 305, 306, 307, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319, 320];
  // Renderers differ in HTML5/XHTML rule spelling and newlines between tags.
  // Keep every element, attribute and all code/paragraph text whitespace.
  const structure = html => html.replaceAll('<hr />', '<hr>').replace(/>\s+</g, '><').trim();
  for (const number of examples) {
    const example = corpus.find(example => example.example === number);
    assert.ok(example, `missing CommonMark example ${number}`);
    assert.equal(structure(render(example.markdown)), structure(example.html), `CommonMark example ${number}`);
  }
});

test('long blank runs and many changing sibling indents complete within the render budget', () => {
  const bounded = vm.createContext({source: '- parent\n  - first\n' + '\n'.repeat(12000) + '  - second'});
  vm.runInContext(code, bounded);
  assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}),
    '<ul>\n<li>parent\n<ul>\n<li><p>first</p></li>\n<li><p>second</p></li>\n</ul></li>\n</ul>\n');
  bounded.source = Array.from({length: 2048}, (_, i) => `${' '.repeat(i % 4)}${i + 7}. item ${i}`).join('\n');
  assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}),
    '<ol start="7">\n' + Array.from({length: 2048}, (_, i) => `<li>item ${i}</li>\n`).join('') + '</ol>\n');
  assert.equal(vm.runInContext('parseMarkdownClient("- fresh")', bounded), '<ul>\n<li>fresh</li>\n</ul>\n');
});
