import { type BookLinkReport, checkBookLinks, createBook, parseBookLinkReport } from "../book.js";
import { type BookWorkerFormat, createBookWorker } from "../book-worker.js";

const files = [{ path: "a.md", source: "# A" }] as const;
const options = { includeSources: [{ path: "part.md", source: "shared" }] };
async function exercise(format: BookWorkerFormat) {
  const output = await checkBookLinks(files, options);
  const name: string = output.filename();
  const report: BookLinkReport = parseBookLinkReport(output.bytes, ["a.md"]);
  const source = report.chapters[0].findings[0]?.source;
  if (source?.status === "mapped") {
    const precision: "destination" | "block" = source.precision;
    const path: string = source.ranges[0].path;
    const start: number = source.ranges[0].start;
    // @ts-expect-error native provenance is immutable
    source.ranges[0].start = 0;
    // @ts-expect-error an enclosing-block location is not edit authority
    source.replacement;
    void [precision, path, start];
  } else if (source) {
    const reason: string = source.reason;
    // @ts-expect-error unavailable provenance cannot expose partial ranges
    source.ranges;
    void reason;
  }
  const book = await createBook(files, options);
  try {
    const retained = book.validateLinks();
    retained.blob();
  } finally {
    book.dispose();
  }
  const worker = createBookWorker();
  try {
    const result = await worker.render(files, "links", options, {
      signal: new AbortController().signal,
    });
    const kind: "book-links" = result.format;
    const anyResult = await worker.render(files, format, options);
    anyResult.blob();
    // @ts-expect-error worker results do not have facade filename helpers
    result.filename();
    // @ts-expect-error validated reports are immutable
    report.chapters[0].findings.push({ code: "missing_anchor", destination: "#x", message: "bad" });
    return [name, kind];
  } finally {
    worker.dispose();
  }
}
void exercise;
