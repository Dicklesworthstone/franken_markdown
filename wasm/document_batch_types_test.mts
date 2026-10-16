import {
  renderDocumentBatch, DocumentBatchError, FlowWorkerError,
  type DocumentBatchItem, type DocumentBatchIterator, type DocumentBatchReceipt,
} from "@franken-suite/franken-markdown/document-batch";
import type { DocumentOutput } from "./document-worker.js";

const pdf = renderDocumentBatch([{ format: "pdf", source: "# PDF", options: {
  page: { size: "a4", orientation: "landscape" },
  running: { footer: { right: "{page}/{pages}" } },
} }], { concurrency: 2, order: "completion", worker: { timeoutMs: 5000 } });
const pdfIterator: DocumentBatchIterator<"pdf"> = pdf;
for await (const receipt of pdfIterator) {
  if (receipt.status === "fulfilled") {
    const output: DocumentOutput<"pdf"> = receipt.value;
    const extension: "pdf" = output.extension;
    void extension;
  } else {
    const error: unknown = receipt.reason;
    // @ts-expect-error Rejected receipts do not carry output bytes.
    receipt.value.bytes;
    void error;
  }
}
const mixed: readonly DocumentBatchItem[] = [
  { format: "html", source: "# HTML", options: { customCss: "body { color: black; }" } },
  { format: "svg", source: "# SVG", options: { maxWidthPt: 700 } },
  { format: "epub", source: "# EPUB", options: { title: "Book" } },
  { format: "interactive-html", source: "# Interactive", options: { lang: "en" } },
];
const outputs: AsyncIterable<DocumentBatchReceipt> = renderDocumentBatch(mixed);
for await (const receipt of outputs) {
  if (receipt.status === "fulfilled" && receipt.value.format === "svg") {
    const extension: "svg" = receipt.value.extension;
    void extension;
  }
}
async function* source(): AsyncGenerator<DocumentBatchItem<"html">> {
  yield { format: "html", source: "# Async" };
}
const asynchronous: DocumentBatchIterator<"html"> = renderDocumentBatch(source());
await asynchronous.return("closed");
await asynchronous.throw(null);
const error = new DocumentBatchError("ABORTED", "stopped");
const workerError: FlowWorkerError | undefined = undefined;
void [error.code, workerError];

// @ts-expect-error PDF-only page settings must not be accepted for HTML.
renderDocumentBatch([{ format: "html", source: "x", options: { page: { size: "a4" } } }]);
// @ts-expect-error The source is Markdown text, not encoded bytes.
renderDocumentBatch([{ format: "pdf", source: new Uint8Array() }]);
// @ts-expect-error A batch cannot hide queues within individual workers.
renderDocumentBatch([], { worker: { maxPendingOperations: 8 } });
// @ts-expect-error Only the documented ordering modes are supported.
renderDocumentBatch([], { order: "unordered" });
// @ts-expect-error Semantic comparisons are not document render jobs.
renderDocumentBatch([{ format: "diff-html", source: "x" }]);
