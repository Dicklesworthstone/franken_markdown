// Canvas presentation for measured flow snapshots. No Markdown/font parser,
// fillText(), DOM insertion, URL loading, or implicit worker/session ownership.

import { validateOutlineBatch } from "./flow_outlines.mjs";
import { FlowError, identity, coveringViewport, validateViewportPage } from "./flow_session.mjs";

const fail = (code, message) => {
  throw new FlowError(code, message);
};
const DEFAULT_LIMITS = Object.freeze({
  maxPixels: 16777216,
  maxScannedItems: 500000,
  maxVisibleItems: 10000,
  maxGlyphs: 50000,
  maxDrawCommands: 2000000,
  maxCachedGlyphs: 2048,
  maxCachedCommands: 262144,
  maxFrameCommands: 1048576,
});
const DEFAULT_COLORS = Object.freeze({
  background: "#ffffff",
  text: "#1f2328",
  heading: "#1f2328",
  code: "#1f2328",
  link: "#0969da",
  border: "#d0d7de",
  "table-border": "#d0d7de",
  accent: "#0969da",
  quote: "#656d76",
  muted: "#656d76",
  strikethrough: "#656d76",
  selection: "#c8e1ff",
});
const number = (n, name, max = 1e12) => {
  if (typeof n !== "number" || !Number.isFinite(n) || Math.abs(n) > max)
    fail("INVALID_ARGUMENT", `${name} must be a finite bounded number`);
  return n;
};
const uint = (n, name, max) => {
  if (!Number.isSafeInteger(n) || n < 0 || n > max) fail("INVALID_ARGUMENT", `invalid ${name}`);
  return n;
};
function fields(value, allowed) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    fail("INVALID_OPTIONS", "options must be an object");
  for (const key of Object.keys(value))
    if (!allowed.includes(key)) fail("INVALID_OPTIONS", `unknown option: ${key}`);
}
function token(value) {
  if (!value || typeof value !== "object")
    fail("INVALID_IDENTITY", "a display revision token is required");
  return Object.freeze({
    revision: identity(value.revision),
    layoutRevision: identity(value.layoutRevision),
  });
}
function same(a, b) {
  return a.revision === b.revision && a.layoutRevision === b.layoutRevision;
}
function rect(value) {
  if (!value) fail("INVALID_WASM_RESPONSE", "missing display bounds");
  const r = {
    x: number(value.x, "x"),
    y: number(value.y, "y"),
    width: number(value.width, "width"),
    height: number(value.height, "height"),
  };
  if (r.width < 0 || r.height < 0) fail("INVALID_WASM_RESPONSE", "negative display extent");
  number(r.x + r.width, "right");
  number(r.y + r.height, "bottom");
  return Object.freeze(r);
}
function intersect(a, b) {
  const x = Math.max(a.x, b.x),
    y = Math.max(a.y, b.y);
  return {
    x,
    y,
    width: Math.max(0, Math.min(a.x + a.width, b.x + b.width) - x),
    height: Math.max(0, Math.min(a.y + a.height, b.y + b.height) - y),
  };
}
function surface(width, height) {
  if (typeof OffscreenCanvas === "function") return new OffscreenCanvas(width, height);
  if (typeof document !== "undefined") {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    return canvas;
  }
  fail(
    "CANVAS_UNAVAILABLE",
    "supply a canvasFactory when OffscreenCanvas and document are unavailable",
  );
}
function abortError(signal, external) {
  return external
    ? new FlowError("ABORTED", "Canvas paint was aborted")
    : signal.reason instanceof FlowError
      ? signal.reason
      : new FlowError("RENDER_SUPERSEDED", "a newer paint replaced this one");
}
function wait(value, signals) {
  // Cancel waiting, NOT the session request. In-flight worker cancellation would
  // destroy the editing session; abandoning a paint must never do that.
  return new Promise((resolve, reject) => {
    const listeners = [];
    const cleanup = () => {
      for (const [signal, listener] of listeners) signal.removeEventListener("abort", listener);
    };
    let done = false;
    const finish = (f, result) => {
      if (!done) {
        done = true;
        cleanup();
        f(result);
      }
    };
    for (const [signal, external] of signals) {
      if (!signal) continue;
      const listener = () => finish(reject, abortError(signal, external));
      if (signal.aborted) listener();
      else if (!done) {
        signal.addEventListener("abort", listener, { once: true });
        listeners.push([signal, listener]);
      }
    }
    // Observe even an abandoned request's rejection; do not leak an unhandled
    // rejection when a late worker reply arrives after a superseding paint.
    Promise.resolve(value).then(
      (v) => finish(resolve, v),
      (e) => finish(reject, e),
    );
  });
}
function lineKey(plan) {
  return `${plan.bounds.y}/${plan.bounds.height}`;
}
function key(fontId, glyphId) {
  return `${fontId}/${glyphId}`;
}

/** Exclusively owns the target's 2D drawing state, not its CSS size or session.
 * One immutable viewport is staged, then blitted after revision checks. Pixels
 * and paths are bounded separately. Async failures leave the last frame intact.
 */
export class FlowCanvasRenderer {
  #canvas;
  #factory;
  #limits;
  #colors;
  #cache = new Map();
  #cachedCommands = 0;
  #metrics = new Map();
  #abort = null;
  #disposed = false;
  #presented = null;

  constructor(canvas, options = {}) {
    fields(options, ["canvasFactory", "limits", "colors"]);
    if (!canvas || typeof canvas.getContext !== "function")
      fail("CANVAS_UNAVAILABLE", "a Canvas or OffscreenCanvas is required");
    this.#canvas = canvas;
    this.#factory = options.canvasFactory ?? surface;
    if (typeof this.#factory !== "function")
      fail("INVALID_OPTIONS", "canvasFactory must be a function");
    fields(options.limits ?? {}, Object.keys(DEFAULT_LIMITS));
    this.#limits = { ...DEFAULT_LIMITS, ...options.limits };
    for (const name of Object.keys(DEFAULT_LIMITS))
      uint(this.#limits[name], name, DEFAULT_LIMITS[name]);
    fields(options.colors ?? {}, Object.keys(DEFAULT_COLORS));
    this.#colors = { ...DEFAULT_COLORS, ...options.colors };
    for (const value of Object.values(this.#colors)) {
      if (
        typeof value !== "string" ||
        value.length > 128 ||
        (typeof CSS !== "undefined" && !CSS.supports("color", value))
      )
        fail("INVALID_OPTIONS", "palette entries must be CSS colors");
    }
  }

  get disposed() {
    return this.#disposed;
  }
  get frame() {
    return this.#presented?.frame ?? null;
  }
  get cacheStats() {
    return Object.freeze({ glyphs: this.#cache.size, commands: this.#cachedCommands });
  }
  clearCache() {
    this.#cache.clear();
    this.#metrics.clear();
    this.#cachedCommands = 0;
  }
  /** Immediately stop showing a revoked/stale snapshot without a new render. */
  clear() {
    this.#abort?.abort(new FlowError("RENDER_SUPERSEDED", "Canvas output was cleared"));
    this.#presented = null;
    this.#canvas.width = this.#canvas.width;
  }
  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#abort?.abort(new FlowError("RENDERER_DISPOSED", "Canvas renderer was disposed"));
    this.clearCache();
    this.#presented = null;
    this.#canvas.width = this.#canvas.width;
  }

  async render(session, options = {}) {
    if (this.#disposed) fail("RENDERER_DISPOSED", "Canvas renderer was disposed");
    fields(options, [
      "width",
      "height",
      "scrollX",
      "scrollY",
      "pixelRatio",
      "token",
      "signal",
      "resolveImage",
      "selection",
    ]);
    if (
      !session ||
      typeof session.snapshot !== "function" ||
      typeof session.glyphOutlines !== "function"
    ) {
      fail(
        "UNSUPPORTED_WASM_PACKAGE",
        "Canvas rendering requires a flow session with glyphOutlines support",
      );
    }
    const width = number(options.width ?? session.layoutOptions.viewportWidth, "width", 16384);
    const height = number(options.height ?? 600, "height", 16384);
    const pixelRatio = number(options.pixelRatio ?? 1, "pixelRatio", 8);
    if (width <= 0 || height <= 0 || pixelRatio <= 0)
      fail("INVALID_OPTIONS", "viewport and pixel ratio must be positive");
    const pixelsWide = Math.ceil(width * pixelRatio),
      pixelsHigh = Math.ceil(height * pixelRatio);
    if (
      pixelsWide > 16384 ||
      pixelsHigh > 16384 ||
      pixelsWide * pixelsHigh > this.#limits.maxPixels
    ) {
      fail("BUDGET_EXCEEDED", "Canvas pixel budget exceeded");
    }
    const view = rect({ x: options.scrollX ?? 0, y: options.scrollY ?? 0, width, height });
    const expected = token(options.token ?? session.token);
    const signal = options.signal;
    if (
      signal !== undefined &&
      (!signal ||
        typeof signal.aborted !== "boolean" ||
        typeof signal.addEventListener !== "function" ||
        typeof signal.removeEventListener !== "function")
    ) {
      fail("INVALID_OPTIONS", "signal must be an AbortSignal");
    }
    const resolveImage = options.resolveImage;
    if (resolveImage !== undefined && typeof resolveImage !== "function")
      fail("INVALID_OPTIONS", "resolveImage must be a function");
    let selection = [];
    if (options.selection !== undefined) {
      if (
        !same(token(options.selection), expected) ||
        !Array.isArray(options.selection.rectangles)
      ) {
        fail("STALE_LAYOUT", "selection does not belong to the requested display");
      }
      if (options.selection.rectangles.length > this.#limits.maxVisibleItems)
        fail("BUDGET_EXCEEDED", "too many selection rectangles");
      selection = options.selection.rectangles.map(rect);
    }
    if (signal?.aborted) throw abortError(signal, true);
    this.#abort?.abort(new FlowError("RENDER_SUPERSEDED", "a newer paint replaced this one"));
    const attempt = new AbortController();
    this.#abort = attempt;
    const signals = [
      [attempt.signal, false],
      [signal, true],
    ];
    const check = () => {
      for (const [s, external] of signals) if (s?.aborted) throw abortError(s, external);
      if (session.disposed) fail("SESSION_DISPOSED", "the drawing session has been disposed");
      if (!same(token(session.token), expected))
        fail("STALE_LAYOUT", "layout changed while preparing Canvas output");
    };
    const read = async (value) => {
      const result = await wait(value, signals);
      check();
      return result;
    };
    check();
    // Negotiate once per paint. A method name alone is not a capability, and
    // a rejected indexed read must never trigger an unbounded snapshot retry.
    const indexed = session.supportsViewport === true;
    if (indexed && typeof session.viewport !== "function") {
      fail("UNSUPPORTED_WASM_PACKAGE", "session advertises indexed viewports without a viewport method");
    }
    const viewport = indexed ? coveringViewport(view) : null;
    const plans = [],
      need = new Map(),
      outlines = new Map(),
      metrics = new Map();
    const requireFont = (id) => {
      if (!need.has(id)) need.set(id, new Set());
    };
    let offset = 0,
      total = null,
      totalBounds = null,
      glyphCount = 0,
      frameCommands = 0;
    let clips = [], scannedItems = 0, receivedItems = 0;
    while (true) {
      let page, next;
      if (indexed) {
        const query = { viewport, afterIndex: offset, limit: 256, glyphs: true, token: expected };
        page = validateViewportPage(await read(session.viewport(query)), query, expected);
        next = page.nextIndex;
        scannedItems += page.visitedEntries;
      } else {
        page = await read(session.snapshot({ offset, limit: 256, glyphs: true, token: expected }));
        if (page?.schemaVersion !== 1 || !same(token(page), expected) || page.offset !== offset
            || !Number.isSafeInteger(page.total) || page.total < offset || !Array.isArray(page.items)
            || page.items.length !== Math.min(256, page.total - offset)
            || page.nextOffset !== (offset + page.items.length < page.total ? offset + page.items.length : null)) {
          fail("INVALID_WASM_RESPONSE", "inconsistent drawing page");
        }
        if (page.total > this.#limits.maxScannedItems) fail("BUDGET_EXCEEDED", "drawing inventory exceeds scan budget");
        if (page.shapingProfile !== "bundled-simple-ltr") fail("UNSUPPORTED_DISPLAY_ITEM", "Canvas currently supports the bundled flow profile");
        next = page.nextOffset;
        scannedItems += page.items.length;
      }
      if (scannedItems > this.#limits.maxScannedItems) fail("BUDGET_EXCEEDED", "drawing query exceeds scan budget");
      const boundsForPage = rect(page.totalBounds);
      if ((total !== null && page.total !== total) || (totalBounds !== null
          && Object.keys(totalBounds).some(key => totalBounds[key] !== boundsForPage[key]))) {
        fail("INVALID_WASM_RESPONSE", "drawing inventory changed between pages");
      }
      total = page.total; totalBounds ??= boundsForPage;
      receivedItems += page.items.length;
      let sequentialIndex = offset;
      for (const item of page.items) {
        // Indexed pages retain sparse ORIGINAL item IDs. Their effective clips
        // include ancestors omitted by the spatial query; never reconstruct a
        // clip stack from a sparse page or renumber items as visible ordinals.
        if (!indexed && item?.index !== sequentialIndex++) fail("INVALID_WASM_RESPONSE", "drawing items are not in stream order");
        const index = item.index;
        const bounds = rect(item.bounds);
        if (!indexed) {
          clips = clips.filter(clip => clip.end > index);
          if (item.kind === "clip") {
            uint(item.childCount, "clip child count", total - index - 1);
            if (clips.length >= 64) fail("BUDGET_EXCEEDED", "Canvas clip nesting exceeds 64");
            if (item.childCount) clips.push({ bounds, end: index + item.childCount + 1 });
            continue;
          }
        }
        if (bounds.y + bounds.height <= view.y || bounds.y >= view.y + view.height) continue;
        if (item.kind === "anchor") continue; // Interaction remains in the session.
        const clip = indexed ? intersect(view, rect(item.effectiveClip))
          : clips.reduce((area, active) => intersect(area, active.bounds), view);
        // Retain every text fragment at visible Y, even outside horizontal view,
        // so a style at the other end of a line cannot change its baseline on scroll.
        if (
          item.kind !== "text" &&
          (!intersect(bounds, clip).width || !intersect(bounds, clip).height)
        )
          continue;
        if (plans.length >= this.#limits.maxVisibleItems)
          fail("BUDGET_EXCEEDED", "too many visible drawing items");
        if (item.kind === "text") {
          const plan = textPlan(item, bounds, clip, this.#limits.maxGlyphs - glyphCount);
          glyphCount += plan.glyphs.length;
          requireFont(plan.fontId);
          for (const glyph of plan.glyphs) {
            requireFont(glyph.fontId);
            need.get(glyph.fontId).add(glyph.glyphId);
          }
          plans.push(plan);
        } else if (item.kind === "vector") {
          if (
            ![
              "horizontal-rule",
              "table-border",
              "callout-accent-bar",
              "checkbox-outline",
              "checkbox-check",
            ].includes(item.shape)
          ) {
            fail(
              "UNSUPPORTED_DISPLAY_ITEM",
              "diagram routes require full path geometry, not a guessed bounding-box drawing",
            );
          }
          const stroke = number(item.strokeWidth, "stroke width", 1000000);
          if (stroke < 0) fail("INVALID_WASM_RESPONSE", "negative stroke width");
          plans.push({
            kind: "vector",
            bounds,
            clip,
            shape: item.shape,
            stroke,
            color: item.colorRole,
          });
        } else if (item.kind === "image") {
          if (
            typeof item.isResolved !== "boolean" ||
            typeof item.destination !== "string" ||
            typeof item.altText !== "string"
          ) {
            fail("INVALID_WASM_RESPONSE", "invalid image descriptor");
          }
          plans.push({
            kind: "image",
            bounds,
            clip,
            image: null,
            info: Object.freeze({
              requestId: identity(item.requestId),
              destination: item.destination,
              altText: item.altText,
              isResolved: item.isResolved,
              bounds,
            }),
          });
        } else fail("UNSUPPORTED_DISPLAY_ITEM", "unknown display primitive");
      }
      if (next === null) break;
      offset = next;
    }
    const retain = (id, glyphId, entry) => {
      const k = key(id, glyphId);
      if (outlines.has(k)) return;
      frameCommands += entry.commands.length;
      if (frameCommands > this.#limits.maxFrameCommands)
        fail("BUDGET_EXCEEDED", "visible glyph paths exceed frame budget");
      outlines.set(k, entry);
    };
    for (const [id, ids] of need) {
      const missing = [];
      const known = this.#metrics.get(id);
      if (known) metrics.set(id, known);
      for (const glyphId of ids) {
        const k = key(id, glyphId),
          entry = this.#cache.get(k);
        if (entry) {
          this.#cache.delete(k);
          this.#cache.set(k, entry);
          retain(id, glyphId, entry);
        } else missing.push(glyphId);
      }
      // Empty batch requests metrics for a primary font whose glyphs all fell
      // back to other faces. No invented metrics or substitute face is used.
      for (
        let start = 0;
        start < missing.length || (start === 0 && !metrics.has(id));
        start += 256
      ) {
        const requested = missing.slice(start, start + 256);
        const batch = validateOutlineBatch(
          await read(session.glyphOutlines(id, requested)),
          id,
          requested,
        );
        const m = Object.freeze({
          unitsPerEm: batch.unitsPerEm,
          ascent: batch.ascent,
          descent: batch.descent,
          lineGap: batch.lineGap,
        });
        if (metrics.has(id) && JSON.stringify(metrics.get(id)) !== JSON.stringify(m))
          fail("INVALID_WASM_RESPONSE", "immutable font metrics changed");
        metrics.set(id, m);
        if (this.#metrics.size >= 32 && !this.#metrics.has(id))
          this.#metrics.delete(this.#metrics.keys().next().value);
        this.#metrics.set(id, m);
        for (const glyph of batch.glyphs) {
          const entry = { commands: glyph.commands.map((command) => Object.freeze([...command])) };
          retain(id, glyph.glyphId, entry);
          this.#cachePath(key(id, glyph.glyphId), entry);
        }
        if (!requested.length) break;
      }
    }
    let missingImages = 0;
    for (const plan of plans)
      if (plan.kind === "image") {
        if (plan.info.isResolved && resolveImage)
          plan.image = await read(resolveImage(plan.info, expected));
        if (plan.image == null) missingImages++;
      }
    check();
    const baselines = new Map();
    for (const plan of plans)
      if (plan.kind === "text") {
        const k = lineKey(plan),
          line = baselines.get(k) ?? { ascent: 0, descent: 0 };
        for (const id of new Set([plan.fontId, ...plan.glyphs.map((glyph) => glyph.fontId)])) {
          const m = metrics.get(id),
            scale = plan.size / m.unitsPerEm;
          line.ascent = Math.max(line.ascent, m.ascent * scale);
          line.descent = Math.max(line.descent, -m.descent * scale);
        }
        baselines.set(k, line);
      }
    let drawCommands = 0;
    for (const plan of plans)
      if (plan.kind === "text") {
        for (const glyph of plan.glyphs)
          drawCommands += outlines.get(key(glyph.fontId, glyph.glyphId)).commands.length;
      }
    if (drawCommands > this.#limits.maxDrawCommands)
      fail("BUDGET_EXCEEDED", "Canvas path execution budget exceeded");
    const stage = this.#factory(pixelsWide, pixelsHigh);
    if (
      stage === this.#canvas ||
      !stage ||
      stage.width !== pixelsWide ||
      stage.height !== pixelsHigh
    ) {
      fail("INVALID_OPTIONS", "canvasFactory must return a fresh correctly sized staging surface");
    }
    const context = stage.getContext("2d");
    if (!context) fail("CANVAS_UNAVAILABLE", "2D staging context is unavailable");
    context.fillStyle = this.#colors.background;
    context.fillRect(0, 0, pixelsWide, pixelsHigh);
    context.setTransform(pixelRatio, 0, 0, pixelRatio, -view.x * pixelRatio, -view.y * pixelRatio);
    context.fillStyle = this.#colors.selection;
    for (const area of selection) context.fillRect(area.x, area.y, area.width, area.height);
    for (const plan of plans) {
      context.save();
      context.beginPath();
      context.rect(plan.clip.x, plan.clip.y, plan.clip.width, plan.clip.height);
      context.clip();
      const color = Object.hasOwn(this.#colors, plan.color)
        ? this.#colors[plan.color]
        : this.#colors.text;
      context.fillStyle = color;
      context.strokeStyle = color;
      if (plan.kind === "text") {
        const line = baselines.get(lineKey(plan));
        const baseline =
          plan.bounds.y + (plan.bounds.height - line.ascent - line.descent) / 2 + line.ascent;
        for (const glyph of plan.glyphs) {
          const entry = outlines.get(key(glyph.fontId, glyph.glyphId)),
            m = metrics.get(glyph.fontId);
          context.save();
          context.translate(glyph.x, baseline - glyph.yOffset);
          context.scale(plan.size / m.unitsPerEm, -plan.size / m.unitsPerEm);
          drawPath(context, entry.commands);
          context.fill("nonzero");
          context.restore();
        }
      } else if (plan.kind === "vector") drawVector(context, plan);
      else if (plan.image != null)
        context.drawImage(
          plan.image,
          plan.bounds.x,
          plan.bounds.y,
          plan.bounds.width,
          plan.bounds.height,
        );
      else placeholder(context, plan.bounds, this.#colors.border);
      context.restore();
    }
    check();
    // No awaits between the final fence and publication. The renderer owns the
    // target context: resizing resets prior transforms/clips before a single blit.
    const target = this.#canvas.getContext("2d");
    if (!target) fail("CANVAS_UNAVAILABLE", "2D target context is unavailable");
    this.#presented = null; // A browser allocation/blit failure cannot leave a falsely valid token.
    this.#canvas.width = pixelsWide;
    this.#canvas.height = pixelsHigh;
    target.drawImage(stage, 0, 0);
    const frame = Object.freeze({
      ...expected,
      width,
      height,
      pixelRatio,
      scrollX: view.x,
      scrollY: view.y,
      totalBounds,
      totalItems: total,
      queryMode: indexed ? "indexed-viewport" : "snapshot",
      scannedItems,
      receivedItems,
      visibleItems: plans.length,
      glyphs: glyphCount,
      missingImages,
    });
    this.#presented = { session, frame };
    return frame;
  }

  #cachePath(k, entry) {
    if (!this.#limits.maxCachedGlyphs || entry.commands.length > this.#limits.maxCachedCommands)
      return;
    const previous = this.#cache.get(k);
    if (previous) {
      this.#cachedCommands -= previous.commands.length;
      this.#cache.delete(k);
    }
    while (
      this.#cache.size >= this.#limits.maxCachedGlyphs ||
      this.#cachedCommands + entry.commands.length > this.#limits.maxCachedCommands
    ) {
      const first = this.#cache.keys().next().value;
      this.#cachedCommands -= this.#cache.get(first).commands.length;
      this.#cache.delete(first);
    }
    this.#cache.set(k, entry);
    this.#cachedCommands += entry.commands.length;
  }

  documentPoint(x, y) {
    const frame = this.frame;
    if (!frame) fail("NO_FRAME", "no successfully presented Canvas frame");
    number(x, "local x");
    number(y, "local y");
    if (x < 0 || y < 0 || x >= frame.width || y >= frame.height) return null;
    return Object.freeze({ x: x + frame.scrollX, y: y + frame.scrollY });
  }

  async hitTest(x, y) {
    const shown = this.#presented,
      point = this.documentPoint(x, y);
    if (!point) fail("INVALID_ARGUMENT", "point is outside the displayed viewport");
    const hit = await shown.session.hitTest(point.x, point.y, shown.frame);
    if (shown !== this.#presented) fail("STALE_LAYOUT", "Canvas frame changed during hit testing");
    return hit;
  }
}

function textPlan(item, bounds, clip, glyphBudget) {
  const run = item.fontRun;
  if (
    !run ||
    run.coordinateSpace !== "fragment-utf8-and-utf16" ||
    run.fontOrigin !== "bundled" ||
    run.direction !== "ltr" ||
    !Array.isArray(run.glyphs) ||
    !Array.isArray(run.clusters) ||
    run.glyphCount !== run.glyphs.length ||
    run.clusterCount !== run.clusters.length ||
    run.clusters.length > run.glyphs.length
  ) {
    fail(
      "INVALID_WASM_RESPONSE",
      "complete bundled LTR glyph/cluster data is required; request glyphs:true",
    );
  }
  if (run.glyphs.length > glyphBudget) fail("BUDGET_EXCEEDED", "too many visible glyphs");
  const size = number(item.fontSize, "font size", 1000000);
  if (size <= 0 || size !== run.fontSize) fail("INVALID_WASM_RESPONSE", "font sizes disagree");
  const glyphs = new Array(run.glyphs.length);
  let count = 0;
  for (let index = 0; index < run.clusters.length; index++) {
    const cluster = run.clusters[index],
      range = cluster?.glyphs;
    if (cluster?.index !== index || !Array.isArray(range) || range.length !== 2)
      fail("INVALID_WASM_RESPONSE", "invalid glyph cluster");
    const start = uint(range[0], "glyph range start", glyphs.length),
      end = uint(range[1], "glyph range end", glyphs.length);
    if (end <= start) fail("INVALID_WASM_RESPONSE", "empty/reversed glyph range");
    let x = number(cluster.xStart, "cluster x"),
      advance = 0;
    const right = number(cluster.xEnd, "cluster end");
    if (x < 0 || right < x) fail("INVALID_WASM_RESPONSE", "reversed cluster geometry");
    const id = identity(cluster.fontId);
    for (let j = start; j < end; j++) {
      const glyph = run.glyphs[j];
      if (glyphs[j] || glyph?.clusterIndex !== index || glyph.fontId !== id)
        fail("INVALID_WASM_RESPONSE", "glyph ownership mismatch");
      const step = number(glyph.xAdvance, "glyph advance"),
        yOffset = number(glyph.yOffset, "glyph y offset");
      if (glyph.yAdvance !== 0)
        fail("UNSUPPORTED_DISPLAY_ITEM", "vertical glyph advances require vertical layout");
      glyphs[j] = {
        fontId: id,
        glyphId: uint(glyph.glyphId, "glyph ID", 65535),
        x: number(
          bounds.x + x + advance + number(glyph.xOffset, "glyph x offset"),
          "positioned glyph x",
        ),
        yOffset,
      };
      advance += step;
      count++;
    }
    if (Math.abs(advance - (right - x)) > 0.02 + Math.abs(advance) * 0.00001)
      fail("INVALID_WASM_RESPONSE", "glyph advances disagree with cluster geometry");
  }
  if (count !== glyphs.length)
    fail("INVALID_WASM_RESPONSE", "glyphs are missing cluster ownership");
  return {
    kind: "text",
    bounds,
    clip,
    size,
    fontId: identity(run.fontId),
    glyphs,
    color: item.colorRole,
  };
}
function drawPath(context, commands) {
  context.beginPath();
  for (const [op, a, b, c, d] of commands) {
    if (op === "M") context.moveTo(a, b);
    else if (op === "L") context.lineTo(a, b);
    else if (op === "Q") context.quadraticCurveTo(a, b, c, d);
    else context.closePath();
  }
}
function drawVector(context, plan) {
  const b = plan.bounds;
  if (plan.shape === "horizontal-rule" || plan.shape === "callout-accent-bar") {
    context.fillRect(b.x, b.y, b.width, b.height);
    return;
  }
  context.lineWidth = Math.max(0.5, plan.stroke);
  if (plan.shape === "table-border") {
    const inset = Math.min(context.lineWidth / 2, b.width / 2, b.height / 2);
    context.strokeRect(
      b.x + inset,
      b.y + inset,
      Math.max(0, b.width - 2 * inset),
      Math.max(0, b.height - 2 * inset),
    );
    return;
  }
  const size = Math.min(b.width, b.height),
    x = b.x,
    y = b.y + (b.height - size) / 2;
  if (plan.shape === "checkbox-outline")
    context.strokeRect(x + 1, y + 1, Math.max(0, size - 2), Math.max(0, size - 2));
  else {
    context.beginPath();
    context.moveTo(x + size * 0.2, y + size * 0.5);
    context.lineTo(x + size * 0.42, y + size * 0.72);
    context.lineTo(x + size * 0.82, y + size * 0.25);
    context.stroke();
  }
}
function placeholder(context, b, color) {
  context.strokeStyle = color;
  context.lineWidth = 1;
  context.strokeRect(b.x + 0.5, b.y + 0.5, Math.max(0, b.width - 1), Math.max(0, b.height - 1));
  context.beginPath();
  context.moveTo(b.x, b.y);
  context.lineTo(b.x + b.width, b.y + b.height);
  context.moveTo(b.x + b.width, b.y);
  context.lineTo(b.x, b.y + b.height);
  context.stroke();
}
