import { createWorkerRenderer, type DocumentPublication, type DocumentPublicationTarget } from "./document-worker.js";

const renderer = createWorkerRenderer({ maxOutputBytes: 8 * 1024 * 1024 });
const publication = await renderer.publish("# Guide", [
  { format: "html", options: { customCss: "p { color: navy }", toc: true } },
  { format: "pdf", options: { page: { size: "a4" }, running: { footer: { right: "{page}/{pages}" } } } },
], { signal: new AbortController().signal, timeoutMs: 1000 });
const narrowed: DocumentPublication<"html" | "pdf"> = publication;
const all: DocumentPublication = narrowed;
for (const output of publication.outputs) {
  const bytes: Uint8Array = output.bytes;
  if (output.format === "pdf") {
    const extension: "pdf" = output.extension;
    void extension;
  } else {
    const format: "html" = output.format;
    const extension: "html" = output.extension;
    void [format, extension];
  }
  void bytes;
}
const epub = await renderer.publish("book", [{ format: "epub", options: { tocDepth: 2 } }]);
const extension: "epub" = epub.outputs[0].extension;
const readonlyTargets = [{ format: "svg", options: { maxWidthPt: 720 } }] as const;
await renderer.publish("poster", readonlyTargets);
const target: DocumentPublicationTarget = { format: "pdf", options: { author: "Writer" } };
void [all, extension, target];
// @ts-expect-error PDF has no CSS option.
renderer.publish("x", [{ format: "pdf", options: { customCss: "p {}" } }]);
// @ts-expect-error A mixed target list must still preserve per-format option types.
renderer.publish("x", [{ format: "pdf" }, { format: "html", options: { page: {} } }]);
// @ts-expect-error Interactive HTML does not accept host image resources.
renderer.publish("x", [{ format: "interactive-html", options: { pdfImages: [] } }]);
// @ts-expect-error Unknown formats are not rendered.
renderer.publish("x", [{ format: "zip" }]);
// @ts-expect-error A publication needs explicit targets.
renderer.publish("x");
// @ts-expect-error Output order cannot be mutated.
publication.outputs.reverse();
// @ts-expect-error Publication metadata is immutable.
publication.totalBytes = 0;
renderer.dispose();
