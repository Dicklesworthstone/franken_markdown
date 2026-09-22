// Compile-only public declarations; this file does not execute a WASM engine.
import { renderBookSite, createRenderer, type FmdBookFile, type FmdBookSiteOptions,
  type FmdBookSiteOutput } from "./franken_markdown.js";

const files: readonly FmdBookFile[] = [{ path: "guide/start.md", source: "# Start" }];
const options: FmdBookSiteOptions = {
  title: "Manual", font: "serif", customCss: "", lang: "de", toc: true, tocDepth: 2,
  fontScale: "lg", expandIncludes: true, allowRawHtml: false,
  includeSources: [{ path: "shared.md", source: "Included" }],
  pdfImages: [{ destination: "guide/chart.svg", bytes: new Uint8Array([1]) }],
  fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([2]), weight: 550 }],
};
const output: FmdBookSiteOutput = await renderBookSite(files, options);
const zip: "zip" = output.extension;
const mime: "application/zip" = output.mimeType;
const format: "book-site" = output.format;
output.blob(); output.filename("manual");
await (await createRenderer()).renderBookSite(files, options);
// @ts-expect-error Site publication does not accept PDF paper geometry.
await renderBookSite(files, { page: { size: "a4" } });
// @ts-expect-error Site parsing deliberately does not admit raw-HTML trust.
await renderBookSite(files, { allowRawHtml: true });
// @ts-expect-error PDF-only page numbers must not be silently ignored.
await (await createRenderer()).renderBookSite(files, { pageNumbers: true });
// @ts-expect-error Include sources must contain UTF-8 source strings.
await renderBookSite(files, { includeSources: [{ path: "x.md", source: new Uint8Array() }] });
// @ts-expect-error Expansion policy must not be coerced from strings.
await renderBookSite(files, { expandIncludes: "false" });
void [zip, mime, format];
