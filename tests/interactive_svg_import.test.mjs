// Production SVG resource/sizing policy. The companion Chromium gate executes
// its real XML parser, image decoder, picker and saved-workspace lifecycle.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';

const context = vm.createContext({TextDecoder, atob, btoa, Uint8Array});
vm.runInContext(readFileSync(new URL('../src/interactive_import.js', import.meta.url), 'utf8'), context);
const {fmdImportSvgLength: length, fmdImportSvgStyle: style,
  fmdImportSvgResource: resource, fmdImportImageInfo: info} = context;

test('SVG absolute dimensions follow native 96-dpi units and keep fractional values', () => {
  for (const [value, expected] of [['160', 160], [' 1.25e2px ', 125], ['.5in', 48],
    ['2.54cm', 96], ['25.4mm', 96], ['101.6q', 96], ['72pt', 96], ['6pc', 96], ['+0.25', .25]]) {
    assert.ok(Math.abs(length(value) - expected) < 1e-10, value);
  }
});

test('SVG intrinsic sizes reject overflow, negative, relative and unevaluated expressions', () => {
  for (const value of ['0', '-1', '16385', '200in', '1e999', 'NaN', 'Infinity', '',
    '10vw', '2em', '100%', 'calc(1px + 1px)', 'var(--width)', '10pxjunk', '1 2']) {
    assert.throws(() => length(value), /SVG dimensions/, value);
  }
});

test('static SVG CSS preserves local paints, gradients, typography and bounded root sizing', () => {
  for (const value of ['fill:url(#gradient);stroke:#123', 'filter: URL( "#shadow" )',
    'fill:u\\72l(\'#paint\')', '/* @import "unread"; */ .chart { font:14px serif; fill: teal }',
    'svg { width: 100%; height: auto; max-width: 1in !important }',
    '/* before */ svg { width/**/: /* value */ 160px; fill: url( " #paint " ) }']) {
    assert.doesNotThrow(() => style(value), value);
  }
  assert.equal(JSON.stringify(style('width:1in;height:72pt')), JSON.stringify({width:96,height:96}));
});

test('escaped and ordinary SVG CSS cannot authorize external fetches', () => {
  for (const value of ['@import "https://example.invalid/a.css";', '@\\69mport "x.css";',
    '@/**/import "x.css";', '@font-face{src:url(data:font/ttf;base64,AA==)}',
    'fill:url(https://example.invalid/a.svg#paint)', 'filter:u\\72l(//example.invalid/#x)',
    'fill:url("\\68ttps://example.invalid/a")', 'fill:url(\'#x\');stroke:url(../other.svg#x)',
    'background:image-set("https://example.invalid/a.png" 1x)',
    'background:-webkit-image-set("x.png" 1x)', 'background:im\\61ge("x.png")',
    'src:src("x.woff2")', 'filter:url(#x', 'fill:url(data:image/svg+xml;base64,AAAA)',
    'fill:url("/*external-resource*/#paint")', "fill:url('/*external-resource*/#paint')",
    'fill:url(\\27#paint\\27)', 'fill:url(\\22#paint\\22)', 'fill:url(\\\'#paint\\\')',
    'fill:url("\\2f*external-resource*\\2f#paint")', 'fill:url(/*ignored*/#paint)',
    'fill:url(\u00a0#paint)', 'fill:url("\u00a0#paint")', 'fill:url(\\a0#paint)', 'fill:url(\u2000#paint)']) {
    assert.throws(() => style(value), /SVG/, value);
  }
});

test('CSS sizing cannot bypass SVG decode budgets with escapes, functions or logical dimensions', () => {
  for (const value of ['width:20000px', 'width:1e9in', 'height:calc(10000px + 10000px)',
    'w\\69 dth:20000px', 'height:var(--height)', 'width:101%', 'inline-size:20000px',
    'min-block-size:20000px', 'aspect-ratio:1/9999999', 'contain-intrinsic-size:999999px',
    'width/**/:20000px', 'width:/* comment */20000px']) {
    assert.throws(() => style(value), /SVG/, value);
  }
});

test('SVG resource links preserve local fragments and ordinary native PDF hyperlinks', () => {
  for (const element of ['use', 'lineargradient', 'pattern']) {
    assert.doesNotThrow(() => resource('#local-id', element));
    assert.doesNotThrow(() => resource(' \t#local-id\n', element));
  }
  for (const target of ['https://example.com/docs#chart', 'http://example.com', 'mailto:reader@example.com']) {
    assert.doesNotThrow(() => resource(target, 'a'));
    assert.throws(() => resource(target, 'image'), /SVG resources/);
  }
});

test('SVG resources cannot load paths, network locations, script URLs or nested SVG documents', () => {
  for (const target of ['other.svg#shape', '//example.invalid/shape.svg', '/image.png', 'file:///tmp/image.png',
    'javascript:alert(1)', 'data:text/html;base64,SGk=', 'data:image/svg+xml;base64,PHN2Zy8+',
    'data:image/png;base64,AAAA', 'https:\n//example.invalid/a', '#bad\nfragment', '\u00a0#paint', '\u2000#paint']) {
    for (const element of ['image', 'use', 'a']) assert.throws(() => resource(target, element), /SVG|PNG|image/);
  }
  for (const element of ['image', 'feimage']) assert.throws(() => resource('#same-document', element), /SVG resources/);
});

test('SVG embedded raster images retain content-based MIME and exact canonical bytes', () => {
  const gif = readFileSync(new URL('./fixtures/gif/animated-colors.base64', import.meta.url), 'utf8').trim();
  const admitted = resource('data:image/gif;base64,' + gif, 'image');
  assert.equal(admitted.pixels, 4); assert.equal(admitted.bytes, Buffer.from(gif, 'base64').length);
  assert.doesNotThrow(() => resource('data:image/gif;base64,' + gif, 'feimage'));
  assert.throws(() => resource('data:image/png;base64,' + gif, 'image'), /declared format/);
  assert.throws(() => resource('data:image/gif;base64,' + gif, 'use'), /SVG resources/);
});

test('SVG DTD and entity declarations are refused before allocating an XML parser', () => {
  for (const source of ['<!DOCTYPE svg SYSTEM "https://example.invalid/svg.dtd"><svg/>',
    '<!DOCTYPE svg [<!ENTITY x "expand">]><svg>&x;</svg>', '<!ENTITY x SYSTEM "file:///tmp/a"><svg/>']) {
    assert.throws(() => info(new TextEncoder().encode(source)), /document types and entity declarations/);
  }
  assert.throws(() => info(Uint8Array.from([60,115,118,103,32,0xff,62])), /valid UTF-8/);
});
