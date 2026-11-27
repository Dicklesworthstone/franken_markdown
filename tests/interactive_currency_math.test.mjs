// Run with: node --test tests/interactive_currency_math.test.mjs
// Exercise the production standalone parser; it preserves math as opaque source.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = source => context.parseMarkdownClient(source);

test('financial prose preserves dollar amounts, bold text and the later formula', () => {
  const source = 'Screen of $5 million–$50 million. Thus $0.470 becomes **$0.260 of UFCF**. Costs $5 and $x = 1$.';
  assert.equal(render(source), '<p>Screen of $5 million–$50 million. Thus $0.470 becomes <strong>$0.260 of UFCF</strong>. Costs $5 and $x = 1$.</p>\n');
});

test('price candidates cannot hide real links or emphasis in intervening prose', () => {
  const source = '$5 and [guide](/target) **$0.260 of UFCF**. Costs $5 and $x **literal**$.';
  assert.equal(render(source), '<p>$5 and <a href="/target">guide</a> <strong>$0.260 of UFCF</strong>. Costs $5 and $x **literal**$.</p>\n');
});

test('a rejected dollar leaves the next formula available to own its contents', () => {
  assert.equal(render('$5 and **kept** $x **literal**$ plus $10 and **kept too** $y$.'),
    '<p>$5 and <strong>kept</strong> $x **literal**$ plus $10 and <strong>kept too</strong> $y$.</p>\n');
  assert.equal(render('$x **kept** $y **literal**$'),
    '<p>$x <strong>kept</strong> $y **literal**$</p>\n');
});

test('single-dollar closers reject ASCII digits while valid numeric math remains opaque', () => {
  assert.equal(render('$x **kept**$2'), '<p>$x <strong>kept</strong>$2</p>\n');
  for (const formula of ['2', '2+2=4', '-3.5', '2|x']) {
    const source = '$' + formula + ' **literal**$';
    assert.equal(render(source), '<p>' + source + '</p>\n');
  }
  for (const suffix of ['suffix', '−2', '²', '٢']) {
    const source = '$x **literal**$' + suffix;
    assert.equal(render(source), '<p>' + source + '</p>\n');
  }
  assert.equal(render('$\u00a0x **literal**\u00a0$'), '<p>$\u00a0x **literal**\u00a0$</p>\n');
});

test('escaped dollars and prices in TeX groups remain opaque math content', () => {
  for (const formula of [
    String.raw`a\$5 + **literal**`,
    String.raw`\text{Price $5} + **literal**`,
    String.raw`\text{Price {USD $5} and $10} + **literal**`,
    String.raw`\text{escaped \} Price $5} + **literal**`,
  ]) {
    const source = '$' + formula + '$';
    assert.equal(render(source), '<p>' + source + '</p>\n');
  }
  assert.equal(render(String.raw`Price \$5 and **kept** $x **literal**$`),
    '<p>Price $5 and <strong>kept</strong> $x **literal**$</p>\n');
});

test('escaped braces and groups opened before a candidate do not hide later formulas', () => {
  assert.equal(render(String.raw`$\{price **kept** $x **literal**$`),
    '<p>${price <strong>kept</strong> $x **literal**$</p>\n');
  assert.equal(render('{Cost $5 and **kept** $x **literal**$}'),
    '<p>{Cost $5 and <strong>kept</strong> $x **literal**$}</p>\n');
});

test('display math, code and table price columns retain their existing ownership', () => {
  assert.equal(render('Value $$x **literal**$$2 and `price $5–$50` then $y$.'),
    '<p>Value $$x **literal**$$2 and <code>price $5–$50</code> then $y$.</p>\n');
  const html = render('Price|Value|Formula\n---|---|---\n$12.50|$19.95|$x|y$\n');
  assert.deepEqual([...html.matchAll(/<td>(.*?)<\/td>/g)].map(match => match[1]),
    ['$12.50', '$19.95', '$x|y$']);
});

test('many price candidates preserve later links and formulas within the render budget', () => {
  const prices = '$5–$50 '.repeat(4096);
  assert.equal(render(prices + '[guide](/target) and $x **literal**$.'),
    '<p>' + prices + '<a href="/target">guide</a> and $x **literal**$.</p>\n');
  const grouped = '$5{'.repeat(4096) + '}'.repeat(4096);
  assert.equal(render(grouped + ' **kept**'), '<p>' + grouped + ' <strong>kept</strong></p>\n');
});

test('braced TeX prices keep header and body pipes inside the formula', () => {
  for (const formula of [
    String.raw`$P(A|B)+\text{USD $5}$`,
    String.raw`$\text{{USD $5} and $10}|x$`,
    String.raw`$\text{escaped \} $5}|x$`,
  ]) {
    const html = render(formula + '|Link\r\n---|---\r\n' + formula + '|[café](/réel)\r\n');
    assert.ok(html.includes('<th>' + formula + '</th><th>Link</th>'), html);
    assert.ok(html.includes('<td>' + formula + '</td><td><a href="/réel">café</a></td>'), html);
  }
});

test('a brace opened before a price cannot let it borrow another cell', () => {
  for (const first of ['{Cost $12', String.raw`\{Cost $12`, String.raw`$\{cost`]) {
    const html = render('Left|Price|Formula|Link\n---|---|---|---\n' + first + ' | $5 | $x|y$ | [go](/ok)');
    const cells = [...html.matchAll(/<td>(.*?)<\/td>/g)].map(match => match[1]);
    assert.deepEqual(cells, [first.replace('\\{', '{'), '$5', '$x|y$', '<a href="/ok">go</a>']);
  }
});

test('unclosed braces cannot merge price columns with a following formula', () => {
  for (const left of ['$10 {', String.raw`$10 \text{`, '$10 {nested {$3}', String.raw`$10 {escaped \}`]) {
    const html = render('Price|Other|Formula|Link\n---|---|---|---\n' + left + ' | $20 | $x$ | [go](/ok)');
    const cells = [...html.matchAll(/<td>(.*?)<\/td>/g)].map(match => match[1]);
    assert.deepEqual(cells, [left.replace('\\}', '}'), '$20', '$x$', '<a href="/ok">go</a>']);
  }
});
