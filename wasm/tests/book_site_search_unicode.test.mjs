import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
function load(instrument = "") {
  const context = vm.createContext({ module: { exports: {} }, setTimeout, clearTimeout, URL });
  vm.runInContext(instrument + script, context);
  return { api: context.module.exports, context };
}
const { api } = load();
const row = (text, title = "Guide") => ({ text, title, source: "guide.md", page: "guide.html", anchor: "part", heading: false, order: 0 });
const noWait = { yieldTask: async () => {} };
const pairs = [
  ["Café", "Cafe\u0301"],
  ["Ångström", "A\u030Angstro\u0308m"],
  ["ΐ", "ι\u0308\u0301"],
  ["각", "각"],
  ["Йога", "И\u0306ога"],
  ["a\u0315\u0300", "a\u0300\u0315"],
  ["ש\u05C1\u05B0", "ש\u05B0\u05C1"],
];

test("canonically equivalent Latin, Greek, Cyrillic, Hebrew and Hangul forms match both ways", async () => {
  for (const [a, b] of pairs) {
    for (const [source, query] of [[a, b], [b, a]]) {
      const result = await api.search([row(source)], query, noWait);
      assert.equal(result.total, 1, `${JSON.stringify(source)} / ${JSON.stringify(query)}`);
      assert.equal(result.results[0].offset, 0);
      assert.equal(result.results[0].score, 55);
    }
  }
  assert.deepEqual(Array.from(api.parseQuery('café "cafe\u0301"')), ["cafe\u0301"]);
});

test("canonical normalization does not strip requested accents or fold compatibility glyphs", async () => {
  for (const [source, query] of [["cafe", "café"], ["ﬀ", "ff"], ["①", "1"], ["alpha", "αλφα"]]) {
    assert.equal((await api.search([row(source)], query, noWait)).total, 0, `${source} / ${query}`);
  }
});

test("normalization and canonical mark reordering remain correct at every chunk edge", async () => {
  for (const padding of [3837, 3838, 3839, 3840, 3841, 4093, 4095, 4096, 7678, 8191]) {
    for (const [a, b] of pairs) {
      const prefix = "x".repeat(padding);
      const result = await api.search([row(prefix + a)], b, noWait);
      assert.equal(result.total, 1, `${padding}: ${a} / ${b}`);
      assert.equal(result.results[0].offset, padding);
      assert.ok(api.excerpt(prefix + a, result.terms, result.results[0].offset).includes(a));
    }
  }
});

test("lazy maps retain the source origin of reordered and decomposed matching scalars", async () => {
  const cases = [
    ["prefix a\u0315\u0300", "\u0300", 9],
    ["prefix a\u0315\u0300", "\u0315", 8],
    ["prefix é", "\u0301", 7],
    ["prefix 각", "ᆨ", 7],
    ["😀 a\u0315\u0300 a\u0315\u0300", "\u0300", 5],
  ];
  for (const [source, query, offset] of cases) {
    const result = await api.search([row(source)], query, noWait);
    assert.equal(result.total, 1);
    assert.equal(result.results[0].offset, offset, `${JSON.stringify(source)} / ${JSON.stringify(query)}`);
    assert.equal(result.results[0].passages[0].offset, offset);
  }
});

test("equivalent words distributed across paragraphs retain their original display text", async () => {
  const data = [row("A cafe\u0301 nearby"), { ...row("Ångström scale"), order: 1 }];
  const result = await api.search(data, "café A\u030Angstro\u0308m", noWait);
  assert.equal(result.total, 1);
  const text = result.results[0].passages.map(hit => api.excerpt(hit.row.text, [], hit.offset)).join(" ");
  assert.ok(text.includes("cafe\u0301"));
  assert.ok(text.includes("Ångström"));
});

test("oversized combining continuations fail explicitly rather than splitting equivalence", async () => {
  const source = "x".repeat(3838) + "a" + "\u0315".repeat(5000) + "\u0300 needle";
  await assert.rejects(api.search([row(source)], "needle", noWait), /normalization.*limit/i);
  // Refusal is per search, with no poisoned shared normalizer state.
  assert.equal((await api.search([row("cafe\u0301")], "café", noWait)).total, 1);
});

test("normalization work stays bounded and cancellation stops a long canonical-equivalence scan", async () => {
  const { api, context } = load(`
    globalThis.largestLower = 0;
    const lower = String.prototype.toLowerCase;
    String.prototype.toLowerCase = function () {
      globalThis.largestLower = Math.max(globalThis.largestLower, this.length);
      return lower.call(this);
    };
  `);
  let stopped = false, yields = 0;
  const result = await api.search([row("é".repeat(500000) + "needle")], "needle", {
    cancelled: () => stopped,
    yieldTask: async () => { stopped = true; yields++; },
  });
  assert.equal(result, null);
  assert.equal(yields, 1);
  assert.ok(context.largestLower <= 4096);
});

test("misses still avoid all scalar source maps even with canonical decomposition", async () => {
  const { api, context } = load(`
    globalThis.mapped = 0;
    const scalar = String.fromCodePoint;
    String.fromCodePoint = function (...args) { globalThis.mapped++; return scalar(...args); };
  `);
  assert.equal((await api.search([row("é".repeat(100000))], "missing", noWait)).total, 0);
  assert.equal(context.mapped, 0);
});

test("streaming canonical matches agree with independent whole-string NFD normalization", async () => {
  const normalized = value => value.toLowerCase().replace(/ς/gu, "σ").normalize("NFD").replace(/\s+/gu, " ");
  let state = 0x17339;
  const next = () => { state = Math.imul(state, 1103515245) + 12345 | 0; return state >>> 0; };
  const alphabet = ["x", "é", "e\u0301", "🙂", "a\u0315\u0300", "a\u0300\u0315", "ΐ", "각", "각", "Å", "A\u030A", "\n", "\t"];
  for (let trial = 0; trial < 90; trial++) {
    const source = Array.from({ length: 4100 + next() % 2000 }, () => alphabet[next() % alphabet.length]).join("");
    const query = ["cafe\u0301", "é", "각", "\u0300\u0315", "Å", '"ΐ 각"'][trial % 6];
    const terms = Array.from(api.parseQuery(query));
    const body = normalized(source);
    const expected = terms.every(term => body.includes(term));
    const result = await api.search([row(source)], query, noWait);
    assert.equal(result.total, Number(expected), `trial ${trial}`);
    if (expected) {
      assert.ok(result.results[0].offset >= 0 && result.results[0].offset < source.length);
      assert.ok(api.excerpt(source, result.terms, result.results[0].offset).isWellFormed());
    }
  }
});
