// Production book facade, worker protocol, retained renderer and output cache.
// NativeBook is an explicit ABI boundary double: its report's positioned text
// observes arguments; it does not lay out Markdown or attest PDF conformance.
import assert from "node:assert/strict";
import { isMainThread, parentPort, workerData } from "node:worker_threads";
import { createRetainedBook, createCachedBookRenderer } from "../book_retained.mjs";
import { installBookWorker } from "../book_worker.mjs";
import { nativeBookFixture } from "./book_pdfa_fixture.mjs";

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const canonical = value => {
  const parts = [];
  for (const part of value.replaceAll("\\", "/").split("/")) {
    if (part === "..") parts.pop();
    else if (part && part !== ".") parts.push(part);
  }
  return parts.join("/");
};

export function verificationReport(paths = ["one.md"], observation = {}) {
  const chapters = paths.map(path => ({ path: canonical(path), checked: 0, external: 0, unchecked: 0, findings: [] }));
  return {
    schema: "fmd-book-pdf-verification-v1", scope: "assembled-pdf-publication",
    layout: { schema_version: "1", target: "pdf", verdict: "clean", page_count: 1,
      digest: "fnv1a64:0123456789abcdef", anchors: { resolved: 0, unresolved: [] }, findings: [],
      pages: [{ number: 1, runs: [{ text: JSON.stringify(observation), x: 36, y: 720, size: 12, kind: "body" }] }] },
    navigation: { schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation", chapters,
      summary: { chapters: chapters.length, checked: 0, external: 0, unchecked: 0, findings: 0 } },
    destinations: [],
    pagination: { requestedOptimal: false, strategy: "greedy", fallback: null,
      paragraphVariants: { status: "not_requested", attemptedParagraphs: 0, limitedParagraphs: 0,
        offeredParagraphs: 0, selectedAlternatives: 0, paragraphItemLimit: false, retainedLineLimit: false,
        measurementWorkLimit: false, candidateStateLimit: false, failure: null } },
  };
}

export function verificationFixture({ old = false, counters } = {}) {
  const native = nativeBookFixture(), { api, NativeBook, state } = native;
  const bump = slot => { if (counters) Atomics.add(counters, slot, 1); };
  NativeBook.newWithProfile = (paths, sources, profile) => {
    const book = new NativeBook(paths, sources); book.settings.profile = profile; return book;
  };
  NativeBook.fromSourcesProfiled = (paths, sources, includedPaths, includedSources, profile) => {
    const book = NativeBook.fromSources(paths, sources, includedPaths, includedSources);
    book.settings.profile = profile; return book;
  };
  NativeBook.prototype.setAuthoringProfile = function(profile) { this.settings.profile = profile; };
  NativeBook.prototype.setBaselineGridPt = function(value) { this.settings.baselineGridPt = value; };
  NativeBook.prototype.verifyPdf = old ? undefined : function(profile, strict, page) {
    bump(0);
    assert.equal(arguments.length, 3); assert.ok(page instanceof Float64Array);
    if (this.files.some(file => file.source === "REFUSE"))
      throw Object.assign(Error("strict pagination cannot place the complete book"), { code: "PAGINATION_INFEASIBLE" });
    if (counters && this.files.some(file => file.source === "BLOCK")) {
      Atomics.store(counters, 1, 1);
      Atomics.wait(counters, 2, 0, 10000);
    }
    const observation = JSON.parse(new TextDecoder().decode(this.emit("verifyPdf", page, profile, strict)));
    const report = verificationReport(this.files.map(file => file.path), observation);
    // Preserve native-shaped original include provenance, independent of the
    // temporary publication binding and the parser-decoded target spelling.
    if (this.includes.length) {
      report.layout.anchors.resolved = 1;
      report.navigation.chapters[0].checked = 1; report.navigation.summary.checked = 1;
      report.destinations.push({ chapter: report.navigation.chapters[0].path,
        destination: `${report.navigation.chapters.at(-1).path}#two`, parsedDestination: "../two.md#two",
        kind: "link", status: "resolved", page: 1, y: 720,
        source: { precision: "destination", status: "mapped",
          ranges: [{ path: canonical(this.includes[0].path), start: 0, end: 4, kind: "rewritten" }] } });
    }
    return JSON.stringify(report);
  };
  const free = NativeBook.prototype.free;
  NativeBook.prototype.free = function() { bump(3); return free.call(this); };
  const preview = (engine, files, options) => engine.renderBookSite(files, options);
  const retained = createRetainedBook(api, preview), cache = createCachedBookRenderer(retained, preview);
  const engine = { ...api, renderRetainedBook: cache.render, clearRetainedBook: cache.clear };
  return { engine, state, cache, NativeBook };
}

if (!isMainThread) {
  const { engine } = verificationFixture({ ...workerData,
    counters: workerData.counters ? new Int32Array(workerData.counters) : undefined });
  if (workerData.oldEngine) engine.verifyBookPdf = undefined;
  installBookWorker({
    addEventListener(kind, listener) { if (kind === "message") parentPort.on(kind, data => listener({ data })); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  }, engine);
}

export const verificationBytes = (...args) => encode(verificationReport(...args));
