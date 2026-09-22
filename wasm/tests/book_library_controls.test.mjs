// Real workbench + library controllers. DOM, worker and storage are explicit
// test doubles; native Node File/Blob/object URLs exercise source I/O. These
// tests do not stand in for native IndexedDB or browser acceptance.

import assert from "node:assert/strict";
import { File } from "node:buffer";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createBookCollection, normalizeBookProject } from "../demo/book_collection.mjs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookLibraryControls } from "../demo/book_library_controls.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
class Element extends EventTarget {
  #value = "";
  children = [];
  textContent = "";
  disabled = false;
  checked = false;
  hidden = false;
  get value() {
    return this.#value;
  }
  set value(value) {
    this.#value = String(value).replace(/\r\n?/g, "\n");
  }
  removeAttribute(name) {
    delete this[name];
  }
  replaceChildren(...children) {
    this.children = children;
  }
  click() {
    const event = new Event("click", { cancelable: true });
    this.dispatchEvent(event);
    return event;
  }
  set innerHTML(_) {
    throw new Error("Source or metadata must never be inserted as HTML");
  }
}
function storeDouble() {
  let serial = 0;
  const records = new Map();
  const api = {
    beforeRead: null,
    requests: [],
    async list() {
      return [...records.values()].map((record) => structuredClone(record.entry));
    },
    async save(value) {
      api.requests.push(structuredClone(value));
      const id = value.id ?? `saved-${++serial}`,
        old = records.get(id);
      if (value.id && old?.entry.revision !== value.expectedRevision)
        throw Object.assign(new Error("Other writer"), { code: "LIBRARY_CONFLICT" });
      const revision = (old?.entry.revision ?? 0) + 1;
      const entry = {
        id,
        name: value.name,
        revision,
        updatedAt: revision,
        versions: [
          { revision, savedAt: revision, bytes: 100, name: value.name },
          ...(old?.entry.versions ?? []),
        ],
      };
      records.set(id, {
        entry,
        versions: new Map([
          ...(old?.versions ?? []),
          [revision, normalizeBookProject(value.project)],
        ]),
      });
      return structuredClone(entry);
    },
    async read(id, version = null) {
      if (api.beforeRead) await api.beforeRead();
      const saved = records.get(id),
        selected = version ?? saved.entry.revision;
      return {
        id,
        revision: saved.entry.revision,
        snapshotRevision: selected,
        name: saved.entry.name,
        project: structuredClone(saved.versions.get(selected)),
      };
    },
    async remove(id, expectedRevision) {
      if (records.get(id)?.entry.revision !== expectedRevision)
        throw Object.assign(new Error("Other writer"), { code: "LIBRARY_CONFLICT" });
      records.delete(id);
    },
    close() {},
  };
  return api;
}
async function setup(t, options = {}) {
  const html = readFileSync(new URL("../demo/book.html", import.meta.url), "utf8");
  const el = Object.fromEntries(
    [...html.matchAll(/id="([^"]+)"/g)].map((match) => [match[1], new Element()]),
  );
  const root = { querySelector: (query) => el[query.slice(1)], createElement: () => new Element() };
  const collection = createBookCollection(),
    host = new EventTarget(),
    store = storeDouble();
  let library,
    confirmation = options.confirm ?? (() => true),
    cancels = 0;
  const worker = {
    cancel() {
      cancels++;
    },
    dispose() {},
    async render() {
      throw new Error("Unused engine double");
    },
  };
  const controls = createBookControls({
    root,
    collection,
    worker,
    confirm: (message) => confirmation(message),
    onProjectReplaced: () => library?.detach(),
  });
  library = createBookLibraryControls({
    root,
    controls,
    collection,
    store,
    window: host,
    confirm: (message) => confirmation(message),
  });
  t.after(() => {
    library.dispose();
    controls.dispose();
  });
  await library.ready;
  return {
    root,
    el,
    collection,
    controls,
    library,
    host,
    store,
    get cancels() {
      return cancels;
    },
    confirm(fn) {
      confirmation = fn;
    },
  };
}
function type(el, value, event = "input") {
  el.value = value;
  el.dispatchEvent(new Event(event));
}
async function initial(h) {
  await h.controls.importFiles([
    new File(["\ufeff# One\r\n"], "one.md"),
    new File(["# Two"], "two.md"),
  ]);
}

test("named save captures current DOM/source order and never includes image authority", async (t) => {
  const h = await setup(t);
  await initial(h);
  await h.controls.importFiles([new File(["image"], "a.svg")]);
  type(h.el["library-name"], "Manual");
  type(h.el["chapter-source"], "# Edited");
  type(h.el.title, "Book metadata");
  h.el["move-down"].click();
  h.el["library-save"].click();
  await tick();
  const saved = h.store.requests.at(-1);
  assert.equal(saved.name, "Manual");
  assert.deepEqual(
    saved.project.files.map((file) => file.path),
    ["two.md", "one.md"],
  );
  assert.equal(saved.project.files[1].source, "# Edited");
  assert.equal(saved.project.options.title, "Book metadata");
  assert.deepEqual(Object.keys(saved.project), ["schemaVersion", "files", "options"]);
  assert.equal(h.library.session.state.dirty, false);
  assert.match(h.el["library-status"].textContent, /Saved locally/);
});
test("selecting a different saved book does not redirect the current save target", async (t) => {
  const h = await setup(t);
  await initial(h);
  const first = await h.library.session.save();
  type(h.el["library-name"], "Separate");
  const second = await h.library.session.save({ copy: true });
  type(h.el["library-books"], first.id, "change");
  type(h.el["chapter-source"], "changed second");
  h.el["library-save"].click();
  await tick();
  assert.equal(h.store.requests.at(-1).id, second.id);
  assert.equal(h.library.session.state.binding.id, second.id);
  assert.equal((await h.store.read(first.id)).project.files[0].source, "\ufeff# One\r\n");
});
test("history controls restore original bytes and revoke old assets and downloads", async (t) => {
  const h = await setup(t);
  await initial(h);
  const first = await h.library.session.save();
  type(h.el["chapter-source"], "second version");
  await h.library.session.save();
  await h.controls.importFiles([new File(["image"], "a.svg")]);
  h.controls.prepareSource(true);
  const url = h.el.download.href;
  type(h.el["library-books"], first.id, "change");
  type(h.el["library-versions"], "1", "change");
  h.el["library-open"].click();
  await tick();
  assert.equal(h.collection.files[0].source, "\ufeff# One\r\n");
  assert.equal(h.el["chapter-source"].value, "\ufeff# One\n");
  assert.equal(h.collection.images.length, 0);
  assert(h.el.download.hidden);
  await assert.rejects(fetch(url));
  assert.equal(h.library.session.state.binding.revision, 2);
  assert(h.library.session.state.dirty);
  h.controls.prepareSource(false);
  assert.equal(await (await fetch(h.el.download.href)).text(), "# One\r\n");
  const bytes = new Uint8Array(await (await fetch(h.el.download.href)).arrayBuffer());
  assert.deepEqual(bytes, new TextEncoder().encode("\ufeff# One\r\n"));
});
test("project-file replacement detaches the previous autosave target", async (t) => {
  const h = await setup(t);
  await initial(h);
  const first = await h.library.session.save();
  h.library.session.setAutosave(true);
  const project = {
    schemaVersion: 1,
    files: [{ path: "imported.md", source: "new book" }],
    options: {},
  };
  await h.controls.importFiles([new File([JSON.stringify(project)], "book.json")], "project");
  assert.equal(h.library.session.state.binding, null);
  assert.equal(h.library.session.state.autosave, false);
  h.el["library-save"].click();
  await tick();
  assert.notEqual(h.library.session.state.binding.id, first.id);
  assert.equal(h.store.requests.at(-1).id, null);
});
test("invalid publishing settings cannot save stale model settings", async (t) => {
  const h = await setup(t);
  await initial(h);
  await h.library.session.save();
  type(h.el["font-scale"], "0");
  h.el["library-save"].click();
  await tick();
  assert.equal(h.store.requests.length, 1);
  assert(h.library.session.state.dirty);
  assert.match(h.el["library-status"].textContent, /Font scale/);
  h.controls.prepareSource(false);
  assert.equal(h.el.download.download, "one.md");
});
test("recovery checkpoint distinguishes invalid raw number inputs with the same numeric value", async (t) => {
  const h = await setup(t);
  await initial(h);
  const first = await h.library.session.save();
  type(h.el["font-scale"], "0");
  const hold = deferred();
  h.store.beforeRead = () => hold.promise;
  const opening = h.library.session.open(first.id);
  type(h.el["font-scale"], "");
  hold.resolve();
  await assert.rejects(opening, { code: "STALE_SOURCE" });
  assert.equal(h.el["font-scale"].value, "");
});
test("recovery checks current DOM even when no input event was dispatched", async (t) => {
  const h = await setup(t);
  await initial(h);
  const first = await h.library.session.save();
  const hold = deferred();
  h.confirm(() => hold.promise);
  const opening = h.library.session.open(first.id);
  await tick();
  h.el["chapter-source"].value = "unreported edit";
  hold.resolve(true);
  await assert.rejects(opening, { code: "STALE_SOURCE" });
  assert.equal(h.el["chapter-source"].value, "unreported edit");
});
test("unsaved-change warning is removed after save and page suspension, restored after resume", async (t) => {
  const h = await setup(t);
  await initial(h);
  const before = () => {
    const event = new Event("beforeunload", { cancelable: true });
    Object.defineProperty(event, "returnValue", { value: "", writable: true });
    h.host.dispatchEvent(event);
    return event.defaultPrevented;
  };
  assert(before());
  await h.library.session.save();
  assert.equal(before(), false);
  type(h.el["chapter-source"], "unsaved");
  assert(before());
  h.library.suspend();
  assert.equal(before(), false);
  h.library.resume();
  assert(before());
  assert.equal(h.library.session.state.autosave, false);
});
test("composition disables library replacement and autosave controls", async (t) => {
  const h = await setup(t);
  await initial(h);
  await h.library.session.save();
  h.el["chapter-source"].dispatchEvent(new Event("compositionstart"));
  assert(h.el["library-save"].disabled);
  assert(h.el["library-open"].disabled);
  assert(h.el["library-autosave"].disabled);
  h.el["chapter-source"].dispatchEvent(new Event("compositionend"));
  assert.equal(h.el["library-save"].disabled, false);
});
test("saved names render only as text and deleting the selected record preserves the editor", async (t) => {
  const h = await setup(t);
  await initial(h);
  type(h.el["library-name"], "<img src=x onerror=bad()>");
  await h.library.session.save();
  assert.match(h.el["library-books"].children[0].textContent, /^<img/);
  h.el["library-delete"].click();
  await tick();
  assert.equal((await h.store.list()).length, 0);
  assert.equal(h.collection.files.length, 2);
  assert.equal(h.library.session.state.binding, null);
  h.el["save-project"].click();
  assert.equal(h.el.download.download, "book.fmdbook.json");
});
