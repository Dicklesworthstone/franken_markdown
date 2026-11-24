import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import { FLOW_SOURCE_LIMIT } from "../flow_session.mjs";
import { normalizeRenderSettings } from "../demo/flow_render_settings.mjs";
import { captureFlowProject, serializeFlowProject, parseFlowProject,
  readFlowProjectFile, flowProjectDownload, FLOW_PROJECT_LIMIT } from "../demo/flow_project.mjs";

const document = Object.freeze({ filename: "café.md", source: "\ufeff# Café😀\r\n\nTerm\r: definition\n" });
const settings = Object.freeze({ profile: "commonmark-gfm", font: "serif",
  bodySize: 18, codeSize: 15, lineHeight: 27, title: "Book <literal>", author: "Émilie", lang: "fr-CA",
  toc: true, tocDepth: 3, pageSize: "a4", pageOrientation: "landscape",
  marginTopPt: 36, marginRightPt: 42, marginBottomPt: 38, marginLeftPt: 44,
  baselineGridPt: 14, optimalPagination: true, strictPagination: true, splitFootnotes: true,
  footnoteBacklinks: true, pdfLineHeight: 1.4, tableCellPaddingXEm: 0, tableCellPaddingYEm: 0.5,
  typographyThemeSpacing: true, microtype: "expansion", darkMode: "disabled" });

test("versioned Flow projects round-trip exact source and the complete applied export profile", () => {
  const text = serializeFlowProject(document, settings), restored = parseFlowProject(text);
  assert.equal(restored.schema, "franken-markdown-flow-project");
  assert.equal(restored.version, 1);
  assert.deepEqual(restored.document, document);
  assert.deepEqual(restored.settings, normalizeRenderSettings(settings).values);
  const configured = normalizeRenderSettings(restored.settings);
  assert.equal(configured.preview.profile, "commonmark-gfm");
  assert.equal(configured.pdf.splitFootnotes, true);
  assert.equal(configured.pdf.typography.tableCellPaddingXEm, 0);
  assert.equal(restored.settings.pageSize, "a4");
  assert.deepEqual(configured.pdf, normalizeRenderSettings(settings).pdf);
  assert.equal(configured.epub.author, "Émilie");
  assert(Object.isFrozen(restored) && Object.isFrozen(restored.document) && Object.isFrozen(restored.settings));
  assert.equal(serializeFlowProject(restored.document, restored.settings), text);
});

test("project file and Blob keep BOM, CRLF, Unicode, nulls and explicit zero values", async () => {
  const output = flowProjectDownload(document, settings);
  assert.equal(output.filename, "café.fmdflow.json");
  const restored = await readFlowProjectFile(new File([output.blob], output.filename));
  assert.equal(restored.document.source, document.source);
  assert.equal(restored.settings.tableCellPaddingXEm, 0);
  assert.equal(restored.settings.fitToPages, null);
  assert.deepEqual(parseFlowProject("\ufeff" + await output.blob.text()), restored);
});

for (const bad of [
  null, [], {}, { schema: "elsewhere", version: 1, document, settings },
  { schema: "franken-markdown-flow-project", version: 2, document, settings },
  { schema: "franken-markdown-flow-project", version: "1", document, settings },
  { schema: "franken-markdown-flow-project", version: 1, document, settings, images: [] },
]) test("unsupported or incomplete project shape is rejected: " + JSON.stringify(bad).slice(0,90), () => {
  assert.throws(() => parseFlowProject(JSON.stringify(bad)), { code: "INVALID_PROJECT" });
});

for (const bad of [{ images: [] }, { workerUrl: "https://example.invalid" }, { page: {} },
  { profile: "typo" }, { pageSize: "custom" }, { baselineGridPt: 14, fitToPages: 2 },
  { pdfLineHeight: 1.4, typographyThemeSpacing: false }, { author: "\ud800" }]) {
  test("projects refuse invalid or authority-bearing settings: " + JSON.stringify(bad), () => {
    assert.throws(() => captureFlowProject(document, bad));
  });
}

test("project capture reads no getters, inherited settings or unknown document fields", () => {
  let reads = 0;
  for (const value of [Object.create({ title: "inherited" }), { get title() { reads++; return "x"; } },
    { [Symbol("extra")]: true }, { title: undefined }])
    assert.throws(() => captureFlowProject(document, value), { code: "INVALID_PROJECT" });
  assert.throws(() => captureFlowProject({ ...document, images: [] }, {}), { code: "INVALID_PROJECT" });
  assert.throws(() => captureFlowProject({ filename: "x.md", get source() { reads++; return "text"; } }, {}),
    { code: "INVALID_PROJECT" });
  assert.equal(reads, 0);
});

test("escape-heavy legal source fits the project budget and source budget remains independent", () => {
  const source = "\0".repeat(FLOW_SOURCE_LIMIT);
  const text = serializeFlowProject({ filename: "large.md", source }, {});
  assert(text.length > FLOW_SOURCE_LIMIT && text.length < FLOW_PROJECT_LIMIT);
  assert.equal(parseFlowProject(text).document.source, source);
  assert.throws(() => serializeFlowProject({ filename: "large.md", source: source + "x" }, {}),
    { code: "BUDGET_EXCEEDED" });
  assert.throws(() => parseFlowProject(" ".repeat(FLOW_PROJECT_LIMIT + 1)), { code: "BUDGET_EXCEEDED" });
});

test("malformed Unicode, malformed JSON and unsafe filenames fail before reopening", () => {
  for (const text of ["{", "", "null", "[1]", '{"schema":"x"}']) assert.throws(() => parseFlowProject(text));
  assert.throws(() => parseFlowProject("\ud800"), { code: "INVALID_UNICODE" });
  for (const filename of ["../x.md", "", "a/b.md", "x\0.md"])
    assert.throws(() => captureFlowProject({ filename, source: "x" }, {}), { code: "INVALID_FILENAME" });
});

test("file admission refuses oversize and mismatched reads without accepting replacement decoding", async () => {
  let reads = 0;
  const fake = (size, read) => ({ [Symbol.toStringTag]: "File", size, arrayBuffer: read });
  await assert.rejects(readFlowProjectFile(fake(FLOW_PROJECT_LIMIT + 1, () => { reads++; })), { code: "INVALID_FILE" });
  assert.equal(reads, 0);
  await assert.rejects(readFlowProjectFile(fake(12, async () => new ArrayBuffer(11))), { code: "INVALID_FILE" });
  await assert.rejects(readFlowProjectFile(new File([Uint8Array.of(255)], "bad.fmdflow.json")), { code: "INVALID_UNICODE" });
  await assert.rejects(readFlowProjectFile(fake(1, async () => { throw new Error("disk"); })), { code: "FILE_READ_FAILED" });
});

test("download names stay portable at the UTF-8 boundary without cutting a Unicode scalar", async () => {
  const output = flowProjectDownload({ filename: "😀".repeat(58) + ".md", source: "text" }, {});
  assert(new TextEncoder().encode(output.filename).length <= 240);
  assert(output.filename.endsWith(".fmdflow.json"));
  assert(!output.filename.includes("\ufffd"));
  assert.equal(parseFlowProject(await output.blob.text()).document.filename, "😀".repeat(58) + ".md");
});
