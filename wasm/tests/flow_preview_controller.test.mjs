// Tests the real live-preview orchestration with explicit synthetic session and
// painter doubles. The Rust renderer and Chromium pixels have separate tests.
import test from "node:test";
import assert from "node:assert/strict";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const next = () => new Promise(resolve => setImmediate(resolve));
const input = (source, options = {}) => ({ source, width: 360, height: 300, scrollY: 0, pixelRatio: 1, ...options });
function fixture(options = {}) {
  const f = { sessions: [], paints: [], events: [], mutations: [], reflows: [], reads: 0, clears: 0,
    replaceGate: null, createGate: null, readFailure: false, paintGate: null, disposedPaint: false };
  const createSession = async (source, layout) => {
    const s = { disposed: false, source, revision: "1", token: {revision: "1", layoutRevision: "1"}, layoutOptions: {...layout},
      dispose() { s.disposed = true; },
      async replaceSource(nextSource, options) {
        assert.equal(options.expectedRevision, s.revision);
        f.mutations.push(nextSource);
        if (f.replaceGate) { const gate = f.replaceGate; f.replaceGate = null; await gate.promise; }
        if (nextSource === "unsupported") throw Object.assign(new Error("unsupported script"), {code: "LAYOUT_ERROR"});
        if (nextSource === "lost") { s.disposed = true; throw Object.assign(new Error("worker lost"), {code: "SESSION_LOST"}); }
        s.source = nextSource; s.revision = String(Number(s.revision) + 1);
        s.token = {revision: s.revision, layoutRevision: String(Number(s.token.layoutRevision) + 1)};
      },
      async reflow(options, token) {
        assert.deepEqual(token, s.token); f.reflows.push(options.viewportWidth); s.layoutOptions = {...s.layoutOptions, ...options};
        s.token = {...s.token, layoutRevision: String(Number(s.token.layoutRevision) + 1)};
      },
      async readingOrder() {
        f.reads++;
        if (f.readFailure) throw new Error("reading failed");
        return {nodes: [{text: s.source}], nextOffset: null};
      }
    };
    f.sessions.push(s);
    if (f.createGate) { const gate = f.createGate; f.createGate = null; await gate.promise; }
    return s;
  };
  const painter = {
    async render(s, options) {
      if (f.paintGate) {
        const gate = f.paintGate; f.paintGate = null;
        await new Promise((resolve, reject) => {
          const aborted = () => reject(Object.assign(new Error("paint aborted"), {code: "ABORTED"}));
          options.signal.addEventListener("abort", aborted, {once: true});
          gate.promise.then(() => { options.signal.removeEventListener("abort", aborted); resolve(); });
        });
      }
      if (options.signal.aborted) throw Object.assign(new Error("paint aborted"), {code: "ABORTED"});
      const frame = {...s.token, scrollY: options.scrollY, width: options.width, height: options.height};
      f.paints.push({source: s.source, frame}); return frame;
    },
    clear() { f.clears++; }, dispose() { f.disposedPaint = true; }
  };
  f.controller = createPreviewController({createSession, painter, ...options, onState: state => f.events.push(state)});
  return f;
}
test("live edits coalesce to the latest source without speculative revision rebasing", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("first")); await c.whenIdle();
  const gate = deferred(); f.replaceGate = gate;
  c.update(input("second")); await next();
  c.update(input("third")); c.update(input("latest"));
  gate.resolve(); await c.whenIdle();
  assert.deepEqual(f.mutations, ["second", "latest"]);
  assert.deepEqual(f.paints.map(p => p.source), ["first", "latest"]);
  assert.equal(c.state.reading, "latest"); assert.equal(c.state.status, "ready"); c.dispose();
});
test("rejected source keeps the previous published preview and a later edit recovers", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("safe")); await c.whenIdle(); const before = c.state.frame;
  const authoritative = input("unsupported"); c.update(authoritative); await c.whenIdle();
  assert.equal(c.state.status, "error"); assert.equal(c.state.error.code, "LAYOUT_ERROR");
  assert.equal(c.state.frame, before); assert.equal(c.state.reading, "safe");
  assert.equal(authoritative.source, "unsupported"); assert.equal(f.sessions[0].source, "safe");
  c.update(input("corrected")); await c.whenIdle(); assert.equal(c.state.reading, "corrected"); c.dispose();
});
test("scrolling and height changes do not reparse or reflow an unchanged document", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("stable")); await c.whenIdle();
  c.update(input("stable", {scrollY: 100, height: 400})); await c.whenIdle();
  assert.deepEqual(f.mutations, []); assert.deepEqual(f.reflows, []); assert.equal(f.reads, 1);
  c.update(input("stable", {width: 720})); await c.whenIdle();
  assert.deepEqual(f.reflows, [720]); assert.equal(f.reads, 1); assert.equal(c.state.frame.revision, "1"); c.dispose();
});
test("a superseded pending paint never publishes an older desired viewport", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("stable")); await c.whenIdle();
  const gate = deferred(); f.paintGate = gate;
  c.update(input("stable", {scrollY: 10})); await next();
  c.update(input("stable", {scrollY: 200})); await c.whenIdle(); gate.resolve();
  assert.equal(c.state.frame.scrollY, 200);
  assert.deepEqual(f.paints.map(p => p.frame.scrollY), [0, 200]);
  assert(!f.sessions[0].disposed); c.dispose();
});
test("worker loss is not automatically replayed and explicit restart uses host source", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("safe")); await c.whenIdle();
  c.update(input("lost")); await c.whenIdle();
  assert.equal(c.state.error.code, "SESSION_LOST"); assert.equal(f.sessions.length, 1);
  c.restart(); await c.whenIdle();
  assert.equal(f.sessions.length, 2); assert(f.sessions[0].disposed);
  assert.equal(f.sessions[1].source, "lost"); assert.equal(c.state.reading, "lost"); assert.equal(f.clears, 1); c.dispose();
});
test("disposal during startup releases a late-created session and publishes no pixels", async () => {
  const f = fixture(), c = f.controller, gate = deferred(); f.createGate = gate;
  c.update(input("pending")); await next(); c.dispose(); gate.resolve(); await next();
  assert(c.disposed); assert(f.sessions[0].disposed); assert(f.disposedPaint);
  assert.equal(f.paints.length, 0); assert.equal(c.state.reading, "");
  assert.throws(() => c.update(input("late")), e => e.code === "SESSION_DISPOSED");
});
test("invalid input never replaces the previously admitted intent", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("good")); await c.whenIdle(); const before = c.state;
  for (const value of [input("bad", {width: NaN}), input("bad", {height: 0}), input("\ud800")]) assert.throws(() => c.update(value));
  assert.equal(c.state, before); c.restart(); await c.whenIdle(); assert.equal(f.sessions[1].source, "good"); c.dispose();
});
test("reading-data failure occurs before painting and leaves the old frame intact", async () => {
  const f = fixture(), c = f.controller;
  c.update(input("safe")); await c.whenIdle(); const before = c.state.frame; f.readFailure = true;
  c.update(input("new")); await c.whenIdle();
  assert.equal(c.state.status, "error"); assert.equal(c.state.frame, before);
  assert.deepEqual(f.paints.map(p => p.source), ["safe"]); c.dispose();
});

test("custom bundled font and measured typography reach session creation", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose());
  c.update(input("source unchanged", { font: "serif", bodySize: 18, codeSize: 16, lineHeight: 26 }));
  await c.whenIdle();
  assert.deepEqual(f.sessions[0].layoutOptions, { font: "serif", viewportWidth: 360, bodySize: 18, codeSize: 16, lineHeight: 26 });
  assert.equal(c.state.reading, "source unchanged"); assert.deepEqual(f.mutations, []);
});
for (const [name, value] of [["bodySize", 17], ["codeSize", 16], ["lineHeight", 25]]) {
  test(`${name} changes reflow without replacing source or rebuilding the worker`, async t => {
    const f = fixture(), c = f.controller; t.after(() => c.dispose());
    c.update(input("stable")); await c.whenIdle(); const token = { ...c.state.frame };
    c.update(input("stable", { [name]: value })); await c.whenIdle();
    assert.equal(f.sessions.length, 1); assert.equal(f.sessions[0].layoutOptions[name], value);
    assert.deepEqual(f.mutations, []); assert.deepEqual(f.reflows, [360]);
    assert.equal(c.state.frame.revision, token.revision); assert.notEqual(c.state.frame.layoutRevision, token.layoutRevision);
    assert.equal(f.reads, 1); assert.equal(f.clears, 0);
  });
}
test("font switching clears old glyph ownership then rebuilds the same source", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose());
  c.update(input("unchanged")); await c.whenIdle(); const old = f.sessions[0];
  c.update(input("unchanged", { font: "serif" }));
  assert.equal(old.disposed, true); assert.equal(c.state.frame, null); assert.equal(f.clears, 1);
  await c.whenIdle(); assert.equal(f.sessions.length, 2);
  assert.equal(f.sessions[1].layoutOptions.font, "serif"); assert.equal(f.sessions[1].source, "unchanged");
  assert.deepEqual(f.mutations, []); assert.equal(c.state.status, "ready");
});
test("typography is normalized to f32 before equality checks, preventing repeat reflows", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose());
  const intent = input("stable", { bodySize: 14.123456789, width: 360.123456789 });
  c.update(intent); await c.whenIdle(); const paints = f.paints.length;
  assert.equal(f.sessions[0].layoutOptions.bodySize, Math.fround(intent.bodySize));
  c.update({ ...intent, bodySize: Math.fround(intent.bodySize), width: Math.fround(intent.width) }); await c.whenIdle();
  assert.equal(f.paints.length, paints); assert.deepEqual(f.reflows, []);
});
test("invalid typography leaves admitted source and settings unchanged", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose());
  c.update(input("safe", { font: "serif", lineHeight: 23 })); await c.whenIdle(); const state = c.state;
  for (const change of [{ font: "custom" }, { font: null }, { bodySize: NaN }, { codeSize: 0 },
    { lineHeight: 12 }, { bodySize: 30 }, { lineHeight: Infinity }, { bodySize: "14" }, { lineHeight: 1000001 }]) {
    assert.throws(() => c.update(input("should not replace", change)));
    assert.equal(c.state, state);
  }
  c.restart(); await c.whenIdle();
  assert.equal(f.sessions[1].layoutOptions.font, "serif"); assert.equal(f.sessions[1].layoutOptions.lineHeight, 23);
  assert.equal(f.sessions[1].source, "safe");
});
test("latest typography and source coalesce while an older mutation is pending", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose());
  c.update(input("initial")); await c.whenIdle(); const hold = deferred(); f.replaceGate = hold;
  c.update(input("intermediate")); await next();
  c.update(input("latest", { bodySize: 17, lineHeight: 25 }));
  c.update(input("latest", { bodySize: 19, lineHeight: 28 })); hold.resolve(); await c.whenIdle();
  assert.deepEqual(f.mutations, ["intermediate", "latest"]); assert.deepEqual(f.reflows, [360]);
  assert.equal(f.sessions[0].layoutOptions.bodySize, 19); assert.equal(f.sessions[0].layoutOptions.lineHeight, 28);
  assert.deepEqual(f.paints.map(p => p.source), ["initial", "latest"]);
});
test("font change during startup disposes the late old session without publishing it", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose()); const hold = deferred(); f.createGate = hold;
  c.update(input("first")); await next();
  c.update(input("latest", { font: "serif", bodySize: 18 })); hold.resolve(); await c.whenIdle();
  assert.equal(f.sessions[0].disposed, true); assert.equal(f.sessions.length, 2);
  assert.equal(f.sessions[1].layoutOptions.font, "serif"); assert.equal(f.sessions[1].layoutOptions.bodySize, 18);
  assert.deepEqual(f.paints.map(p => p.source), ["latest"]);
});
function exportResult(expected) {
  return { schemaVersion: 1, ...expected, format: "pdf", mimeType: "application/pdf", bytes: new Uint8Array([1]),
    font: "sans", sourceLengthBytes: 6, assetCount: 0, assetBytes: 0, diagnostics: [] };
}
test("newly requested typography blocks export before the queued reflow even begins", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose()); let exports = 0;
  c.update(input("stable")); await c.whenIdle();
  f.sessions[0].exportDocument = async (_, __, expected) => { exports++; return exportResult(expected); };
  c.update(input("stable", { lineHeight: 24 }));
  await assert.rejects(c.exportDocument("pdf", {}, "stable"), { code: "STALE_LAYOUT" });
  await c.whenIdle(); await c.exportDocument("pdf", {}, "stable"); assert.equal(exports, 1);
});
test("an ABA settings change cannot publish an older export even without a physical reflow", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose()); const hold = deferred();
  c.update(input("stable")); await c.whenIdle();
  f.sessions[0].exportDocument = async (_, __, expected) => { await hold.promise; return exportResult(expected); };
  const job = c.exportDocument("pdf", {}, "stable");
  c.update(input("stable", { lineHeight: 24 })); c.update(input("stable")); hold.resolve();
  await assert.rejects(job, { code: "STALE_LAYOUT" }); await c.whenIdle(); assert.deepEqual(f.reflows, []);
});
test("scrolling during an export preserves session and layout identity", async t => {
  const f = fixture(), c = f.controller; t.after(() => c.dispose()); const hold = deferred();
  c.update(input("stable")); await c.whenIdle();
  f.sessions[0].exportDocument = async (_, __, expected) => { await hold.promise; return exportResult(expected); };
  const job = c.exportDocument("pdf", {}, "stable");
  c.update(input("stable", { scrollY: 25, height: 500, pixelRatio: 2 })); await c.whenIdle(); hold.resolve();
  assert.equal((await job).format, "pdf"); assert.deepEqual(f.reflows, []);
});
test("font rebuild retains the authorized asset factory and one physical loading slot", async t => {
  const hold = deferred(), owners = [], loaded = [];
  const f = fixture({ createAssets(session) {
    const owner = { disposed: false, async loadPending() { loaded.push(session); if (owners.length === 1) await hold.promise; return { loaded: 0 }; },
      async whenIdle() {}, synchronize() {}, resolveImage() { return null; }, dispose() { owner.disposed = true; } };
    owners.push(owner); return owner;
  } }), c = f.controller; t.after(() => c.dispose());
  c.update(input("image source")); await c.whenIdle(); assert.equal(loaded.length, 1);
  c.update(input("image source", { font: "serif" })); await c.whenIdle();
  assert.equal(owners[0].disposed, true); assert.equal(owners.length, 2); assert.equal(loaded.length, 1);
  hold.resolve(); await next(); await c.whenIdle(); await next();
  assert.equal(loaded.length, 2); assert.equal(loaded[1], f.sessions[1]); assert.equal(c.state.status, "ready");
});
