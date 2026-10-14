import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookDirectorySession, BookDirectoryError } from "../demo/book_directory_session.mjs";

// Normal imports execute the complete production session. Browser file handles
// and the atomic source host are adapters; File/Blob/UTF-8 execute in Node.
const encoder = new TextEncoder();
const decode = bytes => new TextDecoder("utf-8", { ignoreBOM: true }).decode(bytes);
const browserError = name => Object.assign(new Error(name), { name });
class Disk {
  files = new Map(); dirs = new Set([""]); log = []; hook = async () => {}; grant = "granted";
  constructor(files = {}) { for (const [path, source] of Object.entries(files)) this.put(path, source); this.root = this.directory(""); }
  put(path, source) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i++) this.dirs.add(parts.slice(0, i).join("/"));
    const entry = { bytes: encoder.encode(source), locked: false }; this.files.set(path, entry); return entry;
  }
  text(path) { return decode(this.files.get(path).bytes); }
  async call(op, path) { this.log.push([op, path]); await this.hook(op, path); }
  directory(path) {
    const disk = this, child = name => path ? `${path}/${name}` : name;
    return { kind: "directory", name: path.split("/").at(-1) || "sources",
      async requestPermission(options) { assert.deepEqual(options, { mode: "readwrite" }); await disk.call("permission", path); return disk.grant; },
      async getDirectoryHandle(name, { create = false } = {}) {
        const next = child(name); await disk.call(create ? "mkdir" : "directory", next);
        if (!disk.dirs.has(next)) { if (!create) throw browserError("NotFoundError"); disk.dirs.add(next); }
        return disk.directory(next);
      },
      async getFileHandle(name, { create = false } = {}) {
        const next = child(name); await disk.call(create ? "create" : "locate", next);
        if (!disk.files.has(next)) { if (!create) throw browserError("NotFoundError"); disk.put(next, ""); }
        return disk.handle(next, disk.files.get(next));
      },
      entries() { throw Error("No enumeration"); }, removeEntry() { throw Error("No deletion"); },
    };
  }
  handle(path, entry) {
    const disk = this;
    return { kind: "file", name: path.split("/").at(-1), entry,
      async isSameEntry(other) { await disk.call("identity", path); return entry === other.entry; },
      async getFile() { await disk.call("read", path); return new File([entry.bytes], path.split("/").at(-1)); },
      async createWritable(options) {
        assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
        await disk.call("writer", path); if (entry.locked) throw browserError("NoModificationAllowedError");
        entry.locked = true; let bytes, aborted = false;
        return {
          async write(blob) { await disk.call("write", path); assert(blob instanceof Blob); bytes = new Uint8Array(await blob.arrayBuffer()); },
          async close() { try { await disk.call("close", path); if (aborted) throw browserError("AbortError"); entry.bytes = bytes; await disk.call("closed", path); } finally { entry.locked = false; } },
          async abort() { await disk.call("abort", path); aborted = true; entry.locked = false; },
        };
      },
    };
  }
}

const yes = () => true;
const writes = disk => disk.log.filter(([op]) => ["permission", "writer", "create", "mkdir", "close"].includes(op));
function fixture(t, { diskFiles = { "one.md": "one", "two.md": "two" }, sources, limits } = {}) {
  const disk = new Disk(diskFiles);
  let files = sources ?? Object.entries(diskFiles).map(([path, source]) => ({ path, source })), revision = 0, applied = 0;
  const stamp = () => String(revision);
  const session = createBookDirectorySession({ read: () => ({ files, stamp: stamp() }), limits });
  const install = (next, expected) => {
    assert.equal(expected, stamp()); applied++; files = next.map(file => ({ ...file })); revision++; session.changed(); return stamp();
  };
  t.after(() => session.dispose());
  return { disk, session, install, get files() { return files; }, get applied() { return applied; }, stamp,
    edit(path, source, notify = true) { files = files.map(file => file.path === path ? { path, source } : file); revision++; if (notify) session.changed(); },
    setFiles(next) { files = next; revision++; session.changed(); },
    bind: () => session.bind(() => disk.root, yes),
    refresh: (confirm = yes, apply = install) => session.refresh(confirm, apply),
  };
}

const edited = (path, source) => ({ path, choice: "edited", source });
const resolve = (h, choices, apply = h.install) => h.session.reconcile(() => choices, apply);
async function conflict(t, options) {
  const h = fixture(t, options); await h.bind(); h.edit("one.md", "ours"); h.disk.put("one.md", "theirs"); return h;
}

test("edited conflict results install once together with independent line merges and disk-only sources", async t => {
  const h = fixture(t, { diskFiles: { "one.md": "one", "two.md": "a\nb\nc\n", "include.txt": "base" } });
  await h.bind(); h.edit("one.md", "ours"); h.disk.put("one.md", "theirs");
  h.edit("two.md", "LOCAL\nb\nc\n"); h.disk.put("two.md", "a\nb\nDISK\n"); h.disk.put("include.txt", "disk-only");
  const result = await h.session.reconcile(review => {
    assert.equal(review.conflicts, 1); assert.equal(review.files[1].action, "merged");
    assert.equal(review.files[0].local, "ours"); assert.equal(review.files[0].disk, "theirs");
    assert.equal(h.applied, 0); return [edited("one.md", "reviewed combination")];
  }, h.install);
  assert.deepEqual(h.files, [{ path: "one.md", source: "reviewed combination" },
    { path: "two.md", source: "LOCAL\nb\nDISK\n" }, { path: "include.txt", source: "disk-only" }]);
  assert.equal(h.applied, 1); assert.equal(result.changedSources, 3); assert.equal(result.files[0].choice, "edited");
  assert(!Object.hasOwn(result.files[0], "source")); assert.equal(h.session.state.dirty, 2); assert.deepEqual(writes(h.disk), []);
  const saved = await h.session.save(yes); assert.equal(saved.verified, 2);
  for (const file of h.files) assert.equal(h.disk.text(file.path), file.source);
});

for (const text of ["", "\ufeff# 日本 😀\r\nA\rB\n", "ours", "theirs"])
  test(`edited result preserves exact text ${JSON.stringify(text)}`, async t => {
    const h = await conflict(t); const result = await resolve(h, [edited("one.md", text)]);
    assert.equal(h.files[0].source, text); assert.equal(h.disk.text("one.md"), "theirs");
    assert.equal(result.changedSources, text === "ours" ? 0 : 1);
    assert.equal(h.applied, result.changedSources); assert.deepEqual(writes(h.disk), []);
    await h.session.save(yes); assert.equal(h.disk.text("one.md"), text);
  });

test("a missing source can be explicitly edited without deleting or creating disk files", async t => {
  const h = fixture(t); await h.bind(); h.disk.files.delete("one.md");
  await resolve(h, [edited("one.md", "recovered source")]);
  assert.equal(h.files[0].source, "recovered source"); assert(!h.disk.files.has("one.md"));
  assert.deepEqual(writes(h.disk), []); await h.session.save(yes); assert.equal(h.disk.text("one.md"), "recovered source");
});

test("mixed local, disk and edited decisions preserve membership/order and use one transaction", async t => {
  const h = fixture(t, { diskFiles: { "one.md": "one", "two.md": "two", "three.md": "three" } }); await h.bind();
  for (const file of h.files) { h.edit(file.path, "ours"); h.disk.put(file.path, "theirs"); }
  await resolve(h, [{ path: "three.md", choice: "disk" }, edited("two.md", "combined"), { path: "one.md", choice: "local" }]);
  assert.deepEqual(h.files.map(file => file.source), ["ours", "combined", "theirs"]); assert.equal(h.applied, 1);
});

test("edited source must be an owned string without invoking accessors", async t => {
  let calls = 0;
  const inherited = Object.assign(Object.create({ source: "inherited" }), { path: "one.md", choice: "edited" });
  for (const item of [{ path: "one.md", choice: "edited" }, inherited,
    { path: "one.md", choice: "edited", get source() { calls++; return "getter"; } },
    edited("one.md", null), edited("one.md", 12), edited("one.md", { toString() { calls++; return "coerced"; } })]) {
    const h = await conflict(t); await assert.rejects(resolve(h, [item]));
    assert.equal(h.applied, 0); assert.equal(h.files[0].source, "ours"); assert.deepEqual(writes(h.disk), []);
  }
  assert.equal(calls, 0);
});

test("edited decisions cannot target unreviewed, duplicate or nonconflicting paths", async t => {
  for (const choices of [[edited("other.md", "x")], [edited("two.md", "x")],
    [edited("one.md", "x"), edited("one.md", "y")], []]) {
    const h = await conflict(t); await assert.rejects(resolve(h, choices), { code: "UNRESOLVED_SOURCE_CONFLICT" });
    assert.equal(h.applied, 0); assert.equal(h.files[0].source, "ours");
  }
});

test("invalid Unicode and per-file UTF-8 limits reject the complete resolution", async t => {
  for (const [source, code] of [["\ud800", "INVALID_UNICODE"], ["\udfff", "INVALID_UNICODE"], ["😀".repeat(5), "BOOK_LIMIT"]]) {
    const h = await conflict(t, { limits: { fileBytes: 16 } }); h.disk.put("two.md", "disk update");
    await assert.rejects(resolve(h, [edited("one.md", source)]), { code });
    assert.equal(h.applied, 0); assert.equal(h.files[1].source, "two"); assert.deepEqual(writes(h.disk), []);
  }
});

test("resolved aggregate budget includes unchanged sources and path bytes", async t => {
  const h = await conflict(t, { limits: { sourceBytes: 32 } });
  await assert.rejects(resolve(h, [edited("one.md", "x".repeat(18))]), { code: "BOOK_LIMIT" });
  assert.equal(h.applied, 0); assert.equal(h.files[0].source, "ours");
  // A failed result must not acknowledge the external baseline.
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_CHANGED" });
});

test("late disk identity changes reject edited decisions before any source transaction", async t => {
  const h = await conflict(t);
  await assert.rejects(h.session.reconcile(() => {
    h.disk.put("two.md", "two"); return [edited("one.md", "combined")];
  }, h.install), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.applied, 0); assert.equal(h.files[0].source, "ours"); assert.deepEqual(writes(h.disk), []);
});

test("late editor changes invalidate edited results and preserve newer source", async t => {
  const h = await conflict(t);
  await assert.rejects(h.session.reconcile(() => {
    h.edit("one.md", "newer", false); return [edited("one.md", "combined")];
  }, h.install), { code: "STALE_SOURCE" });
  assert.equal(h.applied, 0); assert.equal(h.files[0].source, "newer");
});

test("an edited installation that mutates then fails retires disk authority", async t => {
  const h = await conflict(t);
  await assert.rejects(resolve(h, [edited("one.md", "combined")], (next, stamp) => {
    h.install(next, stamp); throw new BookDirectoryError("VIEW_FAILED", "View failed");
  }), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.files[0].source, "combined"); assert.equal(h.session.state.folder, null); assert.deepEqual(writes(h.disk), []);
});

test("an uncertain earlier disk write cannot be cleared by an edited resolution", async t => {
  const h = await conflict(t); await h.session.bind(() => h.disk.root, yes);
  h.disk.hook = async op => { if (op === "closed") throw browserError("QuotaExceededError"); };
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_SAVE_UNCERTAIN" });
  h.disk.hook = async () => {}; const receipt = h.session.state.receipt;
  await assert.rejects(resolve(h, [edited("one.md", "combined")]), { code: "DIRECTORY_REVIEW_REQUIRED" });
  assert.equal(h.applied, 0); assert.equal(h.session.state.receipt, receipt);
});
