// Production Flow publication controls, preview ownership, retained export and
// adapter admission. Native renderers, painting and DOM elements are explicit
// doubles; these tests do not claim worker-thread or generated-WASM execution.

import assert from "node:assert/strict";
import test from "node:test";
import { createFlowAdapter, FlowError } from "../flow_session.mjs";
import { withFlowExports } from "../flow_export.mjs";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { createPdfVerificationControls } from "../demo/flow_pdf_verification_controls.mjs";
import { createExportControls } from "../demo/flow_export_controls.mjs";
import { normalizeRenderSettings, pdfLayoutVerificationOptions } from "../demo/flow_render_settings.mjs";

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
  const verificationElements = Object.fromEntries(["verify", "cancel", "download", "status", "findings"]
    .map(name => [name, new Element()]));
  const verification = createPdfVerificationControls({
    ...verificationElements, sourceEditor,
    readOptions: () => pdfLayoutVerificationOptions(settings.pdf),
    urls: {
      createObjectURL(blob) { f.blobs.push(blob); return "blob:verification-" + f.blobs.length; },
      revokeObjectURL(url) { f.revoked.push(url); },
    },
    verifyPdf(...args) {
      const job = controller.verifyPdf(...args);
      f.lastVerification = job.then(() => {}, () => {});
      return job;
    },
  });
  const input = (value, extra = {}) => ({
    source: value, width: 360, height: 300, ...settings.preview, ...extra,
  });
  controller = createPreviewController({
    async createSession(markdown, { font, profile, ...layout }) {
      const raw = {
        ...(profile === undefined ? {} : { profile }),
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
            ...(profile === undefined ? {} : { profile }),
            nodes: offset === 0 ? [{ text: raw.source }] : [], nextOffset: null,
          });
        },
        assetBytes() { return f.payload; },
      };
      const adapter = createFlowAdapter(raw, layout, profile);
      const render = (format) => async (markdown, rendererOptions) => {
        f.calls.push({ format, markdown, options: rendererOptions });
        const held = f.renderGate;
        f.renderGate = null;
        if (held) await held.promise;
        if (f.renderError) throw f.renderError;
        return { format, mimeType: mime[format], bytes: new Uint8Array([5, 6, 7]), diagnostics: [],
          ...(rendererOptions.pdfA ? { pdfA: rendererOptions.pdfA, pdfAStrict: rendererOptions.pdfAStrict } : {}),
          ...(profile === undefined ? {} : { profile }) };
      };
      const renderers = Object.fromEntries(formats
        .filter((format) => !options.unsupported?.includes(format))
        .map((format) => [format, render(format)]));
      if (!options.unsupported?.includes("verify")) renderers.verifyPdf = async (markdown, rendererOptions) => {
        f.calls.push({ format: "pdf-verification", markdown, options: rendererOptions });
        const held = f.renderGate;
        f.renderGate = null;
        if (held) await held.promise;
        if (f.renderError) throw f.renderError;
        return {
          schema_version: "1", target: "pdf", verdict: "findings", page_count: 1,
          anchors: { resolved: 3, unresolved: ["missing"] },
          findings: [{ code: "pdf.test", detail: "actual native report detail" }],
          pages: [{ number: 1, runs: [{ text: markdown, x: 72, y: 700, size: 11, kind: "body" }] }],
          digest: "fnv1a64:0123456789abcdef",
          ...(rendererOptions.profile === undefined ? {} : { profile: rendererOptions.profile }),
          extra: { complete: true },
        };
      };
      const session = withFlowExports(adapter, renderers, font, (expected, offset) => ({
        schemaVersion: 1, ...expected, offset, total: 1, nextOffset: null,
        ...(profile === undefined ? {} : { profile }),
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
    onState(state) { f.states.push(state); controls.update(state); verification.update(state); },
  });
  Object.assign(f, { controller, controls, buttons, sourceEditor, download, status, input, verification, verificationElements });
  f.prepare = async (format) => {
    buttons[format].click();
    await f.lastExport;
    await turn();
  };
  f.verify = async () => {
    verificationElements.verify.click();
    await f.lastVerification;
    await turn();
  };
  f.changeSettings = (values) => { settings = normalizeRenderSettings({ ...settings.values, ...values }); };
  f.replace = (value) => {
    sourceEditor.value = value;
    sourceEditor.dispatchEvent(new Event("input"));
    controller.update(input(value));
  };
  f.dispose = () => { verification.dispose(); controls.dispose(); controller.dispose(); };
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
    assert.equal(call.options.author, format === "svg" ? undefined : "PDF author");
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


test("configured verification and PDF export share exact source, settings, dialect, font and complete assets", async t => {
  const f = await fixture(); t.after(f.dispose);
  f.changeSettings({ profile: "commonmark-gfm", footnoteBacklinks: true, splitFootnotes: true,
    optimalPagination: true, strictPagination: true, baselineGridPt: 12,
    typographyThemeSpacing: true, pdfLineHeight: 1.4, tableCellPaddingXEm: 0, tableCellPaddingYEm: 0.75,
    headerLeft: "  {title}\r\nÉdition 😀  ", headerCenter: "{section}", headerRight: "{chapter}",
    footerLeft: "{author}\r{date}", footerCenter: "{page} / {pages}", footerRight: "{literal}",
    headerRule: true, footerRule: false, skipFirstPage: true, pageNumbers: false });
  f.controller.update(f.input(f.source)); await f.controller.whenIdle();
  assert.equal(f.controller.state.status, "ready", JSON.stringify(f.controller.state.error));
  await f.verify();
  assert.equal(f.verificationElements.download.hidden, false, f.verificationElements.status.textContent);
  assert.equal(f.verificationElements.download.click().defaultPrevented, false);
  const report = JSON.parse(await f.blobs[0].text());
  assert.equal(report.pages[0].runs[0].text, f.source);
  assert.deepEqual(report.extra, { complete: true });
  assert.equal(report.profile, "commonmark-gfm");
  assert.match(f.verificationElements.status.textContent, /1 PDF pages; 1 findings/);
  assert.match(f.verificationElements.findings.textContent, /pdf.test: actual native report detail/);
  await f.prepare("pdf");
  assert.deepEqual(f.calls.map(call => call.format), ["pdf-verification", "pdf"]);
  assert.equal(f.calls[0].markdown, f.calls[1].markdown);
  assert.deepEqual(f.calls[0].options, f.calls[1].options);
  assert.equal(f.calls[0].options.footnoteBacklinks, true);
  assert.equal(f.calls[0].options.splitFootnotes, true);
  assert.equal(f.calls[0].options.strictPagination, true);
  assert.equal(f.calls[0].options.typography.tableCellPaddingXEm, 0);
  assert.equal(f.calls[0].options.pageNumbers, false);
  assert.deepEqual(f.calls[0].options.running, {
    header: { left: "  {title}\r\nÉdition 😀  ", center: "{section}", right: "{chapter}", rule: true },
    footer: { left: "{author}\r{date}", center: "{page} / {pages}", right: "{literal}", rule: false },
    skipFirstPage: true,
  });
  assert.deepEqual(f.calls[0].options.pdfImages[0].bytes, f.payload);
});

for (const format of ["pdf", "pdf-verification"]) {
  test("pending " + format + " retains its bands and refuses changed running settings", async t => {
    const f = await fixture(), held = gate(); t.after(f.dispose);
    f.changeSettings({ headerLeft: "Original header", footerCenter: "{page}", skipFirstPage: true });
    f.renderGate = held;
    if (format === "pdf") f.buttons.pdf.click();
    else f.verificationElements.verify.click();
    assert.equal(f.calls.length, 1);
    f.changeSettings({ headerLeft: "Replacement header", footerRule: true, skipFirstPage: false });
    held.resolve();
    await (format === "pdf" ? f.lastExport : f.lastVerification);
    await turn();
    const status = format === "pdf" ? f.status : f.verificationElements.status;
    assert.equal(f.calls[0].options.running.header.left, "Original header");
    assert.equal(f.calls[0].options.running.skipFirstPage, true);
    assert.equal(f.blobs.length, 0);
    assert.match(status.textContent, /STALE_OPTIONS/);
    if (format === "pdf") await f.prepare("pdf");
    else await f.verify();
    assert.deepEqual(f.calls[1].options.running, {
      header: { left: "Replacement header" }, footer: { center: "{page}", rule: true }, skipFirstPage: false,
    });
    assert.equal(f.blobs.length, 1);
    const download = format === "pdf" ? f.download : f.verificationElements.download;
    f.changeSettings({ footerCenter: "{page} / {pages}" });
    assert.equal(download.click().defaultPrevented, true);
    assert.equal(download.hidden, true);
    assert.match(status.textContent, format === "pdf" ? /Publishing settings changed/ : /report is stale/);
    assert.equal(f.revoked.length, 1);
    assert.equal(f.sourceEditor.value, f.source);
  });
}

test("verification rejects unresolved images and missing package support without replacing the source", async t => {
  for (const unavailable of ["assets", "verifier"]) {
    const f = await fixture(unavailable === "verifier" ? { unsupported: ["verify"] } : {});
    t.after(f.dispose);
    if (unavailable === "assets") f.resolved = false;
    await f.verify();
    assert.equal(f.calls.length, 0); assert.equal(f.blobs.length, 0);
    assert.match(f.verificationElements.status.textContent,
      unavailable === "assets" ? /UNRESOLVED_EXPORT_ASSET/ : /UNSUPPORTED_WASM_PACKAGE/);
    assert.equal(f.controller.state.status, "ready");
    assert.equal(f.sourceEditor.value, f.source);
    f.resolved = true; await f.prepare("pdf");
    assert.equal(f.download.hidden, false);
  }
});

test("verification and export share a physical slot even after source replacement", async t => {
  const f = await fixture(); t.after(f.dispose);
  const held = gate(); f.renderGate = held;
  f.verificationElements.verify.click();
  await assert.rejects(f.controller.exportDocument("pdf", {}, f.source), error => error.code === "EXPORT_BUSY");
  f.replace(f.source + "changed");
  held.resolve(); await f.lastVerification; await f.controller.whenIdle(); await turn();
  assert.equal(f.blobs.length, 0);
  await f.prepare("pdf");
  assert.equal(f.download.hidden, false);
});

test("verification worker loss retires readiness without losing source or prior pixels", async t => {
  const f = await fixture(); t.after(f.dispose);
  const held = gate(); f.renderGate = held;
  const frame = f.controller.state.frame;
  f.verificationElements.verify.click();
  f.sessions[0].session.dispose();
  f.renderError = new FlowError("SESSION_LOST", "worker deadline");
  held.resolve(); await f.lastVerification; await turn();
  assert.equal(f.controller.state.status, "error");
  assert.equal(f.controller.state.frame, frame);
  assert.equal(f.controller.state.error.code, "SESSION_LOST");
  assert.equal(f.sourceEditor.value, f.source);
  assert.equal(f.verificationElements.download.hidden, true);
  f.renderError = null; f.controller.restart(); await f.controller.whenIdle();
  await f.verify(); assert.equal(f.verificationElements.download.hidden, false);
});

for (const [policy, strict] of [["2b", false], ["2b-strict", true]]) {
  test("live editor publishes " + policy + " and verifies the same configured layout", async t => {
    const f = await fixture(); t.after(f.dispose);
    f.changeSettings({ pdfPolicy: policy, baselineGridPt: 12, headerLeft: "{title}" });
    await f.prepare("pdf");
    assert.equal(f.download.hidden, false);
    assert.match(f.download.textContent, /PDF\/A-2b/);
    const { pdfA, pdfAStrict, ...layout } = f.calls[0].options;
    assert.equal(pdfA, "2b");
    assert.equal(pdfAStrict, strict);
    assert.equal(layout.baselineGridPt, 12);
    await f.verify();
    assert.equal(f.verificationElements.download.hidden, false);
    assert.deepEqual(f.calls[1].options, layout);
    assert.equal(f.calls[0].markdown, f.calls[1].markdown);
    f.changeSettings({ pdfPolicy: "off" });
    assert.equal(f.download.click().defaultPrevented, true);
  });
}
