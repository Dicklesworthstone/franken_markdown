// Production Flow publication controls, preview ownership, retained export and
// adapter admission. Native renderers, painting and DOM elements are explicit
// doubles; these tests do not claim worker-thread or generated-WASM execution.

import assert from "node:assert/strict";
import test from "node:test";
import { createFlowAdapter, FlowError } from "../flow_session.mjs";
import { withFlowExports } from "../flow_export.mjs";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { createExportControls } from "../demo/flow_export_controls.mjs";
import { normalizeRenderSettings } from "../demo/flow_render_settings.mjs";

const formats = ["html", "pdf", "epub", "svg"];
const mime = {
  html: "text/html; charset=utf-8", pdf: "application/pdf",
  epub: "application/epub+zip", svg: "image/svg+xml",
};
const turn = () => new Promise((resolve) => setTimeout(resolve, 0));
const gate = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
class Element extends EventTarget {
  disabled = false;
  hidden = false;
  value = "";
  textContent = "";
  removeAttribute(key) { delete this[key]; }
  click() {
    const event = new Event("click", { cancelable: true });
    this.dispatchEvent(event);
    return event;
  }
}
async function fixture(options = {}) {
  const source = "# Exact β 😀\r\n\r\n![local](local.png)\n";
  const f = {
    source, sessions: [], calls: [], states: [], blobs: [], revoked: [], clears: 0,
    renderGate: null, paintGate: null, renderError: null,
    payload: new Uint8Array([1, 2, 3, 4]),
    resolved: true,
  };
  let settings = normalizeRenderSettings({
    font: "serif", title: "Edition", lang: "nl", toc: true, tocDepth: 3,
    darkMode: "disabled", svgWidthPt: 900, author: "PDF author", pageSize: "a5",
  });
  const buttons = Object.fromEntries(formats.map((format) => [format, new Element()]));
  const sourceEditor = new Element(), download = new Element(), status = new Element();
  sourceEditor.value = source;
  let controller;
  const controls = createExportControls({
    ...buttons, sourceEditor, download, status,
    readOptions: (format) => settings[format],
    urls: {
      createObjectURL(blob) { f.blobs.push(blob); return "blob:publication-" + f.blobs.length; },
      revokeObjectURL(url) { f.revoked.push(url); },
    },
    exportDocument(...args) {
      const job = controller.exportDocument(...args);
      f.lastExport = job.then(() => {}, () => {});
      return job;
    },
  });
  const input = (value, extra = {}) => ({
    source: value, width: 360, height: 300, ...settings.preview, ...extra,
  });
  controller = createPreviewController({
    async createSession(markdown, { font, ...layout }) {
      const raw = {
        source: markdown, revision: "9007199254740993", layoutRevision: "9007199254740995",
        free() {},
        replaceSource(expected, next) {
          assert.equal(expected, raw.revision);
          raw.source = next;
          raw.revision = String(BigInt(raw.revision) + 1n);
          raw.layoutRevision = String(BigInt(raw.layoutRevision) + 1n);
        },
        reflow(expectedRevision, expectedLayout) {
          assert.equal(expectedRevision, raw.revision);
          assert.equal(expectedLayout, raw.layoutRevision);
          raw.layoutRevision = String(BigInt(raw.layoutRevision) + 1n);
        },
        readingJson(revision, layoutRevision, offset) {
          return JSON.stringify({
            schemaVersion: 1, revision, layoutRevision, offset, total: 1,
            nodes: offset === 0 ? [{ text: raw.source }] : [], nextOffset: null,
          });
        },
        assetBytes() { return f.payload; },
      };
      const adapter = createFlowAdapter(raw, layout);
      const render = (format) => async (markdown, rendererOptions) => {
        f.calls.push({ format, markdown, options: rendererOptions });
        const held = f.renderGate;
        f.renderGate = null;
        if (held) await held.promise;
        if (f.renderError) throw f.renderError;
        return { format, mimeType: mime[format], bytes: new Uint8Array([5, 6, 7]), diagnostics: [] };
      };
      const renderers = Object.fromEntries(formats
        .filter((format) => !options.unsupported?.includes(format))
        .map((format) => [format, render(format)]));
      const session = withFlowExports(adapter, renderers, font, (expected, offset) => ({
        schemaVersion: 1, ...expected, offset, total: 1, nextOffset: null,
        items: [{ kind: "image", requestId: "7", destination: "local.png", isResolved: f.resolved }],
      }));
      f.sessions.push({ session, adapter, raw });
      return session;
    },
    painter: {
      async render(session, options) {
        const frame = { ...session.token, width: options.width, height: options.height };
        const held = f.paintGate;
        f.paintGate = null;
        if (held) await held.promise; // Intentionally ignores abort to test the owner fence.
        return frame;
      },
      clear() { f.clears++; },
      dispose() {},
    },
    onState(state) { f.states.push(state); controls.update(state); },
  });
  Object.assign(f, { controller, controls, buttons, sourceEditor, download, status, input });
  f.prepare = async (format) => {
    buttons[format].click();
    await f.lastExport;
    await turn();
  };
  f.changeSettings = (values) => { settings = normalizeRenderSettings({ ...settings.values, ...values }); };
  f.replace = (value) => {
    sourceEditor.value = value;
    sourceEditor.dispatchEvent(new Event("input"));
    controller.update(input(value));
  };
  f.dispose = () => { controls.dispose(); controller.dispose(); };
  controller.update(input(source));
  await controller.whenIdle();
  assert.equal(controller.state.status, "ready");
  return f;
}

for (const format of formats) {
  test("retained publication controls carry source, font and authorized image into " + format, async (t) => {
    const f = await fixture();
    t.after(f.dispose);
    const frame = f.controller.state.frame;
    await f.prepare(format);
    assert.equal(f.calls.length, 1);
    const call = f.calls[0];
    assert.equal(call.markdown, f.source);
    assert.equal(call.options.font, "serif");
    assert.equal(call.options.toc, true);
    assert.equal(call.options.tocDepth, 3);
    assert.deepEqual(call.options.pdfImages, [{ destination: "local.png", bytes: new Uint8Array([1, 2, 3, 4]) }]);
    assert.equal(call.options.pdfImages[0].bytes === f.payload, false);
    assert.equal(call.options[format === "svg" ? "maxWidthPt" : "title"], format === "svg" ? 900 : "Edition");
    assert.equal(call.options.lang, format === "svg" ? undefined : "nl");
    assert.equal(call.options.darkMode, ["html", "epub"].includes(format) ? "disabled" : undefined);
    assert.equal(call.options.author, format === "pdf" ? "PDF author" : undefined);
    assert.equal(call.options.allowRawHtml, format === "svg" ? undefined : false);
    assert.equal(f.blobs[0].type, mime[format]);
    assert.equal(f.download.download, "document." + format);
    assert.equal(f.download.click().defaultPrevented, false);
    assert.equal(f.controller.state.frame, frame);
    assert.equal(f.sessions[0].session.source, f.source);
    assert.equal(f.sourceEditor.value, f.source);
  });
}

for (const format of ["epub", "svg"]) {
  test("missing native " + format + " capability refuses publication without losing HTML/PDF", async (t) => {
    const f = await fixture({ unsupported: [format] });
    t.after(f.dispose);
    await f.prepare(format);
    assert.equal(f.calls.length, 0);
    assert.equal(f.blobs.length, 0);
    assert.match(f.status.textContent, /UNSUPPORTED_WASM_PACKAGE/);
    assert.equal(f.controller.state.status, "ready");
    assert.equal(f.sessions[0].session.disposed, false);
    assert.equal(f.sourceEditor.value, f.source);
    for (const available of ["html", "pdf"]) {
      await f.prepare(available);
      assert.equal(f.download.download, "document." + available);
    }
  });

  test("new " + format + " publication refuses unresolved images before native rendering", async (t) => {
    const f = await fixture();
    t.after(f.dispose);
    f.resolved = false;
    await f.prepare(format);
    assert.equal(f.calls.length, 0);
    assert.equal(f.blobs.length, 0);
    assert.match(f.status.textContent, /UNRESOLVED_EXPORT_ASSET/);
    f.resolved = true;
    await f.prepare(format);
    assert.equal(f.calls.length, 1);
    assert.equal(f.download.hidden, false);
  });

  test("pending " + format + " publication cannot cross a source revision", async (t) => {
    const f = await fixture(), held = gate();
    t.after(f.dispose);
    f.renderGate = held;
    f.buttons[format].click();
    assert.equal(f.calls.length, 1);
    const call = f.calls[0];
    f.payload[0] = 99;
    f.replace("# Replacement\n");
    await f.controller.whenIdle();
    held.resolve();
    await f.lastExport;
    await turn();
    assert.equal(call.markdown, f.source);
    assert.deepEqual([...call.options.pdfImages[0].bytes], [1, 2, 3, 4]);
    assert.equal(f.blobs.length, 0);
    assert.equal(f.sourceEditor.value, "# Replacement\n");
    await f.prepare(format);
    assert.equal(f.calls[1].markdown, "# Replacement\n");
    assert.deepEqual([...f.calls[1].options.pdfImages[0].bytes], [99, 2, 3, 4]);
    assert.equal(f.download.hidden, false);
  });
}

test("retained publication keeps captured options and rejects changed publishing settings", async (t) => {
  const f = await fixture(), held = gate();
  t.after(f.dispose);
  f.renderGate = held;
  f.buttons.svg.click();
  f.changeSettings({ svgWidthPt: 1200, tocDepth: 2 });
  held.resolve();
  await f.lastExport;
  await turn();
  assert.equal(f.calls[0].options.maxWidthPt, 900);
  assert.equal(f.calls[0].options.tocDepth, 3);
  assert.equal(f.blobs.length, 0);
  assert.match(f.status.textContent, /STALE_OPTIONS/);
  await f.prepare("svg");
  assert.equal(f.calls[1].options.maxWidthPt, 1200);
  assert.equal(f.calls[1].options.tocDepth, 2);
});

for (const code of ["TIMEOUT", "ABORTED"]) {
  test("publication " + code + " retires a lost session and requires explicit restart", async (t) => {
    const f = await fixture(), held = gate();
    t.after(f.dispose);
    const frame = f.controller.state.frame;
    f.renderGate = held;
    f.buttons.epub.click();
    f.sessions[0].adapter.dispose();
    held.reject(new FlowError(code, "worker unavailable"));
    await f.lastExport;
    await turn();
    assert.equal(f.controller.state.status, "error");
    assert.equal(f.controller.state.error.code, "SESSION_LOST");
    assert.equal(f.controller.state.frame, frame);
    assert.equal(f.controller.state.reading, "");
    assert.equal(f.controller.state.document, null);
    assert.equal(f.controller.state.readingPending, false);
    assert.equal(f.sourceEditor.value, f.source);
    assert.equal(f.sessions.length, 1);
    assert.equal(f.clears, 0);
    assert.equal(f.blobs.length, 0);
    for (const format of formats) {
      assert.equal(f.buttons[format].disabled, true);
      f.buttons[format].click();
    }
    assert.equal(f.calls.length, 1);
    f.replace("# Kept edit after worker loss\n");
    await f.controller.whenIdle();
    assert.equal(f.controller.state.error.code, "SESSION_LOST");
    assert.equal(f.sessions.length, 1);
    f.controller.restart();
    await f.controller.whenIdle();
    assert.equal(f.sessions[1].session.source, "# Kept edit after worker loss\n");
    assert.equal(f.sessions.length, 2);
    await f.prepare("pdf");
    assert.equal(f.controller.state.status, "ready");
    assert.equal(f.download.download, "document.pdf");
  });
}

test("a retired export cannot poison a replacement session or release the physical slot early", async (t) => {
  const f = await fixture(), held = gate();
  t.after(f.dispose);
  f.renderGate = held;
  f.buttons.epub.click();
  f.controller.restart();
  await f.controller.whenIdle();
  const replacement = f.controller.state.frame;
  await assert.rejects(f.controller.exportDocument("svg", {}, f.source), { code: "EXPORT_BUSY" });
  held.reject(new FlowError("TIMEOUT", "retired worker"));
  await f.lastExport;
  await turn();
  assert.equal(f.controller.state.status, "ready");
  assert.equal(f.controller.state.frame, replacement);
  assert.equal(f.sessions[1].session.disposed, false);
  assert.equal(f.blobs.length, 0);
  await f.prepare("svg");
  assert.equal(f.download.download, "document.svg");
});

test("an ignored paint abort cannot republish readiness after publication loses its worker", async (t) => {
  const f = await fixture(), render = gate(), paint = gate();
  t.after(f.dispose);
  const frame = f.controller.state.frame;
  f.renderGate = render;
  f.buttons.epub.click();
  f.paintGate = paint;
  f.controller.update(f.input(f.source, { height: 301 }));
  await turn();
  f.sessions[0].adapter.dispose();
  render.reject(new FlowError("TIMEOUT", "worker lost during paint"));
  await f.lastExport;
  await turn();
  assert.equal(f.controller.state.status, "error");
  assert.equal(f.controller.state.error.code, "SESSION_LOST");
  paint.resolve();
  await f.controller.whenIdle();
  assert.equal(f.controller.state.status, "error");
  assert.equal(f.controller.state.error.code, "SESSION_LOST");
  assert.equal(f.controller.state.frame, frame);
  assert.equal(f.sessions.length, 1);
  assert.equal(f.blobs.length, 0);
});

test("native publication refusal on a live worker preserves readiness and supports retry", async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  f.renderError = Object.assign(new Error("SVG navigation unavailable"), { code: "UNSUPPORTED_WASM_PACKAGE" });
  await f.prepare("svg");
  assert.equal(f.controller.state.status, "ready");
  assert.equal(f.sessions[0].session.disposed, false);
  assert.equal(f.blobs.length, 0);
  assert.match(f.status.textContent, /UNSUPPORTED_WASM_PACKAGE/);
  f.renderError = null;
  await f.prepare("svg");
  assert.equal(f.download.download, "document.svg");
});
