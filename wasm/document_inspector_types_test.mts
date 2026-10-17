import { createDocumentInspector, type DocumentInspection, type DocumentInspector } from "./document-inspector.js";
const inspector: DocumentInspector = createDocumentInspector({ maxPendingOperations: 1, maxOutputBytes: 1024 });
const output: DocumentInspection = await inspector.inspect("# Hello", { signal: new AbortController().signal, timeoutMs: 1000 });
const schema: "fmd-document-inspection-v1" = output.schema;
const mime: "application/json" = output.mimeType;
const words: number = output.stats.words;
const detail: string | undefined = output.accessibility.findings[0]?.detail;
const bytes: Uint8Array = output.bytes;
const file: string = output.filename("guide");
void [schema, mime, words, detail, bytes, file];
// @ts-expect-error Reports are immutable, including native nested arrays.
output.stats.outline.push({ level: 1, text: "changed", slug: "changed" });
// @ts-expect-error Native findings are deeply readonly.
output.accessibility.findings[0].code = "changed";
// @ts-expect-error Inspection does not render or grant raw-HTML authority.
inspector.inspect("# Hello", { allowRawHtml: true });
// @ts-expect-error Sources must be strings, not coercible objects.
inspector.inspect({ toString: () => "source" });
// @ts-expect-error Worker budgets are numbers.
createDocumentInspector({ maxOutputBytes: "1024" });
inspector.dispose();
