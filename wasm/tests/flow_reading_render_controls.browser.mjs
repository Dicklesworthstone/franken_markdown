// Production reader/admission/search/controls in real Chromium DOM. Native
// semantic pages and deliberately delayed deliveries are explicit fixtures.
import { FlowReaderView, FlowReadingError, readFlowDocument } from "../flow-reader.js";
import { createReadingControls } from "../demo/flow_reading_controls.mjs";
import { node, ReadingSession, deferred } from "./flow_reading_fixtures.mjs";
const results = [], cleanups = [];
const assert = (value, message = "assertion failed") => { if (!value) throw new Error(message); };
const equal = (a, b) => assert(JSON.stringify(a) === JSON.stringify(b), `${JSON.stringify(a)} != ${JSON.stringify(b)}`);
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
async function until(check) { for (let i = 0; i < 100 && !check(); i++) await tick(); assert(check(), "expected progress"); }
async function test(name, run) {
  try { await run(); results.push({ name, pass: true }); }
  catch (error) { results.push({ name, pass: false, error: String(error.stack ?? error) }); }
  finally { for (const cleanup of cleanups.splice(0).reverse()) await cleanup(); document.getSelection().removeAllRanges(); }
}
async function doc(roots) { const session = new ReadingSession(roots); return { session, snapshot: await readFlowDocument(session) }; }
const many = (label = "large") => Array.from({ length: 2500 }, (_, i) => node("paragraph", `${label} ${i}`));
function spyRender(callback) {
  const original = FlowReaderView.prototype.renderAsync;
  FlowReaderView.prototype.renderAsync = function (...args) { return callback.call(this, original, ...args); };
  cleanups.push(() => { FlowReaderView.prototype.renderAsync = original; });
}
async function fixture() {
  const shell = document.createElement("section"); document.body.append(shell);
  const make = tag => { const el = document.createElement(tag); shell.append(el); return el; };
  const f = { shell, root: make("article"), panel: make("details"), query: make("input"),
    insensitive: make("input"), wholeWord: make("input"), previous: make("button"), next: make("button"),
    outline: make("select"), sourceButton: make("button"), status: make("p"), sourceEditor: make("textarea"),
    source: "original source", desired: null, state: "ready", navigations: [] };
  f.sourceEditor.value = f.source;
  f.options = { ...f, getLocation(index, snapshot) {
    if (f.state !== "ready" || snapshot !== f.desired || f.sourceEditor.value !== f.source)
      throw new FlowReadingError("STALE_REVISION", "host preview does not match source");
    return { ...snapshot.locate(index), sourceRange: { start: 0, end: 8 } };
  }, onNavigate: location => f.navigations.push(location) };
  f.controls = createReadingControls(f.options);
  f.ready = snapshot => { f.state = "ready"; f.desired = snapshot;
    f.controls.update({ status: "ready", document: snapshot }); };
  f.busy = () => { f.state = "busy"; f.controls.update({ status: "busy" }); };
  f.input = value => { f.query.value = value; f.query.dispatchEvent(new InputEvent("input")); };
  f.replace = async roots => { const next = await doc(roots); f.ready(next.snapshot); return next; };
  cleanups.push(() => { f.controls.dispose(); shell.remove(); });
  const initial = await f.replace([node("heading", "Start", { level: 1, anchorId: "start" }), node("paragraph", "old selectable")]);
  await f.controls.whenIdle(); f.initial = initial;
  return f;
}

await test("normal preview uses renderAsync, not the blocking compatibility path", async () => {
  let calls = 0; spyRender(function(original, ...args) { calls++; return original.apply(this, args); });
  const sync = FlowReaderView.prototype.render;
  FlowReaderView.prototype.render = () => { throw new Error("blocking render called"); };
  cleanups.push(() => { FlowReaderView.prototype.render = sync; });
  const f = await fixture(); equal(calls, 1); equal(f.root.querySelector("h1").textContent, "Start");
  f.input("selectable"); await f.controls.whenIdle(); f.next.click();
  equal(document.getSelection().toString(), "selectable"); equal(f.navigations.length, 1);
});

await test("large DOM builds return control immediately and preserve the last tree/selection", async () => {
  const f = await fixture(); f.input("selectable"); await f.controls.whenIdle(); f.next.click();
  const old = f.root.firstChild, range = document.getSelection().getRangeAt(0);
  const next = await doc(many()); f.ready(next.snapshot);
  assert(f.controls.busy); assert(f.next.disabled && f.outline.disabled);
  assert(f.root.firstChild === old); assert(document.getSelection().getRangeAt(0) === range);
  await f.controls.whenIdle(); equal(f.root.children.length, 2500); assert(f.root.firstChild !== old);
});

await test("hundreds of ready notices coalesce to one physical build", async () => {
  const f = await fixture(), next = await doc(many()); let calls = 0;
  spyRender(function(original, ...args) { calls++; return original.apply(this, args); });
  for (let i = 0; i < 100; i++) f.ready(next.snapshot);
  await f.controls.whenIdle(); equal(calls, 1); equal(f.root.children.length, 2500);
});

await test("only the latest same-turn snapshot enters the builder", async () => {
  const f = await fixture(), snapshots = [];
  for (let i = 0; i < 20; i++) snapshots.push((await doc([node("paragraph", `version ${i}`)])).snapshot);
  const seen = []; spyRender(function(original, snapshot, options) { seen.push(snapshot); return original.call(this, snapshot, options); });
  for (const snapshot of snapshots) f.ready(snapshot);
  await f.controls.whenIdle(); equal(seen.length, 1); assert(seen[0] === snapshots.at(-1));
  equal(f.root.textContent, "version 19");
});

await test("a cancelled physical builder keeps its slot until cleanup before latest replacement", async () => {
  const f = await fixture(), old = await doc(many()), newest = await doc([node("paragraph", "winner")]);
  const gate = deferred(); cleanups.push(() => gate.resolve()); let active = 0, max = 0, calls = 0, firstSignal;
  spyRender(async function(original, snapshot, options) {
    calls++; active++; max = Math.max(max, active);
    try { if (calls === 1) { firstSignal = options.signal; await gate.promise; }
      return await original.call(this, snapshot, options);
    } finally { active--; }
  });
  f.ready(old.snapshot); await until(() => calls === 1);
  f.ready(newest.snapshot); await tick(); equal(calls, 1); assert(firstSignal.aborted);
  gate.resolve(); await f.controls.whenIdle(); equal(calls, 2); equal(max, 1); equal(f.root.textContent, "winner");
});

await test("scroll-only busy/ready transitions do not restart an already active build", async () => {
  const f = await fixture(), next = await doc(many()); let calls = 0;
  spyRender(function(original, ...args) { calls++; return original.apply(this, args); });
  f.ready(next.snapshot); await until(() => calls === 1);
  for (let i = 0; i < 8; i++) { f.busy(); await tick(); f.ready(next.snapshot); }
  await f.controls.whenIdle(); equal(calls, 1); equal(f.root.children.length, 2500);
});

await test("a build finishing during a busy paint stays noninteractive until ready", async () => {
  const f = await fixture(), next = await doc(many()); let calls = 0;
  spyRender(function(original, ...args) { calls++; return original.apply(this, args); });
  f.ready(next.snapshot); await until(() => calls === 1); f.busy(); await f.controls.whenIdle();
  assert(f.next.disabled && f.outline.disabled); equal(f.root.children.length, 2500);
  f.ready(next.snapshot); f.input("large 1"); await f.controls.whenIdle(); f.next.click();
  equal(calls, 1); equal(document.getSelection().toString(), "large 1");
});

await test("a busy notification before deferred ingress does not poison that snapshot", async () => {
  const f = await fixture(), next = await doc(many());
  f.ready(next.snapshot); f.busy(); await f.controls.whenIdle();
  f.ready(next.snapshot); await f.controls.whenIdle(); equal(f.root.children.length, 2500);
  assert(!f.status.textContent.includes("STALE"));
});

await test("source input cancels building and prevents late DOM or navigation resurrection", async () => {
  const f = await fixture(), next = await doc(many()), old = f.root.firstChild;
  f.ready(next.snapshot); await tick();
  f.sourceEditor.value = "changed source"; f.sourceEditor.dispatchEvent(new Event("input"));
  await f.controls.whenIdle(); assert(f.root.firstChild === old); assert(f.next.disabled);
  equal(f.navigations.length, 0);
  f.source = f.sourceEditor.value;
  const replacement = await f.replace([node("paragraph", "current source")]); await f.controls.whenIdle();
  f.input("current"); await f.controls.whenIdle(); f.next.click();
  equal(document.getSelection().toString(), "current"); equal(replacement.session.calls.length, 1);
});

await test("a programmatic source change cannot enable navigation after a delayed completion", async () => {
  const f = await fixture(), next = await doc(many()), gate = deferred(); cleanups.push(() => gate.resolve());
  let entered = false;
  spyRender(async function(original, ...args) { entered = true; await gate.promise; return original.apply(this, args); });
  f.ready(next.snapshot); await until(() => entered); f.sourceEditor.value = "no input event";
  gate.resolve(); await f.controls.whenIdle(); assert(f.next.disabled && f.outline.disabled);
  assert(f.status.textContent.includes("STALE_REVISION")); equal(f.navigations.length, 0);
});

await test("reflow during building discards old geometry and a fresh snapshot recovers", async () => {
  const f = await fixture(), next = await doc(many()), old = f.root.firstChild;
  f.ready(next.snapshot); setTimeout(() => next.session.reflow(), 0); await f.controls.whenIdle();
  assert(f.root.firstChild === old); assert(f.status.textContent.includes("STALE_LAYOUT"));
  f.ready(await readFlowDocument(next.session)); await f.controls.whenIdle(); equal(f.root.children.length, 2500);
});

await test("new query and options during rendering are used once the current tree is ready", async () => {
  const f = await fixture(); await f.replace([...many(), node("paragraph", "Straße scatter cat")]);
  f.input("old"); f.input("STRASSE"); f.insensitive.checked = true;
  f.insensitive.dispatchEvent(new Event("change")); f.wholeWord.checked = true;
  f.wholeWord.dispatchEvent(new Event("change"));
  await f.controls.whenIdle(); f.next.click(); equal(document.getSelection().toString(), "Straße");
  equal(f.navigations.length, 1);
});

await test("old DOM remains selectable but links/headings do not navigate during replacement", async () => {
  const f = await fixture(); await f.replace([node("heading", "Start", { level: 1, anchorId: "start" }),
    node("paragraph", "Go", { inlineRuns: [{ startByte: 0, endByte: 2,
      style: { bold: false, italic: false, code: false, strikethrough: false },
      link: { target: "#start", activeTarget: "#start" } }] })]); await f.controls.whenIdle();
  const link = f.root.querySelector("a"); const next = await doc(many()); f.ready(next.snapshot);
  link.click(); f.outline.value = "0"; f.outline.dispatchEvent(new Event("change"));
  equal(f.navigations.length, 0); await f.controls.whenIdle();
});

await test("outline options are built cooperatively before publishing the replacement", async () => {
  const f = await fixture(), old = f.root.firstChild, choices = f.outline.firstChild;
  const next = await doc(Array.from({ length: 3000 }, (_, i) => node("heading", `Section ${i}`, { level: 2 })));
  let created = 0; const make = document.createElement;
  document.createElement = function(tag, ...args) { if (tag === "option") created++; return make.call(this, tag, ...args); };
  cleanups.push(() => { document.createElement = make; });
  f.ready(next.snapshot); await tick();
  assert(created > 0 && created < 3001); assert(f.root.firstChild === old && f.outline.firstChild === choices);
  f.sourceEditor.value = "changed"; f.sourceEditor.dispatchEvent(new Event("input"));
  await f.controls.whenIdle(); assert(f.outline.firstChild === choices); assert(f.root.firstChild === old);
});

await test("failed DOM preparation retains the old reader and does not retry on every ready notice", async () => {
  const f = await fixture(), next = await doc(many()), old = f.root.firstChild; let calls = 0;
  spyRender(function() { calls++; throw new Error("private host error"); });
  f.ready(next.snapshot); await f.controls.whenIdle();
  for (let i = 0; i < 20; i++) f.ready(next.snapshot);
  await f.controls.whenIdle(); equal(calls, 1); assert(f.root.firstChild === old);
  assert(f.next.disabled && f.status.textContent.startsWith("READING_ERROR"));
  assert(!f.status.textContent.includes("private host"));
});

await test("missing asynchronous reader support is explicit, never a blocking fallback", async () => {
  const f = await fixture(), next = await doc(many());
  const asyncRender = FlowReaderView.prototype.renderAsync, syncRender = FlowReaderView.prototype.render; let syncCalls = 0;
  FlowReaderView.prototype.renderAsync = undefined;
  FlowReaderView.prototype.render = () => { syncCalls++; };
  cleanups.push(() => { FlowReaderView.prototype.renderAsync = asyncRender; FlowReaderView.prototype.render = syncRender; });
  f.ready(next.snapshot); await f.controls.whenIdle(); equal(syncCalls, 0);
  assert(f.status.textContent.startsWith("UNSUPPORTED_READER"));
});

await test("disposed controls cannot clobber a remounted view after delayed cleanup", async () => {
  const f = await fixture(), next = await doc(many()), gate = deferred(); cleanups.push(() => gate.resolve()); let entered = false;
  spyRender(async function(original, snapshot, options) {
    if (!entered) { entered = true; await gate.promise; } return original.call(this, snapshot, options);
  });
  f.ready(next.snapshot); await until(() => entered); const previous = f.controls, old = previous.whenIdle();
  previous.dispose(); f.controls = createReadingControls(f.options);
  const winner = await doc([node("paragraph", "remounted")]); f.ready(winner.snapshot); await f.controls.whenIdle();
  gate.resolve(); await old; equal(f.root.textContent, "remounted"); assert(!f.controls.busy);
});

await test("completed reader DOM, search results and native selection survive scrolling without rebuilds", async () => {
  let renders = 0; spyRender(function(original, ...args) { renders++; return original.apply(this, args); });
  const f = await fixture(); f.input("selectable"); await f.controls.whenIdle(); f.next.click();
  const old = f.root.firstChild, range = document.getSelection().getRangeAt(0);
  for (let i = 0; i < 20; i++) { f.busy(); f.ready(f.initial.snapshot); }
  await f.controls.whenIdle(); equal(renders, 1); assert(f.root.firstChild === old);
  assert(document.getSelection().getRangeAt(0) === range); assert(!f.next.disabled);
});

const report = { passed: results.filter(r => r.pass).length, failed: results.filter(r => !r.pass).length, results };
document.getElementById("result").textContent = JSON.stringify(report);
