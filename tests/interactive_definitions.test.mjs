// Container-aware collection runs through the production standalone renderer.
// Run directly, or through the established interactive_renderer.test.mjs entry.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
const code = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const quote = text => text.split('\n').map(line => '> ' + line).join('\n');
const list = (text, marker = '- ') => marker + text.replace(/\n/g, '\n' + ' '.repeat(marker.length));
const containers = [
  ['unordered item', text => list(text)],
  ['ordered item', text => list(text, '12. ')],
  ['quote', quote],
  ['list in quote', text => quote(list(text))],
  ['quote in list', text => list(quote(text))],
  ['deep mixed containers', text => list(quote(list(text, '7) ')))],
];

test('forward reference definitions at the first line of every container are document-global', () => {
  for (const [name, wrap] of containers) {
    const html = render('[guide][a b] ![diagram][a b]\n\n' + wrap('[ A  B ]: /manual "Read"'));
    assert.match(html, /<a href="\/manual" title="Read">guide<\/a>/, name);
    assert.match(html, /<img src="\/manual" alt="diagram" title="Read">/, name);
    assert.doesNotMatch(html, /\]: \/manual/, name);
  }
});

test('ordered list starts and empty definition-only items retain their container', () => {
  assert.equal(render('[r]\n\n12. [r]: /manual\n13. visible'),
    '<p><a href="/manual">r</a></p>\n<ol start="12">\n<li></li>\n<li>visible</li>\n</ol>\n');
});

test('first definition wins in source order across sibling and mixed containers', () => {
  for (const [, wrap] of containers) {
    const html = render('[r]\n\n' + wrap('[R]: /first') + '\n\n[r]: /second');
    assert.match(html, /href="\/first"/);
    assert.doesNotMatch(html, /second|\[R\]:/);
    assert.match(render('[r]: /first\n\n' + wrap('[R]: /second') + '\n\n[r]'), /href="\/first"/);
  }
});

test('definitions after a blank preserve loose lists, tasks and sibling blocks', () => {
  const html = render('[r]\n\n- [x] completed\n\n  [r]: /manual\n\n  second paragraph\n\n- [ ] pending');
  assert.match(html, /<a href="\/manual">r<\/a>/);
  assert.match(html, /<li class="task"><p><input type="checkbox" disabled checked> completed<\/p>/);
  assert.match(html, /<p>second paragraph<\/p>/);
  assert.match(html, /<p><input type="checkbox" disabled> pending<\/p>/);
});

test('task first-line definitions do not swallow the checkbox or following content', () => {
  const html = render('[r]\n\n- [x] [r]: /manual\n- ordinary');
  assert.match(html, /href="\/manual"/);
  assert.match(html, /<li class="task"><input type="checkbox" disabled checked> <\/li>/);
  assert.match(html, /<li>ordinary<\/li>/);
});

test('fenced definitions are inert even when the opener is the first item line', () => {
  for (const [name, wrap] of containers) {
    for (const marker of ['```', '~~~~']) {
      const html = render('[r] note[^n]\n\n' + wrap(marker + '\n[r]: /ghost\n[^n]: ghost note\n' + marker));
      assert.doesNotMatch(html, /<a\b|class="footnotes"/, name);
      assert.match(html, /\[r\]: \/ghost\n\[\^n\]: ghost note/, name);
    }
  }
});

test('an unclosed list fence never publishes definitions from its content', () => {
  const html = render('[r] note[^n]\n\n- ```\n  [r]: /ghost\n  [^n]: ghost');
  assert.doesNotMatch(html, /<a\b|class="footnotes"/);
  assert.match(html, /<pre><code>\[r\]: \/ghost/);
});

test('container-relative indented code never publishes reference or note definitions', () => {
  for (const [name, wrap] of containers) {
    const html = render('[r] note[^n]\n\n' + wrap('    [r]: /ghost\n    [^n]: ghost'));
    assert.doesNotMatch(html, /<a\b|class="footnotes"/, name);
    assert.match(html, /<pre><code>/, name);
  }
});

test('reference-shaped paragraph continuation is preserved, not registered or removed', () => {
  for (const [name, wrap] of [['root', text => text], ...containers]) {
    const html = render(wrap('ordinary paragraph\n[r]: /ghost') + '\n\n[r]');
    assert.doesNotMatch(html, /href="\/ghost"/, name);
    assert.match(html, /ordinary paragraph\n\[r\]: \/ghost/, name);
  }
});

test('a real definition cannot be replaced by a later paragraph-shaped definition', () => {
  const html = render('[r]: /real\n\ntext\n[r]: /ghost\n\n[r]');
  assert.match(html, /<a href="\/real">r<\/a>: \/ghost/);
  assert.equal((html.match(/href="\/real"/g) || []).length, 2);
  assert.doesNotMatch(html, /href="\/ghost"/);
});

test('callout labels and table cells stay inline leaves while child definitions resolve', () => {
  const html = render('[r] [fake]\n\n> [!NOTE] [fake]: /ghost\n>\n> - [r]: /real\n\n'
    + '| Reference | Value |\n| - | - |\n| [fake]: /ghost | [r] |');
  assert.equal((html.match(/href="\/real"/g) || []).length, 2);
  assert.doesNotMatch(html, /href="\/ghost"/);
  assert.match(html, /callout-title">\[fake\]: \/ghost/);
  assert.match(html, /<td>\[fake\]: \/ghost<\/td>/);
});

test('definitions consume no render nodes and cannot manufacture a setext heading', () => {
  assert.equal(render('[r]: /real\n===\n[r]'), '<p>===\n<a href="/real">r</a></p>\n');
  assert.equal(render('[r]: /real\n---\n[r]'), '<hr>\n<p><a href="/real">r</a></p>\n');
  assert.equal(render('[r]: /real\nTitle\n===\n[r]'), '<h1 id="title">Title</h1>\n<p><a href="/real">r</a></p>\n');
});

test('definitions following closed code, headings and rules are collected at block starts', () => {
  for (const prefix of ['```\ncode\n```\n', '# Heading\n', '---\n']) {
    assert.match(render('[r]\n\n' + prefix + '[r]: /real'), /href="\/real"/);
  }
});

test('first-line nested footnote definitions are resolved once with repeated backlinks', () => {
  for (const [name, wrap] of containers) {
    const html = render('One[^n], two[^n].\n\n' + wrap('[^n]: **Note**'));
    assert.equal((html.match(/id="fn-1"/g) || []).length, 1, name);
    assert.match(html, /<strong>Note<\/strong>/, name);
    assert.match(html, /href="#fnref-1"/, name);
    assert.match(html, /href="#fnref-1-2"/, name);
  }
});

test('nested footnotes retain multiple paragraphs, code and reference links', () => {
  const html = render('Note[^n].\n\n- [^n]: first [guide]\n\n      second paragraph\n\n'
    + '      ```\n      [fake]: /ghost\n      ```\n\n> - [guide]: /manual\n\n[fake]');
  assert.match(html, /<p>first <a href="\/manual">guide<\/a><\/p>/);
  assert.match(html, /<p>second paragraph<\/p>/);
  assert.match(html, /<pre><code>\[fake\]: \/ghost/);
  assert.doesNotMatch(html, /href="\/ghost"/);
});

test('references inside unused notes are collected without rendering their visible content', () => {
  const html = render('[r]\n\n- [^unused]: hidden text\n\n      > - [r]: /manual');
  assert.match(html, /href="\/manual"/);
  assert.doesNotMatch(html, /hidden text|class="footnotes"/);
});

test('nested note cycles terminate and late references get matching backlinks', () => {
  const html = render('Note[^a].\n\n- [^a]: A[^b]\n\n      - [^b]: B[^a]');
  assert.equal((html.match(/<li id="fn-/g) || []).length, 2);
  const ids = [...html.matchAll(/ id="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const match of html.matchAll(/href="#([^"]+)"/g)) assert.ok(ids.includes(match[1]), match[1]);
  assert.match(html, /href="#fnref-1-2"/);
});

test('a duplicate nested note cannot hijack its earlier containing note', () => {
  const html = render('Note[^n].\n\n[^n]: original\n\n    - [^n]: duplicate\n\n[^n]: also duplicate');
  assert.match(html, /<p>original<\/p>/);
  assert.doesNotMatch(html, /duplicate/);
  assert.equal((html.match(/<li id="fn-/g) || []).length, 1);
});

test('definition collection does not consume heading or footnote render identities early', () => {
  const html = render('# fn-1\n\n# [Guide][r]\n\nnote[^n]\n\n- [r]: /manual\n- [^n]: # Guide');
  const ids = [...html.matchAll(/ id="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  assert.match(html, /id="guide"><a href="\/manual">Guide<\/a>/);
  assert.match(html, /id="guide-1">Guide/);
  for (const match of html.matchAll(/href="#([^"]+)"/g)) assert.ok(ids.includes(match[1]));
});

test('nested definitions still enforce link and image URL admission', () => {
  for (const uri of ['javascript:alert(1)', 'javascript&#58;alert(1)', 'file:///secret', 'data:text/html,evil']) {
    const html = render('[r] ![image][r]\n\n> - [r]: ' + uri);
    assert.doesNotMatch(html, /<(?:a|img)\b/);
  }
});

test('definitions beyond the depth fallback do not leak into document state', () => {
  const html = render('[r]\n\n' + '> '.repeat(200) + '[r]: /hidden');
  assert.match(html, /<pre>/);
  assert.doesNotMatch(html, /href="\/hidden"/);
});

test('the new block tree shares the deterministic structure budget and resets after failure', () => {
  const bounded = vm.createContext({input: '- item\n'.repeat(90000)});
  vm.runInContext(code, bounded);
  assert.throws(() => vm.runInContext('parseMarkdownClient(input)', bounded, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'structure');
  assert.equal(vm.runInContext('parseMarkdownClient("[r]")', bounded), '<p>[r]</p>\n');
});

test('multiline definition labels normalize line breaks and whitespace', () => {
  assert.equal(render('[A B]\n\n[a\n b]: /manual'), '<p><a href="/manual">A B</a></p>\n');
  assert.equal(render('[A B]\r\n\r\n[a\r\n b]: /manual'), '<p><a href="/manual">A B</a></p>\n');
});

test('destinations and all three title delimiters may occupy following lines', () => {
  for (const [open, close] of [['"', '"'], ["'", "'"], ['(', ')']]) {
    const html = render('[r]\n\n[r]:\n  /manual\n  ' + open + 'Read' + close);
    assert.equal(html, '<p><a href="/manual" title="Read">r</a></p>\n');
  }
});

test('multiline definition syntax works at every supported container depth', () => {
  for (const [name, wrap] of containers) {
    const html = render('[A B]\n\n' + wrap('[a\nb]:\n  /manual\n  "Read"'));
    assert.match(html, /<a href="\/manual" title="Read">A B<\/a>/, name);
    assert.doesNotMatch(html, /\]:|&quot;Read/, name);
  }
});

test('multiline titles preserve content and escape attributes exactly once', () => {
  const html = render('[r]\n\n[r]: /manual "Read &amp; learn\nmore \\"quotes\\""');
  assert.equal(html, '<p><a href="/manual" title="Read &amp; learn\nmore &quot;quotes&quot;">r</a></p>\n');
});

test('empty angle destinations resolve links and reference images', () => {
  assert.equal(render('[r] ![image][r]\n\n[r]: <> "Empty"'),
    '<p><a href="" title="Empty">r</a> <img src="" alt="image" title="Empty"></p>\n');
});

test('balanced and escaped parentheses remain part of the destination', () => {
  for (const destination of ['/route_(v2)', '</route_(v2)>', '/route_\\(v2\\)']) {
    assert.equal(render('[r]\n\n[r]: ' + destination), '<p><a href="/route_(v2)">r</a></p>\n');
  }
});

test('escaped closing brackets in labels resolve through the same raw label identity', () => {
  assert.equal(render('[a\\]]\n\n[a\\]]: /manual'), '<p><a href="/manual">a]</a></p>\n');
  assert.equal(render('[a\\[b]\n\n[a\\[b]: /manual'), '<p><a href="/manual">a[b</a></p>\n');
});

test('first matching multiline definition wins over later duplicates', () => {
  const html = render('[a b]\n\n- [a\n  b]:\n    /first\n    "First"\n\n[a b]: /later "Later"');
  assert.match(html, /href="\/first" title="First"/);
  assert.doesNotMatch(html, /later|Later/);
});

test('malformed same-line titles do not consume source or publish a definition', () => {
  for (const suffix of ['"unclosed', '(bad (nested))', '"closed" trailing', 'garbage']) {
    const html = render('[r]\n\n[r]: /manual ' + suffix);
    assert.doesNotMatch(html, /href="\/manual"/);
    assert.match(html, /\[r\]: \/manual/);
  }
});

test('malformed next-line titles retain the valid URL and the untouched following paragraph', () => {
  for (const title of ['"unclosed', '(bad (nested))', '"closed" trailing']) {
    const html = render('[r]\n\n[r]: /manual\n' + title + '\n\nAfter');
    assert.match(html, /href="\/manual"/);
    assert.match(html, /<p>After<\/p>/);
    assert.ok(html.includes(title.replaceAll('"', '&quot;')), html);
  }
});

test('blank lines cannot join a label, destination or title candidate', () => {
  for (const definition of ['[a\n\nb]: /manual', '[r]:\n\n/manual', '[r]: /manual "before\n\nafter"']) {
    assert.doesNotMatch(render('[r] [a b]\n\n' + definition), /<a\b/);
  }
  const html = render('[r]: /manual\n\n"not a title"\n\n[r]');
  assert.match(html, /<p>&quot;not a title&quot;<\/p>/);
  assert.match(html, /<a href="\/manual">r<\/a>/);
});

test('malformed definitions cannot swallow following headings, lists or code blocks', () => {
  const html = render('[r]\n\n[r]:\n\n# Heading\n\n- item\n\n```\ncode\n```');
  assert.doesNotMatch(html, /<a\b/);
  assert.match(html, /<h1 id="heading">Heading<\/h1>/);
  assert.match(html, /<li>item<\/li>/);
  assert.match(html, /<pre><code>code\n<\/code><\/pre>/);
});

test('same-line titles require separating whitespace after angle destinations', () => {
  assert.doesNotMatch(render('[r]\n\n[r]: </manual>"title"'), /<a\b/);
  assert.match(render('[r]\n\n[r]: </manual> "title"'), /href="\/manual" title="title"/);
});

test('unbalanced destinations and raw parentheses inside parenthesized titles are rejected', () => {
  for (const definition of ['/route_(v2', '/route_v2)', '<nested<url>>', '/manual (bad (nest))']) {
    assert.doesNotMatch(render('[r]\n\n[r]: ' + definition), /<a\b/);
  }
  assert.match(render('[r]\n\n[r]: /manual (escaped \\(nest\\))'), /title="escaped \(nest\)"/);
});

test('reference labels enforce their 999-character limit without truncation', () => {
  for (const char of ['a', '😀']) {
    const good = char.repeat(999), bad = char.repeat(1000);
    assert.match(render('[visible][' + good + ']\n\n[' + good + ']: /manual'), /href="\/manual"/);
    assert.doesNotMatch(render('[visible][' + bad + ']\n\n[' + bad + ']: /manual'), /<a\b/);
  }
});

test('empty labels and unescaped nested brackets never register definitions', () => {
  for (const label of ['', '   ', 'a[b']) {
    assert.doesNotMatch(render('[visible][' + label + ']\n\n[' + label + ']: /manual'), /<a\b/);
  }
});

test('multiline reference resources still use URL safety and explicit image bindings', () => {
  assert.doesNotMatch(render('[r] ![image][r]\n\n[r]:\n  javascript&#58;alert(1)\n  "blocked"'), /<(?:a|img)\b/);
  const assets = new Map([['asset.png', 'data:image/png;base64,AAAA']]);
  const html = context.parseMarkdownClient('![image][r]\n\n[r]:\n  asset.png\n  "Embedded"', assets);
  assert.match(html, /src="data:image\/png;base64,AAAA" alt="image" title="Embedded"/);
});

test('valid multiline definitions disappear even if unused', () => {
  assert.equal(render('[unused]:\n  /manual\n  "Read\nmore"\n\nvisible'), '<p>visible</p>\n');
});

test('multiline definitions work inside unused and referenced footnote bodies', () => {
  const html = render('[r] note[^n]\n\n[^unused]:\n    [r]:\n      /manual\n      "Read"\n\n[^n]: [r]');
  assert.equal((html.match(/href="\/manual" title="Read"/g) || []).length, 2);
  assert.equal((html.match(/<li id="fn-/g) || []).length, 1);
});

test('an oversized unterminated label leaves later independent definitions usable', () => {
  const html = render('[' + 'x'.repeat(2000) + '\n\n[r]: /manual\n\n[r]');
  assert.match(html, /href="\/manual"/);
  assert.match(html, /\[xxx/);
});
