import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookDirectorySession, BOOK_DIRECTORY_LIMITS } from "../demo/book_directory_session.mjs";

// File/Blob and encoding are real Node implementations. Only the browser's
// permission/handle/staged-writer boundary is substituted, not production code.
const encoder = new TextEncoder();
const decode = bytes => new TextDecoder("utf-8", { ignoreBOM: true }).decode(bytes);
const yes = () => true;
const error = name => Object.assign(new Error(name), { name });
class Disk {
  files = new Map(); dirs = new Set([""]); log = []; hook = async () => {}; grant = "granted";
  constructor(files = {}) {
    for (const [path, source] of Object.entries(files)) this.put(path, source);
    this.root = this.directory("");
  }
  put(path, source) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i++) this.dirs.add(parts.slice(0, i).join("/"));
    const entry = { bytes: typeof source === "string" ? encoder.encode(source) : new Uint8Array(source), locked: false };
    this.files.set(path, entry); return entry;
  }
  text(path) { return decode(this.files.get(path).bytes); }
  async call(op, path, extra) { this.log.push([op, path]); await this.hook(op, path, extra); }
  directory(path) {
    const fs = this, child = name => path ? `${path}/${name}` : name;
    return { kind: "directory", name: path.split("/").at(-1) || "sources",
      async requestPermission(value) { assert.deepEqual(value, { mode: "readwrite" }); await fs.call("permission", path); return fs.grant; },
      async getDirectoryHandle(name, { create = false } = {}) {
        const target = child(name); await fs.call(create ? "mkdir" : "directory", target);
        if (!fs.dirs.has(target)) { if (!create) throw error("NotFoundError"); fs.dirs.add(target); }
        return fs.directory(target);
      },
      async getFileHandle(name, { create = false } = {}) {
        const target = child(name); await fs.call(create ? "create" : "locate", target);
        if (!fs.files.has(target)) { if (!create) throw error("NotFoundError"); fs.put(target, ""); }
        return fs.handle(target, fs.files.get(target));
      },
      entries() { throw Error("Directory enumeration is not authorized"); },
      removeEntry() { throw Error("No deletion is authorized"); },
    };
  }
  handle(path, entry) {
    const fs = this;
    return { kind: "file", name: path.split("/").at(-1), entry,
      async isSameEntry(other) { await fs.call("identity", path); return entry === other.entry; },
      async getFile() { await fs.call("read", path); return new File([entry.bytes], path.split("/").at(-1)); },
      async createWritable(options) {
        assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
        await fs.call("writer", path); if (entry.locked) throw error("NoModificationAllowedError");
        entry.locked = true; let bytes = null, aborted = false;
        return {
          async write(blob) { await fs.call("write", path); assert(blob instanceof Blob); bytes = new Uint8Array(await blob.arrayBuffer()); },
          async close() {
            try {
              await fs.call("close", path); if (aborted) throw error("AbortError");
              entry.bytes = bytes; await fs.call("closed", path);
            } finally { entry.locked = false; }
          },
          async abort() { await fs.call("abort", path); aborted = true; entry.locked = false; },
        };
      },
    };
  }
}
function fixture(t, diskFiles = { "a.md": "A", "part/code.rs": "fn main() {}", "b.md": "B" }, limits) {
  const disk = new Disk(diskFiles);
  let files = Object.entries(diskFiles).map(([path, source]) => ({ path, source: typeof source === "string" ? source : decode(source) })), stamp = "0";
  const notices = [];
  const session = createBookDirectorySession({ read: () => ({ files, stamp }), limits, onState: value => notices.push(value) });
  t.after(() => session.dispose());
  return { disk, session, notices, get files() { return files; },
    edit(path, source, dispatch = true) { files = files.map(file => file.path === path ? { path, source } : file); stamp += "x"; if (dispatch) session.changed(); },
    setFiles(value) { files = value; stamp += "x"; session.changed(); },
    connect(confirm = yes) { return session.bind(() => disk.root, confirm); },
  };
}
const writes = disk => disk.log.filter(([op]) => ["writer", "create", "mkdir", "close"].includes(op));

test("binding reviews only exact known paths, exposes no handles, and never writes", async t => {
  const h = fixture(t); h.disk.put("private.txt", "not in book"); h.edit("a.md", "edited");
  let review;
  await h.connect(value => { review = value; return true; });
  assert.equal(review.files.find(file => file.path === "a.md").action, "replace");
  assert(Object.isFrozen(review)); assert(Object.isFrozen(review.files));
  assert.equal(JSON.stringify(review).includes("handle"), false);
  assert.deepEqual(writes(h.disk), []); assert(!h.disk.log.some(([, path]) => path === "private.txt"));
  assert.equal(h.session.state.phase, "edited"); assert(h.session.state.canSave);
});

test("an edited multi-source book saves exact UTF-8, BOM and mixed newlines", async t => {
  const h = fixture(t); await h.connect();
  h.edit("a.md", "\ufeff# 日本 😀\r\nA\rB\n"); h.edit("part/code.rs", "fn main() {\r\n}\r\n");
  const result = await h.session.save(yes);
  assert.equal(result.outcome, "saved"); assert.equal(result.verified, 2); assert(result.current);
  for (const file of h.files) assert.equal(h.disk.text(file.path), file.source);
  assert.equal(h.session.state.phase, "saved"); assert.equal(h.session.state.canSave, false);
  assert(!h.disk.log.some(([op, path]) => op === "writer" && path === "b.md"));
});

test("all modified writers are staged and the entire set is rechecked before the first close", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "new A"); h.edit("b.md", "new B");
  h.disk.log = []; await h.session.save(yes);
  const firstClose = h.disk.log.findIndex(([op]) => op === "close");
  assert(h.disk.log.findIndex(([op, path]) => op === "write" && path === "b.md") < firstClose);
  const lastStage = h.disk.log.findLastIndex(([op]) => op === "write");
  assert(h.disk.log.slice(lastStage, firstClose).some(([op, path]) => op === "read" && path === "part/code.rs"));
});

test("new nested sources are created only on explicitly approved save", async t => {
  const h = fixture(t); h.setFiles([...h.files, { path: "guide/new.md", source: "# New\n" }]);
  await h.connect(); assert(!h.disk.dirs.has("guide")); assert(!h.disk.files.has("guide/new.md"));
  const result = await h.session.save(yes);
  assert.equal(h.disk.text("guide/new.md"), "# New\n"); assert.equal(result.verified, 1);
  assert.deepEqual(result.createdDirectories, ["guide"]); assert.equal(result.files[0].status, "verified");
});

test("an initially empty source is created instead of mistaken for an unchanged missing file", async t => {
  const h = fixture(t); h.setFiles([{ path: "blank.md", source: "" }]); await h.connect();
  const receipt = await h.session.save(yes); assert.equal(receipt.verified, 1); assert(h.disk.files.has("blank.md"));
});

test("unchanged saves perform disk checks without asking write permission or opening streams", async t => {
  const h = fixture(t); await h.connect(); h.disk.log = [];
  const result = await h.session.save(() => { throw Error("No write confirmation needed"); });
  assert.equal(result.outcome, "unchanged"); assert.equal(result.verified, 0);
  assert(!h.disk.log.some(([op]) => op === "permission")); assert.deepEqual(writes(h.disk), []);
});

test("external changes in even an unchanged source prevent every write", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours");
  h.disk.files.get("b.md").bytes = encoder.encode("external"); h.disk.log = [];
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_CHANGED" });
  assert.deepEqual(writes(h.disk), []); assert.equal(h.disk.text("a.md"), "A");
  assert.equal(h.session.state.phase, "conflict"); assert(!h.session.state.canSave);
});

test("a disk mutation after staging aborts all streams without closing a file", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours A"); h.edit("b.md", "ours B");
  h.disk.hook = async (op, path) => { if (op === "write" && path === "b.md") h.disk.files.get("a.md").bytes = encoder.encode("external"); };
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_CHANGED" });
  assert(!h.disk.log.some(([op]) => op === "close")); assert.equal(h.disk.text("a.md"), "external");
  assert.equal(h.disk.text("b.md"), "B"); assert.equal(h.disk.log.filter(([op]) => op === "abort").length, 2);
});

test("a newly appearing target is never overwritten under an absent baseline", async t => {
  const h = fixture(t); h.setFiles([{ path: "new.md", source: "ours" }]); await h.connect();
  h.disk.put("new.md", "external");
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_CHANGED" }); assert.equal(h.disk.text("new.md"), "external");
});

test("nonempty content racing creation is not overwritten", async t => {
  const h = fixture(t); h.setFiles([{ path: "new.md", source: "ours" }]); await h.connect();
  h.disk.hook = async op => { if (op === "create") h.disk.put("new.md", "external"); };
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_CHANGED" }); assert.equal(h.disk.text("new.md"), "external");
});

test("failure on a later close exposes verified and uncertain files and blocks silent retry", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "first"); h.edit("b.md", "second");
  h.disk.hook = async (op, path) => { if (op === "closed" && path === "b.md") throw error("QuotaExceededError"); };
  await assert.rejects(h.session.save(yes), error => {
    assert.equal(error.code, "DIRECTORY_SAVE_UNCERTAIN"); assert.equal(error.receipt.outcome, "uncertain");
    assert.deepEqual(error.receipt.files.map(file => file.status), ["verified", "uncertain"]); return true;
  });
  assert.equal(h.disk.text("a.md"), "first"); assert.equal(h.disk.text("b.md"), "second");
  assert.equal(h.session.state.phase, "uncertain");
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_REVIEW_REQUIRED" });
});

test("read-back mismatch never reports a successful save", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours");
  h.disk.hook = async (op, path) => { if (op === "closed") h.disk.files.get(path).bytes = encoder.encode("different"); };
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_SAVE_UNCERTAIN" });
  assert.equal(h.session.state.receipt.verified, 0);
});

test("replacement of the target path during close cannot be called verified", async t => {
  const h = fixture(t); await h.connect(); h.edit("part/code.rs", "ours");
  h.disk.hook = async (op, path) => { if (op === "closed") h.disk.put(path, "external replacement"); };
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_SAVE_UNCERTAIN" });
  assert.equal(h.disk.text("part/code.rs"), "external replacement");
});

test("editing while staging stops all unclosed writes, including uncaptured host changes", async t => {
  for (const dispatch of [true, false]) {
    const h = fixture(t); await h.connect(); h.edit("a.md", "captured");
    h.disk.hook = async op => { if (op === "write") h.edit("a.md", "newer", dispatch); };
    await assert.rejects(h.session.save(yes), { code: "STALE_SOURCE" });
    assert.equal(h.disk.text("a.md"), "A"); assert.equal(h.files[0].source, "newer");
    assert(!h.disk.log.some(([op]) => op === "close"));
  }
});

test("editing during one close yields a partial receipt and never commits the remaining sources", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "captured A"); h.edit("b.md", "captured B");
  h.disk.hook = async (op, path) => { if (op === "closed" && path === "a.md") h.edit("b.md", "newer B"); };
  await assert.rejects(h.session.save(yes), error => {
    assert.equal(error.code, "STALE_SOURCE"); assert.equal(error.receipt.outcome, "partial"); assert.equal(error.receipt.verified, 1); return true;
  });
  assert.equal(h.disk.text("a.md"), "captured A"); assert.equal(h.disk.text("b.md"), "B");
  h.disk.hook = async () => {}; const next = await h.session.save(yes);
  assert.deepEqual(next.files.map(file => file.path), ["b.md"]); assert.equal(h.disk.text("b.md"), "newer B");
});

test("editing during the last close reports the captured revision without marking newer source saved", async t => {
  const h = fixture(t); await h.connect(); h.edit("part/code.rs", "captured");
  h.disk.hook = async op => { if (op === "closed") h.edit("part/code.rs", "newer"); };
  const result = await h.session.save(yes);
  assert.equal(result.verified, 1); assert.equal(result.current, false);
  assert.equal(h.disk.text("part/code.rs"), "captured"); assert.equal(h.session.state.phase, "edited");
});

test("cancelled creation records possible leftovers rather than claiming rollback", async t => {
  const h = fixture(t); h.setFiles([{ path: "new/file.md", source: "ours" }]); await h.connect();
  h.disk.hook = async op => { if (op === "write") h.session.changed(); };
  await assert.rejects(h.session.save(yes), error => {
    assert.equal(error.receipt.files[0].status, "created-unverified"); assert.deepEqual(error.receipt.createdDirectories, ["new"]); return true;
  });
  assert.equal(h.disk.text("new/file.md"), ""); assert.equal(h.session.state.phase, "needs-review");
  assert.match(h.session.state.message, /No rollback or deletion/);
});

test("permission denial and declined save leave bytes and connection retryable", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); h.disk.grant = "denied";
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_PERMISSION_DENIED" });
  h.disk.grant = "granted"; assert.equal(await h.session.save(() => false), null);
  assert.equal(h.disk.text("a.md"), "A"); assert(h.session.state.canSave);
  await h.session.save(yes); assert.equal(h.disk.text("a.md"), "ours");
});

test("both connect and write require explicit affirmative confirmation", async t => {
  const h = fixture(t); assert.equal(await h.session.bind(() => h.disk.root), false);
  assert.equal(h.session.state.folder, null); await h.connect(); h.edit("a.md", "ours");
  assert.equal(await h.session.save(), null); assert.equal(h.disk.text("a.md"), "A");
});

test("a failed or declined different-folder review preserves the prior connection", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); const other = new Disk({ "a.md": "different" });
  assert.equal(await h.session.bind(() => other.root, () => false), false);
  await assert.rejects(h.session.bind(() => other.root, () => { other.put("b.md", "raced"); return true; }), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.session.state.phase, "edited"); await h.session.save(yes); assert.equal(h.disk.text("a.md"), "ours");
});

test("changes during binding confirmation cannot authorize a stale source snapshot", async t => {
  const h = fixture(t);
  await assert.rejects(h.connect(() => { h.edit("a.md", "newer", false); return true; }), { code: "STALE_SOURCE" });
  assert.equal(h.session.state.folder, null); assert.deepEqual(writes(h.disk), []);
});

test("reordering keeps the connection but added, removed or renamed paths require review", async t => {
  const h = fixture(t); await h.connect(); h.setFiles(h.files.slice().reverse()); assert.equal(h.session.state.phase, "saved");
  const original = h.files;
  for (const files of [original.slice(1), [...original, { path: "c.md", source: "C" }], original.map((f, i) => i ? f : { ...f, path: "renamed.md" })]) {
    h.setFiles(files); assert.equal(h.session.state.phase, "needs-review");
    await assert.rejects(h.session.save(yes), { code: "DIRECTORY_REVIEW_REQUIRED" });
  }
  assert(h.disk.files.has("b.md")); assert.deepEqual(writes(h.disk), []);
});

test("explicit re-review resolves a conflict without writing or deleting files", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); h.disk.put("a.md", "external");
  await assert.rejects(h.session.save(yes)); let reviewed;
  await h.connect(value => { reviewed = value; return true; });
  assert.equal(reviewed.files[0].action, "replace"); assert.equal(h.disk.text("a.md"), "external");
  await h.session.save(yes); assert.equal(h.disk.text("a.md"), "ours");
});

test("malformed Unicode, nonportable paths and prefix collisions fail before opening a picker", async () => {
  for (const files of [
    [{ path: "../escape.md", source: "x" }], [{ path: "/a.md", source: "x" }], [{ path: "a%2Fb.md", source: "x" }],
    [{ path: "NUL.txt", source: "x" }], [{ path: "a.md", source: "\ud800" }], [{ path: "bad\udc00.md", source: "x" }],
    [{ path: "a.md", source: "x" }, { path: "A.md", source: "y" }],
    [{ path: "é.md", source: "x" }, { path: "e\u0301.md", source: "y" }],
    [{ path: "a.md", source: "x" }, { path: "A.md/b.md", source: "y" }],
  ]) {
    let picks = 0; const session = createBookDirectorySession({ read: () => ({ files, stamp: "0" }) });
    await assert.rejects(session.bind(() => { picks++; }, yes)); assert.equal(picks, 0); session.dispose();
  }
});

test("actual handle aliases are refused even when their path spellings are unrelated", async t => {
  const h = fixture(t); h.disk.files.set("b.md", h.disk.files.get("a.md"));
  await assert.rejects(h.connect(), { code: "PATH_COLLISION" }); assert.deepEqual(writes(h.disk), []);
});

test("non-UTF-8 disk files cannot be selected as overwrite targets", async t => {
  const h = fixture(t); h.disk.files.get("a.md").bytes = new Uint8Array([255]);
  await assert.rejects(h.connect(), { code: "INVALID_UNICODE" }); assert.deepEqual(writes(h.disk), []);
});

test("count and UTF-8 byte budgets admit neither oversized source nor oversized disk sets", async () => {
  const cases = [
    { limits: { files: 1 }, files: [{ path: "a", source: "" }, { path: "b", source: "" }] },
    { limits: { fileBytes: 3 }, files: [{ path: "a", source: "😀" }] },
    { limits: { pathBytes: 3 }, files: [{ path: "日本", source: "" }] },
    { limits: { sourceBytes: 4 }, files: [{ path: "a", source: "1234" }] },
  ];
  for (const { files, limits } of cases) {
    let picks = 0; const session = createBookDirectorySession({ read: () => ({ files, stamp: "0" }), limits });
    await assert.rejects(session.bind(() => { picks++; }, yes)); assert.equal(picks, 0); session.dispose();
  }
  const disk = new Disk({ a: "123456", b: "123456" });
  const session = createBookDirectorySession({ read: () => ({ files: [{ path: "a", source: "" }, { path: "b", source: "" }], stamp: "0" }), limits: { sourceBytes: 10 } });
  await assert.rejects(session.bind(() => disk.root, yes), { code: "BOOK_LIMIT" }); session.dispose();
});

test("caller-owned data is captured once and accessor sources are not evaluated", async () => {
  let called = 0;
  const session = createBookDirectorySession({ read: () => ({ files: [{ path: "a.md", get source() { called++; return "x"; } }], stamp: "0" }) });
  await assert.rejects(session.bind(() => {}, yes), { code: "INVALID_SOURCE" }); assert.equal(called, 0); session.dispose();
});

test("disconnect and disposal retire in-flight review and clear handle ownership", async t => {
  for (const dispose of [false, true]) {
    const h = fixture(t); let release;
    const pending = h.session.bind(() => new Promise(resolve => { release = resolve; }), yes);
    if (dispose) h.session.dispose(); else h.session.detach(); release(h.disk.root);
    await assert.rejects(pending, { code: dispose ? "SESSION_DISPOSED" : "STALE_SOURCE" });
    assert.equal(h.session.state.folder, null); assert.deepEqual(writes(h.disk), []);
  }
});

test("a late writer acquired after disconnect is aborted rather than leaked or committed", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours");
  h.disk.hook = async op => { if (op === "writer") h.session.detach(); };
  await assert.rejects(h.session.save(yes), { code: "STALE_SOURCE" });
  assert(h.disk.log.some(([op]) => op === "abort")); assert.equal(h.disk.files.get("a.md").locked, false);
  assert.equal(h.disk.text("a.md"), "A");
});

test("overlapping operations and host composition are rejected without starting another picker", async t => {
  const h = fixture(t); let release;
  const pending = h.session.bind(() => new Promise(resolve => { release = resolve; }), yes);
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_BUSY" }); release(h.disk.root); await pending;
  const blocked = createBookDirectorySession({ read: () => ({ files: h.files, stamp: "0" }), isBlocked: () => true });
  await assert.rejects(blocked.bind(() => { throw Error("must not pick"); }, yes), { code: "DIRECTORY_BUSY" }); blocked.dispose();
});

test("exclusive-writer refusal leaves every original file unchanged and retryable", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); h.disk.files.get("a.md").locked = true;
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_LOCKED" }); assert.equal(h.disk.text("a.md"), "A");
  h.disk.files.get("a.md").locked = false; await h.session.save(yes); assert.equal(h.disk.text("a.md"), "ours");
});

test("invalid limits, empty books and unavailable pickers return explicit errors", async () => {
  const read = () => ({ files: [{ path: "a.md", source: "A" }], stamp: "0" });
  for (const limits of [{ files: 0 }, { fileBytes: BOOK_DIRECTORY_LIMITS.fileBytes + 1 }, { sourceBytes: -1 }])
    assert.throws(() => createBookDirectorySession({ read, limits }), { code: "INVALID_ARGUMENT" });
  const session = createBookDirectorySession({ read });
  await assert.rejects(session.bind(null, yes), { code: "DIRECTORY_ACCESS_UNAVAILABLE" }); session.dispose();
  const empty = createBookDirectorySession({ read: () => ({ files: [], stamp: "0" }) });
  await assert.rejects(empty.bind(() => {}, yes), { code: "NO_SOURCE_FILES" }); empty.dispose();
});
