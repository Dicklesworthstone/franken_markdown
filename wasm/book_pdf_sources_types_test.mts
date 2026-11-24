// Root one-shot book source options; declarations only, no renderer execution.
import {
  createRenderer, renderBookPdf, renderBookSite,
  type FmdBookFile, type FmdBookPdfOptions, type FmdBookSourceOptions,
} from "./franken_markdown.js";

const files: readonly FmdBookFile[] = [{ path: "guide.md", source: "# Guide" }];
const shared: FmdBookSourceOptions = {
  includeSources: [{ path: "parts/code.rs", source: "fn example() {}" }],
  expandIncludes: true,
};
const options: FmdBookPdfOptions = {
  ...shared, profile: "commonmark-gfm", page: { size: "a4" },
  baselineGridPt: 14, typography: { lineHeight: 2 },
  running: { footer: { center: "{page} / {pages}" } },
};
await renderBookPdf(files, options);
await renderBookSite(files, shared);
const renderer = await createRenderer();
await renderer.renderBookPdf(files, options);
await renderer.renderBookPdf(files, { expandIncludes: false });
// @ts-expect-error Expansion must be explicitly boolean, not a string.
await renderBookPdf(files, { expandIncludes: "false" });
// @ts-expect-error Include resources must supply source text.
await renderBookPdf(files, { includeSources: [{ path: "parts.md" }] });
// @ts-expect-error Source lists are arrays, not keyed objects.
await renderBookPdf(files, { includeSources: { "parts.md": "Text" } });
// @ts-expect-error Root book PDF/A remains in the separate ./book entry point.
await renderBookPdf(files, { ...shared, pdfA: "2b" });
