// Production saved-document parsing. Native admission is independently pinned
// in tests/parser_math_boundaries.rs and tests/parser_table_math.rs; this smaller
// renderer must preserve source, not activate Markdown hidden inside formulas.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
const cells = html => [...html.matchAll(/<td(?: [^>]*)?>(.*?)<\/td>/gs)].map(match => match[1]);
const headers = html => [...html.matchAll(/<th(?: [^>]*)?>(.*?)<\/th>/gs)].map(match => match[1]);
const table = row => '| Formula | Result | Link |\n| --- | --- | --- |\n' + row;
const literalBlock = source => '<pre class="math-source">' + escape(source) + '</pre>\n';

test('inline formulas preserve every source character without granting links notes or images', () => {
  const assets = {has() { assert.fail('formula source consulted an image binding'); }};
  for (const formula of [
    '$[ghost](/hidden)$', '$![hidden](asset.png)$', '$[^hidden]$',
    '$<https://hidden.test>$', '$<b title="[ghost](/hidden)">x</b>$',
    String.raw`$\text{a\{b\}} &amp; *bold* _sub_ ~~old~~$`,
    '$`[ghost](/hidden)`$', '$$[ghost](/hidden)|x$$',
  ]) {
    const source = `Before ${formula} after.\n\n[^hidden]: Secret note`;
    assert.equal(render(source, assets), `<p>Before ${escape(formula)} after.</p>\n`, formula);
  }
  assert.equal(render('Before $a\nb$ after.'), '<p>Before $a\nb$ after.</p>\n');
});

test('math label ownership cannot retarget links or publish hidden image resources', () => {
  assert.equal(render('[$[ghost](/hidden)$](/visible)'),
    '<p><a href="/visible">$[ghost](/hidden)$</a></p>\n');
  const seen = [];
  const assets = {has(url) { seen.push(url); return false; }};
  assert.equal(render('![$![hidden](wrong.png)$](visible.png)', assets),
    '<p><img src="visible.png" alt="$![hidden](wrong.png)$"></p>\n');
  assert.deepEqual(seen, ['visible.png']);
  assert.equal(render('[doc](/literal$amount$ "price $x$")'),
    '<p><a href="/literal$amount$" title="price $x$">doc</a></p>\n');
  assert.equal(render('[doc][r$x$]\n\n[r$x$]: /real'), '<p><a href="/real">doc</a></p>\n');
});

test('escaped dollars code and raw HTML retain priority while invalid math remains Markdown', () => {
  assert.equal(render(String.raw`\$[live](/yes)\$`), '<p>$<a href="/yes">live</a>$</p>\n');
  assert.equal(render('`$[hidden](/wrong)$` [live](/yes)'),
    '<p><code>$[hidden](/wrong)$</code> <a href="/yes">live</a></p>\n');
  assert.equal(render('Before <span title="$">[live](/yes)$ after'),
    '<p>Before &lt;span title=&quot;$&quot;&gt;<a href="/yes">live</a>$ after</p>\n');
  for (const formula of ['$ [live](/yes)$', '$\t[live](/yes)$', '$[live](/yes) $', '$unclosed [live](/yes)']) {
    assert.match(render('Before ' + formula), /<a href="\/yes">live<\/a>/, formula);
  }
  for (const space of ['\u00a0', '\u2003', '\ufeff']) {
    const formula = `$${space}[hidden](/wrong)${space}$`;
    assert.equal(render('Before ' + formula), '<p>Before ' + formula + '</p>\n');
  }
});

test('mixed and long dollar runs retain native suffix and escaped-closer ownership', () => {
  for (const formula of [
    '$$$[hidden](/wrong)$$', '$$$$[hidden](/wrong)$$', '$$[hidden](/wrong)$',
    '$x$$[hidden](/wrong)$', '$x$$$[hidden](/wrong)$',
    String.raw`$a\$[hidden](/wrong)$`, String.raw`$$a\$$[hidden](/wrong)$$`,
    '$x $[hidden](/wrong)$',
  ]) {
    assert.equal(render('Before ' + formula), '<p>Before ' + formula + '</p>\n', formula);
  }
  assert.equal(render(String.raw`Before \$$[hidden](/wrong)$`), '<p>Before $$[hidden](/wrong)$</p>\n');
});

test('formula pipes preserve complete table cells and following links', () => {
  for (const formula of [
    '$P(A|B)$', '$x | y$', String.raw`$$\left|x\right|$$`, String.raw`$\lVert x|y\rVert$`,
    '$[hidden](/wrong)|x$', '$$<b>|&amp;</b>$$', '$x$$y|z$', '$$$x|y$$',
  ]) {
    const html = render(table(`| ${formula} | retained-data | [go](/visible) |`));
    assert.deepEqual(cells(html), [escape(formula), 'retained-data', '<a href="/visible">go</a>'], formula);
    assert.doesNotMatch(html, /href="\/wrong"/, formula);
  }
});

test('formula headers and formatted labels use the same complete table boundaries', () => {
  const source = '$P(A|B)$ | $$x|y$$\n--- | ---\n$C|D$ | retained-data';
  const html = render(source);
  assert.deepEqual(headers(html), ['$P(A|B)$', '$$x|y$$']);
  assert.deepEqual(cells(html), ['$C|D$', 'retained-data']);
  for (const [formula, expected] of [
    ['**$P(A|B)$**', '<strong>$P(A|B)$</strong>'],
    ['[$P(A|B)$](/target)', '<a href="/target">$P(A|B)$</a>'],
    ['![$P(A|B)$](image.png)', '<img src="image.png" alt="$P(A|B)$">'],
  ]) {
    assert.deepEqual(cells(render(table(`| ${formula} | retained | end |`))), [expected, 'retained', 'end']);
  }
});

test('escaped dollars and backtick precedence preserve both table source and column count', () => {
  for (const [formula, expected] of [
    ['`$a|b$`', '<code>$a|b$</code>'],
    ['``$a`|b$``', '<code>$a`|b$</code>'],
    [String.raw`\$not-math\|still-text\$`, '$not-math|still-text$'],
    [String.raw`$a\$|b$`, String.raw`$a\$|b$`],
    [String.raw`$a\\$`, String.raw`$a\\$`],
    ['$`a|b`$', '$`a|b`$'],
    [String.raw`$a\|b$`, String.raw`$a\|b$`],
    [String.raw`$$a\$$|b$$`, String.raw`$$a\$$|b$$`],
    [String.raw`\$$a|b$`, '$$a|b$'],
  ]) {
    assert.deepEqual(cells(render(table(`| ${formula} | retained | end |`))), [expected, 'retained', 'end'], formula);
  }
});

test('currency and invalid formula edges cannot merge columns or borrow a later closer', () => {
  for (const separator of [' | ', '|']) {
    for (const prices of [['$12.50', '$19.95'], ['$-12.50', '$+19.95']]) {
      for (const formula of ['$P(A|B)$', '$2|x$', '$|x|$', '$$P(A|B)$$']) {
        const row = [...prices, formula, 'retained'].join(separator);
        const source = `Price A|Price B|Formula|Result\n---|---|---|---\n${row}`;
        assert.deepEqual(cells(render(source)), [...prices, formula, 'retained'], row);
      }
    }
  }
  for (const [left, right] of [
    ['$', 'remaining'], ['$ x', 'y$'], ['$\tx', 'y$'], ['$x', 'y $'],
    ['$$ x', 'y $$'], ['$unclosed', 'remaining'], ['$12.50', '$'],
  ]) {
    assert.deepEqual(cells(render(`Left|Right\n---|---\n| ${left} | ${right} |`)), [left, right]);
  }
  for (const previous of ['$unclosed', '$12.50', '$-12.50']) {
    for (const formula of ['$P(A|B)$', String.raw`$\alpha|x$`, '$π|θ$']) {
      assert.deepEqual(cells(render(`Left|Formula|Result\n---|---|---\n${previous}|${formula}|retained`)),
        [previous, formula, 'retained']);
    }
  }
  assert.deepEqual(cells(render('Formula|Result\n---|---\n$12.50\\|$|retained')),
    [String.raw`$12.50\|$`, 'retained']);
});

test('display math owns blank lines definitions and complete escaped source', () => {
  const formula = '$$\nx_i + y_i\n\n[ref]: /hidden\n[^secret]: hidden note\n<img src=x> &amp;\n$$';
  assert.equal(render(formula + '\n\n[ref] [^secret]'), literalBlock(formula) + '<p>[ref] [^secret]</p>\n');
  assert.equal(render('$$\nx\n\n[same]: /hidden\n$$\n[same]: /real\n\n[same]'),
    literalBlock('$$\nx\n\n[same]: /hidden\n$$') + '<p><a href="/real">same</a></p>\n');
});

test('all native display delimiter forms release immediately following real definitions', () => {
  for (const formula of [
    '$$ x + y $$', '$$$$', '$$\nx + y\n$$', '$$ x + y\n$$', '$$\nx + y $$',
    '   $$\nx + y\n   $$', '\u2003$$ x + y $$', '\u0085$$ x + y $$\u0085',
  ]) {
    assert.equal(render(`${formula}\n[real]: /target\n\n[real]`),
      literalBlock(formula) + '<p><a href="/target">real</a></p>\n', formula);
  }
  assert.equal(render('```math\nx_i + [hidden](/wrong)\n```\n[real]: /target\n\n[real]'),
    '<pre><code class="language-math">x_i + [hidden](/wrong)\n</code></pre>\n<p><a href="/target">real</a></p>\n');
});

test('nested math keeps definitions literal inside lists quotes and footnotes', () => {
  const formula = '$$\nx\n\n[fake]: /hidden\n[^hidden]: Secret\n$$';
  const body = formula + '\n[real]: /target\n\n[fake] [real]';
  for (const container of [
    body.split('\n').map(line => '> ' + line).join('\n'),
    '- ' + body.replaceAll('\n', '\n  '),
    'Visible[^n]\n\n[^n]: ' + body.replaceAll('\n', '\n    '),
  ]) {
    const html = render(container + '\n\nOutside [fake] [real] [^hidden]');
    assert.ok(html.includes(literalBlock(formula)), container);
    assert.doesNotMatch(html, /href="\/hidden"/, container);
    assert.equal((html.match(/href="\/target"/g) || []).length, 2, container);
    assert.match(html, /Outside \[fake\] <a href="\/target">real<\/a> \[\^hidden\]/, container);
  }
});

test('unterminated math stops at container boundaries and never publishes its hidden definitions', () => {
  assert.equal(render('[outside]\n\n$$\nx\n\n[outside]: /hidden'),
    '<p>[outside]</p>\n' + literalBlock('$$\nx\n\n[outside]: /hidden'));
  for (const container of [
    '> $$\n> x\n>\n> [fake]: /hidden\n',
    '- $$\n  x\n\n  [fake]: /hidden\n',
    'Visible[^n]\n\n[^n]: $$\n    x\n\n    [fake]: /hidden\n',
  ]) {
    const html = render(container + '[real]: /target\n\n[fake] [real]');
    assert.ok(html.includes(literalBlock('$$\nx\n\n[fake]: /hidden').trimEnd()), container);
    assert.match(html, /<a href="\/target">real<\/a>/, container);
    assert.doesNotMatch(html, /href="\/hidden"/, container);
  }
  for (const container of ['> $$ x $$', '- $$ x $$']) {
    assert.ok(render(container + '\noutside').endsWith('<p>outside</p>\n'), container);
  }
});

test('math block admission leaves paragraph continuation and indented code in their own scope', () => {
  for (const prefix of ['Before\n$$ x + y $$', '$$x$$ trailing', '\ufeff$$x$$ trailing']) {
    const source = prefix + '\n[not]: /literal\n\n[not]';
    const html = render(source);
    assert.doesNotMatch(html, /math-source|href="\/literal"/, source);
    assert.match(html, /\[not\]: \/literal/, source);
    assert.ok(html.endsWith('<p>[not]</p>\n'), source);
  }
  assert.equal(render('    $$\n    [fake]: /literal\n    $$\n[real]: /target\n\n[fake] [real]'),
    '<pre><code>$$\n[fake]: /literal\n$$\n</code></pre>\n<p>[fake] <a href="/target">real</a></p>\n');
  assert.equal(render('Before\n\n\ufeff$$x$$\n[not]: /literal\n\n[not]'),
    '<p>Before</p>\n<p>\ufeff$$x$$\n[not]: /literal</p>\n<p>[not]</p>\n');
});

test('display math terminates tables without swallowing later tables or container boundaries', () => {
  const source = 'A|B\n---|---\none|two\n$$\nx|y\n\n[hidden]: /wrong\n$$\n[real]: /target\n\nC|D\n---|---\n[real]|last';
  const html = render(source);
  assert.equal((html.match(/<table>/g) || []).length, 2);
  assert.deepEqual(cells(html), ['one', 'two', '<a href="/target">real</a>', 'last']);
  assert.ok(html.includes('</table></div>\n' + literalBlock('$$\nx|y\n\n[hidden]: /wrong\n$$')));
  const quoted = render('> ' + source.replaceAll('\n', '\n> ') + '\noutside');
  assert.ok(quoted.endsWith('</blockquote>\n<p>outside</p>\n'));
});

test('Unicode and all logical line endings preserve math cells and source boundaries', () => {
  for (const newline of ['\n', '\r\n', '\r']) {
    const source = '$$\nα + β\n\n[faux]: /fantôme\n$$\n[real]: /café\n\nFormula|Result\n---|---\n$\\text{图}|é$|[real]';
    const html = render(source.replaceAll('\n', newline));
    assert.ok(html.startsWith(literalBlock('$$\nα + β\n\n[faux]: /fantôme\n$$')));
    assert.deepEqual(cells(html), [String.raw`$\text{图}|é$`, '<a href="/café">real</a>']);
    assert.doesNotMatch(html, /href="\/fantôme"/);
  }
});

test('large unmatched runs and price tables use bounded indexes and preserve complete source', () => {
  const bounded = vm.createContext({});
  vm.runInContext(code, bounded);
  for (const source of ['$x '.repeat(16000).trimEnd(), '$'.repeat(64000) + 'tail', 'a' + ' '.repeat(100000) + 'b']) {
    bounded.input = 'Before ' + source;
    assert.equal(vm.runInContext('parseMarkdownClient(input)', bounded, {timeout: 4000}), '<p>Before ' + source + '</p>\n');
  }
  const count = 512;
  bounded.input = Array(count).fill('Price').join('|') + '\n' + Array(count).fill('---').join('|')
    + '\n' + Array(count).fill('$12.50').join('|');
  assert.deepEqual(cells(vm.runInContext('parseMarkdownClient(input)', bounded, {timeout: 4000})), Array(count).fill('$12.50'));
  bounded.input = '$$\n' + '\n'.repeat(12000) + '[hidden]: /wrong\n$$\n\n[hidden]';
  const html = vm.runInContext('parseMarkdownClient(input)', bounded, {timeout: 4000});
  assert.ok(html.endsWith('</pre>\n<p>[hidden]</p>\n'));
  assert.doesNotMatch(html, /href=/);
});

test('math indexes share admission limits and do not retain definitions after a failed render', () => {
  const bounded = vm.createContext({input: 'Before ' + '$x$ '.repeat(6000) + '\n\n[leaked]: /wrong'});
  vm.runInContext(code.replace('structures: 262144', 'structures: 10000'), bounded);
  assert.throws(() => vm.runInContext('parseMarkdownClient(input)', bounded, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'structure');
  assert.equal(vm.runInContext('parseMarkdownClient("[leaked] $[hidden](/wrong)$")', bounded),
    '<p>[leaked] $[hidden](/wrong)$</p>\n');
});
