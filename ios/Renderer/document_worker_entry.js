// This entry is loaded only inside the owned module worker. There is no
// main-thread fallback and no import specifier received from a worker message.
import { installDocumentWorker } from "./document_worker.mjs";

installDocumentWorker(globalThis, async () => {
  const { createRenderer } = await import("./franken_markdown.js");
  return createRenderer();
});
