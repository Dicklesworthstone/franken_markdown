import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const script = readFileSync(new URL("../../src/book/site_search.js", import.meta.url), "utf8");
function load(extra = {}) {
  const context = vm.createContext({ module: { exports: {} }, setTimeout, clearTimeout, URL, ...extra });
  vm.runInContext(script, context);
  return { api: context.module.exports, context };
}
const { api } = load();
const plain = (value) => JSON.parse(JSON.stringify(value));
const entry = (text, anchor = "section", kind = "paragraph") => ({ kind, text, anchor });
const chapter = (source, title, entries, page = source.replaceAll("/", "__").replace(/\.md$/, ".html")) => ({
  source, title, page, index: { schema: "fmd-search-index-v1", entries },
});
const book = (...chapters) => ({ schema: "fmd-book-search-index-v1", chapters });
const fast = { yieldTask: async () => {} };
const find = (value, query, options = fast) => api.search(api.prepare(value), query, options);
const keys = (answer) => Array.from(answer.results, ({ row }) => `${row.page}#${row.anchor}`).sort();

test("one bounded query plan separates content, exclusions and scoped clauses", () => {
  assert.deepEqual(plain(api.parseSearchQuery(' Rust "memory safety" -obsolete -"old API" PATH:guide/ title:"User Guide" rust ')), {
    terms: ["rust", "memory safety"], excluded: ["obsolete", "old api"],
    filters: [
      { field: "path", value: "guide/", exclude: false },
      { field: "title", value: "user guide", exclude: false },
    ],
  });
  assert.deepEqual(plain(api.parseSearchQuery('"-obsolete" "path:guide/" protocol:tcp -')),
    { terms: ["-obsolete", "path:guide/", "protocol:tcp", "-"], excluded: [], filters: [] });
  assert.deepEqual(plain(api.parseSearchQuery('needle -PATH:old/ -TITLE:"old guide"')).filters, [
    { field: "path", value: "old/", exclude: true },
    { field: "title", value: "old guide", exclude: true },
  ]);
});

test("legacy parseQuery preserves literal operators and its term-only result", () => {
  assert.deepEqual(plain(api.parseQuery(' -obsolete path:guide/ "title:User Guide" -obsolete ')),
    ["-obsolete", "path:guide/", "title:user guide"]);
  assert.deepEqual(plain(api.parseQuery(' ""  ')), []);
});

test("empty clauses, exclusion-only searches and combined budgets are explicit errors", async () => {
  for (const query of ['path:', 'title:  word', 'needle -""', 'path:" "'])
    assert.throws(() => api.parseSearchQuery(query), /nonempty/);
  for (const query of ['-obsolete', '-path:old/', '-title:reference'])
    assert.throws(() => api.parseSearchQuery(query), /before exclusions/);
  for (const query of ['title:"unclosed', 'needle -"open', 'path:"name"suffix'])
    assert.throws(() => api.parseSearchQuery(query), /quotation|spaces/);
  assert.throws(() => api.parseSearchQuery('a b c d -e -f path:g title:h i'), /eight/);
  assert.throws(() => api.parseSearchQuery('x'.repeat(513)), /512/);
  for (const query of [null, 7, {}, []]) assert.throws(() => api.parseSearchQuery(query), /512/);
  assert.equal(api.parseSearchQuery('a a a a a a a a a').terms.length, 1);
  assert.equal((await find(book(chapter('a.md', 'A', [entry('a')])), '""')).total, 0);
});

test("path filters use source identity, not flattened output names or regular expressions", async () => {
  const value = book(
    chapter('guide/start.md', 'Same', [entry('needle')], 'flattened.html'),
    chapter('guide__start.md', 'Same', [entry('needle')], 'other.html'),
    chapter('Café/中文.md', 'Same', [entry('needle')], 'unicode.html'),
    chapter('literal/[x].md', 'Same', [entry('needle')], 'literal.html'),
  );
  assert.deepEqual(keys(await find(value, 'needle path:GUIDE/')), ['flattened.html#section']);
  assert.equal((await find(value, 'needle path:flattened')).total, 0);
  assert.equal((await find(value, 'needle path:.*')).total, 0);
  assert.deepEqual(keys(await find(value, 'needle path:[x]')), ['literal.html#section']);
  assert.deepEqual(keys(await find(value, 'needle path:Cafe\u0301/')), ['unicode.html#section']);
});

test("title and path filters are conjunctive metadata predicates, not body terms", async () => {
  const value = book(
    chapter('guide/a.md', 'User Guide', [entry('needle')]),
    chapter('guide/b.md', 'Other', [entry('needle User Guide')]),
    chapter('old/a.md', 'User Guide', [entry('needle')]),
  );
  assert.deepEqual(keys(await find(value, 'needle title:"User Guide" path:guide/')),
    ['guide__a.html#section']);
  assert.deepEqual(keys(await find(value, 'needle -title:Other -path:old/')),
    ['guide__a.html#section']);
  assert.equal((await find(value, 'needle path:guide/ path:old/')).total, 0);
});

test("positive filter-only queries browse matching sections, including empty chapters", async () => {
  const value = book(
    chapter('guide/a.md', 'Empty', []),
    chapter('guide/b.md', 'Reference', [entry('one', 'first'), entry('two', 'second')]),
    chapter('other.md', 'Outside', [entry('three')]),
  );
  const answer = await find(value, 'path:guide/');
  assert.deepEqual(keys(answer), ['guide__a.html#', 'guide__b.html#', 'guide__b.html#first', 'guide__b.html#second']);
  assert.deepEqual(plain(answer.terms), []);
  assert.ok(answer.results.every((hit) => hit.passages.length === 1));
  assert.deepEqual(keys(await find(value, 'title:Empty -obsolete')), ['guide__a.html#']);
});

test("a later excluded paragraph invalidates an already-matching heading or passage", async () => {
  const value = book(chapter('a.md', 'Guide', [
    entry('needle', 'bad', 'heading'), entry('details', 'bad'), entry('obsolete', 'bad'),
    entry('needle', 'good', 'heading'), entry('current', 'good'),
  ]));
  assert.deepEqual(keys(await find(value, 'needle -obsolete')), ['a.html#good']);
  assert.equal((await find(value, 'needle -needle')).total, 0);
});

test("exclusions apply to inherited titles but do not leak between chapters or sections", async () => {
  const value = book(
    chapter('a.md', 'Obsolete Guide', [entry('needle', 'same')]),
    chapter('b.md', 'Current Guide', [entry('obsolete', 'old'), entry('needle', 'same')]),
    chapter('c.md', 'Current Guide', [entry('needle', 'same')]),
  );
  assert.deepEqual(keys(await find(value, 'needle -obsolete')), ['b.html#same', 'c.html#same']);
  assert.deepEqual(keys(await find(value, 'needle -obsolete path:c.md')), ['c.html#same']);
});

test("negative phrases normalize whitespace but never bridge real entry boundaries", async () => {
  const value = book(chapter('a.md', 'Guide', [
    entry('needle old', 'separate'), entry('API', 'separate'),
    entry('needle old\n\tAPI', 'together'), entry('needle old API', 'literal'),
  ]));
  assert.deepEqual(keys(await find(value, 'needle -"old API"')), ['a.html#separate']);
});

test("late and chunk-straddling exclusions cannot hide behind an early positive hit", async () => {
  const value = book(chapter('a.md', 'Guide', [
    entry('needle ' + 'x'.repeat(3830) + ' old API', 'boundary'),
    entry('needle ' + 'x'.repeat(1024 * 1024) + ' obsolete', 'eof'),
    entry('needle current', 'good'),
  ]));
  let yields = 0;
  const answer = await find(value, 'needle -obsolete -"old API"', {
    yieldTask: async () => { yields++; },
  });
  assert.deepEqual(keys(answer), ['a.html#good']);
  assert.ok(yields >= 10, 'source scanning must yield even after finding all positive terms');
});

test("positive evidence, offsets, ranking and exact-heading scores survive exclusions", async () => {
  const value = book(chapter('a.md', 'Guide', [
    entry('café', 'exact', 'heading'),
    entry('İ'.repeat(5000) + ' cafe\u0301 ' + '中'.repeat(5000), 'later'),
  ]));
  const data = api.prepare(value), before = JSON.stringify(data);
  assert.deepEqual(plain(await api.search(data, 'café -absent', fast)),
    plain(await api.search(data, 'café', fast)));
  assert.equal(JSON.stringify(data), before);
});

test("Unicode exclusions use the same canonical and caseless matching as positives", async () => {
  const value = book(chapter('a.md', 'Guide', [
    entry('needle Cafe\u0301', 'accent'), entry('needle δοκιμή', 'greek'),
    entry('needle 한', 'hangul'), entry('needle ①', 'compatibility'),
  ]));
  assert.deepEqual(keys(await find(value, 'needle -CAFÉ -ΔΟΚΙΜΉ -한')),
    ['a.html#compatibility']);
  assert.equal((await find(value, 'needle -1')).total, 4, 'no compatibility or accent stripping');
});

test("metadata and negative scans never normalize an unbounded source string", async () => {
  const { api: checked, context } = load();
  vm.runInContext(`
    const originalNormalize = String.prototype.normalize;
    String.prototype.normalize = function (...args) {
      if (this.length > 4096) throw new Error("unbounded normalization");
      return originalNormalize.apply(this, args);
    };
  `, context);
  const value = book(chapter('x'.repeat(200_000) + '/needle.md', 'T'.repeat(200_000), [
    entry('needle ' + 'x'.repeat(200_000) + ' obsolete'),
  ], 'a.html'));
  assert.equal((await checked.search(checked.prepare(value), 'needle path:needle.md -obsolete', fast)).total, 0);
});

test("a huge negative scan remains cancellable with no partial answer", async () => {
  let cancelled = false, yields = 0;
  const answer = await find(book(chapter('a.md', 'Guide', [
    entry('needle ' + 'x'.repeat(2 * 1024 * 1024)),
  ])), 'needle -obsolete', {
    cancelled: () => cancelled,
    yieldTask: async () => { cancelled = true; yields++; },
  });
  assert.equal(answer, null);
  assert.equal(yields, 1);
});

test("chapter rejection skips body decoding, caches metadata, and still yields", async () => {
  const data = api.prepare(book(chapter('outside/a.md', 'Guide',
    Array.from({ length: 500 }, (_, i) => entry('needle', `s${i}`)))));
  let reads = 0, yields = 0;
  for (const row of data) {
    Object.defineProperty(row, 'source', { get() { reads++; return 'outside/a.md'; } });
    Object.defineProperty(row, 'text', { get() { throw new Error('filtered body was accessed'); } });
  }
  const answer = await api.search(data, 'needle path:guide/', { yieldTask: async () => { yields++; } });
  assert.equal(answer.total, 0);
  assert.equal(reads, 1, 'only one metadata admission per chapter');
  assert.ok(yields >= 3);
  let cancelled = false;
  assert.equal(await api.search(data, 'needle path:guide/', {
    cancelled: () => cancelled, yieldTask: async () => { cancelled = true; },
  }), null);
});

test("a rejected section skips later source without suppressing the next section", async () => {
  const data = api.prepare(book(chapter('a.md', 'Guide', [
    entry('needle obsolete', 'bad'),
    ...Array.from({ length: 300 }, () => entry('hidden', 'bad')),
    entry('needle', 'good'),
  ])));
  for (const row of data) if (row.text === 'hidden')
    Object.defineProperty(row, 'text', { get() { throw new Error('rejected section was rescanned'); } });
  let yields = 0;
  assert.deepEqual(keys(await api.search(data, 'needle -obsolete', { yieldTask: async () => { yields++; } })),
    ['a.html#good']);
  assert.ok(yields >= 2);
});

test("full totals and deterministic top-200 retention count admitted sections only", async () => {
  const value = book(chapter('a.md', 'Guide', Array.from({ length: 600 }, (_, i) =>
    entry(i % 2 ? 'needle obsolete' : 'needle', `s${i}`))));
  const answer = await find(value, 'needle -obsolete');
  assert.equal(answer.total, 300);
  assert.equal(answer.results.length, 200);
  assert.equal(answer.results[0].row.anchor, 's0');
  assert.equal(answer.results[199].row.anchor, 's398');
  assert.deepEqual(plain(answer), plain(await find(value, 'needle -obsolete')));
});

// Independent whole-section membership oracle: no production lexer, matcher,
// masks, chunking, ranking or source-offset implementation is called here.
const canonical = (s) => s.toLowerCase().replaceAll('ς', 'σ').normalize('NFD').replace(/\s+/gu, ' ');
function oracle(value, { terms, excluded = [], filters = [] }) {
  const keys = [];
  for (const chapter of value.chapters) {
    if (!filters.every(({ field, value, exclude = false }) =>
      canonical(chapter[field === 'path' ? 'source' : 'title']).includes(canonical(value)) !== exclude)) continue;
    const sections = new Map([['', [chapter.title]]]);
    for (const entry of chapter.index.entries) {
      if (!sections.has(entry.anchor)) sections.set(entry.anchor, []);
      sections.get(entry.anchor).push(entry.text);
    }
    for (const [anchor, entries] of sections) {
      const fields = [chapter.title, ...entries].map(canonical);
      if (terms.every((term) => fields.some((value) => value.includes(canonical(term)))) &&
          !excluded.some((term) => fields.some((value) => value.includes(canonical(term)))))
        keys.push(`${chapter.page}#${anchor}`);
    }
  }
  return keys.sort();
}

test("streaming membership agrees with a whole-section oracle over 120 generated books", async () => {
  let seed = 0x57c0ffee;
  const random = (max) => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) % max);
  const words = ['alpha', 'beta', 'omega', 'café', 'cafe\u0301', 'old\nAPI', 'other'];
  const queries = [
    ['alpha -omega', { terms: ['alpha'], excluded: ['omega'] }],
    ['alpha beta -"old API"', { terms: ['alpha', 'beta'], excluded: ['old API'] }],
    ['café -omega path:guide/', { terms: ['café'], excluded: ['omega'], filters: [{ field: 'path', value: 'guide/' }] }],
    ['path:guide/ -"old API"', { terms: [], excluded: ['old API'], filters: [{ field: 'path', value: 'guide/' }] }],
    ['alpha -title:omega -path:old/', { terms: ['alpha'], filters: [
      { field: 'title', value: 'omega', exclude: true }, { field: 'path', value: 'old/', exclude: true },
    ] }],
  ];
  for (let iteration = 0; iteration < 120; iteration++) {
    const chapters = Array.from({ length: 3 }, (_, c) => chapter(
      `${c % 2 ? 'old' : 'guide'}/${c}.md`, words[random(words.length)],
      Array.from({ length: 20 }, (_, r) => entry(
        Array.from({ length: random(5) }, () => words[random(words.length)]).join(' '),
        `s${Math.floor(r / 4)}`, r % 4 ? 'paragraph' : 'heading',
      )),
    ));
    const value = book(...chapters);
    for (const [query, plan] of queries) {
      const answer = await find(value, query), expected = oracle(value, plan);
      assert.deepEqual(keys(answer), expected, `book ${iteration}: ${query}`);
      assert.equal(answer.total, expected.length);
    }
  }
});

function documentFor(value) {
  class Element {
    constructor(tagName = '') { this.tagName = tagName; this.children = []; this.listeners = new Map(); this.value = ''; this.textContent = ''; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; }
    addEventListener(type, handler) { this.listeners.set(type, handler); }
    removeEventListener(type, handler) { if (this.listeners.get(type) === handler) this.listeners.delete(type); }
    dispatch(type) { this.listeners.get(type)?.({ preventDefault() {} }); }
  }
  const elements = Object.fromEntries(['query', 'search-form', 'search-help', 'status', 'results', 'previous', 'next', 'search-data']
    .map((id) => [id, new Element()]));
  elements['search-data'].textContent = JSON.stringify(value);
  return { elements, getElementById: (id) => elements[id], createElement: (tag) => new Element(tag),
    location: { href: 'file:///book/~fmd-search.html' } };
}
const settled = () => new Promise((resolve) => setImmediate(resolve));

test("the actual reader controller accepts scoped deep links and displays inert source identity", async () => {
  const document = documentFor(book(
    chapter('guide/<img onerror=bad>/a.md', 'Guide', [entry('needle', 'found')], 'safe.html'),
    chapter('other/a.md', 'Guide', [entry('needle')]),
  ));
  const controller = api.mount(document, 'file:///book/~fmd-search.html?q=' + encodeURIComponent('needle path:guide/ -obsolete'));
  await settled();
  const items = document.elements.results.children;
  assert.equal(items.length, 1);
  assert.equal(items[0].children[0].href, './safe.html#found');
  assert.equal(items[0].children[2].textContent, 'Source: guide/<img onerror=bad>/a.md');
  assert.equal(items[0].children[2].children.length, 0);
  assert.match(document.elements['search-help'].textContent, /path:guide\/.*title:.*-obsolete/);
  controller.dispose();
});

test("operator errors replace prior reader results instead of retaining a misleading answer", async () => {
  const document = documentFor(book(chapter('guide/a.md', 'Guide', [entry('needle')])));
  const controller = api.mount(document, 'file:///book/~fmd-search.html?q=needle');
  await settled();
  assert.equal(document.elements.results.children.length, 1);
  document.elements.query.value = '-obsolete';
  document.elements['search-form'].dispatch('submit');
  await settled();
  assert.equal(document.elements.results.children.length, 0);
  assert.match(document.elements.status.textContent, /before exclusions/);
  assert.equal(document.elements.next.disabled, true);
  controller.dispose();
});
