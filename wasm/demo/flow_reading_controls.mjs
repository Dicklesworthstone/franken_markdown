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
  // One physical DOM builder and one latest intent, not one abandoned tree per
  // preview notification. Generic paint-busy states pause navigation without
  // restarting an otherwise current build on every scroll event.
  let renderJob = null, renderWanted = null, renderFailed = null;
  let renderSettled = Promise.resolve(), previewReady = false;
  const sameRender = (a, b) => a && b && a.document === b.document && a.source === b.source;
  const cancelRender = () => {
    renderWanted = null;
    renderFailed = null;
    renderJob?.controller.abort();
  };
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
    previewReady = false;
    cancelRender();
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
  const renderCurrent = job => {
    if (disposed || !sameRender(job, renderWanted) || job.controller.signal.aborted)
      throw new FlowReadingError("RENDER_SUPERSEDED", "reading presentation was replaced");
    if (sourceEditor.value !== job.source)
      throw new FlowReadingError("STALE_REVISION", "reading presentation does not match the editor");
    job.document.assertCurrent();
  };
  const enableCurrent = () => {
    if (!previewReady || !sameRender(renderWanted, { document: snapshot, source: snapshotSource })) return;
    if (sourceEditor.value !== snapshotSource) { sourceChanged(); return; }
    snapshot.assertCurrent();
    enabled = true;
    if (!sameKey(searched, key())) search();
    else summary();
  };
  async function prepareOutline(job) {
    const doc = root.ownerDocument, fragment = doc.createDocumentFragment();
    const option = doc.createElement("option");
    option.value = "";
    option.textContent = "Choose a heading";
    fragment.append(option);
    let work = 0;
    for (const heading of job.document.headings) {
      const item = doc.createElement("option");
      item.value = String(heading.index);
      item.textContent = `${"  ".repeat(heading.level - 1)}H${heading.level}: ${heading.text}`;
      fragment.append(item);
      if (++work === 256) {
        work = 0;
        await outlineTurn(job.controller.signal);
        renderCurrent(job);
      }
    }
    return fragment;
  }
  function beginRender() {
    if (disposed || !previewReady || renderJob || !renderWanted) return;
    if (sameRender(renderWanted, renderFailed)) { error(renderFailed.error); return; }
    if (renderWanted.document === snapshot && renderWanted.source === snapshotSource) {
      enableCurrent();
      return;
    }
    const job = { ...renderWanted, controller: new AbortController() };
    renderJob = job;
    enabled = false;
    reset();
    status.textContent = "Preparing selectable reading text; Canvas and source remain usable.";
    buttons();
    renderSettled = Promise.resolve().then(async () => {
      renderCurrent(job);
      if (!previewReady) { job.controller.abort(); return; }
      // Fence unsent editor input before doing DOM work. Empty documents have
      // no location; their source value is still checked throughout this job.
      if (job.document.nodes.length) getLocation(0, job.document);
      renderCurrent(job);
      const choices = await prepareOutline(job);
      renderCurrent(job);
      if (typeof view.renderAsync !== "function")
        throw new FlowReadingError("UNSUPPORTED_READER", "use a matching reader with asynchronous rendering");
      await view.renderAsync(job.document, { signal: job.controller.signal });
      renderCurrent(job);
      if (view.document !== job.document)
        throw new FlowReadingError("READING_DOM_CHANGED", "reading view did not publish the requested document");
      // A completed tree may be retained while a scroll-only paint is busy,
      // but navigation is enabled only for a subsequently ready preview.
      if (previewReady && job.document.nodes.length) getLocation(0, job.document);
      renderCurrent(job);
      outline.replaceChildren(choices);
      renderCurrent(job);
      snapshot = job.document;
      snapshotSource = job.source;
    }).catch(failure => {
      if (disposed || !sameRender(job, renderWanted) || job.controller.signal.aborted) return;
      // Cache a refused build for this exact snapshot/source, rather than retry
      // it indefinitely on every scroll or image-state notification.
      renderFailed = { ...job, error: { code: typeof failure?.code === "string" ? failure.code : "READING_ERROR" } };
      if (previewReady) error(renderFailed.error);
    }).finally(() => {
      if (renderJob === job) renderJob = null;
      if (disposed) return;
      try { beginRender(); }
      catch (failure) { enabled = false; error(failure); }
      buttons();
    });
  }
  const clear = () => {
    cancelRender();
    reset();
    view.clear();
    snapshot = null;
    snapshotSource = null;
    outline.replaceChildren();
  };
  buttons();
  return Object.freeze({
    get busy() { return renderJob !== null || pending !== null; },
    async whenIdle() {
      // A completed build may start a search or release the latest queued
      // replacement. Drain those local stages, not Canvas, images or worker I/O.
      for (;;) {
        const rendering = renderSettled, searching = settled;
        await Promise.all([rendering, searching]);
        if (rendering === renderSettled && searching === settled) return;
      }
    },
    update(state) {
      if (disposed) return;
      const wasEnabled = enabled;
      enabled = false;
      previewReady = state.status === "ready" && !state.readingPending && !!state.document && !state.readingError;
      if (state.status === "idle" || state.status === "disposed") {
        clear();
        status.textContent = "Reading navigation is not active.";
      } else if (state.status === "ready" && state.readingPending) {
        cancelRender();
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
            const wanted = { document: state.document, source: sourceEditor.value };
            // Never relabel the displayed snapshot with a new, unsent source.
            if (state.document === snapshot && wanted.source !== snapshotSource) sourceChanged();
            else {
              if (!sameRender(wanted, renderWanted)) {
                renderJob?.controller.abort();
                renderFailed = null;
                renderWanted = wanted;
                if (state.document !== snapshot) reset();
              }
              if (state.document === snapshot && !renderJob) {
                enabled = true;
                if (!sameKey(searched, key())) search();
                else if (!wasEnabled) summary();
              } else {
                beginRender();
                if (renderJob) status.textContent = "Preparing selectable reading text; Canvas and source remain usable.";
              }
            }
          } catch (failure) {
            enabled = false;
            cancelRender();
            cancel();
            error(failure);
          }
        }
      } else {
        cancel();
        if (state.status !== "busy") cancelRender();
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

// Outline construction also cooperates: a large heading inventory must not
// replace the DOM-render stall with a synchronous select-option build.
function outlineTurn(signal) {
  return new Promise((resolve, reject) => {
    const finish = () => {
      clearTimeout(timer);
      signal.removeEventListener("abort", finish);
      if (signal.aborted) reject(new FlowReadingError("ABORTED", "reading outline was aborted"));
      else resolve();
    };
    const timer = setTimeout(finish, 0);
    signal.addEventListener("abort", finish, { once: true });
    if (signal.aborted) finish();
  });
}
