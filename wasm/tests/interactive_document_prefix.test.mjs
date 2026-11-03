// Document-prefix ownership through the production standalone renderer.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => context.parseMarkdownClient(source);
const prefix = (metadata, body = '# Body\n\nVisible.') => `---\n${metadata}\n---\n${body}`;
const endings = source => {
  // Adjacent CR then LF would become one CRLF and erase a blank line. The
  // mixed cycle keeps every original break distinct, including empty lines.
  let index = 0;
  return [source, source.replace(/\n/g, '\r\n'), source.replace(/\n/g, '\r'),
    source.replace(/\n/g, () => ['\r\n', '\n', '\r'][index++ % 3])];
};

test('a leading BOM cannot turn code definitions into document definitions', () => {
  const source = '\ufeff```\n\n[secret]: /wrong\n[^n]: Hidden note\n```\n\n[secret] [^n]';
  assert.equal(render(source), '<pre><code>\n[secret]: /wrong\n[^n]: Hidden note\n</code></pre>\n<p>[secret] [^n]</p>\n');
});

test('a single initial BOM is transparent to supported leading blocks', () => {
  for (const source of [
    '# Heading', '- first\n- second', '> quoted', '    code', '~~~js\ncode\n~~~',
    '[r]: /right\n\n[r]', '[^n]: A note\n\nUse[^n].', '| A | B |\n|---|---|\n| 1 | 2 |',
    '---\n\nBody', '', '\r\n# Heading\r\n',
  ]) assert.equal(render('\ufeff' + source), render(source), JSON.stringify(source));
});

test('interior BOM characters remain source, including after consumed metadata', () => {
  assert.equal(render('before \ufeff after'), '<p>before \ufeff after</p>\n');
  assert.equal(render('\ufeff\ufeff# literal'), '<p>\ufeff# literal</p>\n');
  assert.equal(render('before\n\n\ufeff# literal'), '<p>before</p>\n<p>\ufeff# literal</p>\n');
  assert.equal(render(prefix('title: Metadata', '\ufeff# literal')), '<p>\ufeff# literal</p>\n');
});

test('valid leading metadata does not render headings, rules or paragraphs', () => {
  for (const metadata of [
    'title=A document\nauthor=Jane\nlang=en\ntoc=true\ntoc_depth=2',
    'title: "A document"\nauthor: \'Jane\'\nlang: fr\ntoc: yes\ntoc_depth: 3',
    '# Comment\n\nunknown: retained metadata\n  # another comment',
    'title: "E = mc²: a guide"\nauthor: https://example.test/?a=b\nlang=en:custom',
    'title: unmatched quote "\ntoc: unsupported value\ntoc_depth: 99',
    '\ufeff: An interior BOM is a nonempty key',
  ]) assert.equal(render(prefix(metadata)), render('# Body\n\nVisible.'), metadata);
});

test('metadata definitions cannot leak links, images, notes or duplicate precedence', () => {
  const metadata = 'title: Document\n\n[ref]: /wrong "Hidden"\n[^n]: PRIVATE NOTE';
  assert.equal(render(prefix(metadata, '[ref] ![image][ref] [^n]')), '<p>[ref] ![image][ref] [^n]</p>\n');
  const body = '[ref] ![image][ref] [^n]\n\n[ref]: /right\n[^n]: Visible note';
  assert.equal(render(prefix(metadata, body)), render(body));
  assert.doesNotMatch(render(prefix(metadata, body)), /PRIVATE|\/wrong|Hidden/);
});

test('all native literal and folded block-scalar headers own their entire content', () => {
  for (const style of ['|', '|-', '|+', '>', '>-', '>+']) {
    const metadata = `title: ${style} # header comment\n\n  [ref]: /wrong\n  # not a heading\n  ---\n  \`\`\`\n  [^n]: PRIVATE NOTE\n    more indented\n\n  final\nauthor: Jane`;
    assert.equal(render(prefix(metadata, '[ref] [^n]')), '<p>[ref] [^n]</p>\n', style);
  }
});

test('empty scalars, blank indentation and unknown scalar keys match native admission', () => {
  for (const metadata of [
    'title: |\n\nlang: en', 'title: |-\n    \n  ', 'notes: >+\n  content\ntitle: Visible',
    'title: |+\n\n  first', 'title: |-\n  \u00a0', '  title: |\n    content\nlang: en',
    'title: |#literal\nother: still an ordinary assignment',
  ]) assert.equal(render(prefix(metadata)), render('# Body\n\nVisible.'), metadata);
});

test('invalid scalar indentation preserves the entire Markdown source', () => {
  for (const metadata of [
    'title: |\n    first\n  partial dedent',
    'title: |\n    \n  text',
    'title: |\n  first\nunindented prose',
    'title: |2\n  unsupported explicit indentation',
    'title: |\n\ttext without space indentation',
    'title: |\n  first\n# metadata comment\n  ordinary prose after the scalar',
  ]) {
    const source = prefix(metadata);
    assert.equal(render(source), render('\n' + source), metadata);
    assert.match(render(source), /Body/);
  }
});

test('empty, comment-only, unclosed and malformed prefixes retain their content', () => {
  for (const source of [
    '---\n---\n# Body', '---\n# only a comment\n\n---\n# Body',
    '---\ntitle: An unclosed block\n', '---\ntitle: |\n  unclosed scalar\n',
    '---\nordinary prose\n---\n# Body', '---\n=not a key\n---\n# Body',
    '---\n: not a key\n---\n# Body', '---\n\u00a0: not a key\n---\n# Body',
    '---\ntitle: Text\n ---\n# Body',
  ]) assert.equal(render(source), render('\n' + source), JSON.stringify(source));
});

test('metadata requires the exact first-line delimiter at the document start', () => {
  for (const beginning of ['\n', ' ', '\t', 'before\n\n', '> ']) {
    const source = beginning + prefix('title: Visible metadata-shaped content');
    assert.match(render(source), /Visible metadata-shaped content/);
  }
  for (const opener of ['--- ', '---\t', '----']) {
    assert.match(render(opener + '\ntitle: Visible metadata-shaped content\n---\nBody'), /Visible metadata-shaped content/);
  }
});

test('BOM and every native line ending retain the exact following body semantics', () => {
  const body = '# Body\n\n[ref] [^n]\n\n[ref]: /right\n[^n]: Visible';
  const source = '\ufeff---\ntitle: >-\n  first\n  second\n---\n' + body;
  for (const variant of endings(source)) assert.equal(render(variant), render(body));
  const crOnly = '---\rtitle: Visible\r---\rBody';
  assert.equal(render(crOnly), '<p>Body</p>\n');
  for (const ending of ['\n', '\r\n', '\r']) {
    assert.equal(render(`---${ending}title: Metadata${ending}---${ending}\ufeff# literal`), '<p>\ufeff# literal</p>\n');
  }
});

test('mixed metadata endings retain scalar and definition ownership across body blocks', () => {
  const bodies = [
    '# Body\n\nVisible.', '> [ref] [^n]\n\n[ref]: /right\n[^n]: Visible note',
    '- first\n- second\n\n[secret] [^private]',
    '```text\n\n[secret]: /literal\n```\n\n[secret]',
  ];
  for (const style of ['|', '|-', '|+', '>', '>-', '>+']) {
    const metadata = `title: ${style} # comment\n\n  [secret]: /wrong\n  [^private]: PRIVATE NOTE\n  ---\n\n  final\nauthor: Jane`;
    for (const body of bodies) {
      for (const source of endings('\ufeff' + prefix(metadata, body))) {
        assert.equal(render(source), render(body), JSON.stringify(source));
        assert.doesNotMatch(render(source), /PRIVATE NOTE|\/wrong/);
      }
    }
  }
});

test('invalid metadata retains all content with CR and mixed source boundaries', () => {
  for (const source of [
    '---\n---\n# Body', '---\n# comment only\n---\n# Body',
    '---\ntitle: unclosed\n# Body', '---\ntitle: |\n  unclosed scalar\n',
    prefix('title: |\n    first\n  partial dedent'),
    prefix('title: |\n    \n  text'), prefix('title: |\n\tunsupported indentation'),
    prefix('title: Visible\nordinary prose'), prefix('=empty key'),
    '---\ntitle: Text\n ---\n# Body',
  ]) {
    for (const variant of endings(source)) {
      assert.equal(render(variant), render('\n' + variant), JSON.stringify(variant));
      assert.equal(render(variant), render(source), JSON.stringify(variant));
    }
  }
});

test('a later metadata-looking block keeps its normal Markdown ownership', () => {
  const later = '---\n[ref]: /right\n[^n]: Visible\n---\n\n[ref] [^n]';
  const source = prefix('title: First metadata', '# Body\n\n' + later);
  assert.equal(render(source), render('# Body\n\n' + later));
  assert.match(render(source), /href="\/right"/);
  assert.match(render(source), /Visible/);
});

test('metadata scanning shares admission and work budgets and resets after failure', () => {
  const source = prefix('title: |-\n' + '  literal metadata\n'.repeat(10000), 'Body');
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}), '<p>Body</p>\n');
  const small = vm.createContext({source});
  vm.runInContext(code.replace('workUnits: 32 * 1024 * 1024', 'workUnits: 300000'), small);
  assert.throws(() => vm.runInContext('parseMarkdownClient(source)', small, {timeout: 4000}), error => error.code === 'FMD_RENDER_LIMIT' && error.limit === 'work');
  assert.equal(vm.runInContext('parseMarkdownClient("---\\ntitle: Fresh\\n---\\nBody")', small), '<p>Body</p>\n');
});

test('long metadata whitespace and interior carriage-return runs use bounded scans', () => {
  const source = prefix('title: ' + ' '.repeat(100000) + 'literal' + '\r'.repeat(100000) + 'author: value', 'Body');
  const bounded = vm.createContext({source});
  vm.runInContext(code, bounded);
  assert.equal(vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000}), '<p>Body</p>\n');
  // Lone CR is a boundary, so ordinary prose after the run invalidates the
  // whole candidate. Preserve that content instead of hiding it as a value.
  bounded.source = source.replace('author: value', 'ordinary prose');
  const retained = vm.runInContext('parseMarkdownClient(source)', bounded, {timeout: 4000});
  assert.match(retained, /literal/);
  assert.match(retained, /ordinary prose/);
  assert.match(retained, /Body/);
});
