// Production controller with explicit boundary doubles for semantic-document
// admission and exports. These tests prove orchestration, not the reader codec,
// export encoder, native worker, or Markdown renderer. No imports are patched.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createContext, SourceTextModule, SyntheticModule } from "node:vm";
import * as boundary from "../flow_session.mjs";
const code = readFileSync(process.env.FMD_PREVIEW_MODULE || new URL("../demo/flow_preview_controller.mjs", import.meta.url), "utf8");
const context = createContext({ AbortController, queueMicrotask, Error, console });
const issued = new WeakSet();
const exports = {
  "../flow_session.mjs": boundary,
  "../flow_reading.mjs": {
    requireReadingDocument(document, owner) {
      if (!issued.has(document) || document.owner !== owner) throw new boundary.FlowError("INVALID_READING", "wrong reading owner");
      document.assertCurrent(); return document;
    },
    sourceSpanToUtf16: (_source, span) => ({start: span.start, end: span.end}),
  },
  "./flow_preview_export.mjs": { createPreviewExport: get => async () => get() },
};
const module = new SourceTextModule(code, {context});
await module.link(specifier => {
  assert.ok(exports[specifier], `unexpected production dependency: ${specifier}`);
  return new SyntheticModule(Object.keys(exports[specifier]), function () {
    for (const [name, value] of Object.entries(exports[specifier])) this.setExport(name, value);
  }, {context});
});
await module.evaluate();
const { createPreviewController } = module.namespace;
function gate() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
async function until(predicate) {
  for (let i = 0; i < 200; i++) { if (predicate()) return; await Promise.resolve(); }
  assert.fail("expected controller transition did not occur within the microtask budget");
}
const input = (source = "first", patch = {}) => ({source, width: 200, height: 100, ...patch});
const same = (a, b) => a.revision === b.revision && a.layoutRevision === b.layoutRevision;
class Session {
  disposed = false; _token = {revision: "1", layoutRevision: "1"}; replacements = []; reflows = [];
  constructor(source, layout) { this.source = source; this.layoutOptions = layout; }
  get revision() { return this._token.revision; }
  get token() { return {...this._token}; }
  async replaceSource(source, options) {
    assert.equal(options.expectedRevision, this.revision);
    this.source = source; this.replacements.push(source);
    this._token = {revision: String(BigInt(this.revision) + 1n), layoutRevision: String(BigInt(this._token.layoutRevision) + 1n)};
  }
  async reflow(layout, token) {
    assert.ok(same(token, this.token)); this.layoutOptions = {...layout}; this.reflows.push(layout);
    this._token = {...this._token, layoutRevision: String(BigInt(this._token.layoutRevision) + 1n)};
  }
  dispose() { this.disposed = true; }
}
function documentFor(owner, token = owner.token, text = owner.source) {
  const document = {owner, token: {...token}, text,
    assertCurrent() { if (owner.disposed || !same(owner.token, token)) throw new boundary.FlowError("STALE_LAYOUT", "old document"); },
    locate(index) { this.assertCurrent(); return {...this.token, nodeIndex: index,
      bounds: {x: 0, y: 10, width: 10, height: 20}, enclosingSourceSpan: {start: 0, end: text.length}}; }};
  issued.add(document); return document;
}
function fixture(t, options = {}) {
  const states = [], sessions = [], paints = [], reads = [];
  const painter = {frame: null, cleared: 0, disposed: false,
    async render(session, options) {
      const frame = {...session.token, width: options.width, scrollY: options.scrollY,
        totalBounds: {x: 0, y: 0, width: options.width, height: 100}, glyphs: 1};
      paints.push({session, options, frame});
      if (painter.wait) await painter.wait.promise;
      if (options.signal.aborted) throw new boundary.FlowError("ABORTED", "obsolete paint");
      this.frame = frame; return frame;
    }, clear() { this.frame = null; this.cleared++; }, dispose() { this.disposed = true; this.frame = null; }};
  const controller = createPreviewController({painter,
    async createSession(source, options) {
      const session = new Session(source, options); sessions.push(session); return session;
    },
    readDocument(session, options) {
      assert.deepEqual(Object.keys(options), ["token"], "never pass worker cancellation or ambient privileges");
      const request = {...gate(), session, token: options.token, source: session.source}; reads.push(request);
      return request.promise;
    },
    onState(state) { states.push(state); options.onState?.(state); }, ...options,
  });
  t.after(() => controller.dispose());
  return {controller, painter, states, sessions, paints, reads};
}
function finish(request, text = request.source) {
  request.resolve(documentFor(request.session, request.token, text));
}

test("first Canvas frame is ready before semantic I/O, while whenIdle waits for reading", async t => {
  const f = fixture(t); f.controller.update(input());
  await until(() => f.reads.length === 1);
  assert.equal(f.paints.length, 1);
  assert.equal(f.controller.state.status, "ready");
  assert.equal(f.controller.state.readingPending, true);
  assert.equal(f.controller.state.document, null);
  let idle = false;
  const pending = f.controller.whenIdle().then(state => { idle = true; return state; });
  await Promise.resolve(); assert.equal(idle, false);
  finish(f.reads[0]); const state = await pending;
  assert.equal(state.readingPending, false); assert.equal(state.document.text, "first");
  assert.equal(f.paints.length, 1, "reading arrival must not repaint");
  assert.equal((await f.controller.exportDocument()).status, "ready");
});

test("edits and paints coalesce without waiting for one slow semantic callback", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  for (let i = 0; i < 40; i++) f.controller.update(input(`edit-${i}`));
  await until(() => f.controller.state.frame?.revision === "2");
  assert.deepEqual(f.sessions[0].replacements, ["edit-39"]);
  assert.equal(f.reads.length, 1, "one physical callback, not a growing abandoned queue");
  assert.equal(f.controller.state.document, null);
  finish(f.reads[0]); await until(() => f.reads.length === 2);
  assert.equal(f.reads[1].source, "edit-39");
  finish(f.reads[1]); await f.controller.whenIdle();
  assert.equal(f.controller.state.document.text, "edit-39");
  assert.equal(f.paints.length, 2);
  assert.ok(!f.states.some(state => state.frame?.revision === "2" && state.document?.text === "first"));
});

test("scrolling reuses an in-flight reader and publishes against the latest camera", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  f.controller.update(input("first", {scrollY: 35}));
  await until(() => f.controller.state.frame?.scrollY === 35);
  finish(f.reads[0]); const state = await f.controller.whenIdle();
  assert.equal(state.frame.scrollY, 35); assert.equal(state.document.text, "first");
  assert.equal(f.reads.length, 1); assert.equal(f.sessions[0].reflows.length, 0);
});

test("same-layout reading arriving during a scroll paint is cached, not republished early", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  f.painter.wait = gate(); f.controller.update(input("first", {scrollY: 25}));
  await until(() => f.paints.length === 2);
  finish(f.reads[0]);
  for (let i = 0; i < 20; i++) await Promise.resolve();
  assert.equal(f.controller.state.status, "busy");
  assert.equal(f.controller.state.frame.scrollY, 0);
  f.painter.wait.resolve(); const state = await f.controller.whenIdle();
  assert.equal(state.frame.scrollY, 25); assert.equal(state.document.text, "first");
  assert.equal(f.reads.length, 1); assert.equal(state.readingPending, false);
});

test("resizing rejects old semantic geometry and reads only the newest completed layout", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  f.controller.update(input("first", {width: 160}));
  await until(() => f.controller.state.frame?.layoutRevision === "2");
  finish(f.reads[0]); await until(() => f.reads.length === 2);
  assert.equal(f.reads[1].token.layoutRevision, "2");
  finish(f.reads[1]); const state = await f.controller.whenIdle();
  assert.equal(state.document.token.layoutRevision, state.frame.layoutRevision);
});

for (const mode of ["restart", "font"]) {
  test(`${mode} paints a new session but never overlaps or adopts the old semantic job`, async t => {
    const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
    if (mode === "restart") f.controller.restart();
    else f.controller.update(input("first", {font: "serif"}));
    await until(() => f.sessions.length === 2 && f.controller.state.status === "ready");
    assert.equal(f.sessions[0].disposed, true); assert.equal(f.reads.length, 1);
    // Identical revision strings must not make data from two owners interchangeable.
    assert.ok(same(f.sessions[0].token, f.sessions[1].token));
    f.reads[0].reject(new Error("late old reader failure"));
    await until(() => f.reads.length === 2);
    assert.equal(f.reads[1].session, f.sessions[1]);
    finish(f.reads[1], "new-owner"); await f.controller.whenIdle();
    assert.equal(f.controller.state.document.owner, f.sessions[1]);
    assert.equal(f.controller.state.readingError, null);
  });
}

test("semantic failure leaves Canvas usable and is cached rather than retried on every scroll", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  f.reads[0].reject(new boundary.FlowError("BUDGET_EXCEEDED", "semantic budget"));
  let state = await f.controller.whenIdle();
  assert.equal(state.status, "ready"); assert.equal(state.readingError.code, "BUDGET_EXCEEDED");
  assert.equal(state.readingPending, false);
  f.controller.update(input("first", {scrollY: 30})); state = await f.controller.whenIdle();
  assert.equal(state.status, "ready"); assert.equal(f.reads.length, 1);
  f.controller.update(input("new")); await until(() => f.reads.length === 2);
  finish(f.reads[1]); state = await f.controller.whenIdle();
  assert.equal(state.readingError, null); assert.equal(state.document.text, "new");
});

test("legacy transcript pages no longer block ink and stop requesting stale pages", async t => {
  const first = gate(), calls = [];
  const f = fixture(t, {readDocument: null,
    async createSession(source, options) {
      const s = new Session(source, options);
      s.readingOrder = query => {
        calls.push(query);
        return calls.length === 1 ? first.promise : Promise.resolve({nodes: [{text: s.source}], nextOffset: null});
      };
      return s;
    }});
  f.controller.update(input()); await until(() => calls.length === 1);
  assert.equal(f.paints.length, 1);
  f.controller.update(input("second")); await until(() => f.controller.state.frame?.revision === "2");
  first.resolve({nodes: [{text: "old"}], nextOffset: 256});
  const state = await f.controller.whenIdle();
  assert.equal(state.reading, "second"); assert.equal(state.readingPending, false);
  assert.deepEqual(calls.map(call => call.offset), [0, 0]);
});

test("optional image loading and refreshed Canvas do not wait for semantic collection", async t => {
  const imageGate = gate(); let imageStarted = 0;
  const f = fixture(t, {createAssets: session => ({
    async loadPending() { imageStarted++; await imageGate.promise;
      session._token = {...session.token, layoutRevision: "2"};
      return {loaded: 1, failed: 0, skipped: 0, errors: []}; },
    whenIdle: async () => {}, synchronize() {}, resolveImage: () => null, dispose() {},
  })});
  f.controller.update(input()); await until(() => f.reads.length === 1 && imageStarted === 1);
  imageGate.resolve(); await until(() => f.controller.state.frame?.layoutRevision === "2");
  assert.equal(f.reads.length, 1); assert.equal(f.controller.state.images.loaded, 1);
  finish(f.reads[0]); await until(() => f.reads.length === 2); finish(f.reads[1]);
  const state = await f.controller.whenIdle(); assert.equal(state.document.token.layoutRevision, "2");
});

test("pending reading never authorizes navigation using an old displayed document", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  finish(f.reads[0]); const initial = await f.controller.whenIdle();
  assert.equal(f.controller.locateReading(0, initial.document, "first").sourceRange.end, 5);
  f.controller.update(input("second")); await until(() => f.reads.length === 2);
  assert.equal(f.controller.state.status, "ready"); assert.equal(f.controller.state.readingPending, true);
  assert.throws(() => f.controller.locateReading(0, initial.document, "second"), {code: "STALE_REVISION"});
  finish(f.reads[1]); await f.controller.whenIdle();
});

test("disposal settles waiters, observes late read rejection, and cannot resurrect UI", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  const idle = f.controller.whenIdle(); f.controller.dispose();
  assert.equal((await idle).status, "disposed"); const count = f.states.length;
  f.reads[0].reject(new Error("late reader"));
  for (let i = 0; i < 30; i++) await Promise.resolve();
  assert.equal(f.states.length, count); assert.equal(f.controller.state.readingPending, false);
  assert.equal(f.painter.disposed, true);
});

test("worker loss in the semantic job is reported without automatic session recreation", async t => {
  const f = fixture(t); f.controller.update(input()); await until(() => f.reads.length === 1);
  f.sessions[0].dispose(); f.reads[0].reject(new boundary.FlowError("SESSION_LOST", "gone"));
  const state = await f.controller.whenIdle();
  assert.equal(state.status, "error"); assert.equal(state.error.code, "SESSION_LOST");
  assert.equal(f.sessions.length, 1); assert.equal(f.paints.length, 1);
});


test("non-Error legacy reader failures remain visible instead of becoming silent empty text", async t => {
  const f = fixture(t, {readDocument: null, async createSession(source, options) {
    const session = new Session(source, options);
    session.readingOrder = () => Promise.reject(null);
    return session;
  }});
  f.controller.update(input()); const state = await f.controller.whenIdle();
  assert.equal(state.status, "ready"); assert.equal(state.readingError.code, "READING_ERROR");
  assert.equal(state.readingPending, false);
});

test("a throwing host observer cannot erase a successfully published reading snapshot", async t => {
  const f = fixture(t, {onState(state) { if (state.document) throw new Error("host observer"); }});
  f.controller.update(input()); await until(() => f.reads.length === 1);
  finish(f.reads[0]); const state = await f.controller.whenIdle();
  assert.equal(state.document.text, "first"); assert.equal(state.readingError, null);
  assert.equal(f.reads.length, 1); assert.equal(state.readingPending, false);
});
