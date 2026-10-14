import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookDirectorySession } from "../demo/book_directory_session.mjs";

// Production session/merger, real Node File/Blob/UTF-8. Only browser handles,
// permissions and the synchronous editor installation boundary are substituted.
const encoder = new TextEncoder(), decode = bytes => new TextDecoder("utf-8", { ignoreBOM: true }).decode(bytes);
const missing = () => new DOMException("missing", "NotFoundError");
class Disk {
  files = new Map(); directories = new Set([""]); log = []; hook = () => {};
  constructor(files) { for (const [path, source] of Object.entries(files)) this.put(path, source); this.root = this.directory(""); }
  put(path, source) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i++) this.directories.add(parts.slice(0, i).join("/"));
    this.files.set(path, { bytes: typeof source === "string" ? encoder.encode(source) : new Uint8Array(source) });
  }
  edit(path, source) { this.files.get(path).bytes = typeof source === "string" ? encoder.encode(source) : new Uint8Array(source); }
  text(path) { return decode(this.files.get(path).bytes); }
  async call(op, path) { this.log.push([op, path]); await this.hook(op, path); }
  directory(path) {
    const fs = this, child = name => path ? `${path}/${name}` : name;
    return { kind: "directory", name: path.split("/").at(-1) || "sources",
      async requestPermission(value) { assert.deepEqual(value, { mode: "readwrite" }); await fs.call("permission", path); return "granted"; },
      async getDirectoryHandle(name, { create = false } = {}) {
        const target = child(name); await fs.call(create ? "mkdir" : "directory", target);
        if (!fs.directories.has(target)) { if (!create) throw missing(); fs.directories.add(target); }
        return fs.directory(target);
      },
      async getFileHandle(name, { create = false } = {}) {
        const target = child(name); await fs.call(create ? "create" : "locate", target);
        if (!fs.files.has(target)) { if (!create) throw missing(); fs.put(target, ""); }
        return fs.handle(target, fs.files.get(target));
      },
      entries() { throw Error("No directory enumeration"); }, removeEntry() { throw Error("No deletion"); },
    };
  }
  handle(path, entry) {
    const fs = this;
    return { kind: "file", name: path.split("/").at(-1), entry,
      async isSameEntry(other) { await fs.call("identity", path); return entry === other.entry; },
      async getFile() { await fs.call("read", path); return new File([entry.bytes], path.split("/").at(-1)); },
      async createWritable(options) {
        assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" }); await fs.call("writer", path);
        let bytes;
        return { async write(blob) { await fs.call("write", path); bytes = new Uint8Array(await blob.arrayBuffer()); },
          async close() { await fs.call("close", path); entry.bytes = bytes; }, async abort() { await fs.call("abort", path); } };
      },
    };
  }
}
function fixture(t, sources = { "a.md": "a\nb\nc\n", "part/code.rs": "fn main() {}\n", "z.md": "Z\n" }, extra = {}) {
  const disk = new Disk(sources);
  let files = Object.entries(sources).map(([path, source]) => ({ path, source })), stamp = 0, calls = 0, blocked = false, hook = () => {};
  const install = (next, expected) => {
    assert.equal(expected, String(stamp)); calls++; hook("before");
    files = next.map(file => ({ ...file })); stamp++; core.changed(); hook("after");
    return String(stamp);
  };
  const { applySources = install, ...options } = extra;
  const core = createBookDirectorySession({ read: () => ({ files, stamp: String(stamp) }),
    isBlocked: () => blocked, ...options });
  const session = { ...core, get state() { return core.state; },
    refresh: confirm => core.reconcile(confirm, applySources) };
  t.after(() => session.dispose());
  return { disk, session, get files() { return files; }, get calls() { return calls; },
    source(path) { return files.find(file => file.path === path)?.source; },
    edit(path, source, notify = true) { files = files.map(file => file.path === path ? { path, source } : file); stamp++; if (notify) session.changed(); },
    setFiles(value) { files = value; stamp++; session.changed(); },
    block(value) { blocked = value; }, onApply(value) { hook = value; },
    async connect() { await session.bind(() => disk.root, () => true); disk.log = []; },
  };
}
const approve = () => [];
const decision = (path, choice) => [{ path, choice }];
const assertReadOnly = disk => assert(disk.log.every(([op]) => !["permission", "mkdir", "create", "writer", "write", "close", "abort"].includes(op)));

test("refresh merges local and disk edits across chapters/resources, once, without writing", async t => {
  const h = fixture(t); await h.connect(); h.disk.put("unlisted.txt", "private");
  h.edit("a.md", "LOCAL\nb\nc\n"); h.disk.edit("a.md", "a\nb\nDISK\n"); h.disk.edit("part/code.rs", "fn main() { changed(); }\n");
  let review;
  const value = await h.session.refresh(next => { review = next; return []; });
  assert.equal(value.changedSources, 2); assert.equal(h.calls, 1);
  assert.equal(h.source("a.md"), "LOCAL\nb\nDISK\n"); assert.equal(h.source("part/code.rs"), "fn main() { changed(); }\n");
  assert.equal(h.source("z.md"), "Z\n"); assert.equal(h.session.state.dirty, 1); assert(h.session.state.canSave);
  assert(Object.isFrozen(review)); assert(Object.isFrozen(review.files)); assert(review.files.every(Object.isFrozen));
  assert.deepEqual(Object.keys(review.files[0]).sort(), ["path", "base", "local", "disk", "identityChanged", "action", "source", "reason", "editorBytes", "diskBytes"].sort());
  assertReadOnly(h.disk); assert(!h.disk.log.some(([, path]) => path === "unlisted.txt"));
  h.disk.log = []; const saved = await h.session.save(() => true);
  assert.equal(saved.verified, 1); assert.equal(h.disk.text("a.md"), h.source("a.md"));
  assert(!h.disk.log.some(([op, path]) => op === "writer" && path === "part/code.rs"));
});
test("cancelled review changes neither local source nor acknowledged disk baseline", async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  assert.equal(await h.session.refresh(() => false), null); assert.equal(h.calls, 0);
  assert.equal(h.source("a.md"), "a\nb\nc\n"); assertReadOnly(h.disk);
  await assert.rejects(h.session.save(() => true), { code: "DIRECTORY_CHANGED" });
});
test("conflicting files require complete explicit choices and install a single full source set", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); h.edit("z.md", "our Z");
  h.disk.edit("a.md", "theirs"); h.disk.edit("z.md", "their Z");
  const result = await h.session.refresh(review => {
    assert.equal(review.files.filter(file => file.action === "conflict").length, 2);
    return [{ path: "a.md", choice: "local" }, { path: "z.md", choice: "disk" }];
  });
  assert.equal(result.changedSources, 1); assert.equal(h.calls, 1); assert.equal(h.source("a.md"), "ours");
  assert.equal(h.source("z.md"), "their Z"); assert.equal(h.session.state.dirty, 1); assertReadOnly(h.disk);
});
for (const [label, answer] of [["boolean", true], ["empty", []], ["unknown", decision("other.md", "local")],
  ["invalid side", decision("a.md", "merge")], ["duplicate", [{ path: "a.md", choice: "local" }, { path: "a.md", choice: "disk" }]]])
  test(`invalid conflict approval (${label}) cannot update source or baseline`, async t => {
    const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); h.disk.edit("a.md", "theirs");
    await assert.rejects(h.session.refresh(() => answer), { code: "UNRESOLVED_SOURCE_CONFLICT" });
    assert.equal(h.source("a.md"), "ours"); assert.equal(h.calls, 0); assertReadOnly(h.disk);
  });
test("accessor and sparse conflict decisions are rejected without executing source getters", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "ours"); h.disk.edit("a.md", "theirs");
  let invoked = false;
  await assert.rejects(h.session.refresh(() => [{ get path() { invoked = true; return "a.md"; }, choice: "disk" }]), { code: "INVALID_SOURCE" });
  await assert.rejects(h.session.refresh(() => new Array(1)), { code: "INVALID_SOURCE" });
  assert(!invoked); assert.equal(h.calls, 0);
});
test("disk deletion cannot delete source or masquerade as an empty file", async t => {
  const h = fixture(t); await h.connect(); h.disk.files.delete("a.md");
  await assert.rejects(h.session.refresh(() => decision("a.md", "disk")), { code: "UNRESOLVED_SOURCE_CONFLICT" });
  let review;
  const value = await h.session.refresh(next => { review = next; return decision("a.md", "local"); });
  assert.equal(review.files[0].reason, "deleted-on-disk"); assert.equal(review.files[0].disk, null);
  assert.equal(value.changedSources, 0); assert.equal(h.calls, 0); assert.equal(h.session.state.dirty, 1); assertReadOnly(h.disk);
  await h.session.save(() => true); assert.equal(h.disk.text("a.md"), "a\nb\nc\n");
});
test("a new external file under an absent baseline is reviewed as an add/add conflict", async t => {
  const h = fixture(t); h.disk.files.delete("a.md"); await h.connect(); h.disk.put("a.md", "created elsewhere");
  const value = await h.session.refresh(review => {
    assert.equal(review.files[0].reason, "created-on-disk"); return decision("a.md", "disk");
  });
  assert.equal(value.changedSources, 1); assert.equal(h.source("a.md"), "created elsewhere"); assert.equal(h.session.state.dirty, 0);
});
test("same-content atomic replacement is acknowledged only after review, and later saving works", async t => {
  const h = fixture(t); await h.connect(); h.disk.put("a.md", h.source("a.md"));
  const value = await h.session.refresh(review => { assert(review.files[0].identityChanged); return []; });
  assert.equal(value.changedSources, 0); assert.equal(h.calls, 0); h.edit("a.md", "later");
  await h.session.save(() => true); assert.equal(h.disk.text("a.md"), "later");
});
for (const [label, mutate] of [
  ["reviewed disk bytes", h => h.disk.edit("a.md", "newer disk")],
  ["otherwise unchanged disk source", h => h.disk.edit("z.md", "newer Z")],
  ["file identity", h => h.disk.put("a.md", "external")],
]) test(`${label} changing during review rejects the complete editor transaction`, async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  await assert.rejects(h.session.refresh(() => { mutate(h); return []; }), { code: "DIRECTORY_CHANGED" });
  assert.equal(h.calls, 0); assert.equal(h.source("a.md"), "a\nb\nc\n"); assertReadOnly(h.disk);
});
for (const notify of [false, true]) test(`new editor input during review is protected (dispatch=${notify})`, async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  await assert.rejects(h.session.refresh(() => { h.edit("z.md", "new local", notify); return []; }), { code: "STALE_SOURCE" });
  assert.equal(h.calls, 0); assert.equal(h.source("z.md"), "new local"); assertReadOnly(h.disk);
});
test("editing while disk reads are pending prevents review and source installation", async t => {
  const h = fixture(t); await h.connect(); h.disk.hook = op => { if (op === "read") h.edit("z.md", "changed", false); };
  await assert.rejects(h.session.refresh(() => { throw Error("No stale review"); }), { code: "STALE_SOURCE" });
  assert.equal(h.calls, 0); assertReadOnly(h.disk);
});
test("detach and disposal retire pending refresh ownership without reconnecting", async t => {
  for (const dispose of [false, true]) {
    const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
    await assert.rejects(h.session.refresh(() => { if (dispose) h.session.dispose(); else h.session.detach(); return []; }),
      { code: dispose ? "SESSION_DISPOSED" : "STALE_SOURCE" });
    assert.equal(h.calls, 0); assert.equal(h.session.state.folder, null);
  }
});
test("same paths may be reordered but new membership requires explicit reconnection", async t => {
  const h = fixture(t); await h.connect(); h.setFiles([...h.files].reverse()); h.disk.edit("a.md", "external");
  await h.session.refresh(approve); assert.equal(h.files.at(-1).path, "a.md");
  h.setFiles([...h.files, { path: "new.md", source: "new" }]); h.disk.log = [];
  assert(!h.session.state.canRefresh);
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_REVIEW_REQUIRED" }); assert.deepEqual(h.disk.log, []);
});
test("refresh clears an acknowledged disk conflict but retains the historical write receipt", async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  await assert.rejects(h.session.save(() => true), { code: "DIRECTORY_CHANGED" });
  const receipt = h.session.state.receipt; assert(h.session.state.canRefresh);
  await h.session.refresh(approve); assert.equal(h.session.state.phase, "saved"); assert.equal(h.session.state.receipt, receipt);
});
test("a rejected host transaction retains original sources and does not adopt disk bytes", async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  h.onApply(when => { if (when === "before") throw Error("host refusal"); });
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_OPERATION_FAILED" });
  assert.equal(h.source("a.md"), "a\nb\nc\n"); await assert.rejects(h.session.save(() => true), { code: "DIRECTORY_CHANGED" }); assertReadOnly(h.disk);
});
test("a reentrant source edit after installation is reported as uncertain, never rolled back", async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  h.onApply(when => { if (when === "after") h.edit("z.md", "reentrant"); });
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.source("z.md"), "reentrant"); assert.equal(h.source("a.md"), "external");
  assert(!h.session.state.canSave); assert.equal(h.session.state.folder, null); assertReadOnly(h.disk);
});
test("a detached connection during source installation cannot acquire a new disk baseline", async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "external");
  h.onApply(when => { if (when === "after") h.session.detach(); });
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.session.state.folder, null); assert.equal(h.source("a.md"), "external");
});
test("an asynchronous installer is rejected and its rejection is observed", async t => {
  const h = fixture(t, undefined, { applySources: () => Promise.reject(Error("invalid async host")) });
  await h.connect(); h.disk.edit("a.md", "external");
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.session.state.folder, null); assert.equal(h.source("a.md"), "a\nb\nc\n");
});
test("legacy save-only hosts remain functional and expose no refresh capability", async t => {
  const h = fixture(t, undefined, { applySources: null }); await h.connect();
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_REFRESH_UNAVAILABLE" });
  h.edit("a.md", "save still works"); await h.session.save(() => true); assert.equal(h.disk.text("a.md"), "save still works");
});
test("invalid UTF-8, disk aliases and read budgets fail before review or installation", async t => {
  for (const mode of ["utf8", "alias", "size"]) {
    const h = fixture(t, undefined, { limits: { fileBytes: 128, sourceBytes: 256 } }); await h.connect();
    if (mode === "utf8") h.disk.edit("a.md", [255]);
    if (mode === "alias") h.disk.files.set("z.md", h.disk.files.get("a.md"));
    if (mode === "size") h.disk.edit("a.md", "x".repeat(129));
    await assert.rejects(h.session.refresh(() => { throw Error("No invalid review"); }),
      { code: mode === "utf8" ? "INVALID_UNICODE" : mode === "alias" ? "PATH_COLLISION" : "DIRECTORY_CHANGED" });
    assert.equal(h.calls, 0); assertReadOnly(h.disk);
  }
});
test("combined merge byte limits are checked before any source installation", async t => {
  const h = fixture(t, { "a.md": "a\nb\nc\n" }, { limits: { fileBytes: 24, sourceBytes: 28 } }); await h.connect();
  h.edit("a.md", "local-long\nb\nc\n"); h.disk.edit("a.md", "a\nb\ndisk-longer\n");
  await assert.rejects(h.session.refresh(approve), { code: "BOOK_LIMIT" }); assert.equal(h.calls, 0); assertReadOnly(h.disk);
});
test("BOM, mixed newlines, Unicode and empty sources survive disk-to-editor admission", async t => {
  const h = fixture(t); await h.connect(); h.disk.edit("a.md", "\ufeff日本 😀\r\nB\rC\n"); h.disk.edit("z.md", "");
  await h.session.refresh(approve); assert.equal(h.source("a.md"), h.disk.text("a.md")); assert.equal(h.source("z.md"), "");
  assertReadOnly(h.disk);
});
test("refresh is serialized with other disk operations and blocked by composition", async t => {
  const h = fixture(t); await h.connect(); h.block(true);
  await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_BUSY" }); h.block(false);
  await h.session.refresh(async () => {
    await assert.rejects(h.session.save(() => true), { code: "DIRECTORY_BUSY" });
    await assert.rejects(h.session.refresh(approve), { code: "DIRECTORY_BUSY" }); return [];
  });
});

test("complex within-file differences fall back to a complete explicit version choice", async t => {
  const text = prefix => Array.from({ length: 1500 }, (_, i) => `${prefix}-${i}\n`).join("");
  const h = fixture(t, { "a.md": text("base") }); await h.connect();
  h.edit("a.md", text("local")); h.disk.edit("a.md", text("disk"));
  const result = await h.session.refresh(review => {
    assert.equal(review.files[0].reason, "merge-budget");
    assert.equal(review.files[0].local, text("local")); assert.equal(review.files[0].disk, text("disk"));
    return decision("a.md", "disk");
  });
  assert.equal(result.changedSources, 1); assert.equal(h.source("a.md"), text("disk")); assertReadOnly(h.disk);
});

test("the aggregate admission is performed after a conflict choice can shrink the final source set", async t => {
  const h = fixture(t, { a: "a", b: "b" }, { limits: { sourceBytes: 64 } }); await h.connect();
  h.edit("b", "l".repeat(40)); h.disk.edit("a", "r".repeat(40)); h.disk.edit("b", "d");
  const result = await h.session.refresh(review => {
    assert.equal(review.conflicts, 1); return decision("b", "disk");
  });
  assert.equal(result.changedSources, 2); assert.deepEqual(h.files, [{ path: "a", source: "r".repeat(40) }, { path: "b", source: "d" }]);
  assertReadOnly(h.disk);
});

test("merge cannot clear an uncertain save or erase its verification receipt", async t => {
  const h = fixture(t); await h.connect(); h.edit("a.md", "local");
  h.disk.hook = op => { if (op === "close") throw new DOMException("close uncertain", "QuotaExceededError"); };
  await assert.rejects(h.session.save(() => true), { code: "DIRECTORY_SAVE_UNCERTAIN" });
  const receipt = h.session.state.receipt; h.disk.hook = () => {}; h.disk.log = [];
  await assert.rejects(h.session.refresh(() => { assert.fail("Uncertain writes need reconnect, not merge"); }), { code: "DIRECTORY_REVIEW_REQUIRED" });
  assert.equal(h.session.state.receipt, receipt); assert.equal(h.calls, 0); assert.deepEqual(h.disk.log, []);
});
