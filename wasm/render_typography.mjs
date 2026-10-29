// Shared admission for worker and Flow publication. These options select the
// existing native algorithms; this module never loads WASM or shapes text.
function invalid(message) {
  throw Object.assign(new TypeError(message), { code: "INVALID_OPTIONS" });
}

/** Capture only the three native switches, without retaining caller objects or
 * invoking accessors. Missing and explicit false values keep their spelling. */
export function normalizePdfTypography(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    invalid("typography must be a plain data object");
  const result = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!["homogeneous", "antiriver", "pareto"].includes(key)
        || !field || !Object.hasOwn(field, "value"))
      invalid("typography has an unsupported field or accessor");
    if (field.value === undefined) continue;
    if (typeof field.value !== "boolean") invalid(`typography.${key} must be boolean`);
    result[key] = field.value;
  }
  return Object.freeze(result);
}

export function normalizeMicrotype(value) {
  if (!["disabled", "protrusion", "expansion"].includes(value))
    invalid("microtype must be disabled, protrusion or expansion");
  return value;
}

export function normalizeHtmlFontFormat(value) {
  if (!["woff1", "woff2", "ttf"].includes(value))
    invalid("htmlFontFormat must be woff1, woff2 or ttf");
  return value;
}

/** Single-document archival emission policy. Ordinary calls retain their
 * original ABI and output envelope; explicit off is identical to omission. */
export function normalizePdfArchival(profile, strict) {
  if (profile !== undefined && profile !== "off" && profile !== "2b")
    invalid("pdfA must be off or 2b");
  if (strict !== undefined && typeof strict !== "boolean")
    invalid("pdfAStrict must be boolean");
  if (strict === true && profile !== "2b") invalid("pdfAStrict requires pdfA: 2b");
  return Object.freeze(profile === "2b" ? { pdfA: "2b", pdfAStrict: strict ?? false } : {});
}

/** Validate a successful exporter receipt, not PDF/A conformance. Supplying
 * expected also fences against an older worker silently dropping the policy. */
export function pdfArchivalReceipt(output, expected, code = "INVALID_WASM_RESPONSE") {
  const profile = output?.pdfA, strict = output?.pdfAStrict;
  const present = profile !== undefined || strict !== undefined;
  const valid = !present || (output.format === "pdf" && profile === "2b" && typeof strict === "boolean");
  const matches = expected === undefined || (profile === expected.pdfA && strict === expected.pdfAStrict);
  if (!valid || !matches) throw Object.assign(new Error(
    "PDF output did not acknowledge the requested archival emission policy; rebuild matching renderer and worker packages",
  ), { code });
  return present ? { pdfA: profile, pdfAStrict: strict } : {};
}

/** Only the native typed-pagination contract becomes a coded host Error.
 * Other renderer failures retain their existing representation and policy. */
const PAGINATION_REASONS = Object.freeze({
  PAGINATION_INFEASIBLE: "infeasible",
  PAGINATION_BUDGET_EXCEEDED: "budget_exceeded",
  PAGINATION_INVALID_INPUT: "invalid_input",
  PAGINATION_COST_OVERFLOW: "cost_overflow",
  PAGINATION_INVALID_PLAN: "invalid_plan",
});
export function isPdfPaginationError(error) {
  return error instanceof Error && Object.hasOwn(PAGINATION_REASONS, error.code);
}
export function normalizePdfPaginationError(error) {
  if (typeof error !== "string" || error.length > 4096) return error;
  let value;
  try { value = JSON.parse(error); } catch { return error; }
  if (!value || typeof value !== "object" || Array.isArray(value)
      || !Object.hasOwn(PAGINATION_REASONS, value.code)
      || value.reason !== PAGINATION_REASONS[value.code]
      || typeof value.message !== "string" || value.message.length > 2048) return error;
  return Object.assign(new Error(value.message), { code: value.code, reason: value.reason });
}

/** PDF body-grid spacing in points. Preserve host precision until the core
 * rounds once to milli-points. Zero keeps the default grid disabled. */
export function normalizeBaselineGridPt(value) {
  if (value === undefined) return undefined;
  if (typeof value !== "number" || !Number.isFinite(value)
      || (value !== 0 && (value < 1 || value > 144)))
    invalid("pdf_baseline_grid: baselineGridPt must be 0 (disabled) or a finite number from 1 through 144 points");
  return value;
}

/** Fitting changes font sizes and margins, which cannot preserve an explicitly
 * chosen fixed grid. Every host rejects this combination before rendering. */
export function validateBaselineGridFit(grid, fitToPages) {
  if (grid > 0 && fitToPages > 0)
    invalid("pdf_baseline_grid_incompatible: baseline grids cannot be combined with fitToPages");
}
