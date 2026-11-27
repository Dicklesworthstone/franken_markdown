// Native authoring grammar contracts exercised through the production
// dependency-free renderer. HTML/math remain its documented inert subset;
// selecting a dialect does not authorize arbitrary HTML or promise typesetting.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, profile, assets) => context.parseMarkdownClient(source, assets, profile);
const dl = inner => '<dl>\n' + inner + '</dl>\n';
const pair = (term, body) => '<dt>' + term + '</dt>\n<dd>' + body + '</dd>\n';
const pagebreak = '<div class="fmd-pagebreak" style="break-after: page;"></div>\n';
const escape = text => text.replace(/[&<>"']/g, ch => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[ch]);
const literal = text => '<p>' + escape(text) + '</p>\n';
const plusProfiles = [undefined, null, 'gfm-plus'];

test('omitted, null and gfm-plus enable the native default extensions', () => {
  const source = 'Term\n: Definition\n\n<!-- pagebreak -->\n\nAfter';
  const expected = dl(pair('Term', 'Definition')) + pagebreak + '<p>After</p>\n';
  for (const profile of plusProfiles) assert.equal(render(source, profile), expected);
  assert.equal(render(source, 'commonmark-gfm'),
    '<p>Term\n: Definition</p>\n' + literal('<!-- pagebreak -->') + '<p>After</p>\n');
});

test('invalid explicit profiles fail before source coercion or asset access', () => {
  let touched = false;
  const source = {toString() { touched = true; throw Error('source observed'); }};
  const assets = new Proxy({}, {get() { touched = true; throw Error('assets observed'); }});
  const object = {[Symbol.toPrimitive]() { touched = true; throw Error('profile observed'); }};
  for (const profile of ['', 'commonmark', 'gfm', 'GFM-Plus', ' gfm-plus', 'gfm-plus ',
    true, false, 0, 1, NaN, 1n, Symbol('gfm-plus'), [], {}, object, new String('gfm-plus')]) {
    assert.throws(() => render(source, profile, assets),
      error => error.name === 'TypeError' && error.code === 'FMD_INVALID_PROFILE');
  }
  assert.equal(touched, false);
  assert.equal(render('Fresh', 'commonmark-gfm'), '<p>Fresh</p>\n');
});

test('native multi-term and multi-definition items remain one list across blank runs', () => {
  const source = 'Rust\nC++\n: Systems programming language\n: Statically typed language'
    + '\n\n \t\nPython\n: Dynamically typed language\n\n# After';
  const expected = dl('<dt>Rust</dt>\n<dt>C++</dt>\n'
    + '<dd>Systems programming language</dd>\n<dd>Statically typed language</dd>\n'
    + pair('Python', 'Dynamically typed language')) + '<h1 id="after">After</h1>\n';
  for (const profile of plusProfiles) assert.equal(render(source, profile), expected);
});

test('definition markers and term trimming use ASCII indentation and whitespace', () => {
  for (const indent of ['', ' ', '  ', '   ']) {
    for (const marker of [': ', ':\t']) {
      assert.equal(render('   Term \t\n' + indent + marker + 'Definition'), dl(pair('Term', 'Definition')));
    }
  }
  assert.equal(render('\u00a0Term\u00a0\n: Definition'), dl(pair('\u00a0Term\u00a0', 'Definition')));
  for (const colon of ['    : Definition', '\t: Definition', ':Definition', ':\u00a0Definition']) {
    assert.equal(render('Term\n' + colon), '<p>Term\n' + colon.replace(/^[ \t]+/, '') + '</p>\n', colon);
  }
  assert.equal(render(': Definition'), '<p>: Definition</p>\n');
  assert.equal(render('Term\n\n: Definition'), '<p>Term</p>\n<p>: Definition</p>\n');
});

test('plain definitions preserve the first delimiter suffix and join continuation prose', () => {
  assert.equal(render('Term\n: First line\n  second line\n\tthird line'),
    dl(pair('Term', 'First line second line third line')));
  assert.equal(render('Term\n:  padded  \n\nAfter'), dl(pair('Term', ' padded  ')) + '<p>After</p>\n');
  assert.equal(render('Term\n:\n  first\n  second'), dl(pair('Term', 'first second')));
  assert.equal(render('Term\n:\n\nAfter'), dl(pair('Term', '')) + '<p>After</p>\n');
});

test('a new term immediately before a marker ends the previous definition', () => {
  assert.equal(render('Term\n: first\nNext\nAlias\n: second'),
    dl(pair('Term', 'first Next') + pair('Alias', 'second')));
  assert.equal(render('Term\n: first\n\nNext\nAlias\n: second'),
    dl(pair('Term', 'first') + '<dt>Next</dt>\n' + pair('Alias', 'second')));
  assert.equal(render('Term\n===\n: definition'), dl('<dt>Term</dt>\n' + pair('===', 'definition')));
  assert.equal(render('Term\n---\n: definition'), '<h2 id="term">Term</h2>\n<p>: definition</p>\n');
});

test('CommonMark keeps multiline inline delimiters across colon lines', () => {
  // Mirrors tests/parser_authoring_profile.rs before any extension AST exists.
  for (const [source, expected] of [
    ['*Term\n: definition*', '<em>Term\n: definition</em>'],
    ['**Term\n: definition**', '<strong>Term\n: definition</strong>'],
    ['\x60Term\n: definition\x60', '<code>Term : definition</code>'],
    ['[Term\n: definition](/target)', '<a href="/target">Term\n: definition</a>'],
    ['[Term\n: definition][target]\n\n[target]: /target', '<a href="/target">Term\n: definition</a>'],
    ['[Term\n: definition][]\n\n[Term : definition]: /target', '<a href="/target">Term\n: definition</a>'],
  ]) {
    assert.equal(render(source, 'commonmark-gfm'), '<p>' + expected + '</p>\n', source);
    assert.match(render(source, 'gfm-plus'), /^<dl>\n/, source);
  }
  assert.equal(render('*Term\n: definition*', 'gfm-plus'), dl(pair('*Term', 'definition*')));
});

test('the selected inline grammar reaches quotes, lists and rich footnotes', () => {
  const source = '> *Term\n> : definition*\n\n- [Term\n  : definition](/target)\n\nUse[^n].'
    + '\n\n[^n]: \x60Term\n    : definition\x60';
  const common = render(source, 'commonmark-gfm');
  assert.match(common, /<blockquote>\n<p><em>Term\n: definition<\/em><\/p>\n<\/blockquote>/);
  assert.match(common, /<li><a href="\/target">Term\n: definition<\/a><\/li>/);
  assert.match(common, /<li id="fn-n"><p><code>Term : definition<\/code><\/p>/);
  assert.doesNotMatch(common, /<dl>/);
  const extended = render(source, 'gfm-plus');
  assert.equal((extended.match(/<dl>/g) || []).length, 3);
  assert.match(extended, /<dt>\x60Term<\/dt>\n<dd>:? ?definition\x60<\/dd>/);
});

test('definition bodies retain real physical lines for code, hard breaks and destinations', () => {
  for (const [body, expected] of [
    ['Café  \nSecond\\\nThird', 'Café<br>\nSecond<br>\nThird'],
    ['\x60one \ntwo\x60', '<code>one  two</code>'],
    ['prefix\n\x60one \ntwo\x60', 'prefix\n<code>one  two</code>'],
    ['[bad](<docs/\nsecret.md>)', '[bad](&lt;docs/\nsecret.md&gt;)'],
    ['![bad](<docs/\nsecret.png>)', '![bad](&lt;docs/\nsecret.png&gt;)'],
    ['$x % comment\ny$', '$x % comment\ny$'],
    ['before <span title="first\nsecond">after</span>', 'before &lt;span title=&quot;first\nsecond&quot;&gt;after&lt;/span&gt;'],
    ['first &amp;\nsecond', 'first &amp;\nsecond'],
  ]) assert.equal(render('Term\n: ' + body.replace(/\n/g, '\n  ')), dl(pair('Term', expected)), body);
  assert.equal(render('Term\n:\n  **first**  \n  second'),
    dl(pair('Term', '<strong>first</strong><br>\nsecond')));
  assert.equal(render('Term\n: [café](guide.md\n  "Guide") and [bad](<docs/\n  missing.md>) [next](/kept)'),
    dl(pair('Term', '<a href="guide.md" title="Guide">café</a> and [bad](&lt;docs/\nmissing.md&gt;) <a href="/kept">next</a>')));
});

test('reference prefixes and definitions are collected with native block precedence', () => {
  const source = '[r]: /target\nTerm\n: [definition](/literal)\n\n[r]';
  assert.equal(render(source, 'gfm-plus'), dl(pair('Term', '<a href="/literal">definition</a>'))
    + '<p><a href="/target">r</a></p>\n');
  assert.equal(render(source, 'commonmark-gfm'), '<p>Term\n: <a href="/literal">definition</a></p>\n'
    + '<p><a href="/target">r</a></p>\n');
  for (const destination of [':', '===']) assert.equal(render('[term]:\n' + destination + '\n'), '');
  assert.equal(render('[term]: /target\n: An ordinary paragraph.\n\nUse [term].'),
    '<p>: An ordinary paragraph.</p>\n<p>Use <a href="/target">term</a>.</p>\n');
  assert.equal(render('[term]:\n: Definition.'), dl(pair('[term]:', 'Definition.')));
});

test('blank boundaries protect real reference definitions but not literal term lookalikes', () => {
  assert.equal(render('Term\n: first\n\n[r]: /real\n: second\n\n[r]'),
    dl(pair('Term', 'first')) + '<p>: second</p>\n<p><a href="/real">r</a></p>\n');
  assert.equal(render('Term\n: first\n[r]: /real\n: second\n\n[r]'),
    dl(pair('Term', 'first') + pair('[r]: /real', 'second')) + '<p>[r]</p>\n');
  assert.equal(render('Term\n: first\n\n[^n]: body\n: second\n\nUse[^n].'),
    dl(pair('Term', 'first') + pair('[^n]: body', 'second')) + '<p>Use[^n].</p>\n');
});

test('footnote-shaped terms retain source ownership across blank runs and line endings', () => {
  for (const profile of plusProfiles) {
    for (const newline of ['\n', '\r\n', '\r']) {
      for (const id of ['n', 'Café', 'quoted"name', 'a&b']) {
        for (const body of ['', 'body', '/evidence "Title"', '<evidence>', 'several words']) {
          const term = '[^' + id + ']:' + (body ? ' ' + body : '');
          const source = 'Term\n: first\n\n \t\n' + term + '\nAlias\n: second\n\nUse[^' + id + '].';
          assert.equal(render(source.replace(/\n/g, newline), profile),
            dl(pair('Term', 'first') + '<dt>' + escape(term) + '</dt>\n' + pair('Alias', 'second'))
              + '<p>Use[^' + escape(id) + '].</p>\n',
            JSON.stringify({profile, newline, term}));
        }
      }
    }
  }
});

test('nested definition lists keep footnote-shaped terms out of the document note registry', () => {
  const source = 'Term\n: first\n\n[^inner]: body\n: second';
  const expected = dl(pair('Term', 'first') + pair('[^inner]: body', 'second'));
  for (const profile of plusProfiles) {
    for (const container of [
      value => value.split('\n').map(line => '> ' + line).join('\n'),
      value => '- ' + value.replace(/\n/g, '\n  '),
      value => 'See[^outer].\n\n[^outer]: ' + value.replace(/\n/g, '\n    '),
    ]) {
      const html = render(container(source) + '\n\nUse[^inner].', profile);
      assert.ok(html.includes(expected.trimEnd()), html);
      assert.match(html, /<p>Use\[\^inner\]\.<\/p>/);
      assert.doesNotMatch(html, /id="fn-inner"|href="#fn-inner"/);
    }
  }
});

test('real notes and non-note reference labels keep their block-start precedence', () => {
  for (const profile of plusProfiles) {
    const html = render('Term\n: first\n\n[^n]: body\n\nUse[^n].', profile);
    assert.ok(html.startsWith(dl(pair('Term', 'first'))));
    assert.match(html, /href="#fn-n" role="doc-noteref"/);
    assert.match(html, /<li id="fn-n"><p>body<\/p>/);
    assert.equal(render('Term\n: first\n\n[^has space]: /real\n: second\n\n[^has space]', profile),
      dl(pair('Term', 'first')) + '<p>: second</p>\n<p><a href="/real">^has space</a></p>\n');
  }
  const common = render('Term\n: first\n\n[^n]: body\n: second\n\nUse[^n].', 'commonmark-gfm');
  assert.ok(common.startsWith('<p>Term\n: first</p>\n<p>: second</p>\n'));
  assert.match(common, /href="#fn-n" role="doc-noteref"/);
  assert.match(common, /<li id="fn-n"><p>body<\/p>/);
  assert.doesNotMatch(common, /<dl>/);
});

test('paragraph interruption keeps headings, lists, quotes, fences and HTML outside definitions', () => {
  for (const [block, opening] of [
    ['# Heading', '<h1'], ['---', '<hr>'], ['> Quote', '<blockquote>'],
    ['- Item', '<ul>'], ['1. Item', '<ol>'], ['~~~\ncode\n~~~', '<pre><code>'],
    ['<div>\nraw\n</div>', '<p>&lt;div&gt;'],
  ]) {
    const html = render('Term\n: body\n' + block);
    assert.ok(html.startsWith(dl(pair('Term', 'body')) + opening), block);
  }
  assert.equal(render('Term\n: body\n2. continues'), dl(pair('Term', 'body 2. continues')));
  assert.equal(render('Term\n: body\n    continues'), dl(pair('Term', 'body continues')));
});

test('container lazy paragraphs retain their native admission and continuation authority', () => {
  assert.equal(render('> Term\n> : definition\nunmarked continuation'),
    '<blockquote>\n' + dl(pair('Term', 'definition unmarked continuation')) + '</blockquote>\n');
  assert.equal(render('- Term\n  : definition\nunmarked continuation'),
    '<ul>\n<li>' + dl(pair('Term', 'definition unmarked continuation')).trimEnd() + '</li>\n</ul>\n');
  assert.equal(render('> [r]: /target\nTerm\n: definition\n\n[r]'),
    '<blockquote>\n<p>Term\n: definition</p>\n</blockquote>\n<p><a href="/target">r</a></p>\n');
  assert.equal(render('- [r]: /target\nTerm\n: definition\n\n[r]'),
    '<ul>\n<li>Term\n: definition</li>\n</ul>\n<p><a href="/target">r</a></p>\n');
});

test('opaque code, HTML, mathematics and table cells never become definition lists', () => {
  const source = 'Term\n: Definition\n<!-- pagebreak -->';
  for (const wrapper of [
    value => '~~~md\n' + value + '\n~~~',
    value => value.split('\n').map(line => '    ' + line).join('\n'),
    value => '<div>\n' + value + '\n</div>',
    value => '<pre>\n' + value + '\n</pre>',
    value => '$$\n' + value + '\n$$',
  ]) {
    const html = render(wrapper(source));
    assert.doesNotMatch(html, /<dl>|class="fmd-pagebreak"/);
    assert.match(html, /Term/);
    assert.match(html, /: Definition/);
  }
  const table = render('A | B\n--- | ---\nTerm | : Definition\n<!-- pagebreak --> | text');
  assert.doesNotMatch(table, /<dl>|class="fmd-pagebreak"/);
  assert.match(table, /<td>Term<\/td><td>: Definition<\/td>/);
  assert.equal(render('Term\n: <!-- pagebreak -->'), dl(pair('Term', '&lt;!-- pagebreak --&gt;')));
});

test('notes referenced in terms and definitions participate in heading-safe anchor planning', () => {
  const html = render('Term[^n]\n: body[^m] again[^n]\n\n[^n]: # fn-m\n\n[^m]: Note.');
  assert.match(html, /<dt>Term<sup class="footnote-ref" id="fnref-1">/);
  assert.match(html, /<dd>body<sup class="footnote-ref" id="fnref-2"><a href="#fn-m-2"/);
  assert.match(html, /id="fnref-1-2"/);
  assert.match(html, /<h1 id="fn-m">fn-m<\/h1>/);
  assert.match(html, /<li id="fn-m-2"><p>Note\.<\/p>/);
  const ids = [...html.matchAll(/\bid="([^"]*)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const [, fragment] of html.matchAll(/href="#([^"]*)"/g)) assert.ok(ids.includes(decodeURIComponent(fragment)));
});

test('notes in linked definitions keep backlinks while image alt and code stay inert', () => {
  const html = render('Term\n: [claim[^n]](/target) ![alt[^hidden]](image.png) \x60[^hidden]\x60'
    + '\n\n[^n]: evidence[^m]\n\n[^m]: cycle[^n]\n\n[^hidden]: PRIVATE');
  assert.match(html, /<dd><a href="\/target">claim<sup class="footnote-ref" id="fnref-1">1<\/sup><\/a>/);
  assert.match(html, /alt="alt\[\^hidden\]"/);
  assert.match(html, /<code>\[\^hidden\]<\/code>/);
  assert.doesNotMatch(html, /PRIVATE|id="fn-hidden"/);
  assert.equal((html.match(/<li id="fn-n">/g) || []).length, 1);
  assert.equal((html.match(/<li id="fn-m">/g) || []).length, 1);
  assert.match(html, /href="#fnref-1-2" class="footnote-backref"/);
});

test('only exact standalone parser-owned pagebreak comments are promoted', () => {
  for (const indent of ['', ' ', '  ', '   ']) {
    for (const ending of ['', '\n', '\r\n', '\r']) {
      const source = indent + '<!-- pagebreak --> \t' + ending;
      assert.equal(render(source), pagebreak);
      assert.equal(render(source, 'commonmark-gfm'), literal(source.replace(/\r\n?|\n$/, '\n').replace(/\n$/, '')));
    }
  }
  for (const source of [
    '<!--pagebreak-->', '<!-- PAGEBREAK -->', '<!-- pagebreak --> after',
    '<!-- pagebreak --> <!-- more -->', '<!--\npagebreak\n-->', '<!-- pagebreak -->\u00a0',
    'before <!-- pagebreak --> after', '    <!-- pagebreak -->', '\t<!-- pagebreak -->',
  ]) assert.doesNotMatch(render(source), /class="fmd-pagebreak"/, source);
});

test('pagebreak promotion follows containers and referenced notes without consuming neighbors', () => {
  assert.equal(render('Before\n<!-- pagebreak -->\nAfter'), '<p>Before</p>\n' + pagebreak + '<p>After</p>\n');
  assert.equal(render('> <!-- pagebreak -->\n> After'), '<blockquote>\n' + pagebreak + '<p>After</p>\n</blockquote>\n');
  assert.equal(render('- <!-- pagebreak -->\n  After'), '<ul>\n<li>' + pagebreak + 'After</li>\n</ul>\n');
  const source = 'Use[^n].\n\n[^n]: <!-- pagebreak -->\n    After';
  assert.match(render(source), /<li id="fn-n"><div class="fmd-pagebreak" style="break-after: page;"><\/div>\n<p>After<\/p>/);
  assert.doesNotMatch(render(source, 'commonmark-gfm'), /class="fmd-pagebreak"/);
});

test('frontmatter, BOM and mixed physical newlines reach the selected authoring grammar', () => {
  const source = '\ufeff---\r\ntitle: Profile\r\n---\r\n*É中\r\n: definition*\r\r\n<!-- pagebreak -->\r';
  assert.equal(render(source, 'commonmark-gfm'),
    '<p><em>É中\n: definition</em></p>\n' + literal('<!-- pagebreak -->'));
  assert.equal(render(source, 'gfm-plus'), dl(pair('*É中', 'definition*')) + pagebreak);
});

test('large term probes and blank runs remain bounded within the ordinary work budget', () => {
  const source = 'Term\n'.repeat(5000) + ': value\n' + '\n'.repeat(12000) + 'Second\n: next\n\nTail\n';
  const bounded = vm.createContext({source});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 700000'), bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000});
  assert.equal((html.match(/<dt>Term<\/dt>/g) || []).length, 5000);
  assert.equal((html.match(/<dl>/g) || []).length, 1);
  assert.match(html, /<dt>Second<\/dt>\n<dd>next<\/dd>\n<\/dl>\n<p>Tail<\/p>\n$/);
});

test('definition list allocation failure cannot poison the next render or change grammar', () => {
  const source = 'Term\n: body\n'.repeat(500);
  const small = vm.createContext({source});
  vm.runInContext(code.replace('structures: 262144', 'structures: 1000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'structure');
  assert.equal(vm.runInContext('parseMarkdownClient("Term\\n: body", undefined, "commonmark-gfm")', small),
    '<p>Term\n: body</p>\n');
  assert.equal(vm.runInContext('parseMarkdownClient("Term\\n: body")', small), dl(pair('Term', 'body')));
});

test('glossary lookahead leaves a preceding setext heading and link target intact', () => {
  for (const [underline, level] of [['=', 1], ['===', 1], ['   == ', 1], ['--', 2], ['---', 2]]) {
    const source = 'First\n: old meaning\n\nAPI *Guide*\r\n' + underline
      + '\r\nTerm\r\n: new meaning\r\n\n[Jump](#api-guide)\n';
    const heading = '<h' + level + ' id="api-guide">API <em>Guide</em></h' + level + '>\n';
    for (const profile of plusProfiles) {
      assert.equal(render(source, profile), dl(pair('First', 'old meaning')) + heading
        + dl(pair('Term', 'new meaning')) + '<p><a href="#api-guide">Jump</a></p>\n', source);
    }
  }
});
