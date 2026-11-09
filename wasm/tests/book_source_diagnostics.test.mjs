// Production adapters and worker transport with explicit native/DOM doubles.
// Rust parser behavior is exercised separately by the book integration tests.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings, parseBookSourceDiagnosticsReport } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createBookSourceDiagnosticsControls, createBookSourceDiagnosticsPanel } from "../demo/book_inspection_controls.mjs";
import { bookDiagnosticSelections, BOOK_SOURCE_DIAGNOSTICS_MAX_BYTES } from "../book_source_diagnostics.mjs";

const files = [{ path: "guide.md", source: "# Guide\n{{#include part.md}}" }];
const resources = [{ path: "part.md", source: "literal fallback" }];
const encoded = value => new TextEncoder().encode(JSON.stringify(value));
const report = () => ({
  schema: "fmd-book-source-diagnostics-v1",
  coordinateSpace: "expanded_chapter",
  chapters: [{
    path: "guide.md", sourceBytes: 40,
    diagnostics: [{
      code: "inline_work_limit", severity: "warning", start: 8, end: 40,
      message: "Nested source retained as literal text.",
    }],
  }],
  summary: { chapters: 1, diagnostics: 1 },
});
const gate = () => {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
};
const code = expected => error => error.code === expected;

function bindings({ value = () => JSON.stringify(report()), supported = true, load = async () => {} } = {}) {
  const state = { created: 0, freed: 0, sources: null, checks: 0 };
  class Book {
    constructor(paths, sources) {
      state.created++;
      this.chapterCount = paths.length;
      this.sourceLength = sources.reduce((total, source) => total + new TextEncoder().encode(source).length, 0);
    }
    static fromSources(paths, sources, includes, includeText) {
      state.sources = { paths, sources, includes, includeText };
      return new Book(paths, [...sources, ...includeText]);
    }
    setMetadata() {}
    setCustomCss() {}
    setTheme() {}
    setNavigation() {}
    setImage() { throw new Error("Diagnostic checking must not load images."); }
    setFont() { throw new Error("Diagnostic checking must not load fonts."); }
    sourceDiagnostics() { state.checks++; return value(); }
    renderPdf() { return new Uint8Array([37, 80, 68, 70]); }
    free() { state.freed++; }
  }
  if (!supported) Book.prototype.sourceDiagnostics = undefined;
  return { state, api: createBookBindings(async () => { await load(); return Book; }) };
}

function sourceOptions() {
  const options = { includeSources: resources.map(file => ({ ...file })) };
  for (const key of ["images", "fontAssets", "page", "pdfA", "title", "customCss"]) {
    Object.defineProperty(options, key, { get() { throw new Error("Read " + key); } });
  }
  return options;
}

test("one-shot diagnostics use include sources without reading publication grants", async () => {
  const { api, state } = bindings();
  const result = await api.checkBookSourceDiagnostics(files, sourceOptions());
  assert.equal(result.format, "book-diagnostics");
  assert.equal(result.filename(), "book-source-diagnostics.json");
  assert.deepEqual(JSON.parse(await result.blob().text()), report());
  assert.deepEqual(state.sources.includes, ["part.md"]);
  assert.equal(state.created, 1);
  assert.equal(state.freed, 1);
  const parsed = parseBookSourceDiagnosticsReport(result.bytes, ["guide.md"]);
  assert.equal(parsed.summary.diagnostics, 1);
  assert.equal(parsed.coordinateSpace, "expanded_chapter");
  assert.ok(Object.isFrozen(parsed));
  assert.ok(Object.isFrozen(parsed.chapters[0].diagnostics[0]));
});

test("retained diagnostics are read-only and output survives session disposal", async () => {
  const { api, state } = bindings();
  const session = await api.createBook(files, { includeSources: resources });
  const first = session.sourceDiagnostics();
  assert.deepEqual(session.renderPdf().bytes, new Uint8Array([37, 80, 68, 70]));
  assert.deepEqual(session.sourceDiagnostics().bytes, first.bytes);
  assert.equal(state.created, 1);
  assert.equal(state.checks, 2);
  session.dispose();
  assert.equal(state.freed, 1);
  assert.throws(() => session.sourceDiagnostics(), /disposed/);
  assert.equal(parseBookSourceDiagnosticsReport(first.bytes, ["guide.md"]).summary.diagnostics, 1);
});

test("one-shot sources are captured before asynchronous engine loading", async () => {
  const pending = gate();
  const { api, state } = bindings({ load: () => pending.promise });
  const chapters = files.map(file => ({ ...file }));
  const options = sourceOptions();
  const result = api.checkBookSourceDiagnostics(chapters, options);
  chapters[0].source = "mutated chapter";
  options.includeSources[0].source = "mutated include";
  pending.resolve();
  await result;
  assert.deepEqual(state.sources.sources, [files[0].source]);
  assert.deepEqual(state.sources.includeText, [resources[0].source]);
});

test("unsupported and failed native checks dispose one-shot handles without disabling old rendering", async () => {
  const missing = bindings({ supported: false });
  await assert.rejects(missing.api.checkBookSourceDiagnostics(files, { includeSources: resources }),
    code("UNSUPPORTED_BOOK_SOURCE_DIAGNOSTICS"));
  assert.equal(missing.state.freed, 1);
  const failed = bindings({ value: () => { throw "book_diagnostics: report budget exceeded"; } });
  await assert.rejects(failed.api.checkBookSourceDiagnostics(files, { includeSources: resources }),
    /report budget exceeded/);
  assert.equal(failed.state.freed, 1);
  const session = await missing.api.createBook(files, { includeSources: resources });
  assert.throws(() => session.sourceDiagnostics(), code("UNSUPPORTED_BOOK_SOURCE_DIAGNOSTICS"));
  assert.deepEqual(session.renderPdf().bytes, new Uint8Array([37, 80, 68, 70]));
  session.dispose();
});

test("wire validation rejects mismatched, malformed and oversized diagnostic reports", () => {
  const mutations = [
    value => { value.coordinateSpace = "original_source"; },
    value => { value.chapters[0].path = "other.md"; },
    value => { value.chapters[0].diagnostics[0].end = 41; },
    value => { value.chapters[0].diagnostics[0].start = 41; },
    value => { value.chapters[0].diagnostics[0].start = 0.5; },
    value => { value.chapters[0].diagnostics[0].code = "bad code"; },
    value => { value.chapters[0].diagnostics[0].message = "bad\ud800"; },
    value => { value.chapters[0].diagnostics[0].message = "中".repeat(683); },
    value => { value.chapters[0].diagnostics[0].severity = "success"; },
    value => { value.chapters[0].sourceBytes = 64 * 1024 * 1024 + 1; },
    value => { value.summary.diagnostics = 0; },
    value => { value.summary.chapters = 0; },
    value => { value.chapters[0].diagnostics = Array.from({ length: 4097 }, () => ({ ...value.chapters[0].diagnostics[0] })); },
  ];
  for (const mutate of mutations) {
    const value = report();
    mutate(value);
    assert.throws(() => parseBookSourceDiagnosticsReport(encoded(value), ["guide.md"]),
      code("INVALID_BOOK_SOURCE_DIAGNOSTICS"));
  }
  for (const bytes of [new Uint8Array(), new Uint8Array([0xff]), new Uint8Array(4 * 1024 * 1024 + 1)]) {
    assert.throws(() => parseBookSourceDiagnosticsReport(bytes, ["guide.md"]),
      code("INVALID_BOOK_SOURCE_DIAGNOSTICS"));
  }
});

function transport(engine) {
  const workers = [];
  const received = [];
  const workerFactory = () => {
    const worker = new EventTarget();
    const scope = new EventTarget();
    worker.terminated = false;
    worker.terminate = () => { worker.terminated = true; };
    worker.postMessage = message => {
      const data = structuredClone(message);
      received.push(data);
      queueMicrotask(() => scope.dispatchEvent(new MessageEvent("message", { data })));
    };
    scope.postMessage = message => {
      const data = structuredClone(message);
      if (!worker.terminated) {
        queueMicrotask(() => worker.dispatchEvent(new MessageEvent("message", { data })));
      }
    };
    installBookWorker(scope, engine);
    workers.push(worker);
    return worker;
  };
  return { workerFactory, workers, received };
}

test("worker diagnostics stay one-shot even when publication retention is enabled", async () => {
  const { api, state } = bindings();
  const wire = transport(api);
  const client = createBookWorkerClient({ workerFactory: wire.workerFactory, retainBook: true });
  try {
    const result = await client.render(files, "diagnostics", sourceOptions());
    assert.equal(result.format, "book-diagnostics");
    assert.equal(parseBookSourceDiagnosticsReport(result.bytes, ["guide.md"]).summary.diagnostics, 1);
    assert.equal(wire.received[0].retainBook, undefined);
    assert.deepEqual(wire.received[0].options.images, []);
    assert.deepEqual(wire.received[0].options.fontAssets, []);
    assert.equal(state.freed, 1);
    assert.equal(wire.workers[0].terminated, true);
  } finally { client.dispose(); }
});

test("worker cancellation retires pending diagnostic work", async () => {
  const pending = gate();
  const { api } = bindings({ load: () => pending.promise });
  const wire = transport(api);
  const client = createBookWorkerClient({ workerFactory: wire.workerFactory });
  const result = client.render(files, "diagnostics", { includeSources: resources });
  client.cancel();
  await assert.rejects(result, code("EXPORT_CANCELLED"));
  pending.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(wire.workers[0].terminated, true);
  assert.equal(client.busy, false);
  client.dispose();
});

function controlsFixture(t, render, selectedFiles = [
  ...files.map(file => ({ ...file, role: "chapter" })),
  ...resources.map(file => ({ ...file, role: "include" })),
]) {
  const ids = new Map(), listeners = new Set(), selected = [], revoked = [];
  class Element extends EventTarget {
    constructor(tag = "") { super(); this.tagName = tag; this.children = []; this.textContent = ""; }
    setAttribute(key, value) {
      if (key === "id") ids.set(value, this);
      this[key] = key === "disabled" || key === "hidden" ? true : value;
    }
    removeAttribute(key) { delete this[key]; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = [...children]; }
    set innerHTML(_) { throw new Error("Parser report UI must not parse HTML."); }
  }
  const root = {
    querySelector: selector => selector.startsWith("#") ? ids.get(selector.slice(1)) ?? null : null,
    createElement: tag => new Element(tag),
    createTextNode: value => ({ textContent: value }),
    body: new Element("body"),
  };
  createBookSourceDiagnosticsPanel(root);
  let stamp = 0;
  const collection = {
    files: selectedFiles,
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
  };
  const controls = {
    sourceBusy: false,
    checkpoint: () => stamp,
    captureProject: () => ({ files: collection.files.map(file => ({ ...file })) }),
    selectSourceRange(...args) { selected.push(args); return ++stamp; },
    subscribeSourceState: () => () => {},
  };
  const worker = { render, cancel() {}, dispose() {} };
  const api = createBookSourceDiagnosticsControls({
    root, controls, collection, worker,
    urls: { createObjectURL: () => "blob:parser", revokeObjectURL: url => revoked.push(url) },
  });
  t.after(() => api.dispose());
  return { api, ids, selected, revoked, collection, controls,
    silentChange() { stamp++; },
    change() { stamp++; for (const listener of listeners) listener(); } };
}

test("browser diagnostics show honest expanded ranges and retire stale report downloads", async t => {
  const fixture = controlsFixture(t, async (chapters, format, options) => {
    assert.deepEqual(chapters, files);
    assert.equal(format, "diagnostics");
    assert.deepEqual(options.includeSources, resources);
    return { format: "book-diagnostics", bytes: encoded(report()) };
  });
  await fixture.api.run();
  assert.match(fixture.ids.get("parser-summary").textContent, /1 parser diagnostics/);
  const row = fixture.ids.get("parser-findings").children[0];
  assert.match(row.children[0].textContent, /inline_work_limit/);
  assert.match(row.children[1].textContent, /UTF-8 bytes 8–40/);
  fixture.api.open("guide.md");
  assert.deepEqual(fixture.selected[0].slice(0, 3), [0, 0, 0]);
  fixture.api.save();
  assert.equal(fixture.ids.get("parser-download").download, "book-source-diagnostics.json");
  fixture.change();
  assert.equal(fixture.ids.get("parser-download").hidden, true);
  assert.deepEqual(fixture.revoked, ["blob:parser"]);
  assert.equal(fixture.ids.get("parser-findings").children.length, 0);
});

test("browser diagnostics reject a result completed for an older source capture", async t => {
  const pending = gate();
  const fixture = controlsFixture(t, () => pending.promise);
  const result = fixture.api.run();
  fixture.change();
  pending.resolve({ format: "book-diagnostics", bytes: encoded(report()) });
  await assert.rejects(result, code("STALE_SOURCE_DIAGNOSTICS"));
  assert.equal(fixture.ids.get("parser-findings").children.length, 0);
  assert.equal(fixture.ids.get("parser-summary").textContent, "");
});

function withSource(ranges, value = report()) {
  value.chapters[0].diagnostics[0].source = { status: "mapped", ranges };
  return value;
}
const origin = (path = "part.md", start = 0, end = 7, kind = "exact") => ({ path, start, end, kind });

test("source locations remain frozen and legacy packages explicitly lack attribution", () => {
  const value = withSource([origin()]);
  const parsed = parseBookSourceDiagnosticsReport(encoded(value), ["guide.md"]);
  const source = parsed.chapters[0].diagnostics[0].source;
  assert.deepEqual(source, value.chapters[0].diagnostics[0].source);
  assert.ok(Object.isFrozen(source) && Object.isFrozen(source.ranges) && Object.isFrozen(source.ranges[0]));
  assert.equal(parseBookSourceDiagnosticsReport(encoded(report())).chapters[0].diagnostics[0].source, undefined);
  value.chapters[0].diagnostics[0].source = { status: "unavailable", reason: "provenance_limit" };
  assert.deepEqual(parseBookSourceDiagnosticsReport(encoded(value)).chapters[0].diagnostics[0].source,
    { status: "unavailable", reason: "provenance_limit" });
});

test("source location validation rejects partial, malformed and oversized mappings", () => {
  for (const source of [
    null, { status: "mapped", ranges: [] },
    { status: "mapped", ranges: Array.from({ length: 65 }, () => origin()) },
    { status: "unavailable", reason: "limit", ranges: [origin()] },
    { status: "unavailable", reason: "invented location" },
    ...[origin("../part.md"), origin("/part.md"), origin("https:part.md"),
      origin("part.md", 1, 2, "generated"), origin("part.md", 0.5),
      origin("part.md", 4, 3), origin("part.md", 0, 1, "guessed"),
      origin("x".repeat(4097))].map(range => ({ status: "mapped", ranges: [range] })),
  ]) {
    const value = report();
    value.chapters[0].diagnostics[0].source = source;
    assert.throws(() => parseBookSourceDiagnosticsReport(encoded(value)), code("INVALID_BOOK_SOURCE_DIAGNOSTICS"));
  }
});

test("original byte coordinates convert to UTF-16 without flattening CRLF or splitting scalars", () => {
  const source = "\uFEFF😀\r\nPréface\r\n[broken]:\r\n";
  const from = source.indexOf("[broken]"), to = from + "[broken]:".length;
  const start = new TextEncoder().encode(source.slice(0, from)).length;
  const end = new TextEncoder().encode(source.slice(0, to)).length;
  const parsed = parseBookSourceDiagnosticsReport(encoded(withSource([origin("part.md", start, end)])));
  const range = parsed.chapters[0].diagnostics[0].source.ranges[0];
  const selections = bookDiagnosticSelections(parsed, [{ path: "part.md", source }]);
  assert.deepEqual(selections.get(range), { path: "part.md", source, start: from, end: to });
  assert.notEqual(start, from);
  for (const [start, end] of [[1, 2], [3, 5], [0, 1000]]) {
    const invalid = parseBookSourceDiagnosticsReport(encoded(withSource([origin("part.md", start, end)])));
    assert.throws(() => bookDiagnosticSelections(invalid, [{ path: "part.md", source }]),
      code("INVALID_BOOK_SOURCE_DIAGNOSTICS"));
  }
});

test("workbench selects include-only source ranges and keeps split origins separate", async t => {
  const snippet = "😀\r\n[broken]:\r\n";
  const start = new TextEncoder().encode("😀\r\n").length;
  const value = withSource([origin("part.md", start, start + 9), origin("guide.md", 0, 7)]);
  const fixture = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(value) }), [
    { ...files[0], role: "chapter" }, { path: "part.md", source: snippet, role: "include" },
  ]);
  const checked = await fixture.api.run();
  const finding = checked.chapters[0].diagnostics[0];
  const row = fixture.ids.get("parser-findings").children[0];
  assert.match(row.children[3].textContent, /Select source: part.md/);
  assert.match(row.children[4].textContent, /Select source: guide.md/);
  fixture.api.openSource(finding, 0);
  assert.deepEqual(fixture.selected[0].slice(0, 3), [1, 4, 13]);
  fixture.api.openSource(finding, 1);
  assert.deepEqual(fixture.selected[1].slice(0, 3), [0, 0, 7]);
  fixture.change();
  assert.throws(() => fixture.api.openSource(finding), code("STALE_SOURCE_DIAGNOSTICS"));
  assert.equal(fixture.selected.length, 2);
});

test("generated boundaries and rewritten tokens retain truthful source-selection labels", async t => {
  const value = withSource([origin("part.md", 7, 7, "generated"), origin("part.md", 0, 7, "rewritten")]);
  const fixture = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(value) }));
  const checked = await fixture.api.run();
  const row = fixture.ids.get("parser-findings").children[0];
  assert.match(row.children[3].textContent, /Open boundary/);
  assert.match(row.children[4].textContent, /original URL token/);
  fixture.api.openSource(checked.chapters[0].diagnostics[0], 0);
  assert.deepEqual(fixture.selected[0].slice(0, 3), [1, 7, 7]);
  assert.match(fixture.ids.get("parser-status").textContent, /inserted a separator/);
});

test("unknown sources and changed source bodies cannot acquire navigation authority", async t => {
  const unknown = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(withSource([origin("unselected.md")])) }));
  await assert.rejects(unknown.api.run(), code("INVALID_BOOK_SOURCE_DIAGNOSTICS"));
  assert.equal(unknown.selected.length, 0);
  const fixture = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(withSource([origin()])) }));
  const checked = await fixture.api.run();
  fixture.collection.files[1].source = "changed without a notification";
  assert.throws(() => fixture.api.openSource(checked.chapters[0].diagnostics[0]), code("STALE_SOURCE_DIAGNOSTICS"));
  assert.equal(fixture.selected.length, 0);
});

test("legacy or unavailable mappings keep expanded diagnostics and explain absent navigation", async t => {
  for (const source of [undefined, { status: "unavailable", reason: "range_limit" }]) {
    const value = report();
    if (source) value.chapters[0].diagnostics[0].source = source;
    const fixture = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(value) }));
    const checked = await fixture.api.run();
    const row = fixture.ids.get("parser-findings").children[0];
    assert.equal(row.children.length, 4);
    assert.match(row.children[3].textContent, source ? /unavailable \(range_limit\)/ : /does not provide original-source locations/);
    assert.throws(() => fixture.api.openSource(checked.chapters[0].diagnostics[0]), code("INVALID_SELECTION"));
    fixture.api.open("guide.md");
    assert.deepEqual(fixture.selected[0].slice(0, 3), [0, 0, 0]);
  }
});

test("provenance has its own wire allowance without expanding the base diagnostic limit", () => {
  const value = report();
  const base = encoded(value).byteLength;
  value.padding = "x".repeat(4 * 1024 * 1024 - base - 14);
  const baseBytes = encoded(value).byteLength;
  assert.ok(baseBytes <= 4 * 1024 * 1024);
  value.chapters[0].diagnostics[0].source = { status: "unavailable", reason: "provenance_limit" };
  const bytes = encoded(value);
  assert.ok(bytes.byteLength > 4 * 1024 * 1024 && bytes.byteLength < BOOK_SOURCE_DIAGNOSTICS_MAX_BYTES);
  assert.equal(parseBookSourceDiagnosticsReport(bytes).summary.diagnostics, 1);
  value.padding += "x".repeat(32);
  assert.throws(() => parseBookSourceDiagnosticsReport(encoded(value)), code("INVALID_BOOK_SOURCE_DIAGNOSTICS"));
});

test("both worker directions admit the separate provenance allowance above four MiB", async () => {
  const value = report();
  value.padding = "x".repeat(4 * 1024 * 1024 - encoded(value).byteLength - 14);
  value.chapters[0].diagnostics[0].source = { status: "unavailable", reason: "provenance_limit" };
  const bytes = encoded(value);
  const { api } = bindings({ value: () => new TextDecoder().decode(bytes) });
  const wire = transport(api);
  const client = createBookWorkerClient({ workerFactory: wire.workerFactory });
  try {
    const result = await client.render(files, "diagnostics", { includeSources: resources });
    assert.equal(result.bytes.byteLength, bytes.byteLength);
    assert.equal(parseBookSourceDiagnosticsReport(result.bytes).summary.diagnostics, 1);
  } finally { client.dispose(); }
});

test("source navigation never adopts a checkpoint changed during host focus or selection", async t => {
  const fixture = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(withSource([origin()])) }));
  const checked = await fixture.api.run();
  fixture.api.save();
  const select = fixture.controls.selectSourceRange;
  fixture.controls.selectSourceRange = (...args) => {
    const receipt = select(...args);
    fixture.collection.files[1].source = "Newer source from a focus handler";
    fixture.silentChange();
    return receipt;
  };
  assert.throws(() => fixture.api.openSource(checked.chapters[0].diagnostics[0]), code("STALE_SOURCE_DIAGNOSTICS"));
  assert.throws(() => fixture.api.save(), code("STALE_SOURCE_DIAGNOSTICS"));
  assert.equal(fixture.ids.get("parser-download").hidden, true);
  assert.equal(fixture.collection.files[1].source, "Newer source from a focus handler");
});

test("a notified reentrant replacement cannot bind an old report to an otherwise current receipt", async t => {
  for (const parserSource of [true, false]) {
    const fixture = controlsFixture(t, async () => ({ format: "book-diagnostics", bytes: encoded(withSource([origin()])) }));
    const checked = await fixture.api.run();
    const select = fixture.controls.selectSourceRange;
    fixture.controls.selectSourceRange = (...args) => {
      select(...args);
      fixture.change();
      return fixture.controls.checkpoint();
    };
    assert.throws(() => parserSource
      ? fixture.api.openSource(checked.chapters[0].diagnostics[0])
      : fixture.api.open("guide.md"), code("STALE_SOURCE_DIAGNOSTICS"));
    assert.throws(() => fixture.api.save(), code("STALE_SOURCE_DIAGNOSTICS"));
    assert.equal(fixture.ids.get("parser-findings").children.length, 0);
  }
});
