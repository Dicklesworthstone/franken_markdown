// Production adapters and worker transport with explicit native/DOM doubles.
// Rust parser behavior is exercised separately by the book integration tests.
import assert from "node:assert/strict";
import test from "node:test";
import { createBookBindings, parseBookSourceDiagnosticsReport } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createBookSourceDiagnosticsControls, createBookSourceDiagnosticsPanel } from "../demo/book_inspection_controls.mjs";

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

function controlsFixture(t, render) {
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
    files: [...files.map(file => ({ ...file, role: "chapter" })),
      ...resources.map(file => ({ ...file, role: "include" }))],
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
  };
  const controls = {
    sourceBusy: false,
    checkpoint: () => stamp,
    captureProject: () => ({ files: collection.files.map(file => ({ ...file })) }),
    selectSourceRange(...args) { selected.push(args); stamp++; },
    subscribeSourceState: () => () => {},
  };
  const worker = { render, cancel() {}, dispose() {} };
  const api = createBookSourceDiagnosticsControls({
    root, controls, collection, worker,
    urls: { createObjectURL: () => "blob:parser", revokeObjectURL: url => revoked.push(url) },
  });
  t.after(() => api.dispose());
  return { api, ids, selected, revoked, change() { stamp++; for (const listener of listeners) listener(); } };
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
