// Exercise the actual saved-document renderer: a long interior whitespace run
// must not turn final paragraph trimming into an uninterruptible suffix search.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const code = readFileSync(new URL('../../src/interactive_renderer.js', import.meta.url), 'utf8');
const context = vm.createContext({});
vm.runInContext(code, context);
const render = source => {
  context.source = source;
  return vm.runInContext('parseMarkdownClient(source)', context, {timeout: 4000});
};

test('large interior ASCII whitespace remains complete without blocking the preview', () => {
  for (const padding of [' '.repeat(100000), '\t'.repeat(100000), ' \t'.repeat(50000)]) {
    assert.equal(render('Before a' + padding + 'b [kept](/kept)'),
      '<p>Before a' + padding + 'b <a href="/kept">kept</a></p>\n');
  }
});

test('literal inline content and nested container paragraphs keep every interior byte', () => {
  const padding = ' \t'.repeat(50000), codeSpan = '`a' + padding + 'b`';
  for (const source of [
    'Before ' + codeSpan + ' [kept](/kept)',
    '> Before ' + codeSpan + ' [kept](/kept)',
    '- Before ' + codeSpan + ' [kept](/kept)',
    'Use[^n]\n\n[^n]: Before ' + codeSpan + ' [kept](/kept)',
  ]) {
    const html = render(source);
    assert.ok(html.includes('<code>a' + padding + 'b</code>'));
    assert.equal(html.split('<a href="/kept">kept</a>').length - 1, 1);
  }
});

test('only terminal ASCII padding is trimmed and real source breaks stay intact', () => {
  const padding = ' \t'.repeat(50000);
  assert.equal(render('Before' + padding), '<p>Before</p>\n');
  for (const last of ['\u00a0', '\u0085', '\u2003', '\ufeff']) {
    assert.equal(render('Before' + padding + last + ' \t'), '<p>Before' + padding + last + '</p>\n');
  }
  assert.equal(render('Before  \n  `a' + padding + 'b` \t'),
    '<p>Before<br>\n<code>a' + padding + 'b</code></p>\n');
});
