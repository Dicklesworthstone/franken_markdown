// The paper-geometry contract shared by the direct, worker and flow PDF APIs.
// Values are PDF points (1/72 inch), not CSS pixels or a post-render scale.
// This module neither loads WASM nor mutates the caller's options.
const PAPER = Object.freeze({
  letter: Object.freeze([612, 792]),
  a4: Object.freeze([210 * 72 / 25.4, 297 * 72 / 25.4]),
});
const SIDES = ["topPt", "rightPt", "bottomPt", "leftPt"];

function fail(message) {
  const error = new TypeError(message);
  error.code = "INVALID_OPTIONS";
  throw error;
}
function record(value, allowed, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) {
    fail(`${label} must be a plain data object`);
  }
  const result = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!allowed.includes(key) || !field || !Object.hasOwn(field, "value")) {
      fail(`${label} has an unsupported field or accessor`);
    }
    if (field.value !== undefined) result[key] = field.value;
  }
  return result;
}
function point(value, min, label) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > 14400) {
    fail(`${label} must be a finite number from ${min} to 14400 points`);
  }
  return value === 0 ? 0 : value; // Canonicalize -0, without rounding host values.
}
function contentExtent(extent, first, second) {
  // Match Rust's two left-associative f32 subtractions, not just a final cast.
  return Math.fround(Math.fround(Math.fround(extent) - Math.fround(first)) - Math.fround(second));
}

/** Undefined means the original renderer defaults, with no new ABI required.
 * A supplied object is an explicit page configuration. Omitted size is Letter;
 * omitted margins are 72 points per side. Custom dimensions retain their order
 * unless an explicit orientation asks to rotate them. Margin sides never rotate.
 * Returns a deeply owned/frozen canonical data record; normalizing it is stable.
 */
export function normalizePdfPage(value) {
  if (value === undefined) return undefined;
  const page = record(value, ["size", "orientation", "margins"], "PDF page");
  let widthPt, heightPt;
  const size = page.size === undefined ? "letter" : page.size;
  if (typeof size === "string") {
    if (!Object.hasOwn(PAPER, size)) fail("PDF page size must be letter, a4 or explicit dimensions");
    [widthPt, heightPt] = PAPER[size];
  } else {
    const dimensions = record(size, ["widthPt", "heightPt"], "PDF page size");
    widthPt = dimensions.widthPt;
    heightPt = dimensions.heightPt;
  }
  widthPt = point(widthPt, 144, "PDF page width");
  heightPt = point(heightPt, 144, "PDF page height");
  if (page.orientation !== undefined) {
    if (!["portrait", "landscape"].includes(page.orientation)) {
      fail("PDF page orientation must be portrait or landscape");
    }
    if ((page.orientation === "portrait" && widthPt > heightPt)
        || (page.orientation === "landscape" && heightPt > widthPt)) {
      [widthPt, heightPt] = [heightPt, widthPt];
    }
  }
  const selected = page.margins === undefined ? 72 : page.margins;
  const margins = typeof selected === "number"
    ? Object.fromEntries(SIDES.map(side => [side, selected]))
    : record(selected, SIDES, "PDF page margins");
  for (const side of SIDES) margins[side] = point(margins[side] === undefined ? 72 : margins[side], 0, side);
  const { topPt, rightPt, bottomPt, leftPt } = margins;
  if (widthPt - leftPt - rightPt < 72 || heightPt - topPt - bottomPt < 72
      || contentExtent(widthPt, leftPt, rightPt) < 72
      || contentExtent(heightPt, topPt, bottomPt) < 72) {
    fail("PDF page margins must leave at least 72 points of content width and height");
  }
  return Object.freeze({ size: Object.freeze({ widthPt, heightPt }), margins: Object.freeze(margins) });
}

/** Pack the canonical page into the native ABI's exact six-value order. */
export function pdfPageGeometry(value) {
  const page = normalizePdfPage(value);
  if (page === undefined) return new Float64Array();
  const { widthPt, heightPt } = page.size;
  const { topPt, rightPt, bottomPt, leftPt } = page.margins;
  return new Float64Array([widthPt, heightPt, topPt, rightPt, bottomPt, leftPt]);
}
