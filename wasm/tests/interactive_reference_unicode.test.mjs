// Exercise the standalone export's production parser, with no browser or WASM.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const data = readFileSync(new URL('../../tests/fixtures/unicode/CaseFolding-17.0.0.txt', import.meta.url), 'utf8');
const official = new Map();
for (const line of data.split('\n')) {
  const [cp, status, mapping] = line.split('#')[0].split(';').map(field => field.trim());
  if (status === 'C' || status === 'F') {
    official.set(parseInt(cp, 16), String.fromCodePoint(...mapping.split(/\s+/).map(value => parseInt(value, 16))));
  }
}

test('every official full fold resolves actual references and images in both directions', () => {
  assert.equal(official.size, 1585);
  const expected = '<p><a href="/target.png" title="Title">label</a> <img src="/target.png" alt="picture" title="Title"></p>\n';
  const mismatches = [];
  for (const [cp, folded] of official) {
    const original = String.fromCodePoint(cp);
    for (const [definition, label] of [[original, folded], [folded, original]]) {
      const html = render(`[label][${label}] ![picture][${label}]\n\n[${definition}]: /target.png "Title"\n`);
      if (html !== expected) mismatches.push(`U+${cp.toString(16)}: ${definition} -> ${label}`);
    }
  }
  assert.equal(mismatches.length, 0, `${mismatches.length} failed resolutions; first cases: ${mismatches.slice(0, 12)}`);
});

test('complete scalar lookup equals the pinned full fold, including all identity gaps', () => {
  for (let cp = 0; cp <= 0x10ffff; cp++) {
    if (cp >= 0xd800 && cp <= 0xdfff) continue;
    assert.equal(context.fmdFoldReferenceScalar(cp), official.get(cp) ?? String.fromCodePoint(cp), `U+${cp.toString(16)}`);
  }
});

test('full, collapsed and shortcut references keep their original visible spelling', () => {
  for (const [label, definition] of [['ﬃ', 'FFI'], ['ſ', 'S'], ['µ', 'Μ'], ['ǰ', 'J\u030c'], ['և', 'ԵՒ'], ['ᾲ', 'ὰΙ'], ['ᏸ', 'Ᏸ'], ['İ', 'i\u0307']]) {
    const html = render(`[label][${label}] [${label}][] [${label}]\n\n[${definition}]: /target "Title"\n`);
    assert.equal(html, `<p><a href="/target" title="Title">label</a> <a href="/target" title="Title">${label}</a> <a href="/target" title="Title">${label}</a></p>\n`);
  }
});

test('folded duplicate definitions keep first source order across containers', () => {
  for (const [first, second] of [['ﬃ', 'FFI'], ['FFI', 'ﬃ'], ['ſ', 'S'], ['S', 'ſ']]) {
    for (const prefix of ['', '> ', '- ']) {
      const html = render(`${prefix}[${first}]: /first "Title"\n\n[${second}]: /second "Wrong"\n\n[label][${second}] ![picture][${first}]`);
      assert.match(html, /<a href="\/first" title="Title">label<\/a>/);
      assert.match(html, /<img src="\/first" alt="picture" title="Title">/);
      assert.doesNotMatch(html, /second|Wrong/);
    }
  }
});

test('reference folding does not imply locale tailoring, normalization or inline decoding', () => {
  for (const [definition, label] of [['I', 'ı'], ['İ', 'i'], ['é', 'e\u0301'], ['Ａ', 'A'], ['&#xFB03;', 'ffi'], ['\\*', '*']]) {
    assert.doesNotMatch(render(`[${definition}]: /wrong\n\n[visible][${label}]\n`), /<a\b/);
  }
});

test('label limits count original Unicode scalars before trimming and expansion', () => {
  for (const label of ['ﬃ'.repeat(999), '𐐀'.repeat(999)]) {
    assert.match(render(`[${label}]: /valid\n\n[visible][${label}]`), /<a href="\/valid">visible<\/a>/);
  }
  const ligatures = 'ﬃ'.repeat(333), expanded = 'FFI'.repeat(333);
  assert.match(render(`[${ligatures}]: /valid\n\n[visible][${expanded}]`), /<a href="\/valid">visible<\/a>/);
  for (const label of ['ﬃ'.repeat(1000), '𐐀'.repeat(1000)]) {
    assert.doesNotMatch(render(`[${label}]: /invalid\n\n[visible][${label}]`), /<a\b/);
  }
  // An oversized use cannot evade the source limit by collapsing whitespace.
  assert.doesNotMatch(render(`[id]: /invalid\n\n[visible][${' '.repeat(998)}id]`), /<a\b/);
});

test('multiline labels fold after whitespace collapsing without touching destinations', () => {
  const html = render('[label][ STRASSE\tFFI ] ![picture][Straße ﬃ]\r\n\r\n[Straße\r\n  ﬃ]: </café#FfI> "Title"\r\n');
  assert.equal(html, '<p><a href="/café#FfI" title="Title">label</a> <img src="/café#FfI" alt="picture" title="Title"></p>\n');
});

test('footnote identities and heading IDs retain their separate existing rules', () => {
  const html = render('# ﬃ\n\n# FFI\n\nOne[^ﬃ] two[^ffi]\n\n[^ﬃ]: ligature note\n[^ffi]: ordinary note');
  assert.match(html, /<h1 id="ﬃ">ﬃ<\/h1>/);
  assert.match(html, /<h1 id="ffi">FFI<\/h1>/);
  assert.match(html, /id="fn-1"/);
  assert.match(html, /id="fn-2"/);
  assert.match(html, /ligature note/);
  assert.match(html, /ordinary note/);
});

test('Unicode label whitespace keeps distinct link and image destinations', () => {
  for (const separator of ['\u00a0', '\u2003', '\ufeff']) {
    const label = 'part' + separator + 'one';
    const html = render('[part one]: /ascii\n[' + label + ']: /distinct\n\n[plain][part one] [special][' + label + '] ![image][' + label + ']\n');
    assert.equal(html, '<p><a href="/ascii">plain</a> <a href="/distinct">special</a> <img src="/distinct" alt="image"></p>\n');
  }
});

test('Unicode label edges and whitespace-only labels work in all reference forms', () => {
  for (const label of ['\u00a0part', 'part\u00a0', '\u00a0']) {
    const html = render('[part]: /ascii\n[' + label + ']: /distinct\n\n[full][' + label + '] [' + label + '][] [' + label + ']\n');
    assert.equal(html, '<p><a href="/distinct">full</a> <a href="/distinct">' + label + '</a> <a href="/distinct">' + label + '</a></p>\n');
  }
});

test('multiline labels preserve Unicode content while folding ASCII padding and case', () => {
  const source = '[Straße\n \u00a0\n ONE]: /first\n[strasse \u00a0 one]: /shadow\n[strasse one]: /other\n\n[label][STRASSE\t\u00a0  ONE]\n';
  assert.equal(render(source), '<p><a href="/first">label</a></p>\n');
  assert.equal(render(source.replace(/\n/g, '\r\n')), '<p><a href="/first">label</a></p>\n');
});

test('reference collection retains Unicode-only label and title lines in containers', () => {
  const quote = '> [\u00a0]: /target "first\n> \u00a0\n> # title content\n> last"\n>\n> [label][\u00a0]\n';
  assert.equal(render(quote), '<blockquote>\n<p><a href="/target" title="first\n\u00a0\n# title content\nlast">label</a></p>\n</blockquote>\n');
  const list = '- [\u00a0]: /target "first\n  \u00a0\n  last"\n\n[label][\u00a0]\n';
  assert.match(render(list), /<a href="\/target" title="first\n\u00a0\nlast">label<\/a>/);
  assert.doesNotMatch(render(list), /<h[1-6]\b/);
});

test('multiline titles accept significant Unicode content in inline and reference links', () => {
  for (const separator of ['\u00a0', '\u2003', '\f', '\u2028']) {
    const title = 'first\n ' + separator + '\nlast';
    const expected = '<p><a href="/target" title="' + title + '">label</a></p>\n';
    assert.equal(render('[label](/target "' + title + '")'), expected);
    for (const opening of [' ', '\n']) {
      assert.equal(render('[ref]: /target' + opening + '"' + title + '"\n\n[label][ref]\n'), expected);
    }
  }
});

test('ASCII blank lines still reject same-line-started titles and empty labels', () => {
  for (const blank of ['', ' ', '\t', ' \t ']) {
    assert.doesNotMatch(render('[ref]: /wrong "first\n' + blank + '\nlast"\n\n[ref]\n'), /<a\b/);
    assert.doesNotMatch(render('[' + blank + ']: /wrong\n\n[' + blank + ']\n'), /<a\b/);
  }
});

test('preserved Unicode title lines keep markup inert and unsafe URLs blocked', () => {
  const title = 'first\n\u00a0\n<script>[hidden](/hidden)</script>&quot; onmouseover=&quot;x';
  const html = render('[ref]: /safe "' + title + '"\n\n[label][ref]\n');
  assert.match(html, /href="\/safe"/);
  assert.match(html, /&lt;script&gt;/);
  assert.doesNotMatch(html, /<script>| onmouseover="|href="\/hidden"/);
  assert.doesNotMatch(render('[ref]: javascript:alert(1) "' + title + '"\n\n[label][ref]\n'), /<a\b|<script>/);
});
