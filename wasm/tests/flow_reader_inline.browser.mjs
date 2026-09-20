// Real DOM/Range/events over the production reader. Native-wire sessions are
// explicit doubles; this suite does not claim Rust/generated-WASM execution.
import { FlowReaderView, readFlowDocument } from "../flow-reader.js";
import { node, ReadingSession } from "./flow_reading_fixtures.mjs";
const assert = (ok, message = "assertion failed") => { if (!ok) throw new Error(message); };
const equal = (actual, expected) => assert(actual === expected, `${JSON.stringify(actual)} !== ${JSON.stringify(expected)}`);
function throws(call, code) {
  try { call(); } catch (error) { equal(error.code, code); return; }
  throw new Error(`expected ${code}`);
}
const style = extra => ({ bold: false, italic: false, code: false, strikethrough: false, ...extra });
const run = (startByte, endByte, extra = {}, link = null) => ({ startByte, endByte, style: style(extra), link });
const target = (text, activeTarget = text) => ({ target: text, activeTarget });
const inline = (role, text, inlineRuns, extra = {}) => node(role, text, { inlineRuns, ...extra });
const linked = () => inline("paragraph", "one TWO three", [
  run(0, 4, { bold: true }, target("#section")),
  run(4, 7, { italic: true, code: true }, target("#section")),
  run(7, 13, { strikethrough: true }, target("#section"))
]);
const click = (element, options = {}) => element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ...options }));
const enter = (element, options = {}) => element.dispatchEvent(new KeyboardEvent("keydown", {
  key: "Enter", bubbles: true, cancelable: true, ...options
}));

export async function runTests() {
  const passed = [], cleanup = [];
  async function view(roots, options = {}) {
    const container = document.createElement("section"); document.body.append(container);
    const session = new ReadingSession(roots), snapshot = await readFlowDocument(session);
    const reader = new FlowReaderView(container, options); reader.render(snapshot);
    cleanup.push(() => { reader.dispose(); container.remove(); });
    return { container, session, snapshot, reader };
  }
  async function test(name, body) {
    try { await body(); passed.push(name); }
    finally { while (cleanup.length) cleanup.pop()(); document.getSelection()?.removeAllRanges(); }
  }
  await test("semantic inline tags survive headings, quotes, task markers and table cells", async () => {
    const { container } = await view([
      inline("heading", "Title", [run(0, 5, { bold: true, italic: true })], { level: 2 }),
      inline("blockquote", "Quote", [run(0, 5, { strikethrough: true })]),
      inline("list-item", "[x] task", [run(4, 8, { bold: true })]),
      node("table-header-row", "head", { children: [inline("table-header-cell", "head", [run(0, 4, { italic: true })])] }),
      node("table-row", "cell", { children: [inline("table-cell", "cell", [run(0, 4, { code: true })])] })
    ]);
    equal(container.querySelector("h2 strong em").textContent, "Title");
    equal(container.querySelector("blockquote s").textContent, "Quote");
    equal(container.querySelector('[role="listitem"]').textContent, "[x] task");
    equal(container.querySelector('[role="listitem"] strong').textContent, "task");
    equal(container.querySelector('th[scope="col"] em').textContent, "head");
    equal(container.querySelector("td code").textContent, "cell");
  });
  await test("native selection crosses Unicode and style boundaries without losing text", async () => {
    const { reader, snapshot } = await view([inline("paragraph", "A é😀z tail", [
      run(2, 4, { bold: true }), run(4, 8, { italic: true }), run(8, 9, { code: true })
    ])]);
    for (const query of ["A é😀z tail", "é😀z", "😀z t", "A ", " tail", "é", "😀", "z"]) {
      equal(reader.selectMatch(snapshot.find(query).matches[0]), query);
      equal(document.getSelection().toString(), query);
    }
    const range = document.getSelection().getRangeAt(0);
    reader.render(snapshot); equal(document.getSelection().getRangeAt(0), range);
  });
  await test("one keyboard link spans adjacent bold italic code and strike fragments", async () => {
    const calls = [], { container, snapshot } = await view([linked()], { onLink: event => calls.push(event) });
    const links = container.querySelectorAll('[role="link"]'); equal(links.length, 1);
    const anchor = links[0]; equal(anchor.tabIndex, 0); equal(anchor.textContent, "one TWO three");
    assert(anchor.querySelector("strong") && anchor.querySelector("em code") && anchor.querySelector("s"));
    anchor.focus(); equal(document.activeElement, anchor);
    click(anchor.querySelector("code")); enter(anchor);
    equal(calls.length, 2); equal(calls[0].target, "#section"); equal(calls[0].startUtf16, 0); equal(calls[0].endUtf16, 13);
    equal(calls[0].location.revision, snapshot.token.revision); equal(calls[0].location.nodeIndex, 0);
    assert(Object.isFrozen(calls[0]) && Object.isFrozen(calls[0].link));
    assert(!anchor.hasAttribute("href"));
    for (const options of [{ ctrlKey: true }, { metaKey: true }, { shiftKey: true }, { altKey: true }, { button: 1 }]) click(anchor, options);
    enter(anchor, { repeat: true }); enter(anchor, { key: " " }); equal(calls.length, 2);
  });
  await test("unsafe targets and absent callbacks remain inert without URL attributes", async () => {
    const calls = [], blocked = inline("paragraph", "bad", [run(0, 3, { bold: true }, target("javascript:bad", null))]);
    const { container } = await view([blocked], { onLink: event => calls.push(event) });
    equal(container.querySelectorAll('[role="link"]').length, 0); click(container.querySelector("strong")); equal(calls.length, 0);
    const inert = await view([linked()]); equal(inert.container.querySelectorAll('[role="link"]').length, 0);
    equal(inert.container.textContent, "one TWO three");
    assert(!document.querySelector("section [href], section [src], section [onclick], section img, section script"));
  });
  await test("linked image descriptions work with empty alt and never load an image", async () => {
    const calls = [], { container, reader, snapshot } = await view([
      node("image", "", { imageLink: target("https://example.com/image") }),
      node("image", "a diagram", { imageLink: target("#diagram") })
    ], { onLink: event => calls.push(event) });
    const links = container.querySelectorAll('figure [role="link"]'); equal(links.length, 2);
    equal(links[0].textContent, "Image: "); enter(links[0]);
    equal(calls[0].target, "https://example.com/image"); equal(calls[0].startUtf16, null); equal(calls[0].endUtf16, null);
    equal(reader.selectMatch(snapshot.find("diagram").matches[0]), "diagram"); equal(document.getSelection().toString(), "diagram");
    equal(container.querySelectorAll("img,[src],[href]").length, 0);
  });
  await test("source, layout and disposed-session fences refuse old links and matches", async () => {
    for (const action of ["edit", "reflow", "dispose"]) {
      const calls = [], { container, reader, snapshot, session } = await view([linked()], { onLink: event => calls.push(event) });
      const match = snapshot.find("TWO").matches[0];
      if (action === "dispose") session.disposed = true; else session[action]();
      click(container.querySelector('[role="link"]')); enter(container.querySelector('[role="link"]')); equal(calls.length, 0);
      throws(() => reader.selectMatch(match), action === "edit" ? "STALE_REVISION" : action === "reflow" ? "STALE_LAYOUT" : "SESSION_DISPOSED");
    }
  });
  await test("changed, inserted and reordered DOM text cannot alter search Copy", async () => {
    for (const mutate of [
      el => { el.querySelector("strong").firstChild.data = "changed"; },
      el => { el.querySelector("em").before(document.createTextNode("injected")); },
      el => { el.querySelector("strong").after(el.querySelector("s")); },
      el => { el.querySelector("em").remove(); }
    ]) {
      const calls = [], { container, reader, snapshot } = await view([linked()], { onLink: event => calls.push(event) });
      const anchor = container.querySelector('[role="link"]'); mutate(anchor);
      throws(() => reader.selectMatch(snapshot.find("one TWO three").matches[0]), "READING_DOM_CHANGED");
      click(anchor); equal(calls.length, 0);
    }
  });
  await test("forged matches and another session's matches cannot select styled text", async () => {
    const a = await view([linked()]), b = await view([linked()]);
    const match = a.snapshot.find("TWO").matches[0];
    throws(() => a.reader.selectMatch({ ...match }), "INVALID_ARGUMENT");
    throws(() => b.reader.selectMatch(match), "INVALID_ARGUMENT");
  });
  await test("replacement, clear and disposal revoke old link listeners", async () => {
    const calls = [], { container, reader } = await view([linked()], { onLink: event => calls.push(event) });
    const old = container.querySelector('[role="link"]');
    reader.render(await readFlowDocument(new ReadingSession([node("paragraph", "new")])));
    container.append(old); click(old); enter(old); equal(calls.length, 0);
    reader.render(await readFlowDocument(new ReadingSession([linked()])));
    const next = container.querySelector('[role="link"]'); reader.clear(); container.append(next); click(next); equal(calls.length, 0);
    reader.render(await readFlowDocument(new ReadingSession([linked()])));
    const last = container.querySelector('[role="link"]'); reader.dispose(); container.append(last); click(last); equal(calls.length, 0);
    throws(() => reader.render({}), "VIEW_DISPOSED");
  });
  await test("rejected snapshot keeps the previous readable DOM and selection", async () => {
    const { container, reader, snapshot } = await view([linked()]);
    reader.selectMatch(snapshot.find("TWO").matches[0]); const old = container.firstChild;
    throws(() => reader.render({ ...snapshot }), "INVALID_ARGUMENT");
    equal(container.firstChild, old); equal(document.getSelection().toString(), "TWO");
    const session = new ReadingSession([linked()]), stale = await readFlowDocument(session); session.edit();
    throws(() => reader.render(stale), "STALE_REVISION"); equal(container.firstChild, old);
  });
  await test("focus-triggered source edits cannot publish a stale native selection", async () => {
    const { container, reader, snapshot, session } = await view([linked()]);
    container.firstChild.addEventListener("focus", () => session.edit(), { once: true });
    throws(() => reader.selectMatch(snapshot.find("TWO").matches[0]), "STALE_REVISION");
    equal(document.getSelection().toString(), "");
  });
  await test("constructor validates callback options without mutating host children", async () => {
    const { container } = await view([node("paragraph", "keep")]);
    for (const options of [null, { onLink: null }, { onLink: 3 }, { unknown: true }]) {
      throws(() => new FlowReaderView(container, options), "INVALID_OPTIONS"); equal(container.textContent, "keep");
    }
  });
  return passed;
}
