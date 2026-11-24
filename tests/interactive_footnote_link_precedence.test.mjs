// Run with: node --test tests/interactive_footnote_link_precedence.test.mjs
// Exercise the shipped parser directly; no browser or replacement grammar.
// cmark-gfm tries links before footnotes in src/inlines.c::handle_close_bracket:
// https://github.com/github/cmark-gfm/blob/7fe8904ee31fa24b772ae565c4908b1394850de6/src/inlines.c
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
const render = new Function(source + '\nreturn parseMarkdownClient;')();
const countNotes = html => (html.match(/class="footnote-ref"/g) || []).length;

test('explicit links win over defined notes without adding phantom citations', () => {
  for (const [id, label] of [['note', '^note'], ['é中🙂', '^é中🙂'], ['a"b', '^a&quot;b'], ['https://example.test', '^https://example.test']]) {
    const html = render(`[^${id}](/chosen) then actual[^${id}].\n\n[^${id}]: Note body.`);
    assert.ok(html.includes(`<a href="/chosen">${label}</a> then actual`), html);
    assert.equal(countNotes(html), 1, html);
    assert.ok(html.includes('Note body.'), html);
    assert.ok(!html.includes('fnref-1-2'), html);
  }
});

test('known-note labels retain destination and title parsing', () => {
  const titled = render('[^note](/route_(v2) "Title &amp; more")\n\n[^note]: body');
  assert.ok(titled.includes('href="/route_(v2)"'), titled);
  assert.ok(titled.includes('title="Title &amp; more"'), titled);
  assert.equal(countNotes(titled), 0, titled);
  const angled = render('[^note](</route with spaces>)\n\n[^note]: body');
  assert.ok(angled.includes('href="/route%20with%20spaces"'), angled);
  assert.equal(countNotes(angled), 0, angled);
});

test('full references win over notes and undefined note labels remain ordinary links', () => {
  const html = render('[^é中][CHOSEN] then actual[^é中].\n\n[Chosen]: /chosen "Title"\n\n[^é中]: body');
  assert.ok(html.includes('<a href="/chosen" title="Title">^é中</a>'), html);
  assert.equal(countNotes(html), 1, html);
  const undefinedNote = render('[^missing](/chosen)');
  assert.ok(undefinedNote.includes('<a href="/chosen">^missing</a>'), undefinedNote);
  assert.equal(countNotes(undefinedNote), 0, undefinedNote);
});

test('inner caret links defeat outer anchors while images retain their own target', () => {
  for (const [inner, reference] of [['[^note](/inside)', ''], ['[^note][ref]', '\n\n[ref]: /inside']]) {
    const definitions = reference + '\n\n[^note]: body';
    const html = render(`[outside ${inner}](/outside)${definitions}`);
    assert.ok(html.includes('[outside <a href="/inside">^note</a>](/outside)'), html);
    assert.ok(!html.includes('href="/outside"'), html);
    assert.equal(countNotes(html), 0, html);
    const image = render(`![outside ${inner}](picture.png)${definitions}`);
    assert.ok(image.includes('<img src="picture.png" alt="outside ^note"'), image);
    assert.ok(!image.includes('href="/inside"'), image);
    assert.equal(countNotes(image), 0, image);
  }
});

test('actual citations keep punctuation opaque inside a surrounding link', () => {
  for (const id of ['a"b', 'a<b>', 'a<https://e>', 'a`', 'a$b', 'https://example.test']) {
    const markdown = `[outside[^${id}]](/chosen) trailing ` + '` $' + `\n\n[^${id}]: body`;
    const html = render(markdown);
    assert.ok(html.includes('href="/chosen"'), html);
    assert.equal(countNotes(html), 1, html);
    assert.ok(html.includes(' trailing ` $'), html);
    assert.ok(!html.includes('href="https://e"'), html);
  }
});

test('failed speculative links preserve notes and reindex subsequent source', () => {
  for (const tail of ['', '(/unterminated', '(<bad\nurl>)', '[missing]', '[]']) {
    const html = render(`[^note]${tail}\n\n[^note]: body`);
    assert.equal(countNotes(html), 1, html);
    assert.ok(!html.includes('href="/unterminated"'), html);
  }
  const html = render('[^a`] literal `after` [real](/after)\n\n[^a`]: body');
  assert.equal(countNotes(html), 1, html);
  assert.ok(html.includes(' literal <code>after</code> <a href="/after">real</a>'), html);
});

test('code can own a bracket inside an explicit caret label', () => {
  const html = render('[^a`]` more](/chosen)\n\n[^a`]: body');
  assert.ok(html.includes('<a href="/chosen">^a<code>]</code> more</a>'), html);
  assert.equal(countNotes(html), 0, html);
});

test('opaque code and escaped brackets do not acquire links or citations', () => {
  for (const markdown of ['`[^note](/hidden)`', '\\[^note](/hidden)', '<span title="[^note](/hidden)">visible</span>']) {
    const html = render(markdown + '\n\n[^note]: body');
    assert.ok(!html.includes('href="/hidden"'), html);
    assert.equal(countNotes(html), 0, html);
  }
});

test('unsafe explicit destinations retain link ownership without becoming citations', () => {
  const html = render('[^note](javascript:alert(1))\n\n[^note]: body');
  assert.equal(countNotes(html), 0, html);
  assert.ok(html.includes('^note'), html);
  assert.ok(!html.includes('href="javascript:'), html);
});

test('containers and repeated links retain link ownership', () => {
  const containers = render('\ufeff# Start\r\n\r\n> [^é中](/quoted)\r\n\r\n- [^note](/listed)\r\n\r\n[^é中]: quoted note\r\n\r\n[^note]: list note');
  assert.ok(containers.includes('<a href="/quoted">^é中</a>'), containers);
  assert.ok(containers.includes('<a href="/listed">^note</a>'), containers);
  assert.equal(countNotes(containers), 0, containers);
  const repeated = render('[^note](/chosen) '.repeat(4096) + '\n\n[^note]: body');
  assert.equal((repeated.match(/href="\/chosen"/g) || []).length, 4096);
  assert.equal(countNotes(repeated), 0);
});
