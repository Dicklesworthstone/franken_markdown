// Explicit native-session/verifier double. Production export capture, admission,
// report validation and worker orchestration remain unchanged in these tests.
import { withFlowExports } from "../flow_export.mjs";
import { FlowError } from "../flow_session.mjs";

export function createVerificationFixture(configuration = {}, source = "\ufeff# Source é中😀\r\n", selected = {}) {
  const state = { source, revision: "9007199254740993", layoutRevision: "9007199254740997",
    profile: selected.profile, hold: null, report: null, failure: null, inventoryReads: 0 };
  const calls = [], images = configuration.images ?? [
    { requestId: "1", destination: "offscreen.png", bytes: Uint8Array.of(1, 2, 3), isResolved: true },
  ];
  const token = () => ({ revision: state.revision, layoutRevision: state.layoutRevision });
  const session = {
    disposed: false,
    get source() { return state.source; },
    get revision() { return state.revision; },
    get layoutRevision() { return state.layoutRevision; },
    get token() { return token(); },
    get profile() { return state.profile; },
    layoutOptions: { viewportWidth: 640, bodySize: 14, codeSize: 13, lineHeight: 20,
      ...Object.fromEntries(Object.entries(selected).filter(([key]) => !["font", "profile"].includes(key))) },
    snapshot() { throw Error("Visible display items are not a complete image inventory"); },
    assetBytes(id, revision) {
      if (revision !== state.revision) throw new FlowError("STALE_REVISION", "source changed");
      return images.find(image => image.requestId === id)?.bytes ?? null;
    },
    replaceSource(markdown, options) {
      if (options.expectedRevision !== state.revision) throw new FlowError("STALE_REVISION", "source changed");
      state.source = markdown; state.revision = String(BigInt(state.revision) + 1n);
      state.layoutRevision = String(BigInt(state.layoutRevision) + 1n);
      return token();
    },
    dispose() { session.disposed = true; },
  };
  const pause = async () => {
    if (state.hold) await state.hold;
    if (configuration.delayMs) await new Promise(resolve => setTimeout(resolve, configuration.delayMs));
  };
  const render = async (markdown, options) => {
    calls.push({ operation: "pdf", markdown, options }); await pause();
    return { format: "pdf", mimeType: "application/pdf", diagnostics: [],
      bytes: new TextEncoder().encode("%PDF-fixture"), ...(options.profile === undefined ? {} : { profile: options.profile }) };
  };
  const verifyPdf = async (markdown, options) => {
    calls.push({ operation: "verify", markdown, options }); await pause();
    if (state.failure) throw state.failure;
    if (configuration.strictFailure && options.strictPagination)
      throw JSON.stringify({ code: "PAGINATION_INFEASIBLE", reason: "infeasible", message: "exact plan refused" });
    const report = { schema_version: "1", target: "pdf", verdict: "findings", page_count: 1,
      digest: "fnv1a64:0123456789abcdef",
      anchors: { resolved: 1, unresolved: ["missing-anchor"] },
      findings: [{ code: "fixture.finding", detail: "native detail \n \"é中😀\"" }],
      pages: [{ number: 1, runs: [{ text: markdown, x: 72, y: 700, size: 11, kind: "body" }] }],
      ...(options.profile === undefined ? {} : { profile: options.profile }),
      // Additive native report fields must survive transport unchanged.
      probe: { title: options.title ?? null, page: options.page ?? null, running: options.running ?? null,
        typography: options.typography ?? null, strictPagination: options.strictPagination ?? null,
        images: options.pdfImages.map(image => [image.destination, [...image.bytes]]) },
    };
    state.report = report;
    if (configuration.reportFault === "page-count") report.page_count = 2;
    if (configuration.reportFault === "profile") delete report.profile;
    return report;
  };
  const api = withFlowExports(session, {
    html() { throw Error("HTML must not be used for PDF verification"); }, pdf: render,
    ...(configuration.legacy ? {} : { verifyPdf }),
  }, selected.font ?? "serif", (expected, offset, limit) => {
    if (expected.revision !== state.revision) throw new FlowError("STALE_REVISION", "source changed");
    if (expected.layoutRevision !== state.layoutRevision) throw new FlowError("STALE_LAYOUT", "layout changed");
    state.inventoryReads++;
    const end = Math.min(offset + limit, images.length);
    return { schemaVersion: 1, ...token(), offset, total: images.length,
      nextOffset: end < images.length ? end : null,
      ...(state.profile === undefined ? {} : { profile: state.profile }),
      items: images.slice(offset, end).map(({ requestId, destination, isResolved }) =>
        ({ kind: "image", requestId, destination, isResolved })) };
  });
  return { api, session, state, calls, images };
}
