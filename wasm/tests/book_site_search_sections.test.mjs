import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
const context = { module: { exports: {} }, setTimeout, clearTimeout, URL };
vm.runInNewContext(script, context);
const { prepare, search, excerpt, mount, LIMITS } = context.module.exports;
const noWait = { yieldTask: async () => {} };
const entry = (text, anchor = "topic", kind = "paragraph") => ({ text, anchor, kind });
const chapter = (entries, page = "guide.html", title = "Guide") => ({
  page, title, source: page.replace(".html", ".md"),
  index: { schema: "fmd-search-index-v1", entries },
});
const index = (...chapters) => ({ schema: "fmd-book-search-index-v1", chapters });
const rows = (...chapters) => prepare(index(...chapters));
const snippets = result => result.passages.map(hit => excerpt(
  hit.inTitle ? hit.row.title : hit.row.text, [], hit.offset));

test("all terms can occur in separate paragraphs under the same heading", async () => {
  const result = await search(rows(chapter([
    entry("Cache management", "cache", "heading"),
    entry("An expiration policy governs stored values.", "cache"),
    entry("Invalidate entries explicitly after writes.", "cache"),
  ])), "cache expiration invalidate", noWait);
  assert.equal(result.total, 1);
  assert.equal(result.results[0].row.anchor, "cache");
  const text = snippets(result.results[0]).join(" ").toLowerCase();
  for (const term of ["cache", "expiration", "invalidate"]) assert.ok(text.includes(term));
});

test("section evidence combines title, prose, code, table and referenced-note entries", async () => {
  const result = await search(rows(chapter([
    entry("Configuration", "fn-n", "heading"),
    entry("retry_budget", "fn-n", "code"),
    entry("timeout 30", "fn-n", "table"),
    entry("Recover from a failed request.", "fn-n"),
  ], "notes.html", "Networking")), 'networking retry_budget timeout "failed request"', noWait);
  assert.equal(result.total, 1);
  assert.equal(result.results[0].row.anchor, "fn-n");
  assert.ok(snippets(result.results[0]).join(" ").includes("Networking"));
});

test("term coverage never crosses a section or chapter boundary", async () => {
  const data = rows(chapter([entry("alpha", "first"), entry("beta", "second")]),
    chapter([entry("beta", "first")], "other.html"));
  assert.equal((await search(data, "alpha beta", noWait)).total, 0);
});

test("quoted phrases stay inside a real entry instead of fabricated concatenations", async () => {
  const data = rows(chapter([entry("memory"), entry("safety")]),
    chapter([entry("memory\n\tsafety"), entry("guarantee")], "real.html"));
  const result = await search(data, '"memory safety" guarantee', noWait);
  assert.equal(result.total, 1);
  assert.equal(result.results[0].row.page, "real.html");
});

test("single-passage and exact heading matches outrank distributed section matches", async () => {
  const data = rows(chapter([
    entry("alpha", "split"), entry("beta", "split"),
    entry("alpha beta", "complete"),
    entry("alpha beta", "heading", "heading"),
  ]));
  const result = await search(data, "alpha beta", noWait);
  assert.equal(result.total, 3);
  assert.deepEqual(Array.from(result.results, hit => hit.row.anchor), ["heading", "complete", "split"]);
  assert.equal(result.results[0].score, 35);
  assert.equal(result.results[1].score, 25);
  assert.ok(result.results[2].score < result.results[1].score);
});

test("source-linked evidence finds distant hits after expanding case folds and astral text", async () => {
  const data = rows(chapter([
    entry("İ🙂".repeat(25000) + "alpha"),
    entry("後".repeat(80000) + "beta"),
  ]));
  let yields = 0;
  const result = await search(data, "alpha beta", { yieldTask: async () => { yields++; } });
  assert.equal(result.total, 1);
  assert.ok(yields > 0);
  const passages = result.results[0].passages;
  assert.equal(passages.find(hit => hit.row.text.endsWith("alpha")).offset, 75000);
  assert.equal(passages.find(hit => hit.row.text.endsWith("beta")).offset, 80000);
  for (const snippet of snippets(result.results[0])) assert.ok(snippet.isWellFormed());
  assert.ok(snippets(result.results[0]).some(text => text.includes("alpha")));
  assert.ok(snippets(result.results[0]).some(text => text.includes("beta")));
});

test("witness storage is bounded by query terms, not matching paragraph count", async () => {
  const terms = ["alfa", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel"];
  const entries = Array.from({ length: 10000 }, (_, i) => entry(terms[i % terms.length]));
  const result = await search(rows(chapter(entries)), terms.join(" "), noWait);
  assert.equal(result.total, 1);
  assert.ok(result.results[0].passages.length <= LIMITS.terms);
  assert.ok(result.results[0].passages.length > 0);
  const evidence = snippets(result.results[0]).join(" ");
  for (const term of terms) assert.ok(evidence.includes(term));
});

test("best-result cap still counts every matching section and admits later stronger hits", async () => {
  const entries = [];
  for (let i = 0; i < 350; i++) entries.push(entry("alpha", `s${i}`), entry("beta", `s${i}`));
  entries.push(entry("alpha beta", "best", "heading"));
  const result = await search(rows(chapter(entries)), "alpha beta", noWait);
  assert.equal(result.total, 351);
  assert.equal(result.results.length, LIMITS.results);
  assert.equal(result.results[0].row.anchor, "best");
  assert.equal(result.results[1].row.anchor, "s0");
});

test("cancellation during a partly matched section returns no stale result", async () => {
  let stopped = false;
  const result = await search(rows(chapter([
    entry("alpha"), entry("x".repeat(300000) + "beta"),
  ])), "alpha beta", { cancelled: () => stopped, yieldTask: async () => { stopped = true; } });
  assert.equal(result, null);
});

test("section membership agrees with an independent exhaustive union oracle", async () => {
  let state = 0x9ab8cd;
  const random = () => { state = Math.imul(state, 1664525) + 1013904223 | 0; return state >>> 0; };
  const vocabulary = ["alfa", "bravo", "charlie", "delta", "echo"];
  for (let sample = 0; sample < 120; sample++) {
    const entries = [];
    const expected = [];
    const selected = vocabulary.slice(0, 2 + random() % 4);
    for (let section = 0; section < 6; section++) {
      const texts = [];
      for (let row = 0; row < 1 + random() % 5; row++) {
        const text = vocabulary.filter(() => random() % 3 === 0).join(" ") || "other";
        texts.push(text);
        entries.push(entry(text, `s${section}`));
      }
      if (selected.every(term => texts.some(text => text.includes(term)))) expected.push(`s${section}`);
    }
    const result = await search(rows(chapter(entries)), selected.join(" "), noWait);
    assert.deepEqual(Array.from(result.results, hit => hit.row.anchor).sort(), expected.sort(), `sample ${sample}`);
    assert.equal(result.total, expected.length);
    for (const hit of result.results) {
      const evidence = snippets(hit).join(" ");
      for (const term of selected) assert.ok(evidence.includes(term), `missing ${term} in sample ${sample}`);
    }
  }
});

class Element {
  constructor() { this.children = []; this.listeners = new Map(); this.value = ""; this.textContent = ""; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  addEventListener(type, handler) { this.listeners.set(type, handler); }
  removeEventListener(type) { this.listeners.delete(type); }
  dispatch(type) { this.listeners.get(type)?.({ preventDefault() {} }); }
}
test("the real controller shows inert evidence from each matching paragraph", async () => {
  const elements = Object.fromEntries(["query", "search-form", "status", "results", "previous", "next", "search-data"].map(id => [id, new Element()]));
  elements["search-data"].textContent = JSON.stringify(index(chapter([
    entry("<img src=x onerror=bad> alpha", 'a"<>'), entry("beta </script>", 'a"<>'),
  ])));
  const document = { getElementById: id => elements[id], createElement: () => new Element(), location: { href: "file:///book/search.html" } };
  const controller = mount(document);
  elements.query.value = "alpha beta";
  elements["search-form"].dispatch("submit");
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(elements.results.children.length, 1);
  const [link, detail] = elements.results.children[0].children;
  assert.equal(link.href, "./guide.html#a%22%3C%3E");
  assert.ok(detail.textContent.includes("alpha"));
  assert.ok(detail.textContent.includes("beta"));
  assert.ok(detail.textContent.includes("</script>"));
  assert.equal(detail.children.length, 0);
  controller.dispose();
});
