// Host UI over engine-owned semantic data. No Markdown parser or implicit URL
// navigation. getLocation must fence both the shown snapshot and current source.
import { FlowReaderView, FlowReadingError } from "../flow-reader.js";

export function createReadingControls({
  root,
  panel,
  query,
  insensitive,
  wholeWord,
  previous,
  next,
  outline,
  sourceButton,
  status,
  sourceEditor,
  getLocation,
  onNavigate = () => {},
}) {
  const view = new FlowReaderView(root, { onLink: activateLink }), listeners = [];
  const empty = () => ({ matches: [], truncated: false });
  let snapshot = null, snapshotSource = null, result = empty(), cursor = -1, active = null;
  let enabled = false, disposed = false, composing = false;
  let pending = null, searched = null, settled = Promise.resolve();
  const listen = (element, type, callback) => {
    const guarded = event => { if (!disposed) callback(event); };
    element.addEventListener(type, guarded);
    listeners.push(() => element.removeEventListener(type, guarded));
  };
  const buttons = () => {
    previous.disabled = next.disabled = !enabled || pending !== null || !result.matches.length;
    outline.disabled = !enabled || !snapshot?.headings.length;
    sourceButton.disabled = !enabled || active === null;
  };
  const summary = () => {
    status.textContent = pending
      ? "Searching logical text… Enter/Shift+Enter selects a match when this search completes."
      : query.value
        ? `${cursor < 0 ? 0 : cursor + 1} of ${result.matches.length}${result.truncated ? "+" : ""} matches. Enter/Shift+Enter moves between matches.`
        : "Choose a heading or search logical text. Native selection and Copy work in the reading surface.";
  };
  const error = failure => {
    status.textContent = `${failure?.code ?? "READING_ERROR"}: navigation unavailable for this preview. Source is unchanged.`;
  };
  const key = () => ({ snapshot, source: sourceEditor.value, query: query.value,
    insensitive: insensitive.checked, wholeWord: wholeWord?.checked ?? false });
  const sameKey = (a, b) => a && b && Object.keys(a).every(name => a[name] === b[name]);
  const cancel = () => {
    const old = pending;
    pending = null;
    if (old) { searched = null; old.controller.abort(); }
  };
  const reset = () => { cancel(); searched = null; result = empty(); cursor = -1; active = null; };
  const sourceChanged = () => {
    enabled = false;
    reset();
    status.textContent = "STALE_REVISION: Source changed; search and navigation are paused until a matching preview is ready.";
    buttons();
  };
  const locate = (index, document = snapshot) => {
    if (disposed || !enabled || document !== snapshot || sourceEditor.value !== snapshotSource)
      throw new FlowReadingError("STALE_REVISION", "reading controls no longer match the source");
    return getLocation(index, document);
  };
  const search = () => {
    if (disposed) return;
    if (composing) {
      reset();
      status.textContent = "Finish composing the search text to find matches.";
      buttons();
      return;
    }
    const wanted = key();
    if (enabled && sameKey(wanted, searched)) return; // Scroll/IME tail events do not restart work.
    reset();
    if (!enabled || !snapshot) { buttons(); return; }
    if (wanted.source !== snapshotSource) { sourceChanged(); return; }
    searched = wanted;
    if (!wanted.query) { summary(); buttons(); return; }
    const run = { key: wanted, controller: new AbortController(), move: 0 };
    pending = run;
    summary();
    buttons();
    // Defer ingress one microtask: many same-turn input events start only the
    // newest search. The engine then cooperates within large semantic leaves.
    const work = Promise.resolve().then(() => {
      if (pending !== run) return null;
      if (typeof wanted.snapshot.findAsync !== "function")
        throw new FlowReadingError("UNSUPPORTED_READER", "use a matching reader with asynchronous search");
      return wanted.snapshot.findAsync(wanted.query, {
        caseInsensitive: wanted.insensitive, wholeWord: wanted.wholeWord,
        signal: run.controller.signal,
      });
    }).then(found => {
      if (pending !== run || disposed) return;
      pending = null;
      if (sourceEditor.value !== wanted.source) { sourceChanged(); return; }
      if (!enabled || composing || !sameKey(wanted, key())) {
        searched = null;
        result = empty();
        status.textContent = "Search changed; enter the current query to search again.";
        buttons();
        return;
      }
      wanted.snapshot.assertCurrent();
      result = found;
      summary();
      buttons();
      if (run.move) move(run.move);
    }).catch(failure => {
      // An old promise never clears a newer search, changes its controls or
      // reports a late failure over a replacement/disposed view.
      if (disposed || searched !== wanted || (pending !== null && pending !== run)) return;
      pending = null;
      result = empty();
      searched = null;
      error(failure);
      buttons();
    });
    // Observe every rejection, including stale searches. This waits for local
    // search completion, not for image loads, painting or any worker operation.
    settled = work;
  };
  const reveal = index => {
    if (panel) panel.open = true;
    const element = root.querySelector(`[data-flow-node="${index}"]`);
    if (element)
      root.scrollTop += element.getBoundingClientRect().top - root.getBoundingClientRect().top - 16;
  };
  function activateLink(activation) {
    if (!enabled || !snapshot) return;
    const document = snapshot;
    try {
      locate(activation.location.nodeIndex, document);
      const destination = document.locateFragment(activation.target);
      if (!destination) {
        status.textContent = "No matching internal destination. External links are not opened by this reader.";
        return;
      }
      const index = destination.nodeIndex;
      locate(index, document);
      if (pending) pending.move = 0;
      view.focusNode(index);
      const location = locate(index, document); // Focus can synchronously run host code.
      active = index;
      reveal(index);
      onNavigate(location);
      if (!disposed && enabled && snapshot === document)
        status.textContent = `Linked section: ${document.nodes[index].text}. Source is unchanged.`;
    } catch (failure) { if (!disposed && snapshot === document) error(failure); }
    buttons();
  }
  function move(step) {
    if (!enabled || !snapshot || composing) return;
    if (!sameKey(searched, key())) search();
    if (pending) { pending.move = step; return; } // At most one deferred action, never an event queue.
    if (!result.matches.length || !sameKey(searched, key())) return;
    const at = cursor < 0 ? (step > 0 ? 0 : result.matches.length - 1)
      : (cursor + step + result.matches.length) % result.matches.length;
    const match = result.matches[at], document = snapshot, searchKey = searched;
    try {
      locate(match.nodeIndex, document);
      if (panel) panel.open = true;
      view.selectMatch(match);
      const location = locate(match.nodeIndex, document);
      if (searched !== searchKey) return;
      cursor = at;
      active = match.nodeIndex;
      reveal(active);
      onNavigate(location);
      if (!disposed && enabled && snapshot === document && searched === searchKey) summary();
    } catch (failure) { if (!disposed && snapshot === document) error(failure); }
    buttons();
  }
  listen(query, "input", event => {
    if (event.isComposing || composing) { reset(); buttons(); return; }
    search();
  });
  listen(query, "compositionstart", () => { composing = true; search(); });
  listen(query, "compositionend", () => { composing = false; search(); });
  listen(insensitive, "change", search);
  if (wholeWord) listen(wholeWord, "change", search);
  listen(sourceEditor, "input", sourceChanged);
  listen(query, "keydown", event => {
    if (event.key === "Enter" && !event.isComposing && !composing && !event.repeat
        && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      move(event.shiftKey ? -1 : 1);
    }
  });
  listen(previous, "click", () => move(-1));
  listen(next, "click", () => move(1));
  listen(outline, "change", () => {
    if (!enabled || !/^(0|[1-9][0-9]*)$/.test(outline.value)) return;
    const index = Number(outline.value), document = snapshot;
    try {
      locate(index, document);
      if (pending) pending.move = 0;
      if (panel) panel.open = true;
      view.focusNode(index);
      const location = locate(index, document);
      active = index;
      reveal(index);
      onNavigate(location);
      if (!disposed && enabled && snapshot === document)
        status.textContent = `Heading: ${document.nodes[index].text}. Source navigation selects its enclosing Markdown block.`;
    } catch (failure) { if (!disposed && snapshot === document) error(failure); }
    buttons();
  });
  listen(sourceButton, "click", () => {
    if (!enabled || active === null) return;
    try {
      locate(active);
      sourceEditor.focus();
      const { sourceRange } = locate(active);
      sourceEditor.setSelectionRange(sourceRange.start, sourceRange.end);
      status.textContent = "Selected the enclosing original Markdown block, not an inferred inline range.";
    } catch (failure) { if (!disposed) error(failure); }
  });
  const clear = () => {
    reset();
    view.clear();
    snapshot = null;
    snapshotSource = null;
    outline.replaceChildren();
  };
  buttons();
  return Object.freeze({
    get busy() { return pending !== null; },
    whenIdle() { return settled; },
    update(state) {
      if (disposed) return;
      const wasEnabled = enabled;
      enabled = false;
      if (state.status === "idle" || state.status === "disposed") {
        clear();
        status.textContent = "Reading navigation is not active.";
      } else if (state.status === "ready" && state.readingPending) {
        cancel();
        status.textContent = "Collecting semantic reading text; Canvas and source remain usable.";
      } else if (state.status === "ready") {
        if (!state.document || state.readingError) {
          clear();
          status.textContent = state.readingError
            ? `${state.readingError.code}: semantic reader unavailable; Canvas and source remain usable.`
            : "No semantic reading snapshot is available.";
        } else {
          try {
            state.document.assertCurrent();
            if (state.document !== snapshot) {
              reset();
              view.render(state.document);
              snapshot = state.document;
              snapshotSource = sourceEditor.value;
              const fragment = root.ownerDocument.createDocumentFragment(),
                option = root.ownerDocument.createElement("option");
              option.value = "";
              option.textContent = "Choose a heading";
              fragment.append(option);
              for (const heading of snapshot.headings) {
                const item = root.ownerDocument.createElement("option");
                item.value = String(heading.index);
                item.textContent = `${"  ".repeat(heading.level - 1)}H${heading.level}: ${heading.text}`;
                fragment.append(item);
              }
              outline.replaceChildren(fragment);
              enabled = true;
              search();
            } else if (sourceEditor.value !== snapshotSource) sourceChanged();
            else {
              enabled = true;
              if (!sameKey(searched, key())) search();
              else if (!wasEnabled) summary();
            }
          } catch (failure) {
            enabled = false;
            cancel();
            error(failure);
          }
        }
      } else {
        cancel();
        status.textContent = "Preview changing or unavailable; navigation is paused. Previously shown text remains selectable.";
      }
      buttons();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      enabled = false;
      for (const remove of listeners) remove();
      clear();
      view.dispose();
      buttons();
    },
  });
}
