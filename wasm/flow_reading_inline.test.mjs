import assert from "node:assert/strict";
import test from "node:test";
import { readFlowDocument } from "./flow_reading.mjs";

const style = (value = {}) => ({
  bold: false,
  italic: false,
  code: false,
  strikethrough: false,
  ...value,
});
const run = (startByte, endByte, flags = {}, link = null) => ({
  startByte,
  endByte,
  style: style(flags),
  link,
});
const link = (target, activeTarget = target) => ({ target, activeTarget });
const leaf = (text, inlineRuns = [], extra = {}) => ({
  role: "paragraph",
  text,
  inlineRuns,
  bounds: { x: 0, y: 0, width: 100, height: 20 },
  enclosingSourceSpan: { startByte: 99, endByte: 199 },
  children: [],
  ...extra,
});
const sessionFor = (nodes) => ({
  disposed: false,
  token: { revision: "1", layoutRevision: "1" },
  readingOrder({ offset, limit }) {
    return {
      schemaVersion: 1,
      ...this.token,
      offset,
      total: nodes.length,
      nextOffset: offset + limit < nodes.length ? offset + limit : null,
      nodes: nodes.slice(offset, offset + limit),
    };
  },
});
const read = (nodes, options) => readFlowDocument(sessionFor(nodes), options);
const rejects = (nodes, code = "INVALID_READING_DATA", options) =>
  assert.rejects(read(nodes, options), { code });

test("Unicode reading ranges convert exactly, independently of enclosing source spans", async () => {
  const source = leaf("A é😀z", [
    run(0, 2),
    run(2, 4, { bold: true }),
    run(4, 8, { italic: true, code: true }),
    run(8, 9),
  ]);
  const document = await read([source]),
    node = document.nodes[0];
  assert.deepEqual(
    node.inlineRuns.map((r) => [r.startUtf16, r.endUtf16]),
    [
      [0, 2],
      [2, 3],
      [3, 5],
      [5, 6],
    ],
  );
  assert.equal(document.matchText(document.find("é😀z").matches[0]), "é😀z");
  assert.equal(document.text, "A é😀z");
  assert.equal(document.inlineRunCount, 4);
  assert.equal(node.enclosingSourceSpan.startByte, 99);
  source.inlineRuns[1].style.bold = false;
  assert.equal(node.inlineRuns[1].style.bold, true);
  assert.ok(
    Object.isFrozen(node.inlineRuns) &&
      Object.isFrozen(node.inlineRuns[1]) &&
      Object.isFrozen(node.inlineRuns[1].style),
  );
});

test("task markers may be unstyled gaps, and table cells use their own reading coordinates", async () => {
  const task = leaf("[x] é", [run(4, 6, { bold: true })], { role: "list-item" });
  const cell = leaf("cell", [run(0, 4, { code: true }, link("#h"))], { role: "table-cell" });
  const table = leaf("cell", [], { role: "table-row", children: [cell] });
  const document = await read([task, table]);
  assert.equal(document.nodes[0].inlineRuns[0].startUtf16, 4);
  assert.equal(document.nodes[2].inlineRuns[0].startUtf16, 0);
  assert.equal(document.nodes[2].inlineRuns[0].link.activeTarget, "#h");
  assert.equal(document.find("cell").matches.length, 1);
});

test("safe links, intentionally inert links, and blocked targets are retained immutably", async () => {
  const nodes = [
    "#part",
    "../guide.md",
    "https://example.com",
    "HTTP://example.com",
    "mailto:a@b",
    "//example.com/x",
    "/a:b",
  ].map((target) => leaf("x", [run(0, 1, {}, link(target))]));
  nodes.push(leaf("bad", [run(0, 3, {}, link("javascript:bad", null))]));
  nodes.push(leaf("inert", [run(0, 5, {}, link("https://example.com", null))]));
  nodes.push(
    leaf("trim", [run(0, 4, {}, link("\u2003 https://example.com \u00a0", "https://example.com"))]),
  );
  const document = await read(nodes);
  assert.equal(document.nodes[7].inlineRuns[0].link.target, "javascript:bad");
  assert.equal(document.nodes[7].inlineRuns[0].link.activeTarget, null);
  assert.equal(document.nodes[8].inlineRuns[0].link.activeTarget, null);
  assert.ok(Object.isFrozen(document.nodes[0].inlineRuns[0].link));
});

test("claimed activation cannot authorize schemes, controls, or a substituted target", async () => {
  for (const target of [
    "javascript:x",
    "data:x",
    "file:///x",
    "vbscript:x",
    "java\tscript:x",
    "https:\\evil",
    "x\u007fy",
    "x\u009fy",
    "httpſ:x",
    "",
  ]) {
    await rejects([leaf("x", [run(0, 1, {}, link(target))])]);
  }
  await rejects([leaf("x", [run(0, 1, {}, link("#good", "https://different.example"))])]);
  await rejects([leaf("x", [run(0, 1, {}, link("bad\ud800", null))])]);
});

test("ranges reject splits, bounds, overlaps, inversions, empties, and missing styles or links", async () => {
  for (const runs of [
    [run(1, 2)],
    [run(0, 3)],
    [run(2, 7)],
    [run(0, 2), run(1, 6)],
    [run(3, 2)],
    [run(0, 0)],
    [run(-1, 2)],
    [run(0.5, 2)],
    [run(0, Infinity)],
    [{ startByte: 0, endByte: 2 }],
    [{ ...run(0, 2), link: undefined }],
    [run(0, 2, { bold: "yes" })],
    [run(0, 2, { color: "red" })],
  ]) {
    await rejects([leaf("é😀", runs)]);
  }
  await rejects([leaf("é", null)]);
  await rejects([leaf("é", new Array(1))]);
});

test("formatting does not escape leaves or attach image links to prose", async () => {
  await rejects([leaf("code", [run(0, 4)], { role: "code-block" })]);
  await rejects([leaf("image", [run(0, 5)], { role: "image" })]);
  await rejects([leaf("text", [run(0, 4)], { role: "document", children: [leaf("child")] })]);
  await rejects([leaf("text", [], { imageLink: link("#x") })]);
  const document = await read([leaf("", [], { role: "image", imageLink: link("#image") })]);
  assert.equal(document.nodes[0].imageLink.activeTarget, "#image");
  assert.equal(document.text, "");
});

test("all page metadata counts toward independently lowerable retention budgets", async () => {
  const nodes = Array.from({ length: 257 }, () => leaf("x", [run(0, 1)]));
  await rejects(nodes, "READING_LIMIT", { limits: { maxInlineRuns: 256 } });
  const document = await read(nodes, { limits: { maxInlineRuns: 257 } });
  assert.equal(document.inlineRunCount, 257);
  await rejects([leaf("x", [run(0, 1, {}, link("#x"))])], "READING_LIMIT", {
    limits: { maxLinkUnits: 3 },
  });
  assert.equal(
    (await read([leaf("x", [run(0, 1, {}, link("#x"))])], { limits: { maxLinkUnits: 4 } }))
      .linkUnits,
    4,
  );
  await rejects([leaf("", [], { role: "image", imageLink: link("abc", null) })], "READING_LIMIT", {
    limits: { maxLinkUnits: 2 },
  });
  await rejects([], "INVALID_OPTIONS", { limits: { maxInlineRuns: 50001 } });
  await rejects([], "INVALID_OPTIONS", { limits: { maxLinkUnits: 1048577 } });
});

test("legacy plain pages remain accepted without inventing formatting or links", async () => {
  const old = leaf("legacy");
  delete old.inlineRuns;
  const document = await read([old]);
  assert.deepEqual(document.nodes[0].inlineRuns, []);
  assert.equal(document.nodes[0].imageLink, null);
  assert.equal(document.inlineRunCount, 0);
  assert.equal(document.linkUnits, 0);
});

test("formatting never weakens source/layout identity fences", async () => {
  const session = sessionFor([leaf("one two", [run(0, 3, { bold: true }), run(3, 7)])]);
  const document = await readFlowDocument(session),
    match = document.find("one two").matches[0];
  assert.equal(document.matchText(match), "one two");
  session.token = { revision: "1", layoutRevision: "2" };
  assert.throws(() => document.matchText(match), { code: "STALE_LAYOUT" });
  assert.throws(() => document.locate(0), { code: "STALE_LAYOUT" });
});

test("encoded dangerous destinations remain inert for prose and linked images", async () => {
  for (const target of [
    "%6a%61vascript%3Aalert(1)",
    "java%09script:alert(1)",
    "javascript&colon;alert(1)",
    "javascript&#58;alert(1)",
    "javascript&#x3a;alert(1)",
    "javascript&#58alert(1)",
    "&#106;avascript:alert(1)",
    "java&Tab;script:alert(1)",
    "java&NewLine;script:alert(1)",
    "%256aavascript%253aalert(1)",
    "javascript&amp;colon;alert(1)",
    "data%3atext/html,payload",
    "https:%5c%5cevil.example",
    "https://example.com/%0d%0aheader",
  ]) {
    const prose = active => leaf("visible label", [run(0, 13, {}, link(target, active))]);
    const image = active => leaf("visible alt", [], { role: "image", imageLink: link(target, active) });
    await rejects([prose(target)]);
    await rejects([image(target)]);
    const document = await read([prose(null), image(null)]);
    assert.equal(document.text, "visible label\n\nvisible alt");
    assert.deepEqual(document.nodes[0].inlineRuns[0].link, link(target, null));
    assert.deepEqual(document.nodes[1].imageLink, link(target, null));
  }
});

test("trimming cannot hide edge controls in a claimed activation", async () => {
  for (const target of [
    "\thttps://example.com", "https://example.com\n", "\u0085 https://example.com \n",
    "\u001f#part", "#part\u009f", "&#9;https://example.com", "%0ahttps://example.com",
  ]) {
    await rejects([leaf("x", [run(0, 1, {}, link(target, target.trim()))])]);
    const document = await read([leaf("x", [run(0, 1, {}, link(target, null))])]);
    assert.equal(document.nodes[0].inlineRuns[0].link.target, target);
  }
});

test("safe activation keeps URL encoding, Unicode and relative-reference semantics", async () => {
  for (const target of [
    "https://example.com/a%20b?q=a%26b#part", "mailto:a@example.com",
    "tel:+1-555-0100", "TEL:+1-555-0100", "../résumé%20final.md",
    "/path/a:b", "./a:b", "?next=javascript:literal", "#javascript:literal",
    "https://example.com/%F0%9F%98%80", "//example.com/path",
    "https://example.com/?x=1&amp;y=2", "#日本語",
  ]) {
    const document = await read([
      leaf("x", [run(0, 1, {}, link(target))]),
      leaf("alt", [], { role: "image", imageLink: link(target) }),
    ]);
    assert.equal(document.nodes[0].inlineRuns[0].link.activeTarget, target);
    assert.equal(document.nodes[1].imageLink.activeTarget, target);
  }
});

test("excessive encoding depth is blocked without changing the retained target", async () => {
  let target = "javascript:alert(1)";
  for (let i = 0; i < 16; i++) target = target.replaceAll("%", "%25").replaceAll(":", "%3a");
  await rejects([leaf("x", [run(0, 1, {}, link(target))])]);
  const document = await read([leaf("x", [run(0, 1, {}, link(target, null))])]);
  assert.equal(document.nodes[0].inlineRuns[0].link.target, target);
});

test("large encoded destinations remain cancellable without publishing a reading snapshot", async () => {
  const target = "https://example.com/?q=" + "%41".repeat(40000);
  const session = sessionFor([leaf("x", [run(0, 1, {}, link(target))])]);
  const controller = new AbortController();
  const pending = readFlowDocument(session, { signal: controller.signal });
  const timer = setTimeout(() => controller.abort(), 0);
  try {
    await assert.rejects(pending, { code: "ABORTED" });
    assert.equal(session.disposed, false);
    assert.deepEqual(session.token, { revision: "1", layoutRevision: "1" });
  } finally {
    clearTimeout(timer);
  }
});
