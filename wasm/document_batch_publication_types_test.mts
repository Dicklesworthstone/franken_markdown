import { renderDocumentBatch, type DocumentBatchItem, type DocumentBatchPublicationItem,
  type DocumentBatchJob, type DocumentBatchIterator } from "./document-batch.js";
import type { DocumentPublication } from "./document-worker.js";

const singles: DocumentBatchItem<"html">[] = [{ source: "x", format: "html", options: { customCss: "p {}" } }];
const legacy: DocumentBatchIterator<"html"> = renderDocumentBatch(singles);
for await (const receipt of legacy) {
  if (receipt.status === "fulfilled") {
    const extension: "html" = receipt.value.extension;
    void extension;
  }
}
const editions: DocumentBatchPublicationItem<"pdf" | "epub">[] = [{ source: "x", targets: [
  { format: "pdf", options: { page: { size: "a4" } } }, { format: "epub", options: { toc: true } },
] }];
for await (const receipt of renderDocumentBatch(editions)) {
  if (receipt.status === "fulfilled") {
    const result: DocumentPublication<"pdf" | "epub"> = receipt.value;
    const extension: "pdf" | "epub" = result.outputs[0].extension;
    void extension;
  }
}
const mixed: DocumentBatchJob<"html" | "pdf">[] = [singles[0], { source: "x", targets: [{ format: "pdf" }] }];
for await (const receipt of renderDocumentBatch(mixed)) {
  if (receipt.status === "fulfilled") {
    if (receipt.value.format === "document-publication") {
      const total: number = receipt.value.totalBytes;
      void total;
    } else {
      const extension: "html" | "pdf" = receipt.value.extension;
      void extension;
    }
  }
}
async function* source() { yield editions[0]; }
renderDocumentBatch(source(), { order: "completion", worker: { maxOutputBytes: 1024 } });
renderDocumentBatch([
  { source: "x", format: "html", options: { title: "Hello" } },
  { source: "x", targets: [{ format: "pdf", options: { page: { size: "a4" } } }] },
]);
// @ts-expect-error Publication and single-output job fields cannot be mixed.
renderDocumentBatch([{ source: "x", format: "html", targets: [{ format: "pdf" }] }]);
// @ts-expect-error Publication options are per target, never top-level.
renderDocumentBatch([{ source: "x", targets: [{ format: "html" }], options: {} }]);
// @ts-expect-error PDF cannot receive HTML-only CSS settings, even in a mixed target list.
renderDocumentBatch([{ source: "x", targets: [{ format: "html" }, { format: "pdf", options: { customCss: "bad" } }] }]);
