import assert from "node:assert/strict";
import test from "node:test";
import { planSourceCommand } from "../demo/source_commands.mjs";

const BOM = "\ufeff";
const TICK = String.fromCharCode(96);
const apply = (source, command, start = 0, end = source.length, direction = "none") => {
  const plan = planSourceCommand(source, { start, end, direction }, command);
  const result = source.slice(0, plan.start) + plan.text + source.slice(plan.end);
  assert.equal(result.slice(0, plan.start), source.slice(0, plan.start));
  assert.equal(result.slice(plan.start + plan.text.length), source.slice(plan.end));
  assert(plan.selection.start >= 0 && plan.selection.end <= result.length);
  assert(plan.selection.start <= plan.selection.end);
  return { result, selected: result.slice(plan.selection.start, plan.selection.end), ...plan };
};

test("heading replacement and toggling keep a leading BOM outside Markdown", () => {
  for (const eol of ["\n", "\r\n", "\r"]) {
    const source = BOM + "# café😀" + eol + "untouched";
    for (const [start, end] of [[0, source.indexOf(eol)], [5, 5]]) {
      const changed = apply(source, "heading-2", start, end);
      assert.equal(changed.result, BOM + "## café😀" + eol + "untouched");
      const removed = apply(changed.result, "heading-2", changed.selection.start, changed.selection.end);
      assert.equal(removed.result, BOM + "café😀" + eol + "untouched");
    }
  }
});

test("first-line lists and quotes recognize existing markers after the BOM", () => {
  for (const [command, prefix] of [
    ["bullet-list", "- "], ["ordered-list", "9) "],
    ["task-list", "+ [x] "], ["blockquote", "> "],
  ]) {
    const source = BOM + "  " + prefix + "café😀\r\nuntouched";
    const changed = apply(source, command, 0, source.indexOf("\r"));
    assert.equal(changed.result, BOM + "  café😀\r\nuntouched");
    assert.equal(changed.selected, "  café😀");
  }
});

test("Select All inline formatting leaves the encoding marker at byte zero", () => {
  for (const [command, prefix, suffix] of [
    ["bold", "**", "**"], ["italic", "*", "*"],
    ["strike", "~~", "~~"], ["inline-code", TICK, TICK],
  ]) {
    const changed = apply(BOM + "café😀", command, 0, (BOM + "café😀").length, "backward");
    assert.equal(changed.result, BOM + prefix + "café😀" + suffix);
    assert.equal(changed.selected, "café😀");
    assert.equal(changed.selection.direction, "backward");
    if (command !== "inline-code") {
      assert.equal(apply(changed.result, command).result, BOM + "café😀");
      assert.equal(apply(changed.result, command, changed.selection.start, changed.selection.end).result, BOM + "café😀");
    }
  }
});

test("code fences keep the BOM outside the literal code body and preserve CRLF", () => {
  const body = "café😀 " + TICK.repeat(3) + " literal";
  const source = BOM + body + "\r\nnext";
  const changed = apply(source, "code-block", 0, source.indexOf("\r") + 2);
  assert.equal(changed.result, BOM + TICK.repeat(4) + "\r\n" + body + "\r\n" + TICK.repeat(4) + "\r\nnext");
  assert.equal(changed.selected, body);
});

test("a caret before the BOM or a selection of only the BOM edits after it", () => {
  for (const [command, text, selected] of [
    ["bold", "**text**", "text"], ["italic", "*text*", "text"],
    ["strike", "~~text~~", "text"], ["inline-code", TICK + "code" + TICK, "code"],
    ["code-block", TICK.repeat(3) + "\n\n" + TICK.repeat(3), ""],
    ["heading-1", "# ", ""], ["bullet-list", "- ", ""],
    ["ordered-list", "1. ", ""], ["task-list", "- [ ] ", ""], ["blockquote", "> ", ""],
  ]) {
    for (const [start, end] of [[0, 0], [0, 1], [1, 1]]) {
      const changed = apply(BOM, command, start, end);
      assert.equal(changed.result, BOM + text);
      assert.equal(changed.selected, selected);
      assert(changed.selection.start >= 1);
    }
  }
});

test("only an initial BOM is protected; interior U+FEFF stays selected content", () => {
  const content = "a" + BOM + "b";
  assert.equal(apply(content, "bold").result, "**" + content + "**");
  assert.equal(apply(BOM + content, "inline-code").result, BOM + TICK + content + TICK);
  const source = "first\n" + BOM + "# text";
  assert.equal(apply(source, "heading-2", 6, source.length).result, "first\n## " + BOM + "# text");
});

test("BOM documents round-trip line formatting without changing mixed line endings", () => {
  for (const command of ["heading-1", "heading-2", "heading-3", "bullet-list", "ordered-list", "task-list", "blockquote"]) {
    const source = BOM + "  café😀\r\nβ\r\n\r  漢字\nlast";
    const changed = apply(source, command);
    assert(changed.result.startsWith(BOM));
    assert.equal(apply(changed.result, command, changed.selection.start, changed.selection.end).result, source);
  }
});
