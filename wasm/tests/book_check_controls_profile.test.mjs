// Production workbench controllers; the DOM, source host and asynchronous
// worker endpoint are explicit doubles. Report validation remains production.
import assert from "node:assert/strict";
import test from "node:test";
import {
  createBookLinkControls, createBookSourceDiagnosticsControls,
} from "../demo/book_inspection_controls.mjs";

class Element extends EventTarget {
  constructor() { super(); this.textContent = ""; this.children = []; }
  removeAttribute(key) { delete this[key]; }
  replaceChildren(...children) { this.children = children; }
  append(...children) { this.children.push(...children); }
}

function setup(parser, initialProfile) {
  const prefix = parser ? "parser-" : "links-", elements = {};
  for (const id of ["run", "cancel", "summary", "status", "findings", "count", "previous", "next", "save", "download"])
    elements[prefix + id] = new Element();
  const root = {
    querySelector: selector => elements[selector.slice(1)] ?? null,
    createElement: () => new Element(),
  };
  const files = [
    { path: "one.md", source: "# One\n", role: "chapter" },
    { path: "part.md", source: "# Part\r\n", role: "include" },
  ];
  let profile = initialProfile, revision = 0, reads = 0, cancelled = 0;
  const listeners = new Set(), jobs = [];
  const options = { get profile() { reads++; return profile; } };
  for (const key of ["images", "fontAssets", "customCss", "typography", "title", "page"])
    Object.defineProperty(options, key, { get() { throw new Error("Unexpected resource access: " + key); } });
  const controls = {
    sourceBusy: false,
    checkpoint: () => revision,
    captureProject: () => ({ files, options }),
    subscribeSourceState: () => () => {},
  };
  const collection = {
    files,
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
  };
  const worker = {
    render(chapters, format, input) {
      let resolve;
      const promise = new Promise(done => { resolve = done; });
      jobs.push({ chapters, format, input, resolve });
      return promise;
    },
    cancel() { cancelled++; },
    dispose() {},
  };
  const create = parser ? createBookSourceDiagnosticsControls : createBookLinkControls;
  const view = create({ root, controls, collection, worker,
    urls: { revokeObjectURL() { throw new Error("No report URL was created."); } } });
  return {
    view, jobs, elements,
    get reads() { return reads; },
    get cancelled() { return cancelled; },
    changeProfile(value) {
      profile = value; revision++;
      for (const listener of listeners) listener();
    },
    complete(job = jobs.at(-1)) {
      const chapters = job.chapters.map(file => parser
        ? { path: file.path, sourceBytes: new TextEncoder().encode(file.source).length, diagnostics: [] }
        : { path: file.path, checked: 0, external: 0, unchecked: 0, findings: [] });
      const report = parser
        ? { schema: "fmd-book-source-diagnostics-v1", coordinateSpace: "expanded_chapter", chapters,
          summary: { chapters: chapters.length, diagnostics: 0 } }
        : { schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation", chapters };
      job.resolve({ format: parser ? "book-diagnostics" : "book-links",
        bytes: new TextEncoder().encode(JSON.stringify(report)) });
    },
  };
}

for (const parser of [false, true]) {
  const surface = parser ? "parser diagnostics" : "navigation";
  test("workbench " + surface + " uses the saved source dialect without reading resources", async () => {
    for (const profile of ["commonmark-gfm", "gfm-plus", undefined]) {
      const h = setup(parser, profile);
      try {
        const pending = h.view.run(), job = h.jobs[0];
        assert.equal(job.format, parser ? "diagnostics" : "links");
        assert.deepEqual(job.input, {
          ...(profile === undefined ? {} : { profile }),
          includeSources: [{ path: "part.md", source: "# Part\r\n" }],
        });
        assert.deepEqual(job.chapters, [{ path: "one.md", source: "# One\n" }]);
        assert.equal(h.reads, 1);
        h.complete();
        await pending;
      } finally { h.view.dispose(); }
    }
  });

  test("workbench " + surface + " retires a pending report when the authoring dialect changes", async () => {
    const h = setup(parser, "commonmark-gfm");
    try {
      const pending = h.view.run(), first = h.jobs[0], cancelled = h.cancelled;
      const rejected = assert.rejects(pending, {
        code: parser ? "STALE_SOURCE_DIAGNOSTICS" : "STALE_LINK_CHECK",
      });
      h.changeProfile("gfm-plus");
      assert(h.cancelled > cancelled);
      assert.equal(first.input.profile, "commonmark-gfm");
      h.complete(first);
      await rejected;
      const next = h.view.run();
      assert.equal(h.jobs[1].input.profile, "gfm-plus");
      h.complete();
      await next;
    } finally { h.view.dispose(); }
  });
}
