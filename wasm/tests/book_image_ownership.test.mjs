// Production collection and capture admission. The retained worker below is an
// explicit routing double: these tests prove byte ownership and invalidation,
// not PDF, image-decoder, native renderer or generated-WASM behavior.
import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import test from "node:test";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookCollectionCapture } from "../book_worker.mjs";

class AliasedSlice extends Uint8Array {
  slice(start, end) { return this.subarray(start, end); }
}
const file = () => ({ path: "one.md", source: "# One\n\n![Cover](cover.png)\n" });
const counters = book => [
  book.revision, book.renderConfigurationRevision, book.renderResourceRevision,
];
const imageBytes = book => book.snapshot().options.images.map(image => Array.from(image.bytes));
function append(book, bytes, path = "cover.png") {
  book.append({ chapters: [file()], images: [{ destination: path, bytes }] });
}

test("append captures the exact image view without retaining an aliasing slice", () => {
  const book = createBookCollection();
  const backing = new AliasedSlice([91, 1, 2, 3, 92]);
  const input = backing.subarray(1, 4);
  append(book, input);
  const before = counters(book);
  backing.fill(99);
  assert.deepEqual(imageBytes(book), [[1, 2, 3]]);
  assert.deepEqual(counters(book), before);
  assert.deepEqual(book.images, [{ destination: "cover.png", size: 3 }]);
  book.dispose();
});

test("append never invokes caller slice or typed-array species while copying images", () => {
  let slices = 0, species = 0;
  class HostBytes extends Uint8Array {
    static get [Symbol.species]() { species++; throw new Error("caller species ran"); }
    slice() { slices++; throw new Error("caller slice ran"); }
  }
  const book = createBookCollection(), input = new HostBytes([4, 5, 6]);
  append(book, input);
  input.fill(80);
  assert.deepEqual(imageBytes(book), [[4, 5, 6]]);
  assert.equal(slices, 0);
  assert.equal(species, 0);
  book.dispose();
});

test("append owns Node Buffer image subranges despite Buffer.slice sharing memory", () => {
  const book = createBookCollection();
  const backing = Buffer.from([91, 7, 8, 9, 92]), input = backing.subarray(1, 4);
  append(book, input);
  const before = counters(book);
  backing.fill(70);
  assert.deepEqual(imageBytes(book), [[7, 8, 9]]);
  assert.deepEqual(counters(book), before);
  const output = book.snapshot().options.images[0].bytes;
  assert.equal(Object.getPrototypeOf(output), Uint8Array.prototype);
  assert.equal(output.byteLength, 3);
  book.dispose();
});

test("reused chapter and image inputs cannot retarget either admitted collection", () => {
  const first = createBookCollection(), second = createBookCollection();
  const input = new AliasedSlice([10, 11]);
  const source = file(), image = { destination: "cover.png", bytes: input };
  const batch = { chapters: [source], images: [image] };
  first.append(batch);
  second.append(batch);
  const sourceBefore = first.files, firstBefore = counters(first), secondBefore = counters(second);
  source.path = "changed.md";
  source.source = "changed source";
  image.destination = "changed.png";
  input.fill(60);
  batch.chapters.length = 0;
  batch.images.length = 0;
  assert.deepEqual(first.files, sourceBefore);
  assert.deepEqual(second.files, sourceBefore);
  assert.deepEqual(imageBytes(first), [[10, 11]]);
  assert.deepEqual(imageBytes(second), [[10, 11]]);
  assert.deepEqual(counters(first), firstBefore);
  assert.deepEqual(counters(second), secondBefore);
  first.dispose();
  assert.deepEqual(imageBytes(second), [[10, 11]]);
  second.dispose();
});

test("later appends preserve earlier image ownership and copy only admitted new views", () => {
  const book = createBookCollection();
  const first = new AliasedSlice([12, 13]), second = new AliasedSlice([14, 15]);
  append(book, first);
  first.fill(50);
  book.append({
    chapters: [{ path: "two.md", source: "# Two\n" }],
    images: [{ destination: "second.png", bytes: second }],
  });
  second.fill(51);
  assert.deepEqual(imageBytes(book), [[12, 13], [14, 15]]);
  const before = counters(book);
  assert.throws(() => book.append({
    chapters: [{ path: "three.md", source: "# Three\n" }],
    images: [{ destination: "cover.png", bytes: new Uint8Array([16]) }],
  }), { code: "DUPLICATE_PATH" });
  assert.deepEqual(counters(book), before);
  assert.equal(book.files.length, 2);
  assert.deepEqual(imageBytes(book), [[12, 13], [14, 15]]);
  book.dispose();
});

test("outbound resource snapshots and metadata cannot mutate installed image grants", () => {
  const book = createBookCollection();
  append(book, new Uint8Array([17, 18]));
  const before = counters(book), first = book.snapshot(), second = book.snapshot();
  const metadata = book.images;
  first.options.images[0].bytes.fill(45);
  first.options.images[0].destination = "other.png";
  first.options.images.length = 0;
  metadata[0].destination = "metadata.png";
  metadata[0].size = 999;
  assert.deepEqual(Array.from(second.options.images[0].bytes), [17, 18]);
  assert.deepEqual(imageBytes(book), [[17, 18]]);
  assert.deepEqual(book.images, [{ destination: "cover.png", size: 2 }]);
  assert.deepEqual(counters(book), before);
  book.dispose();
});

test("explicit image changes keep no-op identity and own replacement views", () => {
  const book = createBookCollection();
  append(book, new Uint8Array([19, 20]));
  const before = counters(book), same = new AliasedSlice([19, 20]);
  assert.equal(book.changeImages([{ destination: "cover.png", bytes: same }], book.revision), before[0]);
  same.fill(40);
  assert.deepEqual(counters(book), before);
  assert.deepEqual(imageBytes(book), [[19, 20]]);
  const replacement = new AliasedSlice([21, 22]);
  book.changeImages([{ destination: "cover.png", bytes: replacement }], book.revision);
  replacement.fill(41);
  assert.deepEqual(imageBytes(book), [[21, 22]]);
  assert.deepEqual(counters(book), before.map(value => value + 1));
  assert.throws(() => book.changeImages([
    { destination: "cover.png", bytes: new Uint8Array([23]) },
  ], before[0]), { code: "STALE_SOURCE" });
  assert.deepEqual(imageBytes(book), [[21, 22]]);
  book.dispose();
});

test("retained source publications keep captured images until an explicit resource change", async () => {
  const book = createBookCollection(), input = new AliasedSlice([24, 25]);
  append(book, input);
  let transport = 0, retainedImages = [], retainedFiles = [];
  const calls = [];
  const result = kind => {
    calls.push(kind);
    return { retainedInputRevision: ++transport,
      images: retainedImages.map(bytes => bytes.slice()),
      files: retainedFiles.map(entry => ({ ...entry })) };
  };
  const worker = {
    supportsSourceSet: true,
    get retainedInputRevision() { return transport || null; },
    async render(files, _format, options) {
      retainedFiles = files.map(entry => ({ ...entry }));
      retainedImages = options.images.map(image => Array.from(image.bytes));
      return result("full");
    },
    async renderSources(files, _format, options) {
      assert.equal(options.expectedRevision, transport);
      retainedFiles = files.map(entry => ({ ...entry }));
      return result("sources");
    },
    async renderSourceSet(files, _format, options) {
      assert.equal(options.expectedRevision, transport);
      retainedFiles = files.map(entry => ({ ...entry }));
      return result("source-set");
    },
  };
  const capture = createBookCollectionCapture(book, worker);
  assert.deepEqual((await capture().render("pdf")).images, [[24, 25]]);
  const originalResource = book.renderResourceRevision;
  input.fill(30);
  assert.deepEqual(imageBytes(book), [[24, 25]]);
  book.edit(0, "one.md", "# Edited\n");
  const edited = await capture().render("pdf");
  assert.deepEqual(edited.images, [[24, 25]]);
  assert.equal(edited.files[0].source, "# Edited\n");
  assert.equal(book.renderResourceRevision, originalResource);
  book.append({ chapters: [{ path: "two.md", source: "# Two\n" }], images: [] });
  assert.deepEqual((await capture().render("pdf")).images, [[24, 25]]);
  assert.equal(book.renderResourceRevision, originalResource);
  const replacement = new AliasedSlice([26, 27]);
  book.changeImages([{ destination: "cover.png", bytes: replacement }], book.revision);
  replacement.fill(31);
  assert.deepEqual((await capture().render("pdf")).images, [[26, 27]]);
  assert.equal(book.renderResourceRevision, originalResource + 1);
  assert.deepEqual(calls, ["full", "sources", "source-set", "full"]);
  book.dispose();
});
