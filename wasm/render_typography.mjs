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
