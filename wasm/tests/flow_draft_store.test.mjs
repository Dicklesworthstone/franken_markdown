// Explicit IDB event-contract doubles, not a substitute for the browser gate.
import test from "node:test";
import assert from "node:assert/strict";
import { openDraftStore } from "../demo/flow_draft_store.mjs";
const code = expected => error => error.code === expected;
function factory() {
  const transactions = [], opens = []; let closes = 0;
  const db = { objectStoreNames: { contains: name => name === "draft" }, close() { closes++; },
    transaction(name, mode) {
      assert.equal(name, "draft");
      const request = {}, writes = [];
      const tx = { mode, writes, error: null,
        objectStore(name) { assert.equal(name, "draft"); return {
          get(key) { assert.equal(key, "current"); return request; },
          put(value, key) { assert.equal(key, "current"); writes.push(value); }
        }; },
        read(value) { request.result = value; request.onsuccess(); },
        commit() { tx.oncomplete(); },
        abort() { queueMicrotask(() => tx.onabort()); }
      };
      transactions.push(tx); return tx;
    }
  };
  return { db, transactions, opens, get closes() { return closes; },
    open() { const request = { result: db }; opens.push(request); queueMicrotask(() => request.onsuccess()); return request; } };
}
test("save checks inside one readwrite transaction and acknowledges only transaction completion", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb });
  const doc = { filename: "a.md", source: "captured" }, pending = store.save(doc, 0);
  doc.source = "changed"; const tx = idb.transactions[0]; assert.equal(tx.mode, "readwrite");
  let resolved = false; pending.then(() => { resolved = true; }); tx.read(undefined); await Promise.resolve();
  assert.equal(resolved, false); assert.equal(tx.writes[0].document.source, "captured"); tx.commit();
  assert.equal((await pending).version, 1); store.dispose();
});
test("conflicts and corrupt records abort without issuing any write", async () => {
  for (const [record, expectedCode] of [
    [{ schemaVersion: 1, version: 2, document: null }, "DRAFT_CONFLICT"],
    [{ schemaVersion: 2, version: 1, document: null }, "CORRUPT_DRAFT"],
    [{ schemaVersion: 1, version: 1, document: { source: 0, filename: "a.md" } }, "CORRUPT_DRAFT"]
  ]) {
    const idb = factory(), store = await openDraftStore({ indexedDB: idb });
    const pending = store.save({ filename: "a.md", source: "new" }, 0); const rejected = assert.rejects(pending, code(expectedCode));
    idb.transactions[0].read(record); await rejected; assert.equal(idb.transactions[0].writes.length, 0); store.dispose();
  }
});
test("forget writes an incremented null-document tombstone, not a deletion vulnerable to ABA", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb }); const pending = store.clear(9), tx = idb.transactions[0];
  tx.read({ schemaVersion: 1, version: 9, document: { filename: "a.md", source: "old" } }); tx.commit();
  assert.deepEqual(await pending, { schemaVersion: 1, version: 10, document: null }); store.dispose();
});
test("transaction quota/abort errors never return a successful prepared record", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb }); const pending = store.save({ filename: "a.md", source: "x" }, 0);
  const rejected = assert.rejects(pending, code("STORAGE_FULL")), tx = idb.transactions[0]; tx.read(undefined);
  tx.error = new DOMException("quota", "QuotaExceededError"); tx.abort(); await rejected; store.dispose();
});
test("read-only retrieval validates data and treats only missing records as empty", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb }); const pending = store.read(), tx = idb.transactions[0];
  assert.equal(tx.mode, "readonly"); tx.read(undefined); tx.commit(); assert.equal((await pending).version, 0);
  const bad = store.read(), rejected = assert.rejects(bad, code("CORRUPT_DRAFT")); idb.transactions[1].read(null); await rejected; store.dispose();
});
test("disposal aborts admitted work and rejects subsequent operations", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb }); const pending = store.read();
  const rejected = assert.rejects(pending, code("STORAGE_CLOSED")); store.dispose(); await rejected;
  await assert.rejects(store.read(), code("STORAGE_CLOSED")); assert.equal(idb.closes, 1);
});
test("a transaction that never completes times out and aborts", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb, timeoutMs: 5 });
  await assert.rejects(store.read(), code("STORAGE_TIMEOUT")); store.dispose();
});
test("blocked or timed-out open closes a late successful connection", async () => {
  for (const blocked of [true, false]) {
    let request, closed = 0;
    const opening = openDraftStore({ indexedDB: { open() { request = {}; return request; } }, timeoutMs: 5 });
    const rejected = assert.rejects(opening, code(blocked ? "STORAGE_BLOCKED" : "STORAGE_TIMEOUT"));
    if (blocked) request.onblocked(); await rejected;
    request.result = { close() { closed++; } }; request.onsuccess(); assert.equal(closed, 1);
  }
});
test("malformed input and unavailable storage fail before creating a transaction", async () => {
  const idb = factory(), store = await openDraftStore({ indexedDB: idb });
  await assert.rejects(store.save({ filename: "a.md", source: "\ud800" }, 0), code("INVALID_UNICODE"));
  await assert.rejects(store.clear(-1), code("INVALID_REVISION")); assert.equal(idb.transactions.length, 0); store.dispose();
  await assert.rejects(openDraftStore({ indexedDB: null }), code("STORAGE_UNAVAILABLE"));
});
