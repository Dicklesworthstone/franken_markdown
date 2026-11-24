import './interactive_toc.test.mjs';
import './interactive_empty_list_items.test.mjs';
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
  // CommonMark example 467 and native rendering put strong inside emphasis.
  assert.equal(render('***both*** **strong** _em_ ~~old~~ \\*literal\\* a_b_c'), '<p><em><strong>both</strong></em> <strong>strong</strong> <em>em</em> <del>old</del> *literal* a_b_c</p>\n');
});

test('all heading levels, setext headings and unique Unicode anchors', () => {
  const html = render('# Café\n\n# Café\n\n## Café-1\n\n# Café\n\n#### Four\n\n##### Five\n\n###### Six\n\nTitle\n=====\n\nSub\n---');
  for (const id of ['café', 'café-1', 'café-2', 'café-3', 'four', 'five', 'six', 'title', 'sub']) assert.match(html, new RegExp('id="' + id + '"'));
  assert.match(html, /<h6 id="six">Six<\/h6>/);
});

test('heading hashes without separating whitespace remain text', () => {
  assert.equal(render('# word#\n\n## word ##'), '<h1 id="word">word#</h1>\n<h2 id="word-2">word</h2>\n');
});

test('offline heading fragments follow native separators, scalar case and duplicate allocation', () => {
  const html = render('[jump](#api-reference)\n\n# API_reference\n\n# API--reference\n\n# API reference-2\n\n# _API reference_\n\n# ΟΣ\u00a0中٢42');
  assert.match(html, /<a href="#api-reference">jump<\/a>/);
  assert.deepEqual([...html.matchAll(/<h1 id="([^"]+)"/g)].map(match => match[1]),
    ['api-reference', 'api-reference-2', 'api-reference-2-2', 'api-reference-3', 'οσ中42']);
});

test('heading IDs use image descriptions, link labels, code and literal HTML ownership', () => {
  const source = '# [![CPU **Chart**](plot.png)](/target "Title")<!-- hidden --> <b>today</b>\n\n'
    + '> ![CPU **Chart**](blocked:plot) today\n> ===\n\n'
    + '# `<b>` &amp; &#65; <custom>\n\n'
    + '# [Résultat](guide.md) and $x_1$';
  const html = render(source);
  assert.deepEqual([...html.matchAll(/<h1 id="([^"]+)"/g)].map(match => match[1]),
    ['cpu-chart-today', 'cpu-chart-today-2', 'b-a-custom', 'résultat-and-x-1']);
  assert.match(html, /alt="CPU Chart"/);
  assert.match(html, /&lt;!-- hidden --&gt; &lt;b&gt;today&lt;\/b&gt;/);
});

test('footnotes reserve the same heading anchor before and after editing', () => {
  const html = render('Read[^name].\n\n# fn_![name](plot.svg)[^name]\n\n[^name]: note');
  assert.match(html, /<h1 id="fn-name">/);
  assert.match(html, /<li id="fn-name-2">/);
  assert.equal((html.match(/href="#fn-name-2" role="doc-noteref"/g) || []).length, 2);
});

test('CommonMark 79 empty ATX headings and literal Unicode edge whitespace', () => {
  assert.equal(render('## \n#\n### ###\n'), '<h2 id="section"></h2>\n<h1 id="section-2"></h1>\n<h3 id="section-3"></h3>\n');
  assert.equal(render('# \u00a0Text\u00a0 ###'), '<h1 id="text">\u00a0Text\u00a0</h1>\n');
});

test('ordered lists preserve start numbers and nested lists', () => {
  // Upstream cmark keeps a non-1 marker inside the item's open paragraph.
  // At a real block boundary, that same start number creates a nested list.
  assert.equal(render('3. first\n4. second\n   - inner\n     7. deep\n   - again\n5. third'), '<ol start="3">\n<li>first</li>\n<li>second\n<ul>\n<li>inner\n7. deep</li>\n<li>again</li>\n</ul></li>\n<li>third</li>\n</ol>\n');
  assert.equal(render('3. first\n4. second\n   - 7. deep\n   - again\n5. third'), '<ol start="3">\n<li>first</li>\n<li>second\n<ul>\n<li><ol start="7">\n<li>deep</li>\n</ol></li>\n<li>again</li>\n</ul></li>\n<li>third</li>\n</ol>\n');
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
  assert.equal((html.match(/id="fn-n"/g) || []).length, 1);
  for (const id of ['fnref-1', 'fnref-1-2', 'fnref-1-3']) assert.match(html, new RegExp('href="#' + id + '"'));
  assert.match(html, /<strong>Important<\/strong> <a href="\/guide">guide<\/a>/);
  assert.match(html, /<p>Second paragraph\.<\/p>/);
  assert.match(html, /Unknown\[\^missing\]/);
});

test('unused and cyclic footnotes neither leak definitions nor recurse forever', () => {
  assert.equal(render('text\n\n[^unused]: hidden'), '<p>text</p>\n');
  const html = render('ref[^cycle]\n\n[^cycle]: recursive[^cycle]');
  assert.equal((html.match(/id="fn-cycle"/g) || []).length, 1);
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
  const html = render('<img src=x onerror=alert(1)>\n\n[x](https://e.test/"onmouseover="bad)\n\n![' + '" onerror="bad' + '](image.png)\n\nref[^\"><img>]\n\n[^\"><img>]: note');
  assert.doesNotMatch(html, /<img src=x| onmouseover="| onerror="/);
  assert.match(html, /&lt;img src=x/);
  assert.doesNotMatch(html, /id="[^\"]*<img/);
});

test('deep containers have a bounded fallback instead of overflowing the stack', () => {
  assert.doesNotThrow(() => render('> '.repeat(200) + 'end'));
});


test('nested notes expand once, including cycles and backlinks from later notes', () => {
  const html = render('first[^a] second[^b]\n\n[^a]: A to B[^b].\n[^b]: B to A[^a].');
  assert.equal((html.match(/id="fn-a"/g) || []).length, 1);
  assert.equal((html.match(/id="fn-b"/g) || []).length, 1);
  for (const id of ['fnref-1', 'fnref-1-2', 'fnref-2', 'fnref-2-2']) assert.match(html, new RegExp('href="#' + id + '"'));
});

test('heading and footnote IDs cannot collide', () => {
  const html = render('# fn-n\n\n# fnref-1\n\nText[^n].\n\n[^n]: note');
  const ids = [...html.matchAll(/ id="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const match of html.matchAll(/href="#([^"]+)"/g)) assert.ok(ids.includes(match[1]));
});

// Shared resource-budget regressions for the real standalone renderer.
{
  const source = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
  const context = vm.createContext({});
  vm.runInContext(source, context);
  const limits = vm.runInContext('FMD_CLIENT_RENDER_LIMITS', context);
  const renderScript = new vm.Script('parseMarkdownClient(input, assets)');
  function render(input, assets) {
    context.input = input;
    context.assets = assets;
    // This is only a hang guard. The admission assertions below require the
    // renderer's own typed failure, never a timeout or a generic RangeError.
    return renderScript.runInContext(context, {timeout: 5000});
  }
  function limited(limit, action) {
    assert.throws(action, error => error.code === 'FMD_RENDER_LIMIT'
      && error.limit === limit && /unchanged.*downloadable/.test(error.message));
  }

  test('large unmatched brackets render exactly rather than rescanning every suffix', () => {
    const text = '['.repeat(50000);
    assert.equal(render(text), '<p>' + text + '</p>\n');
  });

  test('long interior spaces are consumed once and preserve exact literal content', () => {
    const text = 'a' + ' '.repeat(100000) + 'b';
    assert.equal(render(text), '<p>' + text + '</p>\n');
  });

  test('long whitespace before a newline still makes one source hard break', () => {
    assert.equal(render('a' + ' '.repeat(100000) + '\nb'), '<p>a<br>\nb</p>\n');
  });

  test('ATX headings avoid backtracking without changing trailing hash semantics', () => {
    const gap = ' '.repeat(100000);
    assert.equal(render('# x' + gap + 'z'), '<h1 id="x-z">x' + gap + 'z</h1>\n');
    assert.equal(render('# x' + gap + '### \t'), '<h1 id="x">x</h1>\n');
    assert.equal(render('# x###\n\n# ###'), '<h1 id="x">x###</h1>\n<h1 id="section"></h1>\n');
  });

  test('indexed brackets preserve nested pairs and escaped opening/closing delimiters', () => {
    assert.equal(render(String.raw`[outer \[literal\] [inner]](/ok_(v2))`),
      '<p><a href="/ok_(v2)">outer [literal] [inner]</a></p>\n');
    assert.equal(render(String.raw`[label](/a\(b\))`), '<p><a href="/a(b)">label</a></p>\n');
    assert.equal(render(String.raw`\[literal] [yes](/ok)`), '<p>[literal] <a href="/ok">yes</a></p>\n');
    assert.equal(render('`[x](/hidden)` [x](/visible)'), '<p><code>[x](/hidden)</code> <a href="/visible">x</a></p>\n');
  });

  test('many short links are admitted without charging their entire remaining tails', () => {
    const count = 6000;
    // CommonMark 645 discards final paragraph spaces; all 6000 links and
    // their 5999 interior separators must still survive.
    assert.equal(render('[x](/ok) '.repeat(count)), '<p>' + '<a href="/ok">x</a> '.repeat(count).slice(0, -1) + '</p>\n');
  });

  test('dotted invalid email autolinks do not trigger regex backtracking', () => {
    const label = 'a@' + '.'.repeat(100000) + '@';
    assert.equal(render('<' + label + '>'), '<p>&lt;' + label + '&gt;</p>\n');
    // CommonMark angle-email domains admit one label, but no empty labels.
    // Keep the hostile-input assertion above and both positive/negative controls.
    for (const label of ['a@b.c', 'a@b', 'a@localhost', 'a@b-c', 'a.b@c.d']) {
      assert.equal(render('<' + label + '>'), '<p><a href="mailto:' + label + '">' + label + '</a></p>\n');
    }
    for (const label of ['a@.b', 'a@b.', 'a@b..', 'a@..b', '@b.c', 'a@@b.c', 'a @b.c']) {
      assert.equal(render('<' + label + '>'), '<p>&lt;' + label + '&gt;</p>\n');
    }
  });

  test('source admission runs before resource lookup and any parsing', () => {
    const assets = {has() { assert.fail('source admission must precede resource lookup'); }};
    limited('source', () => render('x'.repeat(limits.sourceUnits + 1), assets));
  });

  test('line-array admission accounts for LF, CR and CRLF before allocation', () => {
    for (const newline of ['\n', '\r', '\r\n']) {
      limited('structure', () => render(newline.repeat(limits.structures)));
    }
    assert.equal(render('\r\n'.repeat(10000)), '');
  });

  test('delimiter index allocation has an explicit bound', () => {
    limited('structure', () => render('['.repeat(limits.structures)));
  });

  test('table-cell allocation cannot multiply unchecked across measurement and emission', () => {
    limited('structure', () => render('|' + 'h|'.repeat(90000) + '\n|' + '-|'.repeat(90000)));
  });

  test('repeated unmatched autolink searches stop at the shared work budget', () => {
    limited('work', () => render('<'.repeat(10000)));
  });

  test('unmatched emphasis completes within budget while expensive reference probes remain bounded', () => {
    // A delimiter stack consumes this once; the old repeated suffix search
    // exhausted work. Preserve every unmatched run rather than rejecting it.
    const text = 'start ' + '_x '.repeat(7000);
    assert.equal(render(text), '<p>' + text.slice(0, -1) + '</p>\n');
    limited('work', () => render('['.repeat(20000) + ']'.repeat(20000)));
  });

  test('blank-list runs are scanned once without losing loose-list semantics', () => {
    assert.equal(render('- a\n' + ' \n'.repeat(10000) + '- b'),
      '<ul>\n<li><p>a</p></li>\n<li><p>b</p></li>\n</ul>\n');
  });

  test('large blockquotes do not spread their lines onto the JavaScript call stack', () => {
    const count = 130000;
    assert.equal(render('> x\n'.repeat(count)), '<blockquote>\n<p>' + 'x\n'.repeat(count - 1)
      + 'x</p>\n</blockquote>\n');
  });

  test('oversized bound images are rejected before data-URI validation or output copying', () => {
    const value = 'data:image/png;base64,' + 'AAAA'.repeat(limits.outputUnits / 4);
    const assets = new Map([['local.png', value]]);
    limited('output', () => render('![x](local.png)', assets));
    assert.equal(assets.get('local.png'), value);
  });

  test('reference expansion is bounded even when Markdown source is small', () => {
    const url = '/' + 'a'.repeat(100000);
    limited('work', () => render('[x][r] '.repeat(200) + '\n\n[r]: ' + url));
  });

  test('footnotes still expand once with every published backlink', () => {
    const text = render('ref[^n] '.repeat(1800) + '\n\n[^n]: body');
    assert.equal((text.match(/<li id="fn-n">/g) || []).length, 1);
    assert.equal((text.match(/class="footnote-backref"/g) || []).length, 1800);
  });

  test('a failed request does not retain budget, heading IDs, resources or definitions', () => {
    limited('work', () => render('# Same\n\n' + '<'.repeat(10000)));
    assert.equal(render('# Same\n\n[x][missing]'), '<h1 id="same">Same</h1>\n<p>[x][missing]</p>\n');
    assert.equal(render('# Same'), '<h1 id="same">Same</h1>\n');
    assert.equal(render('![x](local.png)', new Map([['local.png', 'data:image/png;base64,AAAA']])),
      '<p><img src="data:image/png;base64,AAAA" alt="x"></p>\n');
  });
}

// Keep container/reference coverage on the established CI entry point.
import './interactive_definitions.test.mjs';
import './interactive_reference_targets.test.mjs';
import './interactive_reference_prefixes.test.mjs';
import './interactive_autolink_unicode.test.mjs';
import './interactive_block_payloads.test.mjs';

// Source editing runs through the same self-contained workspace asset bundle.
import './interactive_search.test.mjs';
