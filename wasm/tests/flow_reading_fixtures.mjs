// Explicit wire/session doubles, not a Markdown parser or Rust execution proof.
export const box = (y = 0) => ({ x: 0, y, width: 300, height: 20 });
export function node(role = "paragraph", text = "text", extra = {}) {
  return { role, text, bounds: box(), enclosingSourceSpan: { startByte: 0, endByte: 10 }, children: [], ...extra };
}
export class ReadingSession {
  disposed = false; revision = "1"; layoutRevision = "1"; calls = [];
  constructor(roots = [node()]) { this.roots = roots; }
  get token() { return { revision: this.revision, layoutRevision: this.layoutRevision }; }
  reflow() { this.layoutRevision = String(BigInt(this.layoutRevision) + 1n); }
  edit() { this.revision = String(BigInt(this.revision) + 1n); this.reflow(); }
  readingOrder(options) {
    this.calls.push(options);
    const { offset, limit } = options, nodes = this.roots.slice(offset, offset + limit), end = offset + nodes.length;
    return { ...this.token, schemaVersion: 1, offset, total: this.roots.length, nextOffset: end < this.roots.length ? end : null, nodes };
  }
}
export const deferred = () => {
  let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
