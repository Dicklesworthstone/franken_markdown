import test from "node:test";
import assert from "node:assert/strict";
import { readFlowDocument } from "./flow_reading.mjs";
import { node, ReadingSession } from "./tests/flow_reading_fixtures.mjs";

const source = 'sequenceDiagram\n    Alice->>Bob: <img src=x onerror=alert(1)>\n    Note over Bob: é 😀  \n';
test("diagram reading leaves preserve raw source, Unicode, indentation and terminal LF for Copy", async () => {
  const raw = node("diagram", source, { enclosingSourceSpan: { startByte: 11, endByte: 200 }, inlineRuns: [] });
  const session = new ReadingSession([raw]), doc = await readFlowDocument(session);
  assert.equal(doc.text, source); assert.equal(doc.nodes[0].text, source);
  assert.equal(doc.nodes[0].role, "diagram"); assert.deepEqual(doc.nodes[0].children, []);
  assert.equal(doc.matchText(doc.find(source).matches[0]), source);
  assert.equal(doc.matchText(doc.find("é 😀  \n").matches[0]), "é 😀  \n");
  assert.deepEqual(doc.locate(0).enclosingSourceSpan, { startByte: 11, endByte: 200 });
  assert.deepEqual(doc.nodes[0].inlineRuns, []); assert.equal(doc.nodes[0].imageLink, null);
  raw.text = "mutated provider"; assert.equal(doc.text, source);
  session.reflow(); assert.throws(() => doc.find("Alice"), { code: "STALE_LAYOUT" });
  session.edit(); assert.throws(() => doc.locate(0), { code: "STALE_REVISION" });
});

for (const [name, extra] of [
  ["children", { children: [node()] }],
  ["inline markup", { inlineRuns: [{ startByte: 0, endByte: 1, style: { bold: true, italic: false, code: false, strikethrough: false }, link: null }] }],
  ["image link", { imageLink: { target: "https://example.com/", activeTarget: "https://example.com/" } }],
  ["heading anchor", { anchorId: "invented" }],
]) test(`diagram transcript refuses ${name} instead of inventing diagram semantics`, async () => {
  await assert.rejects(readFlowDocument(new ReadingSession([node("diagram", "x", extra)])), { code: "INVALID_READING_DATA" });
});

test("diagram leaves retain all text, node and Unicode admission bounds", async () => {
  await assert.rejects(readFlowDocument(new ReadingSession([node("diagram", "x".repeat(33))]), { limits: { maxTextUnits: 32 } }), { code: "READING_LIMIT" });
  await assert.rejects(readFlowDocument(new ReadingSession([node("diagram", "x"), node("diagram", "y")]), { limits: { maxNodes: 1 } }), { code: "READING_LIMIT" });
  await assert.rejects(readFlowDocument(new ReadingSession([node("diagram", "bad\ud800")])), { code: "INVALID_READING_DATA" });
});

test("diagram source remains literal when it contains links, HTML and mathematical delimiters", async () => {
  const literal = '<a href="https://example.com/">$not math$</a>\nclick A callback\n';
  const doc = await readFlowDocument(new ReadingSession([node("diagram", literal)]));
  assert.equal(doc.text, literal); assert.equal(doc.inlineRunCount, 0);
  assert.equal(doc.find("$not math$").matches.length, 1); assert.equal(doc.nodes[0].imageLink, null);
});
