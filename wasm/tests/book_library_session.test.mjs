// Explicit store/host/timer doubles test orchestration, NOT native IndexedDB.

import assert from "node:assert/strict";
import test from "node:test";
import { bookError } from "../book_worker.mjs";
import { createBookCollection, normalizeBookProject } from "../demo/book_collection.mjs";
import { createBookLibrarySession } from "../demo/book_library_session.mjs";

const project = (source = "# Original\r\n") => ({
  schemaVersion: 1,
  files: [{ path: "a.md", source }],
  options: {},
});
const tick = () => new Promise((resolve) => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
function storage() {
  const records = new Map(),
    requests = [];
  let serial = 0,
    closed = false;
  const api = {
    requests,
    records,
    beforeSave: null,
    beforeRead: null,
    error: null,
    async list() {
      if (api.error) throw api.error;
      return [...records.values()].map((value) => structuredClone(value.entry));
    },
    async save(input) {
      input = structuredClone(input);
      requests.push(input);
      if (api.beforeSave) await api.beforeSave();
      if (api.error) throw api.error;
      if (closed) throw bookError("LIBRARY_CLOSED", "closed");
      const id = input.id ?? `book-${++serial}`,
        old = records.get(id);
      if (input.id && old?.entry.revision !== input.expectedRevision)
        throw bookError("LIBRARY_CONFLICT", "changed elsewhere");
      const revision = (old?.entry.revision ?? 0) + 1;
      const snapshot = { project: normalizeBookProject(input.project), name: input.name.trim() };
      const versions = [revision, ...(old?.entry.versions ?? []).map((item) => item.revision)].map(
        (rev) => ({
          revision: rev,
          savedAt: rev,
          bytes: 100,
          name: rev === revision ? snapshot.name : old.snapshots.get(rev).name,
        }),
      );
      const entry = { id, name: snapshot.name, revision, updatedAt: revision, versions };
      records.set(id, {
        entry,
        snapshots: new Map([...(old?.snapshots ?? []), [revision, snapshot]]),
      });
      return structuredClone(entry);
    },
    async read(id, revision = null) {
      if (api.beforeRead) await api.beforeRead();
      if (api.error) throw api.error;
      const saved = records.get(id),
        selected = revision ?? saved?.entry.revision,
        snapshot = saved?.snapshots.get(selected);
      if (!snapshot) throw bookError("LIBRARY_NOT_FOUND", "missing");
      return {
        id,
        revision: saved.entry.revision,
        snapshotRevision: selected,
        ...structuredClone(snapshot),
      };
    },
    async remove(id, expectedRevision) {
      if (api.error) throw api.error;
      if (records.get(id)?.entry.revision !== expectedRevision)
        throw bookError("LIBRARY_CONFLICT", "changed elsewhere");
      records.delete(id);
    },
    close() {
      closed = true;
    },
  };
  return api;
}
function harness({ store = storage(), confirm = () => true, onState = () => {} } = {}) {
  const collection = createBookCollection();
  collection.replaceProject(project());
  let raw = "",
    session,
    next = 0;
  const jobs = new Map(),
    timers = {
      setTimeout: (fn) => {
        jobs.set(++next, fn);
        return next;
      },
      clearTimeout: (id) => jobs.delete(id),
    };
  const host = {
    checkpoint: () => `${collection.revision}:${raw}`,
    captureProject: () => {
      if (raw) throw bookError("INVALID_OPTIONS", "invalid typed setting");
      return collection.project();
    },
    replaceProject: (value, stamp) => {
      assert.equal(stamp, host.checkpoint());
      collection.replaceProject(value);
    },
  };
  session = createBookLibrarySession({ store, host, confirm, timers, onState });
  collection.subscribe(() => session.changed());
  return {
    session,
    store,
    collection,
    jobs,
    host,
    edit: (text) => collection.edit(0, "a.md", text),
    raw: (value) => {
      raw = value;
      session.changed();
    },
    fire: async () => {
      const pending = [...jobs.values()];
      jobs.clear();
      for (const fn of pending) fn();
      await tick();
    },
    close: () => {
      session.dispose();
      collection.dispose();
    },
  };
}

test("no automatic writes or restores occur on startup or refresh", async () => {
  const h = harness();
  await h.session.refresh();
  assert.equal(h.store.requests.length, 0);
  assert.equal(h.session.state.autosave, false);
  assert.equal(h.collection.files[0].source, "# Original\r\n");
  h.close();
});
test("save acknowledgment never marks newer edits clean", async () => {
  const h = harness(),
    gate = deferred();
  h.store.beforeSave = () => gate.promise;
  const saving = h.session.save();
  h.edit("newer");
  assert.equal(h.session.state.binding, null);
  gate.resolve();
  await saving;
  assert.equal(h.session.state.binding.revision, 1);
  assert.equal(h.session.state.dirty, true);
  assert.equal(h.store.requests[0].project.files[0].source, "# Original\r\n");
  h.close();
});
test("autosave coalesces bursts and follows edits made during an in-flight save", async () => {
  const h = harness();
  await h.session.save();
  h.session.setAutosave(true);
  h.edit("a");
  h.edit("b");
  h.edit("latest");
  assert.equal(h.jobs.size, 1);
  const gate = deferred();
  h.store.beforeSave = () => gate.promise;
  await h.fire();
  h.edit("during-save");
  assert.equal(h.jobs.size, 0);
  gate.resolve();
  await tick();
  assert.equal(h.jobs.size, 1);
  h.store.beforeSave = null;
  await h.fire();
  assert.equal(h.store.requests.length, 3);
  assert.equal(h.store.requests[2].project.files[0].source, "during-save");
  assert.equal(h.session.state.dirty, false);
  h.close();
});
test("conflicts pause autosave without advancing the observed revision", async () => {
  const h = harness();
  const first = await h.session.save();
  await h.store.save({
    id: first.id,
    expectedRevision: 1,
    name: "Other tab",
    project: project("other"),
  });
  h.session.setAutosave(true);
  h.edit("my edit");
  await h.fire();
  assert.equal(h.session.state.conflict, true);
  assert.equal(h.session.state.autosave, false);
  assert.equal(h.session.state.binding.revision, 1);
  assert.equal(h.collection.files[0].source, "my edit");
  await h.session.refresh();
  assert.equal(h.session.state.binding.revision, 1);
  const copy = await h.session.save({ copy: true });
  assert.notEqual(copy.id, first.id);
  assert.equal((await h.store.read(first.id)).project.files[0].source, "other");
  assert.equal(h.session.state.conflict, false);
  h.close();
});
test("refresh detects deletion but never silently changes the save target", async () => {
  const h = harness();
  const first = await h.session.save();
  await h.store.remove(first.id, 1);
  await h.session.refresh();
  assert.equal(h.session.state.conflict, true);
  assert.equal(h.session.state.binding.id, first.id);
  h.close();
});
test("recovery is rejected when source changes during its read", async () => {
  const h = harness();
  const first = await h.session.save();
  const gate = deferred();
  h.store.beforeRead = () => gate.promise;
  const opening = h.session.open(first.id);
  h.edit("new edit");
  gate.resolve();
  await assert.rejects(opening, { code: "STALE_SOURCE" });
  assert.equal(h.collection.files[0].source, "new edit");
  h.close();
});
test("recovery is rejected when source or the local name changes during confirmation", async () => {
  for (const kind of ["source", "name"]) {
    const gate = deferred(),
      h = harness({ confirm: () => gate.promise });
    const first = await h.session.save();
    const opening = h.session.open(first.id);
    await tick();
    if (kind === "source") h.edit("during dialog");
    else h.session.rename("typed during dialog");
    gate.resolve(true);
    await assert.rejects(opening, { code: "STALE_SOURCE" });
    h.close();
  }
});
test("cancelled recovery preserves source, grants and disables autosave", async () => {
  const h = harness({ confirm: () => false });
  const first = await h.session.save();
  h.session.setAutosave(true);
  h.collection.append({
    chapters: [],
    images: [{ destination: "a.png", bytes: new Uint8Array([1]) }],
  });
  assert.equal(await h.session.open(first.id), false);
  assert.equal(h.collection.images.length, 1);
  assert.equal(h.session.state.autosave, false);
  h.close();
});
test("historical recovery revokes image grants and saves forward from the current head", async () => {
  const h = harness();
  const first = await h.session.save();
  h.edit("v2");
  await h.session.save();
  h.collection.append({
    chapters: [],
    images: [{ destination: "a.png", bytes: new Uint8Array([1]) }],
  });
  await h.session.open(first.id, 1);
  assert.equal(h.collection.images.length, 0);
  assert.equal(h.session.state.dirty, true);
  assert.equal(h.session.state.binding.revision, 2);
  const next = await h.session.save();
  assert.equal(next.revision, 3);
  assert.equal((await h.store.read(first.id, 2)).project.files[0].source, "v2");
  h.close();
});
test("import replacement during a pending save cannot rebind the imported book", async () => {
  const h = harness(),
    gate = deferred();
  h.store.beforeSave = () => gate.promise;
  const saving = h.session.save();
  h.collection.replaceProject(project("imported"));
  h.session.detach();
  gate.resolve();
  await saving;
  assert.equal(h.session.state.binding, null);
  assert.equal(h.session.state.dirty, true);
  assert.equal(h.session.state.autosave, false);
  assert.equal(h.collection.files[0].source, "imported");
  h.close();
});
test("suspension fences pending recovery even after resume", async () => {
  const h = harness();
  const first = await h.session.save();
  const gate = deferred();
  h.store.beforeRead = () => gate.promise;
  const opening = h.session.open(first.id);
  h.session.suspend();
  h.session.resume();
  gate.resolve();
  await assert.rejects(opening, { code: "STALE_SOURCE" });
  h.close();
});
test("an already-started save may acknowledge during suspension without enabling autosave", async () => {
  const h = harness(),
    gate = deferred();
  h.store.beforeSave = () => gate.promise;
  const saving = h.session.save();
  h.session.suspend();
  gate.resolve();
  await saving;
  assert.equal(h.session.state.binding.revision, 1);
  assert.equal(h.session.state.autosave, false);
  h.session.resume();
  h.close();
});
test("invalid typed settings pause autosave, without saving stale valid settings instead", async () => {
  const h = harness();
  await h.session.save();
  h.session.setAutosave(true);
  h.raw("bad scale");
  await h.fire();
  assert.equal(h.store.requests.length, 1);
  assert.equal(h.session.state.autosave, false);
  assert.equal(h.session.state.dirty, true);
  h.raw("");
  await h.session.save();
  assert.equal(h.session.state.binding.revision, 2);
  h.close();
});
test("name edits during save remain dirty and are not overwritten by the receipt", async () => {
  const h = harness(),
    gate = deferred();
  h.store.beforeSave = () => gate.promise;
  const saving = h.session.save();
  h.session.rename("new name");
  gate.resolve();
  await saving;
  assert.equal(h.session.state.name, "new name");
  assert.equal(h.session.state.dirty, true);
  h.close();
});
test("composition pauses autosave until the final source state is available", async () => {
  const h = harness();
  await h.session.save();
  h.session.setAutosave(true);
  h.session.composition(true);
  h.edit("partial");
  assert.equal(h.jobs.size, 0);
  h.edit("complete");
  h.session.composition(false);
  assert.equal(h.jobs.size, 1);
  await h.fire();
  assert.equal(h.store.requests.at(-1).project.files[0].source, "complete");
  h.close();
});
test("delete is compare-and-swap and never deletes the current editor", async () => {
  const h = harness();
  const first = await h.session.save();
  h.edit("unsaved");
  await h.session.remove(first.id, 1);
  assert.equal(h.collection.files[0].source, "unsaved");
  assert.equal(h.session.state.binding, null);
  assert.equal(h.store.records.size, 0);
  h.close();
});
test("another writer during delete confirmation prevents deletion", async () => {
  const gate = deferred(),
    h = harness({ confirm: () => gate.promise });
  const first = await h.session.save();
  const deleting = h.session.remove(first.id, 1);
  await tick();
  await h.store.save({
    id: first.id,
    expectedRevision: 1,
    name: "Other",
    project: project("other"),
  });
  gate.resolve(true);
  await assert.rejects(deleting, { code: "LIBRARY_CONFLICT" });
  assert.equal(h.store.records.size, 1);
  h.close();
});
test("new book clears only the editor and detaches the saved project", async () => {
  const h = harness();
  await h.session.save();
  await h.session.newBook();
  assert.equal(h.collection.files.length, 0);
  assert.equal(h.store.records.size, 1);
  assert.equal(h.session.state.binding, null);
  h.close();
});
test("concurrent operations reject instead of accumulating a hidden write queue", async () => {
  const h = harness(),
    gate = deferred();
  h.store.beforeSave = () => gate.promise;
  const first = h.session.save();
  await assert.rejects(h.session.save(), { code: "BOOK_BUSY" });
  assert.throws(() => h.session.setAutosave(true), { code: "BOOK_BUSY" });
  gate.resolve();
  await first;
  assert.equal(h.store.requests.length, 1);
  h.close();
});
test("observer failure cannot change an acknowledged save", async () => {
  const h = harness({
    onState() {
      throw new Error("observer");
    },
  });
  await h.session.save();
  assert.equal(h.session.state.binding.revision, 1);
  assert.equal(h.session.state.dirty, false);
  h.close();
});
test("dispose clears pending autosave and ignores late recovery", async () => {
  const h = harness();
  const first = await h.session.save();
  h.session.setAutosave(true);
  h.edit("dirty");
  assert.equal(h.jobs.size, 1);
  const gate = deferred();
  h.store.beforeRead = () => gate.promise;
  const opening = h.session.open(first.id);
  h.session.dispose();
  gate.resolve();
  await assert.rejects(opening, { code: "LIBRARY_CLOSED" });
  assert.equal(h.jobs.size, 0);
  h.collection.dispose();
});

test("failure from an old save cannot attach conflict state to an imported project", async () => {
  const h = harness(),
    gate = deferred();
  h.store.beforeSave = () => gate.promise;
  const saving = h.session.save();
  h.collection.replaceProject(project("imported"));
  h.session.detach();
  gate.reject(bookError("LIBRARY_CONFLICT", "old save lost its race"));
  await assert.rejects(saving, { code: "LIBRARY_CONFLICT" });
  assert.equal(h.session.state.binding, null);
  assert.equal(h.session.state.conflict, false);
  assert.equal(h.session.state.dirty, true);
  assert.equal(h.collection.files[0].source, "imported");
  h.close();
});
