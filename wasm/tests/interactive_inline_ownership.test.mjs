// Exercise source ownership through the production standalone renderer.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);

test('code brackets cannot select a different link destination', () => {
  assert.equal(render('[label `](/wrong)` tail](/right)'),
    '<p><a href="/right">label <code>](/wrong)</code> tail</a></p>\n');
  for (const literal of [']', '[', '[[]]', '](/wrong)', 'é🙂 [ ]']) {
    assert.equal(render(`[use \`${literal}\` safely](/guide)`),
      `<p><a href="/guide">use <code>${literal}</code> safely</a></p>\n`);
  }
});

test('emphasis and strike never activate links or footnotes from code content', () => {
  for (const [marker, open, close] of [['*', '<em>', '</em>'], ['_', '<em>', '</em>'], ['**', '<strong>', '</strong>'], ['***', '<strong><em>', '</em></strong>'], ['~~', '<del>', '</del>']]) {
    const source = `${marker}before \`${marker} [evil] [^n]\` after${marker}\n\n[evil]: /wrong\n[^n]: SECRET NOTE`;
    assert.equal(render(source), `<p>${open}before <code>${marker} [evil] [^n]</code> after${close}</p>\n`);
  }
});

test('code spans keep maximal delimiters and escape semantics inside labels', () => {
  assert.equal(render('[before ``a ` ](/wrong) b`` after](/right)'),
    '<p><a href="/right">before <code>a ` ](/wrong) b</code> after</a></p>\n');
  assert.equal(render('[before `a `` ](/wrong) b` after](/right)'),
    '<p><a href="/right">before <code>a `` ](/wrong) b</code> after</a></p>\n');
  assert.equal(render('[literal \\` and \\[x\\]](/right)'),
    '<p><a href="/right">literal ` and [x]</a></p>\n');
});

test('reference links and images use code-aware label boundaries', () => {
  assert.equal(render('[use `]`][r] ![show `](/wrong)`][r]\n\n[r]: /right "Title"'),
    '<p><a href="/right" title="Title">use <code>]</code></a> <img src="/right" alt="show ](/wrong)" title="Title"></p>\n');
});

test('angle destinations and all title forms own their punctuation', () => {
  for (const [source, destination, title] of [
    ['[doc](<foo)bar>)', 'foo)bar', null],
    ['[doc](<foo(bar>)', 'foo(bar', null],
    ['[doc](/right "literal ) [ ] *")', '/right', 'literal ) [ ] *'],
    ["[doc](/right 'literal ( [ ] _')", '/right', 'literal ( [ ] _'],
    [String.raw`[doc](/right (literal \) [ ]))`, '/right', 'literal ) [ ]'],
    ['[doc](<> "empty")', '', 'empty'],
  ]) {
    assert.equal(render(source), `<p><a href="${destination}"${title === null ? '' : ` title="${title}"`}>doc</a></p>\n`, source);
  }
});

test('backticks in accepted destinations and titles cannot hide subsequent links', () => {
  for (const first of ['[one](/a`x)', '[one](</a`x>)', '[one](/one "`code")', "[one](/one '`code')", '[one](/one (`code))']) {
    const html = render(`${first} [two](/two) \``);
    assert.equal((html.match(/<a /g) || []).length, 2, first);
    assert.match(html, /<a href="\/two">two<\/a> `/, first);
    assert.doesNotMatch(html, /<code>/, first);
  }
});

test('image targets cannot steal their enclosing link bracket', () => {
  for (const destination of ['image].png', 'image[.png', '[image].png', 'image`name.png']) {
    for (const title of ['caption]', 'caption[', 'caption ` [ ]']) {
      const source = `[![alt](${destination} "${title}")](/viewer)`;
      assert.equal(render(source), `<p><a href="/viewer"><img src="${destination}" alt="alt" title="${title}"></a></p>\n`, source);
    }
  }
});

test('accepted child links defeat an outer link while rejected tails remain source', () => {
  const html = render('[outer [inner](/inside)](/outside "[other](/other)")');
  assert.match(html, /href="\/inside"/);
  assert.match(html, /href="\/other"/);
  assert.doesNotMatch(html, /href="\/outside"/);
  assert.equal((html.match(/<a /g) || []).length, 2);
  for (const child of ['[inner][ref]', '<https://inside.test>']) {
    const html = render(`[outer ${child}](/outside "[other](/other)")\n\n[ref]: /inside`);
    assert.doesNotMatch(html, /href="\/outside"/);
    assert.match(html, /href="\/other"/);
    assert.equal((html.match(/<a /g) || []).length, 2);
  }
});

test('images flatten child links and image alt text without disabling their outer link', () => {
  assert.equal(render('[![before [inner](/inside "`x") after](image].png)](/outer)'),
    '<p><a href="/outer"><img src="image].png" alt="before inner after"></a></p>\n');
  assert.equal(render('![before ![child](child.png) after](parent.png)'),
    '<p><img src="parent.png" alt="before child after"></p>\n');
  const assets = {has: url => { assert.notEqual(url, 'child.png'); return false; }};
  assert.equal(context.parseMarkdownClient('![before ![child](child.png) after](parent.png)', assets),
    '<p><img src="parent.png" alt="before child after"></p>\n');
});

test('explicit reference labels are raw source even when they contain backticks', () => {
  for (const label of ['ref `x', 'ref *x', 'réf \\] `x']) {
    const html = render(`[text][${label}] [next](/next) \` *\n\n[${label}]: /chosen`);
    assert.match(html, /<a href="\/chosen">text<\/a>/, label);
    assert.match(html, /<a href="\/next">next<\/a>/, label);
    assert.doesNotMatch(html, /<code>/, label);
  }
});

test('an unresolved full reference blocks shortcut fallback but malformed suffixes do not', () => {
  assert.equal(render('[foo][missing]\n\n[foo]: /chosen'), '<p>[foo][missing]</p>\n');
  for (const suffix of ['[', '[broken', '[not[bar]]', '[' + '🙂'.repeat(1000) + ']']) {
    assert.equal(render(`[foo]${suffix}\n\n[foo]: /chosen`), `<p><a href="/chosen">foo</a>${suffix}</p>\n`);
  }
});

test('raw footnote identifiers stay opaque without publishing speculative references', () => {
  const html = render('[label [^n`q]](/outer) [later](/later) `\n\n[^n`q]: Private body');
  assert.equal(html, '<p><a href="/outer">label [^n`q]</a> <a href="/later">later</a> `</p>\n');
  assert.doesNotMatch(html, /Private body|footnote-ref/);
  const visible = render('*before [^n`q] after*\n\n[^n`q]: One body');
  assert.equal((visible.match(/class="footnote-ref"/g) || []).length, 1);
  assert.equal((visible.match(/One body/g) || []).length, 1);
});

test('emphasis delimiters in accepted link targets and reference labels are literal', () => {
  assert.equal(render('*before [link](/a*b "title *") after*'),
    '<p><em>before <a href="/a*b" title="title *">link</a> after</em></p>\n');
  assert.equal(render('*before [link][r*] after*\n\n[r*]: /right'),
    '<p><em>before <a href="/right">link</a> after</em></p>\n');
});

test('rejected targets preserve source and URL policy still applies after ownership', () => {
  assert.equal(render('[text](/ok "unterminated) [next](/next)'),
    '<p>[text](/ok &quot;unterminated) <a href="/next">next</a></p>\n');
  const html = render('[**label**](javascript:bad "fake [x](/evil)")');
  assert.equal(html, '<p><strong>label</strong></p>\n');
  assert.doesNotMatch(render('![alt](data:text/html,bad "fake [x](/evil)")'), /<(?:a|img)\b/);
});

test('accepted target values are decoded once and escaped once', () => {
  assert.equal(render('[x](/a&amp;amp; "T &amp;amp;")'),
    '<p><a href="/a&amp;amp;" title="T &amp;amp;">x</a></p>\n');
  assert.equal(render(String.raw`[x](/a\(b\) "T \" quoted")`),
    '<p><a href="/a(b)" title="T &quot; quoted">x</a></p>\n');
});

test('inline ownership works inside headings, tables, quotes and rich footnotes', () => {
  for (const source of [
    '# [use `]` safely](/guide)',
    '> [use `]` safely](/guide)',
    '| [use `]` safely](/guide) |\n| --- |\n| cell |',
    'Use[^n].\n\n[^n]: [use `]` safely](/guide)',
  ]) {
    assert.match(render(source), /<a href="\/guide">use <code>\]<\/code> safely<\/a>/, source);
  }
});

test('large literal and linked inputs use bounded local indexes and reset after failure', () => {
  const count = 2000, source = '[x `]` ](/ok) '.repeat(count);
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 5000});
  assert.equal((html.match(/href="\/ok"/g) || []).length, count);
  const small = vm.createContext({source: '*a '.repeat(500)});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 8000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}), error => error.code === 'FMD_RENDER_LIMIT');
  assert.equal(vm.runInContext('parseMarkdownClient("[x](/ok)")', small), '<p><a href="/ok">x</a></p>\n');
});
