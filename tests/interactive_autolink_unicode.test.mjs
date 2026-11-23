// Run the production offline editor parser. URI syntax ownership must stay
// independent of HTML URL admission, especially inside another link label.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = source => context.parseMarkdownClient(source);

test('angle URIs own Unicode whitespace throughout their literal destinations', () => {
  for (const character of ['\u00a0', '\u1680', '\u2003', '\u2028', '\u2029', '\u202f', '\u3000']) {
    const destination = 'https://example.test/a' + character + 'b?q=one&two=three';
    const attribute = destination.replaceAll('&', '&amp;');
    assert.equal(render('<' + destination + '>'),
      '<p><a href="' + attribute + '">' + attribute + '</a></p>\n');
  }
  const destination = 'https://example.test/trailing\u00a0';
  assert.equal(render('<' + destination + '>'),
    '<p><a href="' + destination + '">' + destination + '</a></p>\n');
});

test('a Unicode angle URI prevents an enclosing link from taking its target', () => {
  const destination = 'https://example.test/a\u00a0b';
  assert.equal(render('[outer <' + destination + '>](/outside)'),
    '<p>[outer <a href="' + destination + '">' + destination + '</a>](/outside)</p>\n');
  assert.equal(render('![<' + destination + '>](picture.png)'),
    '<p><img src="picture.png" alt="' + destination + '"></p>\n');
  assert.equal(render('`<' + destination + '>`'),
    '<p><code>&lt;' + destination + '&gt;</code></p>\n');
});

test('URI recognition does not relax unsafe-scheme or control-character URL admission', () => {
  for (const destination of [
    'javascript:alert\u00a0(1)',
    'https://example.test/a\u0085b',
    'https://example.test/a\u009fb',
  ]) {
    assert.equal(render('[outer <' + destination + '>](/outside)'),
      '<p>[outer &lt;' + destination + '&gt;](/outside)</p>\n');
  }
});

test('ASCII exclusions and the separate email grammar still reject invalid angle targets', () => {
  for (const character of [' ', '\t', '\u0001', '\u000b', '\u000c', '\u001f', '\u007f']) {
    assert.doesNotMatch(render('<https://example.test/a' + character + 'b>'), /<a\b/);
  }
  assert.doesNotMatch(render('<reader@\u00a0example.test>'), /<a\b/);
});
