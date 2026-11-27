import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = (source, profile) => context.parseMarkdownClient(source, undefined, profile);
const fixture = readFileSync(new URL('./fixtures/financial_statement.md', import.meta.url), 'utf8');
const statement = (...rows) => ['| Item | Amount |', '| --- | ---. |', ...rows].join('\n');

test('editing the shipped financial statement retains its table, totals, styles and link', () => {
  for (const profile of [undefined, 'commonmark-gfm', 'gfm-plus']) {
    const html = render(fixture, profile);
    assert.match(html, /<table class="fmd-financial-table"[ >]/);
    assert.equal(html.split('<tr>').length - 1, 4);
    assert.equal(html.split('<th scope="col"').length - 1, 2);
    assert.match(html, /href="https:\/\/example.com\/revenue"/);
    assert.match(html, /border-bottom:1px solid currentColor/);
    assert.match(html, /border-bottom:3px double currentColor/);
    assert.match(html, /<strong>/);
    assert.doesNotMatch(html, />(?:---|===)</);
  }
});

test('markers combine on the prior visible row; styled or mixed markers remain data', () => {
  const html = render(statement('| | === |', '| Balance | 10 |', '| --- | --- |',
    '| | &#61;&#61;&#61; | ignored overflow cell |', '| | --- |',
    '| | **---** |', '| === | pending |', '| | `---` |'));
  assert.equal(html.split('<tr>').length - 1, 6);
  assert.match(html, />===<\/td>/);
  assert.match(html, /border-bottom:1px solid currentColor">Balance/);
  assert.match(html, /text-align:right;border-bottom:3px double currentColor"/);
  assert.match(html, /<strong>---<\/strong>/);
  assert.match(html, /<code>---<\/code>/);
});

test('ordinary alignments and malformed decimal delimiters retain their prior meaning', () => {
  const ordinary = render('| C | D |\n| :---: | ---: |\n| A | B |\n| | === |');
  assert.match(ordinary, /<table>/);
  assert.match(ordinary, /style="text-align:center"/);
  assert.match(ordinary, />===<\/td>/);
  for (const delimiter of ['--.', ':---.', '---.:', '---..', '--- .']) {
    assert.doesNotMatch(render('| A | B |\n| --- | ' + delimiter + ' |'), /<table/);
  }
});

test('each decimal column shares its own suffix track across mixed precision and styles', () => {
  const html = render('| Year | Amount | Rate |\n| --- | ---. | ----. |\n'
    + '| A | [1,234.5](#detail) | **.12789%** |\n| B | (25.50) | 9 |\n'
    + '| C | **20**.1 | 1e3 |\n| D | 1\'234 | `0.10` |');
  assert.match(html, /--fmd-suffix-1:4rem;--fmd-suffix-2:7rem/);
  assert.equal(html.split('var(--fmd-suffix-1)').length - 1, 4);
  assert.equal(html.split('var(--fmd-suffix-2)').length - 1, 2);
  assert.match(html, /href="#detail"><span class="fmd-decimal"/);
  assert.match(html, /class="fmd-decimal-before"><strong>20<\/strong><\/span><span class="fmd-decimal-after">\.1/);
  assert.match(html, /class="fmd-decimal-before">1&#39;234<\/span><span class="fmd-decimal-after"><\/span>/);
  assert.match(html, />1e3<\/td>/);
  assert.match(html, /<code>0\.10<\/code>/);
  const partial = render(statement('| Row | 1[2.3](#detail)4 |'));
  assert.doesNotMatch(partial, /class="fmd-decimal"/);
  assert.match(partial, /1<a href="#detail">2\.3<\/a>4/);
});

test('trailing footnotes occupy the suffix and keep their original reference identities', () => {
  const html = render(statement('| A | **12.30[^a]** |', '| B | [7](#detail)[^a] |',
    '| C | 12[^a].30 |', '| D | 12.30[^missing] |') + '\n\n[^a]: Audit note.');
  assert.equal(html.split('class="fmd-decimal"').length - 1, 2);
  assert.match(html, /class="fmd-decimal-after">\.30<sup class="footnote-ref" id="fnref-1">/);
  assert.match(html, /class="fmd-decimal-after"><sup class="footnote-ref" id="fnref-1-2">/);
  assert.equal(html.split('role="doc-backlink"').length - 1, 3);
  assert.equal(html.split('href="#detail"').length - 1, 1);
});

test('numbers retain grouping, accounting and signs while prose and malformed amounts remain intact', () => {
  for (const [value, prefix, suffix] of [['−€ 12.50%', '−€ 12', '.50%'],
    ['(1\u202f234.5)', '(1\u202f234', '.5)'], ['-7.', '-7', '.'], ['.125', '', '.125']]) {
    const html = render(statement('| Row | ' + value + ' |'));
    assert.ok(html.includes('class="fmd-decimal-before">' + prefix + '</span><span class="fmd-decimal-after">' + suffix + '</span>'));
  }
  for (const value of ['1,23.40', '1,234 567', '(12', '12kg', '10.3.4', '££10.3', '**1**kg']) {
    assert.doesNotMatch(render(statement('| Row | ' + value + ' |')), /class="fmd-decimal"/);
  }
});
