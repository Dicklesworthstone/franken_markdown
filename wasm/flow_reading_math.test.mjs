import test from "node:test";
import assert from "node:assert/strict";
import { readFlowDocument } from "./flow_reading.mjs";
import { node, ReadingSession } from "./tests/flow_reading_fixtures.mjs";

test("math reading leaves retain exact TeX whitespace, Unicode and enclosing source for copying", async () => {
  const tex = "  \\frac{α + 😀}{\\sqrt{x}}\n\\text{<img src=x>}  ";
  const raw = node("math", tex, { enclosingSourceSpan: { startByte: 11, endByte: 100 }, inlineRuns: [] });
  const s = new ReadingSession([raw]), doc = await readFlowDocument(s);
  assert.equal(doc.text, tex); assert.equal(doc.nodes[0].text, tex); assert.equal(doc.nodes[0].role, "math");
  assert.equal(doc.matchText(doc.find("α + 😀").matches[0]), "α + 😀");
  assert.deepEqual(doc.locate(0).enclosingSourceSpan, { startByte: 11, endByte: 100 });
  assert.deepEqual(doc.nodes[0].inlineRuns, []); assert.equal(doc.nodes[0].imageLink, null);
  raw.text = "reused provider buffer"; assert.equal(doc.text, tex);
  s.edit(); assert.throws(() => doc.locate(0), { code: "STALE_REVISION" });
});

for (const [name, extra] of [
  ["children", { children: [node()] }],
  ["inline metadata", { inlineRuns: [{ startByte: 0, endByte: 1,
    style: { bold: false, italic: true, code: false, strikethrough: false }, link: null }] }],
  ["image link", { imageLink: { target: "https://example.com/", activeTarget: "https://example.com/" } }],
]) test(`math leaves refuse ${name} instead of inventing parser semantics`, async () => {
  await assert.rejects(readFlowDocument(new ReadingSession([node("math", "x", extra)])), { code: "INVALID_READING_DATA" });
});

test("math retains reading text and node limits", async () => {
  await assert.rejects(readFlowDocument(new ReadingSession([node("math", "x".repeat(33))]),
    { limits: { maxTextUnits: 32 } }), { code: "READING_LIMIT" });
  await assert.rejects(readFlowDocument(new ReadingSession([node("math", "x"), node("math", "y")]),
    { limits: { maxNodes: 1 } }), { code: "READING_LIMIT" });
});

test("math fallback text is not parsed as HTML, links or nested mathematics", async () => {
  const tex = '<a href="https://example.com/">$not reinterpreted$</a> \\unsupported{raw}';
  const doc = await readFlowDocument(new ReadingSession([node("math", tex)]));
  assert.equal(doc.nodes[0].text, tex); assert.deepEqual(doc.nodes[0].children, []);
  assert.equal(doc.inlineRunCount, 0); assert.equal(doc.find("$not reinterpreted$").matches.length, 1);
});
