// Production coordinator and both verification adapters execute unchanged.
// Native rendering/report generation and the browser endpoint are explicit
// boundary doubles; these tests do not claim native rendering or conformance.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createBookPublicationClient, createBookPublicationRetainedLinkCheck,
  createBookPublicationLinkCheck } from "../book_publication.mjs";

const failure = code => Object.assign(new Error(code), { code });
const hasCode = code => error => error?.code === code;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const decode = value => JSON.parse(new TextDecoder().decode(value.bytes));
const files = () => [{ path: "one.md", source: "\ufeff# One\r\n😀" }, { path: "two.md", source: "# Two\n" }];
const options = () => ({ title: "Original", expandIncludes: true,
  includeSources: [{ path: "part.txt", source: "Included\r" }],
  images: [{ destination: "plot.svg", bytes: new Uint8Array([1, 2, 3]) }],
  fontAssets: [{ slot: "body-regular", bytes: new Uint8Array([4, 5]), weight: 650 }],
});
const sourceBytes = input => [...input.files, ...(input.options.includeSources ?? [])]
  .reduce((sum, file) => sum + new TextEncoder().encode(file.source).length, 0);
function report(paths, findings = 0) {
  return { schema: "fmd-book-link-report-v1", scope: "expanded-html-navigation",
    chapters: paths.map((path, i) => ({ path, checked: 1, external: 0, unchecked: 0,
      findings: i === 0 && findings ? [{ code: "FIXTURE", destination: "missing", message: "fixture finding" }] : [] })),
    summary: { chapters: paths.length, checked: paths.length, external: 0, unchecked: 0, findings },
  };
}
function harness(settings = {}, hooks = {}) {
  const state = { captures: 0, clients: [], calls: [], checks: 0, finals: 0, events: [] };
  const parseReport = (bytes, paths) => {
    const value = JSON.parse(new TextDecoder().decode(bytes));
    assert.deepEqual(value.chapters.map(chapter => chapter.path), paths);
    return value;
  };
  const final = createBookPublicationRetainedLinkCheck({ parseReport });
  const publisher = createBookPublicationClient({
    timeoutMs: 5000,
    ...settings,
    captureInput(fs, opts) {
      state.captures++;
      hooks.capture?.();
      return structuredClone({ files: fs, options: opts });
    },
    checkLinks: async (fs, opts, request) => {
      state.checks++;
      await hooks.check?.(state.checks, request);
      return { sourceLength: sourceBytes({ files: fs, options: opts }),
        report: report(fs.map(file => file.path), hooks.earlyFindings ?? 0) };
    },
    checkRetainedLinks: async (...args) => {
      state.finals++;
      return final(...args);
    },
    createClient(config) {
      const id = state.clients.length + 1;
      const record = { id, disposed: 0, input: null, revision: 0, config };
      state.clients.push(record);
      hooks.create?.(id);
      async function render(format, request, full) {
        state.calls.push({ id, format, full, revision: record.revision, signal: request.signal });
        assert.equal(record.disposed, 0);
        await hooks.render?.({ id, format, full, record, request });
        const value = format === "links"
          ? report(record.input.files.map(file => file.path), hooks.finalFindings ?? 0)
          : { id, format, input: record.input };
        return hooks.output?.({ id, format, record }) ?? {
          format: `book-${format}`, bytes: encode(value), sourceLength: sourceBytes(record.input),
          retainedInputRevision: ++record.revision,
          ...(format === "links" ? { mimeType: "application/json" } : {}),
        };
      }
      return {
        render(fs, format, opts, request) {
          record.input = structuredClone({ files: fs, options: opts });
          return render(format, request, true);
        },
        renderSourceUpdate(changes, format, { expectedRevision }, request) {
          assert.deepEqual(changes, []);
          assert.equal(expectedRevision, record.revision, "revision belongs to the new client, never the retired one");
          return render(format, request, false);
        },
        dispose() { record.disposed++; hooks.dispose?.(id); },
      };
    },
  });
  return { publisher, state, hooks,
    run: (request = {}, fs = files(), opts = options(), formats = ["pdf", "epub", "site"]) =>
      publisher.render(fs, formats, opts, { strictLinks: true,
        onProgress: event => state.events.push(event), ...request }),
  };
}

for (const phase of ["pdf", "epub", "site", "links"]) {
  test(`worker loss during ${phase} restarts the complete strictly verified edition`, async () => {
    const h = harness({}, { render({ id, format }) { if (id === 1 && format === phase) throw failure("WORKER_FAILED"); } });
    const result = await h.run();
    assert.deepEqual(result.outputs.map(output => decode(output).id), [2, 2, 2]);
    assert.deepEqual(h.state.calls.filter(call => call.id === 2).map(call => call.format), ["pdf", "epub", "site", "links"]);
    assert.deepEqual(h.state.calls.filter(call => call.id === 2).map(call => call.full), [true, false, false, false]);
    assert.equal(h.state.captures, 1); assert.equal(h.state.checks, 2);
    assert.equal(h.state.finals, phase === "links" ? 2 : 1);
    assert(h.state.clients.every(client => client.disposed === 1 && client.config.strictLinks));
    assert.equal(result.linkReport.summary.findings, 0);
    assert.equal(h.publisher.busy, false);
    h.publisher.dispose();
  });
}

test("a lost idle capture also restarts rather than replaying a delta into another book", async () => {
  const h = harness({}, { render({ id, format }) { if (id === 1 && format === "epub") throw failure("BOOK_CAPTURE_EXPIRED"); } });
  const result = await h.run();
  assert.deepEqual(result.outputs.map(output => decode(output).id), [2, 2, 2]);
  assert.equal(h.state.events.find(event => event.phase === "retrying").retryCode, "BOOK_CAPTURE_EXPIRED");
  h.publisher.dispose();
});

test("source-only checker worker loss retries the preflight before creating any renderer", async () => {
  const h = harness({}, { check(number) { if (number === 1) throw failure("WORKER_FAILED"); } });
  await h.run();
  assert.equal(h.state.captures, 1); assert.equal(h.state.checks, 2); assert.equal(h.state.clients.length, 1);
  assert.equal(h.state.events.find(event => event.phase === "retrying").attempt, 2);
  assert.deepEqual(h.state.events.filter(event => event.phase === "checking-links").map(event => event.attempt), [1, 2]);
  h.publisher.dispose();
});

test("recovery uses the original source, ordering, options and asset bytes without recapture", async () => {
  const h = harness({}, { render({ id }) { if (id === 1) throw failure("WORKER_FAILED"); } });
  const fs = files(), opts = options(), formats = ["epub", "pdf"], expected = structuredClone({ files: fs, options: opts });
  const result = await h.run({ onProgress(event) {
    if (event.phase === "retrying") {
      fs.reverse(); fs[0].source = "NEW EDIT"; opts.title = "New title";
      opts.images[0].bytes.fill(9); opts.fontAssets[0].bytes.fill(0);
      opts.includeSources[0].source = "New include"; formats.reverse();
    }
  } }, fs, opts, formats);
  assert.deepEqual(result.outputs.map(output => decode(output).format), ["epub", "pdf"]);
  for (const output of result.outputs) assert.deepEqual(decode(output).input, JSON.parse(JSON.stringify(expected)));
  assert.equal(h.state.captures, 1);
  assert.deepEqual(h.state.clients[1].input, expected); h.publisher.dispose();
});

test("the last worker error is returned after the finite retry budget", async () => {
  let last;
  const h = harness({}, { render() { throw last = failure("WORKER_FAILED"); } });
  await assert.rejects(h.run(), cause => cause === last);
  assert.equal(h.state.clients.length, 2); assert.equal(h.state.events.filter(event => event.phase === "retrying").length, 1);
  assert(h.state.clients.every(client => client.disposed === 1));
  assert.equal(h.publisher.busy, false); h.publisher.dispose();
});

test("retry budget zero opts out and two permits at most three attempts", async () => {
  for (const maxWorkerRetries of [0, 2]) {
    const h = harness({ maxWorkerRetries }, { render() { throw failure("WORKER_FAILED"); } });
    await assert.rejects(h.run(), hasCode("WORKER_FAILED"));
    assert.equal(h.state.clients.length, maxWorkerRetries + 1);
    assert.equal(h.state.captures, 1); h.publisher.dispose();
  }
});

for (const code of ["WORKER_PROTOCOL_ERROR", "INVALID_LINK_REPORT", "BOOK_VERIFICATION_MISMATCH", "BOOK_LINKS_FAILED",
  "INVALID_BOOK_UPDATE_REPORT", "INVALID_BOOK_SOURCE_SET_REPORT", "STALE_BOOK_CAPTURE", "STALE_BOOK_REVISION",
  "UNSUPPORTED_BOOK_PUBLICATION", "UNSUPPORTED_BOOK_OPTIONS", "OUTPUT_LIMIT", "EXPORT_TIMEOUT", "EXPORT_CANCELLED", "BOOK_BUSY", "NATIVE_FAILURE"]) {
  test(`${code} is a terminal failure, not a recovery opportunity`, async () => {
    const cause = failure(code), h = harness({ maxWorkerRetries: 2 }, { render() { throw cause; } });
    await assert.rejects(h.run(), actual => actual === cause);
    assert.equal(h.state.clients.length, 1);
    assert(!h.state.events.some(event => event.phase === "retrying")); h.publisher.dispose();
  });
}

test("strict findings discovered after recovery still prevent every download", async () => {
  const h = harness({}, { render({ id, format }) {
    if (id === 1 && format === "epub") throw failure("WORKER_FAILED");
  } });
  h.hooks.finalFindings = 1;
  await assert.rejects(h.run(), cause => cause.code === "BOOK_LINKS_FAILED" && cause.linkReport.summary.findings === 1);
  assert.equal(h.state.clients.length, 2); assert(h.state.clients.every(client => client.disposed === 1));
  h.publisher.dispose();
});

test("a changed preflight on recovery is not bypassed using the earlier clean report", async () => {
  const h = harness({}, { render({ id }) { if (id === 1) throw failure("WORKER_FAILED"); },
    check(number) { if (number === 2) h.hooks.earlyFindings = 1; } });
  await assert.rejects(h.run(), hasCode("BOOK_LINKS_FAILED"));
  assert.equal(h.state.clients.length, 1); assert.equal(h.state.checks, 2); h.publisher.dispose();
});

test("partial output accounting is reset before retry, while the combined ceiling is unchanged", async () => {
  const h = harness({ maxOutputBytes: 6 }, {
    render({ id, format }) { if (id === 1 && format === "epub") throw failure("WORKER_FAILED"); },
    output({ id, format, record }) {
      if (format !== "links") return { format: `book-${format}`, bytes: Uint8Array.of(id, 7),
        sourceLength: sourceBytes(record.input), retainedInputRevision: ++record.revision };
    },
  });
  const result = await h.run();
  assert.equal(result.byteLength, 6); assert.deepEqual(result.outputs.map(output => [...output.bytes]), [[2, 7], [2, 7], [2, 7]]);
  h.publisher.dispose();
});

test("output overflow after recovery is terminal even with retries remaining", async () => {
  const h = harness({ maxWorkerRetries: 2, maxOutputBytes: 1 }, {
    render({ id }) { if (id === 1) throw failure("WORKER_FAILED"); },
  });
  await assert.rejects(h.run(), hasCode("OUTPUT_LIMIT"));
  assert.equal(h.state.clients.length, 2); h.publisher.dispose();
});

test("malformed success after recovery is not hidden by another restart", async () => {
  const h = harness({ maxWorkerRetries: 2 }, {
    render({ id }) { if (id === 1) throw failure("WORKER_FAILED"); },
    output: () => ({ format: "book-pdf", bytes: Uint8Array.of(1), sourceLength: -1, retainedInputRevision: 1 }),
  });
  await assert.rejects(h.run(), hasCode("WORKER_PROTOCOL_ERROR"));
  assert.equal(h.state.clients.length, 2); h.publisher.dispose();
});

test("non-strict recovery never enables or impersonates native verification", async () => {
  const h = harness({}, { render({ id }) { if (id === 1) throw failure("WORKER_FAILED"); } });
  const result = await h.run({ strictLinks: false });
  assert.equal(h.state.checks, 0); assert.equal(h.state.finals, 0);
  assert.equal(result.linkReport, undefined); assert(h.state.clients.every(client => !client.config.strictLinks));
  h.publisher.dispose();
});

test("cancelling from a retry observer prevents creation of the next endpoint", async () => {
  const h = harness({}, { render() { throw failure("WORKER_FAILED"); } });
  await assert.rejects(h.run({ onProgress(event) { if (event.phase === "retrying") h.publisher.cancel(); } }), hasCode("EXPORT_CANCELLED"));
  assert.equal(h.state.clients.length, 1); assert.equal(h.state.clients[0].disposed, 1);
  delete h.hooks.render; await h.run(); assert.equal(h.state.clients.length, 2); h.publisher.dispose();
});

test("abort during backoff releases the busy slot and cannot start a late retry", async () => {
  const controller = new AbortController(), h = harness({}, { render() { throw failure("WORKER_FAILED"); } });
  await assert.rejects(h.run({ signal: controller.signal, onProgress(event) {
    if (event.phase === "retrying") setTimeout(() => controller.abort(), 5);
  } }), hasCode("EXPORT_CANCELLED"));
  assert.equal(h.publisher.busy, false); await delay(125);
  assert.equal(h.state.clients.length, 1); h.publisher.dispose();
});

test("disposal during backoff is permanent and prevents a replacement worker", async () => {
  const h = harness({}, { render() { throw failure("WORKER_FAILED"); } });
  await assert.rejects(h.run({ onProgress(event) {
    if (event.phase === "retrying") queueMicrotask(() => h.publisher.dispose());
  } }), hasCode("EXPORT_CANCELLED"));
  assert.equal(h.state.clients.length, 1); await assert.rejects(h.run(), hasCode("SESSION_DISPOSED"));
});

test("backoff consumes the original deadline instead of resetting it", async () => {
  const h = harness({ timeoutMs: 60, maxWorkerRetries: 2 }, { render() { throw failure("WORKER_FAILED"); } });
  await assert.rejects(h.run(), hasCode("EXPORT_TIMEOUT"));
  assert.equal(h.state.clients.length, 1); assert.equal(h.publisher.busy, false); h.publisher.dispose();
});

test("a deadline crossed inside a retry observer cannot start another client", async () => {
  const h = harness({ timeoutMs: 80 }, { render() { throw failure("WORKER_FAILED"); } });
  await assert.rejects(h.run({ onProgress(event) {
    if (event.phase === "retrying") { const end = performance.now() + 100; while (performance.now() < end) {} }
  } }), hasCode("EXPORT_TIMEOUT"));
  assert.equal(h.state.clients.length, 1); h.publisher.dispose();
});

for (const phase of ["checking-links", "rendering", "verifying-publication"]) {
  test(`cancelling from ${phase} progress fences the next native operation`, async () => {
    const h = harness();
    await assert.rejects(h.run({ onProgress(event) { if (event.phase === phase) h.publisher.cancel(); } }), hasCode("EXPORT_CANCELLED"));
    if (phase === "checking-links") { assert.equal(h.state.checks, 0); assert.equal(h.state.clients.length, 0); }
    if (phase === "rendering") assert.equal(h.state.calls.length, 0);
    if (phase === "verifying-publication") assert.equal(h.state.finals, 0);
    assert(h.state.clients.every(client => client.disposed === 1)); h.publisher.dispose();
  });
}

test("observers get frozen metadata, never source, resources or partial publication bytes", async () => {
  const h = harness({}, { render({ id, format }) { if (id === 1 && format === "epub") throw failure("WORKER_FAILED"); } });
  await h.run();
  for (const event of h.state.events) {
    assert(Object.isFrozen(event));
    assert(Object.keys(event).every(key => ["phase", "attempt", "maxAttempts", "completedFormats", "totalFormats", "format", "retryCode"].includes(key)));
    assert.equal(event.maxAttempts, 2); assert.equal(event.totalFormats, 3);
  }
  const events = h.state.events;
  const reset = events.findIndex(event => event.phase === "retrying");
  assert.equal(events[reset].attempt, 2); assert.equal(events[reset].completedFormats, 0);
  assert(events.slice(reset).every(event => event.attempt === 2));
  assert.deepEqual(events.filter(event => event.attempt === 2 && event.phase === "rendering").map(event => event.completedFormats), [0, 1, 2]);
  h.publisher.dispose();
});

test("throwing and rejecting observers cannot break successful publication", async () => {
  for (const onProgress of [() => { throw Error("observer failed"); }, async () => { throw Error("async observer failed"); }]) {
    const h = harness(); await h.run({ onProgress }); h.publisher.dispose();
  }
  await delay(0);
});

test("reentrant publication remains busy throughout recovery", async () => {
  const h = harness({}, { render({ id }) { if (id === 1) throw failure("WORKER_FAILED"); } }), pending = [];
  await h.run({ onProgress() { pending.push(assert.rejects(h.run(), hasCode("BOOK_BUSY"))); } });
  await Promise.all(pending); assert.equal(h.state.captures, 1); h.publisher.dispose();
});

test("invalid observer and retry settings fail before capture or worker creation", async () => {
  for (const maxWorkerRetries of [-1, 3, 0.5, NaN, Infinity, "1", null])
    assert.throws(() => harness({ maxWorkerRetries }), hasCode("INVALID_OPTIONS"));
  for (const onProgress of [null, 1, {}, "log"]) {
    const h = harness(); await assert.rejects(h.run({ onProgress }), hasCode("INVALID_OPTIONS"));
    assert.equal(h.state.captures, 0); assert.equal(h.state.clients.length, 0); h.publisher.dispose();
  }
});

test("request observer accessors are captured once before asynchronous work", async () => {
  const h = harness(); let reads = 0, calls = 0;
  await h.publisher.render(files(), ["pdf"], options(), { strictLinks: true,
    get onProgress() { reads++; return () => { calls++; }; },
  });
  assert.equal(reads, 1); assert.equal(calls, 3); h.publisher.dispose();
});

test("capture failures never retry or reread host inputs even with a worker-like code", async () => {
  const h = harness({}, { capture() { throw failure("WORKER_FAILED"); } });
  await assert.rejects(h.run(), hasCode("WORKER_FAILED"));
  assert.equal(h.state.captures, 1); assert.equal(h.state.clients.length, 0); h.publisher.dispose();
});

test("a late rejected operation from a cancelled edition cannot retry into its replacement", async () => {
  let rejectOld, began;
  const started = new Promise(resolve => { began = resolve; });
  const h = harness({}, { render({ id }) {
    if (id === 1) { began(); return new Promise((_, reject) => { rejectOld = reject; }); }
  } });
  const old = h.run(), rejected = assert.rejects(old, hasCode("EXPORT_CANCELLED"));
  await started; h.publisher.cancel(); await rejected;
  const fresh = h.run(); rejectOld(failure("WORKER_FAILED"));
  const result = await fresh;
  assert(result.outputs.every(output => decode(output).id === 2));
  assert.equal(h.state.clients.length, 2); h.publisher.dispose();
});

test("the production source-only checker adapter releases a failed checker before retry", async () => {
  let created = 0, disposed = 0, renders = 0;
  const checkLinks = createBookPublicationLinkCheck({
    textBytes: source => new TextEncoder().encode(source).length,
    parseReport: bytes => JSON.parse(new TextDecoder().decode(bytes)),
    createClient() {
      const id = ++created;
      return { render(fs, _format, opts) {
        if (id === 1) throw failure("WORKER_FAILED");
        return { format: "book-links", mimeType: "application/json", bytes: encode(report(fs.map(file => file.path))),
          sourceLength: sourceBytes({ files: fs, options: opts }) };
      }, dispose() { disposed++; } };
    },
  });
  const publisher = createBookPublicationClient({ captureInput: (fs, opts) => structuredClone({ files: fs, options: opts }),
    checkLinks, createClient() {
      assert.equal(disposed, 2, "no checker endpoint survives preflight");
      return { render(fs, format, opts) { renders++; return { format: `book-${format}`, bytes: Uint8Array.of(1),
        sourceLength: sourceBytes({ files: fs, options: opts }), retainedInputRevision: 1 }; },
        renderSourceUpdate() { throw Error("one format only"); }, dispose() {} };
    },
  });
  await publisher.render(files(), ["pdf"], options(), { strictLinks: true });
  assert.equal(created, 2); assert.equal(disposed, 2); assert.equal(renders, 1); publisher.dispose();
});

test("actual public factory wires retry opt-out, default recovery and progress to the production coordinator", () => {
  const entry = new URL("../book-worker.js", import.meta.url), module = new URL("../book_publication.mjs", import.meta.url);
  const program = `
    import vm from 'node:vm'; import fs from 'node:fs'; import assert from 'node:assert/strict';
    import * as production from ${JSON.stringify(module.href)};
    let creations=0;
    const context=vm.createContext({ URL, Worker:class{}, makeClient:()=>{
      const id=++creations;
      return {render(_files,format){if(id===1)throw Object.assign(Error('crash'),{code:'WORKER_FAILED'});
        return {format:'book-'+format,bytes:new Uint8Array([1]),sourceLength:1,retainedInputRevision:1}},
        renderSourceUpdate(){throw Error('only one format')},dispose(){}};
    },capture:(files,options)=>structuredClone({files,options})});
    const src=new vm.SourceTextModule(fs.readFileSync(${JSON.stringify(entry.pathname)},'utf8'),{context,
      initializeImportMeta(meta){meta.url=${JSON.stringify(entry.href)}}});
    const deps={
      './book_worker.mjs':new vm.SourceTextModule('export const createBookWorkerClient=globalThis.makeClient',{context}),
      './book_session.mjs':new vm.SourceTextModule('export const prepareBookInput=globalThis.capture; export const parseBookLinkReport=()=>{}; export const bookTextBytes=s=>s.length',{context}),
      './book_publication.mjs':new vm.SyntheticModule(Object.keys(production),function(){for(const [k,v]of Object.entries(production))this.setExport(k,v)},{context})
    };
    await src.link(name=>deps[name]);await src.evaluate();
    for(const retries of [0,1]){
      creations=0; const events=[];
      const pub=src.namespace.createBookPublicationWorker({workerFactory:()=>{},maxWorkerRetries:retries});
      const pending=pub.render([{path:'a.md',source:'A'}],['pdf'],{},{onProgress:e=>events.push(e)});
      if(retries===0)await assert.rejects(pending,e=>e.code==='WORKER_FAILED');else await pending;
      assert.equal(creations,retries+1);assert.equal(events.some(e=>e.phase==='retrying'),retries===1);pub.dispose();
    }
    creations=0;const defaultPublisher=src.namespace.createBookPublicationWorker({workerFactory:()=>{}});
    await defaultPublisher.render([{path:'a.md',source:'A'}],['pdf']);assert.equal(creations,2);defaultPublisher.dispose();
    assert.throws(()=>src.namespace.createBookPublicationWorker({maxWorkerRetries:3}),e=>e.code==='INVALID_OPTIONS');
  `;
  const run = spawnSync(process.execPath, ["--experimental-vm-modules", "--input-type=module", "-e", program], { encoding: "utf8", timeout: 10000 });
  assert.equal(run.status, 0, run.stderr || run.stdout);
});
