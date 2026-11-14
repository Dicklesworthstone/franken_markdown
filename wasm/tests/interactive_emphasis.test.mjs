// Execute production saved-document formatting with unchanged official source.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));

test('all official emphasis examples retain exact structure and the existing raw-HTML policy', () => {
  const corpus = JSON.parse(readFileSync(new URL('../../tests/fixtures/commonmark/spec.json', import.meta.url), 'utf8'));
  const emphasis = corpus.filter(example => example.section === 'Emphasis and strong emphasis');
  assert.equal(emphasis.length, 132);
  // These three unchanged examples intentionally expose raw HTML; standalone
  // output must still escape it. Every other expected string is used verbatim.
  const escaped = new Map([
    [475, '<p>*&lt;img src=&quot;foo&quot; title=&quot;*&quot;/&gt;</p>\n'],
    [476, '<p>**&lt;a href=&quot;**&quot;&gt;</p>\n'],
    [477, '<p>__&lt;a href=&quot;__&quot;&gt;</p>\n'],
  ]);
  for (const example of emphasis) {
    assert.equal(render(example.markdown), escaped.get(example.example) ?? example.html,
      `CommonMark example ${example.example}: ${JSON.stringify(example.markdown)}`);
  }
});

test('nested mixed runs and crossing delimiters follow source event order', () => {
  for (const [source, expected] of [
    ['*outer **strong** tail*', '<em>outer <strong>strong</strong> tail</em>'],
    ['***a** b*', '<em><strong>a</strong> b</em>'],
    ['*a **b* c**', '<em>a <em><em>b</em> c</em></em>'],
    ['*a _b* c_', '<em>a _b</em> c_'],
    ['a***b**c*', 'a<em><strong>b</strong>c</em>'],
    ['**a *b **c** d* e**', '<strong>a <em>b <strong>c</strong> d</em> e</strong>'],
    ['a**b*c**d*e', 'a<strong>b*c</strong>d*e'],
    ['~~a **b *c* d** e~~', '<del>a <strong>b <em>c</em> d</strong> e</del>'],
  ]) assert.equal(render(source), `<p>${expected}</p>\n`, source);
});

test('inert underscore runs remain complete inside ASCII and Unicode words', () => {
  for (const [left, right] of [['foo', 'bar'], ['5', '6'], ['пристаням', 'стремятся'], ['𐐀', '𐐁'], ['e\u0301', 'word']]) {
    for (let size = 1; size <= 8; size++) {
      const marker = '_'.repeat(size), source = left + marker + right + marker + left;
      assert.equal(render(source), '<p>' + source + '</p>\n', source);
    }
  }
});

test('Unicode scalar whitespace punctuation and symbols control flanking', () => {
  for (const source of ['a*"foo"*', 'a**£foo£**', '*😀*word', 'word*😀*', 'a_\u0301b_', 'Before _word_\ufeff', 'Before \ufeff_word_']) {
    assert.equal(render(source), '<p>' + escape(source) + '</p>\n', source);
  }
  for (const [source, expected] of [
    ['😀_word_😀', '😀<em>word</em>😀'],
    ['Before *😀* after', 'Before <em>😀</em> after'],
    ['*e\u0301*', '<em>e\u0301</em>'],
    ['_word_\u0085', '<em>word</em>\u0085'],
    ['_word_\u2003', '<em>word</em>\u2003'],
    ['&nbsp;*word*', '\u00a0<em>word</em>'],
    ['*&nbsp;*', '<em>\u00a0</em>'],
  ]) assert.equal(render(source), '<p>' + expected + '</p>\n', source);
});

test('residual and escaped runs retain every unused delimiter', () => {
  for (const [source, expected] of [
    ['**foo*', '*<em>foo</em>'], ['*foo**', '<em>foo</em>*'],
    ['***foo**', '*<strong>foo</strong>'], ['**foo***', '<strong>foo</strong>*'],
    [String.raw`\***foo***`, '*<strong>foo</strong>*'],
    [String.raw`*foo\**`, '<em>foo*</em>'],
    [String.raw`__foo\___`, '<strong>foo_</strong>'],
  ]) assert.equal(render(source), '<p>' + expected + '</p>\n', source);
  for (const source of ['before ~one~ after', 'before ~~~three~~~ after', 'before ~~~~four~~~~ after']) {
    assert.equal(render(source), '<p>' + source + '</p>\n', source);
  }
});

test('code math HTML autolinks and link targets cannot donate formatting delimiters', () => {
  const assets = {has() { assert.fail('opaque content consulted an image'); }};
  for (const [marker, open, close] of [
    ['*', '<em>', '</em>'], ['**', '<strong>', '</strong>'],
    ['***', '<em><strong>', '</strong></em>'], ['___', '<em><strong>', '</strong></em>'],
    ['~~', '<del>', '</del>'],
  ]) {
    const codeText = marker + ' [hidden](/wrong) [^n]';
    const math = '$' + marker + ' ![hidden](wrong.png) [^n]$';
    const html = '<span title="' + marker + ' [hidden](/wrong)">';
    const url = 'https://example.test/' + marker;
    const source = `${marker}before \`${codeText}\` ${math} ${html} <${url}> [link](/target${marker}) after${marker}\n\n[^n]: Secret`;
    assert.equal(render(source, assets), `<p>${open}before <code>${codeText}</code> ${math} ${escape(html)} <a href="${url}">${url}</a> <a href="/target${marker}">link</a> after${close}</p>\n`, marker);
  }
});

test('link and image labels resolve their styles independently without crossing the label edge', () => {
  assert.equal(render('*outer [**label**](/url) tail*'),
    '<p><em>outer <a href="/url"><strong>label</strong></a> tail</em></p>\n');
  assert.equal(render('[*label **strong***](/url)'),
    '<p><a href="/url"><em>label <strong>strong</strong></em></a></p>\n');
  assert.equal(render('![*label **strong***](image.png)'), '<p><img src="image.png" alt="label strong"></p>\n');
  assert.equal(render('*[label*](/url)'), '<p>*<a href="/url">label*</a></p>\n');
  assert.equal(render('[label *](/url)tail*'), '<p><a href="/url">label *</a>tail*</p>\n');
});

test('nested styling works in headings lists tables and repeated footnotes', () => {
  const body = '*outer **strong** tail*', expected = '<em>outer <strong>strong</strong> tail</em>';
  for (const source of [`# ${body}`, `- ${body}`, `> ${body}`, `A|B\n---|---\n${body}|kept`, `Use[^n] again[^n]\n\n[^n]: ${body}`]) {
    const html = render(source);
    assert.ok(html.includes(expected), source);
    assert.equal((html.match(/<strong>strong<\/strong>/g) || []).length, 1, source);
  }
});

test('styles within the depth limit retain exact nesting and ordinary links', () => {
  const marker = '*'.repeat(126);
  assert.equal(render(marker + '[live](/target)' + marker),
    '<p>' + '<strong>'.repeat(63) + '<a href="/target">live</a>' + '</strong>'.repeat(63) + '</p>\n');
});

test('deeper spans remain complete literal source without publishing hidden atoms', () => {
  const marker = '*'.repeat(192), body = '<unsafe> [hidden](/wrong) ![hidden](wrong.png) [^n]';
  const bounded = vm.createContext({source: marker + body + marker + '\n\n[^n]: Secret', assets: {
    has() { assert.fail('depth fallback consulted an image binding'); },
  }});
  vm.runInContext(code, bounded);
  assert.equal(vm.runInContext('parseMarkdownClient(source, assets)', bounded, {timeout: 4000}),
    '<p>' + '<strong>'.repeat(64) + '*'.repeat(64) + escape(body) + '*'.repeat(64) + '</strong>'.repeat(64) + '</p>\n');
  assert.equal(vm.runInContext('parseMarkdownClient("*fresh*")', bounded), '<p><em>fresh</em></p>\n');
});

test('many unmatched and deeply paired delimiters complete without repeated suffix searches', () => {
  const bounded = vm.createContext({});
  vm.runInContext(code, bounded);
  for (const marker of ['*', '_', '**', '__']) {
    bounded.source = 'Before ' + (marker + 'word ').repeat(12000);
    // CommonMark 645 removes only the paragraph's final source space.
    assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}), '<p>' + bounded.source.slice(0, -1) + '</p>\n', marker);
  }
  const marker = '*'.repeat(16000);
  bounded.source = marker + 'deep' + marker;
  assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}),
    '<p>' + '<strong>'.repeat(64) + '*'.repeat(15872) + 'deep' + '*'.repeat(15872) + '</strong>'.repeat(64) + '</p>\n');
});

test('delimiter events and rendering retain independent work and structure admission', () => {
  const work = vm.createContext({plain: 'a b c '.repeat(2000), formatted: 'a *b* '.repeat(2000)});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 150000'), work);
  assert.equal(vm.runInContext('parseMarkdownClient(plain)', work, {timeout: 4000}), '<p>' + work.plain.slice(0, -1) + '</p>\n');
  assert.throws(() => vm.runInContext('parseMarkdownClient(formatted)', work, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("**fresh**")', work), '<p><strong>fresh</strong></p>\n');

  const structure = vm.createContext({source: '*x* '.repeat(3000) + '\n\n[hidden]: /wrong'});
  vm.runInContext(code.replace('structures: 262144', 'structures: 10000'), structure);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', structure, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'structure');
  assert.equal(vm.runInContext('parseMarkdownClient("[hidden] *fresh*")', structure), '<p>[hidden] <em>fresh</em></p>\n');
});
