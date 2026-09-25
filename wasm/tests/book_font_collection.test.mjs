import test from "node:test";
import assert from "node:assert/strict";
import { createBookCollection } from "../demo/book_collection.mjs";
import { BOOK_FONT_SLOTS as S } from "../demo/book_font_assets.mjs";
import { createBookBindings } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
// Production collection, facade, client and worker handler. Only the Rust class
// and message endpoints are doubles; these tests do not parse/render fonts.
const font = (slot = S[0], value = 1, weight) => ({ slot, name: `${slot}.ttf`, weight, bytes: new Uint8Array([value]) });
function book() {
  const value = createBookCollection();
  value.append({ chapters: [{ path: "start.md", source: "# Start\n" }],
    includeSources: [{ path: "shared.md", source: "shared" }],
    images: [{ destination: "chart.svg", bytes: new Uint8Array([60]) }] });
  return value;
}

test("all roles enter real prepared input, with one notification and no exposed ownership", () => {
  const b = book(), assets = S.map((slot, i) => font(slot, i + 1, 300 + i * 100));
  const before = b.revision; let notifications = 0; b.subscribe(() => notifications++);
  b.setFonts(assets, before); assert.equal(b.revision, before + 1); assert.equal(notifications, 1);
  assets[0].bytes[0] = 99;
  const snapshot = b.snapshot();
  assert.deepEqual(snapshot.options.fontAssets.map(f => f.slot), S);
  assert.equal(snapshot.options.fontAssets[0].bytes[0], 1);
  assert.deepEqual(snapshot.options.includeSources, [{ path: "shared.md", source: "shared" }]);
  snapshot.options.fontAssets[0].bytes[0] = 88;
  assert.equal(b.snapshot().options.fontAssets[0].bytes[0], 1);
  assert.equal(b.fonts[0].size, 1);
  assert.equal(Object.hasOwn(b.fonts[0], "bytes"), false);
});

test("font transactions reject stale revisions and bad batches without changing anything", () => {
  const b = book(); b.setFonts([font()], b.revision); const before = b.snapshot(), revision = b.revision;
  for (const expected of [undefined, revision - 1, NaN, "2"]) {
    assert.throws(() => b.setFonts([font(S[1])], expected), { code: "STALE_SOURCE" });
  }
  assert.throws(() => b.setFonts([font(S[1]), font("invalid")], revision));
  assert.deepEqual(b.snapshot(), before); assert.equal(b.revision, revision);
});

test("reentrant input access cannot overwrite fonts after a source edit or disposal", () => {
  const b = book(); b.setFonts([font()], b.revision); const revision = b.revision;
  const candidate = { ...font(S[1]), get name() { b.edit(0, "start.md", "new edit"); return "new.ttf"; } };
  assert.throws(() => b.setFonts([candidate], revision), { code: "STALE_SOURCE" });
  assert.equal(b.files[0].source, "new edit"); assert.equal(b.fonts.length, 1);
  const dispose = { ...font(S[1]), get bytes() { b.dispose(); return new Uint8Array([5]); } };
  assert.throws(() => b.setFonts([dispose], b.revision), { code: "SESSION_DISPOSED" });
});

test("removal invalidates published snapshots once and does not revoke images or source", () => {
  const b = book(); b.setFonts([font(), font(S[1])], b.revision); const before = b.project(), image = b.images;
  const revision = b.revision; b.revokeFont(S[0]); assert.equal(b.revision, revision + 1);
  b.revokeFont(S[0]); assert.equal(b.revision, revision + 1);
  b.revokeFonts(); assert.deepEqual(b.fonts, []); assert.equal(b.revision, revision + 2);
  b.revokeFonts(); assert.equal(b.revision, revision + 2);
  assert.deepEqual(b.project(), before); assert.deepEqual(b.images, image);
});

test("source-only projects exclude font bytes/names/weights and recovery revokes all fonts", async () => {
  const b = book(), before = b.project(); b.setFonts([font(S[0], 99, 666)], b.revision);
  assert.deepEqual(b.project(), before);
  const json = await b.projectDownload().blob.text();
  assert(!json.includes("fontAssets") && !json.includes(".ttf") && !json.includes("666"));
  assert.throws(() => b.replaceProject({ ...before, options: { ...before.options, fontAssets: [font()] } }));
  assert.equal(b.fonts.length, 1);
  b.replaceProject(JSON.parse(json)); assert.deepEqual(b.fonts, []); assert.deepEqual(b.images, []);
  assert.deepEqual(b.project(), before);
});

test("source edits, roles and undo transactions retain fonts; disposal releases them", () => {
  const b = book(); b.setFonts([font()], b.revision); const original = b.files;
  b.replaceSources(original.map(f => ({ ...f, source: f.source + "edit" })), b.revision);
  b.replaceSources(original, b.revision);
  b.setRole(1, "chapter"); b.move(1, -1); b.configure({ font: "serif" });
  assert.equal(b.fonts.length, 1); b.revokeImages(); assert.equal(b.fonts.length, 1);
  b.dispose(); b.dispose(); assert.throws(() => b.fonts, { code: "SESSION_DISPOSED" });
});

function engineDouble({ fail = false } = {}) {
  const calls = [], freed = [];
  class Raw {
    static fromSources(...args) { calls.push(["sources", ...args]); return new Raw(); }
    setMetadata() {} setCustomCss() {} setTheme() {} setNavigation() {} setFontScale() {} setImage() {}
    setFont(slot, bytes) { calls.push(["font", slot, [...bytes]]); if (fail) throw new Error("explicit invalid font"); }
    setFontWeight(slot, weight) { calls.push(["weight", slot, weight]); }
    renderPdf() { calls.push(["pdf"]); return new Uint8Array([1]); }
    renderEpub() { calls.push(["epub"]); return new Uint8Array([2]); }
    renderSite() { calls.push(["site"]); return new Uint8Array([3]); }
    sourceLength = 13;
    free() { freed.push(this); }
  }
  return { api: createBookBindings(async () => Raw), calls, freed };
}
for (const [method, kind] of [["renderBookPdf", "pdf"], ["renderBookEpub", "epub"], ["renderBookSite", "site"]]) {
  test(`${kind} facade receives fonts, weights and include sources without new ABI`, async () => {
    const b = book(); b.setFonts([font(S[0], 42, 525), font(S[4], 64)], b.revision);
    const engine = engineDouble(), input = b.snapshot();
    await engine.api[method](input.files, input.options);
    assert.deepEqual(engine.calls.filter(c => c[0] === "font"), [["font", S[0], [42]], ["font", S[4], [64]]]);
    assert.deepEqual(engine.calls.filter(c => c[0] === "weight"), [["weight", S[0], 525]]);
    assert.deepEqual(engine.calls.at(-1), [kind]); assert.equal(engine.freed.length, 1);
    assert.deepEqual(engine.calls[0][3], ["shared.md"]);
  });
}

test("engine font refusal retires its handle without changing workbench source", async () => {
  const b = book(); b.setFonts([font()], b.revision); const before = b.project();
  const engine = engineDouble({ fail: true }), input = b.snapshot();
  await assert.rejects(engine.api.renderBookEpub(input.files, input.options), /explicit invalid font/);
  assert.equal(engine.freed.length, 1); assert.deepEqual(b.project(), before); assert.equal(b.fonts.length, 1);
});

function transport(engine) {
  let receive;
  const scope = { addEventListener(_, fn) { receive = fn; }, postMessage(data, transfers) {
    const copy = structuredClone(data, { transfer: transfers });
    queueMicrotask(() => endpoint.dispatchEvent(new MessageEvent("message", { data: copy })));
  } };
  const endpoint = new EventTarget(); endpoint.terminate = () => {};
  endpoint.postMessage = (data, transfers) => {
    const copy = structuredClone(data, { transfer: transfers });
    queueMicrotask(() => { void receive({ data: copy }); });
  };
  installBookWorker(scope, engine);
  return createBookWorkerClient({ workerFactory: () => endpoint });
}
for (const format of ["pdf", "epub", "site", "preview"]) test(`${format} worker transfers only private font copies`, async () => {
  const b = book(); b.setFonts([font(S[0], 7, 455)], b.revision);
  const input = b.snapshot();
  const method = { pdf: "renderBookPdf", epub: "renderBookEpub", site: "renderBookSite", preview: "renderBookPreview" }[format];
  let captured;
  const worker = transport({ async [method](files, options) {
    captured = options.fontAssets; options.fontAssets[0].bytes[0] = 9;
    assert.equal(files.length, 1); assert.equal(options.includeSources.length, 1);
    return { bytes: new Uint8Array([1]), sourceLength: 13 };
  } });
  try { await worker.render(input.files, format, input.options); }
  finally { worker.dispose(); }
  assert.equal(captured[0].weight, 455);
  assert.equal(input.options.fontAssets[0].bytes.byteLength, 1);
  assert.equal(input.options.fontAssets[0].bytes[0], 7);
  assert.equal(b.snapshot().options.fontAssets[0].bytes[0], 7);
});

for (const format of ["links", "inspection"]) test(`${format} worker never reads or transfers supplied font options`, async () => {
  const b = book(), input = b.snapshot(), options = { ...input.options,
    get fontAssets() { throw new Error("fonts are not authorized on this route"); } };
  const method = format === "links" ? "checkBookLinks" : "inspectBook";
  const worker = transport({ async [method](_, settings) {
    assert.deepEqual(settings.fontAssets, []); return { bytes: new Uint8Array([1]), sourceLength: 13 };
  } });
  try { await worker.render(input.files, format, options); }
  finally { worker.dispose(); }
});
