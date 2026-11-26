import assert from "node:assert/strict";
import test from "node:test";
import { parseStaticSvg } from "./flow_svg.mjs";
import { assetFailure } from "./flow_raster.mjs";

const svg = (body = "", attributes = 'width="20" height="10"') =>
  "<svg " + attributes + ">" + body + "</svg>";
const admit = source => parseStaticSvg(source, assetFailure);
const code = expected => error => error?.code === expected;
const unsupported = source => assert.throws(() => admit(source), code("UNSUPPORTED_IMAGE"));
const invalid = source => assert.throws(() => admit(source), code("INVALID_IMAGE"));

test("static SVG derives CSS lengths and rounds only its canonical raster viewport", () => {
  for (const [width, height, expectedWidth, expectedHeight] of [
    ["96", "48", 96, 48],
    ["10.1px", "4.01px", 11, 5],
    ["72pt", "36pt", 96, 48],
    ["6pc", "3pc", 96, 48],
    ["25.4mm", "12.7mm", 96, 48],
    ["2.54cm", "1.27cm", 96, 48],
    ["1in", ".5in", 96, 48],
    ["1e2", "5e1", 100, 50],
  ]) {
    const info = admit(svg('<rect width="1" height="1"/>',
      'width="' + width + '" height="' + height + '"'));
    assert.equal(info.mime, "image/svg+xml");
    assert.equal(info.width, expectedWidth, width);
    assert.equal(info.height, expectedHeight, height);
    assert.ok(info.preview.includes('width="' + expectedWidth + '"'));
    assert.ok(info.preview.includes('height="' + expectedHeight + '"'));
    assert.ok(info.preview.includes('xmlns="http://www.w3.org/2000/svg"'));
  }
});

test("viewBox sizing preserves aspect ratio with missing or percentage dimensions", () => {
  for (const [attributes, width, height] of [
    ['viewBox="0 0 32 16"', 32, 16],
    ['viewBox="-5.5, -4, 12.25, 6.125"', 13, 7],
    ['width="40" viewBox="0 0 32 16"', 40, 20],
    ['height="20" viewBox="0 0 32 16"', 40, 20],
    ['width="50%" height="100%" viewBox="0 0 32 16"', 32, 16],
    ['viewBox="0 , 0,32 ,16"', 32, 16],
    ['viewBox="0&#x9;0&#xA;32&#xD;16"', 32, 16],
  ]) {
    const info = admit(svg("", attributes));
    assert.equal(info.width, width, attributes);
    assert.equal(info.height, height, attributes);
  }
  unsupported("<svg/>");
  for (const attributes of [
    'width="0" height="1"', 'width="-1" height="1"', 'width="NaN" height="1"',
    'width="1em" height="1"', 'width="1%" height="1"', 'width="1" height=""',
    'viewBox="0,,0 10 10"', 'viewBox="0 0, ,10 10"', 'viewBox="0 0 10"',
    'viewBox="0 0 -10 10"', 'viewBox="0 0 10 0"', 'viewBox="0 0 1e999 1"',
  ]) invalid(svg("", attributes));
  assert.throws(() => admit(svg("", 'width="1e308in" height="1"')), code("ASSET_PIXEL_LIMIT"));
});

test("BOM, XML declaration, comments, character entities and CDATA are canonicalized safely", () => {
  const source = '\uFEFF<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n' +
    '<!-- harmless <script> --><!---->' +
    svg('<title>A &amp; B &#x1F600;</title><text>x &lt; y <![CDATA[& < >]]></text>',
      'viewBox="0 0 20 10"');
  const info = admit(source);
  assert.ok(info.preview.includes("<title>A &amp; B 😀</title>"));
  assert.ok(info.preview.includes("<text>x &lt; y &amp; &lt; &gt;</text>"));
  assert.equal(info.preview.includes("script"), false);
  assert.equal(info.preview.includes("<?xml"), false);
  const encodedBytes = encodeURIComponent(info.preview).replace(/%[0-9A-F]{2}|./g, "x").length;
  assert.equal(info.previewByteLength, encodedBytes);
});

test("local references, gradients, xlink and static inline presentation survive the preview", () => {
  const source = svg(
    '<defs><path id="line" d="M0 0L2 2"/>' +
    '<linearGradient id="paint"><stop offset="0" stop-color="red"/>' +
    '<stop offset="1" stop-color="blue"/></linearGradient></defs>' +
    '<use xlink:href="&#35;line" transform="translate(1 2)"/>' +
    '<rect width="10" height="5" fill="url(&quot;#paint&quot;)" ' +
    'style="stroke : rgb(0, 0, 0); opacity: .5;"/>',
    'width="20" height="10" xmlns:xlink="http://www.w3.org/1999/xlink"');
  const info = admit(source);
  assert.ok(info.preview.includes('<use href="#line"'));
  assert.equal(info.preview.includes("xlink:"), false);
  assert.ok(info.preview.includes('fill="url(&quot;#paint&quot;)"'));
  assert.ok(info.preview.includes('style="stroke:rgb(0, 0, 0);opacity:.5"'));
});

test("active content, unknown namespaces and external resource paths never reach a preview", () => {
  for (const body of [
    "<script>alert(1)</script>", "<foreignObject/>", "<iframe/>", "<animate/>", "<set/>",
    '<image href="https://example.test/a.png"/>',
    '<image href="data:image/png;base64,AA=="/>',
    '<use href="https://example.test/x.svg#part"/>',
    '<use href="data:image/svg+xml,hello"/>',
    '<use href="javascript:alert(1)"/>',
    '<rect onload="alert(1)"/>',
    '<g xmlns="http://www.w3.org/1999/xhtml"/>',
    '<g xmlns:xlink="https://example.test/changed"/>',
    '<style>@import "https://example.test/a.css";</style>',
    '<rect fill="url(https://example.test/a.svg#x)"/>',
    '<rect style="fill:u\\72l(https://example.test/a.svg#x)"/>',
    '<rect style="fill:var(--paint)"/>',
    '<rect style="fill:red/*comment*/"/>',
    '<rect style="font-family:@downloaded"/>',
  ]) unsupported(svg(body));
  unsupported(svg("", 'width="1" height="1" xmlns="https://example.test/not-svg"'));
});

test("malformed XML, unbound references and invalid scalars fail explicitly", () => {
  for (const source of [
    '<svg width="1" height="1"><g></svg>',
    '<svg width="1" width="2" height="1"/>',
    '<svg width="1" height="1"/>tail',
    '<svg width="1" height="1"/><svg width="1" height="1"/>',
    svg("<!-- bad -- comment -->"), svg("<![CDATA[missing close"),
    svg("<text>&unknown;</text>"), svg("<text>&</text>"),
    svg("<text>&#0;</text>"), svg("<text>&#xD800;</text>"),
    svg("<text>\uD800</text>"), svg("<text>\u0000</text>"),
    svg('<rect width="unclosed/>'),
    '<?xml version="1.0" encoding="UTF-16"?>' + svg(),
  ]) invalid(source);
  for (const body of [
    '<use href="#missing"/>',
    '<use xlink:href="#missing"/>',
    '<path id="same"/><path id="same"/>',
    '<g id="same"><use href="#same"/></g>',
    '<g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g>',
    '<linearGradient id="g" href="#g"/>',
    '<use href="#bad%20id"/>',
  ]) unsupported(svg(body));
  unsupported('<!DOCTYPE svg [<!ENTITY test "payload">]>' + svg("<text>&test;</text>"));
  invalid('<?xml-stylesheet href="https://example.test/a.css"?>' + svg());
  unsupported('<?processing href="https://example.test/a.css"?>' + svg());
});

test("source UTF-8 bytes and canonical expansion have independent bounds", () => {
  for (const source of [
    "x".repeat(2 * 1024 * 1024 + 1),
    svg("<text>" + "猫".repeat(700000) + "</text>"),
    svg("<text><![CDATA[" + "<".repeat(600000) + "]]></text>"),
  ]) assert.throws(() => admit(source), code("ASSET_BYTE_LIMIT"));
});

test("tree, nesting and reference counts are rejected before browser decoding", () => {
  unsupported(svg("<g/>".repeat(8192)));
  assert.equal(admit(svg("<g>".repeat(60) + "</g>".repeat(60))).width, 20);
  unsupported(svg("<g>".repeat(64) + "</g>".repeat(64)));
  const source = svg('<defs><path id="shape" d="M0 0L1 1"/></defs>' +
    '<use href="#shape"/>'.repeat(4097));
  assert.throws(() => admit(source), error =>
    error.code === "UNSUPPORTED_IMAGE" && error.message.includes("4096-reference"));
});

test("acyclic local references still obey expanded node and depth budgets", () => {
  const doubling = levels => {
    let body = '<g id="n0"><rect width="1" height="1"/></g>';
    for (let i = 1; i <= levels; i++)
      body += '<g id="n' + i + '"><use href="#n' + (i - 1) +
        '"/><use href="#n' + (i - 1) + '"/></g>';
    return svg("<defs>" + body + '</defs><use href="#n' + levels + '"/>');
  };
  assert.equal(admit(doubling(5)).width, 20);
  unsupported(doubling(16));
  let chain = '<g id="n0"><path/></g>';
  for (let i = 1; i <= 32; i++)
    chain += '<g id="n' + i + '"><use href="#n' + (i - 1) + '"/></g>';
  unsupported(svg("<defs>" + chain + '</defs><use href="#n32"/>'));
});

test("reused text and path payloads cannot multiply beyond the expanded content budget", () => {
  const text = svg('<defs><g id="big"><text>' + "a".repeat(1024 * 1024) +
    '</text></g></defs>' + '<use href="#big"/>'.repeat(100));
  const path = svg('<defs><path id="big" d="M0 0 ' + "L1 1 ".repeat(13000) +
    '"/></defs>' + '<use href="#big"/>'.repeat(128));
  for (const source of [text, path]) {
    assert.throws(() => admit(source), error =>
      error.code === "UNSUPPORTED_IMAGE" && error.message.includes("expanded content"));
  }
  assert.equal(admit(svg('<defs><text id="small">Label</text></defs>' +
    '<use href="#small"/>'.repeat(10))).width, 20);
});

test("implicit marker and pattern fanout is refused even when the explicit graph is small", () => {
  const marker = svg('<defs><marker id="m"><text>' + "a".repeat(512 * 1024) +
    '</text></marker></defs><path d="M0 0 ' + "L1 1 ".repeat(20000) +
    '" marker-mid="url(#m)"/>');
  unsupported(marker);
  unsupported(svg('<defs><pattern id="p" width="1" height="1"><rect/></pattern></defs>' +
    '<rect width="20" height="10" fill="url(#p)"/>'));
  unsupported(svg('<path marker-start="url(#m)"/>'));
});

test("long whitespace, function-free identifiers and malformed suffixes retain bounded admission", () => {
  const whitespace = " ".repeat(64 * 1024);
  assert.equal(admit(svg(whitespace)).width, 20);
  invalid(svg("", 'viewBox="0' + whitespace + 'x"'));
  invalid(svg("<text>" + "&".repeat(64 * 1024) + "</text>"));
  for (const value of ["M".repeat(64 * 1024), "rgb(0,0,0) " + "M".repeat(64 * 1024)]) {
    const info = admit(svg('<g class="' + value + '"/>'));
    assert.ok(info.preview.includes(value));
  }
});
