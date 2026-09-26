// Canvas-to-source/navigation UI over the existing native hit and reading APIs.
// Never treat fragment-local offsets or display-item indices as Markdown offsets.
export function createCanvasNavigationControls({
  canvas, viewport, sourceEditor, sourceButton, followButton, status,
  getOwner, onNavigate = () => {},
}) {
  let disposed = false, suspended = false, serial = 0, selected = null, pending = null;
  let settled = Promise.resolve();
  const listeners = [];
  const fail = (code, message) => { throw Object.assign(new Error(message), { code }); };
  const tokenEqual = (a, b) => a && b && a.revision === b.revision
    && a.layoutRevision === b.layoutRevision;
  const buttons = () => {
    sourceButton.disabled = disposed || suspended || pending !== null || selected === null;
    followButton.disabled = sourceButton.disabled || selected?.destination === null;
  };
  const clear = (message) => {
    serial++;
    selected = null;
    buttons();
    if (!disposed && message) status.textContent = message;
  };
  function capture() {
    const owner = getOwner(), controller = owner?.controller, painter = owner?.painter;
    const state = controller?.state, frame = painter?.frame, document = state?.document;
    if (disposed || suspended || !controller || controller.disposed || !painter || painter.disposed
        || state?.status !== "ready" || !frame || state.frame !== frame
        || !document || state.readingPending || state.readingError || !document.nodes.length)
      fail("NAVIGATION_UNAVAILABLE", "Wait for the current Canvas and reading snapshot.");
    const source = sourceEditor.value;
    if (!tokenEqual(document.token, frame) || viewport.scrollTop !== frame.scrollY
        || viewport.scrollLeft !== frame.scrollX)
      fail("STALE_LAYOUT", "Canvas navigation is waiting for the displayed viewport.");
    document.assertCurrent();
    // This checks the private applied/desired source, session identity, pending
    // typography and displayed layout, even before an editor input event/RAF.
    controller.locateReading(0, document, source);
    return { controller, painter, document, frame, source, serial };
  }
  function current(context) {
    const now = capture();
    if (context.serial !== serial || now.controller !== context.controller
        || now.painter !== context.painter || now.document !== context.document
        || now.frame !== context.frame || now.source !== context.source)
      fail("STALE_LAYOUT", "This Canvas inspection no longer owns the shown document.");
    return now;
  }
  function location(context, index) {
    current(context);
    const result = context.controller.locateReading(index, context.document, context.source);
    current(context); // Host callbacks may synchronously change the document.
    return result;
  }
  function destination(context, target) {
    if (typeof target !== "string" || target.length > 4096 || !target.startsWith("#")
        || target.length === 1 || /[\u0000-\u001f\u007f-\u009f\\]/u.test(target)) return null;
    current(context);
    const found = context.document.locateFragment(target);
    current(context);
    if (!found) return null;
    return location(context, found.nodeIndex);
  }
  function origin(document, hit) {
    const span = hit?.enclosingSourceSpan;
    if (!Number.isSafeInteger(hit?.itemIndex) || hit.itemIndex < 0 || !span
        || !Number.isSafeInteger(span.startByte) || !Number.isSafeInteger(span.endByte)
        || span.startByte < 0 || span.endByte < span.startByte) return null;
    // Reading nodes may share an enclosing source block (e.g. table cells).
    // Exact envelope equality is sufficient for selecting THAT block; neither
    // the first cell nor the display-item index is claimed as an inline match.
    const index = document.nodes.findIndex(node =>
      node.enclosingSourceSpan.startByte === span.startByte
      && node.enclosingSourceSpan.endByte === span.endByte);
    return index < 0 ? null : index;
  }
  function report(error) {
    clear(`${error?.code ?? "NAVIGATION_ERROR"}: navigation unavailable; source is unchanged.`);
  }
  function inspect(event) {
    if (disposed || suspended || event.defaultPrevented || event.button !== 0
        || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    // One physical hit RPC across edits and restarts. Invalidation cancels its
    // authority, NOT the worker operation. Observe every late rejection and do
    // not build an event queue or retry against a different document.
    if (pending) {
      status.textContent = "A Canvas lookup is still pending. Click again after it finishes.";
      return;
    }
    clear();
    let context, x, y;
    try {
      context = capture();
      const area = canvas.getBoundingClientRect();
      if (![area.left, area.top, area.width, area.height, event.clientX, event.clientY,
            context.frame.width, context.frame.height].every(Number.isFinite)
          || area.width <= 0 || area.height <= 0
          || context.frame.width <= 0 || context.frame.height <= 0)
        fail("INVALID_POINT", "Canvas coordinates are not finite and positive.");
      x = (event.clientX - area.left) * context.frame.width / area.width;
      y = (event.clientY - area.top) * context.frame.height / area.height;
      if (x < 0 || y < 0 || x >= context.frame.width || y >= context.frame.height)
        fail("INVALID_POINT", "The point is outside the displayed Canvas.");
      current(context);
    } catch (error) { report(error); return; }
    const job = { context };
    pending = job;
    buttons();
    status.textContent = "Inspecting the shown Canvas location…";
    settled = Promise.resolve().then(() => {
      current(context);
      // FlowCanvasRenderer adds its captured scroll offsets. These are logical
      // viewport coordinates, not document coordinates or device pixels.
      return context.painter.hitTest(x, y);
    }).then(hit => {
      current(context);
      if (hit?.schemaVersion !== 1 || !tokenEqual(hit, context.frame))
        fail("INVALID_HIT", "Hit response does not match the displayed frame.");
      const index = origin(context.document, hit.hit);
      if (index === null) {
        status.textContent = "No source-backed text or image at this point. Nothing was selected.";
        return;
      }
      const sourceLocation = location(context, index);
      const span = hit.hit.enclosingSourceSpan;
      if (sourceLocation.enclosingSourceSpan.startByte !== span.startByte
          || sourceLocation.enclosingSourceSpan.endByte !== span.endByte)
        fail("INVALID_HIT", "Hit and reading source envelopes differ.");
      const target = typeof hit.linkTarget === "string" ? hit.linkTarget : null;
      const linked = destination(context, target);
      current(context);
      selected = { context, index, target, destination: linked };
      status.textContent = linked
        ? "Internal link selected. Follow the linked section or show its enclosing Markdown source."
        : target
          ? "Source block selected. This link is external, unsupported or unresolved and will not be opened."
          : "Source block selected. Show enclosing source to select the original Markdown, not fragment offsets.";
    }).catch(error => {
      // Stale lookups may complete after a newer error, edit, scroll or restart.
      // Never publish that old result or a late error over the current UI.
      if (!disposed && context.serial === serial) {
        try { current(context); report(error); }
        catch { clear("Canvas changed during lookup. Click the current preview again."); }
      }
    }).finally(() => {
      if (pending === job) pending = null;
      buttons();
    });
  }
  function showSource() {
    const choice = selected;
    if (!choice || pending || disposed) return;
    try {
      location(choice.context, choice.index);
      sourceEditor.focus();
      const found = location(choice.context, choice.index);
      const range = found.sourceRange;
      if (!Number.isSafeInteger(range?.start) || !Number.isSafeInteger(range?.end)
          || range.start < 0 || range.end < range.start || range.end > choice.context.source.length)
        fail("INVALID_SOURCE_RANGE", "Source mapping is not an in-bounds UTF-16 range.");
      // Focus above can run editor code; location was revalidated AFTER it.
      sourceEditor.setSelectionRange(range.start, range.end);
      current(choice.context);
      status.textContent = "Selected the enclosing original Markdown block. Source text is unchanged.";
    } catch (error) { if (!disposed) report(error); }
  }
  function followLink() {
    const choice = selected;
    if (!choice || pending || disposed || !choice.destination) return;
    try {
      const found = destination(choice.context, choice.target);
      if (!found || found.nodeIndex !== choice.destination.nodeIndex)
        fail("STALE_LAYOUT", "The selected link destination is no longer available.");
      // Revoke before calling host scrolling code. Native buttons handle Enter
      // and Space; no page-wide keyboard hook, href or location.hash is used.
      clear("Navigating to the engine-supplied internal heading. Source is unchanged.");
      onNavigate(found);
    } catch (error) { if (!disposed) report(error); }
  }
  function listen(element, type, handler, options) {
    const guarded = event => { if (!disposed && !suspended) handler(event); };
    element.addEventListener(type, guarded, options);
    listeners.push(() => element.removeEventListener(type, guarded, options));
  }
  listen(canvas, "click", inspect);
  listen(sourceButton, "click", showSource);
  listen(followButton, "click", followLink);
  listen(sourceEditor, "input", () => clear("Source changed; select a location in the matching preview."));
  listen(sourceEditor, "fmd-document-replaced", () => clear("Document replaced; old Canvas navigation was revoked."));
  listen(viewport, "scroll", () => clear("Viewport changed; select a location after it is repainted."), { passive: true });
  buttons();
  return Object.freeze({
    get busy() { return pending !== null; },
    whenIdle() { return settled; },
    update() {
      if (disposed || suspended) return;
      if (!selected && !pending) { buttons(); return; }
      const context = selected?.context ?? pending.context;
      try { current(context); }
      catch { clear("Preview changed; select a location in the current Canvas."); }
      buttons();
    },
    suspend() {
      if (disposed) return;
      suspended = true;
      clear("Canvas navigation is suspended. Select a fresh location after returning.");
    },
    resume() {
      if (disposed) return;
      suspended = false;
      clear("Select a location in the current Canvas to inspect its source or internal link.");
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      clear();
      for (const remove of listeners) remove();
    },
  });
}
