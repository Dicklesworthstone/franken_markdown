import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import { createLocalImageSources } from "../demo/local_image_sources.mjs";

const context = () => ({ signal: new AbortController().signal, maxBytes: 8 * 1024 * 1024 });
const file = (name, bytes = [1, 2]) => new File([new Uint8Array(bytes)], name);

test("only exact selected local names and their explicit encoded aliases are authorized", async () => {
  const sources = createLocalImageSources([file("a b.png")]);
  for (const url of ["a b.png", "a%20b.png", "./a%20b.png"])
    assert.deepEqual(await sources.load({ url }, context()), new Uint8Array([1, 2]));
  for (const url of [
    "../a%20b.png",
    "/a%20b.png",
    "//host/a%20b.png",
    "https://host/a%20b.png",
    "a%20b.png?q=1",
    "a%2520b.png",
  ]) {
    assert.equal(await sources.load({ url }, context()), null);
  }
  assert.deepEqual(sources.references, ["![a b.png](a%20b.png)"]);
  assert.equal(sources.count, 1);
});

test("filename ambiguity and traversal-shaped names are rejected atomically", () => {
  for (const names of [
    ["a.png", "a.png"],
    ["a b.png", "a%20b.png"],
    ["../a.png"],
    ["x:y.png"],
    ["a\\b.png"],
    [".."],
  ]) {
    assert.throws(
      () => createLocalImageSources(names.map((n) => file(n))),
      (e) => e.code === "INVALID_LOCAL_IMAGES",
    );
  }
});

test("file and aggregate limits reject before reading any bytes", async () => {
  const large = new File([new Uint8Array(8 * 1024 * 1024 + 1)], "large.png");
  assert.throws(() => createLocalImageSources([large]));
  assert.throws(() =>
    createLocalImageSources(Array.from({ length: 129 }, (_, i) => file(`${i}.png`))),
  );
  assert.throws(() => createLocalImageSources([new File([], "empty.png")]));
  const sources = createLocalImageSources([file("a.png")]);
  await assert.rejects(sources.load({ url: "a.png" }, { ...context(), maxBytes: 1 }));
  await assert.rejects(
    sources.load({ url: "a.png" }, { ...context(), signal: AbortSignal.abort() }),
  );
});

test("source references escape alt delimiters and loader grants survive caller array mutation", async () => {
  const files = [file("a[b].png")],
    sources = createLocalImageSources(files);
  files.length = 0;
  assert.deepEqual(sources.references, ["![a\\[b\\].png](a%5Bb%5D.png)"]);
  assert.deepEqual(await sources.load({ url: "a%5Bb%5D.png" }, context()), new Uint8Array([1, 2]));
  assert(Object.isFrozen(sources) && Object.isFrozen(sources.references));
});

test("generated Markdown escapes unmatched destination parentheses", async () => {
  const sources = createLocalImageSources([file("a)b(.png")]);
  assert.deepEqual(sources.references, ["![a)b(.png](a%29b%28.png)"]);
  assert.deepEqual(await sources.load({ url: "a%29b%28.png" }, context()), new Uint8Array([1, 2]));
});
