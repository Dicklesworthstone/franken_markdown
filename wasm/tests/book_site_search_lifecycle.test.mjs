import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
class Clock {
  next = 0;
  jobs = new Map();
  setTimeout = fn => { const id = ++this.next; this.jobs.set(id, fn); return id; };
  clearTimeout = id => { this.jobs.delete(id); };
  tick() {
    const job = this.jobs.entries().next().value;
    if (job) { this.jobs.delete(job[0]); job[1](); }
  }
}
class Element {
  children = [];
  listeners = new Map();
  value = "";
  textContent = "";
  disabled = false;
  append(...items) { this.children.push(...items); }
  replaceChildren(...items) { this.children = items; }
  addEventListener(type, callback) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(callback);
  }
  removeEventListener(type, callback) { this.listeners.get(type)?.delete(callback); }
  dispatch(type, extra = {}) {
    const event = {preventDefault() {}, ...extra};
    for (const callback of this.listeners.get(type) || []) callback(event);
  }
}
function fixture(body, title = "Guide") {
  const clock = new Clock();
  const context = vm.createContext({module: {exports: {}}, setTimeout: clock.setTimeout,
    clearTimeout: clock.clearTimeout, URL});
  vm.runInContext(`
    globalThis.largest = 0;
    const lower = String.prototype.toLowerCase;
    String.prototype.toLowerCase = function () {
      globalThis.largest = Math.max(globalThis.largest, this.length);
      return lower.call(this);
    };
  ` + script, context);
  const elements = Object.fromEntries(["query", "search-form", "status", "results",
    "previous", "next", "search-data"].map(id => [id, new Element()]));
  elements["search-data"].textContent = JSON.stringify({schema: "fmd-book-search-index-v1", chapters: [{
    title, source: "guide.md", page: "guide.html", index: {schema: "fmd-search-index-v1", entries: [
      {kind: "paragraph", text: body, anchor: "details"},
    ]},
  }]});
  const document = {elements, location: {href: "file:///book/search.html"},
    getElementById: id => elements[id], createElement: () => new Element()};
  return {clock, context, api: context.module.exports, document, elements};
}
async function until(clock, condition) {
  for (let step = 0; step < 10000; step++) {
    await Promise.resolve();
    if (condition()) return;
    clock.tick();
  }
  assert.fail("controller did not settle within the deterministic task budget");
}
async function drain(clock) {
  for (let step = 0; step < 100; step++) { clock.tick(); await Promise.resolve(); }
  assert.equal(clock.jobs.size, 0);
}
const callbacks = elements => Object.values(elements).flatMap(element =>
  [...element.listeners.values()].flatMap(set => [...set]));

test("recorded source offsets make snippet extraction independent of document length", () => {
  const {api, context} = fixture("");
  const prefix = "İ🙂".repeat(100000), content = prefix + "target" + "x".repeat(500000);
  const snippet = api.excerpt(content, ["target"], prefix.length);
  assert.ok(snippet.includes("target"));
  assert.ok(snippet.isWellFormed());
  assert.ok(snippet.length <= 264);
  assert.equal(context.largest, 0, "snippet extraction must not normalize the document again");
});

test("standalone snippets use the same whitespace and Unicode matching as search", () => {
  const {api} = fixture("");
  const prefix = "İ🙂".repeat(2000);
  for (const [source, query] of [["memory\r\n\tSAFETY", '"memory safety"'], ["ΟΣ", "οσ"],
    ["İ🙂", '"i̇🙂"']]) {
    const snippet = api.excerpt(prefix + source + "後".repeat(300), api.parseQuery(query));
    assert.ok(snippet.includes(source), `${source} must be visible in the excerpt`);
    assert.ok(snippet.isWellFormed());
  }
});

test("production snippets center normalized phrase hits without reprocessing whole entries", async () => {
  const prefix = "İ🙂".repeat(6000);
  const f = fixture(prefix + "memory\n\tSAFETY" + "後".repeat(300));
  const controller = f.api.mount(f.document);
  f.elements.query.value = '"memory safety"';
  f.elements["search-form"].dispatch("submit");
  await until(f.clock, () => /^Results/u.test(f.elements.status.textContent));
  const detail = f.elements.results.children[0].children[1].textContent;
  assert.ok(detail.includes("memory\n\tSAFETY"));
  assert.ok(detail.isWellFormed());
  assert.ok(f.context.largest <= 4097, `renormalized ${f.context.largest} characters while rendering`);
  controller.dispose();
});

test("oversized titles are bounded in the visible result label", async () => {
  const f = fixture("needle", "x".repeat(40000) + "needle");
  const controller = f.api.mount(f.document);
  f.elements.query.value = "needle";
  f.elements["search-form"].dispatch("submit");
  await until(f.clock, () => /^Results/u.test(f.elements.status.textContent));
  for (const item of f.elements.results.children) {
    assert.ok(item.children[0].textContent.length <= 530);
    assert.ok(item.children[0].textContent.includes("needle"));
  }
  controller.dispose();
});

test("a replacement query fences an older search paused inside one large entry", async () => {
  const f = fixture("needle " + "x".repeat(100000));
  const controller = f.api.mount(f.document);
  f.elements.query.value = "missing";
  f.elements["search-form"].dispatch("submit");
  await until(f.clock, () => f.clock.jobs.size > 0);
  assert.equal(f.elements.status.textContent, "Searching…");
  f.elements.query.value = "needle";
  f.elements["search-form"].dispatch("submit");
  await until(f.clock, () => /^Results/u.test(f.elements.status.textContent));
  const status = f.elements.status.textContent;
  await drain(f.clock);
  assert.equal(f.elements.status.textContent, status);
  assert.equal(f.elements.results.children.length, 1);
  controller.dispose();
});

test("dispose removes listeners, cancels pending work and makes captured callbacks inert", async () => {
  const f = fixture("x".repeat(100000) + "needle");
  const controller = f.api.mount(f.document);
  const oldCallbacks = callbacks(f.elements);
  assert.equal(oldCallbacks.length, 5);
  f.elements.query.value = "needle";
  f.elements["search-form"].dispatch("submit");
  await until(f.clock, () => f.clock.jobs.size > 0);
  controller.dispose();
  assert.equal(callbacks(f.elements).length, 0);
  f.elements.status.textContent = "disposed";
  for (const callback of oldCallbacks) callback({preventDefault() {}, key: "Escape"});
  await drain(f.clock);
  assert.equal(f.elements.status.textContent, "disposed");
  assert.equal(f.elements.results.children.length, 0);
  assert.equal(f.elements.query.value, "needle");
  controller.dispose();
  assert.equal(f.elements.status.textContent, "disposed");
});

test("disposing an old controller twice cannot erase a remounted controller's results", async () => {
  const f = fixture("needle");
  const first = f.api.mount(f.document);
  first.dispose();
  const second = f.api.mount(f.document);
  assert.equal(callbacks(f.elements).length, 5);
  f.elements.query.value = "needle";
  f.elements["search-form"].dispatch("submit");
  await until(f.clock, () => /^Results/u.test(f.elements.status.textContent));
  first.dispose();
  assert.equal(f.elements.results.children.length, 1);
  assert.equal(callbacks(f.elements).length, 5);
  second.dispose();
  assert.equal(callbacks(f.elements).length, 0);
});
