import { FlowReaderView, readFlowDocument } from "../flow-reader.js";
import { node, ReadingSession } from "./flow_reading_fixtures.mjs";
const assert = (value, label) => { if (!value) throw new Error(label); };
const throws = (fn, code) => { try { fn(); } catch (error) { assert(error.code === code, `expected ${code}, got ${error.code}`); return; } throw new Error(`expected ${code}`); };
export async function run() {
  const results = [], root = document.createElement("article"); document.body.append(root);
  const test = async (name, fn) => { const view = new FlowReaderView(root); try { await fn(view); results.push(name); } finally { view.dispose(); } };
  try {
    await test("real semantic heading/table/list/code DOM uses only engine reading text", async view => {
      const s = new ReadingSession([node("heading", "Guide", { level: 2 }), node("table-header-row", "A | B", {
        children: [node("table-header-cell", "A"), node("table-header-cell", "B")]
      }), node("table-row", "one | two", { children: [node("table-cell", "one"), node("table-cell", "two")] }),
      node("list-item", "first"), node("list-item", "second"), node("code-block", "  a\n    b"), node("image", "Diagram description")]);
      view.render(await readFlowDocument(s));
      assert(root.querySelector("h2")?.textContent === "Guide", "heading level");
      assert(root.querySelectorAll("table").length === 1 && root.querySelectorAll("thead th[scope=col]").length === 2, "table header association");
      assert(root.querySelectorAll("tbody td").length === 2 && root.querySelectorAll('[role=list] > [role=listitem]').length === 2, "rows and list semantics");
      assert(root.querySelector("pre code")?.textContent === "  a\n    b", "literal code whitespace");
      assert(!root.textContent.includes("A | B") && !root.textContent.includes("one | two"), "no duplicated row summary");
      assert(root.querySelector("figcaption")?.textContent === "Image: Diagram description", "image transcript without fetch");
    });
    await test("untrusted HTML/URL-looking text is inert; no generated links, images or scripts", async view => {
      globalThis.readerExecuted = false;
      const payload = '<img src="https://invalid.example/private" onerror="readerExecuted=true"><script>readerExecuted=true</script>';
      view.render(await readFlowDocument(new ReadingSession([node("paragraph", payload), node("image", payload)])));
      assert(!root.querySelector("img,script,a,iframe,style"), "no active markup");
      assert(root.querySelector("p").textContent === payload && !globalThis.readerExecuted, "literal payload");
    });
    await test("native DOM Range selects exact Unicode search text for ordinary browser copying", async view => {
      const snapshot = await readFlowDocument(new ReadingSession([node("paragraph", "a😀bc and second")]));
      view.render(snapshot); const match = snapshot.find("😀b").matches[0];
      assert(view.selectMatch(match) === "😀b" && document.getSelection().toString() === "😀b", "exact native selection");
      assert(root.contains(document.activeElement), "focus stays in reader");
      view.render(snapshot); assert(document.getSelection().toString() === "😀b", "same snapshot preserves native selection");
    });
    await test("foreign-session/forged matches cannot select text with coincident revision tokens", async view => {
      const snapshot = await readFlowDocument(new ReadingSession()), other = await readFlowDocument(new ReadingSession());
      view.render(snapshot);
      throws(() => view.selectMatch(other.find("text").matches[0]), "INVALID_ARGUMENT");
      throws(() => view.selectMatch({ ...snapshot.find("text").matches[0] }), "INVALID_ARGUMENT");
    });
    await test("stale geometry is unusable while the last successful readable DOM remains intact", async view => {
      const s = new ReadingSession(), snapshot = await readFlowDocument(s); view.render(snapshot);
      const before = root.firstChild; s.reflow();
      throws(() => view.focusNode(0), "STALE_LAYOUT"); throws(() => view.render(snapshot), "STALE_LAYOUT");
      assert(root.firstChild === before && root.textContent === "text", "prior readable content retained");
      view.render(await readFlowDocument(s)); assert(view.focusNode(0).layoutRevision === "2", "new layout re-enables navigation");
    });
    await test("nested engine structures and separate table source spans keep distinct DOM ownership", async view => {
      const cell = () => node("table-cell", "cell");
      const s = new ReadingSession([node("blockquote", "summary", { children: [node("paragraph", "quoted")] }),
        node("table-row", "cell", { children: [cell()] }),
        node("table-row", "cell", { enclosingSourceSpan: { startByte: 20, endByte: 40 }, children: [cell()] })]);
      view.render(await readFlowDocument(s));
      assert(root.querySelector("blockquote > p")?.textContent === "quoted", "nested paragraph");
      assert(!root.textContent.includes("summary") && root.querySelectorAll("table").length === 2, "separate structures");
    });
    await test("host DOM mutation is detected before selection, and disposal leaves session ownership alone", async view => {
      const s = new ReadingSession(), snapshot = await readFlowDocument(s); view.render(snapshot);
      const match = snapshot.find("text").matches[0]; root.querySelector("p").firstChild.data = "changed";
      throws(() => view.selectMatch(match), "READING_DOM_CHANGED");
      view.dispose(); view.dispose(); assert(!s.disposed && root.childNodes.length === 0, "view-only disposal");
      throws(() => view.render(snapshot), "VIEW_DISPOSED");
    });
    await test("failed admission cannot replace a successful native reading surface", async view => {
      view.render(await readFlowDocument(new ReadingSession([node("heading", "Retained", { level: 1 })])));
      const before = root.firstChild;
      try { view.render(await readFlowDocument(new ReadingSession([node("script", "bad")]))); throw new Error("admitted bad role"); }
      catch (error) { assert(error.code === "INVALID_READING_DATA", "expected admission error"); }
      assert(root.firstChild === before, "unchanged DOM");
    });
    return results;
  } finally { root.remove(); }
}
