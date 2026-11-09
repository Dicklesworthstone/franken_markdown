// Scoped transport for the primary demo's complete native workspace download.
// This does not change document-worker's lightweight interactive-html format.
import { FlowWorkerError, OwnedWorkerRpc, serveOwnedWorker } from "../worker_transport.mjs";

export const DEMO_WORKSPACE_LIMITS = Object.freeze({
  sourceBytes: 4 * 1024 * 1024, imageBytes: 8 * 1024 * 1024,
  assetBytes: 48 * 1024 * 1024, imageCount: 512, destinationBytes: 8192,
  totalDestinationBytes: 65536, requestBytes: 64 * 1024 * 1024,
  outputBytes: 256 * 1024 * 1024,
});
const fail = (code, message) => { throw new FlowWorkerError(code, message); };
function record(value, keys, label) {
  if (!value || typeof value !== "object" || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail("INVALID_OPTIONS", `${label} must be a plain object`);
  const result = {};
  for (const key of Reflect.ownKeys(value)) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (!keys.includes(key) || !descriptor || !Object.hasOwn(descriptor, "value"))
      fail("UNSUPPORTED_WORKSPACE_SETTINGS", `Unsupported ${label} field or accessor; this workspace cannot preserve it.`);
    if (descriptor.value !== undefined) result[key] = descriptor.value;
  }
  return result;
}
function text(value, maximum, label) {
  if (typeof value !== "string" || value.length > maximum)
    fail("WORKSPACE_INPUT_LIMIT", `${label} must be text within its byte limit.`);
  let size = 0;
  for (const character of value) {
    const point = character.codePointAt(0);
    if (point >= 0xd800 && point <= 0xdfff) fail("INVALID_UNICODE", `${label} contains an unpaired surrogate.`);
    size += point < 128 ? 1 : point < 2048 ? 2 : point < 65536 ? 3 : 4;
    if (size > maximum) fail("WORKSPACE_INPUT_LIMIT", `${label} exceeds its byte limit.`);
  }
  return size;
}
function byteView(value, maximum) {
  let bytes;
  try {
    const view = ArrayBuffer.isView(value), buffer = view ? value.buffer : value;
    const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(buffer);
    bytes = new Uint8Array(buffer, view ? value.byteOffset : 0, view ? value.byteLength : length);
  } catch { fail("INVALID_OPTIONS", "Workspace bytes must use a private, non-detached ArrayBuffer."); }
  if (!bytes.length || bytes.length > maximum) fail("WORKSPACE_INPUT_LIMIT", "Workspace bytes exceed their limit or are empty.");
  return bytes;
}
function input(source, options) {
  const sourceLength = text(source, DEMO_WORKSPACE_LIMITS.sourceBytes, "Markdown");
  const captured = record(options, ["font", "darkMode", "title", "author", "codeLineNumbers", "metadataEpochSeconds", "pdfImages", "allowRawHtml", "customCss"], "workspace option");
  if (captured.customCss !== undefined) {
    text(captured.customCss, 1024 * 1024, "Custom CSS");
    if (captured.customCss.trim()) fail("UNSUPPORTED_WORKSPACE_SETTINGS", "Editable workspaces cannot preserve custom CSS. Clear Custom CSS before preparing the workspace.");
    delete captured.customCss;
  }
  if (captured.allowRawHtml !== undefined && captured.allowRawHtml !== false)
    fail("UNSUPPORTED_WORKSPACE_SETTINGS", "Editable workspaces require safe Markdown. Turn off Allow raw HTML before preparing the workspace.");
  for (const [key, choices] of [["font", ["sans", "serif"]], ["darkMode", ["auto", "disabled"]]])
    if (captured[key] !== undefined && !choices.includes(captured[key])) fail("INVALID_OPTIONS", `Invalid workspace ${key}.`);
  for (const key of ["title", "author"]) if (captured[key] !== undefined) text(captured[key], 4096, key);
  if (captured.codeLineNumbers !== undefined && typeof captured.codeLineNumbers !== "boolean")
    fail("INVALID_OPTIONS", "Workspace code line numbers must be boolean.");
  if (captured.metadataEpochSeconds !== undefined && (!Number.isSafeInteger(captured.metadataEpochSeconds) || captured.metadataEpochSeconds < 0))
    fail("INVALID_OPTIONS", "Workspace metadata time must be a nonnegative safe integer.");
  const images = captured.pdfImages ?? [];
  if (!Array.isArray(images) || images.length > DEMO_WORKSPACE_LIMITS.imageCount)
    fail("WORKSPACE_INPUT_LIMIT", "Too many workspace image bindings.");
  let total = 0, names = 0, charge = 1024 + 2 * source.length;
  const keys = new Set(), owned = [];
  for (let index = 0; index < images.length; index++) {
    const descriptor = Object.getOwnPropertyDescriptor(images, String(index));
    if (!descriptor || !Object.hasOwn(descriptor, "value")) fail("INVALID_OPTIONS", "Image bindings must be a dense data array.");
    const image = record(descriptor.value, ["destination", "bytes"], "image binding");
    names += text(image.destination, DEMO_WORKSPACE_LIMITS.destinationBytes, "Image destination");
    if (!image.destination || image.destination.trim() !== image.destination || /[\u0000-\u001f\u007f-\u009f]/u.test(image.destination) || keys.has(image.destination))
      fail("INVALID_OPTIONS", "Workspace image destinations must be unique, nonempty, control-free and have no surrounding whitespace.");
    if (names > DEMO_WORKSPACE_LIMITS.totalDestinationBytes)
      fail("WORKSPACE_INPUT_LIMIT", "Editable workspace image destinations exceed 64 KiB; choose fewer images or shorter paths.");
    keys.add(image.destination);
    image.bytes = byteView(image.bytes, DEMO_WORKSPACE_LIMITS.imageBytes);
    total += image.bytes.byteLength;
    if (total > DEMO_WORKSPACE_LIMITS.assetBytes) fail("WORKSPACE_INPUT_LIMIT", "Workspace image bindings exceed 48 MiB.");
    charge += 256 + image.bytes.byteLength + 2 * image.destination.length;
    owned.push(image);
  }
  captured.pdfImages = owned;
  for (const value of Object.values(captured)) if (typeof value === "string") charge += 64 + value.length * 2;
  if (charge > DEMO_WORKSPACE_LIMITS.requestBytes) fail("WORKSPACE_INPUT_LIMIT", "Workspace request exceeds 64 MiB.");
  return { source, sourceLength, options: captured, charge };
}
function output(value, sourceLength, maximum) {
  if (!value || value.format !== "interactive-html" || value.mimeType !== "text/html;charset=utf-8" || value.extension !== "html"
    || value.sourceLength !== sourceLength || !(value.bytes instanceof Uint8Array) || !Array.isArray(value.diagnostics) || value.diagnostics.length > 2048)
    fail("WORKER_PROTOCOL_ERROR", "Invalid native workspace output.");
  const bytes = byteView(value.bytes, maximum), diagnostics = [];
  let size = 0;
  for (const finding of value.diagnostics) {
    if (!finding || !["warning", "error"].includes(finding.severity) || typeof finding.message !== "string"
      || !Number.isSafeInteger(finding.start) || !Number.isSafeInteger(finding.end)
      || finding.start < 0 || finding.end < finding.start || finding.end > sourceLength
      || (finding.code !== undefined && (typeof finding.code !== "string" || finding.code.length > 128)))
      fail("WORKER_PROTOCOL_ERROR", "Invalid native workspace diagnostic.");
    size += finding.message.length + (finding.code?.length ?? 0);
    if (size > 128 * 1024) fail("WORKER_PROTOCOL_ERROR", "Native workspace diagnostics exceed their text limit.");
    diagnostics.push({ severity: finding.severity, start: finding.start, end: finding.end, message: finding.message,
      ...(finding.code === undefined ? {} : { code: finding.code }) });
  }
  return { format: "interactive-html", mimeType: "text/html;charset=utf-8", extension: "html", sourceLength, bytes, diagnostics };
}
function outputLimit(value) {
  if (!Number.isSafeInteger(value) || value < 1 || value > DEMO_WORKSPACE_LIMITS.outputBytes)
    fail("INVALID_OPTIONS", "Invalid native workspace output byte limit.");
  return value;
}

/** Lazy disposable worker; supported calls contain only source/settings/bytes.
 * The factory is a trusted host seam, never selected by Markdown or messages. */
export function createDemoWorkspaceWorker(options = {}) {
  const settings = record(options, ["workerFactory", "timeoutMs", "maxOutputBytes"], "worker option");
  const maximum = outputLimit(settings.maxOutputBytes ?? DEMO_WORKSPACE_LIMITS.outputBytes);
  const timeoutMs = settings.timeoutMs ?? 30000;
  if (!Number.isInteger(timeoutMs) || timeoutMs < 0 || timeoutMs > 2147483647)
    fail("INVALID_OPTIONS", "Invalid workspace worker deadline.");
  if (settings.workerFactory !== undefined && typeof settings.workerFactory !== "function") fail("INVALID_OPTIONS", "workerFactory must be a function.");
  const factory = settings.workerFactory ?? (() => {
    if (typeof Worker !== "function") fail("WORKER_UNAVAILABLE", "This browser cannot start the editable workspace worker.");
    return new Worker(new URL("./demo_workspace_worker_entry.js", import.meta.url), { type: "module", name: "franken-markdown-workspace" });
  });
  let rpc = null, disposed = false, starting = false;
  return Object.freeze({
    get disposed() { return disposed || (rpc?.closed ?? false); },
    prepare(source, options = {}, controls = {}) {
      try {
        if (disposed || rpc?.closed) fail("SESSION_DISPOSED", "Workspace preparation worker is closed.");
        if (starting) fail("WORKER_STARTING", "workerFactory cannot reenter workspace preparation.");
        if (controls?.signal?.aborted) fail("ABORTED", "Workspace preparation was cancelled before dispatch.");
        const captured = input(source, options);
        if (!rpc) {
          let worker;
          starting = true;
          try {
            worker = factory();
            if (disposed) fail("SESSION_DISPOSED", "Workspace worker was disposed during startup.");
            rpc = new OwnedWorkerRpc(worker, { maxPendingOperations: 1, maxPendingBytes: DEMO_WORKSPACE_LIMITS.requestBytes, timeoutMs });
          } catch (error) {
            try { worker?.terminate()?.catch?.(() => {}); } catch { /* Stop the owned endpoint even after failed initialization. */ }
            throw error;
          } finally { starting = false; }
        }
        return rpc.request("prepare-workspace", captured.charge, () => {
          const transfer = [], images = captured.options.pdfImages.map(image => {
            const bytes = byteView(image.bytes, DEMO_WORKSPACE_LIMITS.imageBytes).slice();
            transfer.push(bytes.buffer);
            return { destination: image.destination, bytes };
          });
          return { args: [captured.source, { ...captured.options, pdfImages: images }, maximum], transfer };
        }, controls, (state, value) => {
          if (state !== null) fail("WORKER_PROTOCOL_ERROR", "Unexpected workspace worker state.");
          output(value, captured.sourceLength, maximum);
        }).then(value => Object.freeze({ ...value,
          blob: () => new Blob([value.bytes], { type: value.mimeType }),
          text: () => new TextDecoder("utf-8", { fatal: true }).decode(value.bytes),
        }));
      } catch (error) { return Promise.reject(error); }
    },
    dispose() { disposed = true; rpc?.dispose(); },
  });
}

/** The fixed worker entry supplies the loader. No runtime or import URL is
 * accepted from the message. Exported for host wiring and physical-worker tests. */
export function installDemoWorkspaceWorker(endpoint, loadExporter) {
  return serveOwnedWorker(endpoint, async (method, args) => {
    if (method !== "prepare-workspace" || args.length !== 3) fail("WORKER_PROTOCOL_ERROR", "Invalid workspace preparation request.");
    const captured = input(args[0], args[1]), maximum = outputLimit(args[2]);
    try {
      const exporter = await loadExporter();
      const rendered = await exporter.render(captured.source, captured.options);
      const value = output(rendered, captured.sourceLength, maximum);
      // Do not detach storage owned by the exporter, even for a subarray.
      value.bytes = value.bytes.slice();
      return { state: null, value, transfer: [value.bytes.buffer] };
    } catch (error) {
      const failure = new FlowWorkerError(/^[A-Z0-9_]{1,64}$/.test(error?.code) ? error.code : "WORKSPACE_PREPARATION_FAILED",
        error instanceof Error ? error.message : "The editable workspace could not be prepared.");
      failure.fatal = true;
      throw failure;
    }
  });
}
