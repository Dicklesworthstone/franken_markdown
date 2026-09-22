// Real generated-artifact test. This is intentionally separate from transport
// tests and must never be replaced by a source-shape or mock-only gate.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [packageArg, wasmArg] = process.argv.slice(2);
if (!packageArg || !wasmArg)
  throw new Error("usage: node wasm/flow_smoke.mjs <assembled-package> <generated.wasm>");
const { init, createFlowSession } = await import(
  pathToFileURL(join(resolve(packageArg), "flow.js"))
);
await init(await readFile(resolve(wasmArg)));
const check = (id, fn) => {
  fn();
  console.error(`flow-wasm: ${id} PASS`);
};
const source =
  "# Editor\n\nAlpha **bold** [link](#editor).\n\n| A | B |\n|---|---|\n| left | right |\n\n![asset](image.png)\n";
const session = await createFlowSession(source, { viewportWidth: 360 });
const items = (state) => [...state.pages({ limit: 2, glyphs: true })].flatMap((page) => page.items);
try {
  const shown = session.snapshot({ limit: 2048, glyphs: true });
  const originalItems = items(session);
  check("generated-class-and-glyphs", () => {
    assert.equal(shown.shapingProfile, "bundled-simple-ltr");
    assert.equal(typeof shown.revision, "string");
    assert(
      originalItems.some(
        (item) => item.kind === "text" && item.fontRun.glyphs.some((glyph) => glyph.glyphId > 0),
      ),
    );
    assert.deepEqual(originalItems, shown.items);
  });
  const bold = originalItems.find((item) => item.kind === "text" && item.text === "bold");
  check("selection-domains", () => {
    assert(bold);
    const selected = session.selectText(bold.index, 0, bold.text.length, shown);
    assert.equal(selected.text, "bold");
    assert(selected.rectangles.length > 0);
    assert.equal(selected.coordinateSpace, "fragment-utf16");
    assert(
      session
        .copySource(
          selected.enclosingSourceSpan.startByte,
          selected.enclosingSourceSpan.endByte,
          shown.revision,
        )
        .includes("**bold**"),
    );
  });
  const link = originalItems.find((item) => item.kind === "anchor" && !item.isHeading);
  check("wrapped-link-hit", () => {
    assert.equal(
      session.hitTest(link.bounds.x + 1, link.bounds.y + 1, shown).linkTarget,
      "#editor",
    );
  });
  const text = originalItems.find((item) => item.kind === "text");
  check("immutable-font-bytes", () => {
    const data = session.fontBytes(text.fontRun.fontId);
    assert(data instanceof Uint8Array && data.length > 100);
    const first = data[0];
    data[0] ^= 255;
    assert.equal(session.fontBytes(text.fontRun.fontId)[0], first);
  });
  check("accessible-table-cells", () => {
    const reading = session.readingOrder({ limit: 2048 });
    assert(
      reading.nodes.some(
        (node) =>
          node.role === "table-header-row" &&
          node.children.some((child) => child.role === "table-header-cell"),
      ),
    );
  });
  const start = source.indexOf("Alpha");
  session.edit(start, start + 5, "café", { expectedRevision: shown.revision });
  check("utf16-edit-and-stale-source", () => {
    assert(session.source.includes("café **bold**"));
    assert.throws(
      () => session.edit(0, 0, "bad", { expectedRevision: shown.revision }),
      (error) => error.code === "STALE_REVISION",
    );
    assert.throws(
      () => session.hitTest(1, 1, shown),
      (error) => error.code === "STALE_REVISION",
    );
  });
  const beforeResize = session.token;
  session.reflow({ viewportWidth: 240 }, beforeResize);
  check("layout-fence-keeps-document-generation", () => {
    assert.equal(session.revision, beforeResize.revision);
    assert.notEqual(session.layoutRevision, beforeResize.layoutRevision);
    assert.throws(
      () => session.hitTest(1, 1, beforeResize),
      (error) => error.code === "STALE_LAYOUT",
    );
  });
  const beforeFailure = session.token;
  const goodSource = session.source;
  check("failed-shaping-is-atomic", () => {
    assert.throws(
      () => session.replaceSource("😀", { expectedRevision: session.revision }),
      (error) => error.code === "LAYOUT_ERROR",
    );
    assert.equal(session.source, goodSource);
    assert.deepEqual(session.token, beforeFailure);
  });
  const request = session.pendingAssets({ limit: 2048 }).requests[0];
  session.provideAsset({
    requestId: request.id,
    generation: request.generation,
    width: 640,
    height: 480,
  });
  check("asset-resolution-and-fitting", () => {
    assert.equal(session.pendingAssets().total, 0);
    assert.equal(session.assetBytes(request.id, session.revision), null);
    const image = items(session).find((item) => item.kind === "image");
    assert.equal(image.isResolved, true);
    assert.equal(image.bounds.width, 240);
    assert.equal(image.bounds.height, 180);
  });
  const fresh = await createFlowSession(session.source, { viewportWidth: 240 });
  try {
    const freshRequest = fresh.pendingAssets().requests[0];
    fresh.provideAsset({
      requestId: freshRequest.id,
      generation: freshRequest.generation,
      width: 640,
      height: 480,
    });
    check("edited-session-equals-fresh-layout", () =>
      assert.deepEqual(items(session), items(fresh)),
    );
  } finally {
    fresh.dispose();
  }
  session.reloadAssets(session.revision);
  check("resource-generation-rejects-late-completion", () => {
    assert.equal(session.pendingAssets().total, 1);
    assert.throws(
      () =>
        session.provideAsset({
          requestId: request.id,
          generation: request.generation,
          width: 1,
          height: 1,
        }),
      (error) => error.code === "STALE_ASSET_GENERATION",
    );
  });
} finally {
  session.dispose();
}
check("dispose", () => {
  session.dispose();
  assert(session.disposed);
  assert.throws(
    () => session.snapshot(),
    (error) => error.code === "SESSION_DISPOSED",
  );
});
console.error("flow-wasm: generated persistent editor session checks passed");
