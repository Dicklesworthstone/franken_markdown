// Production autosave/recovery state machine with an explicit transactional-store
// double. IndexedDB itself is exercised separately in Chromium, not mocked here.

import assert from "node:assert/strict";
import test from "node:test";
import { createDraftSession } from "../demo/flow_draft_session.mjs";

const code = (expected) => (error) => error.code === expected;
const gate = () => {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
function fixture(initial = null) {
  let record = { schemaVersion: 1, version: initial ? 1 : 0, document: initial },
    doc = { source: "initial", filename: "initial.md" };
  let hold = null,
    readHold = null,
    failure = null,
    confirmation = () => true,
    disposed = false;
  const saves = [],
    restored = [],
    states = [];
  const store = {
    async read() {
      if (readHold) await readHold.promise;
      return structuredClone(record);
    },
    async save(value, expected) {
      saves.push({ ...value });
      if (hold) await hold.promise;
      if (failure) throw failure;
      if (expected !== record.version)
        throw Object.assign(new Error("conflict"), { code: "DRAFT_CONFLICT" });
      record = { schemaVersion: 1, version: expected + 1, document: { ...value } };
      return structuredClone(record);
    },
    async clear(expected) {
      if (expected !== record.version)
        throw Object.assign(new Error("conflict"), { code: "DRAFT_CONFLICT" });
      record = { schemaVersion: 1, version: expected + 1, document: null };
      return structuredClone(record);
    },
    dispose() {
      disposed = true;
    },
  };
  const session = createDraftSession({
    store,
    readDocument: () => doc,
    restoreDocument(value) {
      doc = { ...value };
      restored.push(value);
      session.changed();
    },
    confirmRestore: (value) => confirmation(value),
    onState: (state) => states.push(state),
    delayMs: 60000,
  });
  return {
    session,
    saves,
    restored,
    states,
    get doc() {
      return doc;
    },
    get record() {
      return record;
    },
    get disposed() {
      return disposed;
    },
    edit(source, emit = true) {
      doc = { ...doc, source };
      if (emit) session.changed();
    },
    external(source) {
      record = {
        schemaVersion: 1,
        version: record.version + 1,
        document: { source, filename: "other.md" },
      };
    },
    hold(value) {
      hold = value;
    },
    readHold(value) {
      readHold = value;
    },
    reject(value) {
      failure = value;
    },
    confirm(fn) {
      confirmation = fn;
    },
  };
}
test("storage is read-only until explicit consent; saved recovery is never silently replaced", async (t) => {
  const f = fixture({ source: "recovered", filename: "saved.md" });
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.edit("new local");
  await f.session.flush();
  assert.equal(f.saves.length, 0);
  assert.equal(f.doc.source, "new local");
  assert.equal(f.session.state.available, true);
  assert.throws(() => f.session.setEnabled(true), code("RECOVERY_PENDING"));
});
test("restore uses the latest stored record, revokes through host replacement, and stays opt-in", async (t) => {
  const f = fixture({ source: "old", filename: "saved.md" });
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.external("latest");
  await f.session.restore();
  assert.equal(f.doc.source, "latest");
  assert.equal(f.restored.length, 1);
  assert.equal(f.session.state.version, 2);
  assert.equal(f.session.state.dirty, false);
  assert.equal(f.session.state.enabled, false);
  assert.equal(f.session.state.available, false);
});
test("a single in-flight save coalesces arbitrarily many changes to the latest source", async (t) => {
  const f = fixture(),
    hold = gate();
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.hold(hold);
  f.session.setEnabled(true);
  const first = f.session.flush();
  await Promise.resolve();
  for (let i = 0; i < 100; i++) f.edit(`revision ${i}`);
  const second = f.session.flush();
  assert.equal(f.saves.length, 1);
  assert.equal(f.session.state.dirty, true);
  hold.resolve();
  await Promise.all([first, second]);
  assert.deepEqual(
    f.saves.map((v) => v.source),
    ["initial", "revision 99"],
  );
  assert.equal(f.record.document.source, "revision 99");
  assert.equal(f.session.state.dirty, false);
});
test("save acknowledgment cannot erase typing and detects programmatic edits without events", async (t) => {
  const f = fixture(),
    hold = gate();
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.session.setEnabled(true);
  f.hold(hold);
  const saving = f.session.flush();
  await Promise.resolve();
  f.edit("programmatic", false);
  hold.resolve();
  await saving;
  assert.equal(f.doc.source, "programmatic");
  assert.equal(f.record.document.source, "programmatic");
});
test("cross-tab conflicts stop autosave without overwriting either document; explicit recovery resolves", async (t) => {
  const f = fixture();
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.external("other tab");
  f.session.setEnabled(true);
  await assert.rejects(f.session.flush(), code("DRAFT_CONFLICT"));
  assert.equal(f.record.document.source, "other tab");
  assert.equal(f.doc.source, "initial");
  assert.equal(f.session.state.enabled, false);
  assert.throws(() => f.session.setEnabled(true), code("DRAFT_CONFLICT"));
  await f.session.inspect();
  await f.session.restore();
  assert.equal(f.doc.source, "other tab");
});
test("forget compares the observed revision, never deletes a newer tab's draft, and preserves local source", async (t) => {
  const f = fixture({ source: "saved", filename: "saved.md" });
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.external("newer");
  await assert.rejects(f.session.forget(), code("DRAFT_CONFLICT"));
  assert.equal(f.record.document.source, "newer");
  await f.session.inspect();
  await f.session.forget();
  assert.equal(f.record.document, null);
  assert.equal(f.doc.source, "initial");
  assert.equal(f.session.state.enabled, false);
});
test("disabling cancels future saves but waits honestly for an already-dispatched acknowledgment", async (t) => {
  const f = fixture(),
    hold = gate();
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.hold(hold);
  f.session.setEnabled(true);
  const saving = f.session.flush();
  await Promise.resolve();
  f.edit("new");
  f.session.setEnabled(false);
  hold.resolve();
  await saving;
  assert.equal(f.record.document.source, "initial");
  assert.equal(f.doc.source, "new");
  assert.equal(f.saves.length, 1);
  assert.equal(f.session.state.dirty, true);
});
test("typing during recovery or its confirmation prevents late replacement", async (t) => {
  for (const phase of ["read", "confirm"]) {
    const f = fixture({ source: "saved", filename: "saved.md" }),
      hold = gate(),
      entered = gate();
    t.after(() => f.session.dispose());
    await f.session.inspect();
    if (phase === "read") f.readHold(hold);
    else
      f.confirm(() => {
        entered.resolve();
        return hold.promise;
      });
    const restoring = f.session.restore();
    if (phase === "confirm") await entered.promise;
    f.edit("keep typing");
    hold.resolve(true);
    await assert.rejects(restoring, code("STALE_SOURCE"));
    assert.equal(f.restored.length, 0);
  }
});
test("quota/storage errors retain dirty source, pause saving and support an explicit retry", async (t) => {
  const f = fixture();
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.reject(Object.assign(new Error("quota"), { code: "STORAGE_FULL" }));
  f.session.setEnabled(true);
  await assert.rejects(f.session.flush(), code("STORAGE_FULL"));
  assert.equal(f.session.state.dirty, true);
  assert.equal(f.doc.source, "initial");
  f.reject(null);
  f.session.setEnabled(true);
  await f.session.flush();
  assert.equal(f.session.state.dirty, false);
});
test("malformed source is reported before storage and cannot leave a false saved status", async (t) => {
  const f = fixture();
  t.after(() => f.session.dispose());
  await f.session.inspect();
  f.session.setEnabled(true);
  f.edit("\ud800");
  await assert.rejects(f.session.flush(), code("INVALID_UNICODE"));
  assert.equal(f.saves.length, 0);
  assert.equal(f.session.state.enabled, false);
});
test("disposal suppresses late recovery and late save publication", async () => {
  const f = fixture({ source: "saved", filename: "saved.md" }),
    hold = gate();
  await f.session.inspect();
  f.readHold(hold);
  const pending = f.session.restore();
  f.session.dispose();
  hold.resolve();
  await assert.rejects(pending, code("SESSION_DISPOSED"));
  assert.equal(f.restored.length, 0);
  assert.equal(f.disposed, true);
  assert.equal(f.session.state.disposed, true);
});
