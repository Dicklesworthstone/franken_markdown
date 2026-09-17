// Production DOM controls and draft state machine, with explicit elements and
// transactional store doubles. Actual IndexedDB belongs to the browser gate.
import test from "node:test";
import assert from "node:assert/strict";
import { createDraftControls } from "../demo/flow_draft_controls.mjs";
const tick = () => new Promise(resolve => setImmediate(resolve));
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { resolve, promise }; };
class Element extends EventTarget {
  disabled = false; checked = false; value = ""; textContent = "";
  click() { this.dispatchEvent(new Event("click")); }
}
function fixture(document = null) {
  const sourceEditor = new Element(), filename = new Element(), remember = new Element(), refresh = new Element();
  const restore = new Element(), forget = new Element(), save = new Element(), status = new Element();
  sourceEditor.value = "local"; filename.value = "local.md";
  let record = { schemaVersion: 1, version: document ? 1 : 0, document }, unavailable = false, openGate = null;
  let allowRestore = true, allowForget = true, confirms = 0, replacements = 0, writes = 0, opens = 0, closes = 0;
  const controls = createDraftControls({ sourceEditor, filename, remember, refresh, restore, forget, save, status, delayMs: 60000,
    readDocument: () => ({ filename: filename.value, source: sourceEditor.value }),
    restoreDocument(value) { replacements++; sourceEditor.value = value.source; filename.value = value.filename; sourceEditor.dispatchEvent(new Event("input")); },
    confirmRestore: () => { confirms++; return allowRestore; }, confirmForget: () => { confirms++; return allowForget; },
    async openStore() {
      opens++; if (openGate) await openGate.promise;
      if (unavailable) throw Object.assign(new Error("Unavailable storage"), { code: "STORAGE_UNAVAILABLE" });
      let closed = false;
      const check = () => { if (closed) throw Object.assign(new Error("closed"), { code: "STORAGE_CLOSED" }); };
      return {
        async read() { check(); return structuredClone(record); },
        async save(document, version) {
          check();
          if (version !== record.version) throw Object.assign(new Error("other tab"), { code: "DRAFT_CONFLICT" });
          writes++; record = { schemaVersion: 1, version: version + 1, document }; return structuredClone(record);
        },
        async clear(version) {
          check();
          if (version !== record.version) throw Object.assign(new Error("other tab"), { code: "DRAFT_CONFLICT" });
          record = { schemaVersion: 1, version: version + 1, document: null }; return structuredClone(record);
        }, dispose() { if (!closed) { closed = true; closes++; } }
      };
    }
  });
  return { controls, sourceEditor, filename, remember, refresh, restore, forget, save, status,
    get record() { return record; }, get writes() { return writes; }, get opens() { return opens; }, get closes() { return closes; },
    get confirms() { return confirms; }, get replacements() { return replacements; },
    block(value) { unavailable = value; }, gate(value) { openGate = value; },
    approveRestore(value) { allowRestore = value; }, approveForget(value) { allowForget = value; },
    external(value) { record = { schemaVersion: 1, version: record.version + 1, document: { source: value, filename: "external.md" } }; },
    enable(value = true) { remember.checked = value; remember.dispatchEvent(new Event("change")); }
  };
}
test("initial inspection never changes source or consents to saving; checkbox is the opt-in", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); await f.controls.whenIdle();
  assert.equal(f.sourceEditor.value, "local"); assert.equal(f.remember.checked, false); assert.equal(f.writes, 0);
  f.enable(); assert.equal(f.remember.checked, true); f.save.click(); await f.controls.whenIdle();
  assert.equal(f.record.document.source, "local"); assert.equal(f.controls.state.dirty, false);
  assert(f.status.textContent.includes("acknowledged")); assert.equal(f.save.disabled, true);
});
test("offered recovery blocks autosave; declined restore keeps editor and grant unchanged", async t => {
  const f = fixture({ source: "saved", filename: "saved.md" }); t.after(() => f.controls.dispose()); await f.controls.whenIdle();
  assert.equal(f.remember.disabled, true); assert.equal(f.restore.disabled, false); assert.equal(f.forget.disabled, false);
  f.approveRestore(false); f.restore.click(); await f.controls.whenIdle();
  assert.equal(f.sourceEditor.value, "local"); assert.equal(f.replacements, 0); assert.equal(f.remember.checked, false);
  f.approveRestore(true); f.restore.click(); await f.controls.whenIdle();
  assert.equal(f.sourceEditor.value, "saved"); assert.equal(f.replacements, 1); assert.equal(f.remember.checked, false);
});
test("forget requires confirmation, removes only stored source, and never silently reenables saving", async t => {
  const f = fixture({ source: "saved", filename: "saved.md" }); t.after(() => f.controls.dispose()); await f.controls.whenIdle();
  f.approveForget(false); f.forget.click(); await f.controls.whenIdle(); assert.equal(f.record.document.source, "saved");
  f.approveForget(true); f.forget.click(); await f.controls.whenIdle();
  assert.equal(f.record.document, null); assert.equal(f.sourceEditor.value, "local"); assert.equal(f.remember.checked, false); assert.equal(f.writes, 0);
});
test("source and filename input invalidate the saved acknowledgment independently of preview", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); await f.controls.whenIdle(); f.enable(); await f.controls.flush();
  f.sourceEditor.value = "not renderable 😀"; f.sourceEditor.dispatchEvent(new Event("input"));
  assert.equal(f.controls.state.dirty, true); await f.controls.flush();
  f.filename.value = "renamed.md"; f.filename.dispatchEvent(new Event("input")); await f.controls.flush();
  assert.deepEqual(f.record.document, { source: "not renderable 😀", filename: "renamed.md" });
});
test("conflict visibly pauses saving and refresh offers the other version without replacing local text", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); await f.controls.whenIdle(); f.external("other"); f.enable();
  await assert.rejects(f.controls.flush(), error => error.code === "DRAFT_CONFLICT");
  assert.equal(f.remember.checked, false); assert.equal(f.remember.disabled, true); assert(f.status.textContent.includes("DRAFT_CONFLICT"));
  await f.controls.refresh(); assert.equal(f.sourceEditor.value, "local"); assert.equal(f.controls.state.available, true);
  assert.equal(f.restore.disabled, false); assert.equal(f.opens, 2); assert.equal(f.closes, 1);
});
test("unavailable storage leaves source untouched and explicit Refresh can reconnect", async t => {
  const f = fixture(); t.after(() => f.controls.dispose()); f.block(true);
  await assert.rejects(f.controls.whenIdle()); assert.equal(f.sourceEditor.value, "local"); assert.equal(f.refresh.disabled, false);
  assert.equal(f.remember.disabled, true); assert(f.status.textContent.includes("Download Markdown"));
  f.block(false); await f.controls.refresh(); assert.equal(f.remember.disabled, false); assert.equal(f.writes, 0);
});
test("one physical connection is retained; late open is closed after disposal", async () => {
  const f = fixture(), hold = gate(); f.gate(hold); const pending = f.controls.whenIdle();
  assert.throws(() => f.controls.refresh(), error => error.code === "DRAFT_BUSY");
  await Promise.resolve(); f.controls.dispose(); hold.resolve(); await pending;
  assert.equal(f.opens, 1); assert.equal(f.closes, 1); assert.equal(f.sourceEditor.value, "local");
});
test("disposing during recovery confirmation cannot replace the document", async () => {
  const f = fixture({ source: "saved", filename: "saved.md" }), hold = gate(); await f.controls.whenIdle();
  f.approveRestore(hold.promise); const pending = f.controls.restore(); await tick();
  f.controls.dispose(); hold.resolve(true); await assert.rejects(pending, error => error.code === "SESSION_DISPOSED");
  assert.equal(f.replacements, 0); assert.equal(f.sourceEditor.value, "local");
});
test("disposal removes input and command listeners and leaves buttons disabled", async () => {
  const f = fixture(); await f.controls.whenIdle(); f.controls.dispose(); f.controls.dispose();
  const opens = f.opens; f.refresh.click(); f.enable(); f.sourceEditor.dispatchEvent(new Event("input")); await tick();
  assert.equal(f.opens, opens); assert.equal(f.closes, 1); assert.equal(f.refresh.disabled, true); assert.equal(f.save.disabled, true);
});
