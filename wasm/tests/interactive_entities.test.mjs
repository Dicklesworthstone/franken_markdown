// Execute the production saved-document parser without a DOM or entity library.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g,
  ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
const tick = String.fromCharCode(96);

test('every native named reference decodes with exact scalar and case identity', () => {
  const native = readFileSync(new URL('../../src/parse/entities.rs', import.meta.url), 'utf8');
  const table = native.match(/static HTML5_ENTITIES: &\[\(&str, &str\)\] = &\[\n([\s\S]*?)\n\];/)[1];
  const rows = [...table.matchAll(/^\s*\("([A-Za-z0-9]+)", "((?:\\u\{[0-9a-f]+\})+)"\),$/gm)];
  assert.equal(rows.length, 2125);
  for (const [, name, scalars] of rows) {
    const value = scalars.replace(/\\u\{([0-9a-f]+)\}/g, (_, hex) => String.fromCodePoint(parseInt(hex, 16)));
    assert.equal(render('before &' + name + '; after'), '<p>before ' + escape(value) + ' after</p>\n', name);
    assert.equal(render('[&' + name + ';](/ok "' + '&' + name + ';")'),
      '<p><a href="/ok" title="' + escape(value) + '">' + escape(value) + '</a></p>\n', name);
  }
});

test('unchanged official entity examples retain their exact expected HTML', () => {
  const corpus = JSON.parse(readFileSync(new URL('../../tests/fixtures/commonmark/spec.json', import.meta.url), 'utf8'));
  for (const id of [25, 26, 27, 28, 29, 30, 34, 35]) {
    const example = corpus.find(item => item.example === id);
    assert.equal(render(example.markdown), example.html, 'CommonMark example ' + id);
  }
});

test('case-sensitive names include multi-scalar and astral replacements', () => {
  assert.equal(render('&aelig; &AElig; &AELIG; &AMP; &Amp; &NotEqualTilde; &ngE; &Afr;'),
    '<p>æ Æ &amp;AELIG; &amp; &amp;Amp; ≂̸ ≧̸ 𝔄</p>\n');
  for (const name of ['constructor', 'toString', 'hasOwnProperty', 'notARealEntity']) {
    assert.equal(render('&' + name + ';'), '<p>&amp;' + name + ';</p>\n');
  }
});

test('numeric references follow native digit bounds and Unicode scalar rules', () => {
  for (const [source, expected] of [
    ['&#65;', 'A'], ['&#0000065;', 'A'], ['&#x41;', 'A'], ['&#X000041;', 'A'],
    ['&#128578;', '🙂'], ['&#x1F642;', '🙂'], ['&#x10ffff;', '\u{10ffff}'],
    ['&#0;', '\ufffd'], ['&#xD800;', '\ufffd'], ['&#xDFFF;', '\ufffd'],
    ['&#1114112;', '\ufffd'], ['&#9999999;', '\ufffd'], ['&#xFFFFFF;', '\ufffd'],
    ['&#128;', '\u0080'], ['&#9;', '\t'], ['&#10;', '\n'],
  ]) assert.equal(render('a' + source + 'z'), '<p>a' + escape(expected) + 'z</p>\n', source);
});

test('malformed and overlong references remain complete literal source', () => {
  for (const source of [
    '&copy', '&x;', '&#;', '&#x;', '&#-1;', '&#+65;', '&# 65;', '&#６５;',
    '&#00000065;', '&#87654321;', '&#x0000041;', '&#x1G;', '&NotEqualTilde',
  ]) assert.equal(render(source), '<p>' + escape(source) + '</p>\n', source);
});

test('escaping and decoding consume original source once in prose', () => {
  assert.equal(render('\\&copy; \\\\&copy; &amp;copy; &#38;copy; &bsol;&copy;'),
    '<p>&amp;copy; \\© &amp;copy; &amp;copy; \\©</p>\n');
  assert.equal(render('&#92;*literal*'), '<p>\\<em>literal</em></p>\n');
});

test('decoded punctuation cannot create formatting, links or definitions', () => {
  assert.equal(render('&ast;bold&ast; &lbrack;x&rbrack;&lpar;/url&rpar;'),
    '<p>*bold* [x](/url)</p>\n');
  assert.equal(render('&num; heading\n\n&lbrack;r&rbrack;&colon; /hidden\n\n[r]'),
    '<p># heading</p>\n<p>[r]: /hidden</p>\n<p>[r]</p>\n');
  assert.equal(render('a&NewLine;&NewLine;&num; title'),
    '<p>a\n\n# title</p>\n');
});

test('destinations and titles decode named references without a second escape pass', () => {
  assert.equal(render('[&copy;](/f&ouml;&ouml; "Read &NotEqualTilde;")'),
    '<p><a href="/föö" title="Read ≂̸">©</a></p>\n');
  assert.equal(render('[x](/\\&amp; "\\&amp; &amp;copy;")'),
    '<p><a href="/&amp;amp;" title="&amp;amp; &amp;copy;">x</a></p>\n');
  assert.equal(render('[x](/&amp;copy; "&bsol;&copy;")'),
    '<p><a href="/&amp;copy;" title="\\©">x</a></p>\n');
});

test('reference definitions decode destinations and titles but retain raw label keys', () => {
  assert.equal(render('[r&copy;] [r©]\n\n[r&copy;]: /named&num;part "A &copy;"\n[r©]: /literal'),
    '<p><a href="/named#part" title="A ©">r©</a> <a href="/literal">r©</a></p>\n');
  assert.equal(render('[x][r]\n\n[r]: /\\&amp; "\\&copy;"'),
    '<p><a href="/&amp;amp;" title="&amp;copy;">x</a></p>\n');
});

test('image labels, titles and exact decoded resource bindings retain their identity', () => {
  const assets = new Map([['föö.png', 'data:image/png;base64,AAAA']]);
  assert.equal(render('![&copy; &Afr;](f&ouml;&ouml;.png "A &NotEqualTilde;")', assets),
    '<p><img src="data:image/png;base64,AAAA" alt="© 𝔄" title="A ≂̸"></p>\n');
  assert.equal(render('![&lt;img onerror=&quot;bad&quot;&gt;](image.png)'),
    '<p><img src="image.png" alt="&lt;img onerror=&quot;bad&quot;&gt;"></p>\n');
});

test('code, raw HTML and math retain literal reference spellings', () => {
  assert.equal(render(tick + '&copy; &#65;' + tick), '<p><code>&amp;copy; &amp;#65;</code></p>\n');
  assert.equal(render(tick.repeat(3) + '\n&copy; &#65;\n' + tick.repeat(3)),
    '<pre><code>&amp;copy; &amp;#65;\n</code></pre>\n');
  assert.equal(render('    &copy; &#65;'), '<pre><code>&amp;copy; &amp;#65;\n</code></pre>\n');
  assert.equal(render('$&copy; &#65;$'), '<p>$&amp;copy; &amp;#65;$</p>\n');
  assert.equal(render('<div>&copy;</div>'), '<p>&lt;div&gt;&amp;copy;&lt;/div&gt;</p>\n');
});

test('angle autolinks retain literal references instead of changing their destinations', () => {
  const url = 'https://example.test/?a=&copy;&amp;b=&#65;';
  assert.equal(render('<' + url + '>'), '<p><a href="' + escape(url) + '">' + escape(url) + '</a></p>\n');
  const backslash = 'https://example.test/\\&amp;';
  assert.equal(render('<' + backslash + '>'), '<p>&lt;' + escape(backslash) + '&gt;</p>\n');
});

test('URL policy examines decoded schemes and controls before publishing links or images', () => {
  for (const url of [
    'javascript&colon;alert(1)', 'java&Tab;script:alert(1)', 'java&NewLine;script:alert(1)',
    'data&colon;text/html,evil', 'file&colon;///secret', 'https&colon;&bsol;&bsol;evil.test',
  ]) {
    const html = render('[x](' + url + ') ![image](' + url + ')');
    assert.doesNotMatch(html, /<(?:a|img)\b/, url);
  }
  assert.equal(render('[x](https&colon;//example.test/)'),
    '<p><a href="https://example.test/">x</a></p>\n');
});

test('fenced info decodes before native whitespace tokenization', () => {
  for (const info of ['&Tab;rust other', 'rust&Tab;other', 'rust&NewLine;other', 'rust&#133;other', 'rust&nbsp;other']) {
    assert.equal(render(tick.repeat(3) + info + '\nx\n' + tick.repeat(3)),
      '<pre><code class="language-rust">x\n</code></pre>\n', info);
  }
  assert.equal(render(tick.repeat(3) + '\\&copy;\nx\n' + tick.repeat(3)),
    '<pre><code class="language-&amp;copy;">x\n</code></pre>\n');
  assert.equal(render(tick.repeat(3) + 'a\ufeffb\nx\n' + tick.repeat(3)),
    '<pre><code class="language-a\ufeffb">x\n</code></pre>\n');
});

test('heading IDs decode the emitted HTML layer once without merging literal entities', () => {
  assert.equal(render('# &copy;\n\n# &amp;copy;\n\n# &Afr;'),
    '<h1 id="section">©</h1>\n<h1 id="copy">&amp;copy;</h1>\n<h1 id="𝔄">𝔄</h1>\n');
});

test('malformed ampersand runs retain complete source within the existing work budget', () => {
  for (const source of ['&'.repeat(100000), '&' + 'a'.repeat(100000), '&#' + '9'.repeat(100000) + ';']) {
    assert.equal(render(source), '<p>' + escape(source) + '</p>\n');
  }
});

test('entity-heavy requests preserve typed budgets and fresh-render recovery', () => {
  const script = new vm.Script('parseMarkdownClient(input)');
  context.input = '&CounterClockwiseContourIntegral;'.repeat(200000);
  assert.throws(() => script.runInContext(context, {timeout: 5000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(render('&copy;'), '<p>©</p>\n');
});
