// Production editor settings and export coordination. Native rendering is an
// explicit boundary double; these checks do not claim generated-WASM output.
import assert from "node:assert/strict";
import test from "node:test";
import { normalizeRenderSettings } from "../demo/flow_render_settings.mjs";
import { withFlowExports } from "../flow_export.mjs";

const author = "  Émilie & Research 😀  ";
const source = "---\nauthor: Frontmatter creator\n---\n# Publication\n";
const token = Object.freeze({ revision: "1", layoutRevision: "1" });

function publication(format = "epub") {
  const calls = [];
  const session = {
    disposed: false, source, token,
    snapshot({ offset }) {
      return { schemaVersion: 1, ...token, offset, total: 0, nextOffset: null, items: [] };
    },
  };
  const render = async (markdown, options) => {
    calls.push({ markdown, options });
    return { format, mimeType: format === "html" ? "text/html; charset=utf-8" : "application/epub+zip", diagnostics: [],
      bytes: new Uint8Array([80, 75, 3, 4]) };
  };
  const unexpected = () => { throw new Error("Unrequested output format"); };
  return { calls, api: withFlowExports(session, { html: unexpected, pdf: unexpected, epub: unexpected, [format]: render }, "sans") };
}

test("the applied Flow editor author reaches EPUB with its exact original source", async () => {
  const input = { author, title: "Guide", lang: "fr" };
  const applied = normalizeRenderSettings(input);
  const f = publication();
  const pending = f.api.exportDocument("epub", applied.epub, token);
  input.author = "Later draft";
  const result = await pending;
  assert.equal(result.format, "epub");
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].options.author, author);
  assert.equal(f.calls[0].options.title, "Guide");
  assert.equal(f.calls[0].options.lang, "fr");
  assert.equal(f.calls[0].markdown, source);
  assert.equal(applied.pdf.author, author);
  assert.equal(Object.hasOwn(applied.svg, "author"), false);
});

test("blank editor author retains source-frontmatter inheritance rather than inventing a creator", async () => {
  for (const settings of [{}, { author: "" }]) {
    const applied = normalizeRenderSettings(settings), f = publication();
    await f.api.exportDocument("epub", applied.epub, token);
    assert.equal(Object.hasOwn(applied.epub, "author"), false);
    assert.equal(Object.hasOwn(f.calls[0].options, "author"), false);
    assert.equal(f.calls[0].markdown, source);
  }
  assert.equal(normalizeRenderSettings({ author: "  " }).epub.author, "  ",
    "authored whitespace is metadata, not an omitted setting");
});

test("EPUB author settings reject invalid text before any renderer is reached", () => {
  for (const author of [null, 42, "broken\ud800", "é".repeat(32769)]) {
    assert.throws(() => normalizeRenderSettings({ author }));
  }
});

test("the applied Flow editor author reaches HTML with its exact source and whitespace", async () => {
  for (const configured of [author, "  "]) {
    const input = { author: configured, title: "Guide", lang: "fr" };
    const applied = normalizeRenderSettings(input), f = publication("html");
    const pending = f.api.exportDocument("html", applied.html, token);
    input.author = "Later draft";
    const result = await pending;
    assert.equal(result.format, "html");
    assert.equal(f.calls.length, 1);
    assert.equal(f.calls[0].options.author, configured);
    assert.equal(f.calls[0].markdown, source);
    assert.equal(applied.pdf.author, configured);
    assert.equal(applied.epub.author, configured);
    assert.equal(Object.hasOwn(applied.svg, "author"), false);
  }
});

test("blank Flow editor author omits the HTML option so source frontmatter can supply it", async () => {
  for (const input of [{}, { author: "" }]) {
    const applied = normalizeRenderSettings(input), f = publication("html");
    await f.api.exportDocument("html", applied.html, token);
    assert.equal(Object.hasOwn(applied.html, "author"), false);
    assert.equal(Object.hasOwn(f.calls[0].options, "author"), false);
    assert.equal(f.calls[0].markdown, source);
  }
});
