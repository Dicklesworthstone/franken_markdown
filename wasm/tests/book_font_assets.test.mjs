import test from "node:test";
import assert from "node:assert/strict";
import { File } from "node:buffer";
import { createBookFontStore, readBookFont, BOOK_FONT_SLOTS as S, BOOK_FONT_LIMITS as L } from "../demo/book_font_assets.mjs";
// Arbitrary payloads test byte ownership and admission only, not font parsing.
const asset = (slot = S[0], bytes = new Uint8Array([1, 2, 3]), extra = {}) => ({ slot, name: "Selected.ttf", bytes, ...extra });

test("font roles are explicit and snapshots own their bytes", () => {
  const store = createBookFontStore(), bytes = new Uint8Array([0, 1, 2, 3, 4]);
  store.set([asset(S[1], bytes.subarray(1, 4), { weight: 625 }), asset(S[0])]);
  bytes[1] = 99;
  assert.deepEqual(store.list().map(f => f.slot), [S[0], S[1]]);
  const result = store.snapshot();
  assert.deepEqual([...result[1].bytes], [1, 2, 3]);
  assert.equal(result[1].weight, 625);
  assert.equal(Object.hasOwn(result[1], "name"), false);
  result[1].bytes[0] = 88;
  assert.equal(store.snapshot()[1].bytes[0], 1);
  const metadata = store.list(); metadata[0].name = "changed";
  assert.equal(store.list()[0].name, "Selected.ttf");
  assert(metadata.every(f => !Object.hasOwn(f, "bytes")));
});

test("replacement changes one role and releases its previous weight pin", () => {
  const store = createBookFontStore();
  store.set([asset(S[0], undefined, { weight: 300 }), asset(S[1], undefined, { weight: 700 })]);
  store.set([asset(S[0], new Uint8Array([7]), { name: "Replacement.ttf" })]);
  assert.equal(store.list().length, 2);
  assert.equal(store.snapshot()[0].weight, undefined);
  assert.equal(store.snapshot()[1].weight, 700);
  assert.deepEqual([...store.snapshot()[0].bytes], [7]);
});

test("invalid batch leaves all installed roles unchanged", () => {
  const store = createBookFontStore(); store.set([asset()]); const before = store.snapshot();
  for (const values of [[], new Array(1), [asset(S[1]), asset("unknown")], [asset(), asset()],
    [asset(S[1]), asset(S[2], new Uint8Array())], Array.from({ length: 6 }, () => asset())]) {
    assert.throws(() => store.set(values));
    assert.deepEqual(store.snapshot(), before);
  }
});

test("weights admit 1 through 1000 only and metadata rejects malformed text", () => {
  const store = createBookFontStore();
  for (const weight of [0, -1, 1001, 3.5, NaN, Infinity, "400", null]) {
    assert.throws(() => store.set([asset(S[0], undefined, { weight })]), { code: "INVALID_FONT_WEIGHT" });
  }
  for (const name of ["", "x".repeat(513), "bad\n.ttf", "bad\u001b.ttf", "bad\ud800.ttf", null]) {
    assert.throws(() => store.set([asset(S[0], undefined, { name })]), { code: "INVALID_FONT" });
  }
  for (const weight of [1, 1000, undefined]) store.set([asset(S[0], undefined, { weight })]);
  store.set([asset(S[0], undefined, { name: "<b>字体😀</b>.ttf" })]);
  assert.equal(store.list()[0].name, "<b>字体😀</b>.ttf");
});

test("unsupported shared, detached and oversized buffers are not retained", () => {
  const store = createBookFontStore();
  const detached = new Uint8Array(5); structuredClone(detached, { transfer: [detached.buffer] });
  for (const bytes of [new ArrayBuffer(5), new DataView(new ArrayBuffer(5)), new Uint16Array(5),
    new Uint8Array(new SharedArrayBuffer(5)), detached, new Uint8Array(L.faceBytes + 1)]) {
    assert.throws(() => store.set([asset(S[0], bytes)]), { code: "FONT_LIMIT" });
  }
  assert.deepEqual(store.list(), []);
});

test("aggregate budgets account for replacement rather than charging the old face twice", () => {
  const store = createBookFontStore(), bytes = new Uint8Array(L.faceBytes);
  store.set(S.slice(0, 4).map(slot => asset(slot, bytes)));
  store.set([asset(S[0], bytes, { name: "new.ttf" })]);
  assert.equal(store.list().reduce((sum, f) => sum + f.size, 0), L.totalBytes);
  assert.throws(() => store.set([asset(S[4], new Uint8Array([1]))]), { code: "FONT_LIMIT" });
  assert.equal(store.list().length, 4);
  assert.equal(store.list()[0].name, "new.ttf");
  store.remove(S[1]); store.set([asset(S[4])]); assert.equal(store.list().length, 4);
});

test("snapshot copying does not invoke a caller-overridden slice", () => {
  class HostBytes extends Uint8Array { slice() { throw new Error("not owned"); } }
  const store = createBookFontStore(); store.set([asset(S[0], new HostBytes([1, 2]))]);
  assert.deepEqual([...store.snapshot()[0].bytes], [1, 2]);
});

test("removal and disposal release only font state and report true changes", () => {
  const store = createBookFontStore(); assert.equal(store.clear(), false);
  store.set([asset(), asset(S[1])]);
  assert.equal(store.remove(S[0]), true); assert.equal(store.remove(S[0]), false);
  assert.throws(() => store.remove("all"), { code: "INVALID_FONT_SLOT" });
  assert.equal(store.clear(), true); assert.deepEqual(store.snapshot(), []);
  store.dispose(); store.dispose();
  for (const operation of [() => store.list(), () => store.snapshot(), () => store.set([asset()]), () => store.clear()]) {
    assert.throws(operation, { code: "SESSION_DISPOSED" });
  }
});

test("local font file reading captures role, name, weight and exact bytes", async () => {
  const font = await readBookFont(new File([new Uint8Array([0, 255, 2, 3])], "Local.TTF"), S[2], 425);
  assert.deepEqual(font, { slot: S[2], name: "Local.TTF", weight: 425, bytes: new Uint8Array([0, 255, 2, 3]) });
});

test("bad extension, role, weight, and file-size admission precede reading", async () => {
  let reads = 0;
  class Counted extends File { async arrayBuffer() { reads++; return super.arrayBuffer(); } }
  class Large extends Counted { get size() { return L.faceBytes + 1; } }
  for (const [file, slot, weight] of [[new Counted(["x"], "x.woff2"), S[0]],
    [new Large(["x"], "x.ttf"), S[0]], [new Counted([], "x.ttf"), S[0]],
    [new Counted(["x"], "x.ttf"), "bad"], [new Counted(["x"], "x.ttf"), S[0], 1001]]) {
    await assert.rejects(readBookFont(file, slot, weight));
  }
  assert.equal(reads, 0);
});

test("failed or size-changing reads reject without exposing partial assignments", async () => {
  class Broken extends File { async arrayBuffer() { throw new Error("disk error"); } }
  class Changed extends File { async arrayBuffer() { return new ArrayBuffer(10); } }
  class Shared extends File { async arrayBuffer() { return new SharedArrayBuffer(1); } }
  for (const Kind of [Broken, Changed, Shared]) {
    await assert.rejects(readBookFont(new Kind(["x"], "x.ttf"), S[0]), { code: "FONT_READ_FAILED" });
  }
});
