import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings } from "../book_session.mjs";

const FILES = [
  { path: "guide/start.md", source: "# Start" },
  { path: "index.md", source: "# Home" },
];

function harness(overrides = {}) {
  const state = { loads: 0, constructions: 0, frees: 0, calls: [], instances: [] };
  class EngineBook {
    constructor(paths, sources) {
      state.constructions++;
      this.paths = paths;
      this.chapterCount = paths.length;
      this.sourceLength = sources.reduce(
        (n, source) => n + new TextEncoder().encode(source).length,
        0,
      );
      this.dead = false;
      state.instances.push(this);
    }
    setMetadata(...values) {
      state.calls.push(["metadata", ...values]);
    }
    setCustomCss(...values) {
      state.calls.push(["css", ...values]);
    }
    setTheme(...values) {
      state.calls.push(["theme", ...values]);
    }
    setNavigation(...values) {
      state.calls.push(["navigation", ...values]);
    }
    setFontScale(...values) {
      state.calls.push(["scale", ...values]);
    }
    setImage(key, bytes) {
      state.calls.push(["image", key, [...bytes]]);
    }
    setFont(key, bytes) {
      state.calls.push(["font", key, [...bytes]]);
    }
    setFontWeight(...values) {
      state.calls.push(["weight", ...values]);
    }
    renderPdf() {
      return new Uint8Array([37, 80, 68, 70]);
    }
    renderEpub() {
      return new Uint8Array([80, 75, 3, 4]);
    }
    renderSite() {
      return new Uint8Array([80, 75, 1, 2]);
    }
    free() {
      assert.equal(this.dead, false, "double free");
      this.dead = true;
      state.frees++;
    }
  }
  Object.assign(EngineBook.prototype, overrides);
  const api = createBookBindings(async () => {
    state.loads++;
    return EngineBook;
  });
  return { state, api };
}

test("one retained book renders all formats without reparsing", async () => {
  const { api, state } = harness();
  const session = await api.createBook(FILES, {
    title: "Manual",
    lang: "fr",
    toc: true,
    pageNumbers: true,
  });
  assert.equal(session.chapterCount, 2);
  assert.equal(session.sourceLength, 13);
  assert.deepEqual(state.instances[0].paths, ["guide/start.md", "index.md"]);
  const pdf = session.renderPdf();
  const epub = session.renderEpub();
  const site = session.renderSite();
  assert.equal(pdf.format, "book-pdf");
  assert.equal(epub.mimeType, "application/epub+zip");
  assert.equal(site.extension, "zip");
  assert.equal(state.constructions, 1);
  assert.equal(state.frees, 0);
  session.dispose();
  session.dispose();
  assert.equal(state.frees, 1);
  assert.deepEqual([...pdf.bytes], [37, 80, 68, 70]);
  assert.deepEqual(
    state.calls.find((call) => call[0] === "metadata"),
    ["metadata", "Manual", undefined, "fr"],
  );
});

test("input validation rejects bad requests before engine loading", async () => {
  const { api, state } = harness();
  for (const files of [
    [],
    null,
    {},
    [{ path: "a.md", source: 1 }],
    new Array(4097).fill(FILES[0]),
  ]) {
    await assert.rejects(api.createBook(files));
  }
  for (const options of [
    null,
    [],
    { toc: "false" },
    { title: 4 },
    { font: "unknown" },
    { darkMode: "unknown" },
  ]) {
    await assert.rejects(api.createBook(FILES, options));
  }
  assert.equal(state.loads, 0);
  assert.equal(state.constructions, 0);
});

test("source length is UTF-8 bytes rather than JavaScript UTF-16 units", async () => {
  const { api } = harness();
  const session = await api.createBook([{ path: "unicode.md", source: "中🚀" }]);
  assert.equal(session.sourceLength, 7);
  session.dispose();
});

test("image views preserve byte offsets and do not send the whole backing buffer", async () => {
  const { api, state } = harness();
  const buffer = Uint8Array.from([9, 1, 2, 3, 8]).buffer;
  const session = await api.createBook(FILES, {
    images: [{ destination: " guide/figure.png ", bytes: new DataView(buffer, 1, 3) }],
  });
  assert.deepEqual(
    state.calls.find((call) => call[0] === "image"),
    ["image", "guide/figure.png", [1, 2, 3]],
  );
  assert.equal(session.setImage("other.png", new Uint8Array(buffer, 2, 1)), session);
  assert.deepEqual(state.calls.at(-1), ["image", "other.png", [2]]);
  session.dispose();
});

test("invalid font weights and empty images fail before allocating a book", async () => {
  const { api, state } = harness();
  for (const weight of [0, 1001, 1.5, Infinity, "700"]) {
    await assert.rejects(
      api.createBook(FILES, {
        fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([1]), weight }],
      }),
    );
  }
  await assert.rejects(
    api.createBook(FILES, { images: [{ destination: "image.png", bytes: new Uint8Array() }] }),
  );
  assert.equal(state.constructions, 0);
});

test("asset byte budgets are enforced before entering the engine", async () => {
  const { api, state } = harness();
  const tooLarge = new Uint8Array(32 * 1024 * 1024 + 1);
  await assert.rejects(
    api.createBook(FILES, { images: [{ destination: "huge.png", bytes: tooLarge }] }),
    /32 MiB/,
  );
  const shared = new Uint8Array(32 * 1024 * 1024);
  await assert.rejects(
    api.createBook(FILES, {
      images: Array.from({ length: 5 }, (_, i) => ({ destination: `${i}.png`, bytes: shared })),
    }),
    /128 MiB/,
  );
  assert.equal(state.loads, 0);
});

test("a partially configured book is freed when font validation fails", async () => {
  const failure = new Error("malformed font");
  const { api, state } = harness({
    setFont() {
      throw failure;
    },
  });
  await assert.rejects(
    api.createBook(FILES, {
      fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([1, 2]) }],
    }),
    (error) => error === failure,
  );
  assert.equal(state.constructions, 1);
  assert.equal(state.frees, 1);
});

test("one-shot helpers dispose after success and render failure", async () => {
  const success = harness();
  const output = await success.api.renderBookEpub(FILES);
  assert.equal(output.extension, "epub");
  assert.equal(success.state.frees, 1);
  const failure = new Error("render failed");
  const broken = harness({
    renderPdf() {
      throw failure;
    },
  });
  await assert.rejects(broken.api.renderBookPdf(FILES), (error) => error === failure);
  assert.equal(broken.state.frees, 1);
});

test("disposed sessions refuse all operations and getters", async () => {
  const { api } = harness();
  const session = await api.createBook(FILES);
  session.dispose();
  for (const operation of [
    () => session.chapterCount,
    () => session.sourceLength,
    () => session.renderPdf(),
    () => session.renderEpub(),
    () => session.renderSite(),
    () => session.setImage("x", new Uint8Array([1])),
    () => session.setFont("body-regular", new Uint8Array([1])),
  ])
    assert.throws(operation, /disposed/);
});

test("binary output has the right Blob MIME type and filename", async () => {
  const { api } = harness();
  const result = await api.renderBookSite(FILES);
  assert.equal(result.filename("manual"), "manual.zip");
  assert.equal(result.filename(), "book.zip");
  assert.equal(result.blob().type, "application/zip");
  assert.deepEqual([...new Uint8Array(await result.blob().arrayBuffer())], [...result.bytes]);
  assert.equal(Object.isFrozen(result), true);
});

test("outdated WASM builds report the missing class explicitly", async () => {
  const api = createBookBindings(async () => undefined);
  await assert.rejects(api.createBook(FILES), /lacks FmdBook/);
});

test("non-finite and f32-inexpressible scales fail before allocation", async () => {
  const { api, state } = harness();
  for (const fontScale of [NaN, Infinity, -Infinity, 0, -1, 1e100, 1e-320, "large"]) {
    await assert.rejects(api.createBook(FILES, { fontScale }), /fontScale/);
  }
  assert.equal(state.loads, 0);
  const session = await api.createBook(FILES, { customCss: "", fontScale: 1.125 });
  assert.deepEqual(
    state.calls.find((call) => call[0] === "css"),
    ["css", ""],
  );
  assert.deepEqual(
    state.calls.find((call) => call[0] === "scale"),
    ["scale", 1.125],
  );
  session.dispose();
});

test("one-shot output validation failures still dispose the raw book", async () => {
  const { api, state } = harness({
    renderSite() {
      return "not binary";
    },
  });
  await assert.rejects(api.renderBookSite(FILES), /Uint8Array/);
  assert.equal(state.frees, 1);
});
