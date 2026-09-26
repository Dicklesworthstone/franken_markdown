import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import { createDirectoryImageSources, createLocalImageSources } from "../demo/local_image_sources.mjs";
import { FlowAssetError, rasterInfo } from "../flow_raster.mjs";

const context = () => ({ signal: new AbortController().signal, maxBytes: 8 * 1024 * 1024 });
function file(path, bytes = [1, 2]) {
  const value = new File([new Uint8Array(bytes)], path.split("/").at(-1));
  Object.defineProperty(value, "webkitRelativePath", { value: path, configurable: true });
  return value;
}
const read = (sources, url, controls = context()) => sources.load({ url }, controls);
const invalid = (fn) => assert.throws(fn, (error) => error instanceof FlowAssetError && error.code === "INVALID_LOCAL_IMAGES");

test("a selected Markdown tree keeps nested and same-basename image identities", async () => {
  const sources = createDirectoryImageSources([
    file("project/docs/guide.md", [99]), file("project/one/plot.png", [1]),
    file("project/two/plot.png", [2]), file("project/notes.txt", [98]),
  ]);
  assert.equal(sources.count, 2);
  assert.equal(sources.rootName, "project");
  assert.deepEqual(await read(sources, "one/plot.png"), new Uint8Array([1]));
  assert.deepEqual(await read(sources, "./two/plot.png"), new Uint8Array([2]));
  for (const url of ["plot.png", "project/one/plot.png", "docs/guide.md", "notes.txt"])
    assert.equal(await read(sources, url), null);
});

test("a nested document resolves parent references only within its selected root", async () => {
  const sources = createDirectoryImageSources([
    file("project/docs/figure.png", [1]), file("project/shared/figure.png", [2]),
    file("project/docs/chapter/figure.png", [3]),
  ], { documentPath: "docs/chapter/readme.md" });
  for (const [url, byte] of [["figure.png", 3], ["../figure.png", 1],
    ["../../shared/figure.png", 2], ["./.././figure.png", 1]])
    assert.deepEqual(await read(sources, url), new Uint8Array([byte]));
  for (const url of ["../../../shared/figure.png", "../../../project/shared/figure.png", "/shared/figure.png"])
    assert.equal(await read(sources, url), null);
  assert.deepEqual(sources.references, ["![figure.png](figure.png)",
    "![figure.png](../figure.png)", "![figure.png](../../shared/figure.png)"]);
});

test("URI segments decode once, including Unicode, spaces and literal percent names", async () => {
  const sources = createDirectoryImageSources([
    file("root/图 表/a b.png", [1]), file("root/图 表/a%20b.png", [2]),
    file("root/图 表/a[b](c).jpg", [3]),
  ]);
  assert.deepEqual(await read(sources, "%E5%9B%BE%20%E8%A1%A8/a%20b.png"), new Uint8Array([1]));
  assert.deepEqual(await read(sources, "图 表/a%2520b.png"), new Uint8Array([2]));
  assert.deepEqual(await read(sources, "图 表/a%5bb%5d%28c%29.jpg"), new Uint8Array([3]));
  assert(sources.references.some((value) => value.includes("a\\[b\\](c).jpg](%E5%9B%BE%20%E8%A1%A8/a%5Bb%5D%28c%29.jpg)")));
});

test("outside roots, schemes, queries, fragments and disguised separators acquire no access", async () => {
  const sources = createDirectoryImageSources([file("root/img/a.png")], { documentPath: "docs/a.md" });
  for (const url of ["../../img/a.png", "/img/a.png", "//host/img/a.png", "https://host/img/a.png",
    "file:///img/a.png", "data:image/png;base64,abc", "../img/a.png?q", "../img/a.png#part",
    "../img%2fa.png", "..%2fimg/a.png", "%2e%2e/img/a.png", "../img%5ca.png", "../img/a.png%00",
    "../img/../..//img/a.png", "../img/%ED%A0%80.png", "../img/%XX.png", "../img/a.png\n",
    "../img/a.png\u007f", "../img/a.png\ud800", "../img\\a.png", null, 23])
    assert.equal(await read(sources, url), null, JSON.stringify(url));
});

test("bad directory metadata and ambiguous roots reject the entire grant before reads", () => {
  const cases = [[], [file("root/a.png"), file("other/b.png")],
    [file("root/a.png"), file("root/a.png")], [file("root/../a.png")], [file("root//a.png")],
    [file("/root/a.png")], [file("root/a:b.png")], [file("root/a\ud800.png")],
    [file("root/" + "a/".repeat(64) + "a.png")], [new File(["x"], "a.png")]];
  for (const files of cases.slice(1)) invalid(() => createDirectoryImageSources(files));
  const mismatch = file("root/a.png");
  Object.defineProperty(mismatch, "webkitRelativePath", { value: "root/b.png" });
  invalid(() => createDirectoryImageSources([mismatch]));
  assert.equal(createDirectoryImageSources([]).count, 0);
});

test("document base is a literal filename and cannot inject URL resolution policy", () => {
  for (const documentPath of ["../a.md", "/a.md", "a//b.md", "a\\b.md", "https://host/a.md",
    "a.md?x", "a.md#x", "a.html", "", 4, null]) {
    if (documentPath === "") continue;
    invalid(() => createDirectoryImageSources([], { documentPath }));
  }
  assert.equal(createDirectoryImageSources([], { documentPath: "docs/a b.MD" }).documentPath, "docs/a b.MD");
});

test("only bounded raster files are retained; directory admission never reads any file", async () => {
  let reads = 0;
  const image = file("root/a.png");
  const note = file("root/private.md", new Uint8Array(9 * 1024 * 1024));
  for (const entry of [image, note]) entry.arrayBuffer = () => { reads++; throw new Error("unexpected read"); };
  const sources = createDirectoryImageSources([image, note]);
  assert.equal(reads, 0);
  assert.equal(await read(sources, "private.md"), null);
  assert.equal(reads, 0);
  invalid(() => createDirectoryImageSources(Array.from({ length: 129 }, (_, n) => file(`root/${n}.png`))));
  invalid(() => createDirectoryImageSources(Array.from({ length: 4097 }, (_, n) => file(`root/${n}.txt`))));
  invalid(() => createDirectoryImageSources([file("root/a.png", [])]));
  invalid(() => createDirectoryImageSources([file("root/a.png", new Uint8Array(8 * 1024 * 1024 + 1))]));
  const block = new Uint8Array(8 * 1024 * 1024);
  invalid(() => createDirectoryImageSources(Array.from({ length: 5 }, (_, n) => file(`root/${n}.png`, block))));
});

test("path metadata budget applies even to ignored files", () => {
  const component = "a".repeat(500);
  invalid(() => createDirectoryImageSources(Array.from({ length: 600 }, (_, n) =>
    file(`root/${component}/${component}/${component}/${component}/${n}.txt`))));
});

test("captured membership and generated references are stable and private", async () => {
  const files = [file("root/z.png", [9]), file("root/a.png", [1])];
  const sources = createDirectoryImageSources(files);
  const reordered = createDirectoryImageSources([...files].reverse());
  assert.deepEqual(sources.references, reordered.references);
  Object.defineProperty(files[1], "webkitRelativePath", { value: "root/reassigned.png" });
  files.length = 0;
  assert.deepEqual(await read(sources, "a.png"), new Uint8Array([1]));
  assert.equal(await read(sources, "reassigned.png"), null);
  const first = await read(sources, "z.png");
  first[0] = 42;
  assert.deepEqual(await read(sources, "z.png"), new Uint8Array([9]));
  assert(Object.isFrozen(sources) && Object.isFrozen(sources.references));
});

test("loader cancellation and byte ceilings apply before and after asynchronous reads", async () => {
  const image = file("root/a.png");
  const sources = createDirectoryImageSources([image]);
  for (const maxBytes of [1, -1, Infinity, NaN, null])
    await assert.rejects(read(sources, "a.png", { ...context(), maxBytes }));
  await assert.rejects(read(sources, "a.png", { ...context(), signal: AbortSignal.abort() }));
  let release;
  image.arrayBuffer = () => new Promise((resolve) => { release = resolve; });
  const abort = new AbortController();
  const pending = read(sources, "a.png", { ...context(), signal: abort.signal });
  abort.abort();
  release(new Uint8Array([1, 2]).buffer);
  await assert.rejects(pending);
  image.arrayBuffer = async () => new Uint8Array([1, 2, 3]).buffer;
  await assert.rejects(read(sources, "a.png"), (e) => e.code === "INVALID_LOCAL_IMAGES");
});

test("generated references round-trip all images at multiple document depths", async () => {
  const files = [file("root/a.png", [1]), file("root/docs/图.png", [2]),
    file("root/img/a b.png", [3]), file("root/img/%E5.png", [4]), file("root/deep/more/a).jpeg", [5])];
  for (const documentPath of ["", "README.md", "docs/guide.md", "deep/more/guide.md", "x/y/z/guide.md"]) {
    const sources = createDirectoryImageSources(files, { documentPath });
    const found = new Set();
    for (const value of sources.references) {
      const url = value.slice(value.indexOf("](") + 2, -1);
      const bytes = await read(sources, url);
      assert(bytes, value);
      found.add(bytes[0]);
    }
    assert.deepEqual([...found].sort(), [1, 2, 3, 4, 5]);
  }
});

test("folder bytes still enter the real raster admission rather than trusting extensions", async () => {
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jlp8AAAAASUVORK5CYII=", "base64");
  const sources = createDirectoryImageSources([file("root/img/a.png", png), file("root/b.png", [1, 2])]);
  const limits = { maxAssetBytes: 8 * 1024 * 1024, maxDimension: 8192, maxImagePixels: 16777216 };
  assert.equal(rasterInfo(await read(sources, "img/a.png"), limits).pixels, 1);
  assert.throws(() => rasterInfo(new Uint8Array([1, 2]), limits), (e) => e.code === "UNSUPPORTED_IMAGE");
  assert.deepEqual(await read(sources, "b.png"), new Uint8Array([1, 2]));
});

test("flat picker behavior is not silently reinterpreted as directory authority", async () => {
  const image = file("root/docs/a.png");
  const flat = createLocalImageSources([image]);
  assert.deepEqual(await read(flat, "a.png"), new Uint8Array([1, 2]));
  assert.equal(await read(flat, "docs/a.png"), null);
  const malformed = new File(["x"], "a.png");
  Object.defineProperty(malformed, "name", { value: "a\ud800.png" });
  invalid(() => createLocalImageSources([malformed]));
});


test("encoded path growth and long legal relative ascents still round-trip", async () => {
  const directory = "图".repeat(500);
  const entry = file(`root/${directory}/${directory}/a.png`, [7]);
  const sources = createDirectoryImageSources([entry], { documentPath: "guide.md" });
  const url = sources.references[0].slice(sources.references[0].indexOf("](") + 2, -1);
  assert(url.length > 4096);
  assert.deepEqual(await read(sources, url), new Uint8Array([7]));
  const imagePath = Array(60).fill("images").join("/") + "/a.png";
  const documentPath = Array(60).fill("docs").join("/") + "/a.md";
  const deep = createDirectoryImageSources([file(`root/${imagePath}`, [8])], { documentPath });
  const relative = deep.references[0].slice(deep.references[0].indexOf("](") + 2, -1);
  assert(relative.split("/").length > 64);
  assert.deepEqual(await read(deep, relative), new Uint8Array([8]));
});

test("expanded reference inventory is bounded before insertion into an editor", () => {
  const part = "图".repeat(500);
  invalid(() => createDirectoryImageSources(Array.from({ length: 128 }, (_, n) =>
    file(`root/${part}/${part}/${n}.png`))));
});
