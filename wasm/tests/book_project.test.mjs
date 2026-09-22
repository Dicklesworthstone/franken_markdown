import assert from "node:assert/strict";
import test from "node:test";
import {
  createBookCollection,
  normalizeBookProject,
  serializeBookProject,
} from "../demo/book_collection.mjs";
import { createBookLibraryStore } from "../demo/book_library_store.mjs";

const project = (source = "# Original\r\n") => ({
  schemaVersion: 1,
  files: [{ path: "a.md", source }],
  options: {},
});

test("source project snapshots are independent of mutable containers and grants", () => {
  const c = createBookCollection();
  c.replaceProject(project("\ufeff# 中🚀\r\nlast\r"));
  c.append({ chapters: [], images: [{ destination: "a.png", bytes: new Uint8Array([1]) }] });
  const copy = c.project();
  copy.files[0].source = "changed";
  copy.options.title = "changed";
  assert.equal(c.project().files[0].source, "\ufeff# 中🚀\r\nlast\r");
  assert.deepEqual(Object.keys(c.project()), ["schemaVersion", "files", "options"]);
  assert.equal(c.project().options.title, "My book");
  c.dispose();
});
test("serializer agrees with source downloads and preserves exact original text", async () => {
  const c = createBookCollection();
  c.replaceProject(project());
  assert.equal(await c.projectDownload().blob.text(), serializeBookProject(c.project()));
  assert.equal(JSON.parse(serializeBookProject(c.project())).files[0].source, "# Original\r\n");
  c.dispose();
});
test("sparse chapter arrays and forbidden project fields fail validation", () => {
  assert.throws(() => normalizeBookProject({ ...project(), files: new Array(1) }));
  for (const value of [
    { ...project(), images: [] },
    { ...project(), options: { fontAssets: [] } },
    { ...project(), schemaVersion: 2 },
  ]) {
    assert.throws(() => normalizeBookProject(value));
  }
});
test("invalid typed source remains in the editor but cannot overwrite recovery", () => {
  const c = createBookCollection();
  c.replaceProject(project());
  c.edit(0, "a.md", "bad\ud800");
  assert.throws(() => c.project(), /Unicode/);
  assert.equal(c.files[0].source, "bad\ud800");
  c.edit(0, "../incomplete", "valid");
  assert.throws(() => c.project(), /relative book path/);
  assert.equal(c.files[0].path, "../incomplete");
  c.dispose();
});
test("project serialization enforces escaped JSON size, not just source size", () => {
  const source = "\0".repeat(4 * 1024 * 1024);
  assert.throws(
    () =>
      serializeBookProject({
        ...project(),
        files: [0, 1, 2].map((i) => ({ path: `${i}.md`, source })),
      }),
    /budget/,
  );
});
test("storage unavailability is explicit and project validation precedes initialization", async () => {
  const store = createBookLibraryStore({ indexedDB: null });
  await assert.rejects(store.list(), { code: "LIBRARY_UNAVAILABLE" });
  await assert.rejects(store.save({ name: "bad", project: { ...project(), images: [] } }), {
    code: "INVALID_PROJECT",
  });
  store.close();
  await assert.rejects(store.list(), { code: "LIBRARY_CLOSED" });
});
