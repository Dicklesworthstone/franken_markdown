import assert from "node:assert/strict";
import test from "node:test";
import { createWorkerRenderer } from "./document_worker.mjs";
import { WORKER_PROTOCOL } from "./worker_transport.mjs";

// Exercise the production facade and transport. This endpoint records the
// dispatched request; it is not a real worker, renderer, codec or WASM proof.
const formats = {
  html: ["text/html; charset=utf-8", "html"],
  pdf: ["application/pdf", "pdf"],
  svg: ["image/svg+xml", "svg"],
  epub: ["application/epub+zip", "epub"],
  "interactive-html": ["text/html; charset=utf-8", "html"],
};
function recordingEndpoint(onListen = () => {}) {
  const listeners = new Map();
  return {
    sent: [], transfers: [], terminated: 0,
    addEventListener(kind, listener) {
      listeners.set(kind, listener);
      onListen(kind);
    },
    removeEventListener(kind) { listeners.delete(kind); },
    terminate() { this.terminated++; },
    postMessage(message, transfer) {
      this.sent.push(message);
      this.transfers.push(transfer);
      const [format, source, options] = message.args;
      const output = (kind, selected) => ({
        format: kind, mimeType: formats[kind][0], extension: formats[kind][1],
        sourceLength: source.length, bytes: new Uint8Array([42]), diagnostics: [],
        ...(selected.profile === undefined ? {} : { profile: selected.profile }),
      });
      const value = format === "publish" ? {
        format: "document-publication", sourceLength: source.length,
        totalBytes: options.length,
        outputs: options.map(target => output(target.format, target.options)),
      } : output(format, options);
      queueMicrotask(() => listeners.get("message")?.({ data: {
        protocol: WORKER_PROTOCOL, id: message.id, ok: true, state: null, value,
      } }));
    },
  };
}
const slots = ["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"];
function resources() {
  const image = new Uint8Array([99, 1, 2, 3, 99]);
  const font = new Uint8Array([88, 4, 5, 6, 7, 88]);
  return {
    image, font,
    options: {
      pdfImages: [{ destination: "logo", bytes: new DataView(image.buffer, 1, 3) }],
      fontAssets: slots.map((slot, index) => ({
        slot, bytes: new DataView(font.buffer, 1, 4), weight: 400 + index,
      })),
    },
  };
}
function capturedResources(options, original) {
  assert.deepEqual([...options.pdfImages[0].bytes], [1, 2, 3]);
  assert.equal(options.pdfImages[0].destination, "logo");
  assert.notEqual(options.pdfImages[0].bytes.buffer, original.image.buffer);
  assert.equal(options.pdfImages[0].bytes.byteOffset, 0);
  assert.equal(options.pdfImages[0].bytes.buffer.byteLength, 3);
  assert.deepEqual(options.fontAssets.map(asset => asset.slot), slots);
  for (const [index, asset] of options.fontAssets.entries()) {
    assert.deepEqual([...asset.bytes], [4, 5, 6, 7]);
    assert.notEqual(asset.bytes.buffer, original.font.buffer);
    assert.equal(asset.bytes.buffer.byteLength, 4);
    assert.equal(asset.weight, 400 + index);
  }
}
for (const stage of ["factory", "listener"]) {
  test("first render owns exact image and font ranges before " + stage + " callbacks", async t => {
    for (const format of Object.keys(formats)) {
      const original = resources();
      let mutated = false;
      const mutate = () => {
        if (mutated) return;
        mutated = true;
        original.image.fill(9);
        original.font.fill(8);
        original.options.pdfImages[0].destination = "changed";
        original.options.fontAssets[0].weight = 900;
      };
      const endpoint = recordingEndpoint(kind => {
        if (stage === "listener" && kind === "message") mutate();
      });
      const renderer = createWorkerRenderer({ timeoutMs: 0, workerFactory() {
        if (stage === "factory") mutate();
        return endpoint;
      } });
      t.after(() => renderer.dispose());
      await renderer.render(format, "captured", original.options);
      assert.equal(mutated, true);
      assert.equal(endpoint.sent.length, 1);
      capturedResources(endpoint.sent[0].args[2], original);
      assert.equal(endpoint.transfers[0].length, 6);
      assert.ok(endpoint.transfers[0].every(buffer =>
        buffer !== original.image.buffer && buffer !== original.font.buffer));
      assert.deepEqual([...original.image], [9, 9, 9, 9, 9]);
      assert.deepEqual([...original.font], [8, 8, 8, 8, 8, 8]);
      assert.equal(renderer.pendingBytes, 0);
      assert.equal(renderer.pendingOperations, 0);
    }
  });

  test("first render survives caller buffer detachment during " + stage + " callbacks", async t => {
    const original = resources();
    let detached = false;
    const detach = () => {
      if (detached) return;
      detached = true;
      structuredClone(original.image.buffer, { transfer: [original.image.buffer] });
      structuredClone(original.font.buffer, { transfer: [original.font.buffer] });
    };
    const endpoint = recordingEndpoint(kind => {
      if (stage === "listener" && kind === "message") detach();
    });
    const renderer = createWorkerRenderer({ timeoutMs: 0, workerFactory() {
      if (stage === "factory") detach();
      return endpoint;
    } });
    t.after(() => renderer.dispose());
    await renderer.renderPdf("captured", original.options);
    assert.equal(original.image.byteLength, 0);
    assert.equal(original.font.byteLength, 0);
    capturedResources(endpoint.sent[0].args[2], original);
    assert.equal(renderer.disposed, false);
  });

  test("first publication owns and deduplicates exact ranges before " + stage + " callbacks", async t => {
    const original = resources();
    const selected = [
      { format: "pdf", options: original.options },
      { format: "html", options: {
        pdfImages: [{ destination: "cover", bytes: original.image.subarray(1, 4) }],
      } },
      { format: "epub", options: {
        pdfImages: [{ destination: "detail", bytes: original.image.subarray(2, 4) }],
      } },
      { format: "svg" },
      { format: "interactive-html" },
    ];
    let mutated = false;
    const mutate = () => {
      if (mutated) return;
      mutated = true;
      original.image.fill(9);
      original.font.fill(8);
      selected.reverse();
    };
    const endpoint = recordingEndpoint(kind => {
      if (stage === "listener" && kind === "message") mutate();
    });
    const renderer = createWorkerRenderer({ timeoutMs: 0, workerFactory() {
      if (stage === "factory") mutate();
      return endpoint;
    } });
    t.after(() => renderer.dispose());
    const publication = await renderer.publish("captured", selected);
    assert.deepEqual(publication.outputs.map(output => output.format),
      ["pdf", "html", "epub", "svg", "interactive-html"]);
    const [pdf, html, epub, svg, interactive] = endpoint.sent[0].args[2];
    capturedResources(pdf.options, original);
    assert.equal(html.options.pdfImages[0].destination, "cover");
    assert.equal(html.options.pdfImages[0].bytes, pdf.options.pdfImages[0].bytes);
    assert.deepEqual([...epub.options.pdfImages[0].bytes], [2, 3]);
    assert.notEqual(epub.options.pdfImages[0].bytes.buffer, pdf.options.pdfImages[0].bytes.buffer);
    assert.equal(epub.options.pdfImages[0].bytes.buffer.byteLength, 2);
    assert.ok(pdf.options.fontAssets.every(asset => asset.bytes === pdf.options.fontAssets[0].bytes));
    for (const target of [html, epub, svg, interactive]) assert.equal(target.options.fontAssets, undefined);
    for (const target of [svg, interactive]) assert.equal(target.options.pdfImages, undefined);
    assert.equal(endpoint.transfers[0].length, 3,
      "one image range, one shared font range and one overlapping image range");
    assert.equal(new Set(endpoint.transfers[0]).size, 3);
    assert.equal(renderer.pendingBytes, 0);
  });
}

test("first publication survives detached shared resources without widening their ranges", async t => {
  const original = resources(), endpoint = recordingEndpoint();
  const renderer = createWorkerRenderer({ timeoutMs: 0, workerFactory() {
    structuredClone(original.image.buffer, { transfer: [original.image.buffer] });
    structuredClone(original.font.buffer, { transfer: [original.font.buffer] });
    return endpoint;
  } });
  t.after(() => renderer.dispose());
  await renderer.publish("captured", [
    { format: "html", options: original.options },
    { format: "pdf", options: original.options },
  ]);
  const [html, pdf] = endpoint.sent[0].args[2];
  capturedResources(html.options, original);
  capturedResources(pdf.options, original);
  assert.equal(html.options.pdfImages[0].bytes, pdf.options.pdfImages[0].bytes);
  assert.equal(endpoint.transfers[0].length, 2);
  assert.equal(original.image.byteLength, 0);
  assert.equal(original.font.byteLength, 0);
});

test("invalid, over-budget and pre-aborted first requests remain lazy", async t => {
  let starts = 0;
  const renderer = createWorkerRenderer({ maxPendingBytes: 600, timeoutMs: 0,
    workerFactory() { starts++; throw new Error("must stay lazy"); } });
  t.after(() => renderer.dispose());
  await assert.rejects(renderer.render("unknown", "x"), { code: "INVALID_OPTIONS" });
  await assert.rejects(renderer.renderPdf("x", { unknown: true }), { code: "INVALID_OPTIONS" });
  await assert.rejects(renderer.renderPdf("x", {
    pdfImages: [{ destination: "large", bytes: new Uint8Array(1024) }],
  }), { code: "WORKER_QUEUE_FULL" });
  await assert.rejects(renderer.publish("x", [{ format: "html" }, { format: "pdf" }]),
    { code: "WORKER_QUEUE_FULL" });
  await assert.rejects(renderer.renderHtml("x", {}, { signal: { aborted: true } }),
    { code: "ABORTED" });
  await assert.rejects(renderer.publish("x", [{ format: "html" }], { signal: { aborted: true } }),
    { code: "ABORTED" });
  assert.equal(starts, 0);
  assert.equal(renderer.pendingBytes, 0);
  assert.equal(renderer.disposed, false);
});

test("failed first startup releases its captured resources and a retry captures fresh ones", async t => {
  const original = resources();
  const endpoint = recordingEndpoint();
  const failed = Object.assign(recordingEndpoint(), {
    addEventListener() { throw new Error("cannot initialize"); },
  });
  let starts = 0;
  const renderer = createWorkerRenderer({ timeoutMs: 0, workerFactory() {
    starts++;
    original.image.fill(9);
    return starts === 1 ? failed : endpoint;
  } });
  t.after(() => renderer.dispose());
  await assert.rejects(renderer.renderPdf("captured", original.options), { code: "WORKER_FAILED" });
  assert.ok(failed.terminated >= 1);
  assert.equal(renderer.disposed, false);
  assert.equal(renderer.pendingBytes, 0);
  original.image.set([99, 1, 2, 3, 99]);
  await renderer.renderPdf("captured", original.options);
  capturedResources(endpoint.sent[0].args[2], original);
  assert.equal(starts, 2);
  assert.equal(renderer.pendingOperations, 0);
});

test("disposal during worker startup discards captured resources without dispatch", async () => {
  const original = resources(), endpoint = recordingEndpoint();
  const renderer = createWorkerRenderer({ timeoutMs: 0, workerFactory() {
    renderer.dispose();
    return endpoint;
  } });
  await assert.rejects(renderer.renderPdf("captured", original.options), { code: "SESSION_DISPOSED" });
  assert.equal(renderer.disposed, true);
  assert.equal(endpoint.sent.length, 0);
  assert.equal(endpoint.terminated, 1);
  assert.equal(renderer.pendingBytes, 0);
  assert.deepEqual([...original.image], [99, 1, 2, 3, 99]);
});
