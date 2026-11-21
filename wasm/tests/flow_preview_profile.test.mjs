// Real editor orchestration/settings, public Flow entrypoint and worker transport.
// Native bindings, renderers, DOM-independent painter and semantic sessions are
// explicit doubles; this does not claim Rust parsing or browser pixel coverage.
import assert from "node:assert/strict";
import test from "node:test";
import { copyFile, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker } from "node:worker_threads";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { normalizeRenderSettings } from "../demo/flow_render_settings.mjs";
import { createWorkerFlowSessionWith } from "../flow_worker_session.mjs";
import { readFlowDocument } from "../flow_reading.mjs";
import { deferred, node, ReadingSession } from "./flow_reading_fixtures.mjs";

const fixtureUrl = new URL("./flow_authoring_profile_fixture.mjs", import.meta.url);
const source = "# Café\n\nTerm\n: Definition\n\nA note[^n].\n\n[^n]: A footnote.";
const input = (source, settings, other = {}) =>
  ({ source, width: 400, height: 300, ...settings.preview, ...other });
const decode = output => JSON.parse(new TextDecoder().decode(output.bytes));
const next = () => new Promise(resolve => setImmediate(resolve));

async function directFactory(legacy = false) {
  const dir = await mkdtemp(join(tmpdir(), "fmd-preview-profile-"));
  await mkdir(join(dir, "pkg"));
  await Promise.all(["flow.js", "flow_session.mjs", "flow_asset_batch.mjs", "flow_export.mjs",
    "flow_outlines.mjs", "pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"]
    .map(file => copyFile(new URL(`../${file}`, import.meta.url), join(dir, file))));
  await writeFile(join(dir, "package.json"), '{"type":"module"}');
  await writeFile(join(dir, "franken_markdown.js"),
    `export async function init() {}\nexport { renderHtml, renderPdf, renderEpub, renderSvg } from ${JSON.stringify(fixtureUrl.href)};`);
  await writeFile(join(dir, "pkg", "franken_markdown.js"), `
    import { NativeFlowFixture } from ${JSON.stringify(fixtureUrl.href)};
    export class FmdFlowSession extends NativeFlowFixture {}
    ${legacy ? "FmdFlowSession.withProfile = undefined;" : ""}
  `);
  return (await import(pathToFileURL(join(dir, "flow.js")))).createFlowSession;
}
function endpoint(legacyNative) {
  const worker = new Worker(fixtureUrl, { workerData: { legacyNative } });
  const listeners = new Map();
  worker.on("error", () => {});
  return {
    postMessage(value, transfer) { worker.postMessage(value, transfer); },
    terminate() { return worker.terminate(); },
    addEventListener(kind, listener) {
      const fn = value => listener(kind === "message" ? { data: value } : { error: value });
      listeners.set(listener, fn);
      worker.on(kind, fn);
    },
    removeEventListener(kind, listener) {
      worker.off(kind, listeners.get(listener));
      listeners.delete(listener);
    },
  };
}
async function factory(mode, legacy = false) {
  if (mode === "direct") return directFactory(legacy);
  return (source, options, transport) => createWorkerFlowSessionWith(() => endpoint(legacy),
    source, options, { ...transport, timeoutMs: 5000 });
}
function fixture(createSession) {
  const f = { sessions: [], frames: [], creates: [] };
  f.controller = createPreviewController({
    async createSession(source, options, transport) {
      f.creates.push({ source, options });
      const session = await createSession(source, options, transport);
      f.sessions.push(session);
      return session;
    },
    readDocument: readFlowDocument,
    painter: {
      async render(session, options) {
        const page = await session.snapshot({ token: options.token });
        assert.equal(page.profile, session.profile);
        f.frames.push(page);
        return { revision: page.revision, layoutRevision: page.layoutRevision,
          width: options.width, height: options.height };
      },
      clear() {}, dispose() {},
    },
  });
  return f;
}

for (const mode of ["direct", "worker"]) {
  test(`${mode} editor applies each dialect to snapshots and HTML/PDF/EPUB/SVG publications`, async t => {
    const f = fixture(await factory(mode)), c = f.controller;
    t.after(() => c.dispose());
    for (const profile of ["commonmark-gfm", "gfm-plus", null]) {
      const settings = normalizeRenderSettings({ profile, font: "serif", title: "Guide" });
      c.update(input(source, settings));
      await c.whenIdle();
      assert.equal(c.state.status, "ready");
      assert.equal(c.state.readingError, null);
      assert.equal(f.sessions.at(-1).profile, profile ?? undefined);
      assert.equal(f.frames.at(-1).profile, profile ?? undefined);
      assert.equal(Object.hasOwn(f.creates.at(-1).options, "profile"), profile !== null);
      const edited = `${source}\n\nEdited source`;
      const session = f.sessions.at(-1);
      c.update(input(edited, settings, { width: 640, lineHeight: 24 }));
      await c.whenIdle();
      assert.equal(f.sessions.at(-1), session, "editing and reflow retain immutable grammar");
      assert.equal(session.profile, profile ?? undefined);
      for (const format of ["html", "pdf", "epub", "svg"]) {
        const output = await c.exportDocument(format, settings[format], edited);
        assert.equal(output.profile, profile ?? undefined);
        assert.equal(decode(output).options.profile, profile ?? undefined);
        assert.equal(decode(output).source, edited);
        assert.equal(decode(output).options.font, "serif");
        assert.equal(Object.hasOwn(settings[format], "profile"), false);
        assert.equal(Object.hasOwn(output, "profile"), profile !== null);
      }
      c.restart();
      await c.whenIdle();
      assert.equal(session.disposed, true);
      assert.equal(f.sessions.at(-1).profile, profile ?? undefined);
      assert.equal(f.creates.at(-1).source, edited);
      assert.equal(f.frames.at(-1).profile, profile ?? undefined);
    }
    assert.equal(f.sessions.length, 6);
  });

  test(`${mode} editor reports unsupported explicit dialects without fallback and recovers on renderer default`, async t => {
    const f = fixture(await factory(mode, true)), c = f.controller;
    t.after(() => c.dispose());
    c.update(input(source, normalizeRenderSettings({ profile: "commonmark-gfm" })));
    await c.whenIdle();
    assert.equal(c.state.status, "error");
    assert.equal(c.state.error.code, "UNSUPPORTED_WASM_PACKAGE");
    assert.equal(c.state.frame, null);
    assert.equal(f.frames.length, 0);
    assert.equal(f.creates.length, 1, "no automatic unprofiled fallback");
    c.update(input(source, normalizeRenderSettings()));
    await c.whenIdle();
    assert.equal(c.state.status, "ready");
    assert.equal(f.creates.at(-1).source, source);
    assert.equal(Object.hasOwn(f.creates.at(-1).options, "profile"), false);
    const output = await c.exportDocument("html", {}, source);
    assert.equal(Object.hasOwn(output, "profile"), false);
    assert.equal(decode(output).source, source);
  });
}

test("dialect replacement withdraws semantic navigation even when source and tokens match", async t => {
  const sessions = [], text = "Café 😀";
  const c = createPreviewController({
    readDocument: readFlowDocument,
    async createSession(source, options) {
      const s = new ReadingSession([node("paragraph", `${options.profile}: ${source}`, {
        enclosingSourceSpan: { startByte: 0, endByte: new TextEncoder().encode(source).length },
      })]);
      s.layoutOptions = options;
      s.dispose = () => { s.disposed = true; };
      sessions.push(s);
      return s;
    },
    painter: { async render(s) { return s.token; }, clear() {}, dispose() {} },
  });
  t.after(() => c.dispose());
  c.update(input(text, normalizeRenderSettings({ profile: "commonmark-gfm" })));
  await c.whenIdle();
  const old = c.state.document;
  assert.equal(old.text, `commonmark-gfm: ${text}`);
  assert.deepEqual(c.locateReading(0, old, text).sourceRange, { start: 0, end: text.length });
  c.update(input(text, normalizeRenderSettings({ profile: "gfm-plus" })));
  assert.equal(c.state.document, null);
  assert.throws(() => c.locateReading(0, old, text), { code: "STALE_REVISION" });
  await c.whenIdle();
  assert.deepEqual(c.state.document.token, old.token);
  assert.equal(c.state.document.text, `gfm-plus: ${text}`);
  assert.equal(sessions[0].disposed, true);
  assert.throws(() => c.locateReading(0, old, text), { code: "STALE_REVISION" });
  assert.deepEqual(c.locateReading(0, c.state.document, text).sourceRange, { start: 0, end: text.length });
});

test("a slow painter cannot publish an obsolete dialect after the replacement request", async t => {
  const hold = deferred(), states = [], sessions = [];
  let first = true;
  const c = createPreviewController({
    async createSession(source, options) {
      const s = new ReadingSession([]);
      s.layoutOptions = options;
      s.dispose = () => { s.disposed = true; };
      sessions.push(s);
      return s;
    },
    painter: {
      async render(s) {
        const frame = { ...s.token, profile: s.layoutOptions.profile };
        if (first) { first = false; await hold.promise; }
        return frame; // Deliberately ignores cancellation to exercise the owner fence.
      },
      clear() {}, dispose() {},
    },
    onState(state) { states.push(state); },
  });
  t.after(() => { hold.resolve(); c.dispose(); });
  c.update(input(source, normalizeRenderSettings({ profile: "commonmark-gfm" })));
  await next();
  c.update(input(source, normalizeRenderSettings({ profile: "gfm-plus" })));
  hold.resolve();
  await c.whenIdle();
  assert.equal(sessions[0].disposed, true);
  assert.equal(c.state.frame.profile, "gfm-plus");
  assert.equal(states.some(state => state.frame?.profile === "commonmark-gfm"), false);
});
