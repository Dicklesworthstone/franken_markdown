// Synthetic display/native oracle and recording 2D surface. No Markdown parser,
// spatial implementation or font codec is substituted for production evidence.
export const box = (x = 0, y = 0, width = 100, height = 20) => ({ x, y, width, height });
export const vector = (bounds = box(), colorRole = "text") => ({
  kind: "vector", bounds, shape: "horizontal-rule", strokeWidth: 1, colorRole
});
export function text(x = 0, y = 0, fontId = "1", count = 1) {
  return { kind: "text", bounds: box(x, y, 10 * count, 20), text: "x".repeat(count), fontSize: 10, colorRole: "text",
    fontRun: { coordinateSpace: "fragment-utf8-and-utf16", fontId, fontSize: 10, direction: "ltr", fontOrigin: "bundled",
      glyphCount: count, clusterCount: count,
      clusters: Array.from({ length: count }, (_, i) => ({ index: i, glyphs: [i, i + 1], xStart: i * 10, xEnd: (i + 1) * 10, fontId })),
      glyphs: Array.from({ length: count }, (_, i) => ({ glyphId: 1, clusterIndex: i, fontId, xAdvance: 10, yAdvance: 0, xOffset: 0, yOffset: 0 })) } };
}
export function paths(fontId, ids) {
  return { schemaVersion: 1, fontId, unitsPerEm: 10, ascent: fontId === "2" ? 12 : 8,
    descent: -2, lineGap: 0, glyphs: ids.map(glyphId => ({ glyphId,
      commands: [["M", 0, 0], ["L", 10, 0], ["L", 10, 10], ["L", 0, 10], ["Z"]] })) };
}
export const image = (bounds = box()) => ({ kind: "image", bounds, requestId: "9007199254740993",
  destination: "never-fetch://private", altText: "private", isResolved: true });
export const intersect = (a, b) => {
  const x = Math.max(a.x, b.x), y = Math.max(a.y, b.y);
  return box(x, y, Math.max(0, Math.min(a.x + a.width, b.x + b.width) - x),
    Math.max(0, Math.min(a.y + a.height, b.y + b.height) - y));
};
export class Session {
  disposed = false; token = { revision: "1", layoutRevision: "1" }; supportsViewport;
  snapshots = []; queries = []; outlines = []; hits = [];
  layoutOptions = { viewportWidth: 100, bodySize: 10, codeSize: 10, lineHeight: 20 };
  totalBounds = box(0, 0, 1000, 100000); items;
  constructor(items = [], indexed = true) {
    this.items = items.map((item, index) => ({ ...item, index })); this.supportsViewport = indexed;
  }
  snapshot(query) {
    this.snapshots.push(structuredClone(query));
    const { offset, limit } = query, end = Math.min(this.items.length, offset + limit);
    return structuredClone({ schemaVersion: 1, ...this.token, shapingProfile: "bundled-simple-ltr", totalBounds: this.totalBounds,
      offset, total: this.items.length, nextOffset: end < this.items.length ? end : null, items: this.items.slice(offset, end) });
  }
  // Independent full linear clip/visibility oracle for renderer parity checks.
  viewport(query) {
    this.queries.push(structuredClone(query));
    const { viewport: view, afterIndex, limit } = query, selected = [];
    let active = [];
    for (const item of this.items) {
      active = active.filter(clip => clip.end > item.index);
      if (item.kind === "clip") {
        if (item.childCount) active.push({ bounds: item.bounds, end: item.index + item.childCount + 1 });
        continue;
      }
      if (item.index < afterIndex) continue;
      const b = item.bounds;
      if (b.y + b.height <= view.y || b.y >= view.y + view.height) continue;
      const effectiveClip = active.reduce((area, clip) => intersect(area, clip.bounds), view);
      const visible = intersect(b, effectiveClip);
      // TextLines mode deliberately includes horizontally offscreen/clipped text.
      if (item.kind !== "text" && (!visible.width || !visible.height)) continue;
      selected.push({ ...item, effectiveClip });
      if (selected.length === limit) break;
    }
    const last = selected.at(-1)?.index;
    return structuredClone({ schemaVersion: 1, ...this.token, queryKind: "viewport-v1", shapingProfile: "bundled-simple-ltr",
      viewport: view, totalBounds: this.totalBounds, afterIndex, total: this.items.length,
      visitedEntries: this.items.length - afterIndex,
      nextIndex: selected.length === limit && last + 1 < this.items.length ? last + 1 : null, items: selected });
  }
  glyphOutlines(id, ids) { this.outlines.push([id, [...ids]]); return paths(id, ids); }
  hitTest(x, y, token) { this.hits.push({ x, y, token }); return { ...this.token, hit: { x, y, itemIndex: 950000 } }; }
  reflow() { this.token = { ...this.token, layoutRevision: String(BigInt(this.token.layoutRevision) + 1n) }; }
}
export function sparsePage(session, query, items, patch = {}) {
  return { schemaVersion: 1, ...session.token, queryKind: "viewport-v1", shapingProfile: "bundled-simple-ltr",
    viewport: query.viewport, afterIndex: query.afterIndex, total: 1000000,
    totalBounds: box(0, 0, 800, 20000000), visitedEntries: 32, nextIndex: null,
    items: items.map(item => ({ ...item, effectiveClip: query.viewport })), ...patch };
}
export class Surface {
  width; height; log = []; context;
  constructor(width = 100, height = 60) {
    this.width = width; this.height = height;
    const operations = new Set(["save", "restore", "fillRect", "strokeRect", "setTransform", "beginPath", "rect", "clip",
      "translate", "scale", "moveTo", "lineTo", "quadraticCurveTo", "closePath", "fill", "stroke", "drawImage"]);
    this.context = new Proxy({}, {
      get: (_, key) => operations.has(key) ? (...args) => {
        this.log.push([key, ...args.map(arg => arg instanceof Surface ? { stage: arg.log } : arg)]);
      } : undefined,
      set: (_, key, value) => { this.log.push(["set", key, value]); return true; }
    });
  }
  getContext(kind) { return kind === "2d" ? this.context : null; }
}
export const deferred = () => {
  let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
