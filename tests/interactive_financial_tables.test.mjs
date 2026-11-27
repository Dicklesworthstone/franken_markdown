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
    assert.match(html, /<table class="fmd-financial-table">/);
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
