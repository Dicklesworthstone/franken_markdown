// Tests the real live-preview orchestration with explicit synthetic session and
// painter doubles. The Rust renderer and Chromium pixels have separate tests.
import test from "node:test";
import assert from "node:assert/strict";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const next = () => new Promise(resolve => setImmediate(resolve));
const input = (source, options = {}) => ({ source, width: 360, height: 300, scrollY: 0, pixelRatio: 1, ...options });
function fixture() {
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
  f.controller = createPreviewController({createSession, painter, onState: state => f.events.push(state)});
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
