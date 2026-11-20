import assert from "node:assert/strict";
import test from "node:test";
import { copyFile, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker } from "node:worker_threads";
import { createFlowAdapter, validateCreation } from "./flow_session.mjs";
import { normalizeFlowRequest } from "./flow_worker_protocol.mjs";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { NativeFlowFixture, renderCalls } from "./tests/flow_authoring_profile_fixture.mjs";

const profiles = ["commonmark-gfm", "gfm-plus"];
const source = "# Guide\n\nA footnote[^a].\n\n[^a]: Café 😀";
const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));
const code = expected => error => error.code === expected;
const fixtureUrl = new URL("./tests/flow_authoring_profile_fixture.mjs", import.meta.url).href;

async function entrypoint(mode = "current") {
  const dir = await mkdtemp(join(tmpdir(), "fmd-flow-profile-"));
  await mkdir(join(dir, "pkg"));
  for (const file of ["flow.js", "flow_session.mjs", "flow_asset_batch.mjs", "flow_export.mjs",
    "flow_outlines.mjs", "pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"])
    await copyFile(new URL(file, import.meta.url), join(dir, file));
  await writeFile(join(dir, "package.json"), '{"type":"module"}');
  await writeFile(join(dir, "franken_markdown.js"),
    `export async function init() {}\nexport { renderHtml, renderPdf, renderEpub, renderSvg } from ${JSON.stringify(fixtureUrl)};`);
  await writeFile(join(dir, "pkg", "franken_markdown.js"), `
    import { NativeFlowFixture } from ${JSON.stringify(fixtureUrl)};
    export class FmdFlowSession extends NativeFlowFixture {}
    ${mode === "legacy" ? "FmdFlowSession.withProfile = undefined;" : ""}
    ${mode === "short" ? "FmdFlowSession.withProfile = function(source) { throw Error('must not construct'); };" : ""}
    ${mode === "mismatch" ? `FmdFlowSession.withProfile = function(source, font, width, body, code, leading, profile) {
      const raw = NativeFlowFixture.withProfile(source, font, width, body, code, leading, profile);
      raw.profile = 'gfm-plus'; return raw;
    };` : ""}
  `);
  return import(pathToFileURL(join(dir, "flow.js")));
}

function workerEndpoint(data = {}, tracking = {}) {
  tracking.starts = (tracking.starts ?? 0) + 1;
  const worker = new Worker(new URL("./tests/flow_authoring_profile_fixture.mjs", import.meta.url),
    { workerData: data });
  const listeners = new Map();
  worker.on("error", () => {});
  return {
    postMessage(value, transfer) { worker.postMessage(value, transfer); },
    terminate() { tracking.terminated = true; return worker.terminate(); },
    addEventListener(kind, listener) {
      const fn = value => listener(kind === "message" ? { data: value } : { error: value });
      listeners.set(listener, fn); worker.on(kind, fn);
    },
    removeEventListener(kind, listener) {
      worker.off(kind, listeners.get(listener)); listeners.delete(listener);
    },
  };
}
async function worker(t, profile, data = {}) {
  const api = await createWorkerFlowSessionWith(() => workerEndpoint(data), source,
    profile === undefined ? {} : { profile }, { timeoutMs: 5000 });
  t.after(() => api.dispose());
  return api;
}

test("creation captures a canonical authoring dialect on both sides of the protocol", () => {
  for (const profile of profiles) {
    const options = { profile, font: "serif", viewportWidth: 360 };
    const prepared = validateCreation(source, options), wire = normalizeFlowRequest("create", [source, options]);
    options.profile = "changed";
    assert.equal(prepared.profile, profile); assert.equal(wire[1].profile, profile);
    assert.equal(Object.hasOwn(prepared.layout, "profile"), false);
    assert.equal(wire[1].font, "serif"); assert.equal(wire[1].viewportWidth, 360);
  }
  assert.equal(Object.hasOwn(validateCreation(source), "profile"), false);
  assert.equal(Object.hasOwn(normalizeFlowRequest("create", [source, {}])[1], "profile"), false);
});

test("invalid and accessor-valued profiles reject before initialization or worker allocation", async () => {
  const { createFlowSession } = await entrypoint();
  let starts = 0, reads = 0;
  const getter = { get profile() { reads++; return "gfm-plus"; } };
  for (const options of [...[null, "", "gfm", " commonmark-gfm ", true, 1, {}]
    .map(profile => ({ profile })), getter, Object.create(getter)]) {
    await assert.rejects(createFlowSession(source, options), code("INVALID_OPTIONS"));
    await assert.rejects(createWorkerFlowSessionWith(() => { starts++; }, source, options), code("INVALID_OPTIONS"));
  }
  assert.equal(starts, 0); assert.equal(reads, 0);
});

for (const profile of profiles) test(`public Flow ${profile} reaches native creation, preview and every publication`, async t => {
  const { createFlowSession } = await entrypoint(), options = { profile, font: "serif", viewportWidth: 360 };
  const pending = createFlowSession(source, options);
  options.profile = profile === "gfm-plus" ? "commonmark-gfm" : "gfm-plus";
  const api = await pending;
  t.after(() => api.dispose());
  assert.equal(api.profile, profile);
  assert.deepEqual(NativeFlowFixture.calls.at(-1), ["profile", source, "serif", 360, 14, 13, 20, profile]);
  assert.equal(api.snapshot().profile, profile);
  assert.equal(api.readingOrder().profile, profile);
  assert.equal(api.pendingAssets().profile, profile);
  assert.equal(api.hitTest(1, 1, api.token).profile, profile);
  assert.equal(api.selectText(0, 0, 2, api.token).profile, profile);
  api.edit(0, 0, "Introduction\n\n", { expectedRevision: api.revision });
  api.reflow({ viewportWidth: 640 }, api.token);
  api.reloadAssets(api.revision);
  assert.equal(api.profile, profile); assert.equal(api.snapshot().profile, profile);
  const edited = "Introduction\n\n" + source;
  for (const format of ["html", "pdf", "epub", "svg"]) {
    const result = await api.exportDocument(format, {}, api.token);
    assert.equal(result.profile, profile);
    assert.equal(decode(result).options.profile, profile);
    assert.equal(decode(result).source, edited);
  }
  const result = await api.exportDocuments(["html", "pdf", "epub", "svg"].map(format => ({ format })), api.token);
  assert.deepEqual(result.map(output => output.profile), Array(4).fill(profile));
  assert(result.every(output => decode(output).options.profile === profile));
  const before = renderCalls.length;
  for (const format of ["html", "pdf", "epub", "svg"])
    await assert.rejects(api.exportDocument(format, { profile: "gfm-plus" }, api.token), code("INVALID_OPTIONS"));
  assert.equal(renderCalls.length, before, "an export cannot select a dialect different from its preview");
});

test("omitted profiles retain the legacy constructor, native headers and publication option shape", async t => {
  const { createFlowSession } = await entrypoint("legacy"), api = await createFlowSession(source);
  t.after(() => api.dispose());
  assert.equal(api.profile, undefined);
  assert.deepEqual(NativeFlowFixture.calls.at(-1), ["legacy", source, "sans", 800, 14, 13, 20]);
  assert.equal(Object.hasOwn(api.snapshot(), "profile"), false);
  for (const format of ["html", "pdf", "epub", "svg"]) {
    const result = await api.exportDocument(format, {}, api.token);
    assert.equal(Object.hasOwn(result, "profile"), false);
    assert.equal(Object.hasOwn(decode(result).options, "profile"), false);
  }
});

for (const mode of ["legacy", "short"]) test(`explicit profiles refuse ${mode} native bindings without constructing a fallback`, async () => {
  const { createFlowSession } = await entrypoint(mode), before = NativeFlowFixture.calls.length;
  await assert.rejects(createFlowSession(source, { profile: "commonmark-gfm" }), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(NativeFlowFixture.calls.length, before);
});

test("a mismatched native profile is refused and the newly allocated session is freed", async () => {
  const { createFlowSession } = await entrypoint("mismatch");
  await assert.rejects(createFlowSession(source, { profile: "commonmark-gfm" }), code("INVALID_WASM_RESPONSE"));
  assert.equal(NativeFlowFixture.instances.at(-1).frees, 1);
});

test("direct reads and export inventories refuse missing or changed native profile metadata", async t => {
  const { createFlowSession } = await entrypoint(), api = await createFlowSession(source, { profile: "commonmark-gfm" });
  t.after(() => api.dispose());
  const raw = NativeFlowFixture.instances.at(-1);
  for (const profile of [undefined, "gfm-plus", "invalid"]) {
    raw.headerOverride = { profile };
    for (const read of [() => api.snapshot(), () => api.readingOrder(), () => api.pendingAssets(),
      () => api.hitTest(0, 0, api.token), () => api.selectText(0, 0, 1, api.token)])
      assert.throws(read, code("INVALID_WASM_RESPONSE"));
    raw.headerOverride = undefined;
    raw.inventoryOverride = { profile };
    const before = renderCalls.length;
    await assert.rejects(api.exportDocument("html", {}, api.token), code("INVALID_WASM_RESPONSE"));
    assert.equal(renderCalls.length, before);
  }
});

for (const profile of profiles) test(`real worker retains ${profile} through edits, delta snapshots and all publications`, async t => {
  const api = await worker(t, profile);
  assert.equal(api.profile, profile); assert.equal((await api.snapshot()).profile, profile);
  // The second identical, large item page uses the production delta codec.
  assert.equal((await api.snapshot()).profile, profile);
  assert.equal((await api.readingOrder()).profile, profile);
  assert.equal((await api.pendingAssets()).profile, profile);
  assert.equal((await api.hitTest(0, 0, api.token)).profile, profile);
  assert.equal((await api.selectText(0, 0, 1, api.token)).profile, profile);
  await api.edit(0, 0, "Edited\n", { expectedRevision: api.revision });
  await api.reflow({ viewportWidth: 640 }, api.token);
  assert.equal(api.profile, profile); assert.equal(await api.getSource(), "Edited\n" + source);
  for (const format of ["html", "pdf", "epub", "svg"]) {
    const result = await api.exportDocument(format, {}, api.token);
    assert.equal(result.profile, profile); assert.equal(decode(result).options.profile, profile);
    assert.equal(decode(result).source, "Edited\n" + source);
  }
  const result = await api.exportDocuments(["html", "pdf", "epub", "svg"].map(format => ({ format })), api.token);
  assert(result.every(output => output.profile === profile && decode(output).options.profile === profile));
});

test("worker creation owns the profile before a reentrant factory can mutate caller options", async t => {
  const options = { profile: "commonmark-gfm" };
  const api = await createWorkerFlowSessionWith(() => {
    options.profile = "gfm-plus"; return workerEndpoint();
  }, source, options);
  t.after(() => api.dispose());
  assert.equal(api.profile, "commonmark-gfm");
  assert.equal((await api.snapshot()).profile, "commonmark-gfm");
});

for (const corruptCreate of ["missing", "gfm-plus", "invalid"]) test(`worker startup refuses ${corruptCreate} dialect acknowledgment and releases ownership`, async () => {
  const tracking = {};
  await assert.rejects(createWorkerFlowSessionWith(() => workerEndpoint({ corruptCreate }, tracking),
    source, { profile: "commonmark-gfm" }),
  code("WORKER_PROTOCOL_ERROR"));
  assert.equal(tracking.terminated, true);
});

test("worker refuses an older native session that ignored the requested dialect", async () => {
  const tracking = {};
  await assert.rejects(createWorkerFlowSessionWith(() => workerEndpoint({ legacyNative: true }, tracking),
    source, { profile: "commonmark-gfm" }), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(tracking.terminated, true);
});

for (const corruption of ["missing", "gfm-plus"]) {
  test(`worker preview refuses ${corruption} dialect metadata before exposing pixels`, async t => {
    const api = await worker(t, "commonmark-gfm", { corruptRead: corruption });
    await assert.rejects(api.snapshot(), code("WORKER_PROTOCOL_ERROR"));
    assert.equal(api.disposed, true);
  });
  for (const batch of [false, true]) test(`worker ${batch ? "batch" : "single"} export refuses ${corruption} dialect metadata`, async t => {
    const api = await worker(t, "commonmark-gfm", { corruptExport: corruption });
    await assert.rejects(batch ? api.exportDocuments([{ format: "html" }, { format: "pdf" }], api.token)
      : api.exportDocument("html", {}, api.token), code("WORKER_PROTOCOL_ERROR"));
    assert.equal(api.disposed, true);
  });
}

test("legacy workers retain omitted profile metadata and publication behavior", async t => {
  const api = await worker(t, undefined, { legacyNative: true });
  assert.equal(api.profile, undefined);
  assert.equal(Object.hasOwn(await api.snapshot(), "profile"), false);
  const output = await api.exportDocument("html", {}, api.token);
  assert.equal(Object.hasOwn(output, "profile"), false);
  assert.equal(Object.hasOwn(decode(output).options, "profile"), false);
});

test("low-level adapters reject an invalid native profile instead of inventing a dialect", () => {
  const raw = new NativeFlowFixture(source);
  raw.profile = "invalid";
  assert.throws(() => createFlowAdapter(raw, undefined, "invalid"), code("INVALID_WASM_RESPONSE"));
});
