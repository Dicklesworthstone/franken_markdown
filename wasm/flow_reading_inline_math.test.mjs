import test from "node:test";
import assert from "node:assert/strict";
import { readFlowDocument } from "./flow_reading.mjs";
import { node, ReadingSession } from "./tests/flow_reading_fixtures.mjs";

const style = extra => ({ bold: false, italic: false, code: false, strikethrough: false, ...extra });
const run = (startByte, endByte, math, extra = {}) => ({ startByte, endByte, style: style(), link: null,
  ...(math === undefined ? {} : { math }), ...extra });
const session = (text, runs) => new ReadingSession([node("paragraph", text, { inlineRuns: runs })]);
const bytes = value => new TextEncoder().encode(value).length;

test("inline and display-style formula ranges retain exact TeX within continuous logical text", async () => {
  const prefix = "Before α ", tex = "\\frac{😀}{x}", between = " then ", display = "\\sum_{i=1}^{n} i", suffix = " after.";
  const value = prefix + tex + between + display + suffix;
  const first = bytes(prefix), second = bytes(prefix + tex + between);
  const raw = [run(first, first + bytes(tex), "inline", { style: style({ bold: true }) }),
    run(second, second + bytes(display), "display", { style: style({ italic: true }),
      link: { target: "/guide", activeTarget: "/guide" } })];
  const source = session(value, raw), document = await readFlowDocument(source);
  assert.equal(document.text, value);
  assert.deepEqual(document.nodes[0].inlineRuns.map(value => value.math), ["inline", "display"]);
  assert.equal(document.nodes[0].inlineRuns[0].startUtf16, prefix.length);
  assert.equal(document.nodes[0].inlineRuns[0].endUtf16, prefix.length + tex.length);
  assert.equal(document.matchText(document.find(tex + between + display).matches[0]), tex + between + display);
  assert.equal(document.nodes[0].inlineRuns[1].link.activeTarget, "/guide");
  raw[0].math = "display"; assert.equal(document.nodes[0].inlineRuns[0].math, "inline");
  assert(Object.isFrozen(document.nodes[0].inlineRuns[0]));
});

test("ordinary omitted metadata stays omitted and explicit null remains ordinary text", async () => {
  const document = await readFlowDocument(session("abc", [run(0, 1), run(1, 2, null)]));
  assert(!Object.hasOwn(document.nodes[0].inlineRuns[0], "math"));
  assert.equal(document.nodes[0].inlineRuns[1].math, null);
  assert.equal(document.text, "abc");
});

for (const value of ["", "text", "Math", "display\u0000", true, false, 0, 1, {}, [], ["inline"]])
  test(`invalid inline math marker ${JSON.stringify(value)} rejects the complete reading snapshot`, async () => {
    await assert.rejects(readFlowDocument(session("x", [run(0, 1, value)])), { code: "INVALID_READING_DATA" });
  });

test("formula metadata cannot split a Unicode scalar or escape the leaf's text", async () => {
  for (const range of [[1, 2], [2, 4], [0, 7], [0, 0]]) {
    await assert.rejects(readFlowDocument(session("é😀", [run(...range, "inline")])), { code: "INVALID_READING_DATA" });
  }
});

test("math annotations do not relax safe link activation", async () => {
  await assert.rejects(readFlowDocument(session("x", [run(0, 1, "inline", {
    link: { target: "javascript:alert(1)", activeTarget: "javascript:alert(1)" },
  })])), { code: "INVALID_READING_DATA" });
  const document = await readFlowDocument(session("x", [run(0, 1, "inline", {
    link: { target: "javascript:alert(1)", activeTarget: null },
  })]));
  assert.equal(document.nodes[0].inlineRuns[0].link.activeTarget, null);
  assert.equal(document.nodes[0].inlineRuns[0].math, "inline");
});

test("math markers cannot inject nested semantics into a block-math or code leaf", async () => {
  for (const role of ["math", "code-block", "image", "thematic-break"]) {
    await assert.rejects(readFlowDocument(new ReadingSession([node(role, "x", { inlineRuns: [run(0, 1, "inline")] })])),
      { code: "INVALID_READING_DATA" });
  }
});

test("inline math uses existing text/run budgets and stale source fences", async () => {
  await assert.rejects(readFlowDocument(session("xy", [run(0, 1, "inline"), run(1, 2, "inline")]),
    { limits: { maxInlineRuns: 1 } }), { code: "READING_LIMIT" });
  const source = session("x", [run(0, 1, "inline")]), document = await readFlowDocument(source);
  source.edit(); assert.throws(() => document.find("x"), { code: "STALE_REVISION" });
});

test("math identity is captured before cooperative link validation yields", async () => {
  const target = "#" + "x".repeat(80000), raw = run(0, 1, "inline", { link: { target, activeTarget: target } });
  const pending = readFlowDocument(session("x", [raw]));
  const timer = setTimeout(() => { raw.math = false; }, 0);
  let document;
  try { document = await pending; } finally { clearTimeout(timer); }
  assert.equal(raw.math, false); assert.equal(document.nodes[0].inlineRuns[0].math, "inline");
});
