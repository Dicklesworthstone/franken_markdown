import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
const context = {module: {exports: {}}, setTimeout, clearTimeout, URL};
vm.runInNewContext(script, context);
const {prepare, parseQuery, search, excerpt, mount, LIMITS} = context.module.exports;
const plain = value => JSON.parse(JSON.stringify(value));
const entry = (text, anchor = "", kind = "paragraph") => ({kind, text, anchor});
const chapter = (title, entries, page = "guide__start.html") =>
  ({title, source: page.replace(".html", ".md"), page, index: {schema: "fmd-search-index-v1", entries}});
const index = chapters => ({schema: "fmd-book-search-index-v1", chapters});
const rows = chapters => prepare(index(chapters));

test("quoted phrases, Unicode, literal regex characters and term limits", () => {
  assert.deepEqual(plain(parseQuery(' RUST "memory safety" rust 中 .* ')), ["rust", "memory safety", "中", ".*"]);
  assert.throws(() => parseQuery('"unclosed'), /quotation/);
  assert.throws(() => parseQuery('"phrase"suffix'), /spaces/);
  assert.throws(() => parseQuery("a b c d e f g h i"), /eight/);
  assert.throws(() => parseQuery("x".repeat(513)), /512/);
});

test("reader search spans chapters, ranks headings, ANDs terms and groups section hits", async () => {
  const data = rows([
    chapter("Guide", [entry("Rust memory safety", "intro", "heading"),
      entry("More rust memory safety details", "intro"), entry("Rust alone", "other")]),
    chapter("Appendix", [entry("memory safety in rust", "appendix")], "end.html"),
  ]);
  const result = await search(data, 'rust "memory safety"');
  assert.equal(result.total, 2);
  assert.deepEqual(plain(result.results.map(item => [item.row.page, item.row.anchor])),
    [["guide__start.html", "intro"], ["end.html", "appendix"]]);
  assert.equal(result.results[0].row.heading, true);
});

test("heading-less and empty chapters remain discoverable by title", async () => {
  const result = await search(rows([chapter("Empty Reference", [])]), "reference");
  assert.equal(result.total, 1);
  assert.equal(result.results[0].row.anchor, "");
});

test("Unicode matching, duplicate anchors in distinct chapters, and literal markup", async () => {
  const data = rows([
    chapter("<img onerror=bad>", [entry("ΔΟΚΙΜΉ 中文 .* </script>", "same")]),
    chapter("Two", [entry("δοκιμή 中文 .*", "same")], "second.html"),
  ]);
  const result = await search(data, "δοκιμή 中文 .*");
  assert.equal(result.total, 2);
  assert.equal((await search(data, "$&")).total, 0);
});

test("unknown schemas, traversal, remote destinations and duplicate pages are rejected", () => {
  assert.throws(() => prepare({}), /Unsupported/);
  for (const page of ["../bad.html", "https://host/a.html", "javascript:x.html", "a.html?x", "a%2fb.html"]) {
    assert.throws(() => rows([chapter("Bad", [], page)]), /destination/);
  }
  assert.throws(() => rows([chapter("A", [], "A.html"), chapter("B", [], "a.html")]), /Duplicate/);
  assert.throws(() => rows([chapter("Bad", [{kind: "unknown", text: "bad", anchor: ""}])]), /kind/);
});

test("bounded best-results list reports the full match count, with stable ties", async () => {
  const entries = Array.from({length: 350}, (_, i) => entry("needle details", `s${i}`));
  entries.push(entry("needle", "best", "heading"));
  const result = await search(rows([chapter("Guide", entries)]), "needle", {yieldTask: async () => {}});
  assert.equal(result.total, 351);
  assert.equal(result.results.length, LIMITS.results);
  assert.equal(result.results[0].row.anchor, "best");
  assert.equal(result.results[1].row.anchor, "s0");
});

test("long searches yield and cancellation never produces a stale answer", async () => {
  let cancelled = false, yields = 0;
  const data = rows([chapter("Guide", Array.from({length: 500}, (_, i) => entry("needle", `s${i}`)))]);
  const result = await search(data, "needle", {
    cancelled: () => cancelled, yieldTask: async () => { yields++; cancelled = true; },
  });
  assert.equal(result, null);
  assert.equal(yields, 1);
});

test("snippets center hits without cutting surrogate pairs or expanded lowercase", () => {
  const content = "İ".repeat(80) + "🙂".repeat(80) + "target" + "後".repeat(200);
  const snippet = excerpt(content, ["target"]);
  assert.ok(snippet.includes("target"));
  assert.ok(snippet.startsWith("…"));
  assert.ok(snippet.endsWith("…"));
  assert.ok(snippet.isWellFormed());
});

class Element {
  constructor(tag = "") {
    this.tagName = tag; this.children = []; this.listeners = new Map();
    this.value = ""; this.textContent = ""; this.disabled = false;
  }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = [...children]; }
  addEventListener(name, fn) { this.listeners.set(name, fn); }
  dispatch(name, properties = {}) {
    const event = {preventDefault() { this.prevented = true; }, ...properties};
    this.listeners.get(name)?.(event);
    return event;
  }
}
function dom(data) {
  const elements = Object.fromEntries(["query", "search-form", "status", "results",
    "previous", "next", "search-data"].map(id => [id, new Element()]));
  elements["search-data"].textContent = JSON.stringify(data);
  return {elements, location: {href: "file:///book/~fmd-search.html"},
    getElementById: id => elements[id], createElement: tag => new Element(tag)};
}
const settle = () => new Promise(resolve => setTimeout(resolve, 20));

test("production controller renders safe local links and inert text; no HTML parsing", async () => {
  const title = '<img src=x onerror="bad">';
  const document = dom(index([chapter(title, [entry("</script> needle <svg/onload=bad>", 'a"<>')])]));
  const controller = mount(document);
  document.elements.query.value = "needle";
  assert.equal(document.elements["search-form"].dispatch("submit").prevented, true);
  await settle();
  const item = document.elements.results.children[0];
  assert.equal(item.children[0].href, "./guide__start.html#a%22%3C%3E");
  assert.ok(item.children[0].textContent.includes(title));
  assert.ok(item.children[1].textContent.includes("<svg/onload=bad>"));
  assert.match(document.elements.status.textContent, /1.*1/);
  controller.dispose();
});

test("pagination reaches every retained result and Escape cancels and clears", async () => {
  const entries = Array.from({length: 60}, (_, i) => entry("needle", `s${i}`));
  const document = dom(index([chapter("Guide", entries)]));
  const controller = mount(document);
  document.elements.query.value = "needle";
  document.elements["search-form"].dispatch("submit");
  await settle();
  assert.equal(document.elements.results.children.length, 25);
  assert.equal(document.elements.previous.disabled, true);
  document.elements.next.dispatch("click");
  assert.equal(document.elements.results.start, 26);
  document.elements.next.dispatch("click");
  assert.equal(document.elements.results.children.length, 10);
  assert.equal(document.elements.next.disabled, true);
  document.elements.previous.dispatch("click");
  assert.equal(document.elements.results.start, 26);
  assert.equal(document.elements.query.dispatch("keydown", {key: "Escape"}).prevented, true);
  assert.equal(document.elements.results.children.length, 0);
  assert.equal(document.elements.query.value, "");
  controller.dispose();
});

test("errors replace prior results; malformed indexes disable only search", async () => {
  const document = dom(index([chapter("Guide", [entry("needle")])]));
  const controller = mount(document);
  document.elements.query.value = "needle";
  document.elements["search-form"].dispatch("submit");
  await settle();
  document.elements.query.value = '"bad';
  document.elements["search-form"].dispatch("submit");
  await settle();
  assert.equal(document.elements.results.children.length, 0);
  assert.match(document.elements.status.textContent, /quotation/);
  controller.dispose();
  const broken = dom({schema: "future"});
  assert.equal(mount(broken), null);
  assert.equal(broken.elements.query.disabled, true);
});

test("file URLs can prefill searches without fetch, storage or a server", async () => {
  const document = dom(index([chapter("Guide", [entry("needle", "found")])]));
  const controller = mount(document, "file:///book/~fmd-search.html?q=needle");
  await settle();
  assert.equal(document.elements.query.value, "needle");
  assert.equal(document.elements.results.children.length, 1);
  assert.doesNotMatch(script, /\b(?:fetch|XMLHttpRequest|localStorage|innerHTML)\b/);
  controller.dispose();
});
