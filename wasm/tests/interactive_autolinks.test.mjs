// Execute the saved-document renderer's angle-autolink grammar.
// Native source: src/parse/mod.rs::{parse_autolink,is_uri_autolink,is_email_autolink}.
// Exact CommonMark 0.31.2 fixtures remain unchanged. URL admission remains the
// lightweight renderer's existing allowlist; bare links use the GFM extension.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
const anchor = (label, url = label) => '<a href="' + escape(url) + '">' + escape(label) + '</a>';

test('official autolink examples retain exact grammar and the existing safe URL policy', () => {
  const corpus = JSON.parse(readFileSync(new URL('../../tests/fixtures/commonmark/spec.json', import.meta.url), 'utf8'));
  const examples = corpus.filter(example => example.section === 'Autolinks');
  assert.equal(examples.length, 19);
  // Four unsupported schemes and a literal-backslash URI remain inert source.
  // These existing policy differences are not being claimed as conformance.
  const refused = new Set([596, 598, 599, 601, 603]);
  // GFM intentionally links these two bare forms; the original CommonMark
  // fixture remains intact and all angle-autolink expectations stay exact.
  const gfm = new Map([
    [611, '<p>' + anchor('https://example.com') + '</p>\n'],
    [612, '<p>' + anchor('foo@bar.example.com', 'mailto:foo@bar.example.com') + '</p>\n'],
  ]);
  for (const example of examples) {
    const expected = refused.has(example.example)
      ? '<p>' + escape(example.markdown.replace(/\n$/, '')) + '</p>\n' : gfm.get(example.example) ?? example.html;
    assert.equal(render(example.markdown), expected, 'CommonMark example ' + example.example);
  }
});

test('URI schemes keep their complete destination before email recognition', () => {
  for (const label of [
    'mailto:reader@example.test',
    'MAILTO:Reader@Example.TEST',
    'https://reader@example.test/manual',
    'http://user:password@example.test/',
    'https://example.test/?contact=reader@example.test',
    'https://example.test/#reader@example.test',
    'tel:+1-555-0100',
    'mailto:reader@localhost',
  ]) {
    assert.equal(render('<' + label + '>'), '<p>' + anchor(label) + '</p>\n', label);
  }
  for (const label of ['xmpp:reader@example.test', 'javascript:reader@example.test', 'file:reader@example.test', 'data:reader@example.test']) {
    assert.equal(render('<' + label + '>'), '<p>&lt;' + escape(label) + '&gt;</p>\n', label);
  }
});

test('email local characters remain literal inside a recognized autolink', () => {
  const punctuation = ".!#$%&'*+/=?^_" + String.fromCharCode(96) + '{|}~-';
  for (const local of ['reader', 'First.Last', 'a..b', ...Array.from(punctuation, ch => 'a' + ch + 'b')]) {
    const label = local + '@Example.TEST';
    assert.equal(render('<' + label + '>'), '<p>' + anchor(label, 'mailto:' + label) + '</p>\n', local);
  }
});

test('email domains accept single labels and exact 63-character label boundaries', () => {
  for (const domain of [
    'b', 'localhost', 'LOCALHOST', '9', 'example.test', 'foo-bar', 'foo-bar.test',
    'a'.repeat(63), 'a'.repeat(63) + '.' + 'b'.repeat(63),
    ('a'.repeat(63) + '.').repeat(128) + 'last',
  ]) {
    const label = 'reader@' + domain;
    assert.equal(render('<' + label + '>'), '<p>' + anchor(label, 'mailto:' + label) + '</p>\n', domain);
  }
  for (const domain of [
    '', '.', '.test', 'test.', 'first..last', '..test', 'test..',
    '-test', 'test-', '-test.example', 'test-.example', 'test.-example', 'test.example-',
    '_test', 'test_', 'test._example', 'éxample.test', '例え.テスト', 'a'.repeat(64),
    'ok.' + 'b'.repeat(64), 'a'.repeat(64) + '.ok',
  ]) {
    const label = 'reader@' + domain;
    assert.equal(render('<' + label + '>'), '<p>&lt;' + escape(label) + '&gt;</p>\n', domain);
  }
});

test('invalid email local parts and extra at signs do not gain link authority', () => {
  for (const label of [
    '@example.test', 'reader@@example.test', 'reader@example@test', 'reader example@test',
    'reader(example)@test', 'reader,other@test', 'reader;other@test', 'reader"other@test',
    'réader@example.test', '読者@example.test', 'reader\u0085other@test', 'reader\ufeffother@test',
  ]) {
    assert.equal(render('<' + label + '>'), '<p>&lt;' + escape(label) + '&gt;</p>\n', label);
  }
  assert.equal(render('<reader\\+@example.test>'),
    '<p>&lt;reader+@example.test&gt;</p>\n');
  assert.equal(render('<reader&copy;@example.test>'),
    '<p>&lt;reader©@example.test&gt;</p>\n');
  assert.equal(render('<reader*kept*@broken..test> [live](/ok)'),
    '<p>&lt;reader<em>kept</em>@broken..test&gt; <a href="/ok">live</a></p>\n');
});

test('URI scheme boundaries distinguish literal atoms from ordinary Markdown source', () => {
  const hidden = '[hidden](/wrong)';
  for (const scheme of ['ab', 'A9', 'a+b.c-d', 'a'.repeat(32)]) {
    const label = scheme + ':' + hidden;
    assert.equal(render('<' + label + '>'), '<p>&lt;' + label + '&gt;</p>\n', scheme);
  }
  for (const scheme of ['a', '1a', 'a_b', 'a'.repeat(33)]) {
    assert.equal(render('<' + scheme + ':[live](/ok)>'),
      '<p>&lt;' + scheme + ':<a href="/ok">live</a>&gt;</p>\n', scheme);
  }
});

test('native whitespace and control boundaries preserve literal URI identity', () => {
  for (const gap of [' ', '\t', '\u0001', '\u001f', '\u007f', '\u0085', '\u00a0', '\u1680', '\u2003', '\u2028', '\u202f', '\u3000']) {
    const label = 'https://example.test/a' + gap + 'b';
    assert.equal(render('<' + label + '>'), '<p>&lt;' + escape(label) + '&gt;</p>\n',
      'boundary U+' + gap.charCodeAt(0).toString(16));
  }
  // U+FEFF is not Unicode White_Space: no parser or URL-trim step may drop it.
  for (const label of ['https://example.test/\ufeff', 'https://example.test/a\ufeffb']) {
    assert.equal(render('<' + label + '>'), '<p>' + anchor(label) + '</p>\n');
  }
  assert.equal(render('<https://example.test/a<b>'), '<p>&lt;https://example.test/a&lt;b&gt;</p>\n');
});

test('autolinks keep entities literal while invalid syntax returns to ordinary inlining', () => {
  for (const label of ['https://example.test/&copy;', 'https://example.test/?a=1&amp;b=2', 'mailto:reader&copy;@example.test']) {
    assert.equal(render('<' + label + '>'), '<p>' + anchor(label) + '</p>\n', label);
  }
  assert.equal(render('<https://example.test/a\\*b>'),
    '<p>&lt;https://example.test/a\\*b&gt;</p>\n');
  assert.equal(render('<reader\\*@example.test>'),
    '<p>&lt;reader*@example.test&gt;</p>\n');
});

test('autolinks retain nested label and inline atom ownership', () => {
  assert.equal(render('*<reader@localhost>*'),
    '<p><em>' + anchor('reader@localhost', 'mailto:reader@localhost') + '</em></p>\n');
  assert.equal(render('[outer <reader@localhost>](/wrong)'),
    '<p>[outer ' + anchor('reader@localhost', 'mailto:reader@localhost') + '](/wrong)</p>\n');
  assert.equal(render('![<reader@localhost>](picture.png)'),
    '<p><img src="picture.png" alt="reader@localhost"></p>\n');
  const assets = {has() { assert.fail('opaque autolink inspected an image binding'); }};
  for (const scheme of ['https', 'unsupported']) {
    const label = scheme + ':![hidden](private.png)[^n]';
    const expected = scheme === 'https' ? anchor(label) : '&lt;' + label + '&gt;';
    assert.equal(render('<' + label + '>\n\n[^n]: Secret', assets), '<p>' + expected + '</p>\n');
  }
  const literal = '<reader@localhost>';
  assert.equal(render(String.fromCharCode(96) + literal + String.fromCharCode(96)),
    '<p><code>&lt;reader@localhost&gt;</code></p>\n');
  assert.equal(render('$' + literal + '$'), '<p>$&lt;reader@localhost&gt;$</p>\n');
});

test('autolinks work through headings lists tables quotes and footnotes', () => {
  const label = 'reader@localhost', link = anchor(label, 'mailto:' + label);
  for (const source of [
    '# <' + label + '>', '- <' + label + '>', '> <' + label + '>',
    'A | B\n--- | ---\n<' + label + '> | kept',
    'Use[^n]\n\n[^n]: <' + label + '>',
  ]) {
    const html = render(source);
    assert.equal(html.split(link).length - 1, 1, source);
  }
});

test('long valid locals and hostile dotted domains retain bounded complete output', () => {
  const bounded = vm.createContext({});
  vm.runInContext(code, bounded);
  const script = new vm.Script('parseMarkdownClient(source)');
  const run = source => { bounded.source = source; return script.runInContext(bounded, {timeout: 5000}); };
  const valid = 'a'.repeat(40000) + '@localhost';
  assert.equal(run('<' + valid + '>'), '<p>' + anchor(valid, 'mailto:' + valid) + '</p>\n');
  for (const suffix of ['', '@', 'last']) {
    const invalid = 'reader@' + '.'.repeat(100000) + suffix;
    assert.equal(run('<' + invalid + '>'), '<p>&lt;' + invalid + '&gt;</p>\n');
  }
  const repeated = '<reader@localhost> '.repeat(5000);
  assert.equal(run(repeated), '<p>' + (anchor('reader@localhost', 'mailto:reader@localhost') + ' ').repeat(5000).trimEnd() + '</p>\n');
  assert.equal(run('<reader@localhost>'), '<p>' + anchor('reader@localhost', 'mailto:reader@localhost') + '</p>\n');
});
