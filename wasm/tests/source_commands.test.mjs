import assert from "node:assert/strict";
import test from "node:test";
import { planSourceCommand, SourceCommandError } from "../demo/source_commands.mjs";
const apply = (source, command, start = 0, end = source.length, direction = "none") => {
  const plan = planSourceCommand(source, { start, end, direction }, command);
  const result = source.slice(0, plan.start) + plan.text + source.slice(plan.end);
  assert(Object.isFrozen(plan) && Object.isFrozen(plan.selection));
  assert(plan.selection.start >= 0 && plan.selection.end <= result.length && plan.selection.start <= plan.selection.end);
  assert.equal(result.slice(0, plan.start), source.slice(0, plan.start));
  assert.equal(result.slice(plan.start + plan.text.length), source.slice(plan.end));
  return { result, selected: result.slice(plan.selection.start, plan.selection.end), ...plan };
};
for (const [command, marker] of [["bold", "**"], ["italic", "*"], ["strike", "~~"]]) {
  test(`${command} wraps content without consuming its whitespace`, () => {
    const r = apply("left  café😀  right", command, 4, 14, "backward");
    assert.equal(r.result, `left  ${marker}café😀${marker}  right`);
    assert.equal(r.selected, "café😀");
    assert.equal(r.selection.direction, "backward");
  });
  test(`${command} reverses adjacent markers and full selected markers`, () => {
    const r = apply("word", command);
    assert.equal(apply(r.result, command, r.selection.start, r.selection.end).result, "word");
    assert.equal(apply(`${marker}word${marker}`, command).result, "word");
  });
  test(`${command} gives an empty caret a selected placeholder`, () => {
    const r = apply("ab", command, 1, 1);
    assert.equal(r.result, `a${marker}text${marker}b`);
    assert.equal(r.selected, "text");
  });
}
for (const [content, expected] of [["word", "`word`"], ["a`b", "``a`b``"], ["`a`", "`` `a` ``"],
  ["  a  ", "`   a   `"], ["   ", "`   `"], ["`````", "`````` ````` ``````"]]) {
  test(`inline code preserves ${JSON.stringify(content)}`, () => {
    const r = apply(content, "inline-code");
    assert.equal(r.result, expected);
    assert.equal(r.selected, content);
  });
}
test("inline code placeholder", () => assert.equal(apply("", "inline-code").selected, "code"));
test("inline commands refuse multiline selections without changing input", () => {
  for (const command of ["bold", "italic", "strike", "inline-code"])
    assert.throws(() => apply("a\nb", command), { code: "MULTILINE_SELECTION" });
});
test("heading commands select complete lines, replace ATX prefixes, and toggle", () => {
  const r = apply("first\n  ### café\nlast", "heading-2", 11, 12);
  assert.equal(r.result, "first\n  ## café\nlast");
  assert.equal(r.selected, "  ## café");
  assert.equal(apply(r.result, "heading-2", r.selection.start, r.selection.end).result, "first\n  café\nlast");
});
test("a selection ending at the next line start leaves that line alone", () => {
  for (const eol of ["\n", "\r\n", "\r"]) {
    const source = `one${eol}two${eol}three`;
    const r = apply(source, "bullet-list", 0, 3 + eol.length);
    assert.equal(r.result, `- one${eol}two${eol}three`);
  }
});
test("line commands preserve mixed separators and indentation", () => {
  const r = apply("a\r\n  b\rc\n\td", "ordered-list");
  assert.equal(r.result, "1. a\r\n  2. b\r3. c\n\t4. d");
});
test("list conversion replaces markers and task states only when requested", () => {
  const source = "  - a\n  9) b\n  + [x] c";
  assert.equal(apply(source, "ordered-list").result, "  1. a\n  2. b\n  3. c");
  assert.equal(apply(source, "task-list").result, "  - [ ] a\n  - [ ] b\n  - [ ] c");
  assert.equal(apply("- [x] a\n* [ ] b", "task-list").result, "a\nb");
});
test("blank selected lines stay blank and do not consume ordered counters", () => {
  assert.equal(apply("a\n  \nb", "ordered-list").result, "1. a\n  \n2. b");
  assert.equal(apply("\n\n", "heading-1").result, "\n\n");
});
test("empty current line begins the requested block", () => {
  for (const [command, marker] of [["heading-3", "### "], ["ordered-list", "1. "], ["task-list", "- [ ] "], ["blockquote", "> "]]) {
    const r = apply("before\n\nafter", command, 7, 7);
    assert.equal(r.result, `before\n${marker}\nafter`);
    assert.equal(r.selection.start, 7 + marker.length);
    assert.equal(r.selection.end, r.selection.start);
  }
});
test("collapsed caret tracks prefix insertion, replacement, and removal", () => {
  for (const [source, command, start, expected, pos] of [["abc", "heading-1", 2, "# abc", 4],
    ["  #### abc", "heading-2", 9, "  ## abc", 7], ["- abc", "bullet-list", 4, "abc", 2]]) {
    const r = apply(source, command, start, start);
    assert.equal(r.result, expected); assert.equal(r.selection.start, pos); assert.equal(r.selection.end, pos);
  }
});
test("quote toggling changes one level without mining the rest of the source", () => {
  assert.equal(apply("> > a\n> b", "blockquote").result, "> a\nb");
  assert.equal(apply("a\n> b", "blockquote").result, "> a\n> b");
});
test("code fence exceeds every literal backtick run and preserves the selected body", () => {
  const r = apply("before\nalpha ``` beta\n````\nafter", "code-block", 8, 25);
  assert.equal(r.result, "before\n`````\nalpha ``` beta\n````\n`````\nafter");
  assert.equal(r.selected, "alpha ``` beta\n````");
});
test("code block retains CRLF and an empty code body", () => {
  assert.equal(apply("a\r\nb", "code-block").result, "```\r\na\r\nb\r\n```");
  assert.equal(apply("", "code-block").result, "```\n\n```");
});
test("unknown commands, malformed selections, scalar interiors and CRLF interiors are rejected", () => {
  assert.throws(() => apply("a", "__proto__"), SourceCommandError);
  for (const sel of [null, {}, {start: -1, end: 0}, {start: 2, end: 1}, {start:0,end:99},
    {start:NaN,end:1}, {start:0,end:1,direction:"wrong"}, {start:1,end:1}])
    assert.throws(() => planSourceCommand("😀\r\nx", sel, "bold"), {code:"INVALID_SELECTION"});
  assert.throws(() => planSourceCommand("x\r\ny", {start:2,end:2}, "bold"), {code:"INVALID_SELECTION"});
  assert.throws(() => apply("\ud800", "bold"), {code:"INVALID_UNICODE"});
  assert.throws(() => apply("\udc00", "bold"), {code:"INVALID_UNICODE"});
});
test("source and resulting bytes, not UTF-16 length alone, are bounded", () => {
  assert.throws(() => apply("x".repeat(4 * 1024 * 1024 + 1), "bold"), {code:"BUDGET_EXCEEDED"});
  assert.throws(() => apply("x".repeat(4 * 1024 * 1024), "bold", 0, 1), {code:"BUDGET_EXCEEDED"});
  const huge = "😀".repeat(1024 * 1024);
  assert.throws(() => apply(huge, "bold", 0, 2), {code:"BUDGET_EXCEEDED"});
});
test("line work is bounded before a bulk transformation", () => {
  assert.throws(() => apply("a\n".repeat(10001), "bullet-list"), {code:"FORMATTING_LIMIT"});
  assert.equal(apply("a\n".repeat(10000), "bullet-list").result, "- a\n".repeat(10000));
});
test("512 generated list transformations round-trip exact source and separators", () => {
  for (let seed = 0; seed < 512; seed++) {
    const source = Array.from({length:seed % 13 + 1}, (_, i) => `${" ".repeat((seed + i) % 4)}word${seed}😀${i}`)
      .join(["\n", "\r\n", "\r"][seed % 3]);
    for (const command of ["bullet-list", "task-list", "ordered-list", "heading-2", "blockquote"]) {
      const r = apply(source, command);
      assert.equal(apply(r.result, command, r.selection.start, r.selection.end).result, source);
    }
  }
});
