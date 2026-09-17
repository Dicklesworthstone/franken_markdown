// Host UI over engine-owned semantic data. No Markdown parser or implicit URL
// navigation. getLocation must fence both the shown snapshot and current source.
import { FlowReaderView } from "../flow-reader.js";

export function createReadingControls({ root, panel, query, insensitive, previous, next, outline,
  sourceButton, status, sourceEditor, getLocation, onNavigate = () => {} }) {
  const view = new FlowReaderView(root), listeners = [];
  let snapshot = null, result = { matches: [], truncated: false }, cursor = -1, active = null;
  let enabled = false, disposed = false;
  const listen = (element, type, callback) => {
    element.addEventListener(type, callback); listeners.push(() => element.removeEventListener(type, callback));
  };
  const buttons = () => {
    previous.disabled = next.disabled = !enabled || result.matches.length === 0;
    outline.disabled = !enabled || !snapshot?.headings.length;
    sourceButton.disabled = !enabled || active === null;
  };
  const summary = () => {
    status.textContent = query.value ? `${cursor < 0 ? 0 : cursor + 1} of ${result.matches.length}${result.truncated ? "+" : ""} matches. Enter/Shift+Enter moves between matches.`
      : "Choose a heading or search logical text. Native selection and Copy work in the reading surface.";
  };
  const error = failure => {
    status.textContent = `${failure?.code ?? "READING_ERROR"}: navigation unavailable for this preview. Source is unchanged.`;
  };
  const search = () => {
    result = { matches: [], truncated: false }; cursor = -1; active = null;
    if (enabled && snapshot) {
      try { result = snapshot.find(query.value, { asciiCaseInsensitive: insensitive.checked }); summary(); }
      catch (failure) { error(failure); }
    }
    buttons();
  };
  const reveal = index => {
    if (panel) panel.open = true;
    const element = root.querySelector(`[data-flow-node="${index}"]`);
    if (element) root.scrollTop += element.getBoundingClientRect().top - root.getBoundingClientRect().top - 16;
  };
  function move(step) {
    if (!enabled || !result.matches.length) return;
    const at = cursor < 0 ? (step > 0 ? 0 : result.matches.length - 1)
      : (cursor + step + result.matches.length) % result.matches.length;
    const match = result.matches[at];
    try {
      // Validate before changing native selection or any viewport/source state.
      const location = getLocation(match.nodeIndex, snapshot);
      if (panel) panel.open = true;
      view.selectMatch(match); cursor = at; active = match.nodeIndex;
      reveal(active); onNavigate(location); summary();
    } catch (failure) { error(failure); }
    buttons();
  }
  listen(query, "input", search); listen(insensitive, "change", search);
  listen(query, "keydown", event => {
    if (event.key === "Enter" && !event.isComposing) { event.preventDefault(); move(event.shiftKey ? -1 : 1); }
  });
  listen(previous, "click", () => move(-1)); listen(next, "click", () => move(1));
  listen(outline, "change", () => {
    if (!enabled || !/^(0|[1-9][0-9]*)$/.test(outline.value)) return;
    const index = Number(outline.value);
    try {
      const location = getLocation(index, snapshot);
      if (panel) panel.open = true;
      view.focusNode(index); active = index; reveal(index); onNavigate(location);
      status.textContent = `Heading: ${snapshot.nodes[index].text}. Source navigation selects its enclosing Markdown block.`;
    } catch (failure) { error(failure); }
    buttons();
  });
  listen(sourceButton, "click", () => {
    if (!enabled || active === null) return;
    try {
      const { sourceRange } = getLocation(active, snapshot);
      sourceEditor.focus(); sourceEditor.setSelectionRange(sourceRange.start, sourceRange.end);
      status.textContent = "Selected the enclosing original Markdown block, not an inferred inline range.";
    } catch (failure) { error(failure); }
  });
  const clear = () => {
    view.clear(); snapshot = null; result = { matches: [], truncated: false }; cursor = -1; active = null;
    outline.replaceChildren();
  };
  buttons();
  return Object.freeze({
    update(state) {
      if (disposed) return;
      enabled = false;
      if (state.status === "idle" || state.status === "disposed") {
        clear(); status.textContent = "Reading navigation is not active.";
      } else if (state.status === "ready") {
        if (!state.document || state.readingError) {
          clear(); status.textContent = state.readingError ? `${state.readingError.code}: semantic reader unavailable; Canvas and source remain usable.`
            : "No semantic reading snapshot is available.";
        } else {
          try {
            state.document.assertCurrent();
            if (state.document !== snapshot) {
              view.render(state.document); snapshot = state.document;
              const fragment = root.ownerDocument.createDocumentFragment(), option = root.ownerDocument.createElement("option");
              option.value = ""; option.textContent = "Choose a heading"; fragment.append(option);
              for (const heading of snapshot.headings) {
                const item = root.ownerDocument.createElement("option"); item.value = String(heading.index);
                item.textContent = `${"  ".repeat(heading.level - 1)}H${heading.level}: ${heading.text}`; fragment.append(item);
              }
              outline.replaceChildren(fragment); enabled = true; search();
            } else { enabled = true; summary(); }
          } catch (failure) { error(failure); }
        }
      } else status.textContent = "Preview changing or unavailable; navigation is paused. Previously shown text remains selectable.";
      buttons();
    },
    dispose() {
      if (disposed) return;
      disposed = true; enabled = false;
      for (const remove of listeners) remove();
      clear(); view.dispose(); buttons();
    }
  });
}
