// Match the shared native parser's bracket-link precedence after image labels
// are flattened. Execute the renderer embedded in actual standalone exports.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);

test('bracket links in image descriptions retire earlier ordinary link openers', () => {
  for (const [label, alt] of [
    ['alt [inner](/inside)', 'alt inner'],
    ['alt **[inner](/inside)**', 'alt inner'],
    ['alt [inner](/inside "caption [ ]")', 'alt inner'],
    ['alt [^inner](/inside)', 'alt ^inner'],
  ]) {
    assert.equal(render(`[![${label}](image.png)](/outside)`),
      `<p>[<img src="image.png" alt="${alt}">](/outside)</p>\n`, label);
  }
});

test('reference links and reference images retain the same delimiter precedence', () => {
  for (const inner of ['[inner][ref]', '[inner][]', '[inner]']) {
    for (const suffix of ['(image.png)', '[picture]']) {
      const source = `[![alt ${inner}]${suffix}](/outside)\n\n[ref]: /inside\n[inner]: /inside\n[picture]: image.png\n`;
      assert.equal(render(source), '<p>[<img src="image.png" alt="alt inner">](/outside)</p>\n', source);
    }
  }
  const source = '[![alt [inner][ref]][picture]][outer]\n\n[ref]: /inside\n[picture]: image.png\n[outer]: /outside\n';
  assert.equal(render(source), '<p>[<img src="image.png" alt="alt inner">]<a href="/outside">outer</a></p>\n');
});

test('nested image descriptions retain bracket provenance without blocking angle autolinks', () => {
  assert.equal(render('[![outer ![alt [inner](/inside)](nested.png)](image.png)](/outside)'),
    '<p>[<img src="image.png" alt="outer alt inner">](/outside)</p>\n');
  for (const [label, alt] of [
    ['plain', 'plain'],
    ['<https://inside.example>', 'https://inside.example'],
    ['<inside@example.com>', 'inside@example.com'],
    ['![nested](nested.png)', 'nested'],
    ['![<https://inside.example>](nested.png)', 'https://inside.example'],
  ]) {
    for (const suffix of ['(image.png)', '[picture]']) {
      const source = `[![${label}]${suffix}](/outside)\n\n[picture]: image.png\n`;
      assert.equal(render(source), `<p><a href="/outside"><img src="image.png" alt="${alt}"></a></p>\n`, source);
    }
  }
});

test('rejected outer destinations and titles remain ordinary inline source', () => {
  assert.equal(render('[![alt [inner](/inside)](image.png)](/outside "[tail](/tail)")'),
    '<p>[<img src="image.png" alt="alt inner">](/outside &quot;<a href="/tail">tail</a>&quot;)</p>\n');
  assert.equal(render('[![alt [inner][missing]](image.png)](/outside)'),
    '<p><a href="/outside"><img src="image.png" alt="alt [inner][missing]"></a></p>\n');
});

test('a citation-shaped bracket link still retires its outer opener with a live note definition', () => {
  const source = '[![alt [^inner](/inside)](image.png)](/outside)\n\n[^inner]: Dormant note\n';
  assert.equal(render(source), '<p>[<img src="image.png" alt="alt ^inner">](/outside)</p>\n');
});

test('bracket provenance cannot escape its paragraph or a repeated rendering', () => {
  const rejected = '[![alt [inner](/inside)](image.png)](/outside)';
  const valid = '[![plain](image.png)](/valid) [after](/after)';
  const source = `[before](/before) ${rejected} [after](/after)\n\n${valid}\n\n${rejected}\n\n${valid}`;
  const expected = '<p><a href="/before">before</a> [<img src="image.png" alt="alt inner">](/outside) <a href="/after">after</a></p>\n'
    + '<p><a href="/valid"><img src="image.png" alt="plain"></a> <a href="/after">after</a></p>\n'
    + '<p>[<img src="image.png" alt="alt inner">](/outside)</p>\n'
    + '<p><a href="/valid"><img src="image.png" alt="plain"></a> <a href="/after">after</a></p>\n';
  for (let attempt = 0; attempt < 3; attempt++) assert.equal(render(source), expected);
});
