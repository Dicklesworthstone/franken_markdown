import assert from "node:assert/strict";
import test from "node:test";
import { createBookCollectionCapture } from "../book_worker.mjs";
import { createBookPdfProof } from "../book_pdf_proof.mjs";
import { publisher, sourceHost, transport } from "./book_publishing_delta_helpers.mjs";
const tick = () => new Promise(resolve => setImmediate(resolve));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const decode = result => JSON.parse(new TextDecoder().decode(result.bytes).replace(/^%PDF-1.7\n/, ""));
const code = expected => error => error?.code === expected;
function rig(t, options) {
  const host = sourceHost(), wire = transport(t, options);
  const capture = createBookCollectionCapture(host.collection, wire.worker);
  return { ...host, ...wire, capture, run: (format = "site") => capture().render(format) };
}
function sources(f) {
  return { files: f.state.files.filter(file => file.role !== "include"),
    options: { expectedRevision: f.worker.retainedInputRevision,
      includeSources: f.state.files.filter(file => file.role === "include").map(({ path, source }) => ({ path, source })) } };
}

test("complete source capture sends only changed chapters/includes, with unchanged files omitted", async t => {
  const f = rig(t); await f.run();
  f.collection.edit(0, "first.md", "Edited root"); f.collection.edit(2, "shared.txt", "Edited include");
  const next = sources(f);
  const result = await f.worker.renderSources(next.files, "epub", next.options);
  assert.equal(decode(result).files[0].source, "Edited root");
  assert.equal(decode(result).includes[0].source, "Edited include");
  const packet = f.endpoints[0].messages[1];
  assert.deepEqual(packet.data.sourceUpdate.files, [{ path: "first.md", source: "Edited root" }, { path: "shared.txt", source: "Edited include" }]);
  assert.equal(packet.bytes, 0); assert(!Object.hasOwn(packet.data, "options"));
  assert.equal(f.endpoints[0].native.stats.updates, 1);
});

test("manual deltas and complete source captures share the exact successful baseline", async t => {
  const f = rig(t); await f.run();
  await f.worker.renderSourceUpdate([{ path: "first.md", source: "Manual" }], "site", { expectedRevision: f.worker.retainedInputRevision });
  f.collection.edit(0, "first.md", "Manual");
  let next = sources(f); await f.worker.renderSources(next.files, "pdf", next.options);
  assert.deepEqual(f.endpoints[0].messages[2].data.sourceUpdate.files, []);
  f.collection.edit(0, "first.md", "# First");
  next = sources(f); await f.worker.renderSources(next.files, "epub", next.options);
  assert.deepEqual(f.endpoints[0].messages[3].data.sourceUpdate.files, [{ path: "first.md", source: "# First" }]);
});

test("full renders replace the delta baseline along with source order and resource grants", async t => {
  const f = rig(t); await f.run();
  f.collection.move(0, 1); f.collection.revokeImages();
  const full = f.collection.snapshot(); await f.worker.render(full.files, "site", full.options);
  let next = sources(f); await f.worker.renderSources(next.files, "epub", next.options);
  assert.deepEqual(f.endpoints[0].messages[2].data.sourceUpdate.files, []);
  f.collection.edit(0, "second.md", "New first chapter"); next = sources(f);
  const result = decode(await f.worker.renderSources(next.files, "pdf", next.options));
  assert.equal(result.files[0].path, "second.md"); assert.equal(result.files[0].source, "New first chapter");
  assert.deepEqual(result.images, []);
});

test("source-only capture rejects membership, order and role changes without posting or losing the idle book", async t => {
  const f = rig(t); await f.run();
  const next = sources(f);
  for (const files of [[...next.files].reverse(), [next.files[0]], [...next.files, { path: "added.md", source: "x" }],
    [next.files[0], { path: "renamed.md", source: next.files[1].source }]]) {
    await assert.rejects(f.worker.renderSources(files, "pdf", next.options), code("BOOK_SOURCE_SET_CHANGED"));
  }
  await assert.rejects(f.worker.renderSources(next.files, "pdf", { expectedRevision: 1 }), code("BOOK_SOURCE_SET_CHANGED"));
  assert.equal(f.endpoints[0].messages.length, 1); assert.equal(f.worker.retainedInputRevision, 1);
  await f.worker.renderSources(next.files, "site", next.options);
});

test("renderSources never accepts presentation/resources or invokes option accessors", async t => {
  const f = rig(t); await f.run(); const next = sources(f); let accessed = false;
  for (const options of [{ ...next.options, images: [] }, { ...next.options, title: "bad" },
    { expectedRevision: 1, get includeSources() { accessed = true; return []; } }]) {
    await assert.rejects(f.worker.renderSources(next.files, "pdf", options), code("INVALID_BOOK_SOURCE_DELTA"));
  }
  assert.equal(accessed, false); assert.equal(f.endpoints[0].messages.length, 1);
});

test("a postMessage adapter cannot mutate the privately captured source baseline", async t => {
  const f = rig(t, { mutatePosted(value) { if (value.files) value.files[0].source = "Adapter mutation after cloning"; } });
  await f.run(); const next = sources(f);
  const value = decode(await f.worker.renderSources(next.files, "pdf", next.options));
  assert.equal(value.files[0].source, "# First");
  assert.deepEqual(f.endpoints[0].messages[1].data.sourceUpdate.files, []);
});

test("collection capture avoids full snapshot and asset access after compatible success", async t => {
  const f = rig(t); f.state.images[0].bytes = new Uint8Array(2 * 1024 * 1024).fill(3);
  await f.run(); assert.equal(f.state.snapshots, 1);
  f.collection.snapshot = () => { throw Error("Full resource snapshot must not run on source-only capture"); };
  Object.defineProperty(f.state, "fonts", { get() { throw Error("Do not read the font store"); } });
  for (const format of ["epub", "pdf", "site"]) {
    f.collection.edit(0, "first.md", format);
    const value = decode(await f.run(format)); assert.equal(value.files[0].source, format);
    assert.equal(value.images[0].size, 2 * 1024 * 1024); assert.equal(value.fonts[0].size, 3);
  }
  assert(f.endpoints[0].messages.slice(1).every(packet => packet.bytes === 0));
  assert.equal(f.endpoints[0].native.stats.creates, 1);
});

test("configuration, asset and structural changes require a new full capture", async t => {
  const f = rig(t); await f.run();
  for (const change of [() => f.collection.configure({ ...f.state.options, title: "Changed" }),
    () => f.collection.revokeImages(), () => f.collection.move(0, 1), () => f.collection.revokeFonts()]) {
    const before = f.state.snapshots; change();
    await f.run(); assert.equal(f.state.snapshots, before + 1);
    assert(Object.hasOwn(f.endpoints[0].messages.at(-1).data, "options"));
  }
  const result = decode(await f.run()); assert.equal(result.settings.metadata[0], "Changed");
  assert.deepEqual(result.images, []); assert.deepEqual(result.fonts, []);
});

test("externally reused workers cannot authorize a collection's stale profile", async t => {
  const f = rig(t); await f.run();
  const input = f.collection.snapshot();
  await f.worker.render(input.files, "site", { ...input.options, title: "Another owner" });
  const before = f.state.snapshots;
  assert.equal(decode(await f.run()).settings.metadata[0], "Manual");
  assert.equal(f.state.snapshots, before + 1);
});

test("expiry and explicit release fall back before capture, but never silently retry a submitted delta", async t => {
  const f = rig(t, { idleTimeoutMs: 30 }); await f.run();
  await sleep(70); assert.equal(f.worker.retainedInputRevision, null);
  await f.run(); assert.equal(f.state.snapshots, 2);
  const task = f.capture(); f.worker.cancel();
  await assert.rejects(task.render("pdf"), code("BOOK_CAPTURE_EXPIRED"));
  assert.equal(f.state.snapshots, 2); assert.equal(f.endpoints.length, 2);
  await f.run(); assert.equal(f.state.snapshots, 3); assert.equal(f.endpoints.length, 3);
});

test("legacy configuration and worker adapters continue to use full snapshots", async t => {
  for (const mode of ["configuration", "method", "acknowledgement", "project"]) {
    const f = rig(t, mode === "acknowledgement" ? { acknowledge: false } : {});
    let worker = f.worker;
    if (mode === "configuration") Object.defineProperty(f.collection, "renderConfigurationRevision", { get: () => undefined });
    if (mode === "project") f.collection.project = undefined;
    if (mode === "method") worker = { render: f.worker.render, get retainedInputRevision() { return f.worker.retainedInputRevision; } };
    const capture = createBookCollectionCapture(f.collection, worker);
    await capture().render("site"); await capture().render("epub");
    assert.equal(f.state.snapshots, 2);
  }
});

test("capture rejects source changes before submission and task reuse", async t => {
  const f = rig(t), task = f.capture(); f.collection.edit(0, "first.md", "Changed");
  await assert.rejects(task.render("site"), code("STALE_SOURCE")); assert.equal(f.endpoints.length, 0);
  await assert.rejects(task.render("site"), code("BOOK_CAPTURE_USED"));
  await f.run();
  const project = f.collection.project;
  f.collection.project = () => { const value = project(); f.collection.edit(0, "first.md", "During capture"); return value; };
  assert.throws(() => f.capture(), code("STALE_SOURCE"));
  assert.equal(f.endpoints[0].messages.length, 1);
});

test("late failures cannot replace a newer successful capture identity", async t => {
  const f = rig(t, { hold: true }); await f.run();
  f.collection.edit(0, "first.md", "HOLD"); const old = f.run();
  const rejected = assert.rejects(old, code("EXPORT_CANCELLED"));
  for (let n = 0; !f.endpoints[0].held && n < 20; n++) await tick();
  assert(f.endpoints[0].held); f.worker.cancel();
  f.collection.edit(0, "first.md", "Recovered"); await f.run(); await rejected;
  f.endpoints[0].release(); await tick(); const snapshots = f.state.snapshots;
  f.collection.edit(0, "first.md", "Next delta"); await f.run();
  assert.equal(f.state.snapshots, snapshots); assert.equal(f.endpoints[1].messages[1].bytes, 0);
});

test("publisher uses source capture for ordinary text edits and sequential PDF/EPUB/site exports", async t => {
  const f = await publisher(t); await f.controls.prepare("pdf");
  assert.equal(f.state.snapshots, 1);
  const firstUrl = f.el("download").href;
  f.edit("# Edited 😀"); assert.equal(f.el("download").hidden, true); assert(!f.urls.has(firstUrl));
  await f.controls.prepare("epub"); assert.equal((await f.output()).files[0].source, "# Edited 😀");
  await f.controls.prepare("site");
  assert.equal(f.state.snapshots, 1); assert.equal(f.endpoints.length, 1);
  assert.deepEqual(f.endpoints[0].messages[1].data.sourceUpdate.files, [{ path: "first.md", source: "# Edited 😀" }]);
  assert.deepEqual(f.endpoints[0].messages[2].data.sourceUpdate.files, []);
  assert(f.endpoints[0].messages.slice(1).every(packet => packet.bytes === 0));
});

test("publisher include edits and batched replacements share the latest source baseline", async t => {
  const f = await publisher(t); await f.controls.prepare("site");
  f.el("chapters").value = "2"; f.el("chapters").fire("change"); f.edit("Revised include");
  await f.controls.prepare("pdf"); assert.equal((await f.output()).includes[0].source, "Revised include");
  const next = f.collection.files; next[0].source = "Batch root"; next[2].source = "Batch include";
  f.controls.applySources(next, f.controls.checkpoint()); await f.controls.prepare("epub");
  const output = await f.output(); assert.equal(output.files[0].source, "Batch root"); assert.equal(output.includes[0].source, "Batch include");
  assert.equal(f.state.snapshots, 1); assert.equal(f.endpoints[0].native.stats.updates, 2);
});

test("publisher configuration edits, revocation and cancellation never reuse stale source-only authority", async t => {
  const f = await publisher(t); await f.controls.prepare("site");
  f.el("title").value = "New title"; f.el("title").fire("input");
  assert.equal(f.worker.hasRetainedBook, false); await f.controls.prepare("pdf");
  assert.equal((await f.output()).settings.metadata[0], "New title"); assert.equal(f.state.snapshots, 2);
  f.collection.revokeImages(); await f.controls.prepare("epub"); assert.deepEqual((await f.output()).images, []);
  f.el("cancel-export").fire("click"); assert.equal(f.worker.hasRetainedBook, false);
  await f.controls.prepare("site"); assert.equal(f.state.snapshots, 4);
});

test("publisher refuses stale results after silent DOM edits without granting a new capture", async t => {
  const f = await publisher(t, { hold: true }); await f.controls.prepare("site");
  f.edit("HOLD"); const old = f.controls.prepare("pdf");
  for (let n = 0; !f.endpoints[0].held && n < 20; n++) await tick(); assert(f.endpoints[0].held);
  f.el("chapter-source").value = "Unannounced edit";
  f.endpoints[0].release(); await assert.rejects(old, code("STALE_SOURCE"));
  assert.equal(f.el("download").hidden, true);
  await f.controls.prepare("epub"); assert.equal((await f.output()).files[0].source, "Unannounced edit");
});

test("PDF proofs independently use source deltas while preserving immutable exact-byte reuse", async t => {
  const f = await publisher(t), wire = transport(t);
  const proof = createBookPdfProof({ collection: f.collection, controls: f.controls, worker: wire.worker });
  t.after(() => proof.dispose());
  await f.controls.prepare("site"); const first = await proof.render();
  const snapshots = f.state.snapshots, bytes = await first.blob.text();
  assert.equal(await proof.render(), first); assert.equal(f.state.snapshots, snapshots);
  f.edit("New PDF source"); assert.equal(proof.current, null);
  const next = await proof.render();
  assert.equal(f.state.snapshots, snapshots); assert.equal(wire.endpoints.length, 1);
  assert.equal(wire.endpoints[0].messages[1].bytes, 0);
  assert.match(await next.blob.text(), /New PDF source/); assert.equal(await first.blob.text(), bytes);
  proof.cancel(); assert.equal(wire.worker.hasRetainedBook, false); assert(f.worker.hasRetainedBook);
  await f.controls.prepare("epub"); assert.equal(f.state.snapshots, snapshots);
});

test("PDF capture observes resource revocation and suspension even after a successful delta", async t => {
  const f = await publisher(t), wire = transport(t);
  const proof = createBookPdfProof({ collection: f.collection, controls: f.controls, worker: wire.worker });
  t.after(() => proof.dispose()); await proof.render(); f.edit("Edited"); await proof.render();
  assert.equal(f.state.snapshots, 1);
  f.collection.revokeFonts(); assert.equal(wire.worker.hasRetainedBook, false); assert.equal(proof.current, null);
  const next = await proof.render(); assert.deepEqual(JSON.parse((await next.blob.text()).slice(9)).fonts, []);
  assert.equal(f.state.snapshots, 2);
  proof.suspend(); assert.equal(wire.worker.hasRetainedBook, false);
  proof.resume(); assert.equal(wire.endpoints.length, 2);
  await proof.render(); assert.equal(f.state.snapshots, 3);
});
