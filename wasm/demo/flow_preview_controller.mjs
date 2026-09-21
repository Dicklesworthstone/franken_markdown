// Live-preview orchestration, not a renderer. At most one worker mutation or
// paint is awaited; user input replaces the one latest desired state. Markdown
// remains authoritative in the host, never reconstructed from a stale preview.

import { requireReadingDocument, sourceSpanToUtf16 } from "../flow_reading.mjs";
import { FlowError, validateCreation } from "../flow_session.mjs";
import { createPreviewExport } from "./flow_preview_export.mjs";

export function createPreviewController({
  createSession,
  painter,
  createAssets = null,
  readDocument = null,
  onState = () => {},
}) {
  let desired = null,
    version = 0,
    epoch = 0,
    running = false,
    queued = false;
  let session = null,
    appliedSource = null,
    appliedFont = null,
    disposed = false,
    reading = "";
  let layoutVersion = 0;
  let readingDocument = null,
    readingToken = null,
    readingError = null;
  let readingRevision = null,
    startup = null,
    paint = null,
    waiters = [];
  let assets = null,
    assetJob = null,
    imagesRevision = null;
  let state = Object.freeze({
    status: "idle",
    frame: null,
    reading: "",
    error: null,
    images: null,
    document: null,
    readingError: null,
  });
  const layoutKeys = ["viewportWidth", "bodySize", "codeSize", "lineHeight"];
  const targetLayout = (value) => ({
    viewportWidth: value.width,
    bodySize: value.bodySize,
    codeSize: value.codeSize,
    lineHeight: value.lineHeight,
  });
  const layoutCurrent = () => {
    if (!session || session.disposed || !desired || appliedFont !== desired.font) return false;
    const actual = session.layoutOptions,
      expected = targetLayout(desired);
    return layoutKeys.every((key) => actual[key] === expected[key]);
  };
  const exportDocument = createPreviewExport(() => ({
    session,
    source: appliedSource,
    desiredSource: desired?.source,
    status: state.status,
    frame: state.frame,
    imagesBusy: Boolean(assetJob?.owner === assets && assetJob),
    layoutPending: !layoutCurrent(),
    layoutVersion,
    epoch,
    disposed,
  }));
  const publish = (update) => {
    state = Object.freeze({ ...state, ...update });
    onState(state);
  };
  const settle = () => {
    const all = waiters;
    waiters = [];
    for (const resolve of all) resolve(state);
  };
  const alive = () => {
    if (disposed) throw new FlowError("SESSION_DISPOSED", "preview controller is disposed");
  };
  const valid = (input) => {
    if (!input || typeof input !== "object" || Array.isArray(input))
      throw new FlowError("INVALID_OPTIONS", "preview input must be an object");
    if (typeof input.width !== "number")
      throw new FlowError("INVALID_OPTIONS", "viewport width is required");
    // Use the same defaults and f32 admission as the worker, not browser CSS
    // or a second set of layout rules. Snapshot primitives before any await.
    const creation = validateCreation(input.source, {
      font: input.font,
      viewportWidth: input.width,
      bodySize: input.bodySize,
      codeSize: input.codeSize,
      lineHeight: input.lineHeight,
    });
    const value = {
      source: creation.source,
      font: creation.font,
      width: creation.layout.viewportWidth,
      bodySize: creation.layout.bodySize,
      codeSize: creation.layout.codeSize,
      lineHeight: creation.layout.lineHeight,
      height: input.height,
      scrollY: input.scrollY ?? 0,
      pixelRatio: input.pixelRatio ?? 1,
    };
    for (const name of ["width", "height", "scrollY", "pixelRatio"]) {
      if (typeof value[name] !== "number" || !Number.isFinite(value[name]) || value[name] < 0) {
        throw new FlowError("INVALID_OPTIONS", `${name} must be finite and nonnegative`);
      }
    }
    if (value.width < 1 || value.height < 1 || value.pixelRatio <= 0)
      throw new FlowError(
        "INVALID_OPTIONS",
        "viewport dimensions and pixel ratio must be positive",
      );
    return Object.freeze(value);
  };
  const schedule = () => {
    if (queued || running || disposed || desired === null) return;
    queued = true;
    queueMicrotask(() => {
      queued = false;
      if (!disposed) void run();
      else settle();
    });
  };
  const refresh = () => {
    if (disposed) return;
    version++;
    paint?.abort();
    paint = null;
    schedule();
  };
  // Images never block the first text/placeholder frame or subsequent edits.
  // One physical image batch is allowed across ALL worker restarts, including
  // old callbacks ignoring cancellation. New sessions can still render text.
  function beginImages(current) {
    if (!assets || assetJob || imagesRevision === current.revision || current.disposed) return;
    const job = {
      owner: assets,
      session: current,
      revision: current.revision,
      token: current.token,
      epoch,
    };
    assetJob = job;
    imagesRevision = job.revision;
    publish({ images: Object.freeze({ status: "loading", revision: job.revision }) });
    void (async () => {
      let report = null,
        error = null;
      try {
        report = await job.owner.loadPending();
      } catch (failure) {
        error = failure;
      }
      // A public timeout may precede physical completion. Retain this slot and
      // observe late deliveries/cleanup before scheduling any replacement batch.
      await job.owner.whenIdle();
      if (assetJob === job) assetJob = null;
      if (disposed) return;
      if (assets !== job.owner || session !== current || epoch !== job.epoch) {
        refresh();
        return;
      }
      if (current.disposed) {
        publish({
          status: "error",
          error: Object.freeze({
            code: "SESSION_LOST",
            message: "Image delivery lost the worker; restart explicitly.",
          }),
        });
        return;
      }
      if (current.revision !== job.revision) {
        refresh();
        return;
      }
      if (
        error?.code === "STALE_LAYOUT" &&
        current.token.layoutRevision !== job.token.layoutRevision
      ) {
        imagesRevision = null;
        refresh();
        return; // Inventory read raced a resize, before any delivery.
      }
      publish({
        images: Object.freeze(
          report
            ? { status: "ready", ...report }
            : {
                status: "error",
                revision: job.revision,
                code: typeof error?.code === "string" ? error.code : "ASSET_LOAD_FAILED",
              },
        ),
      });
      if (current.token.layoutRevision !== state.frame?.layoutRevision) refresh();
    })().catch((error) => {
      // Host factory/observer errors must not become unhandled rejections or
      // an automatic replay loop. The authoritative source is still retained.
      if (assetJob === job) assetJob = null;
      if (!disposed && assets === job.owner)
        publish({ images: Object.freeze({ status: "error", code: "ASSET_LOAD_FAILED" }) });
    });
  }
  async function transcript(current) {
    if (readDocument) {
      const expected = current.token;
      if (
        readingToken?.revision === expected.revision &&
        readingToken.layoutRevision === expected.layoutRevision
      )
        return;
      try {
        const document = requireReadingDocument(
          await readDocument(current, { token: expected }),
          current,
        );
        if (
          document.token.revision !== expected.revision ||
          document.token.layoutRevision !== expected.layoutRevision
        ) {
          throw new FlowError("STALE_LAYOUT", "reading snapshot differs from prepared layout");
        }
        return {
          revision: expected.revision,
          token: expected,
          text: document.text,
          document,
          error: null,
        };
      } catch (error) {
        // An optional reading limit/format failure must not disable valid Canvas
        // ink. Token races and worker loss still follow the normal recovery path.
        if (
          current.disposed ||
          [
            "STALE_LAYOUT",
            "STALE_REVISION",
            "ABORTED",
            "SESSION_LOST",
            "SESSION_DISPOSED",
          ].includes(error?.code)
        )
          throw error;
        return {
          revision: expected.revision,
          token: expected,
          text: "",
          document: null,
          error: Object.freeze({
            code: typeof error?.code === "string" ? error.code : "READING_ERROR",
          }),
        };
      }
    }
    if (readingRevision === current.revision) return;
    const expected = current.token;
    const parts = [];
    let offset = 0,
      length = 0,
      truncated = false;
    // A selectable text counterpart, not an invented accessibility structure.
    // Limit DOM/transcript retention separately from the renderer's data pages.
    while (offset < 2048 && length < 1024 * 1024) {
      const page = await current.readingOrder({ offset, limit: 256, token: expected });
      for (const node of page.nodes) {
        if (typeof node.text !== "string")
          throw new FlowError("INVALID_WASM_RESPONSE", "reading node lacks text");
        if (length + node.text.length + 2 > 1024 * 1024) {
          truncated = true;
          break;
        }
        parts.push(node.text);
        length += node.text.length + 2;
      }
      if (truncated || page.nextOffset === null) break;
      if (!Number.isInteger(page.nextOffset) || page.nextOffset <= offset)
        throw new FlowError("INVALID_WASM_RESPONSE", "reading page did not advance");
      offset = page.nextOffset;
      if (offset >= 2048) truncated = true;
    }
    return {
      revision: expected.revision,
      text:
        parts.join("\n\n") +
        (truncated ? "\n\n[Reading text truncated by this demo's display limit.]" : ""),
    };
  }
  async function run() {
    if (running || disposed || !desired) return;
    running = true;
    try {
      while (!disposed) {
        const intent = desired,
          ticket = version,
          generation = epoch;
        const currentIntent = () => !disposed && epoch === generation && version === ticket;
        publish({ status: "busy", error: null });
        try {
          if (!session) {
            const controller = new AbortController();
            startup = controller;
            const created = await createSession(
              intent.source,
              { font: intent.font, ...targetLayout(intent) },
              { signal: controller.signal },
            );
            if (startup === controller) startup = null;
            if (disposed || epoch !== generation) {
              created.dispose();
              if (disposed) break;
              continue;
            }
            session = created;
            appliedSource = intent.source;
            appliedFont = intent.font;
            assets = createAssets ? createAssets(created) : null;
            imagesRevision = null;
          }
          const current = session;
          if (!currentIntent()) continue;
          if (appliedSource !== intent.source) {
            await current.replaceSource(intent.source, { expectedRevision: current.revision });
            if (epoch !== generation || disposed) continue;
            appliedSource = intent.source;
            assets?.synchronize();
            publish({ images: null });
          }
          if (!currentIntent()) continue;
          const layout = targetLayout(intent),
            actualLayout = current.layoutOptions;
          if (layoutKeys.some((key) => actualLayout[key] !== layout[key])) {
            await current.reflow(layout, current.token);
          }
          if (!currentIntent()) continue;
          const expected = current.token;
          const nextReading = await transcript(current);
          if (!currentIntent()) continue;
          const controller = new AbortController();
          paint = controller;
          const owner = assets;
          const frame = await painter.render(current, {
            width: intent.width,
            height: intent.height,
            scrollY: intent.scrollY,
            pixelRatio: intent.pixelRatio,
            token: expected,
            signal: controller.signal,
            ...(owner ? { resolveImage: (image, token) => owner.resolveImage(image, token) } : {}),
          });
          if (paint === controller) paint = null;
          if (!currentIntent()) continue;
          if (nextReading) {
            reading = nextReading.text;
            readingRevision = nextReading.revision;
            readingDocument = nextReading.document ?? null;
            readingToken = nextReading.token ?? null;
            readingError = nextReading.error ?? null;
          }
          publish({
            status: "ready",
            frame,
            reading,
            document: readingDocument,
            readingError,
            error: null,
          });
          beginImages(current);
          if (currentIntent()) break;
        } catch (error) {
          if (!currentIntent()) {
            if (disposed) break;
            continue;
          }
          // Image deliveries may change layout during reflow/read/paint. Wait
          // for that one batch, then repaint once, rather than spin or reparse.
          if (assetJob?.owner === assets && error?.code === "STALE_LAYOUT") break;
          // Do not automatically recreate/replay after a lost worker. The host
          // retains desired.source and can explicitly request restart().
          publish({
            status: "error",
            error: Object.freeze({
              code: typeof error?.code === "string" ? error.code : "PREVIEW_ERROR",
              message: error instanceof Error ? error.message : "preview operation failed",
            }),
          });
          break;
        }
      }
    } finally {
      running = false;
      settle();
    }
  }
  function restart() {
    alive();
    epoch++;
    version++;
    startup?.abort();
    startup = null;
    paint?.abort();
    paint = null;
    const previous = session,
      previousAssets = assets;
    session = null;
    appliedFont = null;
    assets = null;
    imagesRevision = null;
    appliedSource = null;
    reading = "";
    readingRevision = null;
    readingDocument = null;
    readingToken = null;
    readingError = null;
    painter.clear();
    previousAssets?.dispose();
    previous?.dispose();
    publish({
      status: "idle",
      frame: null,
      reading: "",
      error: null,
      images: null,
      document: null,
      readingError: null,
    });
    schedule();
  }
  return Object.freeze({
    get state() {
      return state;
    },
    get disposed() {
      return disposed;
    },
    exportDocument,
    locateReading(index, document, source) {
      alive();
      if (
        !readingDocument ||
        document !== readingDocument ||
        state.status !== "ready" ||
        appliedSource !== desired?.source ||
        source !== appliedSource
      ) {
        throw new FlowError(
          "STALE_REVISION",
          "reading navigation does not match the current editor and preview",
        );
      }
      if (!layoutCurrent())
        throw new FlowError(
          "STALE_LAYOUT",
          "reading navigation is awaiting the requested typography",
        );
      const location = readingDocument.locate(index);
      if (
        location.revision !== state.frame?.revision ||
        location.layoutRevision !== state.frame?.layoutRevision
      ) {
        throw new FlowError("STALE_LAYOUT", "reading location does not match displayed pixels");
      }
      return Object.freeze({
        ...location,
        sourceRange: sourceSpanToUtf16(appliedSource, location.enclosingSourceSpan),
      });
    },
    update(input) {
      alive();
      const next = valid(input);
      if (desired && Object.keys(next).every((key) => next[key] === desired[key])) return;
      const fontChanged = desired && desired.font !== next.font;
      if (
        !desired ||
        ["font", "width", "bodySize", "codeSize", "lineHeight"].some(
          (key) => desired[key] !== next[key],
        )
      )
        layoutVersion++;
      desired = next;
      version++;
      // Bundled faces are chosen at native session creation. Rebuild explicitly
      // rather than relabeling old glyphs, while retaining the host image grant.
      if (fontChanged) {
        restart();
        return;
      }
      paint?.abort();
      paint = null;
      schedule();
    },
    restart,
    whenIdle() {
      // Preview idle does not mean that optional background image I/O settled.
      if (!running && !queued) return Promise.resolve(state);
      return new Promise((resolve) => waiters.push(resolve));
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      epoch++;
      startup?.abort();
      paint?.abort();
      painter.dispose();
      assets?.dispose();
      assets = null;
      session?.dispose();
      session = null;
      desired = null;
      appliedSource = null;
      appliedFont = null;
      reading = "";
      readingDocument = null;
      readingToken = null;
      readingError = null;
      publish({
        status: "disposed",
        frame: null,
        reading: "",
        error: null,
        images: null,
        document: null,
        readingError: null,
      });
      settle();
    },
  });
}
