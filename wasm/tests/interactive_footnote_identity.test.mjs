// Footnote identity and body ownership use the production offline renderer.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const noteCount = html => (html.match(/<li id="fn-/g) || []).length;
const footer = html => html.slice(html.indexOf('<section class="footnotes"'));

test('case-distinct ASCII and Unicode footnotes preserve every separate body', () => {
  for (const [first, second] of [['A', 'a'], ['É', 'é'], ['ẞ', 'ß'], ['Σ', 'σ'], ['İ', 'i\u0307']]) {
    for (const [a, b] of [[first, second], [second, first]]) {
      const source = `One[^${a}] two[^${b}].\n\n[^${a}]: FIRST body\n[^${b}]: SECOND body`;
      const html = render(source);
      assert.equal(noteCount(html), 2, source);
      assert.match(footer(html), /FIRST body/);
      assert.match(footer(html), /SECOND body/);
      assert.match(html, /role="doc-noteref">1<\/a>/);
      assert.match(html, /role="doc-noteref">2<\/a>/);
    }
  }
});

test('a differently cased reference remains literal when its exact definition is absent', () => {
  for (const [label, definition] of [['A', 'a'], ['É', 'é'], ['Σ', 'σ']]) {
    const html = render(`Unknown[^${label}].\n\n[^${definition}]: hidden`);
    assert.equal(html, `<p>Unknown[^${label}].</p>\n`);
  }
});

test('exact duplicate definitions still keep the first body and repeated backlinks', () => {
  const html = render('One[^A] again[^A] other[^a].\n\n[^A]: FIRST\n[^A]: ignored duplicate\n[^a]: SECOND');
  assert.equal(noteCount(html), 2);
  assert.doesNotMatch(html, /ignored duplicate/);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 3);
  assert.match(footer(html), /FIRST/);
  assert.match(footer(html), /SECOND/);
});

test('case-distinct cyclic notes terminate with unique IDs and complete backlinks', () => {
  const html = render('# fn-1\n\n# fnref-1\n\nOne[^A] two[^a].\n\n[^A]: Upper to lower[^a].\n[^a]: Lower to upper[^A].');
  assert.equal(noteCount(html), 2);
  assert.match(footer(html), /Upper to lower/);
  assert.match(footer(html), /Lower to upper/);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 4);
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const [, destination] of html.matchAll(/href="#([^"]+)"/g)) assert.ok(ids.includes(destination), destination);
});

test('link references retain full folding while note identifiers remain exact', () => {
  const html = render('[guide][STRASSE] upper[^A] lower[^a].\n\n[Straße]: /guide\n[^A]: UPPER\n[^a]: lower');
  assert.match(html, /<a href="\/guide">guide<\/a>/);
  assert.equal(noteCount(html), 2);
});

test('multiple blank lines keep continued paragraphs inside the footnote', () => {
  for (const separator of ['\n\n\n', '\n  \n\t\n\n']) {
    const html = render(`Use[^n].\n\n[^n]: first${separator}    **second**\n\nOutside.`);
    assert.match(footer(html), /<p>first<\/p>\n<p><strong>second<\/strong><\/p>/);
    assert.ok(html.indexOf('Outside.') < html.indexOf('<section class="footnotes"'));
    assert.doesNotMatch(html.slice(0, html.indexOf('<section class="footnotes"')), /second/);
  }
});

test('rich note blocks and global reference definitions survive multiple blank lines', () => {
  const html = render('Use[^n] [guide][r].\n\n[^n]: first\n\n\n    [r]: /guide "Title"\n\n\n    > quoted\n\n\n    - item\n\n\n    ![note picture][r]\n\n\n    | H |\n    | --- |\n    | V |');
  assert.match(html, /<a href="\/guide" title="Title">guide<\/a>/);
  const note = footer(html);
  assert.match(note, /<blockquote>\n<p>quoted<\/p>/);
  assert.match(note, /<li>item<\/li>/);
  assert.match(note, /<img src="\/guide" alt="note picture" title="Title">/);
  assert.match(note, /<th>H<\/th>/);
  assert.match(note, /<td>V<\/td>/);
});

test('multiple blank lines and reference-shaped text inside note code remain literal', () => {
  const html = render('Use[^n] [fake].\n\n[^n]: ```text\n    first\n\n\n    [fake]: /hidden\n    ```\n\nOutside.');
  assert.match(footer(html), /<code class="language-text">first\n\n\n\[fake\]: \/hidden\n<\/code>/);
  assert.doesNotMatch(html, /href="\/hidden"/);
  assert.ok(html.indexOf('Outside.') < html.indexOf('<section class="footnotes"'));
});

test('four-column continuations include mixed spaces and tabs without losing extra indent', () => {
  for (const indent of ['    ', '\t', '  \t', ' \t']) {
    const html = render(`Use[^n].\n\n[^n]: first\n\n\n${indent}second`);
    assert.match(footer(html), /<p>second<\/p>/, JSON.stringify(indent));
  }
  assert.match(footer(render('Use[^n].\n\n[^n]: first\n\n\n\t\tcode')),
    /<pre><code>code\n<\/code><\/pre>/);
});

test('container-nested footnotes retain blank-separated bodies', () => {
  const body = '[^n]: first\n\n\n    second';
  for (const definition of [body.split('\n').map(line => '> ' + line).join('\n'), '- ' + body.replace(/\n/g, '\n  ')]) {
    const html = render('Use[^n].\n\n' + definition);
    assert.match(footer(html), /<p>first<\/p>\n<p>second<\/p>/);
  }
});

test('blank runs followed by outside content end the note without swallowing later definitions', () => {
  const html = render('Use[^A] and[^a] [real].\n\n[^A]: UPPER\n\n\nOutside.\n\n[^a]: lower\n\n\n[real]: /real');
  assert.equal(noteCount(html), 2);
  assert.match(html, /<a href="\/real">real<\/a>/);
  assert.ok(html.indexOf('<p>Outside.</p>') < html.indexOf('<section class="footnotes"'));
  assert.doesNotMatch(footer(html), /Outside\./);
});

test('long note blank runs are consumed once and document state remains local', () => {
  const bounded = vm.createContext({input: 'Use[^n].\n\n[^n]: first\n' + '\n'.repeat(20000) + '    tail'});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(input)', bounded, {timeout: 4000});
  assert.match(footer(html), /<p>tail<\/p>/);
  assert.equal(render('Unknown[^A] unknown[^a].'), '<p>Unknown[^A] unknown[^a].</p>\n');
});
