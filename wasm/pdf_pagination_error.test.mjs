import assert from "node:assert/strict";
import test from "node:test";
import { isPdfPaginationError, normalizePdfPaginationError } from "./render_typography.mjs";

test("only the bounded native pagination JSON contract becomes a coded Error", () => {
  for (const reason of ["infeasible", "budget_exceeded", "invalid_input", "cost_overflow", "invalid_plan"]) {
    const code = `PAGINATION_${reason.toUpperCase()}`;
    const error = normalizePdfPaginationError(JSON.stringify({ code, reason, message: 'quoted "detail"\n' }));
    assert(error instanceof Error);
    assert.equal(error.code, code);
    assert.equal(error.reason, reason);
    assert.equal(error.message, 'quoted "detail"\n');
    assert(isPdfPaginationError(error));
    assert.equal(normalizePdfPaginationError(error), error);
  }
  const native = { code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "detail" };
  for (const input of [
    "strict PDF pagination failed: human message alone", "invalid JSON", "null", "[]",
    JSON.stringify({ ...native, code: "UNRECOGNIZED" }),
    JSON.stringify({ ...native, reason: "budget_exceeded" }),
    JSON.stringify({ code: native.code, message: native.message }),
    JSON.stringify({ ...native, message: "x".repeat(2049) }),
    `${JSON.stringify(native)}${" ".repeat(4096)}`,
    new Error("ordinary failure"),
  ]) {
    assert.equal(normalizePdfPaginationError(input), input);
    assert.equal(isPdfPaginationError(input), false);
  }
});
