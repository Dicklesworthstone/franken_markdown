import { FlowReadingError, requireReadingDocument } from "./flow_reading.mjs";
export { FlowReadingError, FlowReadingDocument, readFlowDocument, sourceSpanToUtf16 } from "./flow_reading.mjs";
const fail = (code, message) => { throw new FlowReadingError(code, message); };
const sameSpan = (a, b) => a.enclosingSourceSpan.startByte === b.enclosingSourceSpan.startByte
  && a.enclosingSourceSpan.endByte === b.enclosingSourceSpan.endByte;
const row = node => node.role === "table-row" || node.role === "table-header-row";

/** Native selectable semantic HTML, not a recreation of measured Canvas ink.
 * No innerHTML, href, src, event attributes, ambient navigation, or image loads.
 * Optional link callbacks are host-authorized; URLs never enter DOM attributes.
 * Owns only container children; never owns the flow session or clipboard. */
export class FlowReaderView {
  #container; #document = null; #elements = new Map(); #text = new Map(); #disposed = false;
  #onLink; #listeners = [];
  constructor(container, options = {}) {
    if (!container || container.nodeType !== 1 || typeof container.replaceChildren !== "function"
        || !container.ownerDocument?.createElement) fail("INVALID_ARGUMENT", "a DOM container is required");
    if (!options || typeof options !== "object" || Array.isArray(options)
        || Object.keys(options).some(key => key !== "onLink")
        || (options.onLink !== undefined && typeof options.onLink !== "function")) {
      fail("INVALID_OPTIONS", "onLink must be a function when supplied");
    }
    this.#container = container; this.#onLink = options.onLink;
  }
  get document() { return this.#document; }
  get disposed() { return this.#disposed; }
  #alive() { if (this.#disposed) fail("VIEW_DISPOSED", "reading view is disposed"); }
  render(snapshot) {
    this.#alive(); requireReadingDocument(snapshot);
    if (snapshot === this.#document) return; // Preserve browser selection/focus on scrolling.
    const doc = this.#container.ownerDocument, fragment = doc.createDocumentFragment();
    const elements = new Map(), textNodes = new Map(), listeners = [];
    const make = tag => doc.createElement(tag);
    const linkElement = (node, link, startUtf16, endUtf16) => {
      if (!this.#onLink || !link?.activeTarget) return null;
      const el = make("a"); el.setAttribute("role", "link"); el.tabIndex = 0;
      el.className = "flow-reader-link";
      el.style.textDecoration = "underline"; el.style.cursor = "pointer";
      const activate = event => {
        if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
            || (event.type === "keydown" ? event.key !== "Enter" || event.repeat : event.button !== 0)) return;
        event.preventDefault(); // Never delegate a URL to the browser.
        if (this.#disposed || this.#document !== snapshot || !el.isConnected || !this.#container.contains(el)) return;
        let location;
        try {
          location = snapshot.locate(node.index);
          this.#checkedText(node.index);
          const label = startUtf16 === null ? `Image: ${node.text}` : node.text.slice(startUtf16, endUtf16);
          if (el.textContent !== label) return;
        } catch (error) {
          if (error instanceof FlowReadingError) return; // Stale or host-modified surface.
          throw error;
        }
        this.#onLink(Object.freeze({ target: link.activeTarget, link, location, startUtf16, endUtf16 }));
      };
      for (const type of ["click", "keydown"]) {
        el.addEventListener(type, activate); listeners.push([el, type, activate]);
      }
      return el;
    };
    const text = (node, parent) => {
      const segments = [];
      const part = (start, end, destination, style = null) => {
        if (start === end && node.text.length) return;
        let holder = destination;
        for (const [flag, tag] of [["bold", "strong"], ["italic", "em"], ["code", "code"], ["strikethrough", "s"]]) {
          if (style?.[flag]) { const el = make(tag); holder.append(el); holder = el; }
        }
        const expected = node.text.slice(start, end), leaf = doc.createTextNode(expected);
        holder.append(leaf); segments.push({ leaf, expected, start, end });
      };
      let end = 0;
      for (let i = 0; i < node.inlineRuns.length;) {
        const first = node.inlineRuns[i];
        if (first.startUtf16 > end) part(end, first.startUtf16, parent);
        // Styling inside one logical link must not create one Tab stop per font.
        let next = i + 1, linkEnd = first.endUtf16;
        if (first.link) while (next < node.inlineRuns.length) {
          const candidate = node.inlineRuns[next];
          if (candidate.startUtf16 !== linkEnd || candidate.link?.target !== first.link.target
              || candidate.link?.activeTarget !== first.link.activeTarget) break;
          linkEnd = candidate.endUtf16; next++;
        }
        const link = linkElement(node, first.link, first.startUtf16, linkEnd), holder = link ?? parent;
        if (link) parent.append(link);
        for (; i < next; i++) {
          const run = node.inlineRuns[i]; part(run.startUtf16, run.endUtf16, holder, run.style);
        }
        end = linkEnd;
      }
      if (end < node.text.length || !segments.length) part(end, node.text.length, parent);
      textNodes.set(node.index, { root: parent, segments });
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
        const caption = make("figcaption"), link = linkElement(node, node.imageLink, null, null);
        const holder = link ?? caption, label = make("span"), description = make("span");
        label.textContent = "Image: "; holder.append(label, description); text(node, description);
        if (link) caption.append(link);
        el.append(caption);
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
    this.#unlisten(); this.#listeners = listeners;
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
  #unlisten() {
    for (const [el, type, listener] of this.#listeners) el.removeEventListener(type, listener);
    this.#listeners = [];
  }
  #checkedText(index) {
    const entry = this.#text.get(index), element = this.#elements.get(index);
    if (!entry || !this.#container.contains(element) || !element.contains(entry.root)) {
      fail("READING_DOM_CHANGED", "host changed searchable reading text");
    }
    // Check exact text-node identity, contents AND order. Checking containment
    // alone would copy host-inserted text between two genuine styled segments.
    const walk = this.#container.ownerDocument.createTreeWalker(entry.root, 4); // SHOW_TEXT
    for (const segment of entry.segments) {
      if (walk.nextNode() !== segment.leaf || segment.leaf.data !== segment.expected) {
        fail("READING_DOM_CHANGED", "host changed searchable reading text");
      }
    }
    if (walk.nextNode()) fail("READING_DOM_CHANGED", "host inserted searchable reading text");
    return entry;
  }
  selectMatch(match) {
    this.#alive();
    if (!this.#document) fail("NO_READING_DOCUMENT", "no reading document is presented");
    const snapshot = this.#document, value = snapshot.matchText(match);
    const { segments } = this.#checkedText(match.nodeIndex);
    const start = segments.find(segment => match.startUtf16 >= segment.start && match.startUtf16 < segment.end);
    const end = segments.find(segment => match.endUtf16 > segment.start && match.endUtf16 <= segment.end);
    if (!start || !end) fail("READING_DOM_CHANGED", "reading selection has no text segment");
    const doc = this.#container.ownerDocument, selection = doc.getSelection();
    if (!selection) fail("SELECTION_UNAVAILABLE", "native selection is unavailable in this document");
    const range = doc.createRange();
    range.setStart(start.leaf, match.startUtf16 - start.start); range.setEnd(end.leaf, match.endUtf16 - end.start);
    this.focusNode(match.nodeIndex);
    // Focus can synchronously run host code. Do not overwrite selection after
    // that code replaced the document, edited source, or changed styled text.
    if (this.#document !== snapshot) fail("READING_DOM_CHANGED", "reading document changed during selection");
    snapshot.assertCurrent(); this.#checkedText(match.nodeIndex);
    selection.removeAllRanges(); selection.addRange(range);
    return value; // Native browser copy works; no clipboard write is implicit.
  }
  clear() {
    if (this.#disposed) return;
    this.#unlisten(); this.#document = null; this.#elements.clear(); this.#text.clear(); this.#container.replaceChildren();
  }
  dispose() { if (this.#disposed) return; this.clear(); this.#onLink = undefined; this.#disposed = true; }
}
