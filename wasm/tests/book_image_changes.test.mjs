import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { createBookCollection, readPortableBookProject } from "../demo/book_collection.mjs";

const bytes = (...values) => new Uint8Array(values);
const replace = (destination, data) => ({ destination, bytes: data });
const remove = destination => ({ destination, remove: true });
function fixture() {
  const book = createBookCollection();
  book.append({
    chapters: [
      { path: "guide/start.md", source: "\ufeff# Start\r\n![plot](../assets/plot.png)\r\n" },
      { path: "end.md", source: "![plot](assets/plot.png)\n{{#include shared.txt}}" },
    ],
    includeSources: [{ path: "shared.txt", source: "Shared\rtext" }],
    images: [replace("assets/plot.png", bytes(1, 2)), replace("cover.jpg", bytes(3)), replace("map.svg", bytes(4))],
  });
  book.setFonts([{ slot: "body-regular", name: "body.ttf", weight: 450, bytes: bytes(5) }], book.revision);
  book.setPage({ size: "a4", margins: 36 }, book.revision);
  return book;
}
function stamp(book) {
  return { revision: book.revision, configuration: book.renderConfigurationRevision, snapshot: book.snapshot() };
}

test("replacement and removal install once, preserving all source, settings, fonts and survivor order", () => {
  const book = fixture(), before = stamp(book), events = [];
  book.subscribe(() => events.push(stamp(book)));
  assert.equal(book.changeImages([remove("cover.jpg"), replace("assets/plot.png", bytes(9, 8, 7))], book.revision), before.revision + 1);
  assert.equal(events.length, 1);
  assert.equal(events[0].configuration, before.configuration + 1);
  const after = book.snapshot();
  assert.deepEqual(after.files, before.snapshot.files);
  assert.deepEqual({ ...after.options, images: [] }, { ...before.snapshot.options, images: [] });
  assert.deepEqual(after.options.images, [replace("assets/plot.png", bytes(9, 8, 7)), replace("map.svg", bytes(4))]);
  assert.deepEqual(events[0].snapshot, after);
});

test("exact byte replacement is a no-op; stale no-ops still reject", () => {
  const book = fixture(), before = stamp(book);
  book.subscribe(() => assert.fail("no-op must not invalidate outputs"));
  assert.equal(book.changeImages([replace("assets/plot.png", bytes(1, 2))], book.revision), before.revision);
  assert.deepEqual(stamp(book), before);
  assert.throws(() => book.changeImages([replace("assets/plot.png", bytes(1, 2))], book.revision - 1), { code: "STALE_SOURCE" });
});

test("all ordinary failures preserve the complete collection", () => {
  const book = fixture(), before = stamp(book);
  const invalid = [null, [], Array(129).fill(remove("map.svg")), [null], [remove("absent.png")],
    [remove("map.svg"), remove("map.svg")], [remove("map.svg"), replace("cover.jpg", bytes())],
    [replace("../escape.png", bytes(1))], [{ destination: "map.svg", remove: false }],
    [{ destination: "map.svg", remove: true, bytes: bytes(1) }],
    [{ destination: "map.svg", bytes: bytes(1), url: "https://example.com" }],
    [{ destination: "map.svg", bytes: new DataView(new ArrayBuffer(2)) }],
    [replace("map.svg", new Uint16Array(1))], [replace("map.svg", new Uint8Array(8 * 1024 * 1024 + 1))],
    [Object.assign(Object.create({}), remove("map.svg"))],
    [{ destination: "map.svg", remove: true, [Symbol("hidden")]: 1 }]];
  for (const changes of invalid) {
    assert.throws(() => book.changeImages(changes, book.revision));
    assert.deepEqual(stamp(book), before);
  }
  for (const rev of [undefined, NaN, -1, "3", 0.5, book.revision + 1])
    assert.throws(() => book.changeImages([remove("map.svg")], rev), { code: "STALE_SOURCE" });
});

test("property getters, sparse batches and custom iterators cannot supply hidden changes", () => {
  const book = fixture(), before = stamp(book);
  for (const key of ["destination", "bytes", "remove"]) {
    const change = key === "remove" ? remove("map.svg") : replace("map.svg", bytes(5));
    Object.defineProperty(change, key, { get() { assert.fail("getter invoked"); } });
    assert.throws(() => book.changeImages([change], book.revision), { code: "INVALID_IMAGE_CHANGE" });
  }
  const sparse = new Array(2); sparse[0] = remove("map.svg");
  assert.throws(() => book.changeImages(sparse, book.revision), { code: "INVALID_IMAGE_CHANGE" });
  const indexed = [remove("map.svg")];
  Object.defineProperty(indexed, "0", { get() { assert.fail("array getter invoked"); } });
  assert.throws(() => book.changeImages(indexed, book.revision), { code: "INVALID_IMAGE_CHANGE" });
  assert.deepEqual(stamp(book), before);
  const good = [remove("map.svg")];
  good[Symbol.iterator] = () => { assert.fail("custom iterator invoked"); };
  book.changeImages(good, book.revision);
  assert.equal(book.images.length, 2);
});

test("caller subviews are privately copied without slice, iterator or fake byte metadata", () => {
  const book = fixture(), backing = bytes(255, 9, 8, 254), view = backing.subarray(1, 3);
  view.slice = () => { assert.fail("slice override invoked"); };
  view[Symbol.iterator] = () => { assert.fail("iterator invoked"); };
  Object.defineProperty(view, "byteLength", { get() { assert.fail("byteLength getter invoked"); } });
  Object.defineProperty(view, "buffer", { get() { assert.fail("buffer getter invoked"); } });
  book.changeImages([replace("map.svg", view)], book.revision);
  backing.fill(0);
  const exposed = book.snapshot();
  assert.deepEqual(exposed.options.images.at(-1).bytes, bytes(9, 8));
  exposed.options.images.at(-1).bytes.fill(255);
  assert.deepEqual(book.snapshot().options.images.at(-1).bytes, bytes(9, 8));
});

test("shared, detached, spoofed and resizable buffers cannot enter the image store", () => {
  const book = fixture(), before = stamp(book);
  const detached = bytes(1); structuredClone(detached, { transfer: [detached.buffer] });
  const shared = new SharedArrayBuffer(2);
  Object.defineProperty(shared, Symbol.toStringTag, { value: "ArrayBuffer" });
  const values = [detached, new Uint8Array(shared), { [Symbol.toStringTag]: "Uint8Array", byteLength: 1 }];
  if (Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "resizable"))
    values.push(new Uint8Array(new ArrayBuffer(2, { maxByteLength: 3 })));
  for (const value of values) {
    assert.throws(() => book.changeImages([replace("map.svg", value)], book.revision));
    assert.deepEqual(stamp(book), before);
  }
});

test("final-set budgeting supports replacement at capacity and order-independent funding", () => {
  const m = 1024 * 1024;
  const make = () => {
    const book = createBookCollection();
    book.append({ chapters: [], images: [0, 1, 2, 3].map(i => replace(`${i}.png`, new Uint8Array(8 * m))) });
    return book;
  };
  const book = make(), replacement = new Uint8Array(8 * m).fill(17);
  book.changeImages([replace("0.png", replacement)], book.revision);
  assert.equal(book.images.reduce((s, image) => s + image.size, 0), 32 * m);
  for (const order of [false, true]) {
    const b = make();
    b.append({ chapters: [], images: [] });
    b.changeImages([replace("0.png", bytes(1))], b.revision);
    const changes = [replace("0.png", replacement), remove("1.png")];
    b.changeImages(order ? changes.reverse() : changes, b.revision);
    assert.deepEqual(b.images.map(i => i.destination), ["0.png", "2.png", "3.png"]);
  }
});

test("over-capacity final set rejects without installing an earlier removal", () => {
  const m = 1024 * 1024, book = createBookCollection();
  book.append({ chapters: [], images: [0, 1, 2, 3].map(i => replace(`${i}.png`, new Uint8Array(8 * m - 1))) });
  book.append({ chapters: [], images: [replace("tiny.png", bytes(1)), replace("other.png", bytes(2))] });
  const before = book.images, revision = book.revision;
  assert.throws(() => book.changeImages([remove("tiny.png"), replace("other.png", new Uint8Array(8 * m))], revision), { code: "BOOK_LIMIT" });
  assert.deepEqual(book.images, before);
  assert.equal(book.revision, revision);
});

test("reentrant changes and disposal during descriptor admission cannot overwrite newer state", () => {
  for (const dispose of [false, true]) {
    const book = fixture(), revision = book.revision;
    let done = false;
    const change = new Proxy(remove("map.svg"), { ownKeys(target) {
      if (!done) { done = true; if (dispose) book.dispose(); else book.changeImages([replace("cover.jpg", bytes(99))], revision); }
      return Reflect.ownKeys(target);
    } });
    assert.throws(() => book.changeImages([change], revision), { code: dispose ? "SESSION_DISPOSED" : "STALE_SOURCE" });
    if (!dispose) {
      assert.equal(book.images.length, 3);
      assert.deepEqual(book.snapshot().options.images[1].bytes, bytes(99));
    }
  }
});

test("changed images round-trip in portable backups while source-only files stay unchanged", async () => {
  const book = fixture(), source = await book.projectDownload().blob.text();
  book.changeImages([replace("assets/plot.png", bytes(127, 0, 255)), remove("map.svg")], book.revision);
  assert.equal(await book.projectDownload().blob.text(), source);
  const output = await book.portableDownload();
  const review = await readPortableBookProject(new File([output.blob], output.filename));
  const reopened = createBookCollection();
  reopened.replacePortableProject(review, 0, { authorizeResources: true });
  assert.deepEqual(reopened.snapshot(), book.snapshot());
  book.edit(0, book.files[0].path, book.files[0].source.replace(/\r\n?/g, "\n"));
  assert.equal(book.files[0].source, reopened.files[0].source);
});

test("observers cannot turn a committed image change into a reported rollback", () => {
  const book = fixture(), revision = book.revision;
  book.subscribe(() => { throw Error("observer failed"); });
  assert.equal(book.changeImages([remove("map.svg")], revision), revision + 1);
  assert.equal(book.images.length, 2);
});

test("seeded replacement/removal batches match an independent reference store", () => {
  const book = createBookCollection(), reference = new Map();
  for (let i = 0; i < 100; i++) reference.set(`${i}.png`, [i]);
  book.append({ chapters: [{ path: "x.md", source: "# Original" }], images: [...reference].map(([key, data]) => replace(key, Uint8Array.from(data))) });
  let state = 731;
  for (let n = 0; n < 80; n++) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    const key = [...reference.keys()][state % reference.size], value = [state % 255, n];
    const deleting = state % 3 === 0;
    book.changeImages([deleting ? remove(key) : replace(key, Uint8Array.from(value))], book.revision);
    if (deleting) reference.delete(key); else reference.set(key, value);
    assert.deepEqual(book.snapshot().options.images, [...reference].map(([k, v]) => replace(k, Uint8Array.from(v))));
    assert.equal(book.files[0].source, "# Original");
  }
});
