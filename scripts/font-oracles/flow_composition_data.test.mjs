// Independent ICU oracle over the ACTUAL production composition table.
// This validates Unicode data; it does not execute the Rust renderer.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../../src/fonts/flow/positioned/composition.rs', import.meta.url), 'utf8');
const rows = [...source.matchAll(/\('\\u\{([0-9a-f]+)\}', "([^"]*)", "([^"]*)"\)/gu)]
  .map(([, hex, bases, composites]) => [String.fromCodePoint(parseInt(hex, 16)), [...bases], [...composites]]);
const pairs = new Map();
for (const [mark, bases, composites] of rows) {
  bases.forEach((base, index) => pairs.set(base + mark, composites[index]));
}
test('all 497 production pairs agree with ICU canonical composition and decomposition', () => {
  assert.equal(rows.length, 25);
  assert.equal(pairs.size, 497);
  let previous = 0;
  for (const [mark, bases, composites] of rows) {
    assert.ok(mark.codePointAt(0) > previous);
    previous = mark.codePointAt(0);
    assert.equal(bases.length, composites.length);
    for (let i = 0; i < bases.length; i++) {
      if (i) assert.ok(bases[i].codePointAt(0) > bases[i - 1].codePointAt(0));
      assert.equal((bases[i] + mark).normalize('NFC'), composites[i]);
      assert.equal((bases[i] + mark).normalize('NFD'), composites[i].normalize('NFD'));
    }
  }
});
test('every two-step composition chain retains canonical identity', () => {
  let chains = 0;
  for (const [input, middle] of pairs) {
    for (const [next, final] of pairs) {
      const [base, mark] = [...next];
      if (middle === base) {
        assert.equal((input + mark).normalize('NFD'), final.normalize('NFD'));
        assert.equal((input + mark).normalize('NFC'), final);
        chains++;
      }
    }
  }
  assert.ok(chains > 50);
});
