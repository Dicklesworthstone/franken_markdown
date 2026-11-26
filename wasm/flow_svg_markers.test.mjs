import assert from "node:assert/strict";
import test from "node:test";
import { parseStaticSvg } from "./flow_svg.mjs";
import { assetFailure } from "./flow_raster.mjs";

const svg = body => '<svg width="64" height="32">' + body + '</svg>';
const arrow = (id = "arrow", body = '<path d="M0 0L8 4L0 8Z" fill="red"/>') =>
  '<marker id="' + id + '" markerWidth="8" markerHeight="8" refX="8" refY="4" ' +
  'orient="auto-start-reverse" markerUnits="userSpaceOnUse" viewBox="0 0 8 8">' + body + '</marker>';
const admit = source => parseStaticSvg(source, assetFailure);
const unsupported = (source, reason) => assert.throws(() => admit(source), error =>
  error.code === "UNSUPPORTED_IMAGE" && (!reason || error.message.includes(reason)));

test("direct arrow markers preserve geometry, orientation and presentation attributes", () => {
  for (const [name, geometry] of [
    ["path", 'd="M8 16L32 16L48 16"'],
    ["line", 'x1="8" y1="16" x2="48" y2="16"'],
    ["polyline", 'points="8,16 32,16 48,16"'],
    ["polygon", 'points="8,16 32,8 48,16"'],
  ]) for (const property of ["marker-start", "marker-mid", "marker-end"]) {
    for (const setting of [property + '="url(#arrow)"', 'style="' + property + ': url(#arrow); stroke: black"']) {
      const info = admit(svg('<defs>' + arrow() + '</defs><' + name + ' ' + geometry + ' ' + setting + '/>'));
      assert.equal(info.width, 64); assert.equal(info.height, 32);
      assert.ok(info.preview.includes('<marker id="arrow"'));
      assert.ok(info.preview.includes('orient="auto-start-reverse"'));
      assert.ok(info.preview.includes('markerUnits="userSpaceOnUse"'));
      assert.ok(info.preview.includes('viewBox="0 0 8 8"'));
      assert.ok(info.preview.includes(property));
      assert.ok(info.preview.includes('url(#arrow)'));
    }
  }
});

test("forward references and reusable marker child paths remain self-contained", () => {
  const source = svg('<path d="M0 0L10 0" marker-end="url(&quot;#arrow&quot;)"/>' +
    '<defs>' + arrow("arrow", '<use href="#tip"/>') +
    '<path id="tip" d="M0 0L8 4L0 8Z"/></defs>');
  const info = admit(source);
  assert.ok(info.preview.includes('marker-end="url(&quot;#arrow&quot;)"'));
  assert.ok(info.preview.includes('<use href="#tip">'));
  assert.equal(admit(svg('<path d="M0 0L1 1" marker-mid="none"/>')).width, 64);
  const override = admit(svg('<defs>' + arrow() + '</defs><path d="M0 0L1 1" ' +
    'marker-mid="url(#arrow)" style="marker-mid:none;marker-mid:url(#arrow)"/>'));
  assert.ok(override.preview.includes('marker-mid="url(#arrow)"'));
  assert.ok(override.preview.includes('style="marker-mid:none;marker-mid:url(#arrow)"'));
});

test("marker destinations must be local marker definitions", () => {
  for (const name of ["path", "g", "linearGradient", "clipPath", "symbol"]) {
    unsupported(svg('<defs><' + name + ' id="wrong"/></defs>' +
      '<path d="M0 0L1 1" marker-end="url(#wrong)"/>'), "marker elements");
  }
  for (const destination of [
    "https://example.test/arrows.svg#arrow", "data:image/svg+xml,arrow", "#missing", "#bad%20id",
  ]) unsupported(svg('<path d="M0 0L1 1" marker-end="url(' + destination + ')"/>'));
});

test("inherited, shorthand and indirect marker properties cannot bypass placement accounting", () => {
  for (const name of ["svg", "g", "use", "symbol", "marker", "text", "rect", "circle"]) {
    unsupported(svg('<defs>' + arrow() + '</defs><' + name + ' marker-end="url(#arrow)"/>'), "directly");
    unsupported(svg('<defs>' + arrow() + '</defs><' + name + ' style="marker-mid:url(#arrow)"/>'), "directly");
  }
  for (const value of ["inherit", "initial", "unset", "revert", "var(--arrow)", "url(#arrow) red"]) {
    unsupported(svg('<defs>' + arrow() + '</defs><path d="M0 0L1 1" marker-end="' + value + '"/>'));
  }
  unsupported(svg('<defs>' + arrow() + '</defs><path d="M0 0L1 1" style="marker:url(#arrow)"/>'));
  unsupported(svg('<defs>' + arrow() + '</defs><path d="M0 0L1 1" marker-end="url(#arrow)"><g/></path>'),
    "graphical child");
  assert.equal(admit(svg('<defs>' + arrow() + '</defs><path d="M0 0L1 1" marker-end="url(#arrow)">' +
    '<title>Arrow</title><desc>Bounded line</desc></path>')).width, 64);
  // A marker reachable by ID can inherit from a non-rendering ancestor.
  // Descriptive containers must therefore be text-only, not merely inert
  // where they occur in the DOM.
  for (const description of ["title", "desc"]) {
    unsupported(svg('<defs>' + arrow("big", '<rect/>'.repeat(100)) + '</defs>' +
      '<path d="M0 0L1 1" marker-mid="url(#big)"><' + description + '>' +
      arrow("hidden", '<path d="M0 0H' + '1 '.repeat(1000) + '"/>') +
      '</' + description + '></path><line marker-end="url(#hidden)"/>'), "graphical child");
    unsupported(svg('<defs>' + arrow() + '</defs><path d="M0 0L1 1" marker-end="url(#arrow)">' +
      '<' + description + '><g><path id="hidden" d="M0 0L1 1"/></g></' + description +
      '></path><use href="#hidden"/>'), "graphical child");
  }
});

test("marker references retain cycle and expanded-depth rejection", () => {
  unsupported(svg('<defs>' + arrow("arrow", '<path d="M0 0L1 1" marker-end="url(#arrow)"/>') +
    '</defs><path d="M0 0L1 1" marker-end="url(#arrow)"/>'), "cyclic");
  unsupported(svg('<defs>' +
    arrow("a", '<path d="M0 0L1 1" marker-end="url(#b)"/>') +
    arrow("b", '<path d="M0 0L1 1" marker-end="url(#a)"/>') +
    '</defs><line marker-end="url(#a)"/>'), "cyclic");
  let body = arrow("last");
  for (let i = 0; i < 32; i++) {
    body += '<g id="n' + i + '"><use href="#' + (i ? 'n' + (i - 1) : "last") + '"/></g>';
  }
  unsupported(svg('<defs>' + body + '</defs>' +
    arrow("outer", '<use href="#n31"/>') + '<line marker-end="url(#outer)"/>'), "deep");
});

test("marker contents debit the unchanged expanded-byte ceiling at every possible vertex", () => {
  const definition = '<defs>' + arrow("arrow", '<text>' + "a".repeat(512 * 1024) + '</text>') + '</defs>';
  const geometry = 'M0 0H' + '1 '.repeat(1000);
  unsupported(svg(definition + '<path d="' + geometry + '" marker-mid="url(#arrow)"/>'), "expanded content");
  // An endpoint is not replicated at every intermediate vertex. The same
  // original geometry and marker payload remain admissible for one end.
  assert.equal(admit(svg(definition + '<path d="' + geometry + '" marker-end="url(#arrow)"/>')).width, 64);
});

test("implicit path segments, compact decimals, exponents and point lists debit marker fanout", () => {
  const definition = '<defs>' + arrow("arrow", '<rect width="1" height="1"/>'.repeat(100)) + '</defs>';
  for (const geometry of [
    'M0 0H' + '1 '.repeat(1000),
    'M0 0h' + '-1'.repeat(1000),
    'M0 0H' + '.5'.repeat(1000),
    'M0 0h' + '1e0 '.repeat(1000),
    'M0 0A' + '1 1 0 011 1 '.repeat(1000),
    'M0 0a' + '1 1 0 01.5.5 '.repeat(1000),
    'M0 0H' + '1 '.repeat(1000) + 'malformed-tail',
  ]) unsupported(svg(definition + '<path d="' + geometry + '" marker-mid="url(#arrow)"/>'), "expanded scene");
  for (const name of ["polyline", "polygon"]) {
    unsupported(svg(definition + '<' + name + ' points="' + '1,1 '.repeat(1000) +
      '" marker-mid="url(#arrow)"/>'), "expanded scene");
  }
});

test("close commands and repeated subpaths cannot hide marker expansion", () => {
  const definition = '<defs>' + arrow("arrow", '<rect/>'.repeat(100)) + '</defs>';
  for (const property of ["marker-start", "marker-mid", "marker-end"]) {
    unsupported(svg(definition + '<path d="' + 'M0 0Z'.repeat(1000) +
      '" ' + property + '="url(#arrow)"/>'), "expanded scene");
  }
});

test("cached use subtrees and nested marker definitions retain multiplicative accounting", () => {
  const reused = '<defs>' + arrow("arrow", '<rect/>'.repeat(100)) +
    '<path id="line" d="M0 0H' + '1 '.repeat(100) + '" marker-mid="url(#arrow)"/></defs>';
  assert.equal(admit(svg(reused + '<use href="#line"/>')).width, 64);
  unsupported(svg(reused + '<use href="#line"/>'.repeat(10)), "expanded scene");
  const nested = '<defs>' + arrow("inner", '<rect/>'.repeat(100)) +
    arrow("outer", '<path d="M0 0H' + '1 '.repeat(100) + '" marker-mid="url(#inner)"/>') + '</defs>';
  unsupported(svg(nested + '<path d="M0 0H' + '1 '.repeat(10) + '" marker-mid="url(#outer)"/>'),
    "expanded scene");
  const combined = '<defs>' + arrow("inner", '<rect/>'.repeat(10)) +
    arrow("outer", '<line marker-end="url(#inner)"/>') +
    '<path id="multi" d="M0 0H' + '1 '.repeat(10) + '" marker-mid="url(#outer)"/></defs>';
  assert.equal(admit(svg(combined + '<use href="#multi"/>'.repeat(5))).width, 64);
  unsupported(svg(combined + '<use href="#multi"/>'.repeat(1000)), "expanded scene");
  assert.equal(admit(svg('<defs>' + arrow("inner") +
    arrow("outer", '<line x1="0" y1="4" x2="8" y2="4" marker-end="url(#inner)"/>') +
    '</defs><line marker-end="url(#outer)"/>')).width, 64);
});

test("marker admission preserves external-resource, animation and pattern refusals", () => {
  for (const body of [
    '<image href="https://example.test/image.png"/>',
    '<use href="https://example.test/image.svg#tip"/>',
    '<animate attributeName="orient" values="0;360"/>',
    '<rect fill="url(https://example.test/paint.svg#g)"/>',
    '<pattern id="pattern" width="1" height="1"><rect/></pattern>',
    '<style>path { marker-mid: url(#arrow) }</style>',
  ]) unsupported(svg('<defs>' + arrow("arrow", body) + '</defs><line marker-end="url(#arrow)"/>'));
});
