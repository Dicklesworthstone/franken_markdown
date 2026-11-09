import { checkBookSourceDiagnostics, createBook, parseBookSourceDiagnosticsReport,
  type BookSourceDiagnosticsReport } from "../book.js";
import { createBookWorker } from "../book-worker.js";

async function check() {
  const chapters = [{ path: "guide.md", source: "# Guide" }] as const;
  const result = await checkBookSourceDiagnostics(chapters);
  const report: BookSourceDiagnosticsReport = parseBookSourceDiagnosticsReport(result.bytes, ["guide.md"]);
  const source = report.chapters[0]?.diagnostics[0]?.source;
  if (source?.status === "mapped") {
    const path: string = source.ranges[0].path;
    const kind: "exact" | "rewritten" | "generated" = source.ranges[0].kind;
    // @ts-expect-error verified source ranges are immutable
    source.ranges[0].start = 1;
    void path; void kind;
  } else if (source) {
    const reason: string = source.reason;
    // @ts-expect-error unavailable attribution has no partial ranges
    source.ranges;
    void reason;
  }
  const book = await createBook(chapters);
  try { book.sourceDiagnostics().blob(); } finally { book.dispose(); }
  const worker = createBookWorker();
  try {
    const result = await worker.render(chapters, "diagnostics", {}, { signal: new AbortController().signal });
    const format: "book-diagnostics" = result.format;
    // @ts-expect-error worker output has no facade filename helper
    result.filename();
    // @ts-expect-error diagnostic reports are immutable
    report.summary.diagnostics = 0;
    return format;
  } finally { worker.dispose(); }
}
void check;
