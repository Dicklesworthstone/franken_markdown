// Compile-only contract: tsc --noEmit --strict --target ES2022 --module esnext
// --lib ES2022,DOM wasm/svg_types_test.ts
import { renderSvg, createRenderer, type FmdRenderOptions, type FmdRenderOutput,
  type FmdSvgRenderOutput, type FmdSvgRenderOptions } from "./franken_markdown.js";

async function useSvgContract() {
  const options: FmdSvgRenderOptions = { maxWidthPt: 360,
    pdfImages: [{ destination: "plot.svg", bytes: new DataView(new ArrayBuffer(2)) }],
    fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([1]), weight: 500 }] };
  const output: FmdSvgRenderOutput = await renderSvg("# Image", options);
  const format: "svg" = output.format;
  const extension: "svg" = output.extension;
  const mime: "image/svg+xml" = output.mimeType;
  const generic: FmdRenderOutput = output;
  const code: string | undefined = output.diagnostics[0]?.code;
  const scope: "document" | undefined = output.diagnostics[0]?.scope;
  const renderer = await createRenderer();
  const facade: FmdSvgRenderOutput = await renderer.renderSvg("x", options);
  // Existing callers holding shared options remain structurally assignable.
  const shared: FmdRenderOptions = { font: "serif", maxWidthPt: 612 };
  await renderSvg("x", shared);
  // @ts-expect-error SVG geometry must be numeric.
  await renderSvg("x", { maxWidthPt: "wide" });
  // @ts-expect-error The SVG-specific surface does not advertise PDF-only flags.
  await renderSvg("x", { fitToPages: 1 });
  return { format, extension, mime, generic, code, scope, facade };
}
void useSvgContract;
