// Inline HTML source ownership in the production saved-document renderer.
// Markup stays escaped; only the parser's interpretation of its contents changes.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = (source, assets) => context.parseMarkdownClient(source, assets);
const escape = text => text.replace(/[&<>"']/g, ch => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[ch]);
const literal = text => '<p>' + escape(text) + '</p>\n';
const bodies = [
  '<custom title="[hidden](/hidden) **literal**">',
  "<custom title='[hidden](/hidden) **literal**'/>",
  '<!-- [hidden](/hidden) **literal** -->',
  '<?processing [hidden](/hidden) **literal** ?>',
  '<!doctype [hidden](/hidden) **literal**>',
  '<![CDATA[ [hidden](/hidden) **literal** ]]>',
];

test('inline tags, comments, processing instructions, declarations and CDATA stay inert', () => {
  for (const atom of [...bodies, '</custom-tag \t>', '<custom _flag :ns.x-y=ok/>']) {
    const source = 'before ' + atom + ' [visible](/visible)';
    assert.equal(render(source), '<p>before ' + escape(atom) + ' <a href="/visible">visible</a></p>\n', atom);
  }
});

test('all ASCII declaration initials own their contents without a required name or space', () => {
  for (const first of 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ') {
    for (const tail of ['', '=[hidden](/hidden)', ' [hidden](/hidden)', '-x']) {
      const atom = '<!' + first + tail + '>';
      assert.equal(render('before ' + atom + ' after'), literal('before ' + atom + ' after'), atom);
    }
  }
});

test('HTML brackets cannot retarget links, images or their reference labels', () => {
  for (const atom of bodies) {
    const label = 'before ' + atom + ' after';
    for (const suffix of ['(/chosen)', '[ref]']) {
      const source = '[' + label + ']' + suffix + ' [later](/later)\n\n[ref]: /chosen';
      assert.equal(render(source), '<p><a href="/chosen">' + escape(label) + '</a> <a href="/later">later</a></p>\n', atom);
    }
    assert.equal(render('![' + label + '](cover.png)'), '<p><img src="cover.png" alt="' + escape(label) + '"></p>\n', atom);
  }
});

test('HTML-owned image and note syntax never consults resources or publishes hidden notes', () => {
  const payload = '![hidden](secret.png) [^hidden] [link](/hidden)';
  const atoms = [
    '<custom title="' + payload + '">', '<!-- ' + payload + ' -->',
    '<?p ' + payload + ' ?>', '<!d ' + payload + '>', '<![CDATA[' + payload + ']]>',
  ];
  const lookedUp = [], assets = {has(url) { lookedUp.push(url); return false; }};
  for (const atom of atoms) {
    const source = 'before ' + atom + ' real[^shown]\n\n[^hidden]: SECRET BODY\n[^shown]: Real body';
    const html = render(source, assets);
    assert.ok(html.includes(escape(atom)), atom);
    assert.doesNotMatch(html, /href="\/hidden"|<img|SECRET BODY/);
    assert.equal((html.match(/class="footnote-ref"/g) || []).length, 1);
    assert.equal((html.match(/Real body/g) || []).length, 1);
  }
  assert.deepEqual(lookedUp, []);
});

test('emphasis searches skip HTML-owned delimiters and preserve their literal escapes', () => {
  const tick = String.fromCharCode(96);
  for (const marker of ['*', '**', '_', '~~']) {
    const tag = '<custom title="' + marker + ' [hidden](/hidden) ' + tick + ' &amp; \\*">';
    const [open, close] = marker === '**' ? ['<strong>', '</strong>']
      : marker === '~~' ? ['<del>', '</del>'] : ['<em>', '</em>'];
    assert.equal(render(marker + 'before ' + tag + ' after' + marker),
      '<p>' + open + 'before ' + escape(tag) + ' after' + close + '</p>\n');
  }
});

test('quoted angles, backslashes and entities retain exact HTML source boundaries', () => {
  const tag = '<custom title="a > [hidden](/hidden) &amp; \\*">';
  assert.equal(render('before ' + tag + ' [next](/next)'),
    '<p>before ' + escape(tag) + ' <a href="/next">next</a></p>\n');
  // HTML does not give a backslash authority to escape its closing quote.
  const closed = '<custom title="\\">';
  assert.equal(render('before ' + closed + '[next](/next)'),
    '<p>before ' + escape(closed) + '<a href="/next">next</a></p>\n');
});

test('short comments and first terminators release all following Markdown', () => {
  for (const atom of ['<!-->', '<!--->', '<!-- body -->', '<?p [hidden](/hidden)?>',
    '<!d [hidden](/hidden)>', '<![CDATA[[hidden](/hidden)]]>']) {
    assert.equal(render('before ' + atom + ' [visible](/visible) -->'),
      '<p>before ' + escape(atom) + ' <a href="/visible">visible</a> --&gt;</p>\n', atom);
  }
});

test('code spans and escaped openers retain precedence over HTML recognition', () => {
  const tick = String.fromCharCode(96), atom = '<!d [hidden](/hidden)>';
  assert.equal(render(tick + atom + tick + ' [visible](/visible)'),
    '<p><code>' + escape(atom) + '</code> <a href="/visible">visible</a></p>\n');
  assert.equal(render('\\<!d [visible](/visible)>'),
    '<p>&lt;!d <a href="/visible">visible</a>&gt;</p>\n');
  assert.equal(render('[one](/one "<!d") [two](/two) >'),
    '<p><a href="/one" title="&lt;!d">one</a> <a href="/two">two</a> &gt;</p>\n');
});

test('invalid and unterminated HTML keeps live Markdown and following valid atoms', () => {
  for (const prefix of ['<!1 ', '<!é ', '<!_ ', '<! ', '<!d ', '<!-- ', '<?p ', '<![CDATA[ ',
    '<custom title="', '<custom data="x"other="', '</custom data="', '<custom / ']) {
    const html = render('before ' + prefix + '[visible](/visible)');
    assert.match(html, /<a href="\/visible">visible<\/a>/, prefix);
  }
  const malformed = '<custom title="x"other="[live](/live)">';
  assert.match(render('before ' + malformed + ' <!d [hidden](/hidden)> [next](/next)'),
    /href="\/live"/);
  assert.doesNotMatch(render('before ' + malformed + ' <!d [hidden](/hidden)> [next](/next)'),
    /href="\/hidden"/);
});

test('multiline inline atoms retain ownership inside headings, lists, quotes and notes', () => {
  const tag = '<custom\n title="[hidden](/hidden)">', inline = 'before ' + tag + ' [visible](/visible)';
  for (const newline of ['\n', '\r', '\r\n']) {
    for (const source of [
      inline,
      '- ' + inline.replace(/\n/g, '\n  '),
      '> ' + inline.replace(/\n/g, '\n> '),
      'Use[^n].\n\n[^n]: ' + inline.replace(/\n/g, '\n    '),
    ]) {
      const html = render(source.replace(/\n/g, newline));
      assert.ok(html.includes(escape(tag)), source);
      assert.match(html, /href="\/visible"/);
      assert.doesNotMatch(html, /href="\/hidden"/);
    }
  }
  assert.equal(render('# before <!d [hidden](/hidden)> [visible](/visible)'),
    '<h1 id="before-d-hiddenhidden-visible">before &lt;!d [hidden](/hidden)&gt; <a href="/visible">visible</a></h1>\n');
});

test('many valid HTML atoms consume bounded work and preserve surrounding links', () => {
  const count = 2000, atom = '<custom title="[hidden](/hidden)">', piece = atom + ' [visible](/visible) ';
  const bounded = vm.createContext({source: 'before ' + piece.repeat(count)});
  vm.runInContext(code, bounded);
  const html = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 5000});
  assert.equal((html.match(/href="\/visible"/g) || []).length, count);
  assert.equal((html.match(/&lt;custom/g) || []).length, count);
  assert.doesNotMatch(html, /href="\/hidden"/);
});

test('failed HTML lookahead obeys the shared work limit and releases request state', () => {
  const source = 'before ' + '<custom title="'.repeat(8000);
  const small = vm.createContext({source});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 600000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}),
    error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("[visible](/visible)")', small),
    '<p><a href="/visible">visible</a></p>\n');
});
