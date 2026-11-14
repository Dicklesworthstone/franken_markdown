// Saved, editable documents must keep the native parser's GFM bare links.
// Grammar oracles: tests/parser_autolink_{domains,protocols,bounds}.rs. These
// execute the production dependency-free renderer, with its existing URL policy.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[ch]));
const anchor = (label, url = label) => '<a href="' + escape(url) + '">' + escape(label) + '</a>';
const paragraph = content => '<p>' + content + '</p>\n';

test('ordinary README URLs and contact addresses remain usable in saved documents', () => {
  assert.equal(render('Visit https://example.test/help or www.example.test. Contact team@example.test or mailto:team@example.test.'),
    paragraph('Visit ' + anchor('https://example.test/help') + ' or ' + anchor('www.example.test', 'http://www.example.test')
      + '. Contact ' + anchor('team@example.test', 'mailto:team@example.test') + ' or ' + anchor('mailto:team@example.test') + '.'));
  for (const url of ['HTTP://Example.TEST', 'HtTpS://Example.TEST/path', 'WWW.Example.TEST']) {
    assert.equal(render(url), paragraph(anchor(url, url.startsWith('WWW.') ? 'http://' + url : url)));
  }
});

test('HTTP hosts follow GFM domain rules without replacing them with DNS validation', () => {
  for (const url of [
    'http://localhost', 'http://localhost:8080/path', 'https://a', 'https://api-v2.example:8443/path',
    'https://例え.テスト/path', 'http://🍄.example/', 'http://x🍄.example/', 'http://first..example/path',
    'https://_alpha.beta.gamma/route_with_tail', 'https://sub_name.host.example:8443/a_b?arg=c_d#e_f',
    'https://host.example/path_a.b_c/end', 'https://host.example?query_a=b_c', 'https://host.example#section_one',
  ]) assert.equal(render(url), paragraph(anchor(url)), url);
  for (const url of ['www._alpha.beta.gamma', 'www.first..example/path']) {
    assert.equal(render(url), paragraph(anchor(url, 'http://' + url)), url);
  }
  for (const text of [
    'http://', 'https://', 'https:///path', 'https://?query', 'https://#fragment', 'https://:443/path', 'https://.',
    'www.', 'www.alpha.beta._gamma', 'www.alpha._beta.gamma', 'https://alpha.beta._gamma/path',
    'https://alpha._beta.gamma:8443/path', 'https://alpha.beta_gamma/one.two/three', 'http://underscore_only',
  ]) assert.equal(render(text), paragraph(escape(text)), text);
});

test('bare email grammar preserves local parts and stops before sentence punctuation', () => {
  for (const address of [
    'reader@example.test', 'First.Last+tag@Example.TEST', 'first.last+tag@_host.-example',
    'first-last_tag@-host.example', 'a@host._example', 'a@host.2',
  ]) assert.equal(render(address), paragraph(anchor(address, 'mailto:' + address)), address);
  for (const suffix of ['..tail', '...tail', '.', '...']) {
    assert.equal(render('a@host.example' + suffix), paragraph(anchor('a@host.example', 'mailto:a@host.example') + suffix));
  }
  for (const text of ['a@host..test', 'a@host...test', 'a@host..é', 'a@host.example-', 'a@host.example_', 'a@localhost', 'a@.']) {
    assert.equal(render(text), paragraph(text), text);
  }
  // Angle-email's extra local punctuation and single-label domains stay its
  // own grammar, rather than silently broadening bare-address recognition.
  assert.equal(render('a/b@example.test'), paragraph('a/b@example.test'));
  assert.equal(render('<a/b@localhost>'), paragraph(anchor('a/b@localhost', 'mailto:a/b@localhost')));
});

test('protocol addresses own their complete scheme and preserve the existing allowlist', () => {
  for (const value of ['mailto:foo@bar.baz', 'mailto:a.b-c_d@a.b', 'MAILTO:User@Example.TEST']) {
    assert.equal(render(value), paragraph(anchor(value)), value);
    assert.equal(render('(' + value + ').'), paragraph('(' + anchor(value) + ').'));
  }
  assert.equal(render('mailto:a.b-c_d@a.b/'), paragraph(anchor('mailto:a.b-c_d@a.b') + '/'));
  for (const value of [
    'xmpp:foo@bar.baz', 'xmpp:foo@bar.baz/txt', 'xmpp:foo@bar.baz/txt@bin.com',
    'XmPp:User@Example.TEST/Phone', 'xmpp:foo@bar.baz/txt/bin', 'xmpp:foo@bar.baz/猫',
  ]) assert.equal(render(value), paragraph(value), value);
  for (const value of [
    'xmailto:foo@bar.baz', 'prefixxmpp:foo@bar.baz', 'mailto:foo@localhost', 'mailto:a.b-c_d@a.b-',
    'mailto:a.b-c_d@a.b_', 'xmpp:foo@bar.baz-/txt', 'xmpp:foo@bar..baz/txt', 'mailto:@bar.baz', 'mailto:', 'xmpp:',
  ]) assert.equal(render(value), paragraph(value), value);
});

test('formatting and balanced URL parentheses keep punctuation outside the destination', () => {
  for (const [source, expected] of [
    ['_https://host.example/a_b_', '<em>' + anchor('https://host.example/a_b') + '</em>'],
    ['~~www.sub_domain.host.example/a_b~~.', '<del>' + anchor('www.sub_domain.host.example/a_b', 'http://www.sub_domain.host.example/a_b') + '</del>.'],
    ['(https://host.example/a(b)).', '(' + anchor('https://host.example/a(b)') + ').'],
    ['https://host.example/a((b)c)))', anchor('https://host.example/a((b)c)') + '))'],
    ['"https://host.example/path"', '&quot;' + anchor('https://host.example/path') + '&quot;'],
    ['*alice@example.test*', '<em>' + anchor('alice@example.test', 'mailto:alice@example.test') + '</em>'],
    ['**alice_smith+tag@example.test**.', '<strong>' + anchor('alice_smith+tag@example.test', 'mailto:alice_smith+tag@example.test') + '</strong>.'],
    ['~~mailto:team@example.test~~', '<del>' + anchor('mailto:team@example.test') + '</del>'],
  ]) assert.equal(render(source), paragraph(expected), source);
});

test('entity-shaped URL suffixes return to prose while interior entities stay literal', () => {
  assert.equal(render('www.google.com/search?q=commonmark&hl=en'),
    paragraph(anchor('www.google.com/search?q=commonmark&hl=en', 'http://www.google.com/search?q=commonmark&hl=en')));
  assert.equal(render('www.google.com/search?q=commonmark&hl;'),
    paragraph(anchor('www.google.com/search?q=commonmark', 'http://www.google.com/search?q=commonmark') + '&amp;hl;'));
  for (const [encoded, decoded] of [['&copy;', '©'], ['&amp;', '&amp;'], ['&frac34;', '¾'], ['&NotEqualTilde;', '≂̸'], ['&unknown9;', '&amp;unknown9;'], ['&123;', '&amp;123;']]) {
    assert.equal(render('**https://example.test/a' + encoded + '**.'), paragraph('<strong>' + anchor('https://example.test/a') + decoded + '</strong>.'));
  }
  for (const [source, destination, tail] of [
    ['https://example.test/?a=1&amp;b=2', 'https://example.test/?a=1&amp;b=2', ''],
    ['https://example.test/&#123;', 'https://example.test/&#123', ';'],
    ['https://example.test/&a-b;', 'https://example.test/&a-b', ';'],
    ['https://example.test/&;', 'https://example.test/&', ';'],
    ['https://example.test/a(b))&copy;.)', 'https://example.test/a(b)', ')©.)'],
  ]) assert.equal(render(source), paragraph(anchor(destination) + tail), source);
  assert.equal(render('https://example.test/a&copy;&amp;'), paragraph(anchor('https://example.test/a') + '©&amp;'));
});

test('explicit and reference links and image descriptions keep sole ownership of their labels', () => {
  const label = 'https://example.test/a team@example.test mailto:team@example.test';
  for (const suffix of ['(/chosen)', '[ref]', '[]', '']) {
    const definition = suffix === '[]' || suffix === '' ? label : 'ref';
    assert.equal(render('[' + label + ']' + suffix + '\n\n[' + definition + ']: /chosen'), paragraph(anchor(label, '/chosen')), suffix);
  }
  assert.equal(render('![https://example.test/a team@example.test](picture.png)'),
    paragraph('<img src="picture.png" alt="https://example.test/a team@example.test">'));
  assert.equal(render('[outer [https://example.test](/inner)](/outer)'),
    paragraph('[outer ' + anchor('https://example.test', '/inner') + '](/outer)'));
  assert.equal(render('[*https://example.test*](/chosen)'),
    paragraph('<a href="/chosen"><em>https://example.test</em></a>'));
  assert.equal(render('[outer <reader@localhost>](/outer)'),
    paragraph('[outer ' + anchor('reader@localhost', 'mailto:reader@localhost') + '](/outer)'));
  assert.equal(render('[https://example.test]'), paragraph('[' + anchor('https://example.test]')));
});

test('code mathematics HTML and note identifiers cannot publish hidden bare links', () => {
  const contents = 'https://example.test team@example.test mailto:team@example.test';
  for (const source of ['`' + contents + '`', '$' + contents + '$', '<custom title="' + contents + '">', '<!-- ' + contents + ' -->']) {
    const html = render('before ' + source + ' https://visible.test');
    assert.equal((html.match(/<a /g) || []).length, 1, source);
    assert.ok(html.includes(anchor('https://visible.test')), source);
    assert.ok(html.includes(escape(contents)), source);
  }
  for (const source of ['```\n' + contents + '\n```', '    ' + contents, '<div>\n' + contents + '\n</div>', '$$\n' + contents + '\n$$']) {
    assert.doesNotMatch(render(source), /<a /, source);
  }
  const note = render('Use[^team@example.test]\n\n[^team@example.test]: Actual body');
  assert.doesNotMatch(note, /href="mailto:/);
  assert.match(note, /Actual body/);
});

test('URL bodies own Markdown-looking bytes without consulting hidden image resources', () => {
  const assets = {has() { assert.fail('bare URL consulted an image binding'); }};
  for (const body of ['[hidden](/wrong)', '![hidden](private.png)', '[^hidden]', '$x$', '`code`', '*literal*tail']) {
    const url = 'https://example.test/' + body;
    assert.equal(render(url + ' [visible](/visible)\n\n[^hidden]: SECRET', assets),
      paragraph(anchor(url) + ' ' + anchor('visible', '/visible')), body);
  }
});

test('a URL-owned opener cannot hide the real Markdown released after its body', () => {
  for (const [url, tail, expected] of [
    ['https://example.test/`first', ' [visible](/visible) `', ' ' + anchor('visible', '/visible') + ' `'],
    ['https://example.test/$first', ' [visible](/visible) $', ' ' + anchor('visible', '/visible') + ' $'],
    ['https://example.test/[first', ' visible](/wrong) [kept](/kept)', ' visible](/wrong) ' + anchor('kept', '/kept')],
    ['https://example.test/`first', ' [https://chosen.test](/chosen) `', ' ' + anchor('https://chosen.test', '/chosen') + ' `'],
    ['https://example.test/`first', ' $value$ [next](/next) `', ' $value$ ' + anchor('next', '/next') + ' `'],
  ]) assert.equal(render(url + tail), paragraph(anchor(url) + expected), url + tail);
});

test('source boundaries and malformed angles cannot manufacture autolinks', () => {
  for (const text of ['xhttps://example.test', 'éhttps://example.test', 'https\\://example.test', 'www\\.example.test', 'mailto\\:team@example.test']) {
    assert.doesNotMatch(render(text), /<a /, text);
  }
  for (const text of ['&#104;ttps://example.test', 'https&colon;//example.test', 'team&commat;example.test', 'www&period;example.test']) {
    assert.doesNotMatch(render(text), /<a /, text);
  }
  for (const address of ['https://example.test/path', 'team@example.test', 'mailto:team@example.test']) {
    const url = address.includes(':') ? address : 'mailto:' + address;
    assert.equal(render('< ' + address + ' >'), paragraph('&lt; ' + address + ' &gt;'));
    assert.equal(render('\\< ' + address + ' >'), paragraph('&lt; ' + anchor(address, url) + ' &gt;'));
    assert.equal(render('< ' + address + ' \\>'), paragraph('&lt; ' + anchor(address, url) + ' &gt;'));
    for (const space of ['\u0085', '\u00a0', '\u2003']) {
      assert.equal(render('before' + space + address), paragraph('before' + space + anchor(address, url)));
    }
    assert.doesNotMatch(render('before\ufeff' + address), /<a /);
  }
});

test('bare targets retain literal source and unchanged URL admission', () => {
  for (const url of ['https://example.test/a\\*b', 'https://example.test/a\u0001b', 'https://example.test/a\u007fb']) {
    assert.equal(render(url), paragraph(escape(url)), url);
  }
  assert.equal(render('https://example.test/\ufeff'), paragraph(anchor('https://example.test/\ufeff')));
  assert.equal(render('javascript:team@example.test data:team@example.test file:team@example.test'),
    paragraph('javascript:team@example.test data:team@example.test file:team@example.test'));
  assert.equal(render('https://example.test/?a=1&amp;b=2 [chosen](https://example.test/?a=1&amp;b=2)'),
    paragraph(anchor('https://example.test/?a=1&amp;b=2') + ' ' + anchor('chosen', 'https://example.test/?a=1&b=2')));
});

test('headings tables quotes lists and footnotes share bare-link semantics across line endings', () => {
  for (const newline of ['\n', '\r\n', '\r']) {
    const source = '# https://title.test\n\n> Contact team@example.test\n\n- www.list.test\n\nSite | Action\n--- | ---\nhttps://table.test | Open\n\nUse[^n]\n\n[^n]: mailto:notes@example.test';
    const html = render(source.replace(/\n/g, newline));
    for (const [label, destination] of [
      ['https://title.test', 'https://title.test'], ['team@example.test', 'mailto:team@example.test'],
      ['www.list.test', 'http://www.list.test'], ['https://table.test', 'https://table.test'],
      ['mailto:notes@example.test', 'mailto:notes@example.test'],
    ]) assert.equal(html.split(anchor(label, destination)).length - 1, 1, newline + label);
    assert.match(html, /<h1 id="httpstitletest">/);
  }
});

test('long valid targets and many ordinary links remain bounded and complete', () => {
  const bounded = vm.createContext({});
  vm.runInContext(code, bounded);
  const run = source => { bounded.source = source; return vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 5000}); };
  const address = 'a_'.repeat(16000) + 'z@example.test';
  assert.equal(run(address), paragraph(anchor(address, 'mailto:' + address)));
  const url = 'https://example.test/' + 'a_'.repeat(16000) + 'end';
  assert.equal(run(url), paragraph(anchor(url)));
  const tail = '&unknown;'.repeat(16000);
  assert.equal(run('https://example.test/' + tail), paragraph(anchor('https://example.test/') + '&amp;unknown;'.repeat(16000)));
  const count = 2000;
  const html = run('team@example.test https://example.test/a(b) www.example.test '.repeat(count));
  assert.equal((html.match(/<a /g) || []).length, count * 3);
});

test('failed speculation and suffix reindexing obey typed budgets and reset document state', () => {
  const bounded = vm.createContext({});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 600000'), bounded);
  for (const hostile of [
    'a_'.repeat(4096) + '@invalid',
    'www.a_'.repeat(4096) + 'bad.test',
    // Distinct delimiter widths keep each URL's speculative code atom separate;
    // repeated equal widths legitimately join into later code instead.
    Array.from({length: 32}, (_,i) => 'https://example.test/' + '`'.repeat(i + 1)
      + 'first [hidden](/hidden) ' + '`'.repeat(i + 1) + ' ').join(''),
  ]) {
    bounded.source = '[discard](/discard) ' + hostile + '\n\n[hidden]: /wrong';
    assert.throws(() => vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}),
      error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
    assert.equal(vm.runInContext('parseMarkdownClient("[hidden] https://fresh.test")', bounded),
      paragraph('[hidden] ' + anchor('https://fresh.test')));
  }
  const small = vm.createContext({source: 'https://example.test '.repeat(200)});
  vm.runInContext(code.replace('structures: 262144', 'structures: 100'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'structure');
  assert.equal(vm.runInContext('parseMarkdownClient("https://fresh.test")', small), paragraph(anchor('https://fresh.test')));
});
