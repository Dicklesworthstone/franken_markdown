// Current generated WASM + unchanged production facade and real worker entry.
// No renderer doubles or full browser/book package are required.
// node wasm/analysis_profile_smoke.mjs <packageDir> <generated.wasm>
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (!isMainThread) {
  const listeners = new Set(), pending = [];
  parentPort.on("message", data => {
    if (!listeners.size) pending.push(data);
    else for (const listener of listeners) listener({ data });
  });
  globalThis.addEventListener = (kind, listener) => {
    assert.equal(kind, "message");
    listeners.add(listener);
    for (const data of pending.splice(0)) queueMicrotask(() => listener({ data }));
  };
  globalThis.removeEventListener = (_kind, listener) => listeners.delete(listener);
  globalThis.postMessage = (data, transfer) => parentPort.postMessage(data, transfer);
  const core = await import(pathToFileURL(resolve(workerData.packageDir, "franken_markdown.js")));
  await core.init(new Uint8Array(await readFile(workerData.wasmPath)));
  await import(pathToFileURL(resolve(workerData.packageDir, "document_worker_entry.js")));
} else {
  const [packageArg, wasmArg, ...extra] = process.argv.slice(2);
  if (!packageArg || !wasmArg || extra.length)
    throw new Error("usage: analysis_profile_smoke.mjs <packageDir> <generated.wasm>");
  const packageDir = resolve(packageArg), wasmPath = resolve(wasmArg);
  const core = await import(pathToFileURL(resolve(packageDir, "franken_markdown.js")));
  const bindings = await import(pathToFileURL(resolve(packageDir, "pkg/franken_markdown.js")));
  const direct = await core.createRenderer(new Uint8Array(await readFile(wasmPath)));
  const source = "\ufeff## Shared\r\n\r\n*Term\r\n: café*\r\n";
  const before = "[Term\n: definition](/old)\n", after = "[Term\n: definition](/new)\n";
  const names = { oldName: "  Before <Ω>  ", newName: "After <&>" };

  // Legacy outputs remain exact, including the absence of profile metadata.
  assert.deepEqual(await direct.searchIndex(source), JSON.parse(bindings.searchIndex(source)));
  assert.equal(bindings.searchIndexProfiled(source, undefined), bindings.searchIndex(source));
  assert.deepEqual(await direct.semanticDiff(before, after, names),
    JSON.parse(bindings.semanticDiff(before, after, names.oldName, names.newName)));
  assert.equal(bindings.semanticDiffProfiled(before, after, names.oldName, names.newName, undefined),
    bindings.semanticDiff(before, after, names.oldName, names.newName));
  const legacyHtml = await direct.renderSemanticDiff(before, after, names);
  const legacyNative = bindings.renderSemanticDiffHtml(before, after, names.oldName, names.newName);
  try { assert.deepEqual(legacyHtml.bytes, legacyNative.bytes); }
  finally { legacyNative.free(); }
  assert.equal(Object.hasOwn(legacyHtml, "profile"), false);
  console.log("analysis profiles: legacy search and comparison ABI/output parity PASS");

  const commonmark = await direct.searchIndex(source, { profile: "commonmark-gfm" });
  const plus = await direct.searchIndex(source, { profile: "gfm-plus" });
  assert.equal(commonmark.profile, "commonmark-gfm"); assert.equal(plus.profile, "gfm-plus");
  assert.ok(commonmark.entries.some(entry => entry.anchor === "shared" && entry.kind === "heading"));
  assert.ok(commonmark.entries.some(entry => entry.text === "Term : café"));
  assert.ok(!plus.entries.some(entry => entry.text.includes("café")));
  const published = await direct.renderHtml(source, { profile: "commonmark-gfm" });
  assert.ok(published.text().includes('id="shared"'));
  assert.ok(published.text().includes("<em>Term\n: café</em>"));
  console.log("analysis profiles: selected source grammar matches indexed text and publication anchors PASS");

  class Endpoint extends EventTarget {
    constructor() {
      super();
      this.thread = new Worker(new URL(import.meta.url), { workerData: { packageDir, wasmPath } });
      this.thread.on("message", data => this.dispatchEvent(new MessageEvent("message", { data })));
      this.thread.on("error", error => { console.error(error); this.dispatchEvent(new Event("error")); });
      this.thread.on("messageerror", () => this.dispatchEvent(new Event("messageerror")));
    }
    postMessage(data, transfer) { this.thread.postMessage(data, transfer); }
    terminate() { return this.thread.terminate(); }
  }
  const { createWorkerRenderer } = await import(pathToFileURL(resolve(packageDir, "document_worker.mjs")));
  const worker = createWorkerRenderer({ workerFactory: () => new Endpoint(), timeoutMs: 60000 });
  try {
    for (const profile of ["commonmark-gfm", "gfm-plus"]) {
      const options = { ...names, profile };
      const report = await direct.semanticDiff(before, after, options);
      const html = await direct.renderSemanticDiff(before, after, options);
      assert.equal(report.profile, profile); assert.equal(html.profile, profile);
      assert.equal(report.old_name, names.oldName); assert.equal(report.new_name, names.newName);
      assert.ok(html.text().includes("&lt;Ω&gt;")); assert.ok(html.text().includes("After &lt;&amp;&gt;"));
      if (profile === "commonmark-gfm") {
        assert.equal(report.stats.modified_blocks, 1);
        assert.ok(html.text().includes('href="/old"')); assert.ok(html.text().includes('href="/new"'));
        assert.ok(!html.text().includes("<dl"));
      } else {
        assert.ok(report.stats.inserted_blocks > 0 && report.stats.deleted_blocks > 0);
        assert.ok(html.text().includes("<dl"));
      }
      const actual = await worker.compare(before, after, options);
      assert.equal(actual.profile, profile); assert.equal(actual.html.profile, profile);
      assert.deepEqual(actual.report, report); assert.deepEqual(JSON.parse(actual.json.text()), report);
      for (const key of ["bytes", "diagnostics", "format", "mimeType", "extension", "sourceLength"])
        assert.deepEqual(actual.html[key], html[key], `${profile}: visual review ${key}`);
      assert.equal(actual.oldSourceLength, Buffer.byteLength(before));
      assert.equal(actual.newSourceLength, Buffer.byteLength(after));
      const repeat = await worker.compare(before, after, options);
      assert.deepEqual(repeat.html.bytes, actual.html.bytes); assert.deepEqual(repeat.json.bytes, actual.json.bytes);
      console.log(`analysis profiles: ${profile} real-worker JSON/HTML grammar, parity and determinism PASS`);
    }
    const oldBreak = "Before.\n\n<!-- pagebreak -->\n\nAfter.\n";
    const newBreak = "Before.\n\n<!-- pagebreak --> \t\n\nAfter.\n";
    const transformed = await worker.compare(oldBreak, newBreak, { profile: "gfm-plus" });
    assert.equal(transformed.report.stats.unchanged_blocks, 3);
    for (const key of ["modified_blocks", "inserted_blocks", "deleted_blocks"])
      assert.equal(transformed.report.stats[key], 0);
    const sourceBreak = await worker.compare(oldBreak, newBreak, { profile: "commonmark-gfm" });
    assert.ok(sourceBreak.report.stats.modified_blocks + sourceBreak.report.stats.inserted_blocks
      + sourceBreak.report.stats.deleted_blocks > 0);
    const options = { ...names, profile: "commonmark-gfm" };
    const pending = direct.renderSemanticDiff(before, after, options);
    options.profile = "gfm-plus"; options.oldName = "Mutated";
    const owned = await pending;
    assert.equal(owned.profile, "commonmark-gfm"); assert.ok(owned.text().includes('href="/new"'));
    assert.ok(owned.text().includes("&lt;Ω&gt;"));
    await assert.rejects(direct.searchIndex(source, { profile: "gfm" }), { code: "INVALID_OPTIONS" });
    await assert.rejects(direct.semanticDiff(before, after, { profile: "gfm" }), { code: "INVALID_OPTIONS" });
    await assert.rejects(direct.renderSemanticDiff(before, after, { profile: "gfm" }), { code: "INVALID_OPTIONS" });
    const legacy = await worker.compare(before, after, names);
    assert.equal(Object.hasOwn(legacy, "profile"), false);
    assert.equal(Object.hasOwn(legacy.report, "profile"), false);
    assert.equal(Object.hasOwn(legacy.html, "profile"), false);
    assert.deepEqual(legacy.html.bytes, legacyHtml.bytes);
    console.log("analysis profiles: publication transforms, warm ownership, invalid grammar and legacy worker preservation PASS");
  } finally { worker.dispose(); }
}
