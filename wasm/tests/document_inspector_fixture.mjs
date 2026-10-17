// A real Node worker with explicitly synthetic native-report fixtures. This
// exercises production inspection/transport, not Rust parsing or audit quality.
import { isMainThread, parentPort, workerData } from "node:worker_threads";
import { installDocumentInspector } from "../document_inspector.mjs";

export function statsFixture(source) {
  return { schema: "fmd-document-stats-v1", bytes: new TextEncoder().encode(source).length,
    lines: source ? 1 : 0, words: 7, characters: 27, sentences: 2, syllables: 11,
    reading_time_secs: 2, speaking_time_secs: 3,
    flesch_reading_ease: 71.2, flesch_kincaid_grade: 6.3,
    coleman_liau_index: 8.2, automated_readability_index: 5.4,
    reading_ease_label: "Fixture readability", structure: { headings_total: 1, paragraphs: 1 },
    outline: [{ level: 1, text: "Fixture <heading>", slug: "fixture-heading" }],
    findings: [{ severity: "warning", code: "broken_internal_anchor", message: "Fixture #missing" }],
  };
}
export function auditFixture() {
  return { schema_version: "1", target: "pdf", findings: [
    { code: "image_missing_alt", detail: "Fixture <image> has no alt text" },
  ], future: { evidence: ["native additive fields", null, true, -0] } };
}
export function envelopeFixture(source) {
  const report = { schema: "fmd-document-inspection-v1",
    sourceLength: new TextEncoder().encode(source).length,
    stats: statsFixture(source), accessibility: auditFixture() };
  return { format: "document-inspection", sourceLength: report.sourceLength,
    bytes: new TextEncoder().encode(JSON.stringify(report)) };
}

if (!isMainThread) {
  const wrapped = new Map();
  const endpoint = {
    addEventListener(type, listener) {
      const fn = data => listener({ data });
      wrapped.set(listener, fn); parentPort.on(type, fn);
    },
    removeEventListener(type, listener) { parentPort.off(type, wrapped.get(listener)); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  };
  const counts = workerData.counts && new Int32Array(workerData.counts);
  const latch = workerData.latch && new Int32Array(workerData.latch);
  const block = () => { Atomics.store(latch, 0, 1); Atomics.wait(latch, 1, 0); };
  let lastStats;
  installDocumentInspector(endpoint, async () => {
    if (counts) Atomics.add(counts, 0, 1);
    if (workerData.loadFailure) throw new Error("fixture load failure");
    const engine = {
      async documentStats(source) {
        if (counts) Atomics.add(counts, 1, 1);
        if (source === "block-stats") block();
        if (source === "throw-stats") throw new Error("fixture statistics failure");
        const stats = statsFixture(source);
        Object.assign(stats, workerData.stats);
        if (workerData.extraText) stats.extraText = workerData.extraText;
        if (workerData.depth !== undefined) {
          let node = "leaf";
          for (let i = 0; i < workerData.depth; i++) node = { child: node };
          stats.nested = node;
        }
        if (workerData.wide) stats.wide = Array.from({ length: workerData.wide }, () => 0);
        switch (workerData.invalid) {
          case "cycle": stats.cycle = stats; break;
          case "accessor": Object.defineProperty(stats, "hidden", {
            enumerable: true, get() { throw new Error("accessor was invoked"); },
          }); break;
          case "hidden": Object.defineProperty(stats, "hidden", { value: 1 }); break;
          case "symbol": stats[Symbol("unsupported")] = true; break;
          case "bigint": stats.big = 1n; break;
          case "function": stats.toJSON = () => ({}); break;
          case "sparse": stats.sparse = new Array(2); break;
          case "array-key": stats.extra = Object.assign([1], { other: 2 }); break;
          case "date": stats.date = new Date(0); break;
          case "nan": stats.words = NaN; break;
          case "unicode": stats.extra = "\ud800"; break;
        }
        lastStats = stats;
        return stats;
      },
      async accessibilityAudit(source) {
        if (counts) Atomics.add(counts, 2, 1);
        if (source === "block-audit") block();
        if (source === "throw-audit") throw new Error("fixture accessibility failure");
        if (workerData.mutateStats) {
          lastStats.words = 9999;
          lastStats.outline[0].text = "mutated by later native pass";
        }
        const result = auditFixture();
        Object.assign(result, workerData.audit);
        return result;
      },
    };
    if (workerData.missing) delete engine[workerData.missing];
    return engine;
  });
}
