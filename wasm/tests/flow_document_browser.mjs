// Actual browser IndexedDB, transactions, File, Blob, DOM and object URLs.
// No mocks of storage and no claim that this exercises the Rust renderer.
import { openDraftStore } from "../demo/flow_draft_store.mjs";
import { createDraftSession } from "../demo/flow_draft_session.mjs";
import { createSourceControls, readMarkdownFile, markdownDownload } from "../demo/flow_document.mjs";
const assert = (value, message = "assertion failed") => { if (!value) throw new Error(message); };
const equal = (a, b) => assert(JSON.stringify(a) === JSON.stringify(b), "values differ");
async function rejects(promise, code) {
  try { await promise; } catch (error) { assert(error.code === code, `expected ${code}, got ${error.code}: ${error.message}`); return; }
  throw new Error(`expected ${code}, but operation succeeded`);
}
const rawOpen = (name, version) => new Promise((resolve, reject) => {
  const request = indexedDB.open(name, version);
  request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
});
const rawPut = (db, record) => new Promise((resolve, reject) => {
  const tx = db.transaction("draft", "readwrite"); tx.objectStore("draft").put(record, "current");
  tx.oncomplete = resolve; tx.onabort = () => reject(tx.error);
});
const remove = name => new Promise((resolve, reject) => {
  const request = indexedDB.deleteDatabase(name); request.onsuccess = resolve; request.onerror = () => reject(request.error);
});
export async function run() {
  const checks = [], names = [], stores = [];
  const unique = () => { const name = `fmd-document-check-${crypto.randomUUID()}`; names.push(name); return name; };
  const open = async name => { const store = await openDraftStore({ name }); stores.push(store); return store; };
  const check = async (name, fn) => { try { await fn(); checks.push({ name, ok: true }); } catch (error) { checks.push({ name, ok: false, error: error.stack ?? String(error) }); } };
  const source = "\ufeff# Native browser source\r\n\r\n😀 e\u0301\r\n";
  let reloadName;
  try {
    await check("new storage is empty; writes snapshot only source/name before transaction dispatch", async () => {
      const name = unique(), a = await open(name), b = await open(name);
      equal(await a.read(), { schemaVersion: 1, version: 0, document: null });
      const doc = { filename: "original.md", source, grants: ["must-not-persist"] };
      const pending = a.save(doc, 0); doc.source = "mutated by caller";
      const saved = await pending;
      equal(saved.document, { filename: "original.md", source }); equal(await b.read(), saved);
    });
    await check("two connections racing from one revision have exactly one winner", async () => {
      const name = unique(), a = await open(name), b = await open(name);
      const results = await Promise.allSettled([a.save({ filename: "a.md", source: "first" }, 0), b.save({ filename: "b.md", source: "second" }, 0)]);
      assert(results.filter(result => result.status === "fulfilled").length === 1);
      const loser = results.find(result => result.status === "rejected"); assert(loser.reason.code === "DRAFT_CONFLICT");
      equal((await a.read()).document, results.find(result => result.status === "fulfilled").value.document);
    });
    await check("forget uses a revisioned tombstone and rejects stale recreation/deletion", async () => {
      const a = await open(unique()); await a.save({ filename: "a.md", source: "first" }, 0);
      await a.clear(1); equal(await a.read(), { schemaVersion: 1, version: 2, document: null });
      await rejects(a.save({ filename: "stale.md", source: "wrong" }, 0), "DRAFT_CONFLICT");
      await a.save({ filename: "new.md", source: "new" }, 2);
      await rejects(a.clear(2), "DRAFT_CONFLICT"); assert((await a.read()).document.source === "new");
    });
    await check("successful put followed by transaction abort is not acknowledged as saved", async () => {
      const a = await open(unique()); await a.save({ filename: "a.md", source: "keep" }, 0);
      const original = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function (...args) {
        const request = original.apply(this, args), tx = this.transaction;
        request.addEventListener("success", () => tx.abort(), { once: true }); return request;
      };
      try { await rejects(a.save({ filename: "bad.md", source: "not committed" }, 1), "STORAGE_UNAVAILABLE"); }
      finally { IDBObjectStore.prototype.put = original; }
      assert((await a.read()).document.source === "keep");
    });
    await check("corrupt persisted records cannot be read as empty or silently overwritten", async () => {
      const name = unique(), a = await open(name), db = await rawOpen(name, 1);
      try { await rawPut(db, { schemaVersion: 1, version: 1, document: { filename: "a.md", source: 42 } }); }
      finally { db.close(); }
      await rejects(a.read(), "CORRUPT_DRAFT"); await rejects(a.save({ filename: "x.md", source: "replacement" }, 1), "CORRUPT_DRAFT");
    });
    await check("disposal aborts pending writes; reopening still sees the previous acknowledged source", async () => {
      const name = unique(), a = await open(name); await a.save({ filename: "a.md", source: "keep" }, 0);
      const pending = a.save({ filename: "b.md", source: "late" }, 1); a.dispose();
      await rejects(pending, "STORAGE_CLOSED"); const b = await open(name); assert((await b.read()).document.source === "keep");
    });
    await check("database version changes close old clients rather than leave upgrades blocked", async () => {
      const name = unique(), a = await open(name), b = await open(name), upgraded = await rawOpen(name, 2);
      upgraded.close(); await rejects(a.read(), "STORAGE_CLOSED"); await rejects(b.read(), "STORAGE_CLOSED");
    });
    await check("revision exhaustion and malformed Unicode reject without replacement", async () => {
      const name = unique(), a = await open(name), db = await rawOpen(name, 1);
      try { await rawPut(db, { schemaVersion: 1, version: Number.MAX_SAFE_INTEGER, document: { filename: "keep.md", source: "keep" } }); }
      finally { db.close(); }
      await rejects(a.save({ filename: "x.md", source: "overflow" }, Number.MAX_SAFE_INTEGER), "REVISION_EXHAUSTED");
      await rejects(a.save({ filename: "x.md", source: "\ud800" }, Number.MAX_SAFE_INTEGER), "INVALID_UNICODE");
      assert((await a.read()).document.source === "keep");
    });
    await check("native File/Blob preserve BOM, CRLF and Unicode bytes", async () => {
      const bytes = new TextEncoder().encode(source), doc = await readMarkdownFile(new File([bytes], "report.md"));
      equal([...new Uint8Array(await markdownDownload(doc).blob.arrayBuffer())], [...bytes]);
      await rejects(readMarkdownFile(new File([new Uint8Array([0xff])], "invalid.md")), "INVALID_UNICODE");
    });
    await check("real DOM source controls revoke prior URLs and file replacement revokes grants before input", async () => {
      const sourceEditor = document.createElement("textarea"), filename = document.createElement("input"), input = document.createElement("input");
      const prepare = document.createElement("button"), download = document.createElement("a"), status = document.createElement("p");
      sourceEditor.value = source; filename.value = "old.md"; let revoked = false, inputs = 0;
      const controls = createSourceControls({ sourceEditor, filename, open: input, prepare, download, status, onReplace() { revoked = true; } });
      try {
        controls.replace({ filename: "old.md", source });
        controls.prepareDownload(); const previous = download.href;
        equal([...new Uint8Array(await (await fetch(previous)).arrayBuffer())], [...new TextEncoder().encode(source)]);
        sourceEditor.addEventListener("input", () => { assert(revoked); inputs++; });
        await controls.openFile(new File(["new source"], "new.md"));
        assert(inputs === 1 && download.hidden && sourceEditor.value === "new source");
        let missing = false; try { await fetch(previous); } catch { missing = true; } assert(missing, "old Blob URL must be revoked");
      } finally { controls.dispose(); }
    });
    await check("real storage plus autosave pauses cross-tab conflicts and preserves the local editor", async () => {
      const name = unique(), a = await open(name), b = await open(name); let doc = { filename: "local.md", source: "local" };
      const session = createDraftSession({ store: a, readDocument: () => doc, restoreDocument: value => { doc = value; }, delayMs: 60000 });
      try {
        await session.inspect(); session.setEnabled(true); await session.flush();
        await b.save({ filename: "remote.md", source: "other tab" }, 1);
        doc = { ...doc, source: "unsaved local" }; session.changed(); await rejects(session.flush(), "DRAFT_CONFLICT");
        assert(doc.source === "unsaved local" && (await b.read()).document.source === "other tab" && !session.state.enabled);
      } finally { session.dispose(); }
    });
    await check("source draft is committed for an actual page reload", async () => {
      reloadName = `fmd-reload-check-${crypto.randomUUID()}`;
      const a = await open(reloadName); await a.save({ filename: "reloaded.md", source }, 0); a.dispose();
    });
  } finally {
    for (const store of stores) store.dispose();
    for (const name of names) await remove(name);
  }
  document.querySelector("#results").textContent = JSON.stringify(checks, null, 2);
  return { checks, reloadName };
}
export async function resume(name) {
  const store = await openDraftStore({ name });
  try {
    const saved = await store.read();
    assert(saved.version === 1 && saved.document.filename === "reloaded.md");
    assert(saved.document.source === "\ufeff# Native browser source\r\n\r\n😀 e\u0301\r\n");
    return { name: "actual page reload recovers byte-faithful original source", ok: true };
  } finally { store.dispose(); await remove(name); }
}
