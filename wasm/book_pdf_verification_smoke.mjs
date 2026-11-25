// Real generated-WASM proof for assembled book PDF verification.
// Usage: node wasm/book_pdf_verification_smoke.mjs <matching-package-directory>
// This runner also boots the real packaged worker entry in a Node worker. It
// never substitutes Markdown parsing, PDF layout, native bindings or reports.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isMainThread, parentPort, Worker, workerData } from "node:worker_threads";

const encode = text => new TextEncoder().encode(text);
const decode = bytes => new TextDecoder("utf-8", { fatal: true }).decode(bytes);
const entry = (directory, name) => pathToFileURL(path.join(directory, name));

async function initialize(directory) {
  const renderer = await import(entry(directory, "franken_markdown.js"));
  await renderer.init(await readFile(path.join(directory, "pkg/franken_markdown_bg.wasm")));
  return renderer;
}

// Only the transport adapter is Node-specific. The actual worker entry owns
// protocol admission, captured sources, retained rendering and cancellation.
async function workerMain() {
  const waiting = [], listeners = new Set();
  parentPort.on("message", data => {
    if (!listeners.size) waiting.push(data);
    else for (const listener of listeners) listener({ data });
  });
  globalThis.self = {
    addEventListener(kind, listener) {
      assert.equal(kind, "message");
      listeners.add(listener);
      for (const data of waiting.splice(0)) queueMicrotask(() => listener({ data }));
    },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  };
  await initialize(workerData.packageDir);
  await import(entry(workerData.packageDir, "book_worker_entry.js"));
}

function workerFactory(directory, counters) {
  return () => {
    const worker = new Worker(new URL(import.meta.url), { workerData: { packageDir: directory } });
    counters.created++;
    let terminated = false;
    const listeners = new Map();
    worker.on("error", () => {});
    worker.on("exit", code => {
      if (code && !terminated) {
        for (const listener of listeners.get("error")?.keys() ?? []) {
          listener({ error: new Error(`Book worker exited with code ${code}`) });
        }
      }
    });
    return {
      postMessage: (value, transfer) => worker.postMessage(value, transfer),
      terminate() { terminated = true; counters.terminated++; return worker.terminate(); },
      addEventListener(kind, listener) {
        const wrapped = kind === "message" ? data => listener({ data }) : error => listener({ error });
        if (!listeners.has(kind)) listeners.set(kind, new Map());
        listeners.get(kind).set(listener, wrapped);
        worker.on(kind, wrapped);
      },
      removeEventListener(kind, listener) {
        const wrapped = listeners.get(kind)?.get(listener);
        if (wrapped) worker.off(kind, wrapped);
        listeners.get(kind)?.delete(listener);
      },
    };
  };
}

// Follow the emitted xref and page tree rather than searching binary font or
// content streams for strings that merely look like PDF dictionaries.
function publicationNavigation(bytes) {
  const pdf = Buffer.from(bytes);
  assert.equal(pdf.subarray(0, 5).toString("ascii"), "%PDF-");
  const start = pdf.lastIndexOf("startxref\n");
  assert(start >= 0);
  const xref = Number(pdf.subarray(start + 10).toString("ascii").match(/^\d+/)?.[0]);
  assert(Number.isSafeInteger(xref) && xref > 0 && xref < start);
  const rows = pdf.subarray(xref).toString("ascii").split(/\r?\n/);
  assert.equal(rows[0], "xref");
  const header = /^0 (\d+)$/.exec(rows[1]);
  assert(header);
  const count = Number(header[1]);
  assert.equal(rows[2].trim(), "0000000000 65535 f");
  const offsets = Array.from({ length: count - 1 }, (_, index) => {
    const fields = /^(\d{10}) 00000 n\s*$/.exec(rows[index + 3]);
    assert(fields);
    return Number(fields[1]);
  });
  const objects = offsets.map((offset, index) => {
    const object = pdf.subarray(offset, offsets[index + 1] ?? xref);
    assert(object.subarray(0, `${index + 1} 0 obj\n`.length).equals(Buffer.from(`${index + 1} 0 obj\n`)));
    const stream = object.indexOf("\nstream\n");
    return object.subarray(0, stream < 0 ? object.length : stream).toString("latin1");
  });
  const references = (dictionary, key) => {
    const start = dictionary.indexOf(key);
    if (start < 0) return [];
    const tail = dictionary.slice(start + key.length).split("]", 1)[0].trim();
    if (!tail) return [];
    const fields = tail.split(/\s+/);
    assert.equal(fields.length % 3, 0);
    return Array.from({ length: fields.length / 3 }, (_, index) => {
      assert.deepEqual(fields.slice(index * 3 + 1, index * 3 + 3), ["0", "R"]);
      return Number(fields[index * 3]);
    });
  };
  const tree = objects.find(object => object.includes("/Type /Pages "));
  assert(tree);
  const pages = references(tree, "/Kids [");
  assert(pages.length);
  const destinations = [];
  for (const id of pages) {
    const page = objects[id - 1];
    assert(page.includes("/Type /Page "));
    for (const annotation of references(page, "/Annots [")) {
      const dictionary = objects[annotation - 1];
      assert(dictionary.includes("/Subtype /Link"));
      assert(!dictionary.includes("fmd-book-pdf-nav/"));
      const target = /\/Dest \[\s*(\d+) 0 R \/XYZ null ([\d.-]+) null\s*\]/.exec(dictionary);
      if (!target) continue;
      const page = pages.indexOf(Number(target[1])) + 1, y = Number(target[2]);
      assert(page > 0 && Number.isFinite(y));
      destinations.push({ page, y });
    }
  }
  return { pageCount: pages.length, pages: pages.map(id => objects[id - 1]), destinations };
}

function assertPublication(bytes, report, width, height) {
  const publication = publicationNavigation(bytes);
  assert.equal(report.layout.page_count, publication.pageCount);
  assert.equal(report.layout.pages.length, publication.pageCount);
  for (const page of publication.pages) {
    const box = /\/MediaBox\s*\[0 0 ([\d.]+) ([\d.]+)\]/.exec(page);
    assert(box); assert.equal(Number(box[1]), width); assert.equal(Number(box[2]), height);
  }
  for (const target of report.destinations.filter(target => target.status === "resolved")) {
    assert(publication.destinations.some(emitted => emitted.page === target.page
      && Math.abs(emitted.y - target.y) <= 0.011), `reported target was not emitted: ${JSON.stringify(target)}`);
  }
  return publication;
}

function assertOutput(output, nativeJson, api, paths) {
  assert.equal(output.format, "book-pdf-verification");
  assert.equal(output.mimeType, "application/json");
  assert.equal(output.extension, "json");
  assert.equal(output.blob().type, "application/json");
  assert.equal(output.filename("verification"), "verification.json");
  assert.equal(decode(output.bytes), nativeJson, "public facade must retain complete native JSON bytes");
  const parsed = api.parseBookPdfVerificationReport(output.bytes, paths);
  assert.deepEqual(parsed, JSON.parse(nativeJson));
  assert(Object.isFrozen(parsed.layout.pages[0].runs));
  assert(Object.isFrozen(parsed.pagination.paragraphVariants));
  return parsed;
}

async function main() {
  const directory = path.resolve(process.argv[2] ?? fileURLToPath(new URL(".", import.meta.url)));
  await initialize(directory);
  const api = await import(entry(directory, "book.js"));
  const { FmdBook } = await import(entry(directory, "pkg/franken_markdown.js"));
  assert.equal(typeof FmdBook.prototype.verifyPdf, "function", "matching generated WASM rebuild required");
  assert.equal(typeof api.verifyBookPdf, "function", "matching book facade required");
  const chapters = [
    { path: "guide/one.md", source: "# Shared\n\n[Next](../two.md#shared)\n\n{{#include ../parts/body.md}}\n" },
    { path: "two.md", source: "# Shared\n\n[Back](guide/one.md#shared)\n" },
  ];
  const resources = [{ path: "parts/body.md", source: "Café included text[^n].\n\nTerm\n: Definition with **style**.\n\n![Chart](chart.svg)\n\n[Missing](#missing-included)\n\n[^n]: Kept note.\n\n[^unused]: [Dormant](#missing-note)\n" }];
  const font = new Uint8Array(await readFile(new URL("../tests/fixtures/fonts/source-sans-3/SourceSans3-Regular.otf", import.meta.url)));
  const image = encode('<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>');
  const typography = "optimal-pagination,footnote-backlinks,theme-spacing,line-height=1.7";
  const geometry = new Float64Array([480, 540, 36, 42, 30, 24]);
  const options = { profile: "commonmark-gfm", title: "Verification manual", author: "Author", lang: "en",
    includeSources: resources, images: [{ destination: "parts/chart.svg", bytes: image }],
    fontAssets: [{ slot: "body-regular", bytes: font }], toc: true, pageNumbers: true,
    typography, baseFontSize: 11, headingScale: 1.2, tocDepth: 2, baselineGridPt: 12,
    running: { header: { left: "{chapter}" } }, metadataEpochSeconds: 1700000000,
    page: { size: { widthPt: 480, heightPt: 540 }, margins: { topPt: 36, rightPt: 42, bottomPt: 30, leftPt: 24 } },
    pdfA: "2b", pdfAStrict: true };
  const paths = chapters.map(file => file.path);
  const raw = FmdBook.fromSourcesProfiled(paths, chapters.map(file => file.source),
    resources.map(file => file.path), resources.map(file => file.source), options.profile);
  // Configure generated Rust directly, independently of createBook's setters.
  raw.setMetadata(options.title, options.author, options.lang);
  raw.setTheme("sans", "auto"); raw.setNavigation(true, true);
  raw.setPdfOptions(typography, 11, 1.2, undefined, 2, undefined, false, 1700000000,
    ["{chapter}", "", "", "", "", ""], false, false, false);
  raw.setBaselineGridPt(12); raw.setImage("parts/chart.svg", image); raw.setFont("body-regular", font);
  const session = await api.createBook(chapters, options);
  const passed = [];
  let baseline, changedOutput;
  try {
    const nativeDefault = raw.renderPdf(), nativeRevision = raw.sourceRevision;
    baseline = session.verifyPdf();
    const nativeJson = raw.verifyPdf("2b", true, geometry);
    const report = assertOutput(baseline, nativeJson, api, paths);
    assert.deepEqual((await api.verifyBookPdf(chapters, options)).bytes, baseline.bytes);
    assert.equal(baseline.sourceLength, raw.sourceLength);
    assert.equal(baseline.profile, "commonmark-gfm");
    assert.equal(baseline.pdfA, "2b"); assert.equal(baseline.pdfAStrict, true);
    assert(report.pagination.requestedOptimal);
    assert(report.destinations.some(target => target.kind === "footnote_backreference" && target.status === "resolved"));
    assert(!report.layout.findings.some(finding => ["unresolved_image", "unsupported_image"].includes(finding.code)));
    const preview = JSON.parse(decode(session.renderChapterPreview(0).bytes));
    assert(!preview.html.includes("<dl>"), "selected source dialect must survive publication");
    for (const destination of ["#missing-included", "#missing-note"]) {
      const target = report.destinations.find(target => target.destination === destination);
      assert(target); assert.equal(target.status, "unresolved"); assert.equal(target.page, null); assert.equal(target.y, null);
      assert.equal(target.source.precision, "destination"); assert.equal(target.source.status, "mapped");
      assert.equal(target.source.ranges.length, 1);
      const range = target.source.ranges[0];
      assert.equal(range.path, "parts/body.md");
      assert.equal(decode(encode(resources[0].source).subarray(range.start, range.end)), destination);
    }
    assert.equal(report.navigation.summary.findings, 1, "HTML navigation excludes the unreferenced note");
    assert(report.layout.findings.some(finding => finding.code === "unresolved_pdf_destination" && finding.detail.includes("#missing-note")));
    assert(report.layout.pages.some(page => page.runs.some(run => run.text.includes("Dormant"))));
    const pdf = session.renderPdf();
    assertPublication(pdf.bytes, report, 480, 540);
    assert(Buffer.from(pdf.bytes).includes(Buffer.from("/OutputIntents")));
    assert(Buffer.from(pdf.bytes).includes(Buffer.from("/FontFile3")), "authorized host CFF reaches publication");
    assert.deepEqual(pdf.bytes, raw.renderPdfArchival("2b", true, geometry));
    passed.push("public/session/raw exact JSON parity with configured assets, dialect, grid, includes and archival publication");
    passed.push("physical PDF pages and link annotations agree with the report; dormant notes retain original source findings");

    const override = session.verifyPdf({ page: {}, pdfA: "off" });
    const defaultGeometry = new Float64Array([612, 792, 72, 72, 72, 72]);
    const overridden = assertOutput(override, raw.verifyPdf("off", false, defaultGeometry), api, paths);
    assert.equal(override.pdfA, undefined); assert.equal(override.pdfAStrict, undefined);
    assert.notDeepEqual(override.bytes, baseline.bytes);
    assertPublication(session.renderPdf({ page: {}, pdfA: "off" }).bytes, overridden, 612, 792);
    assert.throws(() => session.verifyPdf({ pdfA: "off", pdfAStrict: true }), { code: "INVALID_OPTIONS" });
    assert.throws(() => session.verifyPdf({ page: { size: { widthPt: NaN, heightPt: 540 } } }));
    assert.throws(() => raw.verifyPdf("3b", false, geometry));
    assert.deepEqual(session.verifyPdf().bytes, baseline.bytes);
    assert.deepEqual(session.renderPdf().bytes, pdf.bytes);
    assert.deepEqual(raw.renderPdf(), nativeDefault);
    assert.equal(raw.sourceRevision, nativeRevision); assert.equal(session.sourceRevision, 0);
    passed.push("verification overrides and rejected requests preserve native state and retained export defaults");

    resources[0].source = resources[0].source.replace("Café included", "Updated café included")
      .replace("#missing-included", "#shared").replace("#missing-note", "#shared");
    const update = session.updateSources(resources, { expectedRevision: 0 });
    raw.updateSources(resources.map(file => file.path), resources.map(file => file.source), 0);
    assert.equal(update.revision, 1);
    changedOutput = session.verifyPdf();
    const changed = assertOutput(changedOutput, raw.verifyPdf("2b", true, geometry), api, paths);
    assert.notDeepEqual(changedOutput.bytes, baseline.bytes);
    assert.equal(changed.layout.verdict, "clean"); assert.deepEqual(changed.layout.anchors.unresolved, []);
    assert.deepEqual(changedOutput.bytes, (await api.verifyBookPdf(chapters, options)).bytes);
    assert.equal(session.sourceRevision, 1); assert.equal(changedOutput.sourceLength, raw.sourceLength);
    passed.push("updated include revisions match fresh native verification without stale findings");
  } finally { session.dispose(); raw.free(); }

  const strict = await api.createBook([{ path: "one.md", source: "A  \nB  \nC" }],
    { strictPagination: true, baseFontSize: 24 });
  try {
    const before = strict.renderPdf().bytes;
    const cramped = { page: { size: { widthPt: 400, heightPt: 144 }, margins: 36 } };
    assert.throws(() => strict.verifyPdf(cramped), { code: "PAGINATION_INFEASIBLE" });
    assert.throws(() => strict.renderPdf(cramped), { code: "PAGINATION_INFEASIBLE" });
    assert.deepEqual(strict.renderPdf().bytes, before); assert.equal(strict.sourceRevision, 0);
  } finally { strict.dispose(); }
  const restricted = await api.createBook([{ path: "one.md", source: "# One\n\n[Local](file:///private/document)\n" }]);
  try {
    const before = restricted.renderPdf().bytes;
    assert.throws(() => restricted.verifyPdf({ pdfA: "2b", pdfAStrict: true }), /pdf_a_file_uri/);
    restricted.verifyPdf({ pdfA: "2b", pdfAStrict: false });
    assert.deepEqual(restricted.renderPdf().bytes, before);
  } finally { restricted.dispose(); }
  passed.push("strict pagination and PDF/A policy failures propagate without fallback or mutation");

  // Include-only paths need not fit the derived 255-byte chapter output name.
  // One hundred real references cross the source-only 256 KiB path allowance
  // while remaining comfortably inside the native PDF provenance allowance.
  const resourcePath = ["parts", ...Array(400).fill("nested"), "body.md"].join("/");
  const longResources = [{ path: resourcePath, source: Array.from({ length: 100 }, (_, index) => `[Link${index}](#start)`).join(" ") }];
  const longChapters = [{ path: "start.md", source: `# Start\n\n{{#include ${resourcePath}}}\n` }];
  const longRaw = FmdBook.fromSources(["start.md"], [longChapters[0].source], [resourcePath], [longResources[0].source]);
  let provenanceBytes;
  try {
    const output = await api.verifyBookPdf(longChapters, { includeSources: longResources });
    const report = assertOutput(output, longRaw.verifyPdf("off", false, new Float64Array()), api, ["start.md"]);
    assert.equal(report.destinations.length, 100);
    provenanceBytes = report.destinations.reduce((sum, target) => {
      assert.equal(target.status, "resolved"); assert.equal(target.source.status, "mapped");
      assert.equal(target.source.ranges.length, 1); assert.equal(target.source.ranges[0].path, resourcePath);
      return sum + encode(target.source.ranges[0].path).length;
    }, 0);
    assert(provenanceBytes > 256 * 1024);
    assert(provenanceBytes + 32 * 100 < 2 * 1024 * 1024);
    assert(output.bytes.length < 8 * 1024 * 1024);
  } finally { longRaw.free(); }
  passed.push("legitimate native PDF provenance exceeding the source-only path budget remains admissible");

  const { createBookWorker } = await import(entry(directory, "book-worker.js"));
  const counters = { created: 0, terminated: 0 };
  const worker = createBookWorker({ workerFactory: workerFactory(directory, counters),
    timeoutMs: 120000, retainBook: true, idleTimeoutMs: 120000 });
  try {
    const actual = await worker.render(chapters, "pdf-verification", options);
    assert.deepEqual(actual.bytes, changedOutput.bytes); assert.equal(actual.sourceLength, changedOutput.sourceLength);
    assert.equal(actual.profile, options.profile); assert.equal(actual.pdfAStrict, true);
    const revised = [{ ...resources[0], source: `${resources[0].source}\nWorker revision.\n` }];
    const delta = await worker.renderSourceUpdate(revised, "pdf-verification",
      { expectedRevision: worker.retainedInputRevision });
    assert.deepEqual(delta.bytes, (await api.verifyBookPdf(chapters, { ...options, includeSources: revised })).bytes);
    assert.equal(counters.created, 1, "source delta must reuse the actual native worker");
    const abort = new AbortController();
    const pending = worker.render(chapters, "pdf-verification", options, { signal: abort.signal });
    abort.abort();
    await assert.rejects(pending, { code: "EXPORT_CANCELLED" });
    assert.equal(worker.busy, false); assert.equal(worker.hasRetainedBook, false);
    assert.equal(counters.terminated, 1);
    const recovered = await worker.render(chapters, "pdf-verification", options);
    assert.deepEqual(recovered.bytes, changedOutput.bytes); assert.equal(counters.created, 2);
  } finally { worker.dispose(); }
  passed.push("real packaged worker parity, retained source deltas, pending cancellation and recovery");
  console.log(JSON.stringify({ schema: "fmd-book-pdf-verification-smoke-v1", native: true,
    passed, provenancePathBytes: provenanceBytes, workersCreated: counters.created }, null, 2));
}

if (!isMainThread) {
  await workerMain();
} else {
  try { await main(); }
  catch (error) {
    console.error(`FAIL real-WASM book PDF verification: ${String(error?.stack ?? error)}`);
    process.exitCode = 1;
  }
}
