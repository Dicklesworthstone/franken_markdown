import { FlowReadingError, requireReadingDocument } from "./flow_reading.mjs";
export { FlowReadingError, FlowReadingDocument, readFlowDocument, sourceSpanToUtf16 } from "./flow_reading.mjs";
const fail = (code, message) => { throw new FlowReadingError(code, message); };
const sameSpan = (a, b) => a.enclosingSourceSpan.startByte === b.enclosingSourceSpan.startByte
  && a.enclosingSourceSpan.endByte === b.enclosingSourceSpan.endByte;
const row = node => node.role === "table-row" || node.role === "table-header-row";

/** Native selectable semantic HTML, not a recreation of measured Canvas ink.
 * No innerHTML, href, src, event attributes, ambient navigation, or image loads.
 * Owns only container children; never owns the flow session or clipboard. */
export class FlowReaderView {
  #container; #document = null; #elements = new Map(); #text = new Map(); #disposed = false;
  constructor(container) {
    if (!container || container.nodeType !== 1 || typeof container.replaceChildren !== "function"
        || !container.ownerDocument?.createElement) fail("INVALID_ARGUMENT", "a DOM container is required");
    this.#container = container;
  }
  get document() { return this.#document; }
  get disposed() { return this.#disposed; }
  #alive() { if (this.#disposed) fail("VIEW_DISPOSED", "reading view is disposed"); }
  render(snapshot) {
    this.#alive(); requireReadingDocument(snapshot);
    if (snapshot === this.#document) return; // Preserve browser selection/focus on scrolling.
    const doc = this.#container.ownerDocument, fragment = doc.createDocumentFragment();
    const elements = new Map(), textNodes = new Map();
    const make = tag => doc.createElement(tag);
    const text = (node, parent) => {
      const leaf = doc.createTextNode(node.text); parent.append(leaf); textNodes.set(node.index, leaf);
    };
    const element = node => {
      let el;
      switch (node.role) {
        case "heading": el = make(`h${node.level}`); break;
        case "paragraph": el = make("p"); break;
        case "code-block": el = make("pre"); break;
        case "blockquote": el = make("blockquote"); break;
        case "thematic-break": el = make("hr"); break;
        case "list": el = make("div"); el.setAttribute("role", "list"); break;
        case "list-item": el = make("div"); el.setAttribute("role", "listitem"); break;
        case "table": el = make("table"); break;
        case "table-header-row": case "table-row": el = make("tr"); break;
        case "table-header-cell": el = make("th"); el.setAttribute("scope", "col"); break;
        case "table-cell": el = make("td"); break;
        case "image": el = make("figure"); break;
        default: el = make("div");
      }
      // Programmatic focus only; do not put every paragraph in the Tab order.
      el.tabIndex = -1; el.setAttribute("data-flow-node", String(node.index)); elements.set(node.index, el);
      if (node.children.length) append(node.children, el, node.role);
      else if (node.role === "code-block") { const code = make("code"); text(node, code); el.append(code); }
      else if (node.role === "image") {
        const caption = make("figcaption");
        const label = make("span"); label.textContent = "Image: "; caption.append(label); text(node, caption); el.append(caption);
      } else if (node.role !== "thematic-break") text(node, el);
      return el;
    };
    // Current flow emits standalone rows/items with truthful enclosing block
    // spans. Group consecutive siblings only; never infer order or nesting from
    // pixel indentation, guessed Markdown, or source text.
    const append = (nodes, parent, role = null) => {
      for (let i = 0; i < nodes.length;) {
        const node = nodes[i];
        if (row(node) && role !== "table") {
          const table = make("table"), group = [];
          do { group.push(nodes[i++]); } while (i < nodes.length && row(nodes[i]) && sameSpan(node, nodes[i]));
          append(group, table, "table"); parent.append(table);
        } else if (node.role === "list-item" && role !== "list") {
          const list = make("div"); list.setAttribute("role", "list");
          do { list.append(element(nodes[i++])); }
          while (i < nodes.length && nodes[i].role === "list-item" && sameSpan(node, nodes[i]));
          parent.append(list);
        } else if (row(node)) {
          // Rows remain in supplied reading order; no header/body reordering.
          const section = make(node.role === "table-header-row" ? "thead" : "tbody"), kind = node.role;
          do { section.append(element(nodes[i++])); } while (i < nodes.length && nodes[i].role === kind);
          parent.append(section);
        } else { parent.append(element(node)); i++; }
      }
    };
    append(snapshot.roots, fragment);
    snapshot.assertCurrent(); // A failed preparation leaves old DOM intact.
    this.#container.replaceChildren(fragment);
    this.#document = snapshot; this.#elements = elements; this.#text = textNodes;
  }
  focusNode(index) {
    this.#alive();
    if (!this.#document) fail("NO_READING_DOCUMENT", "no reading document is presented");
    const location = this.#document.locate(index);
    const element = this.#elements.get(index);
    if (!this.#container.contains(element)) fail("READING_DOM_CHANGED", "host changed the reading surface");
    element.focus({ preventScroll: true }); return location;
  }
  selectMatch(match) {
    this.#alive();
    if (!this.#document) fail("NO_READING_DOCUMENT", "no reading document is presented");
    const value = this.#document.matchText(match), text = this.#text.get(match.nodeIndex);
    if (!text || !this.#container.contains(text) || text.data !== this.#document.nodes[match.nodeIndex].text) {
      fail("READING_DOM_CHANGED", "host changed searchable reading text");
    }
    const doc = this.#container.ownerDocument, selection = doc.getSelection();
    if (!selection) fail("SELECTION_UNAVAILABLE", "native selection is unavailable in this document");
    const range = doc.createRange(); range.setStart(text, match.startUtf16); range.setEnd(text, match.endUtf16);
    this.focusNode(match.nodeIndex); selection.removeAllRanges(); selection.addRange(range);
    return value; // Native browser copy works; no clipboard write is implicit.
  }
  clear() {
    if (this.#disposed) return;
    this.#document = null; this.#elements.clear(); this.#text.clear(); this.#container.replaceChildren();
  }
  dispose() { if (this.#disposed) return; this.clear(); this.#disposed = true; }
}
