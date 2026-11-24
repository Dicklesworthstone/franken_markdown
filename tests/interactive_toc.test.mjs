// Document navigation contracts from src/toc.rs and src/html.rs, exercised
// through the shipped offline parser and configured presentation adapter.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const renderer = readFileSync(new URL('../src/interactive_renderer.js', import.meta.url), 'utf8');
const presentation = readFileSync(new URL('../src/interactive/presentation.js', import.meta.url), 'utf8');
function parser(code = renderer) {
  const context = vm.createContext({});
  vm.runInContext(code, context);
  return context.parseMarkdownClient;
}
const render = parser();
const navs = html => [...html.matchAll(/<nav class="toc">([\s\S]*?)<\/nav>/g)].map(match => match[0]);
const targets = html => [...html.matchAll(/<a href="#([^"]+)">/g)].map(match => match[1]);
const nativeOutline = '<nav class="toc"><ul>\n<li><a href="#alpha">Alpha</a><ul>\n'
  + '<li><a href="#beta">Beta</a></li>\n</ul></li>\n'
  + '<li><a href="#alpha-2">Alpha</a></li>\n</ul></nav>\n';

test('authored native TOC forms preserve navigation after offline rendering', () => {
  for (const marker of ['[TOC]', '[[TOC]]', '[[_TOC_]]', '[__toc__]', '[*TOC*]', '[**TOC**]',
    '\\[TOC\\]', '&#91;TOC&#93;', '[T&#79;C]', '[toc: +002]']) {
    const html = render(marker + '\n\n# Alpha\n\n## Beta\n\n# Alpha');
    assert.equal(html, nativeOutline + '<h1 id="alpha">Alpha</h1>\n<h2 id="beta">Beta</h2>\n<h1 id="alpha-2">Alpha</h1>\n', marker);
  }
});

test('TOC markers honor inline ownership instead of matching source substrings', () => {
  for (const marker of ['\x60[TOC]\x60', '**[TOC]**', '[***TOC***]', '[~~TOC~~]', 'Before [TOC]', '[TOC] after',
    '[TOC](/guide)', '[TOC](javascript:alert(1))', '![TOC](plot.png)', '[TOC][guide]\n\n[guide]: /guide',
    '[TOC]\n\n[toc]: /guide', '[[TOC]]\n\n[toc]: /guide', '[TOC:\n2]']) {
    const html = render(marker + '\n\n# Heading');
    assert.equal(navs(html).length, 0, marker);
    assert.match(html, /<h1 id="heading">Heading<\/h1>/);
  }
});

test('opaque blocks, table cells and definition terms retain marker-looking content', () => {
  for (const source of ['~~~md\n[TOC]\n~~~', '    [TOC]', '<div>\n[TOC]\n</div>', '$$\n[TOC]\n$$',
    'A | B\n--- | ---\n[TOC] | [[TOC]]', '[TOC]\n: Definition']) {
    const html = render(source + '\n\n# Heading');
    assert.equal(navs(html).length, 0, source);
    assert.match(html, /\[TOC\]/);
  }
});

test('marker depths use source heading levels and override captured default depth', () => {
  const body = '\n\n# One\n\n## Two\n\n### Three';
  for (const [marker, expected] of [
    ['[TOC:1]', ['one']], ['[TOC:2]', ['one', 'two']], ['[TOC:6]', ['one', 'two', 'three']],
    ['[__TOC:2__]', ['one', 'two']], ['[TOC:255]', ['one', 'two', 'three']],
  ]) assert.deepEqual(targets(navs(render(marker + body))[0]), expected, marker);
  assert.deepEqual(targets(navs(render('[TOC]' + body, undefined, undefined, {tocDepth: 2}))[0]), ['one', 'two']);
  assert.deepEqual(targets(navs(render('[TOC:1]' + body, undefined, undefined, {toc: true, tocDepth: 2}))[0]), ['one']);
  for (const marker of ['[TOC:0]', '[TOC:256]', '[TOC:-1]', '[TOC:1.0]', '[TOC :2]', '[TOC:NaN]'])
    assert.equal(navs(render(marker + body)).length, 0, marker);
});

test('outline links reuse actual duplicate and Unicode heading IDs with readable semantic labels', () => {
  const html = render('[TOC]\n\n# [Go *now*](/guide) ![Map](map.png) &amp; \x60x\x60\n\n# Café\n\n# Café\n\n# Café-2\n\n# Café');
  assert.deepEqual(targets(navs(html)[0]), ['go-now-map-x', 'café', 'café-2', 'café-2-2', 'café-3']);
  assert.match(navs(html)[0], />Go now Map &amp; x<\/a>/);
  for (const id of targets(navs(html)[0])) assert.ok(html.includes('id="' + id + '"'), id);
  assert.doesNotMatch(navs(html)[0], /<img|<em|href="\/guide"/);
});

test('note labels, formulas and safe HTML text do not corrupt heading labels or create extra citations', () => {
  const html = render('[TOC]\n\n# Plan[^n] $x_1$ <b>today</b><!-- hidden -->\n\nSee[^n].\n\n[^n]: Details.');
  const nav = navs(html)[0];
  assert.match(nav, /href="#plan-x-1-today">Plan\[\^n\] x_1 today<\/a>/);
  assert.equal((html.match(/class="footnote-ref"/g) || []).length, 2);
  assert.equal((html.match(/class="footnote-backref"/g) || []).length, 2);
  assert.doesNotMatch(nav, /doc-noteref|hidden|<b>/);
});

test('body outline excludes note headings while preserving their collision-free destinations', () => {
  const html = render('[^n]: # Shared\n\nSee[^n].\n\n[TOC]\n\n# Shared');
  assert.deepEqual(targets(navs(html)[0]), ['shared']);
  assert.match(html, /<h1 id="shared">Shared<\/h1>/);
  assert.match(html, /<li id="fn-n"><h1 id="shared-2">Shared<\/h1>/);
});

test('markers and headings work inside recursive containers and tight lists', () => {
  const html = render('> [!NOTE]\n>\n> [[TOC:2]]\n\n- # First\n  - ## Second\n\n> ### Third');
  assert.match(html, /<aside class="callout callout-note">[\s\S]*<nav class="toc">/);
  assert.deepEqual(targets(navs(html)[0]), ['first', 'second']);
  const tight = render('- [TOC]\n- Item\n\n# Body');
  assert.match(tight, /<li><nav class="toc">/);
  assert.doesNotMatch(tight, /<p><nav/);
});

test('configured automatic TOC follows native placement and yields to authored body markers', () => {
  const options = {toc: true, tocDepth: 2};
  const source = 'Introduction.\n\n> # First\n\n- ## Second\n\n### Third';
  const html = render(source, undefined, undefined, options);
  assert.match(html, /^<p>Introduction\.<\/p>\n<blockquote>\n<nav class="toc">/);
  assert.equal(navs(html).length, 1);
  assert.deepEqual(targets(navs(html)[0]), ['first', 'second']);
  const placed = render('# First\n\n> [TOC:1]\n\n## Second', undefined, undefined, options);
  assert.match(placed, /^<h1 id="first">First<\/h1>\n<blockquote>\n<nav/);
  assert.equal(navs(placed).length, 1);
  assert.deepEqual(targets(navs(placed)[0]), ['first']);
});

test('a note marker uses the body outline without suppressing the automatic body TOC', () => {
  const html = render('Read[^n].\n\n# Body\n\n[^n]: [TOC]', undefined, undefined, {toc: true});
  assert.equal(navs(html).length, 2);
  for (const nav of navs(html)) assert.deepEqual(targets(nav), ['body']);
  assert.match(html, /<li id="fn-n"><nav class="toc">/);
  const onlyNote = render('Read[^n].\n\n[^n]: # Note', undefined, undefined, {toc: true});
  assert.equal(navs(onlyNote).length, 0);
  assert.match(onlyNote, /<h1 id="note">Note<\/h1>/);
});

test('empty outlines remove markers and navigation policy remains document-local across profiles', () => {
  assert.equal(render('[TOC]\n\nBody'), '<p>Body</p>\n');
  assert.equal(render('[TOC:1]\n\n## Deep'), '<h2 id="deep">Deep</h2>\n');
  for (const profile of [undefined, null, 'commonmark-gfm', 'gfm-plus']) {
    assert.equal(navs(render('[TOC]\n\n# Body', undefined, profile)).length, 1);
    assert.equal(navs(render('# Body', undefined, profile, {toc: true})).length, 1);
    assert.equal(navs(render('# Body', undefined, profile)).length, 0);
  }
});

test('TOC output escapes labels and honors expansion limits without leaking state', () => {
  const html = render('[TOC]\n\n# \x60<img onerror="alert(1)">\x60 & <custom>');
  assert.match(navs(html)[0], /&lt;img onerror=&quot;alert\(1\)&quot;&gt; &amp; &lt;custom&gt;/);
  assert.doesNotMatch(navs(html)[0], /<img|<custom>/);
  const bounded = parser(renderer.replace('outputUnits: 32 * 1024 * 1024', 'outputUnits: 1024'));
  const source = '[TOC]\n\n'.repeat(20) + '# First\n\n## Second\n\n# Third';
  assert.throws(() => bounded(source), error => error.code === 'FMD_RENDER_LIMIT');
  assert.equal(bounded('# Small'), '<h1 id="small">Small</h1>\n');
});

test('configured presentation renders authored navigation through the same production parser', () => {
  const context = vm.createContext({});
  vm.runInContext(presentation.slice(0, presentation.indexOf('\n(function() {')), context);
  const config = {version: 1, css: '', title: 'Guide', lang: 'en', toc: true, tocDepth: 2};
  const adapter = context.fmdCreateStyledPresentation(config);
  config.toc = false; config.tocDepth = 1; // Captured navigation cannot change behind the editor.
  const source = '# ![Map](map.png)\n\n## Two\n\n### Three\n\nRead[^n].\n\n[^n]: # Note';
  const fragment = adapter.render(render, source);
  assert.equal(navs(fragment).length, 1);
  assert.deepEqual(targets(navs(fragment)[0]), ['map', 'two']);
  assert.match(navs(fragment)[0], />Map<\/a>/);
  const placed = adapter.render(render, '# First\n\n[TOC:1]\n\n## Two');
  assert.match(placed, /^<h1 id="first">First<\/h1>\n<nav/);
  assert.equal(navs(placed).length, 1);
  assert.match(adapter.documentHtml(placed), /<main class="fmd"><h1 id="first">/);
});

test('invalid navigation options fail before observing editable source or image bindings', () => {
  let touched = false;
  const source = {toString() { touched = true; return '# Hidden'; }};
  for (const options of [true, false, 1, '', [], {toc: 1}, {tocDepth: 0}, {tocDepth: 7}, {tocDepth: 1.5}])
    assert.throws(() => render(source, undefined, undefined, options), error => error.code === 'FMD_INVALID_TOC_OPTIONS');
  assert.equal(touched, false);
});
