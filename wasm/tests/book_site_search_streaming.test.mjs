import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
function load(instrument = "") {
  const context = vm.createContext({module: {exports: {}}, setTimeout, clearTimeout, URL});
  vm.runInContext(instrument + script, context);
  return {api: context.module.exports, context};
}
const {api} = load();
const row = (text, title = "Guide", anchor = "section") =>
  ({text, title, page: "guide.html", anchor, heading: false, order: 0});
const normalize = text => text.toLowerCase().replace(/ς/gu, "σ").replace(/\s+/gu, " ");
const noWait = {yieldTask: async () => {}};

// Measuring the largest normalization operand catches the old failure even
// though it eventually noticed cancellation *after* scanning the entire row.
for (const field of ["text", "title"]) {
  test(`cancellation bounds normalization work inside one oversized ${field}`, async () => {
    const {api, context} = load(`
      globalThis.largest = 0;
      const lower = String.prototype.toLowerCase;
      String.prototype.toLowerCase = function () {
        globalThis.largest = Math.max(globalThis.largest, this.length);
        return lower.call(this);
      };
    `);
    const item = row("ordinary text");
    item[field] = "İ".repeat(500000) + "needle";
    let stopped = false, yields = 0;
    const result = await api.search([item], "needle", {
      cancelled: () => stopped,
      yieldTask: async () => { yields++; stopped = true; },
    });
    assert.equal(result, null);
    assert.equal(yields, 1);
    assert.ok(context.largest <= 4096, `normalized ${context.largest} characters without yielding`);
  });
}

test("a pre-cancelled search does not touch entry text", async () => {
  const item = row("unused");
  Object.defineProperty(item, "text", {get() { throw new Error("must not read a cancelled row"); }});
  assert.equal(await api.search([item], "needle", {cancelled: () => true}), null);
});

test("phrases span chunk boundaries and arbitrarily long collapsed whitespace", async () => {
  for (const padding of [4093, 4094, 4095, 4096, 8190, 8191, 8192]) {
    for (const gap of [" ", "\r\n\t", " \t\n".repeat(6000), "\u00a0\u2003"]) {
      const content = "x".repeat(padding) + "memory" + gap + "SAFETY";
      const result = await api.search([row(content)], '"memory safety"', noWait);
      assert.equal(result.total, 1, `boundary ${padding}, whitespace ${gap.length}`);
      assert.equal(result.results[0].offset, padding);
      assert.equal(result.results[0].score, 20);
    }
  }
});

test("fold expansion, astral scalars and sigma do not depend on chunk boundaries", async () => {
  for (const padding of [4093, 4094, 4095, 4096, 8191]) {
    const prefix = "x".repeat(padding);
    for (const [source, query] of [["İ🙂 中文", '"i̇🙂 中文"'], ["ΟΣ", "οσ"],
      ["οσ", "ος"], ["𐐀𐐁", "𐐨𐐩"]]) {
      const result = await api.search([row(prefix + source)], query, noWait);
      assert.equal(result.total, 1, `${source} / ${query} at ${padding}`);
      assert.equal(result.results[0].offset, padding);
    }
  }
  assert.deepEqual(Array.from(api.parseQuery("ΟΣ οσ ος")), ["οσ"]);
});

test("all words can be supplied by a combination of title and body", async () => {
  const result = await api.search([row("memory details", "Rust Guide")], "rust memory", noWait);
  assert.equal(result.total, 1);
  assert.equal(result.results[0].score, 0);
  assert.equal(result.results[0].offset, 0);
});

test("exact and prefix ranking does not truncate long or whitespace-heavy entries", async () => {
  for (const [content, score] of [["needle", 55], [" needle ", 50],
    ["\n".repeat(20000) + "needle" + "\t".repeat(20000), 50],
    ["needle" + "x".repeat(20000), 25], ["x".repeat(20000) + "needle", 20]]) {
    const result = await api.search([row(content)], "needle", noWait);
    assert.equal(result.results[0].score, score, `ranking for ${content.length} characters`);
  }
});

test("large chapter titles are matched once rather than rescanned for every body", async () => {
  const title = "İ".repeat(50000) + "needle";
  let yields = 0;
  const data = Array.from({length: 60}, (_, i) => ({...row("body", title, `s${i}`), order: i}));
  const result = await api.search(data, "needle", {yieldTask: async () => { yields++; }});
  assert.equal(result.total, 60);
  assert.ok(yields < 10, `rescanned the chapter title: ${yields} yields`);
});

test("streaming results and scores agree with a whole-string reference", async () => {
  let state = 0x54f00d;
  const next = () => { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0; };
  const alphabet = ["x", "A", " ", "\t", "\n", "İ", "🙂", "Σ", "ς", "σ", "𐐀", "中"];
  for (let sample = 0; sample < 90; sample++) {
    const content = Array.from({length: 4200 + next() % 6000}, () => alphabet[next() % alphabet.length]).join("");
    const query = ["a 中", '"i̇🙂"', '"σ σ"', "𐐨", "missing", "🙂 中"][sample % 6];
    const terms = Array.from(api.parseQuery(query));
    const body = normalize(content), title = normalize("Guide");
    const matches = terms.every(term => body.includes(term) || title.includes(term));
    const result = await api.search([row(content)], query, noWait);
    assert.equal(result.total, Number(matches), `reference sample ${sample}`);
    if (matches) {
      const inBody = terms.every(term => body.includes(term));
      const score = (inBody ? 20 : 0) + (terms.length === 1 && body.trim() === terms[0] ? 30 : 0)
        + (inBody && body.startsWith(terms[0]) ? 5 : 0);
      assert.equal(result.results[0].score, score, `score sample ${sample}`);
    }
  }
});
