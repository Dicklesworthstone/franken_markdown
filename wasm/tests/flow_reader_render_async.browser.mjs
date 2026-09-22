// Real DOM, event-loop tasks, semantic admission and native Range selection.
// Only the native reading-page provider is synthetic; no generated WASM claim.
import { FlowReaderView, readFlowDocument } from "../flow-reader.js";
const reports = [];
const assert = (value, message = "assertion failed") => { if (!value) throw new Error(message); };
const equal = (a, b) => assert(JSON.stringify(a) === JSON.stringify(b), `${JSON.stringify(a)} != ${JSON.stringify(b)}`);
const delay = () => new Promise(resolve => setTimeout(resolve, 0));
const node = (role = "paragraph", text = "text", extra = {}) => ({ role, text,
  bounds: { x: 0, y: 0, width: 300, height: 20 }, enclosingSourceSpan: { startByte: 0, endByte: 10 },
  children: [], ...extra });
const style = extra => ({ bold: false, italic: false, code: false, strikethrough: false, ...extra });
const link = { target: "#end", activeTarget: "#end" };
const item = (listId = "1", itemIndex = 0, extra = {}) => ({ listId, itemIndex, ordered: true, start: "3", task: null, ...extra });
async function documentFor(roots = [node()]) {
  const session = { disposed: false, token: { revision: "1", layoutRevision: "1" }, calls: 0,
    readingOrder({ offset, limit, token }) {
      equal(token, this.token); this.calls++;
      const end = Math.min(offset + limit, roots.length);
      return { schemaVersion: 1, ...this.token, offset, total: roots.length,
        nextOffset: end < roots.length ? end : null, nodes: roots.slice(offset, end) };
    } };
  return { session, snapshot: await readFlowDocument(session) };
}
function view(onLink) {
  const root = document.createElement("article"); document.body.append(root);
  const reader = new FlowReaderView(root, onLink ? { onLink } : {});
  return { root, reader, close() { reader.dispose(); root.remove(); } };
}
async function rejects(promise, code) {
  try { await promise; } catch (error) { equal(error.code, code); return; }
  throw new Error(`expected ${code}`);
}
async function test(name, run) {
  try { await run(); reports.push({ name, pass: true }); }
  catch (error) { reports.push({ name, pass: false, error: String(error.stack ?? error) }); }
}
const large = async () => documentFor(Array.from({ length: 3000 }, (_, i) => node("paragraph", `Paragraph ${i}`)));
function styled(count, linked = true) {
  return node("paragraph", "x".repeat(count), { inlineRuns: Array.from({ length: count }, (_, i) => ({
    startByte: i, endByte: i + 1, style: style({ bold: !(i % 2), italic: !!(i % 2) }), link: linked ? link : null
  })) });
}

await test("small async render uses no timer, no native read and no implicit focus", async () => {
  const { snapshot, session } = await documentFor(), f = view(), original = globalThis.setTimeout;
  let timers = 0; globalThis.setTimeout = (...args) => { timers++; return original(...args); };
  try {
    const calls = session.calls; await f.reader.renderAsync(snapshot);
    equal(timers, 0); equal(session.calls, calls); equal(f.root.textContent, "text");
    assert(f.reader.document === snapshot); assert(document.activeElement !== f.root.firstChild);
  } finally { globalThis.setTimeout = original; f.close(); }
});

await test("sync and async preserve every semantic role and styled original Unicode selection", async () => {
  const text = "Straße 😀", runs = [
    { startByte: 0, endByte: 4, style: style({ bold: true }), link },
    { startByte: 4, endByte: 7, style: style({ italic: true, code: true }), link },
  ];
  const { snapshot } = await documentFor([
    node("heading", "End", { level: 6, anchorId: "end" }), node("paragraph", text, { inlineRuns: runs }),
    node("code-block", "  code\n\tindent"), node("blockquote", "summary", { children: [node("paragraph", "quote")] }),
    node("table-header-row", "header", { children: [node("table-header-cell", "A")] }),
    node("table-row", "row", { children: [node("table-cell", "B")] }),
    node("list", "summary", { children: [node("list-item", "old producer")] }),
    node("image", "", { imageLink: link }), node("thematic-break", ""),
  ]);
  const a = view(() => {}), b = view(() => {});
  try {
    a.reader.render(snapshot); await b.reader.renderAsync(snapshot); equal(b.root.innerHTML, a.root.innerHTML);
    assert(b.root.querySelector("h6") && b.root.querySelector("th[scope=col]") && b.root.querySelector("pre code"));
    assert(!b.root.querySelector("[href],[src]")); equal(b.root.querySelectorAll("a").length, 2);
    const match = snapshot.find("STRASSE", { caseInsensitive: true }).matches[0];
    equal(b.reader.selectMatch(match), "Straße"); equal(document.getSelection().toString(), "Straße");
  } finally { a.close(); b.close(); }
});

await test("structured nested lists, continuations, tables and exact large ordinals survive yields", async () => {
  const path = [item("1", 0, { start: "9007199254740993", task: true })];
  const roots = [node("list-item", "[x] Start", { listPath: path }),
    ...Array.from({ length: 300 }, (_, i) => node("paragraph", `Continuation ${i}`, { listPath: path })),
    node("list-item", "Nested", { listPath: [...path, item("2", 0, { ordered: false })] }),
    node("code-block", "exact\n code", { listPath: [...path, item("2", 0, { ordered: false })] }),
    node("table-row", "row", { listPath: path, children: [node("table-cell", "cell")] }),
    node("list-item", "Next", { listPath: [item("1", 1, { start: "9007199254740993" })] })];
  const { snapshot } = await documentFor(roots), a = view(), b = view();
  try {
    a.reader.render(snapshot); await b.reader.renderAsync(snapshot); equal(b.root.innerHTML, a.root.innerHTML);
    assert(b.root.querySelector("ol > li > ul > li > pre")); assert(b.root.querySelector("ol > li > table td"));
    equal([...b.root.querySelectorAll(".flow-reader-ordinal")].map(el => el.textContent), ["9007199254740993. ", "9007199254740994. "]);
    const task = b.root.querySelector("input"); assert(task.checked && task.disabled); equal(task.tabIndex, -1);
  } finally { a.close(); b.close(); }
});

await test("DOM input interrupts a large build while prior DOM and selection stay intact", async () => {
  const old = await documentFor(), next = await large(), f = view();
  f.reader.render(old.snapshot); f.reader.selectMatch(old.snapshot.find("text").matches[0]);
  const shown = f.root.firstChild, control = new AbortController(), button = document.createElement("button");
  button.addEventListener("click", () => control.abort());
  try {
    const pending = f.reader.renderAsync(next.snapshot, { signal: control.signal });
    const refused = rejects(pending, "ABORTED"); setTimeout(() => button.click(), 0); await refused;
    assert(f.root.firstChild === shown && f.reader.document === old.snapshot);
    equal(document.getSelection().toString(), "text"); assert(!next.session.disposed);
  } finally { f.close(); }
});

await test("one enormous styled leaf yields during logical-link grouping and element creation", async () => {
  for (const linked of [true, false]) {
    const { snapshot } = await documentFor([styled(12000, linked)]), f = view(() => {}), control = new AbortController();
    let turns = 0; const original = document.createElement; document.createElement = function(...args) { turns++; return original.apply(this, args); };
    try {
      const pending = f.reader.renderAsync(snapshot, { signal: control.signal });
      const refused = rejects(pending, "ABORTED"); setTimeout(() => control.abort(), 0); await refused;
      assert(turns < 12000, `constructed all ${turns} elements before input`); equal(f.root.childNodes.length, 0);
    } finally { document.createElement = original; f.close(); }
  }
});

await test("a table with one large descendant subtree is interruptible, not only root pages", async () => {
  const rows = Array.from({ length: 2500 }, () => node("table-row", "", { children: [node("table-cell", "cell")] }));
  const { snapshot } = await documentFor([node("table", "", { children: rows })]), f = view(), control = new AbortController();
  try {
    const pending = f.reader.renderAsync(snapshot, { signal: control.signal }), refused = rejects(pending, "ABORTED");
    setTimeout(() => control.abort(), 0); await refused; equal(f.root.childNodes.length, 0);
  } finally { f.close(); }
});

await test("discarded linked stages remove every registered listener", async () => {
  const { snapshot } = await documentFor(Array.from({ length: 1000 }, () => styled(1))), f = view(() => {});
  const add = EventTarget.prototype.addEventListener, remove = EventTarget.prototype.removeEventListener;
  let added = 0, removed = 0; const staged = [];
  EventTarget.prototype.addEventListener = function(type, ...args) {
    if (this instanceof HTMLAnchorElement) { added++; staged.push(this); } return add.call(this, type, ...args);
  };
  EventTarget.prototype.removeEventListener = function(type, ...args) {
    if (this instanceof HTMLAnchorElement) removed++; return remove.call(this, type, ...args);
  };
  try {
    const control = new AbortController();
    const pending = f.reader.renderAsync(snapshot, { signal: control.signal }), refused = rejects(pending, "ABORTED");
    setTimeout(() => control.abort(), 0); await refused;
    assert(added > 0); equal(added, removed); assert(staged.every(el => !el.isConnected));
  } finally { EventTarget.prototype.addEventListener = add; EventTarget.prototype.removeEventListener = remove; f.close(); }
});

await test("new async rendering supersedes old staging and cannot be overwritten late", async () => {
  const old = await large(), next = await documentFor([node("paragraph", "winner")]), f = view();
  try {
    const pending = f.reader.renderAsync(old.snapshot), refused = rejects(pending, "RENDER_SUPERSEDED");
    await f.reader.renderAsync(next.snapshot); await refused; await delay();
    equal(f.root.textContent, "winner"); assert(f.reader.document === next.snapshot);
  } finally { f.close(); }
});

await test("sync render and same-snapshot no-op both revoke a pending asynchronous replacement", async () => {
  const old = await documentFor(), next = await large(), f = view();
  try {
    for (const asynchronous of [false, true]) {
      f.reader.render(old.snapshot); const el = f.root.firstChild;
      const pending = f.reader.renderAsync(next.snapshot), refused = rejects(pending, "RENDER_SUPERSEDED");
      if (asynchronous) await f.reader.renderAsync(old.snapshot); else f.reader.render(old.snapshot);
      await refused; assert(f.root.firstChild === el); assert(f.reader.document === old.snapshot);
    }
  } finally { f.close(); }
});

await test("source edit, reflow and session disposal fence a resumed private tree", async () => {
  for (const action of ["source", "layout", "dispose"]) {
    const old = await documentFor(), next = await large(), f = view(); f.reader.render(old.snapshot);
    try {
      const pending = f.reader.renderAsync(next.snapshot);
      const refused = rejects(pending, action === "source" ? "STALE_REVISION" : action === "layout" ? "STALE_LAYOUT" : "SESSION_DISPOSED");
      setTimeout(() => {
        if (action === "dispose") next.session.disposed = true;
        else next.session.token = { ...next.session.token, [action === "source" ? "revision" : "layoutRevision"]: "2" };
      }, 0);
      await refused; assert(f.reader.document === old.snapshot); equal(f.root.textContent, "text");
    } finally { f.close(); }
  }
});

await test("clear and disposal prevent late DOM resurrection and release timer listeners", async () => {
  for (const action of ["clear", "dispose"]) {
    const { snapshot } = await large(), f = view(), control = new AbortController();
    const add = control.signal.addEventListener.bind(control.signal), remove = control.signal.removeEventListener.bind(control.signal);
    let balance = 0;
    control.signal.addEventListener = (...args) => { balance++; return add(...args); };
    control.signal.removeEventListener = (...args) => { balance--; return remove(...args); };
    try {
      const pending = f.reader.renderAsync(snapshot, { signal: control.signal });
      const refused = rejects(pending, action === "clear" ? "RENDER_SUPERSEDED" : "VIEW_DISPOSED");
      f.reader[action](); await refused; await delay(); equal(balance, 0);
      equal(f.root.childNodes.length, 0); equal(f.reader.document, null);
    } finally { f.close(); }
  }
});

await test("invalid and already-aborted attempts do not cancel an otherwise valid build", async () => {
  const { snapshot } = await large(), f = view();
  try {
    const pending = f.reader.renderAsync(snapshot);
    await rejects(f.reader.renderAsync(snapshot, { unexpected: true }), "INVALID_OPTIONS");
    await rejects(f.reader.renderAsync(snapshot, { signal: {} }), "INVALID_OPTIONS");
    await rejects(f.reader.renderAsync(snapshot, { signal: AbortSignal.abort() }), "ABORTED");
    await rejects(f.reader.renderAsync({ ...snapshot }), "INVALID_ARGUMENT");
    await pending; equal(f.root.children.length, 3000); assert(f.reader.document === snapshot);
  } finally { f.close(); }
});

await test("staged preparation failure retains prior DOM and permits explicit retry", async () => {
  const old = await documentFor(), next = await large(), f = view(); f.reader.render(old.snapshot);
  const original = document.createTextNode; let count = 0;
  document.createTextNode = function(...args) { if (++count === 180) throw new Error("allocation refused"); return original.apply(this, args); };
  try {
    let failed = false; try { await f.reader.renderAsync(next.snapshot); } catch (error) { failed = error.message === "allocation refused"; }
    assert(failed && f.reader.document === old.snapshot); equal(f.root.textContent, "text");
  } finally { document.createTextNode = original; }
  try { await f.reader.renderAsync(next.snapshot); equal(f.root.children.length, 3000); } finally { f.close(); }
});

await test("async publication preserves hostile text as text and links remain explicitly authorized", async () => {
  const { snapshot } = await documentFor([node("paragraph", '<img src=x onerror="boom">', { inlineRuns: [
    { startByte: 0, endByte: 24, style: style({ bold: true }), link: { target: "javascript:evil()", activeTarget: null } }
  ] }), styled(2)]), calls = [], f = view(value => calls.push(value));
  try {
    await f.reader.renderAsync(snapshot); assert(!f.root.querySelector("img,script,[href],[src]"));
    equal(f.root.querySelectorAll("a").length, 1); f.root.querySelector("a").click(); equal(calls.length, 1);
    equal(calls[0].target, "#end"); equal(f.root.firstChild.textContent, '<img src=x onerror="boom">');
  } finally { f.close(); }
});

await test("reentrant replacement during final DOM publication cannot overwrite newer view metadata", async () => {
  const old = await documentFor(), next = await documentFor([node("paragraph", "winner")]), f = view();
  const replace = f.root.replaceChildren.bind(f.root); let once = true;
  f.root.replaceChildren = (...args) => { replace(...args); if (once) { once = false; f.reader.render(next.snapshot); } };
  try {
    await rejects(f.reader.renderAsync(old.snapshot), "RENDER_SUPERSEDED");
    assert(f.reader.document === next.snapshot); equal(f.root.textContent, "winner");
  } finally { f.close(); }
});

const result = { passed: reports.filter(r => r.pass).length, failed: reports.filter(r => !r.pass).length, results: reports };
document.getElementById("result").textContent = JSON.stringify(result);
