import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import { captureDocumentImageFiles, createDirectoryImageSources } from "../demo/local_image_sources.mjs";

function file(path, bytes = [1, 2, 3]) {
  const result = new File([new Uint8Array(bytes)], path.split("/").at(-1));
  if (path.includes("/")) Object.defineProperty(result, "webkitRelativePath", { value: path });
  return result;
}
const assetMap = assets => new Map(assets.map(asset => [asset.destination, [...asset.bytes]]));

test("flat image snapshots preserve exact PNG/JPEG/SVG bytes and bounded literal aliases", async () => {
  const files = [file("chart.png", [1]), file("photo.jpg", [2]), file("vector a.svg", [3])];
  const selected = captureDocumentImageFiles(files);
  files.length = 0;
  const assets = await selected.read();
  const byKey = assetMap(assets);
  assert.equal(selected.count, 3);
  assert.equal(selected.totalBytes, 3);
  for (const key of ["chart.png", "./chart.png"]) assert.deepEqual(byKey.get(key), [1]);
  for (const key of ["photo.jpg", "./photo.jpg"]) assert.deepEqual(byKey.get(key), [2]);
  for (const key of ["vector a.svg", "vector%20a.svg", "./vector a.svg", "./vector%20a.svg"])
    assert.deepEqual(byKey.get(key), [3]);
  assert.deepEqual(selected.references, ["![chart.png](chart.png)", "![photo.jpg](photo.jpg)", "![vector a.svg](vector%20a.svg)"]);
  assert.equal(byKey.has("../chart.png"), false);
  assert.equal(byKey.has("https://example.test/chart.png"), false);
  assert(Object.isFrozen(selected) && Object.isFrozen(assets) && assets.every(Object.isFrozen));
});

test("folder snapshots bind document-relative paths and do not read unrelated files", async () => {
  const secret = file("project/private.txt");
  secret.arrayBuffer = () => { throw new Error("private content was read"); };
  const markdown = file("project/docs/guide.md");
  markdown.arrayBuffer = secret.arrayBuffer;
  const selected = captureDocumentImageFiles([
    file("project/docs/local.png", [1]), file("project/shared/plot.png", [2]),
    file("project/shared/drawing.svg", [3]), file("project/图 表/photo.jpeg", [4]), secret, markdown,
  ], { folder: true, documentPath: "docs/guide.md" });
  assert.equal(selected.rootName, "project");
  assert.equal(selected.documentPath, "docs/guide.md");
  const assets = assetMap(await selected.read());
  assert.deepEqual(assets.get("local.png"), [1]);
  assert.deepEqual(assets.get("../shared/plot.png"), [2]);
  assert.deepEqual(assets.get("./../shared/drawing.svg"), [3]);
  assert.deepEqual(assets.get("../%E5%9B%BE%20%E8%A1%A8/photo.jpeg"), [4]);
  for (const key of ["project/shared/plot.png", "shared/plot.png", "private.txt", "guide.md", "/shared/plot.png", "../../shared/plot.png"])
    assert.equal(assets.has(key), false, key);
});

test("SVG document admission does not extend the measured Flow raster grant", async () => {
  const svg = file("root/drawing.svg", new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"/>'));
  assert.equal(createDirectoryImageSources([svg]).count, 0);
  const selected = captureDocumentImageFiles([svg], { folder: true });
  assert.equal(selected.count, 1);
  assert.equal(new TextDecoder().decode((await selected.read())[0].bytes), '<svg xmlns="http://www.w3.org/2000/svg"/>');
});

test("once-decoded directory aliases cannot bind a different literal-percent filename", async () => {
  const selected = captureDocumentImageFiles([
    file("root/a b.png", [1]), file("root/a%20b.png", [2]),
  ], { folder: true });
  const assets = assetMap(await selected.read());
  assert.deepEqual(assets.get("a b.png"), [1]);
  assert.deepEqual(assets.get("a%20b.png"), [1]);
  assert.deepEqual(assets.get("a%2520b.png"), [2]);
  assert.deepEqual(assets.get("./a%20b.png"), [1]);
  assert.throws(() => captureDocumentImageFiles([file("a b.png"), file("a%20b.png")]), { code: "INVALID_LOCAL_IMAGES" });
});

test("native key trimming cannot turn a leading-space filename into another image", async () => {
  const assets = assetMap(await captureDocumentImageFiles([file(" a.png", [1]), file("a.png", [2])]).read());
  assert.equal(assets.has(" a.png"), false);
  assert.deepEqual(assets.get("%20a.png"), [1]);
  assert.deepEqual(assets.get("./ a.png"), [1]);
  assert.deepEqual(assets.get("a.png"), [2]);
});

test("references escape Markdown delimiters without editing source or image bytes", async () => {
  const selected = captureDocumentImageFiles([file("a[b](c).png", [7])]);
  assert.deepEqual(selected.references, ["![a\\[b\\](c).png](a%5Bb%5D%28c%29.png)"]);
  assert.deepEqual(assetMap(await selected.read()).get("a%5Bb%5D%28c%29.png"), [7]);
});

test("each selected file is read once, and each read owns its byte snapshot", async () => {
  const selectedFile = file("a b.svg", [1, 2]);
  let reads = 0;
  const storage = new Uint8Array([1, 2]);
  selectedFile.arrayBuffer = async () => { reads++; return storage.buffer; };
  const selected = captureDocumentImageFiles([selectedFile]);
  const assets = await selected.read();
  assert.equal(reads, 1);
  storage.fill(9);
  assert(assets.every(asset => asset.bytes[0] === 1 && asset.bytes[1] === 2));
  assets[0].bytes[0] = 8;
  assert.equal((await selected.read())[0].bytes[0], 9);
});

for (const [name, files, options] of [
  ["empty", [], {}], ["unsupported flat file", [file("note.md")], {}],
  ["no images in folder", [file("root/note.md")], { folder: true }],
  ["mixed roots", [file("one/a.png"), file("two/b.png")], { folder: true }],
  ["duplicate paths", [file("root/a.png"), file("root/a.png")], { folder: true }],
  ["parent escape", [file("root/../a.png")], { folder: true }],
  ["external document path", [file("root/a.png")], { folder: true, documentPath: "https://host/a.md" }],
]) {
  test(`invalid ${name} is rejected before any file content is read`, () => {
    for (const entry of files) entry.arrayBuffer = () => { throw new Error("unexpected read"); };
    assert.throws(() => captureDocumentImageFiles(files, options), { code: "INVALID_LOCAL_IMAGES" });
  });
}

test("image count and byte limits reject before reads or alias allocation", () => {
  const sized = (name, size) => {
    const entry = file(name);
    Object.defineProperty(entry, "size", { value: size });
    entry.arrayBuffer = () => { throw new Error("unexpected read"); };
    return entry;
  };
  for (const entries of [
    Array.from({ length: 129 }, (_, i) => sized(`${i}.png`, 1)),
    [sized("empty.png", 0)], [sized("large.png", 8 * 1024 * 1024 + 1)],
    Array.from({ length: 5 }, (_, i) => sized(`${i}.png`, 8 * 1024 * 1024)),
    [sized("first image.png", 8 * 1024 * 1024), sized("second image.png", 8 * 1024 * 1024)],
  ]) assert.throws(() => captureDocumentImageFiles(entries), { code: "INVALID_LOCAL_IMAGES" });
  const admitted = captureDocumentImageFiles(Array.from({ length: 6 }, (_, i) => sized(`${i}.png`, 4 * 1024 * 1024)));
  assert.equal(admitted.count, 6);
  assert.equal(admitted.totalBytes, 24 * 1024 * 1024);
});

test("document key limits refuse long encoded paths and oversized alias metadata", () => {
  const unicode = "图".repeat(500);
  assert.throws(() => captureDocumentImageFiles([file(`root/${unicode}/${unicode}/a.png`)], { folder: true }), { code: "INVALID_LOCAL_IMAGES" });
  const parent = Array(5).fill("a".repeat(500)).join("/");
  assert.throws(() => captureDocumentImageFiles(Array.from({ length: 128 }, (_, i) =>
    file(`root/${parent}/${i}.png`)), { folder: true }), { code: "INVALID_LOCAL_IMAGES" });
});

test("cancelled reads settle immediately without starting the next file", async () => {
  let finish, reads = 0;
  const first = file("a.png"), second = file("b.png");
  first.arrayBuffer = () => new Promise(resolve => { finish = resolve; });
  second.arrayBuffer = async () => { reads++; return new Uint8Array([1, 2, 3]).buffer; };
  const selected = captureDocumentImageFiles([first, second]);
  const abort = new AbortController();
  const pending = selected.read(abort.signal);
  abort.abort(new Error("host cancelled"));
  await assert.rejects(pending, /host cancelled/);
  assert.equal(reads, 0);
  finish(new Uint8Array([1, 2, 3]).buffer);
  await Promise.resolve();
  assert.equal(reads, 0);
  await assert.rejects(selected.read(AbortSignal.abort()), { name: "AbortError" });
});

test("read failures and changed byte lengths never return a partial image selection", async () => {
  for (const read of [async () => { throw new Error("disk read failed"); }, async () => new Uint8Array([4]).buffer]) {
    const bad = file("b.svg");
    bad.arrayBuffer = read;
    await assert.rejects(captureDocumentImageFiles([file("a.png"), bad]).read());
  }
});
