import assert from "node:assert/strict";
import test from "node:test";
import { Worker, MessageChannel } from "node:worker_threads";
import { createDemoWorkspaceWorker, installDemoWorkspaceWorker, DEMO_WORKSPACE_LIMITS as limits } from "../demo/demo_workspace_worker.mjs";
import { WORKER_PROTOCOL } from "../worker_transport.mjs";

const encode = value => new TextEncoder().encode(value);
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
function fixture(t, { load, maxOutputBytes, timeoutMs } = {}) {
  const outgoing = [], calls = [], listeners = new Map(); let terminated = 0, starts = 0;
  const endpoint = new EventTarget();
  endpoint.postMessage = (message, transfer) => {
    const data = structuredClone(message, { transfer });
    queueMicrotask(() => listeners.get("message")?.({ data }));
  };
  const defaultLoad = async () => ({ render(source, options) {
    calls.push({ source, options });
    return { format: "interactive-html", mimeType: "text/html;charset=utf-8", extension: "html", sourceLength: encode(source).length,
      bytes: encode("<html>workspace</html>"), diagnostics: [] };
  } });
  const uninstall = installDemoWorkspaceWorker(endpoint, load ?? defaultLoad);
  const worker = {
    postMessage(message, transfer) { outgoing.push(structuredClone(message));
      const event = new Event("message"); Object.defineProperty(event, "data", { value: structuredClone(message, { transfer }) });
      queueMicrotask(() => endpoint.dispatchEvent(event));
    },
    addEventListener(type, callback) { listeners.set(type, callback); },
    removeEventListener(type) { listeners.delete(type); },
    terminate() { terminated++; uninstall(); },
  };
  const client = createDemoWorkspaceWorker({ workerFactory: () => { starts++; return worker; }, maxOutputBytes, timeoutMs });
  t.after(() => client.dispose());
  return { client, worker, outgoing, calls, listeners, get terminated() { return terminated; }, get starts() { return starts; } };
}
function threaded(t, workerData) {
  let terminated = 0;
  const { port1, port2 } = new MessageChannel();
  const started = new Promise(resolve => port1.once("message", resolve));
  t.after(() => port1.close());
  const client = createDemoWorkspaceWorker({ workerFactory: () => {
    const worker = new Worker(new URL("./demo_workspace_worker_fixture.mjs", import.meta.url), {
      workerData: { ...workerData, started: port2 }, transferList: [port2],
    });
    const callbacks = new Map();
    return {
      postMessage: (message, transfer) => worker.postMessage(message, transfer),
      addEventListener(type, callback) { const wrapped = type === "message" ? data => callback({ data }) : callback;
        callbacks.set(callback, wrapped); worker.on(type, wrapped); },
      removeEventListener(type, callback) { worker.off(type, callbacks.get(callback)); callbacks.delete(callback); },
      terminate() { terminated++; return worker.terminate(); },
    };
  } });
  t.after(() => client.dispose());
  return { client, started, get terminated() { return terminated; } };
}
const jsonBlock = (html, id) => {
  const open = `<script type="application/json" id="${id}">`, start = html.indexOf(open);
  assert(start >= 0, id);
  return JSON.parse(html.slice(start + open.length, html.indexOf("</script>", start)));
};

test("physical worker uses production entry and exporter to preserve exact source, settings, runtime and unused image bindings", async t => {
  const f = threaded(t), source = "\uFEFF# Offline café😀\r\n\r\n![A](chart.svg)\r\n";
  const backing = Uint8Array.of(9, 1, 2, 3, 8);
  const options = { font: "serif", darkMode: "disabled", title: "Offline", author: "Author", codeLineNumbers: true,
    metadataEpochSeconds: 1700000000, allowRawHtml: false,
    pdfImages: [{ destination: "chart.svg", bytes: new DataView(backing.buffer, 1, 3) }, { destination: "unused.png", bytes: backing.subarray(2, 4) }] };
  const pending = f.client.prepare(source, options);
  backing.fill(0); options.title = "Later edit"; options.pdfImages[0].destination = "later.svg";
  const result = await pending, html = result.text(), payload = jsonBlock(html, "fmd-native-runtime");
  assert.equal(jsonBlock(html, "fmd-raw-source"), source);
  assert.equal(result.sourceLength, encode(source).length);
  assert.equal(payload.options.title, "Offline"); assert.equal(payload.options.author, "Author");
  assert.equal(payload.options.font, "serif"); assert.equal(payload.options.darkMode, "disabled");
  assert.equal(payload.options.codeLineNumbers, true); assert.equal(payload.options.metadataEpochSeconds, 1700000000);
  assert.deepEqual(payload.images, [{ destination: "chart.svg", bytes: "AQID" }, { destination: "unused.png", bytes: "AgM=" }]);
  assert.equal(payload.wasm, "AGFzbQEAAAA="); assert.match(payload.bindings, /renderInteractiveHtmlConfigured/);
  assert.match(html, /id="fmd-native-bootstrap"/); assert.match(html, /sandbox="allow-same-origin"/);
  assert.match(html, /default-src 'none'/); assert.equal(await result.blob().text(), html);
  assert.equal(backing.buffer.byteLength, 5, "caller storage was never transferred");
});

test("older generated package is an explicit capability error, never a lightweight workspace", async t => {
  const f = threaded(t, { oldPackage: true });
  await assert.rejects(f.client.prepare("Source"), error => error.code === "UNSUPPORTED_WASM_PACKAGE" && /renderInteractiveHtmlConfigured/.test(error.message));
  assert.equal(f.client.disposed, true); assert.equal(f.terminated, 1);
});

test("active cancellation terminates a physical worker waiting for runtime and cannot deliver late bytes", async t => {
  const f = threaded(t, { hang: true }), abort = new AbortController();
  const pending = f.client.prepare("Source", {}, { signal: abort.signal });
  assert.equal(await f.started, "runtime-started", "the real worker is waiting for runtime, not merely queued to start");
  abort.abort();
  await assert.rejects(pending, { code: "ABORTED" });
  assert.equal(f.client.disposed, true); assert.equal(f.terminated, 1);
});

for (const [name, source, options, code] of [
  ["custom CSS", "Source", { customCss: "body{color:red}" }, "UNSUPPORTED_WORKSPACE_SETTINGS"],
  ["raw HTML trust", "Source", { allowRawHtml: true }, "UNSUPPORTED_WORKSPACE_SETTINGS"],
  ["unknown native settings", "Source", { arbitraryRuntime: "https://example.invalid/" }, "UNSUPPORTED_WORKSPACE_SETTINGS"],
  ["oversized source", "x".repeat(limits.sourceBytes + 1), {}, "WORKSPACE_INPUT_LIMIT"],
  ["malformed Unicode", "\ud800", {}, "INVALID_UNICODE"],
  ["duplicate bindings", "Source", { pdfImages: [{ destination: "a", bytes: Uint8Array.of(1) }, { destination: "a", bytes: Uint8Array.of(2) }] }, "INVALID_OPTIONS"],
  ["excessive binding count", "Source", { pdfImages: new Array(limits.imageCount + 1) }, "WORKSPACE_INPUT_LIMIT"],
  ["shared bytes", "Source", { pdfImages: [{ destination: "a", bytes: new Uint8Array(new SharedArrayBuffer(1)) }] }, "INVALID_OPTIONS"],
]) test(`${name} refuses before worker startup or runtime fetch`, async t => {
  const f = fixture(t);
  await assert.rejects(f.client.prepare(source, options), { code });
  assert.equal(f.starts, 0); assert.equal(f.outgoing.length, 0);
});

test("image metadata is admitted without invoking accessors, before copying any bytes", async t => {
  const f = fixture(t); let reads = 0;
  const options = { pdfImages: [{ destination: "a", get bytes() { reads++; return Uint8Array.of(1); } }] };
  await assert.rejects(f.client.prepare("Source", options), { code: "UNSUPPORTED_WORKSPACE_SETTINGS" });
  assert.equal(reads, 0); assert.equal(f.starts, 0);
});

test("image destination aggregate follows the native package cap", async t => {
  const f = fixture(t), pdfImages = Array.from({ length: 9 }, (_, index) => ({ destination: `${index}${"x".repeat(8190)}`, bytes: Uint8Array.of(1) }));
  await assert.rejects(f.client.prepare("Source", { pdfImages }), error => error.code === "WORKSPACE_INPUT_LIMIT" && /64 KiB/.test(error.message));
  assert.equal(f.starts, 0);
});

test("deadline closes the endpoint and late acknowledgments cannot publish", async t => {
  const delayed = gate(), f = fixture(t, { timeoutMs: 5, load: () => delayed.promise });
  const pending = f.client.prepare("Source");
  await assert.rejects(pending, { code: "TIMEOUT" });
  assert.equal(f.terminated, 1); assert.equal(f.client.disposed, true);
  delayed.resolve({ render() { throw new Error("No publication after closed endpoint"); } });
  await new Promise(resolve => setImmediate(resolve));
});

test("output byte cap is checked before transferring the exporter's buffer", async t => {
  const bytes = encode("A workspace larger than five bytes");
  const f = fixture(t, { maxOutputBytes: 5, load: async () => ({ render: () => ({ format: "interactive-html", mimeType: "text/html;charset=utf-8",
    extension: "html", sourceLength: 6, bytes, diagnostics: [] }) }) });
  await assert.rejects(f.client.prepare("Source"), { code: "WORKSPACE_INPUT_LIMIT" });
  assert.equal(bytes.buffer.byteLength, bytes.length); assert.equal(bytes[0], 65);
});

test("server validates request methods and data before loading executable runtime", async () => {
  let loaded = 0; const messages = [], endpoint = new EventTarget();
  endpoint.postMessage = message => messages.push(message);
  const close = installDemoWorkspaceWorker(endpoint, () => { loaded++; });
  for (const [id, method, args] of [[1, "fetch", ["https://example.invalid/"]], [2, "prepare-workspace", ["Source", { customCss: "p{}" }, 1000]]]) {
    const event = new Event("message"); Object.defineProperty(event, "data", { value: { protocol: WORKER_PROTOCOL, id, method, args } });
    endpoint.dispatchEvent(event); await new Promise(resolve => setImmediate(resolve));
  }
  assert.equal(loaded, 0); assert.equal(messages.length, 2); assert(messages.every(message => message.ok === false)); close();
});

test("incorrect result identity closes the client without exposing helpers", async t => {
  const delayed = gate(), f = fixture(t, { load: () => delayed.promise });
  const pending = f.client.prepare("Source");
  f.listeners.get("message")({ data: { protocol: WORKER_PROTOCOL, id: f.outgoing[0].id, ok: true, state: null,
    value: { format: "interactive-html", mimeType: "text/html;charset=utf-8", extension: "html", sourceLength: 0, bytes: Uint8Array.of(1), diagnostics: [] } } });
  await assert.rejects(pending, { code: "WORKER_PROTOCOL_ERROR" }); assert.equal(f.terminated, 1);
});
