// Editable Flow projects retain source and applied data-only settings. Images,
// fonts, file handles, worker state and permission grants never enter this file.
import { FlowError } from "../flow_session.mjs";
import { documentSnapshot } from "./flow_document.mjs";
import { normalizeRenderSettings } from "./flow_render_settings.mjs";

export const FLOW_PROJECT_LIMIT = 32 * 1024 * 1024;
const SCHEMA = "franken-markdown-flow-project";
const fail = (message) => { throw new FlowError("INVALID_PROJECT", message); };
function record(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail(label + " must be a plain data record.");
  for (const key of Reflect.ownKeys(value)) {
    if (typeof key !== "string" || (keys && !keys.includes(key))
        || !Object.hasOwn(Object.getOwnPropertyDescriptor(value, key), "value"))
      fail(label + " contains an unsupported field or accessor.");
  }
  return value;
}
export function captureFlowProject(document, settings) {
  record(document, ["filename", "source"], "Project source");
  record(settings, null, "Project settings");
  // Reject nested authority-bearing objects before shared option normalization.
  for (const value of Object.values(settings)) {
    if (value !== null && !["string", "number", "boolean"].includes(typeof value))
      fail("Project settings must contain only primitive values or null.");
  }
  const source = documentSnapshot(document);
  const normalized = normalizeRenderSettings(settings);
  return Object.freeze({
    schema: SCHEMA, version: 1, document: source, settings: normalized.values,
  });
}
export function serializeFlowProject(document, settings) {
  const project = captureFlowProject(document, settings);
  const text = JSON.stringify(project, null, 2) + "\n";
  if (new TextEncoder().encode(text).length > FLOW_PROJECT_LIMIT)
    throw new FlowError("BUDGET_EXCEEDED", "Flow source project exceeds the 32 MiB JSON limit.");
  return text;
}
export function parseFlowProject(text) {
  if (typeof text !== "string") fail("Flow source project must be text.");
  if (text.length > FLOW_PROJECT_LIMIT)
    throw new FlowError("BUDGET_EXCEEDED", "Flow source project exceeds the 32 MiB JSON limit.");
  let bytes = 0;
  for (const scalar of text) {
    const code = scalar.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff)
      throw new FlowError("INVALID_UNICODE", "Flow source project contains an unpaired surrogate.");
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    if (bytes > FLOW_PROJECT_LIMIT)
      throw new FlowError("BUDGET_EXCEEDED", "Flow source project exceeds the 32 MiB JSON limit.");
  }
  let value;
  try { value = JSON.parse(text.charCodeAt(0) === 0xfeff ? text.slice(1) : text); }
  catch { fail("Choose a valid Flow source project JSON file."); }
  record(value, ["schema", "version", "document", "settings"], "Flow source project");
  if (value.schema !== SCHEMA || value.version !== 1)
    fail("This Flow source project schema or version is not supported.");
  return captureFlowProject(value.document, value.settings);
}
export async function readFlowProjectFile(file) {
  if (!file || Object.prototype.toString.call(file) !== "[object File]"
      || typeof file.arrayBuffer !== "function" || !Number.isSafeInteger(file.size)
      || file.size < 1 || file.size > FLOW_PROJECT_LIMIT)
    throw new FlowError("INVALID_FILE", "Choose a local Flow source project up to 32 MiB.");
  const size = file.size;
  let buffer;
  try { buffer = await file.arrayBuffer(); }
  catch (cause) { throw new FlowError("FILE_READ_FAILED", "The project file could not be read.", { cause }); }
  if (Object.prototype.toString.call(buffer) !== "[object ArrayBuffer]" || buffer.byteLength !== size)
    throw new FlowError("INVALID_FILE", "Project bytes did not match the admitted file size.");
  let text;
  try { text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(buffer); }
  catch (cause) { throw new FlowError("INVALID_UNICODE", "The project is not valid UTF-8.", { cause }); }
  return parseFlowProject(text);
}
export function flowProjectDownload(document, settings) {
  const text = serializeFlowProject(document, settings);
  let stem = document.filename.replace(/\.(md|markdown)$/i, "") || "document";
  const suffix = ".fmdflow.json";
  // The original name is already bounded; trim whole Unicode code points only.
  while (new TextEncoder().encode(stem + suffix).length > 240)
    stem = Array.from(stem).slice(0, -1).join("");
  return Object.freeze({ filename: stem + suffix,
    blob: new Blob([text], { type: "application/json;charset=utf-8", endings: "transparent" }) });
}
