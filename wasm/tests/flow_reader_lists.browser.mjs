import { FlowReaderView, readFlowDocument } from "../flow-reader.js";
const assert = (ok, label = "assertion failed") => { if (!ok) throw new Error(label); };
const equal = (a, b) => assert(a === b, `${JSON.stringify(a)} !== ${JSON.stringify(b)}`);
const rejects = async (fn, code) => { try { await fn(); } catch (e) { equal(e.code, code); return; } throw new Error(`expected ${code}`); };
const frame = (listId = "1", itemIndex = 0, extra = {}) => ({ listId, itemIndex, ordered: false, start: "1", task: null, ...extra });
const node = (role = "list-item", text = "item", listPath = [frame()], extra = {}) => ({ role, text, listPath,
  bounds: { x: 0, y: 0, width: 100, height: 20 }, enclosingSourceSpan: { startByte: 0, endByte: 100 }, children: [], ...extra });
const styled = (startByte, endByte, style = {}, link = null) => ({ startByte, endByte,
  style: { bold: false, italic: false, code: false, strikethrough: false, ...style }, link });
class Session {
  disposed = false; token = { revision: "1", layoutRevision: "1" };
  constructor(roots) { this.roots = roots; }
  readingOrder({ offset, limit }) {
    const end = Math.min(this.roots.length, offset + limit);
    return { schemaVersion: 1, ...this.token, offset, total: this.roots.length,
      nextOffset: end < this.roots.length ? end : null, nodes: this.roots.slice(offset, end) };
  }
}

export async function run() {
  const results = [], cleanup = [];
  async function mount(roots, options = {}) {
    const root = document.createElement("article"); document.body.append(root);
    const session = new Session(roots), snapshot = await readFlowDocument(session);
    const reader = new FlowReaderView(root, options); reader.render(snapshot);
    cleanup.push(() => { reader.dispose(); root.remove(); });
    return { root, session, snapshot, reader };
  }
  const test = async (name, fn) => {
    try { await fn(); results.push(name); }
    finally { while (cleanup.length) cleanup.pop()(); document.getSelection()?.removeAllRanges(); }
  };
  await test("real ol/ul/li preserve mixed nesting, starts, continuations and return-to-parent order", async () => {
    const outer = frame("1", 0, { ordered: true, start: "7" }), inner = frame("2");
    const { root } = await mount([node("list-item", "first", [outer]), node("paragraph", "continuation", [outer]),
      node("list-item", "child", [outer, inner]), node("paragraph", "back in parent", [outer]),
      node("list-item", "second", [{ ...outer, itemIndex: 1 }]), node("paragraph", "outside", [])]);
    const list = root.querySelector(":scope > ol"); equal(list.start, 7);
    equal(list.querySelectorAll(":scope > li").length, 2);
    equal(list.firstChild.value, 7); equal(list.lastChild.value, 8);
    equal(list.firstChild.querySelector(":scope > ul > li > p").textContent, "child");
    equal([...list.firstChild.children].map(e => e.tagName).join(","), "P,P,UL,P");
    equal(list.firstChild.lastChild.textContent, "back in parent");
    equal(root.lastChild.textContent, "outside");
    equal(root.querySelectorAll('[role="listitem"]').length, 0); // No duplicate item roles within li.
  });
  await test("separate lists sharing source spans never merge, including inside one parent", async () => {
    const parent = frame();
    const { root } = await mount([node(), node("list-item", "a", [parent, frame("2")]),
      node("list-item", "b", [parent, frame("3")]), node("list-item", "separate root", [frame("4")])]);
    equal(root.querySelectorAll(":scope > ul").length, 2);
    equal(root.firstChild.firstChild.querySelectorAll(":scope > ul").length, 2);
    equal(root.firstChild.firstChild.querySelectorAll(":scope > ul > li").length, 2);
  });
  await test("empty and code-first items retain native li containers", async () => {
    const { root } = await mount([node("list-item", ""), node("list-item", "", [frame("1", 1)]),
      node("code-block", "  literal\n", [frame("1", 1)])]);
    const items = root.querySelectorAll(":scope > ul > li"); equal(items.length, 2);
    equal(items[0].textContent, ""); equal(items[1].querySelector("pre code").textContent, "  literal\n");
  });
  await test("tables, headings, quotes, rules and linked images remain in their owning item", async () => {
    const path = [frame()], cell = (role, text) => node(role, text, []);
    const { root, snapshot, reader } = await mount([node(), node("heading", "Heading", path, { level: 2 }),
      node("table-header-row", "A", path, { children: [cell("table-header-cell", "A")] }),
      node("table-row", "B", path, { children: [cell("table-cell", "B")] }),
      node("blockquote", "Quote", path), node("thematic-break", "", path),
      node("image", "Picture", path, { imageLink: { target: "#picture", activeTarget: "#picture" } })]);
    const item = root.querySelector("li");
    equal(item.querySelectorAll(":scope > table").length, 1);
    equal(item.querySelector("thead th").textContent, "A"); equal(item.querySelector("tbody td").textContent, "B");
    equal(item.querySelector("h2").textContent, "Heading"); equal(item.querySelector("blockquote").textContent, "Quote");
    assert(item.querySelector("hr") && item.querySelector("figure"));
    equal(snapshot.headings[0].index, 1); equal(reader.focusNode(1).nodeIndex, 1);
    equal(reader.selectMatch(snapshot.find("B").matches[0]), "B"); equal(document.getSelection().toString(), "B");
  });
  await test("read-only task state is exposed once per item without altering source or search text", async () => {
    const { root, snapshot, reader } = await mount([
      node("list-item", "[x] done", [frame("1", 0, { task: true })]),
      node("paragraph", "continuation", [frame("1", 0, { task: true })]),
      node("list-item", "[ ] later", [frame("1", 1, { task: false })])]);
    const checkboxes = root.querySelectorAll('input[type="checkbox"]'); equal(checkboxes.length, 2);
    assert(checkboxes[0].checked && !checkboxes[1].checked);
    assert([...checkboxes].every(c => c.disabled && c.tabIndex === -1));
    const before = snapshot.text; checkboxes[1].click(); assert(!checkboxes[1].checked); equal(snapshot.text, before);
    equal(reader.selectMatch(snapshot.find("[x] done").matches[0]), "[x] done");
    equal(document.getSelection().toString(), "[x] done");
  });
  await test("Unicode cross-style selection and link activation survive nested list grouping", async () => {
    const calls = [], link = { target: "#target", activeTarget: "#target" };
    const { root, snapshot, reader } = await mount([node(), node("list-item", "é😀 end", [frame(), frame("2")], {
      inlineRuns: [styled(0, 2, { bold: true }, link), styled(2, 6, { italic: true }, link), styled(6, 10, {}, link)]
    })], { onLink: event => calls.push(event) });
    equal(root.querySelectorAll('[role="link"]').length, 1);
    const anchor = root.querySelector('[role="link"]'); anchor.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    equal(calls.length, 1); equal(calls[0].location.nodeIndex, 1); equal(calls[0].target, "#target");
    equal(reader.selectMatch(snapshot.find("é😀 end").matches[0]), "é😀 end");
    equal(document.getSelection().toString(), "é😀 end");
    const range = document.getSelection().getRangeAt(0); reader.render(snapshot); equal(document.getSelection().getRangeAt(0), range);
    assert(!root.querySelector("[href],[src]"));
  });
  await test("large ordinals are displayed exactly rather than rounded or wrapped by HTML counters", async () => {
    const { root } = await mount([node("list-item", "huge", [frame("9007199254740993", 0, {
      ordered: true, start: "18446744073709551615"
    })])]);
    assert(root.querySelector("ol > li"));
    equal(root.querySelector(".flow-reader-ordinal").textContent, "18446744073709551615. ");
    equal(root.querySelector("li").style.listStyleType, "none");
    assert(!root.querySelector("li").hasAttribute("value"));
    const zero = await mount([node("list-item", "zero", [frame("1", 0, { ordered: true, start: "0" })])]);
    equal(zero.root.querySelector("ol").start, 0); equal(zero.root.querySelector("li").value, 0);
  });
  await test("one item spanning paged reading snapshots is not split into separate lists", async () => {
    const { root } = await mount(Array.from({ length: 520 }, (_, i) => node(i ? "paragraph" : "list-item", `part ${i}`)));
    equal(root.querySelectorAll("ul").length, 1); equal(root.querySelectorAll("li").length, 1);
    equal(root.querySelectorAll("li > p").length, 520);
  });
  await test("malformed new ancestry cannot replace a successful native DOM or selection", async () => {
    const { root, reader, snapshot } = await mount([node()]); reader.selectMatch(snapshot.find("item").matches[0]);
    const before = root.firstChild;
    await rejects(async () => reader.render(await readFlowDocument(new Session([node(), node()]))), "INVALID_READING_DATA");
    equal(root.firstChild, before); equal(document.getSelection().toString(), "item");
  });
  await test("stale source and layout fences still reject nested selection and navigation", async () => {
    const { session, reader, snapshot, root } = await mount([node()]); const match = snapshot.find("item").matches[0];
    session.token = { revision: "1", layoutRevision: "2" };
    await rejects(() => reader.selectMatch(match), "STALE_LAYOUT"); await rejects(() => reader.focusNode(0), "STALE_LAYOUT");
    equal(root.textContent, "item");
    session.token = { revision: "2", layoutRevision: "2" }; await rejects(() => reader.selectMatch(match), "STALE_REVISION");
  });
  await test("host-inserted text inside a nested styled item is rejected before Copy", async () => {
    const { root, reader, snapshot } = await mount([node(), node("list-item", "bold text", [frame(), frame("2")], {
      inlineRuns: [styled(0, 4, { bold: true }), styled(4, 9)]
    })]);
    root.querySelector("strong").after(document.createTextNode("inserted"));
    await rejects(() => reader.selectMatch(snapshot.find("bold text").matches[0]), "READING_DOM_CHANGED");
  });
  await test("legacy pages keep generic list roles instead of inventing numbers or nesting", async () => {
    const a = node(), b = node(); delete a.listPath; delete b.listPath;
    const { root } = await mount([a, b]);
    equal(root.querySelectorAll('[role="list"] > [role="listitem"]').length, 2);
    equal(root.querySelectorAll("ol,ul,li").length, 0);
  });
  await test("literal hostile text in nested lists never becomes active markup", async () => {
    const payload = '<script>globalThis.listExecuted=true</script><img src="https://invalid.example/">';
    const { root } = await mount([node("list-item", payload)]);
    equal(root.querySelector("li p").textContent, payload); equal(root.querySelectorAll("script,img,[src],[href]").length, 0);
    assert(!globalThis.listExecuted);
  });
  await test("clear and disposal remove all structured containers without disposing the session", async () => {
    const { root, reader, snapshot, session } = await mount([node(), node("list-item", "nested", [frame(), frame("2")])]);
    reader.clear(); equal(root.childNodes.length, 0); reader.render(snapshot); assert(root.querySelector("ul ul"));
    reader.dispose(); reader.dispose(); equal(root.childNodes.length, 0); assert(!session.disposed);
  });
  return results;
}
