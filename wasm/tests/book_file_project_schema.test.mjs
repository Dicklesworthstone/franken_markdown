import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { mkdtemp, readFile, writeFile, open, rename } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { createBookFileControls } from "../demo/book_file_controls.mjs";
import { createBookCollection, normalizeBookProject, readBookProject, serializeBookProject } from "../demo/book_collection.mjs";
import { rootDocument } from "./book_publication_archival_helpers.mjs";

// The DOM and file picker are host adapters. Source validation, collection,
// connected-file state machine and all disk bytes use their production paths.
async function fixture(t, publication, storedVersion) {
  const book = createBookCollection(); t.after(() => book.dispose());
  book.append({ chapters: [{ path: "one.md", source: "\ufeff# One\r\n\r\n本文 😀\r\n" }], images: [] });
  book.setPublication(publication, book.revision);
  const saved = book.project();
  if (storedVersion !== undefined) saved.schemaVersion = storedVersion;
  const dir = await mkdtemp(join(tmpdir(), "fmd-connected-schema-"));
  const path = join(dir, "book.json"); await writeFile(path, serializeBookProject(saved));
  const handle = {
    kind: "file", name: "book.json",
    async getFile() { return new File([await readFile(path)], "book.json", { type: "application/json" }); },
    async requestPermission(options) { assert.deepEqual(options, { mode: "readwrite" }); return "granted"; },
    async createWritable(options) {
      assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
      const stage = join(dir, `stage-${randomUUID()}`), file = await open(stage, "wx");
      let closed = false;
      return {
        async write(blob) { assert(blob instanceof Blob); await file.writeFile(new Uint8Array(await blob.arrayBuffer())); },
        async close() { await file.sync(); await file.close(); closed = true; await rename(stage, path); },
        async abort() { if (!closed) { await file.close(); closed = true; } },
      };
    },
  };
  const window = new EventTarget();
  window.showOpenFilePicker = async () => [handle];
  window.showSaveFilePicker = async () => handle;
  window.confirm = () => true;
  const controls = {
    sourceBusy: false,
    captureProject: () => book.project(),
    checkpoint: () => String(book.revision),
    replaceProject: (project, revision) => {
      assert.equal(revision, String(book.revision));
      return book.replaceProject(project, book.revision);
    },
  };
  const files = createBookFileControls({ root: rootDocument(), window, controls, collection: book,
    projectCodec: { normalizeBookProject, readBookProject, serializeBookProject } });
  t.after(() => files.dispose());
  return { book, files, bytes: () => readFile(path, "utf8"), write: value => writeFile(path, value) };
}

for (const [publication, schemaVersion] of [[{ pdfA: "2b", pdfAStrict: true }, 5],
  [{ baselineGridPt: 13, pdfA: "2b" }, 6],
  [{ profile: "commonmark-gfm", baselineGridPt: 13, pdfA: "2b" }, 7]]) {
  test(`connected files open and save schema ${schemaVersion} without downgrading settings`, async t => {
    const h = await fixture(t, publication);
    assert.equal((await h.files.open()).opened, true);
    assert.equal(h.files.state.phase, "saved");
    h.book.edit(0, "one.md", "# Edited\r\n\r\nExact 😀\r\n");
    assert.equal(h.files.state.phase, "edited");
    const result = await h.files.save();
    assert.equal(result.current, true); assert.equal(h.files.state.phase, "saved");
    const saved = JSON.parse(await h.bytes());
    assert.equal(saved.schemaVersion, schemaVersion);
    for (const [key, value] of Object.entries(publication)) assert.equal(saved.options[key], value);
    assert.deepEqual(saved.files, h.book.project().files);
    assert.equal(await h.bytes(), serializeBookProject(h.book.project()));
  });
}

test("equivalent supported project versions share identity while dialect edits mark the file dirty", async t => {
  const h = await fixture(t, {}, 7);
  assert.equal((await h.files.open()).opened, true);
  assert.equal(h.book.project().schemaVersion, 1);
  assert.equal(h.files.state.phase, "saved", "minimal-schema normalization is not a source edit");
  h.book.setPublication({ profile: "gfm-plus" }, h.book.revision);
  assert.equal(h.files.state.phase, "edited");
  await h.files.save();
  assert.equal(JSON.parse(await h.bytes()).schemaVersion, 7);
  assert.equal(JSON.parse(await h.bytes()).options.profile, "gfm-plus");
});

test("schema-independent identity still rejects external disk changes", async t => {
  const h = await fixture(t, { profile: "commonmark-gfm" });
  await h.files.open();
  h.book.setPublication({ profile: "gfm-plus" }, h.book.revision);
  const external = (await h.bytes()).replace("本文", "外部");
  await h.write(external);
  await assert.rejects(h.files.save(), { code: "FILE_CHANGED" });
  assert.equal(await h.bytes(), external);
  assert.equal(h.book.options.profile, "gfm-plus");
});
