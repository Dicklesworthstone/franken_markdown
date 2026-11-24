// Saved-document notes retain the source identity and link ownership specified
// by tests/html_footnote_{links,fragments}.rs. Exercise actual production JS.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
const unescape = text => text.replace(/&(amp|lt|gt|quot|#39);/g,
  (_, name) => ({amp: '&', lt: '<', gt: '>', quot: '"', '#39': "'"}[name]));
const ids = html => [...html.matchAll(/\bid="([^"]*)"/g)].map(([, id]) => unescape(id));
function assertNoNestedAnchors(html) {
  let open = false;
  for (const [tag] of html.matchAll(/<\/?a(?:\s[^>]*|)>/g)) {
    if (tag.startsWith('</')) { assert.ok(open, 'unmatched anchor close'); open = false; }
    else { assert.equal(open, false, 'a citation cannot nest an anchor'); open = true; }
  }
  assert.equal(open, false, 'unclosed anchor');
}
function assertFragmentTargets(html) {
  const targets = ids(html);
  assert.equal(new Set(targets).size, targets.length, 'rendered IDs must stay unique');
  for (const [, fragment] of html.matchAll(/href="#([^"]*)"/g)) {
    assert.ok(targets.includes(decodeURIComponent(unescape(fragment))), fragment);
  }
}

test('a citation in a Markdown link retains its complete note and return anchor', () => {
  const html = render('[Before **claim[^note]** after](/source "Source") outside[^note].\n\n[^note]: Supporting evidence.');
  assert.ok(html.includes('<a href="/source" title="Source">Before <strong>claim<sup class="footnote-ref" id="fnref-1">1</sup></strong> after</a>'));
  assert.ok(html.includes('outside<sup class="footnote-ref" id="fnref-1-2"><a href="#fn-note" role="doc-noteref">1</a></sup>'));
  assert.equal((html.match(/<li id="fn-note">/g) || []).length, 1);
  assert.equal((html.match(/Supporting evidence\./g) || []).length, 1);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 2);
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});

test('reference-link labels can be the only citation without losing their evidence', () => {
  const html = render('[Read **claim[^n]**][paper]\n\n[paper]: /paper "Paper"\n[^n]: First paragraph.\n\n    Second **paragraph** with [source](/source).');
  assert.ok(html.includes('<a href="/paper" title="Paper">Read <strong>claim<sup class="footnote-ref" id="fnref-1">1</sup></strong></a>'));
  assert.ok(html.includes('<li id="fn-n"><p>First paragraph.</p>\n<p>Second <strong>paragraph</strong> with <a href="/source">source</a>.</p>'));
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});

test('refused outer URLs leave valid note links active without activating bare label text', () => {
  const html = render('[Claim[^a] https://label.test](javascript:alert) [Safe[^b]](/safe) after[^a] [Last](/last).\n\n[^a]: First note.\n[^b]: Second note.');
  assert.ok(html.includes('Claim<sup class="footnote-ref" id="fnref-1"><a href="#fn-a" role="doc-noteref">1</a></sup> https://label.test'));
  assert.ok(html.includes('<a href="/safe">Safe<sup class="footnote-ref" id="fnref-2">2</sup></a>'));
  assert.ok(html.includes('<a href="/last">Last</a>'));
  assert.doesNotMatch(html, /href="javascript:|href="https:\/\/label.test"/);
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});

test('authored source-note fragments resolve independently of citation and definition order', () => {
  const definition = '\n\n[^unused]: Unused.\n[^second]: Second evidence.\n[^first]: First evidence.';
  for (const citations of ['A[^first] B[^second]', 'B[^second] A[^first]']) {
    const html = render('[First](#fn-first) [Second](#fn-second) ' + citations + definition);
    assert.ok(html.includes('<li id="fn-first">'));
    assert.ok(html.includes('<li id="fn-second">'));
    assert.doesNotMatch(html, /Unused\.|id="fn-unused"/);
    assertFragmentTargets(html);
  }
  // Numeric source names remain names; the first citation does not claim '1'.
  const numeric = render('B[^2] A[^1] [First](#fn-1) [Second](#fn-2)\n\n[^1]: One.\n[^2]: Two.');
  assert.ok(numeric.includes('<li id="fn-2"><p>Two.</p>'));
  assert.ok(numeric.includes('<li id="fn-1"><p>One.</p>'));
  assertFragmentTargets(numeric);
});

test('generated note fragments encode exact source identifiers once', () => {
  for (const [id, fragment] of [
    ['space%20name', 'space%2520name'], ['nested%2520name', 'nested%252520name'],
    ['malformed%zz', 'malformed%25zz'], ['ending%', 'ending%25'], ['café', 'caf%C3%A9'],
    ['a&b', 'a%26b'], ['quoted"name', 'quoted%22name'], ['a#b?c/d', 'a%23b%3Fc%2Fd'],
    [':~:text=claim', '%3A~%3Atext%3Dclaim'], ["a'b(c)!*", 'a%27b%28c%29%21%2A'],
    ['plain-._~123', 'plain-._~123'], ['猫🙂', '%E7%8C%AB%F0%9F%99%82'],
  ]) {
    const html = render(`Claim[^${id}] [Read](#fn-${fragment})\n\n[^${id}]: Evidence.`);
    assert.ok(html.includes('<li id="fn-' + escape(id) + '">'), id);
    assert.ok(html.includes('href="#fn-' + fragment + '" role="doc-noteref"'), id);
    assertFragmentTargets(html);
    assertNoNestedAnchors(html);
  }
});

test('percent-looking names cannot redirect citations to a different literal note', () => {
  const html = render('First[^chapter%41] Second[^chapterA] First again[^chapter%41].\n\n[^chapter%41]: Percent evidence.\n[^chapterA]: Letter evidence.');
  assert.equal((html.match(/href="#fn-chapter%2541"/g) || []).length, 2);
  assert.equal((html.match(/href="#fn-chapterA"/g) || []).length, 1);
  assert.doesNotMatch(html, /href="#fn-chapter%41"/);
  assert.ok(html.includes('<li id="fn-chapter%41"><p>Percent evidence.</p>'));
  assert.ok(html.includes('<li id="fn-chapterA"><p>Letter evidence.</p>'));
  assertFragmentTargets(html);
});

test('case-sensitive Unicode notes and cycles remain complete when cited only from links', () => {
  const html = render('[Read[^A]](/read) [Other[^a]](/other) [Accent[^é]](/accent)\n\n[^A]: Upper cites lower[^a].\n[^a]: Lower cites upper[^A].\n[^é]: Accent cites decomposed[^é].\n[^é]: Decomposed evidence.');
  for (const id of ['A', 'a', 'é', 'é']) assert.equal((html.match(new RegExp('<li id="fn-' + id + '">', 'g')) || []).length, 1);
  assert.equal((html.match(/class="footnote-ref"/g) || []).length, 6);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 6);
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});

test('heading and citation collisions retain unique note targets and every backlink', () => {
  for (const headingsFirst of [true, false]) {
    const headings = '# fn-note\n\n# fnref-1\n\n# fnref-1-2\n\n';
    const citations = '[Inside[^note]](/inside) outside[^note].\n\n';
    const html = render((headingsFirst ? headings + citations : citations + headings) + '[^note]: Evidence.');
    assert.equal((html.match(/class="footnote-backref"/g) || []).length, 2);
    assertNoNestedAnchors(html);
    assertFragmentTargets(html);
  }
});

test('image descriptions and opaque inline atoms cannot make unused notes visible', () => {
  for (const source of [
    '![Picture[^hidden]](image.png)', '[![Picture[^hidden]](image.png)](/source)',
    '`[^hidden]`', '$[^hidden]$', '<custom title="[^hidden]">', '<!-- [^hidden] -->',
    '[Code `[^hidden]`](/source)', '[Math $[^hidden]$](/source)',
    '[Markup <custom title="[^hidden]">](/source)',
  ]) {
    const html = render(source + '\n\n[^hidden]: SECRET BODY');
    assert.doesNotMatch(html, /SECRET BODY|class="footnote-ref"|class="footnotes"/, source);
    assertNoNestedAnchors(html);
  }
  assert.equal(render('[Unknown[^absent]](/source)'), '<p><a href="/source">Unknown[^absent]</a></p>\n');
});

test('raw quote and backtick identifiers remain text identities without stealing markup', () => {
  for (const id of ['n`q', '"><img>', 'a&amp;b']) {
    const html = render(`[label [^${id}]](/outer) [later](/later) \`\n\n[^${id}]: Kept body`);
    assert.ok(html.includes('<a href="/outer">label <sup class="footnote-ref" id="fnref-1">1</sup></a>'));
    assert.ok(html.includes('<a href="/later">later</a> `'));
    assert.equal((html.match(/Kept body/g) || []).length, 1);
    assert.doesNotMatch(html, /<img/);
    assertFragmentTargets(html);
    assertNoNestedAnchors(html);
  }
});

test('linked citations work in headings tables lists quotes and nested note bodies', () => {
  for (const newline of ['\n', '\r', '\r\n']) {
    for (const source of [
      '# [Claim[^n]](/source)', '> [Claim[^n]](/source)', '- [Claim[^n]](/source)',
      '| H |\n| --- |\n| [Claim[^n]](/source) |',
      'Use[^outer]\n\n[^outer]: [Claim[^n]](/source)',
    ]) {
      const html = render((source + '\n\n[^n]: Kept evidence.').replace(/\n/g, newline));
      assert.equal((html.match(/Kept evidence\./g) || []).length, 1, source);
      assertNoNestedAnchors(html);
      assertFragmentTargets(html);
    }
  }
});

test('many linked citations keep one body with bounded expansion and recover after limits', () => {
  const bounded = vm.createContext({source: '[Claim[^n]](/source) '.repeat(1800) + '\n\n[^n]: Evidence.'});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000});
  assert.equal((html.match(/<li id="fn-n">/g) || []).length, 1);
  assert.equal((html.match(/class="footnote-ref"/g) || []).length, 1800);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 1800);
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
  const small = vm.createContext({source: bounded.source});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 100000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("[Unknown[^n]](/source)")', small),
    '<p><a href="/source">Unknown[^n]</a></p>\n');
});

test('ill-formed UTF-16 note names cannot redirect citations to another note', () => {
  const names = ['name\ud800', 'name\udc00', 'name\ufffd', 'name🙂', 'name\ud800\udc00'];
  const definitions = names.map((name, index) => `[^${name}]: Evidence ${index}.`).join('\n\n');
  for (const order of [names, [...names].reverse()]) {
    const citations = order.map(name => `Claim[^${name}]`).join(' ');
    const html = render(citations + '\n\n' + definitions);
    const bodies = new Map([...html.matchAll(/<li id="([^"]*)"><p>Evidence ([0-9]+)\.<\/p>/g)]
      .map(([, id, index]) => [unescape(id), Number(index)]));
    const references = [...html.matchAll(/<a href="#([^"]*)" role="doc-noteref">/g)]
      .map(([, fragment]) => decodeURIComponent(unescape(fragment)));
    assert.equal(bodies.size, names.length);
    assert.equal(references.length, names.length);
    for (const [index, target] of references.entries()) {
      assert.equal(bodies.get(target), names.indexOf(order[index]), target);
    }
    for (const id of bodies.keys()) assert.doesNotMatch(id, /[\ud800-\udfff]/u);
    assert.ok(bodies.has('fn-name🙂'), 'valid astral source identity must survive');
    assert.ok(bodies.has('fn-name\ud800\udc00'), 'paired high/low surrogates are one valid scalar');
    assertNoNestedAnchors(html);
    assertFragmentTargets(html);
  }
});

test('heading fragments ignore generated note numbers without changing literal code', () => {
  for (const prefix of ['', 'Earlier[^prior].\n\n']) {
    const html = render(prefix + '# [Claim[^n]](/source)\n\n## Claim[^n]\n\n'
      + '[Go](#claim) [Again](#claim-2)\n\n[^n]: Evidence.\n\n[^prior]: Earlier evidence.');
    assert.ok(html.includes('<h1 id="claim"><a href="/source">Claim<sup class="footnote-ref"'));
    assert.ok(html.includes('<h2 id="claim-2">Claim<sup class="footnote-ref"'));
    const number = prefix ? 2 : 1;
    assert.ok(html.includes(`id="fnref-${number}">${number}</sup></a>`));
    assertNoNestedAnchors(html);
    assertFragmentTargets(html);
  }
  const literal = render('# `<sup class="footnote-ref">7</sup>`[^n]\n\n'
    + '[Literal](#sup-classfootnote-ref7sup)\n\n[^n]: Evidence.');
  assert.ok(literal.includes('<h1 id="sup-classfootnote-ref7sup">'));
  assert.ok(literal.includes('<code>&lt;sup class=&quot;footnote-ref&quot;&gt;7&lt;/sup&gt;</code>'));
  assertNoNestedAnchors(literal);
  assertFragmentTargets(literal);
});

test('citations cannot rename later headings or their authored fragment links', () => {
  const headings = '# fn-note\n\n# fnref-1\n\n';
  const citation = 'Claim[^note].\n\n';
  for (const source of [headings + citation, citation + headings]) {
    const html = render(source + '[Heading](#fn-note) [Other heading](#fnref-1)\n\n[^note]: Evidence.');
    assert.ok(html.includes('<h1 id="fn-note">fn-note</h1>'));
    assert.ok(html.includes('<h1 id="fnref-1">fnref-1</h1>'));
    assert.ok(html.includes('<li id="fn-note-2"><p>Evidence.</p>'));
    assert.ok(html.includes('<sup class="footnote-ref" id="fnref-1-2"><a href="#fn-note-2" role="doc-noteref">1</a></sup>'));
    assert.ok(html.includes('<a href="#fn-note">Heading</a>'));
    assert.ok(html.includes('<a href="#fnref-1">Other heading</a>'));
    assertNoNestedAnchors(html);
    assertFragmentTargets(html);
  }
});

test('collision suffixes cannot consume a later source note identity', () => {
  const html = render('First[^note] second[^note-2] third[^note-3].\n\n'
    + '# fn-note\n\n# fn-note-4\n\n# fnref-1\n\n# fnref-1-2\n\n'
    + '[Second](#fn-note-2) [Third](#fn-note-3)\n\n'
    + '[^note-3]: Third body.\n[^note-2]: Second body.\n[^note]: First body.');
  assert.ok(html.includes('<li id="fn-note-5"><p>First body.</p>'));
  assert.ok(html.includes('<li id="fn-note-2"><p>Second body.</p>'));
  assert.ok(html.includes('<li id="fn-note-3"><p>Third body.</p>'));
  assert.ok(html.includes('<sup class="footnote-ref" id="fnref-1-3"><a href="#fn-note-5" role="doc-noteref">1</a></sup>'));
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});

test('reachable note headings reserve anchors before any citation is rendered', () => {
  const html = render('Start[^outer].\n\n'
    + '[^unused]: # fn-note-2\n\n'
    + '[^outer]: # fn-note\n\n    Continue[^inner].\n\n'
    + '[^inner]: # fnref-1\n\n    Back[^outer] and evidence[^note].\n\n'
    + '[^outer]: # fn-note-3\n\n'
    + '[^note]: Kept evidence.');
  assert.ok(html.includes('<h1 id="fn-note">fn-note</h1>'));
  assert.ok(html.includes('<h1 id="fnref-1">fnref-1</h1>'));
  assert.ok(html.includes('<li id="fn-note-2"><p>Kept evidence.</p>'));
  assert.ok(html.includes('id="fnref-1-2"><a href="#fn-outer" role="doc-noteref">1</a>'));
  assert.ok(html.includes('id="fnref-1-2-2"><a href="#fn-outer" role="doc-noteref">1</a>'));
  assert.equal((html.match(/<li id="fn-outer">/g) || []).length, 1);
  assert.equal((html.match(/<li id="fn-inner">/g) || []).length, 1);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 4);
  assert.doesNotMatch(html, /<h1 id="fn-note-[23]">|id="fn-unused"/);
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});

test('heading allocation follows body and referenced-note order rather than definition order', () => {
  const definitions = '[^a]: # Same\n\n[^b]: # Same\n\n[^hidden]: # Same\n';
  const html = render('Use[^b] then[^a].\n\n# Same\n\n' + definitions);
  assert.ok(html.includes('<h1 id="same">Same</h1>'));
  assert.ok(html.includes('<li id="fn-b"><h1 id="same-2">Same</h1>'));
  assert.ok(html.includes('<li id="fn-a"><h1 id="same-3">Same</h1>'));
  assert.doesNotMatch(html, /same-4|id="fn-hidden"/);
  assertFragmentTargets(html);
});

test('discarded table cells cannot activate notes or reserve their headings', () => {
  const html = render('Claim[^shown].\n\n| H |\n| --- |\n| kept | discarded[^hidden] |\n\n'
    + '[^shown]: Evidence.\n\n[^hidden]: # fn-shown');
  assert.ok(html.includes('<li id="fn-shown"><p>Evidence.</p>'));
  assert.doesNotMatch(html, /discarded|<h1|fn-hidden|fn-shown-2/);
  assertFragmentTargets(html);
});

test('noncolliding repeated citations retain their existing complete output', () => {
  assert.equal(render('Evidence[^a] again[^a].\n\n[^a]: A note.'),
    '<p>Evidence<sup class="footnote-ref" id="fnref-1"><a href="#fn-a" role="doc-noteref">1</a></sup> '
    + 'again<sup class="footnote-ref" id="fnref-1-2"><a href="#fn-a" role="doc-noteref">1</a></sup>.</p>\n'
    + '<section class="footnotes" role="doc-endnotes"><hr><ol>\n<li id="fn-a"><p>A note.</p>\n'
    + ' <a href="#fnref-1" class="footnote-backref" role="doc-backlink" aria-label="Back to reference 1">↩</a>'
    + ' <a href="#fnref-1-2" class="footnote-backref" role="doc-backlink" aria-label="Back to reference 1-2">↩</a>'
    + '</li>\n</ol></section>\n');
});

test('many malformed note names sharing a DOM base keep bounded collision allocation', () => {
  const names = Array.from({length: 900}, (_, index) =>
    'long-note-name-'.repeat(6) + String.fromCharCode(0xd800 + index));
  const source = names.map(name => `Claim[^${name}].`).join(' ') + '\n\n'
    + names.map((name, index) => `[^${name}]: Evidence ${index}.`).join('\n');
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000});
  assert.equal((html.match(/<li id="fn-/g) || []).length, names.length);
  assert.equal((html.match(/class="footnote-ref"/g) || []).length, names.length);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, names.length);
  assertNoNestedAnchors(html);
  assertFragmentTargets(html);
});
