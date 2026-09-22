import assert from "node:assert/strict";
import test from "node:test";
import { normalizePdfPage, pdfPageGeometry } from "./pdf_page.mjs";

const invalid = value => assert.throws(() => normalizePdfPage(value), { code: "INVALID_OPTIONS" });
const geometry = value => [...pdfPageGeometry(value)];

test("absence preserves the old ABI while an explicit empty page selects native defaults", () => {
  assert.equal(normalizePdfPage(undefined), undefined);
  assert.deepEqual(geometry(undefined), []);
  assert.deepEqual(geometry({}), [612, 792, 72, 72, 72, 72]);
  assert.deepEqual(geometry({ size: "letter" }), [612, 792, 72, 72, 72, 72]);
});

test("A4 dimensions are converted from millimetres to points before orientation", () => {
  assert.deepEqual(geometry({ size: "a4" }), [210 * 72 / 25.4, 297 * 72 / 25.4, 72, 72, 72, 72]);
  assert.deepEqual(geometry({ size: "a4", orientation: "landscape", margins: 0 }),
    [297 * 72 / 25.4, 210 * 72 / 25.4, 0, 0, 0, 0]);
});

test("custom paper dimensions and all four margins retain exact order and precision", () => {
  assert.deepEqual(geometry({ size: { widthPt: 720.25, heightPt: 540.75 },
    margins: { topPt: 18, rightPt: 24, bottomPt: 30, leftPt: 36 } }),
    [720.25, 540.75, 18, 24, 30, 36]);
});

test("orientation rotates only paper, never margins; omitted orientation preserves custom order", () => {
  const page = { size: { widthPt: 720, heightPt: 540 }, margins: { topPt: 18, rightPt: 24, bottomPt: 30, leftPt: 36 } };
  assert.deepEqual(geometry(page), [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(geometry({ ...page, orientation: "portrait" }), [540, 720, 18, 24, 30, 36]);
  assert.deepEqual(geometry({ ...page, orientation: "landscape" }), [720, 540, 18, 24, 30, 36]);
  assert.deepEqual(geometry({ orientation: "landscape" }), [792, 612, 72, 72, 72, 72]);
});

test("partial margin records preserve per-side defaults, and zero is not treated as absence", () => {
  assert.deepEqual(geometry({ margins: { leftPt: 0, rightPt: 24 } }), [612, 792, 72, 24, 72, 0]);
  assert.deepEqual(geometry({ margins: 0 }), [612, 792, 0, 0, 0, 0]);
  assert.equal(Object.is(normalizePdfPage({ margins: -0 }).margins.topPt, -0), false);
});

test("canonical page records are deeply owned, frozen and idempotent", () => {
  const source = { size: { widthPt: 720, heightPt: 540 }, margins: { leftPt: 24 } };
  const page = normalizePdfPage(source);
  source.size.widthPt = 999; source.margins.leftPt = 0;
  assert.equal(page.size.widthPt, 720);
  assert.equal(page.margins.leftPt, 24);
  for (const value of [page, page.size, page.margins]) assert.equal(Object.isFrozen(value), true);
  assert.deepEqual(normalizePdfPage(page), page);
  const packed = pdfPageGeometry(page); packed[0] = 999;
  assert.equal(page.size.widthPt, 720);
});

test("invalid shapes, unknown keys, strings and incomplete custom dimensions reject", () => {
  for (const value of [null, [], true, 1, "a4", { size: "constructor" }, { size: "A4" },
    { size: null }, { size: [] }, { size: { widthPt: 612 } }, { size: { heightPt: 792 } },
    { size: { widthPt: "612", heightPt: 792 } }, { orientation: "sideways" },
    { orientation: null }, { margins: null }, { margins: "36" }, { margins: [] },
    { margins: { top: 36 } }, { widthPt: 612 }, { [Symbol("page")]: 1 },
    Object.create({ size: "a4" })]) invalid(value);
});

test("accessors in page, size and margins are rejected without invoking getters", () => {
  let reads = 0;
  const accessor = key => Object.defineProperty({}, key, { get() { reads++; return 612; } });
  invalid(accessor("size"));
  invalid({ size: accessor("widthPt") });
  invalid({ margins: accessor("leftPt") });
  assert.equal(reads, 0);
  assert.deepEqual(geometry(Object.assign(Object.create(null), { margins: 36 })), [612, 792, 36, 36, 36, 36]);
});

test("all six dimensions reject NaN, infinity, negative values and values above native bounds", () => {
  for (const bad of [NaN, Infinity, -Infinity, -1, 14400.01, 1n, false, "72"])
    for (const field of ["widthPt", "heightPt", "topPt", "rightPt", "bottomPt", "leftPt"]) {
      const page = { size: { widthPt: 612, heightPt: 792 }, margins: { [field]: bad } };
      if (field === "widthPt" || field === "heightPt") { page.size[field] = bad; page.margins = 0; }
      invalid(page);
    }
  invalid({ size: { widthPt: 143.999999, heightPt: 792 }, margins: 0 });
  assert.deepEqual(geometry({ size: { widthPt: 144, heightPt: 144 }, margins: 0 }), [144, 144, 0, 0, 0, 0]);
  assert.deepEqual(geometry({ size: { widthPt: 14400, heightPt: 14400 }, margins: 0 }), [14400, 14400, 0, 0, 0, 0]);
});

test("content must remain at least 72 points in both axes, including host-precision near misses", () => {
  invalid({ margins: { leftPt: 300, rightPt: 300 } });
  invalid({ margins: { topPt: 400, bottomPt: 400 } });
  invalid({ margins: { leftPt: 270, rightPt: 270.000001 } });
  invalid({ margins: { topPt: 360, bottomPt: 360.000001 } });
  assert.deepEqual(geometry({ margins: { leftPt: 270, rightPt: 270, topPt: 360, bottomPt: 360 } }),
    [612, 792, 360, 270, 360, 270]);
});

test("10,000 seeded pages agree with the native ABI's f64 and staged-f32 rectangle test", () => {
  let seed = 0x5a5a;
  const random = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 2 ** 32);
  const f = Math.fround;
  let roundingRejections = 0, accepted = 0;
  for (let i = 0; i < 10000; i++) {
    const widthPt = 300 + random() * 900, heightPt = 300 + random() * 900;
    const leftPt = random() * (widthPt - 150), topPt = random() * (heightPt - 150);
    const rightPt = widthPt - leftPt - 72 + (i % 3 - 1) * 0.00001;
    const bottomPt = heightPt - topPt - 72 + (i % 5 - 2) * 0.00001;
    const host = widthPt - leftPt - rightPt >= 72 && heightPt - topPt - bottomPt >= 72;
    const native = f(f(f(widthPt) - f(leftPt)) - f(rightPt)) >= 72
      && f(f(f(heightPt) - f(topPt)) - f(bottomPt)) >= 72;
    if (host && !native) roundingRejections++;
    const page = { size: { widthPt, heightPt }, margins: { topPt, rightPt, bottomPt, leftPt } };
    if (host && native) { assert.equal(geometry(page).length, 6); accepted++; }
    else invalid(page);
  }
  assert.ok(roundingRejections > 100, "exercised host-valid, native-invalid rectangles");
  assert.ok(accepted > 100, "exercised valid rectangles");
});
