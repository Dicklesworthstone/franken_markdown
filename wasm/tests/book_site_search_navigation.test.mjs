import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
const entry = (text, anchor) => ({ text, anchor, kind: "paragraph" });
const index = (entries = [entry("alpha", "a"), entry("beta", "b"), entry("中文", "c")]) => ({
  schema: "fmd-book-search-index-v1", chapters: [{ title: "Guide", source: "guide/a.md", page: "a.html",
    index: { schema: "fmd-search-index-v1", entries } }],
});
const many = (word, count) => Array.from({ length: count }, (_, i) => entry(word, `${word}-${i}`));
const settle = () => new Promise((resolve) => setImmediate(resolve));

class Target {
  listeners = new Map();
  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(fn);
  }
  removeEventListener(type, fn) { this.listeners.get(type)?.delete(fn); }
  dispatch(type, properties = {}) {
    const event = { prevented: false, preventDefault() { this.prevented = true; }, ...properties };
    for (const fn of [...(this.listeners.get(type) || [])]) fn(event);
    return event;
  }
}
class Element extends Target {
  children = [];
  value = "";
  textContent = "";
  hidden = false;
  constructor(tagName = "") { super(); this.tagName = tagName; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
}

function setup({ url = "https://docs.example.test/book/~fmd-search.html?keep=1#results", value = index(),
  denyHistory = false, noWindow = false, address } = {}) {
  const tasks = new Map();
  let timer = 0;
  const context = vm.createContext({ module: { exports: {} }, URL,
    setTimeout(fn, delay) { tasks.set(++timer, { fn, delay }); return timer; },
    clearTimeout(id) { tasks.delete(id); },
  });
  vm.runInContext(script, context);
  const elements = Object.fromEntries(["query", "search-form", "search-help", "status", "results", "previous", "next", "search-data"]
    .map((id) => [id, new Element()]));
  elements["search-data"].textContent = JSON.stringify(value);
  const location = { href: url }, view = new Target(), writes = [], stack = [url];
  let current = 0;
  view.history = {
    pushState(state, title, href) {
      if (denyHistory) throw new Error("History writes denied");
      assert.equal(state, null, "history must not retain the book or results");
      assert.equal(title, "");
      writes.push(href);
      stack.splice(current + 1);
      stack.push(href);
      current++;
      location.href = href;
    },
    back() { if (current > 0) { location.href = stack[--current]; view.dispatch("popstate"); } },
    forward() { if (current + 1 < stack.length) { location.href = stack[++current]; view.dispatch("popstate"); } },
  };
  const find = (node, id) => node.id === id ? node : node.children.map((child) => find(child, id)).find(Boolean);
  const document = {
    elements, location, defaultView: noWindow ? undefined : view,
    getElementById(id) { return elements[id] || Object.values(elements).map((node) => find(node, id)).find(Boolean); },
    createElement(tag) { return new Element(tag); },
  };
  let controller = context.module.exports.mount(document, address);
  const flush = async () => {
    await settle();
    for (let i = 0; tasks.size; i++) {
      assert.ok(i < 2000, "timer queue must converge");
      const [id, task] = tasks.entries().next().value;
      tasks.delete(id);
      task.fn();
      await settle();
    }
  };
  const submit = async (query) => {
    elements.query.value = query;
    assert.equal(elements["search-form"].dispatch("submit").prevented, true);
    await flush();
  };
  return { document, elements, view, location, writes, stack, tasks, flush, submit,
    link: () => document.getElementById("search-permalink"),
    dispose: () => controller.dispose(),
    remount() { controller = context.module.exports.mount(document); return controller; },
  };
}
const queryOf = (url) => new URL(url).searchParams.get("q");

test("submitted scoped Unicode queries create encoded same-page permalinks and history entries", async () => {
  const s = setup({ value: index([entry("cafe\u0301 memory safety", "found")]) });
  const query = 'café "memory safety" path:guide/ -obsolete';
  await s.submit(query);
  assert.equal(s.elements.results.children.length, 1);
  const url = new URL(s.location.href);
  assert.equal(url.origin, "https://docs.example.test");
  assert.equal(url.pathname, "/book/~fmd-search.html");
  assert.equal(url.searchParams.get("q"), query);
  assert.equal(url.searchParams.get("keep"), "1");
  assert.equal(url.searchParams.has("page"), false);
  assert.equal(url.hash, "#results");
  assert.equal(s.link().href, url.href);
  assert.equal(s.link().hidden, false);
  assert.equal(s.writes.length, 1);
  s.dispose();
});

test("opening a page bookmark restores the retained result page without adding history", async () => {
  const s = setup({ url: "file:///book/~fmd-search.html?q=needle&page=3", value: index(many("needle", 65)) });
  await s.flush();
  assert.equal(s.elements.results.start, 51);
  assert.equal(s.elements.results.children.length, 15);
  assert.equal(s.elements.previous.disabled, false);
  assert.equal(s.elements.next.disabled, true);
  assert.equal(s.link().href, s.location.href);
  assert.equal(s.writes.length, 0);
  s.dispose();
});

test("bookmark page admission clamps valid large pages and rejects malformed numbers", async () => {
  for (const [number, start] of [["999", 51], ["0", 1], ["-1", 1], ["01", 1], ["1.5", 1],
    ["2x", 1], ["NaN", 1], ["9999999999999999999999", 1]]) {
    const s = setup({ url: `file:///book/~fmd-search.html?q=needle&page=${number}`, value: index(many("needle", 60)) });
    await s.flush();
    assert.equal(s.elements.results.start, start, number);
    assert.equal(new URL(s.link().href).searchParams.get("page"), start === 1 ? null : "3");
    s.dispose();
  }
  const s = setup({ url: "file:///book/~fmd-search.html?q=needle&page=999", value: index(many("needle", 400)) });
  await s.flush();
  assert.equal(s.elements.results.start, 176);
  assert.equal(s.elements.results.children.length, 25);
  assert.equal(s.elements.next.disabled, true, "unretained results do not create phantom pages");
  s.dispose();
});

test("Back and Forward restore queries and result pages without replaying history writes", async () => {
  const s = setup({ value: index([...many("alpha", 60), ...many("beta", 30)]) });
  await s.submit("alpha");
  s.elements.next.dispatch("click");
  assert.equal(s.elements.results.start, 26);
  await s.submit("beta");
  assert.equal(s.writes.length, 3);
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(s.elements.results.start, 26);
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.results.start, 1);
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.query.value, "");
  assert.equal(s.elements.results.children.length, 0);
  assert.equal(s.link().hidden, true);
  s.view.history.forward(); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(s.elements.results.start, 1);
  assert.equal(s.writes.length, 3);
  s.dispose();
});

test("repeated submissions do not duplicate identical history entries", async () => {
  const s = setup();
  await s.submit("alpha");
  await s.submit("alpha");
  assert.equal(s.writes.length, 1);
  assert.equal(s.stack.length, 2);
  s.dispose();
});

test("live drafts have current permalinks without overwriting the last committed search", async () => {
  const s = setup();
  await s.submit("alpha");
  const committed = s.location.href;
  s.elements.query.value = "beta";
  s.elements.query.dispatch("input");
  assert.equal(s.link().hidden, true);
  await s.flush();
  assert.equal(s.elements.results.children[0].children[0].href, "./a.html#b");
  assert.equal(queryOf(s.link().href), "beta");
  assert.equal(s.location.href, committed);
  assert.equal(s.writes.length, 1);
  await s.submit("beta");
  assert.equal(s.writes.length, 2);
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  s.dispose();
});

test("paging a live draft records the displayed query and page, not stale input history", async () => {
  const s = setup({ value: index([...many("alpha", 30), ...many("beta", 40)]) });
  await s.submit("alpha");
  s.elements.query.value = "beta";
  s.elements.query.dispatch("input"); await s.flush();
  s.elements.next.dispatch("click");
  assert.equal(queryOf(s.location.href), "beta");
  assert.equal(new URL(s.location.href).searchParams.get("page"), "2");
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(s.elements.results.start, 1);
  s.dispose();
});

test("Escape records a clear state while preserving unrelated URL parameters and fragment", async () => {
  const s = setup();
  await s.submit("alpha");
  assert.equal(s.elements.query.dispatch("keydown", { key: "Escape" }).prevented, true);
  assert.equal(queryOf(s.location.href), null);
  assert.equal(new URL(s.location.href).searchParams.get("keep"), "1");
  assert.equal(new URL(s.location.href).hash, "#results");
  assert.equal(s.link().hidden, true);
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(s.elements.results.children.length, 1);
  await s.submit("   ");
  assert.equal(queryOf(s.location.href), null);
  s.dispose();
});

test("invalid queries clear results and permalinks without recording an invalid location", async () => {
  const s = setup();
  await s.submit("alpha");
  const before = s.location.href;
  await s.submit('-obsolete');
  assert.match(s.elements.status.textContent, /before exclusions/);
  assert.equal(s.elements.results.children.length, 0);
  assert.equal(s.link().hidden, true);
  assert.equal(s.location.href, before);
  assert.equal(s.writes.length, 1);
  s.dispose();
  const huge = setup({ url: "file:///book/~fmd-search.html?q=" + "x".repeat(1000) });
  await huge.flush();
  assert.equal(huge.elements.query.value.length, 513);
  assert.match(huge.elements.status.textContent, /512/);
  assert.equal(huge.link().hidden, true);
  huge.dispose();
});

test("denied History API writes do not disable search, pagination or explicit result links", async () => {
  const s = setup({ denyHistory: true, url: "file:///book/~fmd-search.html", value: index(many("needle", 50)) });
  await s.submit("needle");
  assert.equal(s.elements.results.children.length, 25);
  assert.equal(queryOf(s.location.href), null);
  assert.equal(queryOf(s.link().href), "needle");
  s.elements.next.dispatch("click");
  assert.equal(s.elements.results.start, 26);
  assert.equal(new URL(s.link().href).searchParams.get("page"), "2");
  s.dispose();
});

test("headless document hosts and opaque origins remain usable without history", async () => {
  const s = setup({ noWindow: true, url: "file:///book/~fmd-search.html" });
  await s.submit("alpha");
  assert.equal(s.elements.results.children.length, 1);
  assert.equal(queryOf(s.link().href), "alpha");
  assert.equal(s.writes.length, 0);
  s.dispose();
  const opaque = setup({ url: "about:blank" });
  await opaque.submit("alpha");
  assert.equal(opaque.elements.results.children.length, 1);
  assert.equal(opaque.link().hidden, true);
  assert.equal(opaque.writes.length, 0);
  opaque.dispose();
});

test("prefill addresses cannot redirect generated result links or history to another host", async () => {
  const s = setup({ address: "https://other.example.test/wrong?q=alpha&extra=secret" });
  await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(new URL(s.link().href).host, "docs.example.test");
  assert.equal(new URL(s.link().href).pathname, "/book/~fmd-search.html");
  assert.equal(new URL(s.link().href).searchParams.has("extra"), false);
  await s.submit("alpha");
  assert.equal(new URL(s.location.href).host, "docs.example.test");
  s.dispose();
});

test("a superseded long search cannot overwrite newer results, permalink or history", async () => {
  const s = setup({ value: index([entry("alpha " + "x".repeat(2 * 1024 * 1024), "huge"), entry("beta", "latest")]) });
  s.elements.query.value = "alpha -obsolete";
  s.elements["search-form"].dispatch("submit");
  await settle();
  assert.ok(s.tasks.size, "the old scan has reached a real cooperative yield");
  await s.submit("beta");
  assert.equal(s.elements.results.children.length, 1);
  assert.equal(s.elements.results.children[0].children[0].href, "./a.html#latest");
  assert.equal(queryOf(s.location.href), "beta");
  assert.equal(queryOf(s.link().href), "beta");
  assert.equal(s.writes.length, 1);
  s.dispose();
});

test("superseded query errors and silent value changes cannot commit stale UI state", async () => {
  const s = setup();
  s.elements.query.value = '-obsolete'; s.elements["search-form"].dispatch("submit");
  await s.submit("beta");
  assert.equal(queryOf(s.location.href), "beta");
  assert.doesNotMatch(s.elements.status.textContent, /exclusions/);
  s.dispose();
  const silent = setup({ value: index([entry("alpha " + "x".repeat(200_000), "huge")]) });
  silent.elements.query.value = "alpha -obsolete";
  silent.elements["search-form"].dispatch("submit");
  await settle();
  silent.elements.query.value = "changed without an input event";
  await silent.flush();
  assert.equal(silent.elements.results.children.length, 0);
  assert.equal(silent.link().hidden, true);
  assert.equal(silent.writes.length, 0);
  silent.dispose();
});

test("Back cancels a pending debounce and ignores untrusted event.state", async () => {
  const s = setup();
  await s.submit("alpha"); await s.submit("beta");
  s.elements.query.value = "draft"; s.elements.query.dispatch("input");
  assert.equal(s.tasks.size, 1);
  s.view.history.back();
  s.view.dispatch("popstate", { state: { query: "injected", page: 9999 } });
  await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(queryOf(s.link().href), "alpha");
  assert.equal(s.writes.length, 2);
  s.dispose();
});

test("Back fences an in-flight scan before it can record its abandoned query", async () => {
  const s = setup({ value: index([entry("alpha", "a"), entry("beta " + "x".repeat(200_000), "huge")]) });
  await s.submit("alpha");
  s.elements.query.value = "beta -obsolete"; s.elements["search-form"].dispatch("submit");
  await settle();
  assert.ok(s.tasks.size);
  s.view.history.back(); await s.flush();
  assert.equal(s.elements.query.value, "");
  assert.equal(s.elements.results.children.length, 0);
  assert.equal(s.link().hidden, true);
  assert.equal(s.writes.length, 1);
  s.dispose();
});

test("composition suppresses provisional queries, Enter and Escape until final input", async () => {
  const s = setup();
  await s.submit("alpha");
  s.elements.query.dispatch("compositionstart");
  s.elements.query.value = "中";
  s.elements.query.dispatch("input", { isComposing: true });
  s.elements["search-form"].dispatch("submit", { isComposing: true });
  assert.equal(s.elements.query.dispatch("keydown", { key: "Escape", isComposing: true }).prevented, false);
  await s.flush();
  assert.equal(s.elements.query.value, "中");
  assert.equal(s.elements.results.children.length, 0);
  assert.equal(s.tasks.size, 0);
  assert.equal(s.writes.length, 1);
  s.elements.query.value = "中文";
  s.elements.query.dispatch("compositionend");
  s.elements.query.dispatch("input", { isComposing: false });
  assert.equal(s.tasks.size, 1, "trailing input coalesces with compositionend");
  await s.flush();
  assert.equal(s.elements.results.children[0].children[0].href, "./a.html#c");
  assert.equal(queryOf(s.location.href), "alpha", "the composed draft does not overwrite committed history");
  await s.submit("中文");
  assert.equal(queryOf(s.location.href), "中文");
  s.dispose();
});

test("event-only composition flags and legacy IME Escape do not clear provisional input", async () => {
  const s = setup();
  s.elements.query.value = "中文";
  s.elements.query.dispatch("input", { isComposing: true });
  await s.flush();
  assert.equal(s.elements.results.children.length, 0);
  assert.equal(s.elements.query.dispatch("keydown", { key: "Escape", keyCode: 229 }).prevented, false);
  assert.equal(s.elements.query.value, "中文");
  await s.submit("中文");
  assert.equal(s.elements.results.children.length, 1);
  s.dispose();
});

test("silent input changes prevent pager clicks from recording an unrelated query", async () => {
  const s = setup({ value: index(many("alpha", 50)) });
  await s.submit("alpha");
  s.elements.query.value = "beta";
  s.elements.next.dispatch("click");
  assert.equal(s.elements.results.start, 1);
  assert.equal(s.writes.length, 1);
  assert.equal(queryOf(s.location.href), "alpha");
  s.dispose();
});

test("disposal releases window listeners and timers; remount reuses one permalink", async () => {
  const s = setup();
  await s.submit("alpha");
  const link = s.link(), captured = [...s.view.listeners.get("popstate")][0];
  s.elements.query.value = "draft"; s.elements.query.dispatch("input");
  s.dispose(); s.dispose();
  assert.equal(s.tasks.size, 0);
  assert.equal(s.view.listeners.get("popstate").size, 0);
  assert.equal(link.hidden, true);
  captured({}); await s.flush();
  assert.equal(s.elements.query.value, "draft");
  s.remount(); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  assert.equal(s.link(), link);
  assert.equal(s.elements["search-form"].children.filter((node) => node.id === "search-permalink").length, 1);
  assert.equal(s.view.listeners.get("popstate").size, 1);
  s.dispose();
});

test("an unrelated same-origin path change is not adopted as the book search document", async () => {
  const s = setup();
  await s.submit("alpha");
  s.location.href = "https://docs.example.test/elsewhere?q=other";
  s.view.dispatch("popstate"); await s.flush();
  assert.equal(s.elements.query.value, "alpha");
  await s.submit("beta");
  assert.equal(s.elements.results.children.length, 1);
  assert.equal(s.link().hidden, true);
  assert.equal(s.location.href, "https://docs.example.test/elsewhere?q=other");
  assert.equal(s.writes.length, 1);
  s.dispose();
});

test("query punctuation is URL-encoded and never becomes link markup", async () => {
  const s = setup({ value: index([entry('needle <img onerror=bad> & #', 'safe')]) });
  const query = 'needle "<img onerror=bad>" & #';
  await s.submit(query);
  assert.equal(s.elements.results.children.length, 1);
  assert.equal(queryOf(s.link().href), query);
  assert.doesNotMatch(s.link().href, /<img|"/);
  assert.equal(s.link().textContent, "Link to these results");
  assert.equal(s.link().children.length, 0);
  assert.equal(new URL(s.link().href).hash, "#results");
  s.dispose();
});
