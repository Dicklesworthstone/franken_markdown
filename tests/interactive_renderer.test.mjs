// Run with: node --test tests/interactive_renderer.test.mjs
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
const source = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(source, context);
const render = md => context.parseMarkdownClient(md);

test('paragraphs preserve soft lines, source hard breaks, escapes and entities', () => {
  assert.equal(render('one\ntwo\n\nthree  \nfour\\\nfive'), '<p>one\ntwo</p>\n<p>three<br>\nfour<br>\nfive</p>\n');
  assert.equal(render('one\\\\\ntwo\n\n&#92;\n&#32;&#32;\nend'), '<p>one\\\ntwo</p>\n<p>\\\n  \nend</p>\n');
  assert.equal(render('one\r\ntwo\rthree\0'), '<p>one\ntwo\nthree\ufffd</p>\n');
});

test('code spans shield formatting, links and markup, with arbitrary backtick runs', () => {
  assert.equal(render('`**literal** [x](https://example.com) <img>`'), '<p><code>**literal** [x](https://example.com) &lt;img&gt;</code></p>\n');
  assert.equal(render('`` `one` `` and ` two `'), '<p><code>`one`</code> and <code>two</code></p>\n');
  assert.equal(render('a `multi\nline` b'), '<p>a <code>multi line</code> b</p>\n');
});

test('emphasis, strike, escapes and intraword underscores', () => {
  assert.equal(render('***both*** **strong** _em_ ~~old~~ \\*literal\\* a_b_c'), '<p><strong><em>both</em></strong> <strong>strong</strong> <em>em</em> <del>old</del> *literal* a_b_c</p>\n');
});

test('all heading levels, setext headings and unique Unicode anchors', () => {
  const html = render('# Café\n\n# Café\n\n## Café-1\n\n# Café\n\n#### Four\n\n##### Five\n\n###### Six\n\nTitle\n=====\n\nSub\n---');
  for (const id of ['café', 'café-1', 'café-1-1', 'café-2', 'four', 'five', 'six', 'title', 'sub']) assert.match(html, new RegExp('id="' + id + '"'));
  assert.match(html, /<h6 id="six">Six<\/h6>/);
});

test('heading hashes without separating whitespace remain text', () => {
  assert.equal(render('# word#\n\n## word ##'), '<h1 id="word">word#</h1>\n<h2 id="word-1">word</h2>\n');
});

test('ordered lists preserve start numbers and nested lists', () => {
  assert.equal(render('3. first\n4. second\n   - inner\n     7. deep\n   - again\n5. third'), '<ol start="3">\n<li>first</li>\n<li>second\n<ul>\n<li>inner\n<ol start="7">\n<li>deep</li>\n</ol></li>\n<li>again</li>\n</ul></li>\n<li>third</li>\n</ol>\n');
});

test('loose lists retain continuation paragraphs, fences and task state', () => {
  const html = render('- [x] first\n\n  another paragraph\n\n  ```txt\n  <literal>\n  ```\n\n- [ ] second');
  assert.match(html, /<li class="task"><p><input type="checkbox" disabled checked> first<\/p>/);
  assert.match(html, /<p>another paragraph<\/p>/);
  assert.match(html, /<code class="language-txt">&lt;literal&gt;\n<\/code>/);
  assert.match(html, /<li class="task"><p><input type="checkbox" disabled> second<\/p>/);
});

test('tables align columns, honor escaped pipes and normalize ragged rows', () => {
  const html = render('| Left | Center | Right |\n| :-- | :-: | --: |\n| a\\|b | `c\\|d` | e |\n| short |\n| one | two | three | ignored |');
  assert.match(html, /role="region" aria-label="Markdown table" tabindex="0"/);
  assert.match(html, /<th style="text-align:center">Center<\/th>/);
  assert.match(html, /<td style="text-align:left">a\|b<\/td>/);
  assert.match(html, /<td style="text-align:right"><\/td>/);
  assert.match(html, /<code>c\|d<\/code>/);
  assert.doesNotMatch(html, /ignored/);
  assert.equal((html.match(/<td\b/g) || []).length, 9);
});

test('invalid table delimiters do not swallow ordinary paragraphs', () => {
  assert.equal(render('one | two\n:-- | oops\nrest'), '<p>one | two\n:-- | oops\nrest</p>\n');
});

test('both fences obey opener length, indentation and language-token boundaries', () => {
  const html = render('  ````rust extra\n  ```\n  <unsafe>\n  ````\n\n~~~js\na\n~~~~\n\n```txt\nunfinished');
  assert.match(html, /<code class="language-rust">```\n&lt;unsafe&gt;\n<\/code>/);
  assert.match(html, /<code class="language-js">a\n<\/code>/);
  assert.match(html, /<code class="language-txt">unfinished\n<\/code>/);
  assert.doesNotMatch(html, /language-rust extra/);
});

test('indented code and all rule markers remain distinct from lists', () => {
  assert.equal(render('    <tag>\n    **code**\n\n***\n\n- - -\n\n___'), '<pre><code>&lt;tag&gt;\n**code**\n</code></pre>\n<hr>\n<hr>\n<hr>\n');
});

test('nested quotes and every callout kind support full block content', () => {
  const html = render('> [!WARNING] **Careful**\n>\n> 2. ordered\n>    - nested\n>\n> > quoted');
  assert.match(html, /<aside class="callout callout-warning"><p class="callout-title"><strong>Careful<\/strong><\/p>/);
  assert.match(html, /<ol start="2">/);
  assert.match(html, /<blockquote>\n<p>quoted<\/p>/);
  for (const kind of ['NOTE', 'TIP', 'IMPORTANT', 'WARNING', 'CAUTION']) assert.match(render('> [!' + kind + ']\n> body'), new RegExp('callout-' + kind.toLowerCase()));
});

test('links and images support titles, nested parentheses, relative URLs and formatted labels', () => {
  const html = render('[**guide**](guide_(v2).html "Read & learn") ![a *diagram*](../img.png \'Picture\') [mail](mailto:a@example.com)');
  assert.match(html, /<a href="guide_\(v2\).html" title="Read &amp; learn"><strong>guide<\/strong><\/a>/);
  assert.match(html, /<img src="\.\.\/img.png" alt="a diagram" title="Picture">/);
  assert.match(html, /href="mailto:a@example.com"/);
});

test('forward, collapsed and shortcut references share normalized document definitions', () => {
  const html = render('[full][ A  B ] [A B][] [A B] ![picture][A B]\n\n[A b]: /asset.png "Asset"\n[a b]: /ignored.png');
  assert.equal((html.match(/href="\/asset.png"/g) || []).length, 3);
  assert.match(html, /<img src="\/asset.png" alt="picture" title="Asset">/);
  assert.doesNotMatch(html, /ignored|\[A b\]:/);
});

test('definitions in fences and indented code do not become active links', () => {
  const html = render('[real] [fake] [also]\n\n[real]: /real\n\n```\n[fake]: /fake\n```\n\n    [also]: /also');
  assert.match(html, /href="\/real"/);
  assert.doesNotMatch(html, /href="\/(?:fake|also)"/);
});

test('footnotes resolve forward references once and provide unique repeated backlinks', () => {
  const html = render('One[^n], two[^n]. Unknown[^missing].\n\n> Also[^n].\n\n[^n]: **Important** [guide][g]\n\n    Second paragraph.\n\n[g]: /guide');
  assert.match(html, /id="fnref-1"/);
  assert.match(html, /id="fnref-1-2"/);
  assert.match(html, /id="fnref-1-3"/);
  assert.equal((html.match(/id="fn-1"/g) || []).length, 1);
  for (const id of ['fnref-1', 'fnref-1-2', 'fnref-1-3']) assert.match(html, new RegExp('href="#' + id + '"'));
  assert.match(html, /<strong>Important<\/strong> <a href="\/guide">guide<\/a>/);
  assert.match(html, /<p>Second paragraph\.<\/p>/);
  assert.match(html, /Unknown\[\^missing\]/);
});

test('unused and cyclic footnotes neither leak definitions nor recurse forever', () => {
  assert.equal(render('text\n\n[^unused]: hidden'), '<p>text</p>\n');
  const html = render('ref[^cycle]\n\n[^cycle]: recursive[^cycle]');
  assert.equal((html.match(/id="fn-1"/g) || []).length, 1);
  assert.match(html, /recursive<sup/);
  assert.match(html, /href="#fnref-1-2"/);
});

test('state never leaks between renders after deleting or changing definitions', () => {
  assert.match(render('[a][d] ref[^n]\n\n[d]: /ok\n[^n]: note'), /href="\/ok"/);
  assert.equal(render('[a][d] ref[^n]'), '<p>[a][d] ref[^n]</p>\n');
  assert.equal(render('# Same'), render('# Same'));
});

test('active URL schemes and obfuscated protocols cannot become links or images', () => {
  for (const url of ['javascript:alert(1)', 'JaVaScRiPt:alert(1)', 'javascript&#58;alert(1)', 'java&#9;script:alert(1)', 'data:text/html,evil', 'vbscript:msgbox(1)', 'file:///etc/passwd']) {
    const html = render('[x](' + url + ') ![alt](' + url + ')\n\n[ref][d]\n\n[d]: ' + url);
    assert.doesNotMatch(html, /<(?:a|img)\b/, url + '\n' + html);
  }
});

test('raw tags, attributes and hostile footnote labels remain inert text', () => {
  const html = render('<img src=x onerror=alert(1)>\n\n[x](https://e.test/\"onmouseover=\"bad)\n\n![' + '" onerror="bad' + '](image.png)\n\nref[^\"><img>]\n\n[^\"><img>]: note');
  assert.doesNotMatch(html, /<img src=x| onmouseover="| onerror="/);
  assert.match(html, /&lt;img src=x/);
  assert.doesNotMatch(html, /id="[^\"]*<img/);
});

test('deep containers have a bounded fallback instead of overflowing the stack', () => {
  assert.doesNotThrow(() => render('> '.repeat(200) + 'end'));
});


test('nested notes expand once, including cycles and backlinks from later notes', () => {
  const html = render('first[^a] second[^b]\n\n[^a]: A to B[^b].\n[^b]: B to A[^a].');
  assert.equal((html.match(/id="fn-1"/g) || []).length, 1);
  assert.equal((html.match(/id="fn-2"/g) || []).length, 1);
  for (const id of ['fnref-1', 'fnref-1-2', 'fnref-2', 'fnref-2-2']) assert.match(html, new RegExp('href="#' + id + '"'));
});

test('heading and footnote IDs cannot collide', () => {
  const html = render('# fn-1\n\n# fnref-1\n\nText[^n].\n\n[^n]: note');
  const ids = [...html.matchAll(/ id="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const match of html.matchAll(/href="#([^"]+)"/g)) assert.ok(ids.includes(match[1]));
});
