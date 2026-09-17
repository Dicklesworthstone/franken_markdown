// Live-preview orchestration, not a renderer. At most one worker mutation or
// paint is awaited; user input replaces the one latest desired state. Markdown
// remains authoritative in the host, never reconstructed from a stale preview.
import { sourceText, FlowError } from "../flow_session.mjs";

export function createPreviewController({ createSession, painter, onState = () => {} }) {
  let desired = null, version = 0, epoch = 0, running = false, queued = false;
  let session = null, appliedSource = null, disposed = false, reading = "";
  let readingRevision = null, startup = null, paint = null, waiters = [];
  let state = Object.freeze({ status: "idle", frame: null, reading: "", error: null });
  const publish = update => { state = Object.freeze({ ...state, ...update }); onState(state); };
  const settle = () => { const all = waiters; waiters = []; for (const resolve of all) resolve(state); };
  const alive = () => { if (disposed) throw new FlowError("SESSION_DISPOSED", "preview controller is disposed"); };
  const valid = input => {
    if (!input || typeof input !== "object") throw new FlowError("INVALID_OPTIONS", "preview input must be an object");
    const value = { source: sourceText(input.source), width: input.width, height: input.height,
      scrollY: input.scrollY ?? 0, pixelRatio: input.pixelRatio ?? 1 };
    for (const name of ["width", "height", "scrollY", "pixelRatio"]) {
      if (typeof value[name] !== "number" || !Number.isFinite(value[name]) || value[name] < 0) {
        throw new FlowError("INVALID_OPTIONS", `${name} must be finite and nonnegative`);
      }
    }
    if (value.width < 1 || value.height < 1 || value.pixelRatio <= 0) throw new FlowError("INVALID_OPTIONS", "viewport dimensions and pixel ratio must be positive");
    return Object.freeze(value);
  };
  const schedule = () => {
    if (queued || running || disposed || desired === null) return;
    queued = true;
    queueMicrotask(() => { queued = false; if (!disposed) void run(); else settle(); });
  };
  async function transcript(current) {
    if (readingRevision === current.revision) return;
    const expected = current.token;
    const parts = [];
    let offset = 0, length = 0, truncated = false;
    // A selectable text counterpart, not an invented accessibility structure.
    // Limit DOM/transcript retention separately from the renderer's data pages.
    while (offset < 2048 && length < 1024 * 1024) {
      const page = await current.readingOrder({ offset, limit: 256, token: expected });
      for (const node of page.nodes) {
        if (typeof node.text !== "string") throw new FlowError("INVALID_WASM_RESPONSE", "reading node lacks text");
        if (length + node.text.length + 2 > 1024 * 1024) { truncated = true; break; }
        parts.push(node.text); length += node.text.length + 2;
      }
      if (truncated || page.nextOffset === null) break;
      if (!Number.isInteger(page.nextOffset) || page.nextOffset <= offset) throw new FlowError("INVALID_WASM_RESPONSE", "reading page did not advance");
      offset = page.nextOffset;
      if (offset >= 2048) truncated = true;
    }
    return { revision: expected.revision, text: parts.join("\n\n") + (truncated ? "\n\n[Reading text truncated by this demo's display limit.]" : "") };
  }
  async function run() {
    if (running || disposed || !desired) return;
    running = true;
    try {
      while (!disposed) {
        const intent = desired, ticket = version, generation = epoch;
        const currentIntent = () => !disposed && epoch === generation && version === ticket;
        publish({ status: "busy", error: null });
        try {
          if (!session) {
            const controller = new AbortController(); startup = controller;
            const created = await createSession(intent.source, { viewportWidth: intent.width }, { signal: controller.signal });
            if (startup === controller) startup = null;
            if (disposed || epoch !== generation) { created.dispose(); if (disposed) break; continue; }
            session = created; appliedSource = intent.source;
          }
          const current = session;
          if (!currentIntent()) continue;
          if (appliedSource !== intent.source) {
            await current.replaceSource(intent.source, { expectedRevision: current.revision });
            if (epoch !== generation || disposed) continue;
            appliedSource = intent.source;
          }
          if (!currentIntent()) continue;
          if (current.layoutOptions.viewportWidth !== intent.width) {
            await current.reflow({ viewportWidth: intent.width }, current.token);
          }
          if (!currentIntent()) continue;
          const nextReading = await transcript(current);
          if (!currentIntent()) continue;
          const controller = new AbortController(); paint = controller;
          const frame = await painter.render(current, { width: intent.width, height: intent.height,
            scrollY: intent.scrollY, pixelRatio: intent.pixelRatio, token: current.token, signal: controller.signal });
          if (paint === controller) paint = null;
          if (!currentIntent()) continue;
          if (nextReading) { reading = nextReading.text; readingRevision = nextReading.revision; }
          publish({ status: "ready", frame, reading, error: null });
          if (currentIntent()) break;
        } catch (error) {
          if (!currentIntent()) { if (disposed) break; continue; }
          // Do not automatically recreate/replay after a lost worker. The host
          // retains desired.source and can explicitly request restart().
          publish({ status: "error", error: Object.freeze({
            code: typeof error?.code === "string" ? error.code : "PREVIEW_ERROR",
            message: error instanceof Error ? error.message : "preview operation failed",
          }) });
          break;
        }
      }
    } finally { running = false; settle(); }
  }
  return Object.freeze({
    get state() { return state; },
    get disposed() { return disposed; },
    update(input) {
      alive(); const next = valid(input);
      if (desired && Object.keys(next).every(key => next[key] === desired[key])) return;
      desired = next; version++;
      paint?.abort(); paint = null; schedule();
    },
    restart() {
      alive(); epoch++; version++;
      startup?.abort(); startup = null; paint?.abort(); paint = null;
      const previous = session; session = null; appliedSource = null; reading = ""; readingRevision = null;
      previous?.dispose(); painter.clear();
      publish({ status: "idle", frame: null, reading: "", error: null }); schedule();
    },
    whenIdle() {
      if (!running && !queued) return Promise.resolve(state);
      return new Promise(resolve => waiters.push(resolve));
    },
    dispose() {
      if (disposed) return;
      disposed = true; epoch++;
      startup?.abort(); paint?.abort(); session?.dispose(); session = null;
      desired = null; appliedSource = null; reading = ""; painter.dispose();
      publish({ status: "disposed", frame: null, reading: "", error: null }); settle();
    }
  });
}
