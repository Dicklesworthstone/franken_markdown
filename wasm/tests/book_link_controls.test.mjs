// Real preflight controller, panel and report reader. The editor, DOM and worker
// endpoints are explicit doubles; no generated WASM or browser execution claim.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { inspectBook } from "../book_inspection.mjs";
import {
  createBookInspectionControls,
  createBookInspectionPanel,
  createBookLinkControls,
  createBookLinkPanel,
} from "../demo/book_inspection_controls.mjs";

const gate = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
class Element extends EventTarget {
  children = [];
  attributes = {};
  textContent = "";
  disabled = false;
  hidden = false;
  value = "";
  constructor(tag, registry) {
    super();
    this.tagName = tag;
    this.registry = registry;
  }
  setAttribute(key, value) {
    this.attributes[key] = value;
    if (key === "id") this.registry.set(value, this);
    if (key === "disabled" || key === "hidden") this[key] = true;
  }
  removeAttribute(key) {
    delete this.attributes[key];
    delete this[key];
  }
  append(...nodes) {
    this.children.push(...nodes);
  }
  replaceChildren(...nodes) {
    this.children = nodes;
  }
  before(node) {
    this.beforeNode = node;
  }
  click() {
    const event = new Event("click", { cancelable: true });
    this.dispatchEvent(event);
    return event;
  }
}
const source = () => [
  { path: "start.md", source: "# Start\n\n{{#include parts.md}}" },
  { path: "parts.md", source: "[shared](end.md#missing)", role: "include" },
  { path: "end.md", source: "# End" },
];
function setup(t, initial = source()) {
  const registry = new Map(),
    body = new Element("body", registry),
    publish = new Element("section", registry);
  const root = {
    body,
    createElement: (tag) => new Element(tag, registry),
    createTextNode: (text) => ({ textContent: text }),
    querySelector: (selector) =>
      selector.startsWith("#")
        ? registry.get(selector.slice(1))
        : selector === '[aria-labelledby="publish-title"]'
          ? publish
          : null,
  };
  for (const id of [
    "source-role",
    "chapter-source",
    "chapter-path",
    "chapters",
    "title",
    "author",
    "lang",
    "font",
    "dark-mode",
    "font-scale",
    "toc",
    "page-numbers",
  ]) {
    root.createElement("input").setAttribute("id", id);
  }
  const panel = createBookLinkPanel(root),
    listeners = new Set(),
    stateListeners = new Set(),
    selections = [];
  let files = structuredClone(initial),
    revision = 0,
    view = 0,
    busy = false,
    cancelled = 0,
    disposed = 0;
  const collection = {
    get files() {
      return structuredClone(files);
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    get images() {
      throw new Error("preflight must not read image authority");
    },
    snapshot() {
      throw new Error("preflight must not clone a publishing snapshot");
    },
  };
  const controls = {
    checkpoint: () => `${revision}:${view}`,
    get sourceBusy() {
      return busy;
    },
    captureProject: () => ({
      schemaVersion: 2,
      files: structuredClone(files),
      options: {
        get images() {
          throw new Error("no assets");
        },
      },
    }),
    subscribeSourceState(fn) {
      stateListeners.add(fn);
      return () => stateListeners.delete(fn);
    },
    selectSourceRange(index, start, end, checkpoint) {
      assert.equal(checkpoint, this.checkpoint());
      selections.push({ index, start, end });
      view++;
    },
  };
  const jobs = [],
    worker = {
      render(files, format, options) {
        const job = gate();
        jobs.push({ ...job, files, format, options });
        return job.promise;
      },
      cancel() {
        cancelled++;
      },
      dispose() {
        disposed++;
      },
    };
  const api = createBookLinkControls({ root, controls, collection, worker });
  t.after(() => api.dispose());
  return {
    api,
    root,
    collection,
    controls,
    jobs,
    selections,
    panel,
    publish,
    el: (id) => registry.get(id),
    get cancelled() {
      return cancelled;
    },
    get disposed() {
      return disposed;
    },
    get listenerCount() {
      return listeners.size + stateListeners.size;
    },
    edit(index = 1, source = "changed snippet") {
      files[index].source = source;
      revision++;
      for (const fn of listeners) fn();
    },
    programmatic() {
      view++;
    },
    setBusy(value) {
      busy = value;
      for (const fn of stateListeners) fn();
    },
  };
}
function response(paths = ["start.md", "end.md"], count = 1) {
  return {
    format: "book-links",
    sourceLength: 10,
    bytes: encode({
      schema: "fmd-book-link-report-v1",
      scope: "expanded-html-navigation",
      chapters: paths.map((path, i) => ({
        path,
        checked: i ? count : 0,
        external: i ? 3 : 0,
        unchecked: i ? 2 : 0,
        findings: i
          ? Array.from({ length: count }, (_, n) => ({
              code: "missing_anchor",
              destination: `#absent-${n}`,
              message: "Missing target.",
            }))
          : [],
      })),
      summary: { findings: 0, checked: 0, allPassed: true },
      unknown: { source: "must not retain extra fields" },
    }),
  };
}
async function complete(f, result = response()) {
  const pending = f.api.run();
  f.jobs.at(-1).resolve(result);
  return pending;
}

test("panel is connected to explicit chapter/include preflight without loading assets", async (t) => {
  const f = setup(t);
  assert.equal(f.publish.beforeNode, f.panel);
  assert.equal(f.jobs.length, 0);
  const before = f.collection.files,
    pending = f.api.run(),
    job = f.jobs[0];
  assert.equal(job.format, "links");
  assert.deepEqual(
    job.files.map((file) => file.path),
    ["start.md", "end.md"],
  );
  assert.deepEqual(job.options, {
    includeSources: [{ path: "parts.md", source: source()[1].source }],
  });
  assert.equal(f.el("links-run").disabled, true);
  assert.equal(f.el("links-cancel").disabled, false);
  job.resolve(response());
  const report = await pending;
  assert.equal(report.summary.findings, 1);
  assert.deepEqual(f.collection.files, before);
  assert.equal(f.el("links-findings").children.length, 1);
  assert.equal(f.el("links-save").disabled, false);
});
test("zero findings never implies that external URLs, downloads or final formats were verified", async (t) => {
  const f = setup(t);
  await complete(f, response(undefined, 0));
  assert.match(f.el("links-summary").textContent, /0 findings/);
  assert.match(
    f.el("links-summary").textContent,
    /3 external URLs and 2 other local resources were NOT verified/,
  );
  assert.match(f.el("links-summary").textContent, /PDF\/EPUB conformance are outside/);
});
test("findings paginate deterministically without losing late results", async (t) => {
  const f = setup(t);
  await complete(f, response(undefined, 105));
  assert.equal(f.el("links-findings").children.length, 50);
  f.el("links-next").click();
  assert.match(f.el("links-count").textContent, /51–100 of 105/);
  f.el("links-next").click();
  assert.equal(f.el("links-findings").children.length, 5);
  assert.equal(f.el("links-next").disabled, true);
  assert.equal(f.el("links-findings").children[4].children[1].textContent, "#absent-104");
  f.el("links-previous").click();
  assert.equal(f.el("links-findings").children.length, 50);
});
test("hostile-looking destinations stay inert and navigation uses the mixed editor source identity", async (t) => {
  const f = setup(t),
    result = response(),
    value = JSON.parse(new TextDecoder().decode(result.bytes));
  value.chapters[1].findings[0].destination = "javascript:alert(1)</pre><img onerror=bad>";
  result.bytes = encode(value);
  await complete(f, result);
  const row = f.el("links-findings").children[0];
  assert.equal(row.children[1].tagName, "pre");
  assert.equal(row.children[1].textContent, value.chapters[1].findings[0].destination);
  assert.equal(row.children[1].href, undefined);
  row.children[2].click();
  assert.deepEqual(f.selections, [{ index: 2, start: 0, end: 0 }]);
  assert.match(f.el("links-status").textContent, /No exact original-source span/);
  f.api.save();
  assert.equal(f.el("links-download").hidden, false);
});
test("download recomputes totals, excludes unknown fields, and requires explicit activation", async (t) => {
  const f = setup(t);
  await complete(f);
  assert.equal(f.el("links-download").hidden, true);
  f.el("links-save").click();
  const href = f.el("links-download").href;
  const report = await (await fetch(href)).json();
  assert.equal(report.summary.findings, 1);
  assert.equal(report.unknown, undefined);
  assert.equal(report.summary.allPassed, undefined);
  assert.equal(f.el("links-download").download, "book-links.json");
  assert.equal(f.el("links-download").click().defaultPrevented, false);
  assert.match(f.el("links-status").textContent, /sensitive/);
  f.edit();
  assert.equal(f.el("links-download").hidden, true);
  await assert.rejects(fetch(href));
});
test("programmatic changes without input events refuse stale navigation and downloads", async (t) => {
  const f = setup(t);
  await complete(f);
  f.api.save();
  const href = f.el("links-download").href;
  f.programmatic();
  assert.equal(f.el("links-download").click().defaultPrevented, true);
  await assert.rejects(fetch(href));
  assert.throws(() => f.api.open("end.md"), { code: "STALE_LINK_CHECK" });
  assert.equal(f.selections.length, 0);
});
test("late results cannot overwrite a newer check after an include edit", async (t) => {
  const f = setup(t),
    old = f.api.run(),
    rejected = assert.rejects(old, { code: "STALE_LINK_CHECK" });
  f.edit();
  await complete(f, response(undefined, 0));
  const summary = f.el("links-summary").textContent;
  f.jobs[0].resolve(response());
  await rejected;
  assert.equal(f.el("links-summary").textContent, summary);
  assert.equal(f.el("links-findings").children.length, 0);
});
test("cancellation clears only preflight and allows a fresh explicit retry", async (t) => {
  const f = setup(t),
    pending = f.api.run(),
    rejected = assert.rejects(pending, { code: "STALE_LINK_CHECK" });
  f.el("links-cancel").click();
  assert.equal(f.el("links-findings").children.length, 0);
  assert(f.cancelled > 0);
  f.jobs[0].resolve(response());
  await rejected;
  assert.match(f.el("links-status").textContent, /cancelled and cleared/);
  await complete(f);
  assert.equal(f.el("links-findings").children.length, 1);
});
test("failed expansion and mismatched reports replace old results with visible failure", async (t) => {
  const f = setup(t);
  await complete(f);
  f.api.save();
  const href = f.el("links-download").href;
  const pending = f.api.run(),
    rejected = assert.rejects(pending, /include_cycle/);
  f.jobs.at(-1).reject(new Error("include_cycle: parts.md -> parts.md"));
  await rejected;
  assert.match(f.el("links-status").textContent, /No clean result was issued/);
  assert.equal(f.el("links-save").disabled, true);
  await assert.rejects(fetch(href));
  for (const result of [
    response(["wrong.md", "end.md"]),
    { ...response(), format: "book-inspection" },
  ]) {
    await assert.rejects(complete(f, result), { code: "INVALID_LINK_REPORT" });
    assert.equal(f.el("links-findings").children.length, 0);
  }
});
test("import and composition state blocks or retires checks without mutating source", async (t) => {
  const f = setup(t);
  await complete(f);
  const before = f.collection.files;
  f.setBusy(true);
  assert.equal(f.el("links-run").disabled, true);
  assert.equal(f.el("links-save").disabled, true);
  await assert.rejects(f.api.run(), { code: "BOOK_BUSY" });
  assert.deepEqual(f.collection.files, before);
  f.setBusy(false);
  assert.equal(f.el("links-run").disabled, false);
  assert.equal(f.el("links-save").disabled, true);
});
test("include-only drafts do not masquerade as successfully checked books", async (t) => {
  const f = setup(t, [source()[1]]);
  assert.equal(f.el("links-run").disabled, true);
  await assert.rejects(f.api.run(), { code: "EMPTY_BOOK" });
  assert.equal(f.jobs.length, 0);
});
test("suspension revokes reports and resume does not automatically restart work", async (t) => {
  const f = setup(t);
  await complete(f);
  f.api.save();
  const href = f.el("links-download").href;
  f.api.suspend();
  await assert.rejects(fetch(href));
  await assert.rejects(f.api.run(), { code: "LINK_CHECK_CLOSED" });
  const count = f.jobs.length;
  f.api.resume();
  assert.equal(f.jobs.length, count);
  assert.equal(f.el("links-save").disabled, true);
  await complete(f);
  assert.equal(f.el("links-save").disabled, false);
});
test("disposal fences late work and releases subscriptions and the dedicated worker", async (t) => {
  const f = setup(t),
    pending = f.api.run(),
    rejected = assert.rejects(pending, { code: "STALE_LINK_CHECK" });
  f.api.dispose();
  f.api.dispose();
  f.jobs[0].resolve(response());
  await rejected;
  assert.equal(f.disposed, 1);
  assert.equal(f.listenerCount, 0);
  assert.equal(f.el("links-findings").children.length, 0);
  f.el("links-run").click();
  assert.equal(f.jobs.length, 1);
});
test("retained detached buttons cannot navigate a different report", async (t) => {
  const f = setup(t);
  await complete(f);
  const button = f.el("links-findings").children[0].children[2];
  await complete(f);
  button.click();
  assert.equal(f.selections.length, 0);
  assert.match(f.el("links-status").textContent, /older check/);
});
test("source inspection still runs independently and survives preflight cancellation", async (t) => {
  const f = setup(t);
  createBookInspectionPanel(f.root);
  let cancelled = 0;
  const engine = {
    documentStats() {
      throw new Error("explicit stats double");
    },
    accessibilityAudit() {
      throw new Error("explicit audit double");
    },
  };
  const worker = {
    async render(files) {
      return { ...(await inspectBook(engine, files)), format: "book-inspection" };
    },
    cancel() {
      cancelled++;
    },
    dispose() {},
  };
  const inspection = createBookInspectionControls({
    root: f.root,
    controls: f.controls,
    collection: f.collection,
    worker,
  });
  t.after(() => inspection.dispose());
  const report = await inspection.run();
  assert.equal(report.summary.totalChapters, 2);
  assert.equal(report.summary.verdict, "incomplete");
  const previous = cancelled;
  await complete(f);
  f.el("links-cancel").click();
  assert.equal(cancelled, previous);
  assert.equal(f.el("inspection-save").disabled, false);
  assert.equal(f.el("inspection-chapters").children.length, 2);
});
test("production entry installs a dedicated bounded link worker and its lifecycle hooks", () => {
  // Static wiring proof, not a browser/WASM execution claim.
  const entry = readFileSync(new URL("../demo/book.js", import.meta.url), "utf8");
  assert.match(entry, /createBookLinkPanel\(document\)/);
  assert.match(entry, /createBookLinkControls\([\s\S]*?maxOutputBytes: 4 \* 1024 \* 1024/);
  for (const method of ["suspend", "resume", "dispose"])
    assert(entry.includes(`links?.${method}()`));
});
