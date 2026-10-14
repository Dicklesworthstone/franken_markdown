import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile, open, rename, realpath } from "node:fs/promises";
import { join, basename } from "node:path";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { createBookFileSession } from "../demo/book_file_session.mjs";

// A real-disk FileSystemFileHandle adapter: File/Blob and all byte reads/writes
// are native. close() syncs a staged file then renames it; abort retains the
// isolated temporary stage without publishing it. No source/module rewriting.
// Hooks suspend or fault actual operations; they do not emulate rendering.
async function diskFile(dir, name, bytes = "") {
  const path = join(dir, name); await writeFile(path, bytes);
  const calls = { reads: 0, permission: 0, writers: 0, writes: 0, closes: 0, aborts: 0 };
  const hooks = {};
  const handle = {
    kind: "file", name,
    async getFile() {
      calls.reads++; await hooks.read?.();
      return new File([await readFile(path)], name, { type: "application/json" });
    },
    async requestPermission(options) {
      calls.permission++; assert.deepEqual(options, { mode: "readwrite" });
      return hooks.permission ? hooks.permission() : "granted";
    },
    async isSameEntry(other) { return await realpath(path) === await realpath(other.path); },
    async createWritable(options) {
      calls.writers++; assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
      await hooks.writer?.();
      const stage = join(dir, `stage-${randomUUID()}`), file = await open(stage, "wx");
      let closed = false;
      return {
        async write(blob) {
          calls.writes++; assert(blob instanceof Blob);
          await file.writeFile(new Uint8Array(await blob.arrayBuffer())); await hooks.write?.();
        },
        async close() {
          calls.closes++; await hooks.close?.();
          await file.sync(); await file.close(); closed = true;
          await rename(stage, path); await hooks.afterClose?.();
        },
        async abort() { calls.aborts++; if (!closed) { await file.close(); closed = true; } },
      };
    },
    path,
  };
  return { handle, calls, hooks, bytes: () => readFile(path), set: value => writeFile(path, value) };
}
const book = title => ({ schemaVersion: 2,
  files: [{ path: "guide/start.md", source: `# ${title}\r\n\r\nA café, 日本語 and 😀.\n{{#include ../parts/shared.txt}}\n` },
    { path: "parts/shared.txt", source: "Shared source\r\n", role: "include" }],
  options: { title, author: "Author", lang: "en", font: "sans", toc: true, pageNumbers: true } });
const serialize = value => JSON.stringify(value) + "\n";
const key = value => JSON.stringify([value.files, value.options]);
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
async function fixture(t) {
  const dir = await mkdtemp(join(tmpdir(), "fmd-book-file-"));
  let project = book("Original"), stamp = 0, blocked = false;
  const events = [];
  let install = value => { project = structuredClone(value); stamp++; };
  const session = createBookFileSession({
    read: () => ({ source: serialize(project), key: key(project), stamp: String(stamp) }),
    decode: async file => {
      const text = new TextDecoder("utf-8", { fatal: true }).decode(await file.arrayBuffer());
      const value = JSON.parse(text);
      if (!Array.isArray(value.files) || !value.options) throw Object.assign(Error("Invalid project"), { code: "INVALID_PROJECT" });
      return { project: value, key: key(value) };
    },
    replace: (value, expected) => { assert.equal(expected, String(stamp)); return install(value); },
    isBlocked: () => blocked,
    onState: value => events.push(value),
  });
  t.after(() => session.dispose());
  return { session, dir, events,
    get project() { return structuredClone(project); },
    change(title = "Edited") { project = book(title); stamp++; session.changed(); },
    silent() { stamp++; },
    blocked(value) { blocked = value; },
    install(fn) { install = fn; },
    file: (name = "book.json", bytes = serialize(project)) => diskFile(dir, name, bytes),
  };
}
const connect = (h, file) => h.session.open(() => Promise.resolve([file.handle]), () => true);

test("opens a real source project without writing and saves a verified revision", async t => {
  const h = await fixture(t), file = await h.file();
  assert.equal((await connect(h, file)).opened, true);
  assert.equal(h.session.state.phase, "saved"); assert.equal(file.calls.permission, 0); assert.equal(file.calls.writers, 0);
  h.change(); assert.equal(h.session.state.canSave, true);
  const result = await h.session.save();
  assert.equal(result.current, true); assert.equal(result.connected, true);
  assert.equal((await file.bytes()).toString(), serialize(h.project));
  assert.equal(h.session.state.phase, "saved"); assert.equal(file.calls.closes, 1);
  assert(!JSON.stringify(h.events).includes(file.handle.path), "state must not leak a path or handle");
});

test("open accepts noncanonical JSON while exact byte baselines detect whitespace-only rewrites", async t => {
  const h = await fixture(t), pretty = JSON.stringify(h.project, null, 2);
  const file = await h.file("pretty.json", pretty); await connect(h, file); h.change();
  await file.set(pretty.replace('  "schemaVersion"', '\t "schemaVersion"'));
  await assert.rejects(h.session.save(), { code: "FILE_CHANGED" });
  assert.equal(file.calls.writers, 0); assert.equal(h.session.state.phase, "conflict");
});

test("BOM and original chapter code units survive saving a connected project", async t => {
  const h = await fixture(t), file = await h.file("bom.json", "\ufeff" + serialize(h.project));
  await connect(h, file); h.change("BOM 😀"); await h.session.save();
  const bytes = await file.bytes(); assert.deepEqual([...bytes.subarray(0, 3)], [239, 187, 191]);
  const value = JSON.parse(new TextDecoder().decode(bytes)); assert.deepEqual(value, h.project);
  assert.match(value.files[0].source, /\r\n/);
});

test("same-sized external source edits are refused and same-file Save as cannot bypass conflict", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  const external = (await file.bytes()).toString().replaceAll("Original", "EXTERNAL"); await file.set(external);
  await assert.rejects(h.session.save(), { code: "FILE_CHANGED" });
  await assert.rejects(h.session.saveAs(() => ({ ...file.handle }), () => true), { code: "FILE_CHANGED" });
  assert.equal((await file.bytes()).toString(), external); assert.equal(file.calls.writers, 0);
});

test("explicit reload resolves conflict only after confirmation and preserves external source", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  await file.set(serialize(book("Outside"))); await assert.rejects(h.session.save(), { code: "FILE_CHANGED" });
  assert.equal((await h.session.reload(() => false)).opened, false); assert.equal(h.project.options.title, "Edited");
  await h.session.reload(() => true); assert.equal(h.project.options.title, "Outside");
  assert.equal(h.session.state.phase, "saved");
});

test("declining open keeps both current book and existing file connection", async t => {
  const h = await fixture(t), a = await h.file(), b = await h.file("other.json", serialize(book("Other")));
  await connect(h, a); h.change();
  assert.equal((await h.session.open(() => [b.handle], () => false)).opened, false);
  assert.equal(h.project.options.title, "Edited"); assert.equal(h.session.state.filename, "book.json");
  await h.session.save(); assert.equal(JSON.parse((await a.bytes()).toString()).options.title, "Edited");
});

test("invalid JSON never installs or acquires a connection", async t => {
  const h = await fixture(t), file = await h.file("bad.json", "not JSON");
  await assert.rejects(connect(h, file), { code: "FILE_OPERATION_FAILED" });
  assert.equal(h.project.options.title, "Original"); assert.equal(h.session.state.filename, null);
});

test("an edit during a picker retires that open without installing old source", async t => {
  const h = await fixture(t), file = await h.file("other.json", serialize(book("Other"))), wait = deferred();
  const promise = h.session.open(() => wait.promise, () => true); h.change("Newer"); wait.resolve([file.handle]);
  await assert.rejects(promise, { code: "STALE_SOURCE" }); assert.equal(h.project.options.title, "Newer");
});

test("confirmation awaits are fenced against both user edits and external disk edits", async t => {
  for (const local of [true, false]) {
    const h = await fixture(t), file = await h.file();
    const promise = h.session.open(() => [file.handle], async () => {
      if (local) h.change("Newer"); else await file.set(serialize(book("Outside")));
      return true;
    });
    await assert.rejects(promise, { code: local ? "STALE_SOURCE" : "FILE_CHANGED" });
    assert.equal(h.session.state.filename, null);
    assert.equal(h.project.options.title, local ? "Newer" : "Original");
  }
});

test("uncaptured host stamp changes prevent disk writes even without a change notification", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  file.hooks.permission = () => { h.silent(); return "granted"; };
  await assert.rejects(h.session.save(), { code: "STALE_SOURCE" }); assert.equal(file.calls.writers, 0);
});

test("editing during a real staged write aborts publication and retains disk baseline", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); const before = await file.bytes(); h.change();
  file.hooks.write = () => h.change("Newer");
  await assert.rejects(h.session.save(), { code: "STALE_SOURCE" });
  assert.deepEqual(await file.bytes(), before); assert.equal(file.calls.closes, 0); assert.equal(file.calls.aborts, 1);
});

test("external edits during staged writes are caught before close", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  const outside = serialize(book("Outside")); file.hooks.write = () => file.set(outside);
  await assert.rejects(h.session.save(), { code: "FILE_CHANGED" });
  assert.equal((await file.bytes()).toString(), outside); assert.equal(file.calls.closes, 0); assert.equal(file.calls.aborts, 1);
});

test("edits after close begins survive and only the captured disk revision becomes the new baseline", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change("Captured");
  file.hooks.close = () => h.change("Newer");
  const result = await h.session.save(); assert.equal(result.saved, true); assert.equal(result.current, false);
  assert.equal(h.project.options.title, "Newer"); assert.equal(JSON.parse((await file.bytes()).toString()).options.title, "Captured");
  assert.equal(h.session.state.phase, "edited"); file.hooks.close = null; await h.session.save();
  assert.equal(JSON.parse((await file.bytes()).toString()).options.title, "Newer");
});

test("detach during close cannot reconnect the obsolete project", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  file.hooks.close = () => h.session.detach(); const result = await h.session.save();
  assert.equal(result.saved, true); assert.equal(result.connected, false); assert.equal(h.session.state.filename, null);
});

test("Save as to another file adopts only a still-current verified revision", async t => {
  const h = await fixture(t), old = await h.file(), next = await h.file("copy.json", ""); await connect(h, old); h.change("Captured");
  next.hooks.close = () => h.change("Newer"); const result = await h.session.saveAs(() => next.handle);
  assert.equal(result.saved, true); assert.equal(result.connected, false); assert.equal(h.session.state.filename, "book.json");
  assert.equal(JSON.parse((await next.bytes()).toString()).options.title, "Captured");
  await h.session.save(); assert.equal(JSON.parse((await old.bytes()).toString()).options.title, "Newer");
});

test("Save as creates and connects an empty chosen file without a redundant overwrite prompt", async t => {
  const h = await fixture(t), file = await h.file("new.json", ""); let prompts = 0;
  const result = await h.session.saveAs(() => file.handle, () => { prompts++; return true; });
  assert.equal(result.current, true); assert.equal(prompts, 0); assert.equal(h.session.state.filename, "new.json");
  assert.deepEqual(JSON.parse((await file.bytes()).toString()), h.project);
});

test("nonempty Save as needs explicit consent and can replace malformed JSON only with that consent", async t => {
  const h = await fixture(t), file = await h.file("replace.json", new Uint8Array([255, 0, 1]));
  assert.equal((await h.session.saveAs(() => file.handle)).saved, false);
  assert.deepEqual([...await file.bytes()], [255, 0, 1]); assert.equal(file.calls.writers, 0);
  await h.session.saveAs(() => file.handle, () => true);
  assert.deepEqual(JSON.parse((await file.bytes()).toString()), h.project);
});

test("denied permission and exclusive-writer conflicts never begin a disk commit", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  file.hooks.permission = () => "denied"; await assert.rejects(h.session.save(), { code: "FILE_PERMISSION_DENIED" });
  assert.equal(file.calls.writers, 0); file.hooks.permission = null;
  file.hooks.writer = () => { throw new DOMException("locked", "NoModificationAllowedError"); };
  await assert.rejects(h.session.save(), { code: "FILE_LOCKED" }); assert.equal(file.calls.closes, 0);
});

test("read-back corruption is uncertain, blocks implicit retry, and permits explicit recovery to a new file", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  file.hooks.afterClose = () => file.set(serialize(book("Concurrent")));
  await assert.rejects(h.session.save(), { code: "FILE_SAVE_UNCERTAIN" }); assert.equal(h.session.state.phase, "uncertain");
  const writers = file.calls.writers; await assert.rejects(h.session.save(), { code: "FILE_CHANGED" }); assert.equal(file.calls.writers, writers);
  const other = await h.file("recovered.json", ""); await h.session.saveAs(() => other.handle);
  assert.equal(h.session.state.phase, "saved"); assert.equal(h.session.state.filename, "recovered.json");
});

test("post-close read failure never claims a saved project", async t => {
  const h = await fixture(t), file = await h.file(); await connect(h, file); h.change();
  file.hooks.afterClose = () => { file.hooks.read = () => { throw new DOMException("unavailable", "NotFoundError"); }; };
  await assert.rejects(h.session.save(), { code: "FILE_SAVE_UNCERTAIN" }); assert.equal(h.session.state.canSave, false);
});

test("operations serialize while a picker is pending; cancellation is explicit", async t => {
  const h = await fixture(t), wait = deferred(), file = await h.file();
  const promise = h.session.open(() => wait.promise, () => true);
  await assert.rejects(h.session.saveAs(() => file.handle), { code: "FILE_BUSY" });
  wait.resolve([file.handle]); await promise;
  await assert.rejects(h.session.open(() => { throw new DOMException("cancel", "AbortError"); }), { code: "FILE_CANCELLED" });
  assert.equal(h.session.state.filename, "book.json");
});

test("composition blocks all file operations without opening pickers", async t => {
  const h = await fixture(t); h.blocked(true); let picks = 0;
  await assert.rejects(h.session.open(() => { picks++; return []; }), { code: "FILE_BUSY" }); assert.equal(picks, 0);
});

test("bounded Unicode snapshots are checked before any picker or disk access", async () => {
  for (const source of ["\ud800", "ééé", "a".repeat(6)]) {
    let picks = 0;
    const session = createBookFileSession({ read: () => ({ source, key: "k", stamp: "s" }), decode() {}, replace() {}, maxBytes: 5 });
    await assert.rejects(session.saveAs(() => { picks++; }), { code: source === "\ud800" ? "INVALID_UNICODE" : source.length > 5 ? "INVALID_PROJECT" : "BOOK_LIMIT" });
    assert.equal(picks, 0); session.dispose();
  }
});

test("host installation mismatch cannot authorize future overwrites", async t => {
  const h = await fixture(t), file = await h.file("other.json", serialize(book("Other")));
  h.install(() => {}); await assert.rejects(connect(h, file), { code: "STALE_SOURCE" });
  assert.equal(h.session.state.filename, null); assert.equal(file.calls.writers, 0);
});

test("disposal retires pending pickers and never installs their late result", async t => {
  const h = await fixture(t), file = await h.file(), wait = deferred();
  const promise = h.session.open(() => wait.promise, () => true); h.session.dispose(); wait.resolve([file.handle]);
  await assert.rejects(promise, { code: "SESSION_DISPOSED" }); assert.equal(h.session.state.phase, "disposed");
  assert.equal(h.session.state.filename, null); assert.equal(file.calls.writers, 0);
});
