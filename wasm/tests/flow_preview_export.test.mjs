// Production preview-export admission; explicit controller/session state doubles.

import assert from "node:assert/strict";
import test from "node:test";
import { createPreviewExport } from "../demo/flow_preview_export.mjs";

const code = (expected) => (error) => error.code === expected;
const gate = () => {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
function fixture() {
  const calls = [];
  let hold = null,
    failure = null;
  const token = { revision: "9007199254740993", layoutRevision: "9007199254740997" };
  const session = {
    disposed: false,
    token,
    async exportDocument(format, options, expected) {
      calls.push({ format, options, expected });
      if (hold) await hold.promise;
      if (failure) throw failure;
      return {
        schemaVersion: 1,
        ...expected,
        format,
        mimeType: format === "pdf" ? "application/pdf" : "text/html; charset=utf-8",
        bytes: new Uint8Array([1, 2]),
        diagnostics: [],
        font: "sans",
        sourceLengthBytes: 3,
        assetCount: 0,
        assetBytes: 0,
      };
    },
  };
  const state = {
    session,
    source: "old",
    desiredSource: "old",
    status: "ready",
    frame: { ...token },
    imagesBusy: false,
    epoch: 1,
    disposed: false,
  };
  return {
    state,
    session,
    calls,
    export: createPreviewExport(() => ({ ...state })),
    hold(value) {
      hold = value;
    },
    reject(value) {
      failure = value;
    },
  };
}
test("exports the applied source/frame without inventing input or extra worker calls", async () => {
  const f = fixture(),
    result = await f.export("pdf", { pageNumbers: true }, "old");
  assert.equal(result.revision, f.state.frame.revision);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(f.calls[0], {
    format: "pdf",
    options: { pageNumbers: true },
    expected: f.session.token,
  });
});
test("rejects unsubmitted textarea edits, unapplied source, missing/stale frames and loading images", async () => {
  const unsubmitted = fixture();
  await assert.rejects(unsubmitted.export("pdf", {}, "new"));
  assert.equal(unsubmitted.calls.length, 0);
  for (const change of [
    (f) => (f.state.desiredSource = "new"),
    (f) => (f.state.status = "busy"),
    (f) => (f.state.imagesBusy = true),
    (f) => (f.state.frame.layoutRevision = "1"),
    (f) => (f.session.disposed = true),
  ]) {
    const f = fixture();
    change(f);
    await assert.rejects(f.export("pdf", {}, "old"));
    assert.equal(f.calls.length, 0);
  }
});
test("source edits, restart, disposal and asset changes invalidate delayed export publication", async () => {
  for (const change of [
    (f) => (f.state.desiredSource = "new"),
    (f) => f.state.epoch++,
    (f) => (f.state.disposed = true),
    (f) => (f.state.session = { ...f.session }),
    (f) => (f.session.token = { ...f.session.token, layoutRevision: "99" }),
  ]) {
    const f = fixture(),
      hold = gate();
    f.hold(hold);
    const pending = f.export("pdf", {}, "old");
    change(f);
    hold.resolve();
    await assert.rejects(pending);
  }
});
test("holds one physical slot across restarts and releases it after stale completion", async () => {
  const f = fixture(),
    hold = gate();
  f.hold(hold);
  const pending = f.export("pdf", {}, "old");
  f.state.epoch++;
  await assert.rejects(f.export("pdf", {}, "old"), code("EXPORT_BUSY"));
  hold.resolve();
  await assert.rejects(pending, code("STALE_REVISION"));
  f.hold(null);
  assert.equal((await f.export("html", {}, "old")).format, "html");
});
test("scroll-only busy state preserves a completed export of the same source and layout", async () => {
  const f = fixture(),
    hold = gate();
  f.hold(hold);
  const pending = f.export("pdf", {}, "old");
  f.state.status = "busy";
  hold.resolve();
  assert.equal((await pending).format, "pdf");
});
test("export errors release the slot and do not mutate or dispose the preview", async () => {
  const f = fixture();
  f.reject(Object.assign(new Error("native reject"), { code: "EXPORT_FAILED" }));
  await assert.rejects(f.export("pdf", {}, "old"), code("EXPORT_FAILED"));
  assert.equal(f.session.disposed, false);
  assert.equal(f.state.source, "old");
  f.reject(null);
  await f.export("pdf", {}, "old");
});
test("validates returned document ownership metadata and lower output cap", async () => {
  const f = fixture();
  await assert.rejects(f.export("pdf", { maxOutputBytes: 1 }, "old"), code("BUDGET_EXCEEDED"));
  f.session.exportDocument = async () => ({ format: "html" });
  await assert.rejects(f.export("pdf", {}, "old"), code("INVALID_WASM_RESPONSE"));
});
