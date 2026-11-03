// Preserve actual table cells and following blocks in the standalone renderer.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const header = '| Name | Value |\n|---|---|\n';

test('matched code pipes cannot split cells and silently discard following content', () => {
  const html = render(header + '| `a | b` | keep me |');
  assert.match(html, /<tr><td><code>a \| b<\/code><\/td><td>keep me<\/td><\/tr>/);
  assert.equal((html.match(/<td>/g) || []).length, 2);
});

test('code-bearing headers retain column counts and all alignment modes', () => {
  const html = render('| `left|right` | B | C | D |\n| :--- | ---: | :---: | --- |\n| a | b | c | d |');
  assert.match(html, /<th style="text-align:left"><code>left\|right<\/code><\/th>/);
  assert.match(html, /<th style="text-align:right">B<\/th>/);
  assert.match(html, /<th style="text-align:center">C<\/th>/);
  assert.match(html, /<td>d<\/td>/);
});

test('maximal backtick runs and escaped openers keep native table code boundaries', () => {
  for (const [literal, expected] of [
    ['``a`|b``', '<code>a`|b</code>'],
    ['`left\\`', '<code>left\\</code>'],
    ['\\``left|right`', '`<code>left|right</code>'],
    ['`a\\\\|b`', '<code>a\\|b</code>'],
  ]) {
    const html = render(header + `| ${literal} | right |`);
    assert.ok(html.includes(`<td>${expected}</td><td>right</td>`), literal + ': ' + html);
  }
});

test('unmatched backtick runs cannot hide separators or later cells', () => {
  for (const literal of ['`a', '``a`', 'a ` b `` c', 'a ``` b ``']) {
    assert.ok(render(header + `| ${literal} | right |`).includes(`<td>${literal}</td><td>right</td>`), literal);
  }
});

test('table-only pipe decoding propagates through formatted link and image labels', () => {
  const html = render(header + '| [**`a\\|b`**](/link) | ![`c\\|d`](image.png) |');
  assert.match(html, /<td><a href="\/link"><strong><code>a\|b<\/code><\/strong><\/a><\/td>/);
  assert.match(html, /<td><img src="image.png" alt="c\|d"><\/td>/);
  const prose = render('`a\\|b`\n\n' + header + '| `a\\|b` | right |\n\n`a\\|b`');
  assert.equal((prose.match(/<code>a\\\|b<\/code>/g) || []).length, 2);
  assert.equal((prose.match(/<code>a\|b<\/code>/g) || []).length, 1);
});

test('escaped pipes retain raw reference-label identity and single URL decoding', () => {
  const source = header + '| [ref\\|label] | ![alt][ref\\|label] |\n\n[ref\\|label]: /right\\|path "T &amp;amp;"';
  const html = render(source);
  assert.match(html, /<a href="\/right\|path" title="T &amp;amp;">ref\|label<\/a>/);
  assert.match(html, /<img src="\/right\|path" alt="alt" title="T &amp;amp;">/);
});

test('escaped final pipes and backslash parity retain actual cell contents', () => {
  for (const [row, expected] of [
    ['| x | y\\|', 'y|'], ['| x | y\\\\|', 'y\\'],
    ['| x | y\\\\\\|', 'y\\|'], ['| x | y\\|   ', 'y|'],
  ]) assert.ok(render(header + row).includes(`<td>x</td><td>${expected}</td>`), row);
  assert.match(render('A | B\\|\n--- | ---\nx | y'), /<th>B\|<\/th>/);
});

test('pipe-bearing indented code ends the table and remains literal', () => {
  for (const indent of ['    ', '\t']) {
    const html = render(header + indent + '**literal** | [ref]: /wrong\n\n[ref]');
    assert.match(html, /<\/table><\/div>\n<pre><code>\*\*literal\*\* \| \[ref\]: \/wrong\n<\/code><\/pre>/);
    assert.match(html, /<p>\[ref\]<\/p>\n$/);
    assert.doesNotMatch(html, /<strong>|href="\/wrong"/);
  }
});

test('pipe-bearing footnote definitions end the table and preserve their complete body', () => {
  const html = render(header + '[^n]: keep | the note\n\n    A second paragraph.\n\nUse[^n].');
  assert.match(html, /class="footnote-ref"/);
  assert.match(html, /<p>keep \| the note<\/p>/);
  assert.match(html, /<p>A second paragraph\.<\/p>/);
  assert.doesNotMatch(html, /<td>\[\^n\]/);
});

test('supported block openers after tables keep priority while malformed markers remain rows', () => {
  for (const [next, expected] of [
    ['# title | text', '<h1'], ['> quote | text', '<blockquote>'],
    ['- item | text', '<ul>'], ['2. item | text', '<ol start="2">'],
    ['~~~lang|pipe\nliteral\n~~~', '<pre><code class="language-lang|pipe">'],
  ]) assert.ok(render(header + next).includes('</table></div>\n' + expected), next);
  for (const row of ['\\# heading | value', '\\> quote | value', '\\- list | value', '#not-a-heading | value', '9999999999. not-a-list | value', '```bad`info | value']) {
    assert.match(render(header + row), /<td>value<\/td>/, row);
  }
});

test('reference-shaped rows remain cells but definitions following code retain their authority', () => {
  const literal = render(header + '[ref]: /hidden|left\n\n[ref]');
  assert.match(literal, /<td>\[ref\]: \/hidden<\/td><td>left<\/td>/);
  assert.match(literal, /<p>\[ref\]<\/p>\n$/);
  const html = render(header + '```lang|pipe\n[ref]: /hidden\n```\n[ref]: /visible\n\n[ref]');
  assert.match(html, /\[ref\]: \/hidden\n<\/code><\/pre>/);
  assert.match(html, /<p><a href="\/visible">ref<\/a><\/p>\n$/);
});

test('nested quotes, lists and footnotes use the same table cell ownership', () => {
  const body = header + '| `a|b` | keep me |';
  for (const source of [
    body.split('\n').map(line => '> ' + line).join('\n'),
    '- ' + body.replace(/\n/g, '\n  '),
    'Use[^n].\n\n[^n]: ' + body.replace(/\n/g, '\n    '),
  ]) assert.match(render(source), /<td><code>a\|b<\/code><\/td><td>keep me<\/td>/);
});

test('a table-ending note or code block cannot capture dedented quote continuation', () => {
  const quoted = header.split('\n').filter(Boolean).map(line => '> ' + line).join('\n');
  const note = render(quoted + '\n> [^n]: keep | note\n[ref]: /right\n\n[ref] [^n]');
  assert.match(note, /<a href="\/right">ref<\/a>/);
  assert.match(note, /<p>keep \| note<\/p>/);
  const code = render(quoted + '\n>     literal | code\n[ref]: /right\n\n[ref]');
  assert.match(code, /<pre><code>literal \| code\n<\/code><\/pre>/);
  assert.match(code, /<a href="\/right">ref<\/a>/);
});

test('ordinary ragged rows and literal marker-looking headers retain their shape', () => {
  const html = render(header + '| a |\n| b | c | excess |');
  assert.match(html, /<td>a<\/td><td><\/td>/);
  assert.match(html, /<td>b<\/td><td>c<\/td>/);
  assert.doesNotMatch(html, /excess/);
  for (const marker of ['- ', '+ ', '* ', '# ', '> ']) {
    assert.match(render(`| ${marker}Name | Value |\n--- | ---`), /<th>Value<\/th>/);
  }
});

test('many code-pipe rows stay bounded and a failed table render resets state', () => {
  const source = header + '| ``a`|b`` | keep |\n'.repeat(3000);
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000});
  assert.equal((html.match(/<td>keep<\/td>/g) || []).length, 3000);
  const small = vm.createContext({source});
  vm.runInContext(code.replace('structures: 262144', 'structures: 1000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}), error => error.code === 'FMD_RENDER_LIMIT');
  assert.match(vm.runInContext('parseMarkdownClient("Name | Value\\n--- | ---\\nx | y")', small), /<td>x<\/td><td>y<\/td>/);
});

test('a single delimiter-heavy table row obeys the shared structure limit', () => {
  const source = header + '| ' + '`a|b` '.repeat(600) + '| right |';
  const small = vm.createContext({source});
  vm.runInContext(code.replace('structures: 262144', 'structures: 1000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}), error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'structure');
  assert.equal(vm.runInContext('parseMarkdownClient("Fresh")', small), '<p>Fresh</p>\n');
});
