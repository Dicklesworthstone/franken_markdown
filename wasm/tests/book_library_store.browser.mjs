// Native browser/IndexedDB acceptance. No database, transaction or storage doubles.
import { createBookLibraryStore } from "../demo/book_library_store.mjs";
const equal = (a, b, message = "values differ") => { if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${message}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`); };
const check = (value, message) => { if (!value) throw new Error(message); };
async function rejects(operation, code) {
  try { await operation(); } catch (error) { equal(error.code, code, error.message); return; }
  throw new Error(`expected ${code}`);
}
const project = (source = "# First\n", path = "start.md") => ({ schemaVersion: 1, files: [{ path, source }], options: { title: "Manual" } });

export async function runBookLibraryStoreTests() {
  const results = [], stores = [];
  const store = (name = `book-library-test-${crypto.randomUUID()}`, options = {}) => {
    const result = createBookLibraryStore({ dbName: name, ...options }); stores.push(result); return result;
  };
  const test = async (name, run) => {
    try { await run(); results.push({ name, passed: true }); }
    catch (error) { results.push({ name, passed: false, error: error.stack ?? String(error) }); }
    finally { for (const item of stores.splice(0)) item.close(); }
  };
  await test("source-only snapshot survives reopen, input mutation and exact BOM/line endings", async () => {
    const name = `book-library-test-${crypto.randomUUID()}`, a = store(name);
    const original = "\ufeff# Café\r\n\r\n中🚀\rLast\n", input = project(original, "guide/start.md");
    input.files.push({ path: "end.md", source: "# End" });
    const pending = a.save({ name: "Manual", project: input });
    input.files[0].source = "mutated"; input.options.title = "mutated";
    const receipt = await pending; equal(receipt.revision, 1); a.close();
    const b = store(name), recovered = await b.read(receipt.id);
    equal(recovered.project.files, [{ path: "guide/start.md", source: original }, { path: "end.md", source: "# End" }]);
    equal(recovered.project.options.title, "Manual");
    const listed = await b.list(); equal(listed.length, 1); check(!("project" in listed[0]) && !("json" in listed[0]), "listing leaked chapter payloads");
    listed[0].name = "edited metadata"; equal((await b.read(receipt.id)).name, "Manual");
  });
  await test("two independent connections race the same revision: exactly one writer wins", async () => {
    const name = `book-library-test-${crypto.randomUUID()}`, a = store(name), b = store(name);
    const first = await a.save({ name: "Race", project: project() });
    const requests = [a, b].map((db, i) => db.save({ id: first.id, expectedRevision: 1, name: "Race", project: project(`writer-${i}`) }));
    const outcomes = await Promise.allSettled(requests);
    equal(outcomes.filter(item => item.status === "fulfilled").length, 1);
    equal(outcomes.find(item => item.status === "rejected").reason.code, "LIBRARY_CONFLICT");
    const latest = await a.read(first.id); equal(latest.revision, 2);
    equal(latest.project.files[0].source, `writer-${outcomes.findIndex(item => item.status === "fulfilled")}`);
    equal((await a.read(first.id, 1)).project.files[0].source, "# First\n");
  });
  await test("stale deletes and writes after deletion cannot replace saved books", async () => {
    const a = store(); let current = await a.save({ name: "Keep", project: project() });
    current = await a.save({ id: current.id, expectedRevision: 1, name: "Keep", project: project("new") });
    await rejects(() => a.remove(current.id, 1), "LIBRARY_CONFLICT"); equal((await a.read(current.id)).revision, 2);
    await a.remove(current.id, 2); equal(await a.list(), []);
    await rejects(() => a.save({ id: current.id, expectedRevision: 2, name: "Old tab", project: project() }), "LIBRARY_CONFLICT");
  });
  await test("five recent versions retained; historical recovery creates a new head", async () => {
    const a = store(); let current = await a.save({ name: "v1", project: project("one") });
    for (let revision = 2; revision <= 7; revision++) current = await a.save({ id: current.id, expectedRevision: revision - 1, name: `v${revision}`, project: project(String(revision)) });
    equal(current.versions.map(item => item.revision), [7, 6, 5, 4, 3]);
    await rejects(() => a.read(current.id, 2), "LIBRARY_NOT_FOUND");
    const historical = await a.read(current.id, 3); equal(historical.revision, 7); equal(historical.snapshotRevision, 3); equal(historical.name, "v3");
    current = await a.save({ id: current.id, expectedRevision: historical.revision, name: "Recovered", project: historical.project });
    equal(current.revision, 8); equal((await a.read(current.id)).project.files[0].source, "3");
    equal((await a.read(current.id, 7)).project.files[0].source, "7");
  });
  await test("capacity refusal is atomic and deleting one project releases its budget", async () => {
    const a = store(undefined, { limits: { projects: 2, bytes: 2048 } });
    const first = await a.save({ name: "First", project: project("1") });
    await a.save({ name: "Second", project: project("2") });
    await rejects(() => a.save({ name: "Third", project: project("3") }), "LIBRARY_LIMIT");
    equal((await a.list()).length, 2); await a.remove(first.id, 1);
    await a.save({ name: "Third", project: project("3") }); equal((await a.list()).length, 2);
  });
  await test("an oversized update does not prune history or replace the last saved head", async () => {
    const a = store(undefined, { limits: { bytes: 1024 } });
    let current = await a.save({ name: "Keep", project: project("old") });
    current = await a.save({ id: current.id, expectedRevision: 1, name: "Keep", project: project("new") });
    const before = await a.list();
    await rejects(() => a.save({ id: current.id, expectedRevision: 2, name: "Too large", project: project("x".repeat(1200)) }), "LIBRARY_LIMIT");
    equal(await a.list(), before); equal((await a.read(current.id, 1)).project.files[0].source, "old");
    equal((await a.read(current.id)).project.files[0].source, "new");
  });
  await test("quota pruning only removes the updating book's own old versions", async () => {
    const a = store(undefined, { limits: { bytes: 900 } });
    const other = await a.save({ name: "Other", project: project("other") });
    let current = await a.save({ name: "Working", project: project("1") });
    for (let i = 2; i < 8; i++) current = await a.save({ id: current.id, expectedRevision: i - 1, name: "Working", project: project(String(i)) });
    equal((await a.read(other.id)).project.files[0].source, "other"); check(current.versions.length < 5, "quota should prune own history");
  });
  await test("asset grants and malformed project input cannot enter persistent storage", async () => {
    const a = store();
    await rejects(() => a.save({ name: "Bad", project: { ...project(), images: [{ destination: "secret.png", bytes: new Uint8Array([1]) }] } }), "INVALID_PROJECT");
    await rejects(() => a.save({ name: "Bad", project: { ...project(), options: { images: [] } } }), "INVALID_OPTIONS");
    await rejects(() => a.save({ name: "Bad", project: project("# x", "../escape.md") }), "INVALID_PATH");
    equal(await a.list(), []);
  });
  await test("closing during database initialization aborts the unacknowledged save", async () => {
    const name = `book-library-test-${crypto.randomUUID()}`, a = store(name);
    const saving = a.save({ name: "Not committed", project: project() }); a.close();
    await rejects(() => saving, "LIBRARY_CLOSED"); equal(await store(name).list(), []);
  });
  await test("closing an active transaction preserves the previously acknowledged snapshot", async () => {
    const name = `book-library-test-${crypto.randomUUID()}`, a = store(name);
    const first = await a.save({ name: "Keep", project: project("old") });
    const saving = a.save({ id: first.id, expectedRevision: 1, name: "New", project: project("new") });
    await Promise.resolve(); a.close(); await rejects(() => saving, "LIBRARY_CLOSED");
    equal((await store(name).read(first.id)).project.files[0].source, "old");
  });
  await test("corrupt persisted snapshot fails closed without discarding the record", async () => {
    const name = `book-library-test-${crypto.randomUUID()}`, a = store(name), first = await a.save({ name: "Keep", project: project() });
    const raw = await new Promise((resolve, reject) => { const r = indexedDB.open(name, 1); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
    await new Promise((resolve, reject) => { const tx = raw.transaction(["snapshots"], "readwrite"); tx.objectStore("snapshots").put({ key: `${first.id}:1`, json: "bad" }); tx.oncomplete = resolve; tx.onabort = () => reject(tx.error); });
    raw.close(); await rejects(() => a.read(first.id), "LIBRARY_CORRUPT"); equal((await a.list()).length, 1);
  });
  return { passed: results.filter(item => item.passed).length, failed: results.filter(item => !item.passed).length, results };
}
