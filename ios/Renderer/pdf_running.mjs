// Data-only running bands shared by Flow export and stateless document workers.
// No WASM is loaded here. Rust owns template expansion, assets and margin fitting.
function fail(code, message) {
  const error = new TypeError(message);
  error.code = code;
  throw error;
}
function record(value, allowed, name) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail("INVALID_OPTIONS", `${name} must be a plain data object`);
  const result = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!allowed.includes(key) || !field || !Object.hasOwn(field, "value"))
      fail("INVALID_OPTIONS", `${name} has an unsupported field or accessor`);
    if (field.value !== undefined) result[key] = field.value;
  }
  return result;
}
function text(value, name) {
  if (typeof value !== "string" || value.length > 4096)
    fail("INVALID_OPTIONS", `invalid ${name}`);
  // Reject malformed UTF-16 before the native encoder can replace it silently.
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        fail("INVALID_UNICODE", `${name} contains an unpaired high surrogate`);
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      fail("INVALID_UNICODE", `${name} contains an unpaired low surrogate`);
    }
  }
  return value;
}
function utf8Length(value) {
  let bytes = 0;
  // The caller has already validated paired surrogates and the length bound.
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) { bytes += 4; i++; }
    else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
  }
  return bytes;
}

/** Returns deeply owned/frozen records, safe to retain before a worker dispatch.
 * Slot templates retain their literal spelling: at most 4096 UTF-8 bytes each
 * and 16384 UTF-16 units together. Image keys trim whitespace and allow at most
 * 4096 UTF-8 bytes. Asset bytes remain in the caller's separate pdfImages input.
 */
export function normalizePdfRunning(value) {
  if (value === undefined) return undefined;
  const input = record(value, ["header", "footer", "skipFirstPage"], "running");
  const result = {};
  let units = 0, draws = false;
  for (const name of ["header", "footer"]) {
    if (input[name] === undefined) continue;
    const band = record(input[name], ["left", "center", "right", "rule", "image"], `running.${name}`);
    const next = {};
    for (const slot of ["left", "center", "right"]) {
      if (band[slot] === undefined) continue;
      const value = text(band[slot], `running.${name}.${slot}`);
      if (utf8Length(value) > 4096)
        fail("INVALID_OPTIONS", `running.${name}.${slot} exceeds 4096 UTF-8 bytes`);
      units += value.length;
      if (units > 16384) fail("BUDGET_EXCEEDED", "running templates exceed 16384 UTF-16 units");
      next[slot] = value;
      draws ||= value.length > 0;
    }
    if (band.rule !== undefined) {
      if (typeof band.rule !== "boolean") fail("INVALID_OPTIONS", "running rule must be boolean");
      next.rule = band.rule;
      draws ||= band.rule;
    }
    if (band.image !== undefined) {
      const label = `running.${name}.image`;
      const image = record(band.image, ["dest", "position", "heightPt"], label);
      const dest = text(image.dest, `${label}.dest`).trim();
      if (!dest || utf8Length(dest) > 4096)
        fail("INVALID_OPTIONS", `${label}.dest requires 1..4096 UTF-8 bytes`);
      if (image.position !== undefined && !["left", "right"].includes(image.position))
        fail("INVALID_OPTIONS", `${label}.position must be left or right`);
      if (image.heightPt !== undefined && (!Number.isSafeInteger(image.heightPt)
          || image.heightPt < 1 || image.heightPt > 65535))
        fail("INVALID_OPTIONS", `invalid ${label}.heightPt`);
      next.image = Object.freeze({ dest,
        ...(image.position === undefined ? {} : { position: image.position }),
        ...(image.heightPt === undefined ? {} : { heightPt: image.heightPt }) });
      draws = true;
    }
    result[name] = Object.freeze(next);
  }
  if (input.skipFirstPage !== undefined) {
    if (typeof input.skipFirstPage !== "boolean")
      fail("INVALID_OPTIONS", "running.skipFirstPage must be boolean");
    // The native wrapper chooses its legacy ABI when no band draws, so this
    // flag alone cannot be honored. Keep the existing Flow admission contract.
    if (input.skipFirstPage && !draws)
      fail("INVALID_OPTIONS", "skipFirstPage requires a nonempty header or footer");
    result.skipFirstPage = input.skipFirstPage;
  }
  return Object.freeze(result);
}
