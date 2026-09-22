// Real compiled-WASM integration. No generated-binding or renderer doubles.
// After building the matching package on a configured DSR host:
//   node wasm/book_source_updates_smoke.mjs
// Uses the same session factory as /book, with the actual generated FmdBook.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createBookBindings } from "./book_session.mjs";

async function main() {
  const engine = await import("./pkg/franken_markdown.js");
  await engine.default({ module_or_path: await readFile(new URL("./pkg/franken_markdown_bg.wasm", import.meta.url)) });
  assert.equal(typeof engine.FmdBook?.prototype?.updateSources, "function", "matching WASM rebuild required");
  const { createBook } = createBookBindings(async () => engine.FmdBook);
  const file = (path, source) => ({ path, source });
  const chapters = [
    file("guide/start.md", "# Start\n\n{{#include ../parts/shared.md}}\n\n![Chart](chart.svg)\n\n[End](../end.md#end)\n"),
    file("end.md", "# End\n\n{{#include parts/shared.md}}\n"),
    file("literal.md", "# Literal\n\nUnchanged.\n"),
  ];
  const resources = [file("parts/shared.md", "Shared before.\n"), file("parts/spare.md", "Unused")];
  const options = {
    includeSources: resources, title: "Retained manual", author: "Author", font: "serif",
    customCss: ".fmd{line-height:1.7}", toc: true, pageNumbers: true,
    page: { size: { widthPt: 720, heightPt: 540 }, margins: 36 },
    images: [{ destination: "guide/chart.svg", bytes: new TextEncoder().encode(
      '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16"/></svg>',
    ) }],
  };
  const bytes = s => new TextEncoder().encode(s).length;
  const sourceLength = () => [...chapters, ...resources].reduce((sum, f) => sum + bytes(f.source), 0);
  const book = await createBook(chapters, options);
  try {
    assert.equal(book.sourceRevision, 0);
    assert.equal(book.sourceLength, sourceLength());
    const before = book.renderSite().bytes;
    resources[0].source = "Shared after, with **style**.\n";
    const report = book.updateSources([resources[0]], { expectedRevision: 0 });
    assert.deepEqual(report, { revision: 1, sourceLength: sourceLength(), chapterCount: 3,
      changedSources: 1, reparsedChapters: [0, 1] });
    assert.notDeepEqual(book.renderSite().bytes, before);
    const fresh = await createBook(chapters, options);
    try {
      for (const method of ["renderPdf", "renderEpub", "renderSite"]) {
        const actual = book[method](), expected = fresh[method]();
        assert.deepEqual(actual.bytes, expected.bytes, `${method}: incremental/full rebuild differ`);
        assert.deepEqual(actual.bytes, book[method]().bytes, `${method}: non-deterministic`);
        assert.equal(actual.sourceLength, sourceLength());
      }
    } finally { fresh.dispose(); }
    const stable = book.renderSite().bytes;
    assert.throws(() => book.updateSources([file("parts/shared.md", "{{#include missing.md}}")]), /missing/);
    assert.equal(book.sourceRevision, 1);
    assert.deepEqual(book.renderSite().bytes, stable);
    assert.throws(() => book.updateSources([file("end.md", "Stale")], { expectedRevision: 0 }), { code: "STALE_BOOK_REVISION" });
    assert.equal(book.updateSources([resources[0]]).changedSources, 0);
    assert.equal(book.sourceRevision, 1);
    resources[1].source = "Unused latest";
    const unused = book.updateSources([resources[1]], { expectedRevision: 1 });
    assert.equal(unused.revision, 2);
    assert.deepEqual(unused.reparsedChapters, []);
    assert.equal(book.sourceLength, sourceLength());
    assert.deepEqual(book.renderSite().bytes, stable);
  } finally { book.dispose(); }

  for (const expandIncludes of [true, false]) {
    const initial = [file("a.md", "# A"), file("b.md", "# B")];
    const session = await createBook(initial, { expandIncludes });
    initial[0].source = "# A\n\n{{#include b.md}}\n";
    try {
      const update = session.updateSources([initial[0]]);
      assert.deepEqual(update.reparsedChapters, [0]);
      const rebuilt = await createBook(initial, { expandIncludes });
      try { assert.deepEqual(session.renderSite().bytes, rebuilt.renderSite().bytes); }
      finally { rebuilt.dispose(); }
    } finally { session.dispose(); }
  }

  const raw = new engine.FmdBook(["a.md"], ["# A"]);
  try {
    for (const revision of [-1, 0.5, NaN, Infinity, 2 ** 32]) {
      let rejected = false;
      try { raw.updateSources(["a.md"], ["New"], revision); } catch { rejected = true; }
      assert.equal(rejected, true, `raw ABI accepted invalid revision ${revision}`);
      assert.equal(raw.sourceRevision, 0);
    }
  } finally { raw.free(); }
  process.stderr.write("PASS real WASM: incremental/full PDF-EPUB-site parity, shared includes, rollback, revisions, assets and determinism\n");
}

try { await main(); }
catch (error) {
  process.stderr.write(`FAIL real-WASM source updates: ${String(error?.message ?? error)}\n`);
  process.exitCode = 1;
}
