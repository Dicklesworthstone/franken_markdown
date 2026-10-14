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

test("disk-only sources apply once after review, preserving exact UTF-8/BOM/newlines without disk writes", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "\ufeff# 日本 😀\r\nA\rB\n"); h.disk.put("two.md", "revised");
  const before = h.files; let review;
  h.disk.log = [];
  const result = await h.refresh(value => { review = value; assert.equal(h.applied, 0); assert.deepEqual(h.files, before); return true; });
  assert.equal(result.outcome, "refreshed"); assert.equal(result.changedSources, 2); assert(result.current);
  assert(Object.isFrozen(review)); assert(Object.isFrozen(review.files)); assert(Object.isFrozen(review.files[0]));
  assert.deepEqual(review.files.map(file => file.action), ["reload", "reload"]);
  for (const file of h.files) assert.equal(file.source, h.disk.text(file.path));
  assert.equal(h.applied, 1); assert.equal(h.session.state.phase, "saved"); assert.deepEqual(writes(h.disk), []);
});

test("local-only and disk-only edits merge as one transaction and later save uses the new baseline", async t => {
  const h = fixture(t); await h.bind(); h.edit("one.md", "local"); h.disk.put("two.md", "external");
  const result = await h.refresh(); assert.deepEqual(result.files.map(file => file.action), ["keep-editor", "reload"]);
  assert.deepEqual(h.files.map(file => file.source), ["local", "external"]); assert.equal(h.disk.text("one.md"), "one");
  assert.equal(h.session.state.dirty, 1); assert.equal(h.applied, 1);
  const saved = await h.session.save(yes); assert.equal(saved.verified, 1);
  assert.equal(h.disk.text("one.md"), "local"); assert.equal(h.disk.text("two.md"), "external");
});

test("divergent edits reject the entire source set and enumerate conflicts without advancing its baseline", async t => {
  const h = fixture(t); await h.bind(); h.edit("one.md", "local"); h.disk.put("one.md", "external"); h.disk.put("two.md", "safe external");
  const before = h.files; let review;
  await assert.rejects(h.refresh(() => { assert.fail("Conflicts cannot be approved"); }), error => {
    assert.equal(error.code, "DIRECTORY_SOURCE_CONFLICT"); review = error.review; return true;
  });
  assert.equal(review.conflicts, 1); assert.deepEqual(review.files.map(file => file.action), ["conflict", "reload"]);
  assert.equal(h.applied, 0); assert.deepEqual(h.files, before); assert.equal(h.session.state.phase, "conflict");
  // Returning the editor to its ORIGINAL baseline makes the disk-only edits
  // safe. A rejected refresh must not have silently moved that baseline.
  h.edit("one.md", "one"); await h.refresh(); assert.equal(h.applied, 1);
  assert.deepEqual(h.files.map(file => file.source), ["external", "safe external"]);
});

test("identical concurrent edits converge without invoking the source host", async t => {
  const h = fixture(t); await h.bind(); h.edit("one.md", "same edit"); h.disk.put("one.md", "same edit");
  const result = await h.refresh(); assert.equal(result.changedSources, 0); assert.equal(result.files[0].action, "converged");
  assert.equal(h.applied, 0); assert.equal(h.session.state.dirty, 0);
  h.edit("one.md", "next edit"); await h.session.save(yes); assert.equal(h.disk.text("one.md"), "next edit");
});

test("a removed existing disk file is reported, never removed from the source tree", async t => {
  const h = fixture(t); await h.bind(); h.disk.files.delete("one.md"); h.disk.put("two.md", "external");
  await assert.rejects(h.refresh(), error => {
    assert.equal(error.code, "DIRECTORY_SOURCE_CONFLICT"); assert.equal(error.review.files[0].action, "missing");
    assert.equal(error.review.files[0].diskBytes, null); return true;
  });
  assert.equal(h.applied, 0); assert.deepEqual(h.files.map(file => file.source), ["one", "two"]);
});

test("a source missing since connection stays a local new file while another source refreshes", async t => {
  const h = fixture(t, { sources: [{ path: "one.md", source: "one" }, { path: "new.md", source: "" }] }); await h.bind();
  h.disk.put("one.md", "external"); const result = await h.refresh();
  assert.deepEqual(result.files.map(file => file.action), ["reload", "keep-editor"]);
  assert(!h.disk.files.has("new.md")); await h.session.save(yes); assert.equal(h.disk.text("new.md"), "");
});

test("an appeared file conflicting with a local new source does not overwrite the editor", async t => {
  const h = fixture(t, { sources: [{ path: "new.md", source: "local" }] }); await h.bind(); h.disk.put("new.md", "external");
  await assert.rejects(h.refresh(), { code: "DIRECTORY_SOURCE_CONFLICT" }); assert.equal(h.files[0].source, "local");
});

test("a captured external atomic-save replacement can be adopted and saved again", async t => {
  const h = fixture(t); await h.bind(); const original = h.disk.files.get("one.md");
  h.disk.put("one.md", "external"); assert.notEqual(h.disk.files.get("one.md"), original); await h.refresh();
  h.edit("one.md", "after refresh"); await h.session.save(yes); assert.equal(h.disk.text("one.md"), "after refresh");
});

test("replacement of a reviewed file identity cancels refresh even for equal bytes", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(() => { h.disk.put("one.md", "external"); return true; }), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.applied, 0); assert.equal(h.files[0].source, "one");
});

test("disk edits during confirmation are rechecked before any source is applied", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(() => { h.disk.files.get("one.md").bytes = encoder.encode("even newer"); return true; }), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.applied, 0); assert.equal(h.files[0].source, "one");
});

test("even unchanged paths are rechecked before applying disk-only changes elsewhere", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(() => { h.disk.put("two.md", "late edit"); return true; }), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.applied, 0);
});

test("merged aggregate budgets are checked independently of valid editor and disk captures", async t => {
  const h = fixture(t, { limits: { sourceBytes: 64 } }); await h.bind();
  h.edit("one.md", "l".repeat(40)); h.disk.put("two.md", "r".repeat(40));
  await assert.rejects(h.refresh(), { code: "BOOK_LIMIT" }); assert.equal(h.applied, 0);
  assert.equal(h.files[1].source, "two");
});

test("invalid UTF-8 and oversize disk sources cannot partially replace the book", async t => {
  const h = fixture(t, { limits: { fileBytes: 16 } }); await h.bind(); h.disk.put("one.md", "valid");
  h.disk.files.get("two.md").bytes = new Uint8Array([255]);
  await assert.rejects(h.refresh(), { code: "INVALID_UNICODE" }); assert.equal(h.applied, 0);
  h.disk.put("two.md", "x".repeat(17)); await assert.rejects(h.refresh(), { code: "DIRECTORY_CHANGED" }); assert.equal(h.applied, 0);
});

test("actual file aliases fail before source approval", async t => {
  const h = fixture(t); await h.bind(); h.disk.files.set("two.md", h.disk.files.get("one.md"));
  await assert.rejects(h.refresh(() => assert.fail("no alias approval")), { code: "PATH_COLLISION" }); assert.equal(h.applied, 0);
});

test("refresh requires a connection, identical path set, and a source transaction capability", async t => {
  const h = fixture(t); await assert.rejects(h.refresh(), { code: "NO_CONNECTED_DIRECTORY" }); await h.bind();
  h.disk.log = []; await assert.rejects(h.session.refresh(yes, null), { code: "DIRECTORY_REFRESH_UNAVAILABLE" }); assert.deepEqual(h.disk.log, []);
  h.setFiles([{ path: "new.md", source: "new" }]); await assert.rejects(h.refresh(), { code: "DIRECTORY_REVIEW_REQUIRED" });
});

test("an uncertain write cannot be cleared by read refresh and retains its original receipt", async t => {
  const h = fixture(t); await h.bind(); h.edit("one.md", "ours");
  h.disk.hook = async op => { if (op === "closed") throw browserError("QuotaExceededError"); };
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_SAVE_UNCERTAIN" });
  h.disk.hook = async () => {}; const receipt = h.session.state.receipt;
  await assert.rejects(h.refresh(), { code: "DIRECTORY_REVIEW_REQUIRED" }); assert.equal(h.session.state.receipt, receipt);
});

test("declining refresh does not advance the disk baseline or change source", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external"); const before = h.files;
  assert.equal(await h.refresh(() => false), null); assert.equal(h.applied, 0); assert.deepEqual(h.files, before);
  h.edit("one.md", "ours"); await assert.rejects(h.session.save(yes), { code: "DIRECTORY_CHANGED" });
});

test("source changes with and without notifications invalidate a reviewed refresh", async t => {
  for (const notify of [true, false]) {
    const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
    await assert.rejects(h.refresh(() => { h.edit("two.md", "new editor", notify); return true; }), { code: "STALE_SOURCE" });
    assert.equal(h.applied, 0); assert.equal(h.files[1].source, "new editor");
  }
});

test("disconnect and disposal retire a pending refresh without applying source", async t => {
  for (const dispose of [false, true]) {
    const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
    await assert.rejects(h.refresh(() => { if (dispose) h.session.dispose(); else h.session.detach(); return true; }),
      { code: dispose ? "SESSION_DISPOSED" : "STALE_SOURCE" });
    assert.equal(h.applied, 0);
  }
});

test("a rejected atomic host transaction with no mutation remains retryable", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(yes, () => { throw new BookDirectoryError("HOST_REJECTED", "No change"); }), { code: "HOST_REJECTED" });
  assert.equal(h.session.state.folder, "sources"); assert.equal(h.applied, 0); await h.refresh(); assert.equal(h.applied, 1);
});

test("a host error after mutation is not misreported as rollback and retires disk authority", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(yes, (next, stamp) => { h.install(next, stamp); throw Error("view failed"); }), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.files[0].source, "external"); assert.equal(h.session.state.folder, null); assert.deepEqual(writes(h.disk), []);
});

test("mismatched post-transaction source or checkpoint rejects the success receipt", async t => {
  for (const mismatch of ["source", "stamp"]) {
    const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
    await assert.rejects(h.refresh(yes, (next, stamp) => {
      const actual = h.install(next, stamp); if (mismatch === "source") h.edit("one.md", "different");
      return mismatch === "stamp" ? "wrong" : actual;
    }), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
    assert.equal(h.session.state.folder, null);
  }
});

test("asynchronous source callbacks are refused rather than accepted as atomic transactions", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(yes, () => Promise.resolve(h.stamp())), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.session.state.folder, null); assert.equal(h.applied, 0);
});

test("lost ownership during a host source transaction cannot revive the old folder", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external");
  await assert.rejects(h.refresh(yes, (next, stamp) => { const result = h.install(next, stamp); h.session.detach(); return result; }), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.session.state.folder, null); assert.equal(h.files[0].source, "external");
});

test("unchanged refresh performs review but never calls the source writer or write-permission API", async t => {
  const h = fixture(t); await h.bind(); h.disk.log = []; let confirmed = 0;
  const result = await h.refresh(value => { confirmed++; assert.equal(value.changedSources, 0); return true; });
  assert.equal(result.outcome, "unchanged"); assert.equal(confirmed, 1); assert.equal(h.applied, 0); assert.deepEqual(writes(h.disk), []);
});

test("an active refresh excludes simultaneous saves and other refreshes", async t => {
  const h = fixture(t); await h.bind(); h.disk.put("one.md", "external"); let approve;
  const pending = h.refresh(() => new Promise(resolve => { approve = resolve; }));
  for (let i = 0; !approve && i < 100; i++) await new Promise(setImmediate); assert(approve);
  await assert.rejects(h.session.save(yes), { code: "DIRECTORY_BUSY" });
  await assert.rejects(h.refresh(), { code: "DIRECTORY_BUSY" }); approve(true); await pending; assert.equal(h.applied, 1);
});

test("200 seeded source sets match an independent whole-file three-way oracle", async t => {
  let seed = 0x5f7a3812;
  const choose = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  const text = ["", "a", "b", "\ufeff日本\r\n", "😀\nend", "line\rbreak"];
  for (let run = 0; run < 200; run++) {
    const paths = ["a.md", "parts/b.txt", "c.md"], baseline = paths.map(() => text[choose(text.length)]);
    const h = fixture(t, { diskFiles: Object.fromEntries(paths.map((path, i) => [path, baseline[i]])) });
    await h.bind(); const local = paths.map(() => text[choose(text.length)]);
    const remote = paths.map(() => choose(6) === 0 ? null : text[choose(text.length)]);
    paths.forEach((path, i) => { h.edit(path, local[i]); if (remote[i] === null) h.disk.files.delete(path); else h.disk.put(path, remote[i]); });
    const conflict = paths.some((_, i) => remote[i] !== local[i] && remote[i] !== baseline[i]
      && (remote[i] === null || local[i] !== baseline[i]));
    if (conflict) {
      await assert.rejects(h.refresh(), { code: "DIRECTORY_SOURCE_CONFLICT" });
      assert.deepEqual(h.files.map(file => file.source), local, `rejected set ${run}`); assert.equal(h.applied, 0);
    } else {
      const expected = paths.map((_, i) => local[i] === baseline[i] && remote[i] !== null ? remote[i] : local[i]);
      const changed = expected.filter((source, i) => source !== local[i]).length;
      const result = await h.refresh(); assert.equal(result.changedSources, changed, `count ${run}`);
      assert.deepEqual(h.files.map(file => file.source), expected, `accepted set ${run}`);
      assert.equal(h.applied, changed ? 1 : 0); assert.equal(result.current, true);
    }
    assert.deepEqual(writes(h.disk), []); h.session.dispose();
  }
});
