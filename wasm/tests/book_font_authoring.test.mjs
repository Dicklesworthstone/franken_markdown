import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { createBookFontAuthoring } from "../demo/book_font_authoring.mjs";
import { BOOK_FONT_SLOTS } from "../demo/book_font_assets.mjs";
import { host, worker, deferred, result, tick } from "./book_font_test_support.mjs";

const file = (name = "chosen.ttf", bytes = [0, 1, 2, 255]) => new File([new Uint8Array(bytes)], name);
const selected = (slot = "body-regular", chosen = file(), weight) => ({ slot, file: chosen, weight });
function fixture(settings = {}) {
  const book = host(), engine = worker(settings);
  const author = createBookFontAuthoring({ ...book, worker: engine, ...(settings.timeoutMs ? { timeoutMs: settings.timeoutMs } : {}) });
  return { ...book, engine, author };
}
async function waiting(engine) { for (let i = 0; i < 50 && !engine.jobs.length; i++) await tick(); assert(engine.jobs.length); }

test("one native preflight admits five named roles and installs one complete revision", async () => {
  const f = fixture({ delay: true }), original = f.controls.checkpoint(), images = f.collection.images;
  const batch = BOOK_FONT_SLOTS.map((slot, i) => selected(slot, file(`${i}.ttf`, [i, 255, 0]), i === 1 ? 650 : undefined));
  const observed = []; f.collection.subscribe(() => observed.push(f.collection.fonts));
  const pending = f.author.assign(batch);
  await waiting(f.engine);
  assert.equal(f.author.phase, "validating"); assert.equal(f.collection.fonts.length, 0);
  const job = f.engine.jobs[0];
  assert.equal(job.format, "pdf"); assert.equal(job.options.fontAssets.length, 5);
  assert.equal(job.options.fontAssets[1].weight, 650);
  assert.equal(job.options.images, undefined); assert.equal(job.options.expandIncludes, false);
  assert(!job.files[0].source.includes("Original source")); // Probe never retargets a real book chapter.
  assert.equal(f.controls.checkpoint(), original);
  job.resolve(result()); const output = await pending;
  assert.equal(output.revision, 1); assert(Object.isFrozen(output.assignments[0]));
  assert.equal(observed.length, 1); assert.equal(observed[0].length, 5);
  assert.equal(f.collection.images, images); assert.equal(f.collection.files[0].source, "\ufeff# Book\r\n\nOriginal source");
  assert.deepEqual(f.store.snapshot().map(x => [...x.bytes]), [[0,255,0],[1,255,0],[2,255,0],[3,255,0],[4,255,0]]);
  f.author.dispose();
});

test("reading a single File for multiple roles does one read and preserves separate weight pins", async () => {
  const f = fixture(), chosen = file(), originalRead = chosen.arrayBuffer.bind(chosen);
  let reads = 0; chosen.arrayBuffer = () => { reads++; return originalRead(); };
  await f.author.assign([selected("body-regular", chosen, 400), selected("body-bold", chosen, 700)]);
  assert.equal(reads, 1); assert.deepEqual(f.collection.fonts.map(x => x.weight), [400, 700]);
  const snapshot = f.store.snapshot(); snapshot[0].bytes.fill(8);
  assert.deepEqual([...f.store.snapshot()[1].bytes], [0,1,2,255]); f.author.dispose();
});

test("replacing one role preserves all other fonts, source, images and explicit input ownership", async () => {
  const f = fixture({ delay: true });
  f.collection.setFonts([{ slot: "body-bold", name: "keep.ttf", bytes: new Uint8Array([9]) }], 0);
  const batch = [selected()], chosen = batch[0].file;
  const pending = f.author.assign(batch);
  batch[0].slot = "mono-regular"; batch[0].file = file("other.ttf"); batch.push(selected("body-italic"));
  await waiting(f.engine);
  f.engine.jobs[0].options.fontAssets[0].bytes.fill(17); // Worker cannot mutate install-owned bytes.
  f.engine.jobs[0].resolve(result()); await pending;
  assert.equal(f.collection.fonts.length, 2); assert.equal(f.collection.fonts[0].name, chosen.name);
  assert.deepEqual([...f.store.snapshot()[0].bytes], [0,1,2,255]);
  assert.deepEqual([...f.store.snapshot()[1].bytes], [9]); f.author.dispose();
});

test("batch metadata and final-set budgets reject before any selected file is read", async () => {
  const f = fixture(); let reads = 0;
  const chosen = file(); chosen.arrayBuffer = () => { reads++; throw Error("unexpected read"); };
  const invalid = [[], [selected("bad", chosen)], [selected("body-regular", chosen, 0)],
    [selected("body-regular", chosen, 1.5)], [selected("body-regular", chosen), selected("body-regular", chosen)],
    [selected("body-regular", file("font.woff"))], [selected("body-regular", file("zero.ttf", []))],
    new Array(2), BOOK_FONT_SLOTS.map(slot => selected(slot, chosen)).concat(selected())];
  for (const batch of invalid) await assert.rejects(f.author.assign(batch));
  const huge = file(); Object.defineProperty(huge, "size", { value: 8 * 1024 * 1024 + 1 });
  await assert.rejects(f.author.assign([selected("body-regular", huge)]), { code: "FONT_LIMIT" });
  for (const slot of BOOK_FONT_SLOTS.slice(0, 4))
    f.collection.setFonts([{ slot, name: `${slot}.ttf`, bytes: new Uint8Array(8 * 1024 * 1024) }], f.collection.revision);
  await assert.rejects(f.author.assign([selected("mono-regular", chosen)]), { code: "FONT_LIMIT" });
  assert.equal(reads, 0); assert.equal(f.engine.jobs.length, 0); f.author.dispose();
});

test("replacement at the 32 MiB store limit needs no extra retained capacity", async () => {
  const f = fixture();
  for (const slot of BOOK_FONT_SLOTS.slice(0, 4))
    f.collection.setFonts([{ slot, name: `${slot}.ttf`, bytes: new Uint8Array(8 * 1024 * 1024) }], f.collection.revision);
  await f.author.assign([selected()]);
  assert.equal(f.collection.fonts[0].size, 4); assert.equal(f.collection.fonts.length, 4); f.author.dispose();
});

test("one bad file or native rejection leaves the whole previous font set unchanged", async () => {
  for (const nativeFailure of [false, true]) {
    const f = fixture({ reject: nativeFailure ? Object.assign(Error("bad native font"), { code: "INVALID_FONT" }) : null });
    f.collection.setFonts([{ slot: "body-regular", name: "old.ttf", bytes: new Uint8Array([9]) }], 0);
    const before = f.store.snapshot(), revision = f.collection.revision;
    const broken = file(); broken.arrayBuffer = async () => { throw Error("I/O"); };
    await assert.rejects(f.author.assign([selected(), selected("body-bold", nativeFailure ? file() : broken)]));
    assert.deepEqual(f.store.snapshot(), before); assert.equal(f.collection.revision, revision);
    f.author.dispose();
  }
});

test("invalid preflight result never installs candidates or retains a busy slot", async () => {
  const cases = [null, {}, { ...result(), format: "book-epub" }, { ...result(), mimeType: "text/plain" },
    { ...result(), bytes: new Uint8Array(4) }, { ...result(), bytes: new Uint8Array(6) }];
  for (const output of cases) {
    const f = fixture({ delay: true }), pending = f.author.assign([selected()]);
    await waiting(f.engine); f.engine.jobs[0].resolve(output);
    await assert.rejects(pending, { code: "FONT_VALIDATION_FAILED" });
    assert.equal(f.collection.fonts.length, 0); assert.equal(f.author.busy, false); f.author.dispose();
  }
});

test("source edits, resource changes and silent editor changes fence in-flight file reads", async () => {
  for (const change of [f => f.edit("new"), f => f.change(), f => f.edit("raw", false)]) {
    const f = fixture(), read = deferred(), chosen = file(); chosen.arrayBuffer = () => read.promise;
    const pending = f.author.assign([selected("body-regular", chosen)]);
    await tick(); change(f); read.resolve(new Uint8Array([0,1,2,255]).buffer);
    await assert.rejects(pending, error => ["FONT_CANCELLED", "STALE_SOURCE"].includes(error.code));
    assert.equal(f.engine.jobs.length, 0); assert.equal(f.collection.fonts.length, 0); f.author.dispose();
  }
});

test("silent edits after preflight starts cannot install previously captured assignments", async () => {
  const f = fixture({ delay: true }), pending = f.author.assign([selected()]);
  await waiting(f.engine); f.edit("raw change", false); f.engine.jobs[0].resolve(result());
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(f.collection.fonts.length, 0); f.author.dispose();
});

test("cancel settles ignored-abort work and obsolete replies cannot cancel or replace a new job", async () => {
  const f = fixture({ delay: true }), old = f.author.assign([selected()]);
  await waiting(f.engine); f.author.cancel(); await assert.rejects(old, { code: "FONT_CANCELLED" });
  assert.equal(f.engine.jobs[0].signal.aborted, true);
  const fresh = f.author.assign([selected("mono-regular", file("new.ttf"))]);
  for (let i = 0; i < 50 && f.engine.jobs.length < 2; i++) await tick();
  f.engine.jobs[0].resolve(result()); await tick();
  assert.equal(f.author.busy, true); assert.equal(f.engine.jobs[1].signal.aborted, false);
  f.engine.jobs[1].resolve(result()); await fresh;
  assert.deepEqual(f.collection.fonts.map(x => x.slot), ["mono-regular"]); f.author.dispose();
});

test("abort and deadline settle hung reads and observe a late I/O rejection", async () => {
  for (const external of [false, true]) {
    const f = fixture({ timeoutMs: external ? 1000 : 10 }), read = deferred(), chosen = file();
    chosen.arrayBuffer = () => read.promise;
    const controller = new AbortController(), pending = f.author.assign([selected("body-regular", chosen)], { signal: controller.signal });
    await tick(); if (external) controller.abort();
    await assert.rejects(pending, { code: external ? "FONT_CANCELLED" : "FONT_TIMEOUT" });
    read.reject(Error("late read failure")); await tick();
    assert.equal(f.author.busy, false); assert.equal(f.collection.fonts.length, 0); f.author.dispose();
  }
});

test("busy admission, aborted signals and composition/import cannot start native work", async () => {
  const f = fixture({ delay: true });
  const controller = new AbortController(); controller.abort();
  await assert.rejects(f.author.assign([selected()], { signal: controller.signal }), { code: "FONT_CANCELLED" });
  await assert.rejects(f.author.assign([selected()], { signal: {} }), { code: "INVALID_OPTIONS" });
  f.busy(true); await assert.rejects(f.author.assign([selected()]), { code: "BOOK_BUSY" });
  f.busy(false); const pending = f.author.assign([selected()]); await waiting(f.engine);
  await assert.rejects(f.author.assign([selected()]), { code: "FONT_BUSY" });
  f.busy(true); await assert.rejects(pending, { code: "FONT_CANCELLED" });
  assert.equal(f.engine.jobs[0].signal.aborted, true); f.engine.jobs[0].reject(Error("late worker"));
  await tick(); f.author.dispose();
});

test("removal and clear require strict confirmation and preserve source and other slots", async () => {
  const f = fixture(); await f.author.assign([selected(), selected("body-bold")]);
  const source = f.collection.files[0].source, before = f.collection.revision;
  for (const answer of [undefined, false, "true", 1]) {
    assert.equal(await f.author.remove("body-regular", () => answer), false);
    assert.equal(f.collection.revision, before);
  }
  await assert.rejects(f.author.remove("body-regular"), { code: "INVALID_OPTIONS" });
  assert.equal(await f.author.remove("mono-regular", () => { assert.fail("absent role must not ask"); }), false);
  assert.equal(await f.author.remove("body-regular", () => true), true);
  assert.deepEqual(f.collection.fonts.map(x => x.slot), ["body-bold"]);
  assert.equal(await f.author.clear(() => true), true); assert.equal(f.collection.fonts.length, 0);
  assert.equal(await f.author.clear(() => true), false); assert.equal(f.collection.files[0].source, source);
  assert.equal(f.engine.jobs.length, 1); f.author.dispose();
});

test("asynchronous removal review cannot authorize deletion after changes or cancellation", async () => {
  for (const action of [f => f.author.cancel(), f => f.edit("changed", false), f => f.change()]) {
    const f = fixture(), confirm = deferred(); await f.author.assign([selected()]);
    const pending = f.author.remove("body-regular", () => confirm.promise); await tick();
    action(f); confirm.resolve(true);
    await assert.rejects(pending, error => ["FONT_CANCELLED", "STALE_SOURCE"].includes(error.code));
    assert.equal(f.collection.fonts.length, 1); f.author.dispose();
  }
});

test("suspension and disposal cancel pending authoring, leave installed fonts owned by the book", async () => {
  const f = fixture({ delay: true }), first = f.author.assign([selected()]); await waiting(f.engine);
  f.author.suspend(); await assert.rejects(first, { code: "FONT_CANCELLED" });
  await assert.rejects(f.author.assign([selected()]), { code: "FONT_CLOSED" });
  f.author.resume(); const second = f.author.assign([selected()]);
  for (let i = 0; i < 50 && f.engine.jobs.length < 2; i++) await tick();
  f.engine.jobs[1].resolve(result()); await second;
  f.author.dispose(); f.author.dispose();
  assert.equal(f.engine.disposals, 1); assert.deepEqual(f.subscribers(), [0,0]);
  assert.equal(f.collection.fonts.length, 1); f.engine.jobs[0].resolve(result()); await tick();
});

test("observer failures cannot block work, and observer cancellation prevents reads", async () => {
  const f = fixture(); f.author.subscribe(() => { throw Error("observer"); });
  const remove = f.author.subscribe(() => { if (f.author.phase === "reading") f.author.cancel(); });
  await assert.rejects(f.author.assign([selected()]), { code: "FONT_CANCELLED" });
  assert.equal(f.engine.jobs.length, 0); remove();
  await f.author.assign([selected()]); assert.equal(f.collection.fonts.length, 1); f.author.dispose();
});

test("capture fences reentrant starts before the current operation owns its checkpoint", async () => {
  const f = fixture(); let attempts = 0; const pending = [];
  f.collection.subscribe(() => {
    if (++attempts === 1) {
      pending.push(assert.rejects(f.author.assign([selected()]), { code: "FONT_BUSY" }));
      pending.push(assert.rejects(f.author.assign([selected()]), { code: "FONT_BUSY" }));
    }
  });
  f.edit("typed silently", false); await f.author.assign([selected()]); await Promise.all(pending);
  assert.equal(f.engine.jobs.length, 1); assert.equal(f.collection.files[0].source, "typed silently"); f.author.dispose();
});
