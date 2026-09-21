// Actual DOM, selection and scheduling over the production reader and controls.
// Only native semantic pages are synthetic. No generated Rust/WASM claim.
import { FlowReadingDocument, FlowReadingError, readFlowDocument } from "../flow-reader.js";
import { createReadingControls } from "../demo/flow_reading_controls.mjs";
import { node, ReadingSession, deferred } from "./flow_reading_fixtures.mjs";
const results = [], cleanups = [];
const assert = (condition, message = "assertion failed") => { if (!condition) throw new Error(message); };
const equal = (a, b) => assert(JSON.stringify(a) === JSON.stringify(b), `${JSON.stringify(a)} != ${JSON.stringify(b)}`);
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
const selected = () => document.getSelection().toString();
async function test(name, run) {
  try { await run(); results.push({ name, pass: true }); }
  catch (error) { results.push({ name, pass: false, error: String(error.stack ?? error) }); }
  finally { for (const cleanup of cleanups.splice(0).reverse()) await cleanup(); document.getSelection().removeAllRanges(); }
}
function wrapSearch(callback) {
  const original = FlowReadingDocument.prototype.findAsync;
  FlowReadingDocument.prototype.findAsync = function (...args) { return callback.call(this, original, ...args); };
  cleanups.push(() => { FlowReadingDocument.prototype.findAsync = original; });
}
async function fixture(roots = [node("paragraph", "Straße STRASSE")], extra = {}) {
  const shell = document.createElement("section"); document.body.append(shell);
  const make = tag => { const el = document.createElement(tag); shell.append(el); return el; };
  const root = make("article"), panel = make("details"), query = make("input"), insensitive = make("input"), wholeWord = make("input");
  query.type = "search"; insensitive.type = wholeWord.type = "checkbox";
  const previous = make("button"), next = make("button"), outline = make("select"), sourceButton = make("button");
  const status = make("p"), sourceEditor = make("textarea"); sourceEditor.value = "Authoritative markdown source";
  const session = new ReadingSession(roots), snapshot = await readFlowDocument(session);
  const f = { root, panel, query, insensitive, wholeWord, previous, next, outline, sourceButton, status, sourceEditor,
    session, snapshot, source: sourceEditor.value, navigations: [], locationCalls: 0 };
  const options = { root, panel, query, insensitive, wholeWord, previous, next, outline, sourceButton, status, sourceEditor,
    getLocation(index, doc) {
      if (doc !== f.snapshot || sourceEditor.value !== f.source)
        throw new FlowReadingError("STALE_REVISION", "unsubmitted source or old document");
      f.locationCalls++;
      return { ...doc.locate(index), sourceRange: { start: 0, end: 10 } };
    }, onNavigate(location) { f.navigations.push(location); }, ...extra };
  f.options = options;
  f.controls = createReadingControls(options);
  f.ready = () => f.controls.update({ status: "ready", document: f.snapshot });
  f.input = value => { query.value = value; query.dispatchEvent(new InputEvent("input", { bubbles: true })); };
  f.enter = flags => query.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true, ...flags }));
  f.setCase = value => { insensitive.checked = value; insensitive.dispatchEvent(new Event("change")); };
  f.setWhole = value => { wholeWord.checked = value; wholeWord.dispatchEvent(new Event("change")); };
  f.refresh = async () => { f.snapshot = await readFlowDocument(session); f.ready(); await f.controls.whenIdle(); };
  f.ready();
  cleanups.push(() => { f.controls.dispose(); shell.remove(); });
  return f;
}
const bold = { bold: true, italic: false, code: false, strikethrough: false };
const italic = { bold: false, italic: true, code: false, strikethrough: false };

await test("Unicode Find selects original text across actual styled DOM ranges", async () => {
  const f = await fixture([node("paragraph", "Straße Straße", { inlineRuns: [
    { startByte: 0, endByte: 4, style: bold, link: null }, { startByte: 4, endByte: 7, style: italic, link: null }
  ] })]);
  f.setCase(true); f.input("STRASSE"); f.enter();
  assert(f.controls.busy); assert(f.next.disabled);
  await f.controls.whenIdle();
  equal(selected(), "Straße"); equal(f.navigations.length, 1); assert(f.status.textContent.startsWith("1 of 2"));
  assert(f.root.querySelector("strong") && f.root.querySelector("em"));
  f.next.click(); equal(selected(), "Straße"); assert(f.status.textContent.startsWith("2 of 2"));
  f.sourceButton.click(); equal(f.sourceEditor.selectionStart, 0); equal(f.sourceEditor.selectionEnd, 10);
  equal(f.sourceEditor.value, f.source);
});

await test("whole-word and Unicode options are connected to the live search", async () => {
  const f = await fixture([node("paragraph", "cat scatter cat_cat cat2 CAT éÉ")]);
  f.input("cat"); await f.controls.whenIdle(); assert(f.status.textContent.includes("of 5 matches"));
  f.setWhole(true); await f.controls.whenIdle(); assert(f.status.textContent.includes("of 1 matches"));
  f.setCase(true); await f.controls.whenIdle(); assert(f.status.textContent.includes("of 2 matches"));
  f.input("éé"); await f.controls.whenIdle(); f.next.click(); equal(selected(), "éÉ");
});

await test("same-turn edits coalesce before starting the production matcher", async () => {
  const calls = []; wrapSearch(function (original, query, options) { calls.push(query); return original.call(this, query, options); });
  const f = await fixture([node("paragraph", "latest")]);
  for (let i = 0; i < 100; i++) f.input(`old${i}`);
  f.input("latest"); await f.controls.whenIdle(); equal(calls, ["latest"]);
  f.next.click(); equal(selected(), "latest");
});

await test("a late successful old query cannot replace the newest results or selection", async () => {
  const gate = deferred(); cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, query, options) {
    const result = await original.call(this, query, options);
    if (query === "old") await gate.promise; // Deliberately ignores cancellation after producing genuine matches.
    return result;
  });
  const f = await fixture([node("paragraph", "old new")]);
  f.input("old"); await tick(); const old = f.controls.whenIdle();
  f.input("new"); f.enter(); await f.controls.whenIdle(); const status = f.status.textContent;
  gate.resolve(); await old; equal(selected(), "new"); equal(f.status.textContent, status); equal(f.navigations.length, 1);
});

await test("a late rejected old query is observed without clobbering new controls", async () => {
  const gate = deferred(); cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, query, options) {
    if (query === "old") { await gate.promise; throw new Error("private late failure"); }
    return original.call(this, query, options);
  });
  const f = await fixture([node("paragraph", "new")]);
  f.input("old"); await tick(); const old = f.controls.whenIdle();
  f.input("new"); await f.controls.whenIdle(); const status = f.status.textContent;
  gate.resolve(); await old; equal(f.status.textContent, status); assert(!f.next.disabled);
});

await test("same-snapshot ready notifications neither restart nor erase a pending search", async () => {
  const gate = deferred(); let calls = 0; cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, query, options) { calls++; const result = await original.call(this, query, options); await gate.promise; return result; });
  const f = await fixture(); f.input("Straße"); await tick();
  for (let i = 0; i < 30; i++) f.ready();
  equal(calls, 1); assert(f.controls.busy); assert(f.status.textContent.startsWith("Searching"));
  gate.resolve(); await f.controls.whenIdle(); assert(!f.next.disabled);
});

await test("completed matches and native selection survive scroll-only pause/resume", async () => {
  let calls = 0; wrapSearch(function (original, ...args) { calls++; return original.apply(this, args); });
  const f = await fixture(); f.input("Straße"); await f.controls.whenIdle(); f.next.click();
  const paragraph = f.root.firstElementChild, range = document.getSelection().getRangeAt(0);
  f.controls.update({ status: "busy" }); assert(f.next.disabled); f.ready(); await f.controls.whenIdle();
  equal(calls, 1); equal(selected(), "Straße"); assert(f.root.firstElementChild === paragraph);
  assert(document.getSelection().getRangeAt(0) === range); assert(!f.next.disabled);
});

await test("pausing a pending search cancels it; resume starts one fresh search", async () => {
  const gate = deferred(); let calls = 0, firstSignal; cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, query, options) {
    calls++; const result = await original.call(this, query, options);
    if (calls === 1) { firstSignal = options.signal; await gate.promise; }
    return result;
  });
  const f = await fixture(); f.input("Straße"); f.enter(); await tick(); const old = f.controls.whenIdle();
  f.controls.update({ status: "busy" }); assert(firstSignal.aborted); f.ready(); await f.controls.whenIdle();
  equal(calls, 2); equal(f.navigations.length, 0); gate.resolve(); await old; equal(f.navigations.length, 0);
});

await test("source input immediately cancels queued navigation and fresh source can recover", async () => {
  const gate = deferred(); let first = true; cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, ...args) {
    const result = await original.apply(this, args);
    if (first) { first = false; await gate.promise; }
    return result;
  });
  const f = await fixture(); f.input("Straße"); f.enter(); await tick(); const old = f.controls.whenIdle();
  f.sourceEditor.value = "new source"; f.sourceEditor.dispatchEvent(new Event("input"));
  f.ready(); assert(f.next.disabled); gate.resolve(); await old; equal(f.navigations.length, 0);
  f.source = f.sourceEditor.value; f.session.edit(); await f.refresh();
  f.next.click(); equal(selected(), "Straße"); equal(f.navigations.length, 1);
});

await test("even programmatic unsubmitted source changes discard a late search", async () => {
  const gate = deferred(); cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, ...args) { const result = await original.apply(this, args); await gate.promise; return result; });
  const f = await fixture(); f.input("Straße"); f.enter(); await tick();
  f.sourceEditor.value = "no input event"; gate.resolve(); await f.controls.whenIdle();
  equal(f.navigations.length, 0); assert(f.next.disabled); assert(f.status.textContent.includes("Source changed"));
});

await test("Enter retains only the latest pending direction and ignores repeat/composition/modifiers", async () => {
  const gate = deferred(); cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, ...args) { const result = await original.apply(this, args); await gate.promise; return result; });
  const f = await fixture([node("paragraph", "a a")]); f.input("a");
  f.enter(); f.enter({ shiftKey: true }); f.enter({ repeat: true }); f.enter({ isComposing: true }); f.enter({ ctrlKey: true });
  gate.resolve(); await f.controls.whenIdle(); assert(f.status.textContent.startsWith("2 of 2")); equal(f.navigations.length, 1);
});

await test("IME composition never searches or moves through intermediate text", async () => {
  const calls = []; wrapSearch(function (original, query, options) { calls.push(query); return original.call(this, query, options); });
  const f = await fixture([node("paragraph", "猫")]);
  f.query.dispatchEvent(new CompositionEvent("compositionstart"));
  f.query.value = "m"; f.query.dispatchEvent(new InputEvent("input", { isComposing: true })); f.enter();
  f.query.value = "猫"; f.query.dispatchEvent(new InputEvent("input", { isComposing: true }));
  await tick(); equal(calls, []); equal(f.navigations.length, 0);
  f.query.dispatchEvent(new CompositionEvent("compositionend"));
  f.query.dispatchEvent(new InputEvent("input")); f.enter(); await f.controls.whenIdle();
  equal(calls, ["猫"]); equal(selected(), "猫");
});

await test("semantic reading-pending state preserves old readable DOM but pauses navigation", async () => {
  const f = await fixture(); f.input("Straße"); await f.controls.whenIdle(); f.next.click();
  const old = f.root.firstElementChild;
  f.controls.update({ status: "ready", readingPending: true, document: null });
  assert(old === f.root.firstElementChild); equal(selected(), "Straße"); assert(f.next.disabled);
  assert(f.status.textContent.startsWith("Collecting semantic")); f.ready(); assert(!f.next.disabled);
});

await test("new snapshots and disposal prevent late work from resurrecting old DOM/status", async () => {
  const gate = deferred(); let first = true; cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, ...args) {
    const result = await original.apply(this, args); if (first) { first = false; await gate.promise; } return result;
  });
  const f = await fixture(); f.input("Straße"); f.enter(); await tick(); const old = f.controls.whenIdle();
  f.controls.dispose();
  f.controls = createReadingControls(f.options); f.input("STRASSE"); f.ready(); await f.controls.whenIdle();
  f.next.click(); const status = f.status.textContent, paragraph = f.root.firstElementChild;
  gate.resolve(); await old;
  equal(selected(), "STRASSE"); equal(f.status.textContent, status); assert(f.root.firstElementChild === paragraph);
  equal(f.navigations.length, 1);
  f.controls.dispose(); f.query.dispatchEvent(new Event("input")); f.next.click(); await tick();
  equal(f.root.children.length, 0); assert(!f.controls.busy);
});

await test("heading and internal-link navigation remain connected during asynchronous Find", async () => {
  const gate = deferred(); cleanups.push(() => gate.resolve());
  wrapSearch(async function (original, ...args) { const result = await original.apply(this, args); await gate.promise; return result; });
  const f = await fixture([node("heading", "Title", { level: 2, anchorId: "title" }),
    node("paragraph", "Go", { inlineRuns: [{ startByte: 0, endByte: 2, style: bold,
      link: { target: "#title", activeTarget: "#title" } }] })]);
  f.input("Go"); f.enter(); await tick();
  f.root.querySelector("a").click(); equal(f.navigations.length, 1); equal(f.navigations[0].nodeIndex, 0);
  gate.resolve(); await f.controls.whenIdle(); equal(f.navigations.length, 1); assert(!f.sourceButton.disabled);
  f.outline.value = "0"; f.outline.dispatchEvent(new Event("change")); equal(f.navigations.length, 2);
});

await test("search and DOM failures leave source intact and a valid later query recovers", async () => {
  const f = await fixture(); const paragraph = f.root.firstElementChild;
  f.input("a".repeat(1025)); await f.controls.whenIdle(); assert(f.status.textContent.startsWith("INVALID_ARGUMENT"));
  assert(f.next.disabled); assert(paragraph === f.root.firstElementChild);
  f.input("Straße"); await f.controls.whenIdle(); assert(!f.next.disabled);
  paragraph.append("host mutation"); f.next.click(); assert(f.status.textContent.startsWith("READING_DOM_CHANGED"));
  equal(f.sourceEditor.value, f.source); equal(f.navigations.length, 0);
});

await test("no blocking fallback is used when a mismatched reader lacks findAsync", async () => {
  const original = FlowReadingDocument.prototype.findAsync;
  FlowReadingDocument.prototype.findAsync = undefined;
  cleanups.push(() => { FlowReadingDocument.prototype.findAsync = original; });
  const f = await fixture(); f.input("Straße"); await f.controls.whenIdle();
  assert(f.status.textContent.startsWith("UNSUPPORTED_READER")); assert(f.next.disabled);
});

await test("actual large-leaf search is interrupted by new browser input", async () => {
  const f = await fixture([node("paragraph", "a ".repeat(1200) + "x".repeat(990000))]);
  f.input("absent"); await tick(); assert(f.controls.busy);
  const old = f.controls.whenIdle(); f.input("a"); f.enter();
  await f.controls.whenIdle(); await old;
  assert(f.status.textContent.startsWith("1 of 1000+")); equal(selected(), "a"); equal(f.navigations.length, 1);
  equal(f.session.calls.length, 1); assert(!f.session.disposed);
});

await test("source mutation during focus cannot navigate with the pre-focus location", async () => {
  const f = await fixture(); f.input("Straße"); await f.controls.whenIdle();
  f.root.firstElementChild.addEventListener("focus", () => {
    f.sourceEditor.value = "changed on focus"; f.sourceEditor.dispatchEvent(new Event("input"));
  }, { once: true });
  f.next.click(); equal(f.navigations.length, 0); assert(f.next.disabled);
});

const report = { passed: results.filter(result => result.pass).length, failed: results.filter(result => !result.pass).length, results };
const output = document.querySelector("#result"); output.textContent = JSON.stringify(report);
output.dataset.status = report.failed ? "failed" : "passed";
