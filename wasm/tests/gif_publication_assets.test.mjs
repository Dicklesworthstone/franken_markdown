import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { readFileSync } from "node:fs";
import {
  BOOK_WORKBENCH_LIMITS,
  createBookCollection,
  readBookFiles,
  readPortableBookProject,
} from "../demo/book_collection.mjs";
import {
  captureDocumentImageFiles,
  createDirectoryImageSources,
} from "../demo/local_image_sources.mjs";

const animation = () => new Uint8Array(Buffer.from(readFileSync(
  new URL("../../tests/fixtures/gif/animated-colors.base64", import.meta.url), "utf8",
).trim(), "base64"));

function file(path, bytes) {
  const result = new File([bytes], path.split("/").at(-1));
  if (path.includes("/")) Object.defineProperty(result, "webkitRelativePath", { value: path });
  return result;
}

test("book imports and portable round trips retain every GIF animation byte", async () => {
  const bytes = animation();
  const source = "# Animated instructions\n\n![Steps](motion.GIF)\n";
  const imported = await readBookFiles([
    file("guide.md", source), file("motion.GIF", bytes),
  ]);
  assert.equal(imported.ignored, 0);
  assert.equal(imported.images.length, 1);
  assert.equal(imported.images[0].destination, "motion.GIF");
  assert.deepEqual(imported.images[0].bytes, bytes);
  const book = createBookCollection();
  book.append(imported);
  book.setPublication({ running: { header: { image: { dest: "motion.GIF" } } } }, book.revision);
  const saved = await book.portableDownload();
  const review = await readPortableBookProject(new File([saved.blob], "book.fmdbook.bundle.json"));
  const reopened = createBookCollection();
  reopened.replacePortableProject(review, reopened.revision, { authorizeResources: true });
  assert.deepEqual(reopened.snapshot(), book.snapshot());
  assert.deepEqual(reopened.snapshot().options.images[0].bytes, bytes);
  assert.equal(reopened.files[0].source, source);
  assert.equal(reopened.options.running.header.image.dest, "motion.GIF");
});

test("book folder import keeps nested GIF keys and does not read unrelated files", async () => {
  const bytes = animation();
  const privateFile = file("manual/private.bmp", "private");
  privateFile.arrayBuffer = () => { throw Error("unrelated bytes were read"); };
  const imported = await readBookFiles([
    file("manual/guide/first.md", "![Steps](../assets/motion.gif)\n"),
    file("manual/assets/motion.gif", bytes), privateFile,
  ], { folder: true });
  assert.equal(imported.ignored, 1);
  assert.equal(imported.images[0].destination, "assets/motion.gif");
  assert.deepEqual(imported.images[0].bytes, bytes);
  const book = createBookCollection();
  book.append(imported);
  const previous = book.snapshot(), revision = book.revision;
  assert.throws(() => book.append({
    chapters: [{ path: "new.md", source: "new" }],
    images: [{ destination: "unsupported.bmp", bytes }],
  }), { code: "INVALID_IMAGE" });
  assert.equal(book.revision, revision);
  assert.deepEqual(book.snapshot(), previous);
});

test("document GIF selection preserves literal and encoded aliases in flat and folder grants", async () => {
  const bytes = animation();
  for (const folder of [false, true]) {
    const selectedFile = file(folder ? "manual/assets/motion demo.GIF" : "motion demo.GIF", bytes);
    const selected = captureDocumentImageFiles([selectedFile], folder
      ? { folder: true, documentPath: "guide/first.md" } : {});
    assert.equal(selected.count, 1);
    const assets = new Map((await selected.read()).map(asset => [asset.destination, asset.bytes]));
    const base = folder ? "../assets/" : "";
    for (const prefix of ["", "./"]) {
      for (const name of ["motion demo.GIF", "motion%20demo.GIF"])
        assert.deepEqual(assets.get(prefix + base + name), bytes);
    }
    assert.equal(assets.has("https://example.test/motion.gif"), false);
    assert.equal(assets.has("../../motion.gif"), false);
  }
  // The separate measured raster adapter still admits only PNG/JPEG.
  assert.equal(createDirectoryImageSources([file("manual/motion.gif", bytes)]).count, 0);
});

test("GIF image limits are enforced before reading selected files", async () => {
  const oversized = file("large.gif", animation());
  Object.defineProperty(oversized, "size", { value: BOOK_WORKBENCH_LIMITS.imageBytes + 1 });
  oversized.arrayBuffer = () => { throw Error("oversized GIF was read"); };
  await assert.rejects(readBookFiles([oversized]), { code: "FILE_LIMIT" });
  assert.throws(() => captureDocumentImageFiles([oversized]), { code: "INVALID_LOCAL_IMAGES" });
});
