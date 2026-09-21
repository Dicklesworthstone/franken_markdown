import { FlowReadingError, requireReadingDocument } from "./flow_reading.mjs";

export {
  FlowReadingDocument,
  FlowReadingError,
  readFlowDocument,
  sourceSpanToUtf16,
} from "./flow_reading.mjs";

const fail = (code, message) => {
  throw new FlowReadingError(code, message);
};
const sameSpan = (a, b) =>
  a.enclosingSourceSpan.startByte === b.enclosingSourceSpan.startByte &&
  a.enclosingSourceSpan.endByte === b.enclosingSourceSpan.endByte;
const row = (node) => node.role === "table-row" || node.role === "table-header-row";

/** Native selectable semantic HTML, not a recreation of measured Canvas ink.
 * No innerHTML, href, src, event attributes, ambient navigation, or image loads.
 * Optional link callbacks are host-authorized; URLs never enter DOM attributes.
 * Owns only container children; never owns the flow session or clipboard. */
export class FlowReaderView {
  #container;
  #document = null;
  #elements = new Map();
  #text = new Map();
  #disposed = false;
  #onLink;
  #listeners = [];
  #generation = 0;
  #pending = null;
  constructor(container, options = {}) {
    if (
      !container ||
      container.nodeType !== 1 ||
      typeof container.replaceChildren !== "function" ||
      !container.ownerDocument?.createElement
    )
      fail("INVALID_ARGUMENT", "a DOM container is required");
    if (
      !options ||
      typeof options !== "object" ||
      Array.isArray(options) ||
      Object.keys(options).some((key) => key !== "onLink") ||
      (options.onLink !== undefined && typeof options.onLink !== "function")
    ) {
      fail("INVALID_OPTIONS", "onLink must be a function when supplied");
    }
    this.#container = container;
    this.#onLink = options.onLink;
  }
  get document() {
    return this.#document;
  }
  get disposed() {
    return this.#disposed;
  }
  #alive() {
    if (this.#disposed) fail("VIEW_DISPOSED", "reading view is disposed");
  }
  #invalidate() {
    this.#generation++;
    this.#pending?.abort();
  }
  render(snapshot) {
    this.#alive();
    requireReadingDocument(snapshot);
    this.#invalidate();
    if (snapshot === this.#document) return; // Preserve browser selection/focus on scrolling.
    const generation = this.#generation;
    const check = () => {
      this.#alive();
      if (generation !== this.#generation) fail("RENDER_SUPERSEDED", "reading render was replaced");
      snapshot.assertCurrent();
    };
    const build = this.#prepare(snapshot, check);
    try { while (!build.next().done) { /* Same preparation, without task yields. */ } }
    finally { build.return(); }
  }
  /** Build privately in bounded DOM-work slices; publish the complete tree once.
   * A newer render (sync or async), clear, or dispose revokes an older attempt.
   * Never sends worker requests, reparses source, or publishes partial structure. */
  async renderAsync(snapshot, options = {}) {
    this.#alive();
    requireReadingDocument(snapshot);
    if (!options || typeof options !== "object" || Array.isArray(options)
        || Object.keys(options).some(key => key !== "signal")) {
      fail("INVALID_OPTIONS", "only signal is accepted for reading rendering");
    }
    const signal = options.signal;
    if (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
        || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")) {
      fail("INVALID_OPTIONS", "signal must be an AbortSignal");
    }
    if (signal?.aborted) fail("ABORTED", "reading render was aborted");
    this.#invalidate();
    if (snapshot === this.#document) return;
    const generation = this.#generation, controller = new AbortController();
    this.#pending = controller;
    const check = () => {
      this.#alive();
      if (generation !== this.#generation) fail("RENDER_SUPERSEDED", "reading render was replaced");
      if (signal?.aborted) fail("ABORTED", "reading render was aborted");
      snapshot.assertCurrent();
    };
    const build = this.#prepare(snapshot, check);
    let work = 0;
    try {
      check();
      for (let step = build.next(); !step.done; step = build.next()) {
        work += step.value;
        if (work >= 256) {
          work = 0;
          await renderTurn(check, [controller.signal, signal]);
        }
      }
    } finally {
      build.return(); // Removes listeners from every unpublished staged link.
      if (this.#pending === controller) this.#pending = null;
    }
  }
  *#prepare(snapshot, check) {
    const doc = this.#container.ownerDocument,
      fragment = doc.createDocumentFragment();
    const elements = new Map(),
      textNodes = new Map(),
      listeners = [];
    const make = (tag) => doc.createElement(tag);
    const linkElement = (node, link, startUtf16, endUtf16) => {
      if (!this.#onLink || !link?.activeTarget) return null;
      const el = make("a");
      el.setAttribute("role", "link");
      el.tabIndex = 0;
      el.className = "flow-reader-link";
      el.style.textDecoration = "underline";
      el.style.cursor = "pointer";
      const activate = (event) => {
        if (
          event.defaultPrevented ||
          event.altKey ||
          event.ctrlKey ||
          event.metaKey ||
          event.shiftKey ||
          (event.type === "keydown" ? event.key !== "Enter" || event.repeat : event.button !== 0)
        )
          return;
        event.preventDefault(); // Never delegate a URL to the browser.
        if (
          this.#disposed ||
          this.#document !== snapshot ||
          !el.isConnected ||
          !this.#container.contains(el)
        )
          return;
        let location;
        try {
          location = snapshot.locate(node.index);
          this.#checkedText(node.index);
          const label =
            startUtf16 === null ? `Image: ${node.text}` : node.text.slice(startUtf16, endUtf16);
          if (el.textContent !== label) return;
        } catch (error) {
          if (error instanceof FlowReadingError) return; // Stale or host-modified surface.
          throw error;
        }
        this.#onLink(
          Object.freeze({ target: link.activeTarget, link, location, startUtf16, endUtf16 }),
        );
      };
      for (const type of ["click", "keydown"]) {
        el.addEventListener(type, activate);
        listeners.push([el, type, activate]);
      }
      return el;
    };
    const text = function* (node, parent) {
      const segments = [];
      const part = function* (start, end, destination, style = null) {
        if (start === end && node.text.length) return;
        let holder = destination;
        for (const [flag, tag] of [
          ["bold", "strong"],
          ["italic", "em"],
          ["code", "code"],
          ["strikethrough", "s"],
        ]) {
          if (style?.[flag]) {
            const el = make(tag);
            holder.append(el);
            holder = el;
          }
        }
        const expected = node.text.slice(start, end),
          leaf = doc.createTextNode(expected);
        holder.append(leaf);
        segments.push({ leaf, expected, start, end });
        yield 1 + Math.ceil(expected.length / 4096);
      };
      let end = 0;
      for (let i = 0; i < node.inlineRuns.length; ) {
        const first = node.inlineRuns[i];
        if (first.startUtf16 > end) yield* part(end, first.startUtf16, parent);
        // Styling inside one logical link must not create one Tab stop per font.
        let next = i + 1,
          linkEnd = first.endUtf16;
        if (first.link)
          while (next < node.inlineRuns.length) {
            const candidate = node.inlineRuns[next];
            if (
              candidate.startUtf16 !== linkEnd ||
              candidate.link?.target !== first.link.target ||
              candidate.link?.activeTarget !== first.link.activeTarget
            )
              break;
            linkEnd = candidate.endUtf16;
            next++;
            yield 1; // One long logical link can span every admitted style run.
          }
        const link = linkElement(node, first.link, first.startUtf16, linkEnd),
          holder = link ?? parent;
        if (link) parent.append(link);
        for (; i < next; i++) {
          const run = node.inlineRuns[i];
          yield* part(run.startUtf16, run.endUtf16, holder, run.style);
        }
        end = linkEnd;
      }
      if (end < node.text.length || !segments.length) yield* part(end, node.text.length, parent);
      textNodes.set(node.index, { root: parent, segments });
    };
    const element = function* (node) {
      yield 1;
      let el;
      switch (node.role) {
        case "heading":
          el = make(`h${node.level}`);
          break;
        case "paragraph":
          el = make("p");
          break;
        case "code-block":
          el = make("pre");
          break;
        case "blockquote":
          el = make("blockquote");
          break;
        case "thematic-break":
          el = make("hr");
          break;
        case "list":
          el = make("div");
          el.setAttribute("role", "list");
          break;
        case "list-item":
          if (node.listPath?.length)
            el = make("p"); // The owning li supplies list-item semantics.
          else {
            el = make("div");
            el.setAttribute("role", "listitem");
          }
          break;
        case "table":
          el = make("table");
          break;
        case "table-header-row":
        case "table-row":
          el = make("tr");
          break;
        case "table-header-cell":
          el = make("th");
          el.setAttribute("scope", "col");
          break;
        case "table-cell":
          el = make("td");
          break;
        case "image":
          el = make("figure");
          break;
        default:
          el = make("div");
      }
      // Programmatic focus only; do not put every paragraph in the Tab order.
      el.tabIndex = -1;
      el.setAttribute("data-flow-node", String(node.index));
      elements.set(node.index, el);
      if (node.children.length) yield* append(node.children, el, node.role);
      else if (node.role === "code-block") {
        const code = make("code");
        yield* text(node, code);
        el.append(code);
      } else if (node.role === "image") {
        const caption = make("figcaption"),
          link = linkElement(node, node.imageLink, null, null);
        const holder = link ?? caption,
          label = make("span"),
          description = make("span");
        label.textContent = "Image: ";
        holder.append(label, description);
        yield* text(node, description);
        if (link) caption.append(link);
        el.append(caption);
      } else if (node.role !== "thematic-break") yield* text(node, el);
      return el;
    };
    // Current flow emits standalone rows/items with truthful enclosing block
    // spans. Group consecutive siblings only; never infer order or nesting from
    // pixel indentation, guessed Markdown, or source text.
    const append = function* (nodes, parent, role = null) {
      for (let i = 0; i < nodes.length; ) {
        const node = nodes[i];
        if (row(node) && role !== "table") {
          const table = make("table"),
            group = [];
          do {
            group.push(nodes[i++]);
            yield 1;
          } while (i < nodes.length && row(nodes[i]) && sameSpan(node, nodes[i]));
          yield* append(group, table, "table");
          parent.append(table);
        } else if (node.role === "list-item" && !node.listPath?.length && role !== "list") {
          const list = make("div");
          list.setAttribute("role", "list");
          do {
            list.append(yield* element(nodes[i++]));
          } while (i < nodes.length && nodes[i].role === "list-item" && sameSpan(node, nodes[i]));
          parent.append(list);
        } else if (row(node)) {
          // Rows remain in supplied reading order; no header/body reordering.
          const section = make(node.role === "table-header-row" ? "thead" : "tbody"),
            kind = node.role;
          do {
            section.append(yield* element(nodes[i++]));
          } while (i < nodes.length && nodes[i].role === kind);
          parent.append(section);
        } else {
          parent.append(yield* element(node));
          i++;
        }
      }
    };
    // Group by admitted AST identities. Continuations and table/image blocks
    // remain inside their exact item; same source spans never merge two lists.
    const appendLists = function* (nodes, parent) {
      const stack = [];
      let pending = [],
        destination = parent;
      const flush = function* () {
        yield* append(pending, destination, "owned-list-item");
        pending = [];
      };
      for (const node of nodes) {
        yield 1;
        const path = node.listPath;
        let common = 0;
        while (
          common < path.length &&
          common < stack.length &&
          path[common].listId === stack[common].frame.listId &&
          path[common].itemIndex === stack[common].frame.itemIndex
        )
          common++;
        if (common !== path.length || common !== stack.length) {
          yield* flush();
          for (let depth = common; depth < path.length; depth++) {
            yield 1;
            const frame = path[depth],
              before = stack[depth];
            let list = before?.frame.listId === frame.listId ? before.list : null;
            if (!list) {
              list = make(frame.ordered ? "ol" : "ul");
              // HTML counters are signed 32-bit, unlike the lossless wire u64.
              if (frame.ordered && BigInt(frame.start) <= 2147483647n)
                list.setAttribute("start", frame.start);
              (depth ? stack[depth - 1].item : parent).append(list);
            }
            const item = make("li");
            if (frame.ordered) {
              const ordinal = BigInt(frame.start) + BigInt(frame.itemIndex);
              if (ordinal <= 2147483647n) item.setAttribute("value", String(ordinal));
              else {
                // Never silently round/wrap a large ordinal through Number or
                // HTML's counter reflection. Keep ordered semantics and show it.
                item.style.listStyleType = "none";
                const label = make("span");
                label.className = "flow-reader-ordinal";
                label.textContent = `${ordinal}. `;
                item.append(label);
              }
            }
            if (frame.task !== null) {
              const checkbox = make("input");
              checkbox.type = "checkbox";
              checkbox.checked = frame.task;
              checkbox.disabled = true;
              checkbox.tabIndex = -1;
              checkbox.setAttribute(
                "aria-label",
                frame.task ? "Completed task" : "Incomplete task",
              );
              item.append(checkbox); // Read-only, no events or source edits.
            }
            list.append(item);
            stack[depth] = { frame, list, item };
          }
          stack.length = path.length;
          destination = stack.length ? stack[stack.length - 1].item : parent;
        }
        pending.push(node);
      }
      yield* flush();
    };
    let published = false;
    try {
      if (snapshot.roots[0]?.listPath != null) yield* appendLists(snapshot.roots, fragment);
      else yield* append(snapshot.roots, fragment);
      check(); // Never publish after an input event revoked the staged tree.
      this.#container.replaceChildren(fragment);
      check(); // Reentrant host DOM hooks must not overwrite a newer attempt.
      this.#unlisten();
      this.#listeners = listeners;
      this.#document = snapshot;
      this.#elements = elements;
      this.#text = textNodes;
      published = true;
    } finally {
      if (!published) {
        for (const [el, type, listener] of listeners) el.removeEventListener(type, listener);
      }
    }
  }
  focusNode(index) {
    this.#alive();
    if (!this.#document) fail("NO_READING_DOCUMENT", "no reading document is presented");
    const location = this.#document.locate(index);
    const element = this.#elements.get(index);
    if (!this.#container.contains(element))
      fail("READING_DOM_CHANGED", "host changed the reading surface");
    element.focus({ preventScroll: true });
    return location;
  }
  #unlisten() {
    for (const [el, type, listener] of this.#listeners) el.removeEventListener(type, listener);
    this.#listeners = [];
  }
  #checkedText(index) {
    const entry = this.#text.get(index),
      element = this.#elements.get(index);
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
    const snapshot = this.#document,
      value = snapshot.matchText(match);
    const { segments } = this.#checkedText(match.nodeIndex);
    const start = segments.find(
      (segment) => match.startUtf16 >= segment.start && match.startUtf16 < segment.end,
    );
    const end = segments.find(
      (segment) => match.endUtf16 > segment.start && match.endUtf16 <= segment.end,
    );
    if (!start || !end) fail("READING_DOM_CHANGED", "reading selection has no text segment");
    const doc = this.#container.ownerDocument,
      selection = doc.getSelection();
    if (!selection)
      fail("SELECTION_UNAVAILABLE", "native selection is unavailable in this document");
    const range = doc.createRange();
    range.setStart(start.leaf, match.startUtf16 - start.start);
    range.setEnd(end.leaf, match.endUtf16 - end.start);
    this.focusNode(match.nodeIndex);
    // Focus can synchronously run host code. Do not overwrite selection after
    // that code replaced the document, edited source, or changed styled text.
    if (this.#document !== snapshot)
      fail("READING_DOM_CHANGED", "reading document changed during selection");
    snapshot.assertCurrent();
    this.#checkedText(match.nodeIndex);
    selection.removeAllRanges();
    selection.addRange(range);
    return value; // Native browser copy works; no clipboard write is implicit.
  }
  clear() {
    if (this.#disposed) return;
    this.#invalidate();
    this.#clearDOM();
  }
  #clearDOM() {
    this.#unlisten();
    this.#document = null;
    this.#elements.clear();
    this.#text.clear();
    this.#container.replaceChildren();
  }
  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#invalidate();
    this.#clearDOM();
    this.#onLink = undefined;
  }
}

// Event-loop tasks, not only resolved-Promise microtasks. There is at most one
// timer per suspended attempt; every settlement releases it and its listeners.
function renderTurn(check, signals) {
  check();
  return new Promise((resolve, reject) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      for (const signal of signals) signal?.removeEventListener("abort", finish);
      try { check(); resolve(); } catch (error) { reject(error); }
    };
    const timer = setTimeout(finish, 0);
    for (const signal of signals) signal?.addEventListener("abort", finish, { once: true });
    if (signals.some(signal => signal?.aborted)) finish();
  });
}
