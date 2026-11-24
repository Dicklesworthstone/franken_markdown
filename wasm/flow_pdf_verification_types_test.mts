import {
  createFlowSession,
  type FlowPdfExportOptions,
  type FlowPdfVerificationOptions,
  type FlowPdfVerificationResult,
  type FlowSession,
} from "./flow.js";
import {
  createWorkerFlowSession,
  type FlowPdfVerificationOptions as WorkerPdfVerificationOptions,
  type FlowPdfVerificationResult as WorkerPdfVerificationResult,
  type WorkerFlowSession,
} from "./flow-worker.js";
import type { FmdPdfVerificationReport } from "./franken_markdown.js";

const settings: FlowPdfVerificationOptions = {
  title: "Configured publication",
  page: { size: "a4", margins: { leftPt: 64, rightPt: 64 } },
  running: { footer: { right: "{page} / {pages}" } },
  typography: { themeSpacing: true, lineHeight: 1.4 },
  footnoteBacklinks: true,
  strictPagination: true,
  pdfImages: [{ destination: "logo.png", bytes: new Uint8Array([1, 2, 3]) }],
  maxOutputBytes: 4 * 1024 * 1024,
};

async function useDirect(session: FlowSession) {
  const capability: boolean = session.supportsPdfVerification;
  const result: FlowPdfVerificationResult = await session.verifyPdf(settings, session.token);
  const report: FmdPdfVerificationReport = result.report;
  const format: "pdf-verification" = result.format;
  const sourceBytes: number = result.sourceLengthBytes;
  const revision: string = result.revision;
  const print: FlowPdfExportOptions = settings;
  await session.exportDocument("pdf", print, session.token);
  await session.verifyPdf(undefined, { revision: 9007199254740993n, layoutRevision: "2" });
  return { capability, report, format, sourceBytes, revision };
}

async function useWorker(session: WorkerFlowSession, signal: AbortSignal) {
  const options: WorkerPdfVerificationOptions = settings;
  const capability: boolean = session.supportsPdfVerification;
  const result: WorkerPdfVerificationResult = await session.verifyPdf(options, session.token, {
    signal, timeoutMs: 15_000,
  });
  const directResult: FlowPdfVerificationResult = result;
  return { capability, directResult };
}

// Type-only negative examples; these functions are never executed.
function invalidOptions(session: FlowSession, worker: WorkerFlowSession) {
  // @ts-expect-error Verification emits no PDF/A artifact.
  session.verifyPdf({ pdfA: "2b" }, session.token);
  // @ts-expect-error Archival strictness is not a verification option.
  session.verifyPdf({ pdfAStrict: false }, session.token);
  // @ts-expect-error The session authoring profile is immutable.
  session.verifyPdf({ profile: "commonmark-gfm" }, session.token);
  // @ts-expect-error Session-owned source must be fenced with a complete token.
  session.verifyPdf({});
  // @ts-expect-error Numeric revisions cannot identify lossless u64 tokens.
  worker.verifyPdf({}, { revision: 1, layoutRevision: "1" });
  // @ts-expect-error Raw HTML policy is fixed for persistent Flow publications.
  worker.verifyPdf({ allowRawHtml: true }, worker.token);
}

void createFlowSession;
void createWorkerFlowSession;
void useDirect;
void useWorker;
void invalidOptions;
