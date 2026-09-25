import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';

// Execute the private production counter, not a reimplementation. Browser tests
// below exercise the complete controller and its actual serialization/downloads.
const controller = readFileSync(new URL('../src/interactive_controller.js', import.meta.url), 'utf8');
const begin = controller.indexOf('  function workspaceTextSize(');
const end = controller.indexOf('\n  function saveRecoveryCopy()', begin);
assert.ok(begin >= 0 && end > begin);
const count = runInNewContext('(' + controller.slice(begin, end) + ')');

test('workspace UTF-8 accounting matches encoding for exact source and embedded JSON', () => {
  for (const text of ['', '\ufeff# Source\r\n', 'é中𝄞', '\0\u007f\u0080\u07ff\u0800\uffff',
    '</script><!--\u2028\u2029', JSON.stringify({bytes: 'AQID', bindings: 'export default function() {}'})]) {
    const bytes = Buffer.byteLength(text);
    assert.equal(count(text, bytes, 'test'), bytes);
    if (bytes) assert.throws(() => count(text, bytes - 1, 'test'), /byte limit/);
  }
});

test('workspace counter refuses malformed UTF-16 rather than replacing source', () => {
  for (const text of ['\ud800', '\udc00', 'a\ud800b', '\ud800\ud800', '\udc00\ud800', '𝄞\udfff']) {
    assert.throws(() => count(text, 100, 'source'), /invalid Unicode/);
  }
});

test('workspace counter checks native limits without allocating another encoded buffer', () => {
  const limit = 32 * 1024 * 1024;
  assert.equal(count('x'.repeat(limit), limit, 'source'), limit);
  assert.throws(() => count('x'.repeat(limit + 1), limit, 'source'), /byte limit/);
  assert.throws(() => count('中'.repeat(Math.floor(limit / 3) + 1), limit, 'source'), /byte limit/);
});

test('workspace counter does not coerce non-string source into a saveable document', () => {
  for (const value of [null, undefined, 3, {}, new Uint8Array([1])]) {
    assert.throws(() => count(value, 1024, 'source'), /byte limit/);
  }
});
