// Child-process ESM linkage check. Native transport and capture dependencies are
// explicit adapters; the public facade source and coordinator are unchanged.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { createBookPublicationClient, createBookPublicationLinkCheck, createBookPublicationRetainedLinkCheck } from "../book_publication.mjs";
import { bookTextBytes, parseBookLinkReport } from "../book_session.mjs";

const source = readFileSync(new URL("../book-worker.js", import.meta.url), "utf8");
const checks = [], configs = [], calls = [], workers = [];
let captures = 0, disposals = 0;
const check = (name, fn) => { fn(); checks.push(name); };
const context = vm.createContext({ URL, Worker: class {
  constructor(url, options) { workers.push([url, options]); }
} });
const dependencies = {
  "./book_publication.mjs": { createBookPublicationClient, createBookPublicationLinkCheck, createBookPublicationRetainedLinkCheck },
  "./book_session.mjs": { bookTextBytes, parseBookLinkReport, prepareBookInput(files, options) {
    captures++;
    return structuredClone({ files, options });
  } },
  "./book_worker.mjs": { createBookWorkerClient(options) {
    configs.push(options);
    let revision = 0, worker;
    const emit = (format) => ({ format: `book-${format}`, retainedInputRevision: ++revision,
      sourceLength: 7, bytes: Uint8Array.of(revision) });
    return {
      render(files, format, settings, request) {
        worker = options.workerFactory();
        calls.push(["render", files, format, settings, request]); return Promise.resolve(emit(format));
      },
      renderSourceUpdate(files, format, settings, request) {
        assert.equal(settings.expectedRevision, revision);
        calls.push(["delta", files, format, settings, request]); return Promise.resolve(emit(format));
      },
      dispose() { disposals++; },
    };
  } },
};
const module = new vm.SourceTextModule(source, {
  context, initializeImportMeta(meta) { meta.url = "file:///assembled/book-worker.js"; },
});
await module.link(async (specifier) => {
  const values = dependencies[specifier];
  assert.ok(values, `unexpected runtime dependency ${specifier}`);
  return new vm.SyntheticModule(Object.keys(values), function () {
    for (const [key, value] of Object.entries(values)) this.setExport(key, value);
  }, { context });
});
await module.evaluate();
check("import is lazy", () => { assert.equal(configs.length, 0); assert.equal(workers.length, 0); assert.equal(captures, 0); });
const publisher = module.namespace.createBookPublicationWorker();
check("publisher construction is lazy", () => { assert.equal(configs.length, 0); assert.equal(workers.length, 0); });
const input = [{ path: "guide.md", source: "# Guide" }];
const output = await publisher.render(input);
check("public default publishes all formats", () => assert.deepEqual(output.outputs.map(item => item.format), ["book-pdf", "book-epub", "book-site"]));
check("shared capture normalizer is wired", () => assert.equal(captures, 1));
check("exactly one private retained client", () => { assert.equal(configs.length, 1); assert.equal(configs[0].retainBook, true); });
check("existing module worker entry is used", () => {
  assert.equal(workers.length, 1); assert.equal(workers[0][0].href, "file:///assembled/book_worker_entry.js");
  assert.equal(workers[0][1].type, "module");
});
check("later formats send only source deltas", () => {
  assert.deepEqual(calls.map(call => call[0]), ["render", "delta", "delta"]);
  assert.ok(calls.slice(1).every(call => call[1].length === 0 && Object.keys(call[3]).join() === "expectedRevision"));
});
check("private worker released before delivery", () => assert.equal(disposals, 1));
publisher.dispose();
check("disposed publisher does not double release", () => assert.equal(disposals, 1));
const customFactory = () => ({ custom: true });
const settings = { timeoutMs: 321, maxOutputBytes: 9, workerFactory: customFactory };
const configured = module.namespace.createBookPublicationWorker(settings);
settings.timeoutMs = 1; settings.maxOutputBytes = 1; settings.workerFactory = () => { throw new Error("late factory read"); };
await configured.render(input, ["pdf"]);
check("factory configuration is captured once", () => {
  assert.equal(configs[1].timeoutMs, 321); assert.equal(configs[1].maxOutputBytes, 9);
  assert.equal(configs[1].workerFactory, customFactory); assert.equal(configs[1].idleTimeoutMs, 600000);
});
configured.dispose();
const legacy = module.namespace.createBookWorker({ retainBook: false, retainPreview: true, workerFactory: customFactory });
check("single-output factory keeps its original retention options", () => {
  assert.equal(configs[2].retainBook, false); assert.equal(configs[2].retainPreview, true);
  assert.equal(configs[2].workerFactory, customFactory);
});
legacy.dispose();
check("invalid publication configuration fails before worker creation", () => {
  const before = configs.length;
  for (const options of [{ workerFactory: 1 }, { timeoutMs: 0 }, { maxOutputBytes: Infinity }])
    assert.throws(() => module.namespace.createBookPublicationWorker(options), error => error.code === "INVALID_OPTIONS");
  assert.equal(configs.length, before);
});
console.log(JSON.stringify({ checks }));
