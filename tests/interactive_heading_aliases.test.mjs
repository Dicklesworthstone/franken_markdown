// Run with: node --test tests/interactive_heading_aliases.test.mjs
// Exercise the production standalone renderer and its emitted destinations.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8'), context);
const render = (source, profile, contents) => context.parseMarkdownClient(source, undefined, profile, contents);
const headingIds = html => [...html.matchAll(/<h[1-6] id="([^"]+)"/g)].map(match => match[1]);

test('GitHub-authored TOC fragments reach existing canonical headings without adding IDs', () => {
  const source = '- [PART 0 — DOCUMENT CONTROL](#part-0--document-control)\n\n# PART 0 — DOCUMENT CONTROL\n';
  const html = render(source);
  assert.equal(html, render(source.replace('#part-0--', '#part-0-')));
  assert.match(html, /href="#part-0-document-control"/);
  assert.deepEqual(headingIds(html), ['part-0-document-control']);
  assert.equal(source.includes('(#part-0--document-control)'), true);
});

test('alias duplicate numbering is independent of canonical heading collisions', () => {
  const html = render('[first](#a--b) [second](#a--b-1) [exact](#a-b) [unknown](#a--b-2)\n\n'
    + '# A B\n\n# A — B\n\n# A — B');
  assert.deepEqual(headingIds(html), ['a-b', 'a-b-2', 'a-b-3']);
  assert.match(html, /href="#a-b-2">first<\/a>/);
  assert.match(html, /href="#a-b-3">second<\/a>/);
  assert.match(html, /href="#a-b">exact<\/a>/);
  assert.match(html, /href="#a--b-2">unknown<\/a>/);
});

test('literal numbered titles reserve their GitHub spelling before later duplicates', () => {
  const html = render('[first](#a--b) [literal](#a--b-1) [third](#a--b-2)\n\n'
    + '# A — B\n\n# A — B-1\n\n# A — B');
  assert.deepEqual(headingIds(html), ['a-b', 'a-b-1', 'a-b-2']);
  assert.match(html, /href="#a-b">first<\/a>/);
  assert.match(html, /href="#a-b-1">literal<\/a>/);
  assert.match(html, /href="#a-b-2">third<\/a>/);
});

test('exact emitted footnote destinations shadow heading aliases even for forward links', () => {
  const html = render('[note](#fn-a--b) [encoded](#fn-a%2D%2Db) [heading](#fn-a-b)\n\n'
    + '# fn A — B\n\nCited[^a--b].\n\n[^a--b]: Note.');
  assert.match(html, /href="#fn-a--b">note<\/a>/);
  assert.match(html, /href="#fn-a%2D%2Db">encoded<\/a>/);
  assert.match(html, /href="#fn-a-b">heading<\/a>/);
  assert.match(html, /<li id="fn-a--b">/);
  assert.deepEqual(headingIds(html), ['fn-a-b']);
});

test('local aliases decode once, preserve queries and retain malformed or unknown fragments', () => {
  const html = render('[encoded](?view=print&a=1#a%2D%2Db) [once](#a%252D%252Db) '
    + '[case](#A--B) [malformed](#a%2--b) [invalid](#a--b%C0%AF) [control](#a--b%00) '
    + '[unknown](#other--heading) [exact](#a%2Db)\n\n# A — B');
  assert.match(html, /href="\?view=print&amp;a=1#a-b">encoded<\/a>/);
  for (const [label, destination] of [
    ['once', '#a%252D%252Db'], ['case', '#A--B'], ['malformed', '#a%2--b'],
    ['invalid', '#a--b%C0%AF'], ['control', '#a--b%00'],
    ['unknown', '#other--heading'], ['exact', '#a%2Db'],
  ]) assert.ok(html.includes('href="' + destination + '">' + label + '</a>'));
});

test('external, file, image and inert raw HTML destinations keep their admission policy', () => {
  const html = render('[remote](https://example.test/#a--b) [file](guide.md#a--b) '
    + '[unsafe](javascript:alert(1)) ![picture](#a--b)\n\n'
    + '<a href="#a--b">raw</a>\n\n# A — B');
  assert.match(html, /href="https:\/\/example\.test\/#a--b">remote<\/a>/);
  assert.match(html, /href="guide\.md#a--b">file<\/a>/);
  assert.match(html, /<img src="#a--b" alt="picture">/);
  assert.match(html, /&lt;a href=&quot;#a--b&quot;&gt;raw&lt;\/a&gt;/);
  assert.doesNotMatch(html, /href="javascript:/);
  assert.doesNotMatch(html, /<a href="#a-b">raw/);
});

test('aliases use visible heading semantics including nested labels, code and Unicode', () => {
  const source = '[styled](#a--b) [image](#c--d) [unicode](#café--οσ) [literal](#x---y)\n\n'
    + '# **A** — [B](#a--b "ignored title")\n\n'
    + '# `C` &amp; ![D](ignored.png)<!-- ignored -->\n\n'
    + '# Café — ΟΣ\n\n# X---Y';
  const html = render(source);
  assert.deepEqual(headingIds(html), ['a-b', 'c-d', 'café-οσ', 'x-y']);
  for (const [label, id] of [['styled', 'a-b'], ['image', 'c-d'], ['unicode', 'café-οσ'], ['literal', 'x-y']])
    assert.ok(html.includes('href="#' + id + '">' + label + '</a>'));
  assert.match(html, /<a href="#a-b" title="ignored title">B<\/a>/);
});

test('inert HTML and code headings cannot reserve source aliases or shift canonical IDs', () => {
  const html = render('[target](#a--b)\n\n# A B\n\n<h2>A B</h2>\n\n'
    + '```md\n# A — B\n```\n\n# A — B');
  assert.deepEqual(headingIds(html), ['a-b', 'a-b-2']);
  assert.match(html, /href="#a-b-2">target<\/a>/);
  assert.match(html, /&lt;h2&gt;A B&lt;\/h2&gt;/);
});

test('nested headings and emitted note headings share body then first-reference order', () => {
  const html = render('[quote](#a--b) [list](#a--b-1) [note](#a--b-2) ref[^used].\n\n'
    + '> # A — B\n\n- # A — B\n\n'
    + '[^unused]:\n    # A — B\n\n'
    + '[^used]:\n    # A — B\n\n'
    + '[^used]:\n    # A — B\n');
  assert.deepEqual(headingIds(html), ['a-b', 'a-b-2', 'a-b-3']);
  for (const [label, id] of [['quote', 'a-b'], ['list', 'a-b-2'], ['note', 'a-b-3']])
    assert.ok(html.includes('href="#' + id + '">' + label + '</a>'));
  assert.doesNotMatch(html, /id="fn-unused"/);
});

test('footnote marks do not enter aliases and first-reference note ordering stays stable', () => {
  const html = render('[body](#a--b) [first](#a--b-1) [second](#a--b-2)\n\n'
    + '# A — B[^later]\n\nThen[^earlier].\n\n'
    + '[^earlier]:\n    # A — B\n\n'
    + '[^later]:\n    # A — B\n');
  assert.deepEqual(headingIds(html), ['a-b', 'a-b-2', 'a-b-3']);
  assert.match(html, /href="#a-b">body<\/a>/);
  assert.match(html, /<li id="fn-later"><h1 id="a-b-2">/);
  assert.match(html, /<li id="fn-earlier"><h1 id="a-b-3">/);
  assert.match(html, /href="#a-b-2">first<\/a>/);
  assert.match(html, /href="#a-b-3">second<\/a>/);
});

test('explicit profiles and generated contents retain canonical navigation and source aliases', () => {
  const source = '[authored](#a--b)\n\n# A — B\n\n## C D';
  for (const profile of [undefined, 'gfm-plus', 'commonmark-gfm']) {
    for (const contents of [undefined, {toc: true, tocDepth: 2}]) {
      const html = render(source, profile, contents);
      assert.match(html, /href="#a-b">authored<\/a>/);
      assert.deepEqual(headingIds(html), ['a-b', 'c-d']);
      assert.equal(html, render(source.replace('#a--b', '#a-b'), profile, contents));
    }
  }
});

test('the heading count limit disables every alias without changing exact heading IDs', () => {
  const heading = '# A — B\n\n';
  const admitted = render('[last](#a--b-4095)\n\n' + heading.repeat(4096));
  assert.match(admitted, /href="#a-b-4096">last<\/a>/);
  const limited = render('[early](#a--b) [exact](#a-b)\n\n' + heading.repeat(4097));
  assert.match(limited, /href="#a--b">early<\/a>/);
  assert.match(limited, /href="#a-b">exact<\/a>/);
  assert.equal(headingIds(limited).length, 4097);
  assert.match(limited, /<h1 id="a-b-4097">/);
});

test('alias byte limits count UTF-8 and discard the complete optional inventory', () => {
  const prefix = '[early](#a--b) [exact](#a-b)\n\n# A — B\n\n';
  // A scalar count below 8192 still exceeds the per-ID UTF-8 byte limit.
  const oversizedId = render(prefix + '# ' + 'é'.repeat(4097));
  assert.match(oversizedId, /href="#a--b">early<\/a>/);
  assert.match(oversizedId, /href="#a-b">exact<\/a>/);
  // Each ID fits, but retained canonical/alias keys exceed the 1 MiB inventory.
  const manyLongIds = Array.from({length: 300}, (_, index) => '# ' + 'x'.repeat(800) + index + '\n\n').join('');
  const oversizedInventory = render(prefix + manyLongIds);
  assert.match(oversizedInventory, /href="#a--b">early<\/a>/);
  assert.match(oversizedInventory, /href="#a-b">exact<\/a>/);
  assert.equal(headingIds(oversizedInventory).length, 301);
});

test('alias inventories never leak between renders or resolve arbitrary collapsed fragments', () => {
  assert.match(render('[known](#a--b)\n\n# A — B'), /href="#a-b">known<\/a>/);
  assert.equal(render('[unknown](#a--b)\n\n# A B'), '<p><a href="#a--b">unknown</a></p>\n<h1 id="a-b">A B</h1>\n');
  assert.equal(render('[unknown](#a---b)\n\n# A — B'), '<p><a href="#a---b">unknown</a></p>\n<h1 id="a-b">A — B</h1>\n');
});
