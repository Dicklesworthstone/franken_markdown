import assert from "node:assert/strict";
import test from "node:test";
import { planSourceCommand } from "../demo/source_commands.mjs";
const apply = (source, command, start = 0, end = source.length) => {
  const plan = planSourceCommand(source, { start, end }, command);
  return { result: source.slice(0, plan.start) + plan.text + source.slice(plan.end), ...plan };
};

for (const [first, second] of [["bold", "italic"], ["italic", "bold"]]) {
  test(`${second} composes with ${first} without removing its marker`, () => {
    const a = apply("word", first);
    const b = apply(a.result, second, a.selection.start, a.selection.end);
    assert.equal(b.result, "***word***");
    const c = apply(b.result, second, b.selection.start, b.selection.end);
    assert.equal(c.result, a.result);
  });
}
test("whole selected strong markup can acquire italic without losing strong", () => {
  assert.equal(apply("**word**", "italic").result, "***word***");
});
test("asymmetric marker runs are not partially stripped", () => {
  assert.equal(apply("**word*", "italic", 2, 6).result, "***word**");
});
