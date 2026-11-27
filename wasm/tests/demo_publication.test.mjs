// Production demo controller and transport. DOM/dialog/download adapters and
// native renderer receipts are explicit doubles; no generated-WASM/browser claim.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createHash } from "node:crypto";
import { createWorkerRenderer } from "../document_worker.mjs";
import { fixture, deferred } from "./demo_file_helpers.mjs";

const prepare = (f, format) => format === "epub" ? f.controller.prepareEpub() : f.controller.prepareSvg();
async function finish(f, pending, text = "publication café😀") {
  const call = f.renders.at(-1); call.resolve(f.output(call, text));
  assert.equal(await pending, true); return call;
}

for (const format of ["epub", "svg"]) test(`${format} preserves captured source, dialect, images and compatible settings`, async t => {
  const f = fixture(t), bytes = new TextEncoder().encode("<svg/>");
  f.input("# Café😀\n\n![Diagram](chart.svg)"); f.input("gfm-plus", "profile");
  f.input("serif", "font"); f.input("disabled", "dark-mode");
  f.input("p { color: green }", "custom-css");
  await f.controller.importImages([new File([bytes], "chart.svg")]);
  const pending = prepare(f, format), call = f.renders.at(-1);
  assert.equal(call.kind, format); assert.equal(call.source, f.els.markdown.value);
  assert.equal(call.options.profile, "gfm-plus"); assert.equal(call.options.font, "serif");
  assert.equal(call.options.darkMode, "disabled");
  assert.deepEqual(call.options.pdfImages.map(asset => asset.destination), ["chart.svg", "./chart.svg"]);
  for (const asset of call.options.pdfImages) assert.deepEqual(asset.bytes, bytes);
  for (const key of ["page", "allowRawHtml", "codeLineNumbers", "metadataEpochSeconds"])
    assert.equal(Object.hasOwn(call.options, key), false, key);
  if (format === "epub") {
    assert.equal(call.options.title, "Original title"); assert.equal(call.options.author, "Author");
    assert.equal(call.options.customCss, "p { color: green }");
  } else for (const key of ["title", "author", "customCss"]) assert.equal(Object.hasOwn(call.options, key), false, key);
  assert.equal(f.els["prepare-epub"].disabled, true); assert.equal(f.els["prepare-svg"].disabled, true);
  assert.equal(f.els["cancel-publication"].disabled, false);
  await finish(f, pending);
  assert.equal(call.client.disposed, true, "release the extra worker after publication");
  const link = f.els["download-publication"];
  assert.equal(link.hidden, false); assert.equal(link.download, `original-title.${format}`);
  assert.equal(await f.urls.at(-1).blob.text(), "publication café😀");
  assert.equal(f.downloads.length, 0, "preparation does not download automatically");
  link.click(); link.click();
  assert.equal(f.downloads.length, 2); assert.equal(f.renders.length, 1);
  assert.match(f.els["publication-status"].textContent, new RegExp(`${format.toUpperCase()} ready`));
});

test("PDF and workspace defaults keep their original option shape", async t => {
  const f = fixture(t);
  await finish(f, f.controller.downloadPdf());
  assert.equal(Object.hasOwn(f.renders[0].options, "page"), false);
  const pending = f.controller.prepareWorkspace(), call = f.workspaces[0];
  assert.equal(Object.hasOwn(call.options, "page"), false);
  call.resolve(f.output(call)); assert.equal(await pending, true);
});

for (const [size, width, height] of [["letter", 612, 792], ["a4", 210 * 72 / 25.4, 297 * 72 / 25.4],
  ["a5", 148 * 72 / 25.4, 210 * 72 / 25.4], ["legal", 612, 1008], ["tabloid", 792, 1224]]) {
  test(`${size} landscape PDF rotates paper without rotating independent margins`, async t => {
    const f = fixture(t);
    for (const [id, value] of [["pdf-page-size", size], ["pdf-orientation", "landscape"],
      ["pdf-margin-top", "0"], ["pdf-margin-right", "12.25"], ["pdf-margin-bottom", "48.5"], ["pdf-margin-left", "61"]]) f.input(value, id);
    const pending = f.controller.downloadPdf(), call = f.renders[0];
    assert.deepEqual(call.options.page, { size: { widthPt: height, heightPt: width },
      margins: { topPt: 0, rightPt: 12.25, bottomPt: 48.5, leftPt: 61 } });
    assert.equal(Object.isFrozen(call.options.page.size), true);
    await finish(f, pending);
  });
}

test("custom paper, partial margins and workspace handoff preserve exact geometry", async t => {
  const f = fixture(t);
  f.input("custom", "pdf-page-size"); assert.equal(f.els["pdf-width"].disabled, false);
  f.input("810.125", "pdf-width"); f.input("450.5", "pdf-height"); f.input("19.75", "pdf-margin-left");
  const pending = f.controller.prepareWorkspace(), call = f.workspaces[0];
  assert.deepEqual(call.options.page, { size: { widthPt: 810.125, heightPt: 450.5 },
    margins: { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 19.75 } });
  call.resolve(f.output(call)); assert.equal(await pending, true);
  const oldUrl = f.els["download-workspace"].href;
  f.input("a4", "pdf-page-size"); assert.equal(f.els["pdf-width"].disabled, true);
  assert.equal(f.els["pdf-width"].value, "810.125", "retain inactive custom dimensions for later use");
  assert.equal(f.els["download-workspace"].hidden, true); assert(f.revoked.includes(oldUrl));
  await finish(f, f.controller.downloadPdf());
  assert.equal(f.renders[0].options.page.size.widthPt, 210 * 72 / 25.4);
});

for (const values of [
  { "pdf-page-size": "custom", "pdf-width": "600" },
  { "pdf-page-size": "custom", "pdf-width": "143", "pdf-height": "600" },
  { "pdf-page-size": "custom", "pdf-width": "14401", "pdf-height": "600" },
  { "pdf-margin-left": "300", "pdf-margin-right": "300" },
  { "pdf-margin-top": "-1" }, { "pdf-margin-top": "NaN" }, { "pdf-margin-top": "Infinity" },
  { "pdf-margin-top": "0x10" }, { "pdf-orientation": "sideways" }, { "pdf-page-size": "unknown" },
]) test(`invalid paper settings reject before rendering: ${JSON.stringify(values)}`, async t => {
  const f = fixture(t);
  for (const [id, value] of Object.entries(values)) f.input(value, id);
  assert.equal(await f.controller.downloadPdf(), false);
  assert.equal(f.renders.length, 0); assert.equal(f.urls.length, 0);
  assert.match(f.els.diagnostics.children[0].textContent, /INVALID_OPTIONS/);
  assert.equal(f.els.markdown.value, "# Original");
  await finish(f, f.controller.prepareEpub()); // PDF-only fields do not constrain an ebook.
});

test("unfinished browser number input cannot silently select default margins", async t => {
  const f = fixture(t);
  f.els["pdf-margin-left"].validity = { badInput: true };
  assert.equal(f.els["pdf-margin-left"].value, "");
  assert.equal(await f.controller.downloadPdf(), false); assert.equal(f.renders.length, 0);
  assert.match(f.els.diagnostics.children[0].textContent, /INVALID_OPTIONS/);
});

test("fresh publication and cancellation preserve completed HTML and PDF output", async t => {
  const f = fixture(t);
  await finish(f, f.controller.renderPreview(), "HTML");
  await finish(f, f.controller.downloadPdf(), "PDF");
  const ordinaryUrls = f.urls.map(value => value.url), htmlUrl = f.els["download-html"].href;
  await finish(f, f.controller.prepareEpub(), "EPUB");
  const epubUrl = f.els["download-publication"].href;
  const pending = f.controller.prepareSvg(), call = f.renders.at(-1);
  assert(f.revoked.includes(epubUrl));
  assert.equal(f.controller.cancelPublication(), true); assert.equal(call.controls.signal.aborted, true);
  call.resolve(f.output(call, "late SVG")); assert.equal(await pending, false);
  assert.equal(f.els["download-publication"].hidden, true);
  assert.equal(f.els["download-html"].href, htmlUrl);
  assert(ordinaryUrls.every(url => !f.revoked.includes(url)));
});

for (const [id, value] of [["markdown", "New source"], ["profile", "commonmark-gfm"],
  ["author", "Another author"], ["pdf-page-size", "a4"], ["pdf-margin-bottom", "20"]]) {
  test(`${id} edit retires an in-flight publication and ignores its late output`, async t => {
    const f = fixture(t), pending = f.controller.prepareEpub(), call = f.renders[0];
    f.input(value, id); assert.equal(call.controls.signal.aborted, true); assert.equal(call.client.disposed, true);
    call.resolve(f.output(call)); assert.equal(await pending, false);
    assert.equal(f.urls.length, 0); assert.equal(f.els["download-publication"].hidden, true);
  });
  test(`silent ${id} mutation blocks activation of an already prepared download`, async t => {
    const f = fixture(t); await finish(f, f.controller.prepareSvg());
    f.els[id].value = value; f.els["download-publication"].click();
    assert.equal(f.downloads.length, 0); assert.equal(f.els["download-publication"].hidden, true);
    assert.deepEqual(f.revoked, f.urls.map(item => item.url));
  });
}

for (const boundary of ["composition", "suspension", "disposal", "image replacement", "file opening"]) {
  test(`${boundary} revokes pending and completed publication ownership`, async t => {
    const f = fixture(t); await finish(f, f.controller.prepareEpub());
    const pending = f.controller.prepareSvg(), call = f.renders.at(-1);
    let gate, open;
    if (boundary === "composition") f.els.markdown.dispatchEvent(new Event("compositionstart"));
    else if (boundary === "suspension") f.win.dispatchEvent(new Event("pagehide"));
    else if (boundary === "disposal") f.controller.dispose();
    else if (boundary === "image replacement") await f.controller.importImages([new File(["image"], "next.png")]);
    else {
      gate = deferred(); const file = new File(["New"], "new.md"); file.arrayBuffer = () => gate.promise;
      open = f.controller.openMarkdown(file);
    }
    assert.equal(call.controls.signal.aborted, true); assert.equal(call.client.disposed, true);
    call.resolve(f.output(call)); assert.equal(await pending, false);
    assert.equal(f.els["download-publication"].hidden, true);
    assert.deepEqual(f.revoked, f.urls.map(item => item.url));
    if (gate) { gate.resolve(new TextEncoder().encode("New").buffer); await open; }
  });
}

test("publication rechecks ownership after renderer helper and Blob URL callbacks", async t => {
  const f = fixture(t), pending = f.controller.prepareSvg(), call = f.renders[0];
  const original = f.win.URL.createObjectURL;
  f.win.URL.createObjectURL = blob => { const url = original(blob); f.els["pdf-margin-left"].value = "20"; return url; };
  call.resolve(f.output(call)); assert.equal(await pending, false);
  assert.equal(f.els["download-publication"].hidden, true); assert.deepEqual(f.revoked, f.urls.map(item => item.url));
});

test("worker-release callbacks cannot leave a stale publication marked ready", async t => {
  const f = fixture(t), pending = f.controller.prepareEpub(), call = f.renders[0];
  call.client.dispose = () => { call.client.disposed = true; f.els.markdown.value = "Changed during release"; };
  call.resolve(f.output(call)); assert.equal(await pending, false);
  assert.equal(f.els["download-publication"].hidden, true);
  assert.match(f.els["publication-status"].textContent, /changed during publication/);
  assert.deepEqual(f.revoked, f.urls.map(item => item.url));
});

test("raw HTML incompatibility and native capability refusals leave source recoverable", async t => {
  const f = fixture(t); f.els["allow-html"].checked = true;
  assert.equal(await f.controller.prepareEpub(), false); assert.equal(f.renders.length, 0);
  assert.match(f.els["publication-status"].textContent, /Turn it off/);
  f.els["allow-html"].checked = false;
  const pending = f.controller.prepareSvg(), call = f.renders[0];
  call.reject(Object.assign(new Error("SVG is unavailable in this package"), { code: "UNSUPPORTED_WASM_PACKAGE" }));
  assert.equal(await pending, false); assert.equal(call.client.disposed, true);
  assert.match(f.els["publication-status"].textContent, /UNSUPPORTED_WASM_PACKAGE/);
  assert.equal(f.els.markdown.value, "# Original"); assert.equal(f.urls.length, 0);
});

function physicalRenderer(settings) {
  return createWorkerRenderer({ ...settings, workerFactory: () => {
    const worker = new Worker(new URL("./demo_publication_worker_fixture.mjs", import.meta.url)), callbacks = new Map();
    return {
      postMessage: (message, transfer) => worker.postMessage(message, transfer), terminate: () => worker.terminate(),
      addEventListener(type, callback) {
        const wrapped = type === "message" ? data => callback({ data }) : error => callback({ error });
        callbacks.set(callback, wrapped); worker.on(type, wrapped);
      },
      removeEventListener(type, callback) { worker.off(type, callbacks.get(callback)); callbacks.delete(callback); },
    };
  } });
}
test("physical workers retain format-specific settings and identical authorized image bytes", async t => {
  const f = fixture(t, { rendererFactory: physicalRenderer }), bytes = new TextEncoder().encode("<svg/>"), receipts = {};
  f.input("# Diagram\n\n![A](chart.svg)"); f.input("commonmark-gfm", "profile");
  f.input("a4", "pdf-page-size"); f.input("landscape", "pdf-orientation"); f.input("23.25", "pdf-margin-left");
  f.input("p{color:green}", "custom-css");
  await f.controller.importImages([new File([bytes], "chart.svg")]);
  for (const format of ["epub", "svg", "pdf"]) {
    assert.equal(await (format === "pdf" ? f.controller.downloadPdf() : prepare(f, format)), true);
    const blob = f.urls.at(-1).blob; receipts[format] = JSON.parse(await blob.text());
    assert.equal(blob.type, { epub: "application/epub+zip", svg: "image/svg+xml", pdf: "application/pdf" }[format]);
    assert.equal(receipts[format].source, f.els.markdown.value);
    assert.equal(receipts[format].settings.profile, "commonmark-gfm");
    assert(receipts[format].images.every(image => image.sha256 === createHash("sha256").update(bytes).digest("hex")));
  }
  assert.deepEqual(receipts.epub.images, receipts.svg.images); assert.deepEqual(receipts.pdf.images, receipts.svg.images);
  assert.equal(receipts.epub.settings.author, "Author"); assert.equal(receipts.epub.settings.customCss, "p{color:green}");
  assert.deepEqual(Object.keys(receipts.svg.settings).sort(), ["darkMode", "font", "profile"]);
  assert.deepEqual(receipts.pdf.settings.page, { size: { widthPt: 297 * 72 / 25.4, heightPt: 210 * 72 / 25.4 },
    margins: { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 23.25 } });
});

// Use the production controller with the worker API as the explicit test boundary.
function verificationFixture(t, { missing = false } = {}) {
  const checks = [], exports = [];
  const f = fixture(t, { rendererFactory: () => {
    const client = { disposed: false, dispose() { this.disposed = true; },
      render(kind, source, options, controls) {
        const pending = deferred();
        exports.push({ kind, source, options, controls, ...pending });
        return pending.promise;
      },
    };
    if (!missing) client.verifyPdf = (source, options, controls) => {
      const pending = deferred();
      checks.push({ source, options, controls, client, ...pending });
      return pending.promise;
    };
    return client;
  }});
  const report = { schema_version: "1", target: "pdf", verdict: "findings", page_count: 1,
    digest: "fnv1a64:0123456789abcdef", anchors: { resolved: 0, unresolved: ["lost"] },
    findings: [{ code: "unresolved_anchor", detail: "Missing #lost" }],
    pages: [{ number: 1, runs: [{ text: "# Guide", x: 24, y: 600, size: 12, kind: "heading" }] }] };
  return { ...f, checks, exports, report };
}

test("main demo verifies the same PDF inputs as export and downloads the complete report", async t => {
  const f = verificationFixture(t);
  f.input("# Guide"); f.input("commonmark-gfm", "profile"); f.input("a4", "pdf-page-size");
  f.input("landscape", "pdf-orientation"); f.input("24", "pdf-margin-left");
  await f.controller.importImages([new File(["pixels"], "image.png")]);
  const pdf = f.controller.downloadPdf(), published = f.exports[0];
  published.resolve(f.output(published)); assert.equal(await pdf, true);
  const verified = f.controller.verifyPdf(), call = f.checks[0];
  assert.equal(call.source, published.source); assert.deepEqual(call.options, published.options);
  assert.equal(call.options.pdfImages.length, 2); assert.equal(call.options.page.margins.leftPt, 24);
  assert.equal(f.els["verify-pdf"].disabled, true);
  call.resolve(f.report); assert.equal(await verified, true); assert.equal(call.client.disposed, true);
  const link = f.els["download-publication"];
  assert.equal(link.download, "original-title.pdf-verification.json");
  assert.equal(f.urls.at(-1).blob.type, "application/json");
  assert.deepEqual(JSON.parse(await f.urls.at(-1).blob.text()), f.report);
  assert.match(f.els["publication-status"].textContent, /1 page, 1 finding.*Missing #lost/);
  assert.equal(f.els["verify-pdf"].disabled, false);
  f.els["pdf-margin-left"].value = "32"; link.click();
  assert.equal(link.hidden, true); assert(f.revoked.includes(f.urls.at(-1).url));
});

test("PDF verification reports never survive a cancelled or superseded check", async t => {
  const f = verificationFixture(t);
  const stale = f.controller.verifyPdf(), first = f.checks[0];
  f.input("revised"); assert.equal(first.controls.signal.aborted, true);
  const fresh = f.controller.verifyPdf(), second = f.checks[1];
  second.resolve(f.report); assert.equal(await fresh, true);
  const link = f.els["download-publication"], current = link.href;
  first.resolve(f.report); assert.equal(await stale, false);
  assert.equal(link.href, current); assert.equal(f.urls.length, 1);
  const stopped = f.controller.verifyPdf(), last = f.checks[2];
  assert.equal(f.controller.cancelPublication(), true); last.resolve(f.report);
  assert.equal(await stopped, false); assert.equal(link.hidden, true);
  assert.equal(f.urls.length, 1); assert.deepEqual(f.revoked, [current]);
});

test("invalid PDF geometry and incompatible verification packages preserve the source", async t => {
  const f = verificationFixture(t, { missing: true });
  f.input("custom", "pdf-page-size"); f.input("144", "pdf-width");
  assert.equal(await f.controller.verifyPdf(), false);
  assert.match(f.els["publication-status"].textContent, /INVALID_OPTIONS/);
  f.input("a5", "pdf-page-size");
  assert.equal(await f.controller.verifyPdf(), false);
  assert.match(f.els["publication-status"].textContent, /UNSUPPORTED_WASM_PACKAGE/);
  assert.equal(f.els.markdown.value, "# Original"); assert.equal(f.urls.length, 0);
});
