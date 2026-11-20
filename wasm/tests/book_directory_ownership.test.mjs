import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { mkdtemp, mkdir, open, readFile, rename, stat, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { createBookDirectorySession } from "../demo/book_directory_session.mjs";

// The browser permission/handle interface is adapted to real temporary files.
// A close fsyncs and renames its staged file; abort closes and retains the stage
// without publishing it. No module rewriting, directory enumeration or deletion.
async function diskFolder(initial) {
  const rootPath = await mkdtemp(join(tmpdir(), "fmd-book-directory-"));
  const log = [], hooks = {}, reads = new Map(), handles = new Map(), locked = new Set();
  const browserError = name => Object.assign(new Error(name), { name });
  const call = async (operation, path) => {
    log.push([operation, path]);
    await hooks[operation]?.(path);
  };
  const set = async (path, source) => {
    await mkdir(dirname(join(rootPath, path)), { recursive: true });
    await writeFile(join(rootPath, path), source);
  };
  for (const [path, source] of Object.entries(initial)) await set(path, source);
  function fileHandle(path) {
    if (handles.has(path)) return handles.get(path);
    const destination = join(rootPath, path), name = basename(path);
    const handle = { kind: "file", name,
      async isSameEntry(other) { await call("identity", path); return handle === other; },
      async getFile() {
        reads.set(path, (reads.get(path) ?? 0) + 1);
        await call("read", path);
        return new File([await readFile(destination)], name);
      },
      async createWritable(options) {
        assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
        await call("writer", path);
        if (locked.has(path)) throw browserError("NoModificationAllowedError");
        locked.add(path);
        const stage = join(rootPath, `stage-${randomUUID()}`), stream = await open(stage, "wx");
        let closed = false, aborted = false;
        return {
          async write(blob) {
            assert(blob instanceof Blob);
            await stream.writeFile(new Uint8Array(await blob.arrayBuffer()));
            await call("write", path);
          },
          async close() {
            await call("close", path);
            if (aborted) throw browserError("AbortError");
            await stream.sync(); await stream.close(); closed = true;
            await rename(stage, destination); locked.delete(path);
            await call("closed", path);
          },
          async abort() {
            aborted = true;
            await call("abort", path);
            if (!closed) { closed = true; await stream.close(); }
            locked.delete(path);
          },
        };
      },
    };
    handles.set(path, handle); return handle;
  }
  function directory(path = "") {
    const child = name => path ? `${path}/${name}` : name;
    return { kind: "directory", name: path ? basename(path) : basename(rootPath),
      async requestPermission(options) {
        assert.deepEqual(options, { mode: "readwrite" });
        await call("permission", path); return "granted";
      },
      async getDirectoryHandle(name, { create = false } = {}) {
        const next = child(name);
        await call(create ? "mkdir" : "directory", next);
        if (create) await mkdir(join(rootPath, next), { recursive: true });
        let entry;
        try { entry = await stat(join(rootPath, next)); }
        catch (error) { if (error.code === "ENOENT") throw browserError("NotFoundError"); throw error; }
        if (!entry.isDirectory()) throw browserError("TypeMismatchError");
        return directory(next);
      },
      async getFileHandle(name, { create = false } = {}) {
        const next = child(name);
        await call(create ? "create" : "locate", next);
        if (create) {
          try { const empty = await open(join(rootPath, next), "wx"); await empty.close(); }
          catch (error) { if (error.code !== "EEXIST") throw error; }
        }
        let entry;
        try { entry = await stat(join(rootPath, next)); }
        catch (error) { if (error.code === "ENOENT") throw browserError("NotFoundError"); throw error; }
        if (!entry.isFile()) throw browserError("TypeMismatchError");
        return fileHandle(next);
      },
      entries() { throw Error("Source-folder saves must not enumerate unrelated files"); },
      removeEntry() { throw Error("Source-folder saves must not delete files"); },
    };
  }
  return { root: directory(), log, hooks, reads, locked, set,
    text: async path => (await readFile(join(rootPath, path))).toString("utf8"),
    reset() { log.length = 0; reads.clear(); },
  };
}

async function fixture(t) {
  const disk = await diskFolder({ "a.md": "Original A\r\n", "parts/b.md": "Original B\n" });
  let files = [{ path: "a.md", source: "Original A\r\n" }, { path: "parts/b.md", source: "Original B\n" }];
  let revision = 0, blocked = false, reading = null, guarding = null, observing = null;
  const notices = [];
  const session = createBookDirectorySession({
    read() {
      const value = { files, stamp: String(revision) }, callback = reading;
      reading = null; callback?.(); return value;
    },
    isBlocked() {
      const value = blocked, callback = guarding;
      guarding = null; callback?.(); return value;
    },
    onState(value) {
      notices.push(value);
      const callback = observing; observing = null; callback?.(value);
    },
  });
  t.after(() => session.dispose());
  const h = { disk, session, notices,
    get files() { return files; },
    read(callback) { reading = callback; },
    guard(callback) { guarding = callback; },
    observe(callback) { observing = callback; },
    block() { blocked = true; },
    edit(path, source) {
      files = files.map(file => file.path === path ? { path, source } : file);
      revision++; session.changed();
    },
    install(next, expected) {
      assert.equal(expected, String(revision));
      files = next.map(file => ({ ...file })); revision++; session.changed(); return String(revision);
    },
    bind: () => session.bind(() => disk.root, () => true),
  };
  return h;
}

function afterSourceReads(h, count, callback) {
  const next = () => h.read(() => { if (--count === 0) callback(); else next(); });
  next();
}
function beforeClose(h, path, boundary, callback) {
  h.disk.hooks.read = name => {
    // Fourth read: all writers are staged and the last path-specific disk
    // comparison is finishing. Its own fence precedes the final close fence.
    if (name !== path || h.disk.reads.get(path) !== 4) return;
    if (boundary === "read") afterSourceReads(h, 2, callback);
    else afterSourceReads(h, 1, () => h.guard(callback));
  };
}
const closePaths = h => h.disk.log.filter(([op]) => op === "close").map(([, path]) => path);
async function editBoth(h) {
  await h.bind();
  h.edit("a.md", "\ufeff# Captured A 😀\r\n"); h.edit("parts/b.md", "# Captured B\n");
  h.disk.reset();
}

test("real folder saves stage both files and preserve exact UTF-8 and newline bytes", async t => {
  const h = await fixture(t); await editBoth(h);
  const result = await h.session.save(() => true);
  assert.equal(result.outcome, "saved"); assert.equal(result.verified, 2); assert.equal(result.current, true);
  for (const file of h.files) assert.equal(await h.disk.text(file.path), file.source);
  const firstClose = h.disk.log.findIndex(([op]) => op === "close");
  assert(h.disk.log.slice(0, firstClose).some(([op, path]) => op === "write" && path === "parts/b.md"));
  assert.equal(h.disk.locked.size, 0);
});

for (const boundary of ["read", "guard"]) for (const change of ["detach", "edit", "lock", "dispose"]) {
  test(`final ${boundary} callback ${change} stops every staged source before a real file close`, async t => {
    const h = await fixture(t); await editBoth(h);
    beforeClose(h, "a.md", boundary, () => {
      if (change === "edit") h.edit("a.md", "Newer editor source");
      else if (change === "lock") h.block();
      else h.session[change]();
    });
    await assert.rejects(h.session.save(() => true), { code: change === "dispose" ? "SESSION_DISPOSED" : "STALE_SOURCE" });
    assert.deepEqual(closePaths(h), []);
    assert.equal(await h.disk.text("a.md"), "Original A\r\n");
    assert.equal(await h.disk.text("parts/b.md"), "Original B\n");
    assert.equal(h.disk.log.filter(([op]) => op === "abort").length, 2);
    assert.equal(h.disk.locked.size, 0);
    if (change === "detach") {
      assert.equal(h.session.state.folder, null); assert.equal(h.session.state.receipt, null);
      assert.match(h.session.state.message, /^Source folder disconnected/);
    }
  });
}

test("a final callback after one verified file yields a partial receipt and retains the second disk source", async t => {
  const h = await fixture(t); await editBoth(h);
  beforeClose(h, "parts/b.md", "read", () => h.session.detach());
  await assert.rejects(h.session.save(() => true), error => {
    assert.equal(error.code, "STALE_SOURCE"); assert.equal(error.receipt.outcome, "partial");
    assert.equal(error.receipt.verified, 1); assert.equal(error.receipt.current, false);
    assert.deepEqual(error.receipt.files.map(file => file.status), ["verified", "not-saved"]);
    return true;
  });
  assert.deepEqual(closePaths(h), ["a.md"]);
  assert.equal(await h.disk.text("a.md"), h.files[0].source);
  assert.equal(await h.disk.text("parts/b.md"), "Original B\n");
  assert.equal(h.session.state.receipt, null);
  assert.match(h.notices.at(-1).message, /^Source folder disconnected/);
});

test("post-save ownership callbacks retain the verified receipt for the caller without publishing to a detached book", async t => {
  const h = await fixture(t); await editBoth(h);
  h.disk.hooks.read = path => {
    if (path === "parts/b.md" && h.disk.reads.get(path) === 5) h.read(() => h.session.detach());
  };
  const result = await h.session.save(() => true);
  assert.equal(result.verified, 2); assert.equal(result.current, false);
  assert.equal(h.session.state.folder, null); assert.equal(h.session.state.receipt, null);
  assert.match(h.session.state.message, /^Source folder disconnected/);
  assert.match(h.notices.at(-1).message, /^Source folder disconnected/);
  for (const file of h.files) assert.equal(await h.disk.text(file.path), file.source);
});

test("a revoked final folder-review snapshot cannot reconnect the old folder", async t => {
  const h = await fixture(t);
  h.disk.hooks.read = path => {
    if (path === "parts/b.md" && h.disk.reads.get(path) === 2) h.read(() => h.session.detach());
  };
  await assert.rejects(h.bind(), { code: "STALE_SOURCE" });
  assert.equal(h.session.state.folder, null); assert.deepEqual(closePaths(h), []);
  assert.match(h.session.state.message, /^Source folder disconnected/);
});

test("external bytes appearing after staging are preserved and every unclosed writer is aborted", async t => {
  const h = await fixture(t); await editBoth(h);
  h.disk.hooks.write = async path => {
    if (path === "parts/b.md") await h.disk.set("a.md", "External editor won\n");
  };
  await assert.rejects(h.session.save(() => true), { code: "DIRECTORY_CHANGED" });
  assert.equal(await h.disk.text("a.md"), "External editor won\n");
  assert.equal(await h.disk.text("parts/b.md"), "Original B\n");
  assert.deepEqual(closePaths(h), []); assert.equal(h.disk.locked.size, 0);
});

for (const change of ["detach", "lock", "dispose", "revision"]) {
  test(`post-refresh guard ${change} cannot acknowledge an unowned disk baseline`, async t => {
    const h = await fixture(t); await h.bind();
    await h.disk.set("a.md", "External source\r\n"); h.disk.reset();
    await assert.rejects(h.session.refresh(() => true, (next, expected) => {
      const stamp = h.install(next, expected);
      h.guard(() => {
        if (change === "lock") h.block();
        else if (change === "revision") h.session.changed();
        else h.session[change]();
      });
      return stamp;
    }), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
    assert.equal(h.files[0].source, "External source\r\n", "the applied source is retained; no rollback is claimed");
    assert.equal(h.session.state.folder, null);
    assert.deepEqual(closePaths(h), []);
    if (change === "detach") assert.match(h.session.state.message, /^Source folder disconnected/);
  });
}

test("a host observer can detach during startup without a stale operation replacing its status", async t => {
  const h = await fixture(t); await editBoth(h);
  h.observe(value => { if (value.busy === "Save source files") h.session.detach(); });
  await assert.rejects(h.session.save(() => true), { code: "STALE_SOURCE" });
  assert.deepEqual(closePaths(h), []); assert.equal(h.session.state.receipt, null);
  assert.match(h.notices.at(-1).message, /^Source folder disconnected/);
});

test("an initial access callback cannot transfer an old folder request to a newly detached book", async t => {
  const h = await fixture(t);
  let picks = 0;
  h.guard(() => h.session.detach());
  await assert.rejects(h.session.bind(() => { picks++; return h.disk.root; }, () => true), { code: "STALE_SOURCE" });
  assert.equal(picks, 0); assert.equal(h.session.state.folder, null);
  assert.match(h.session.state.message, /^Source folder disconnected/);
});

test("an initial source capture cannot publish operation status after its host detached the book", async t => {
  const h = await fixture(t);
  let picks = 0;
  h.read(() => h.session.detach());
  await assert.rejects(h.session.bind(() => { picks++; return h.disk.root; }, () => true), { code: "STALE_SOURCE" });
  assert.equal(picks, 0); assert.equal(h.session.state.folder, null);
  assert.match(h.notices.at(-1).message, /^Source folder disconnected/);
});

test("a nested access notification cannot publish the old connected state after detaching", async t => {
  const h = await fixture(t); await editBoth(h);
  h.guard(() => h.session.detach());
  h.session.changed();
  assert.equal(h.notices.at(-1).folder, null);
  assert.equal(h.notices.at(-1).canSave, false);
  assert.match(h.notices.at(-1).message, /^Source folder disconnected/);
});

test("refresh verification cannot adopt a new revision from its post-install source reader", async t => {
  const h = await fixture(t); await h.bind();
  await h.disk.set("a.md", "External source\r\n"); h.disk.reset();
  await assert.rejects(h.session.refresh(() => true, (next, expected) => {
    const stamp = h.install(next, expected);
    h.read(() => h.session.changed());
    return stamp;
  }), { code: "DIRECTORY_REFRESH_UNCERTAIN" });
  assert.equal(h.files[0].source, "External source\r\n");
  assert.equal(h.session.state.folder, null); assert.deepEqual(closePaths(h), []);
});
