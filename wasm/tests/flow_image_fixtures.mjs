// Container-only fixtures; the mock decoder tests do NOT prove a browser codec.
export function png(width = 2, height = 3, extra = []) {
  const chunk = (kind, body) => {
    const out = new Uint8Array(body.length + 12);
    new DataView(out.buffer).setUint32(0, body.length);
    out.set(
      [...kind].map((c) => c.charCodeAt(0)),
      4,
    );
    out.set(body, 8);
    return out; // Zero CRCs intentionally; no production decoder is claimed.
  };
  const ihdr = new Uint8Array(13),
    view = new DataView(ihdr.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const chunks = [
    new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    ...extra.map(([name, data]) => chunk(name, data)),
    chunk("IDAT", new Uint8Array([1])),
    chunk("IEND", new Uint8Array()),
  ];
  const out = new Uint8Array(chunks.reduce((n, part) => n + part.length, 0));
  let offset = 0;
  for (const part of chunks) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}
export function jpeg(width = 2, height = 3, progressive = false) {
  return new Uint8Array([
    255,
    216,
    255,
    progressive ? 194 : 192,
    0,
    11,
    8,
    height >> 8,
    height & 255,
    width >> 8,
    width & 255,
    1,
    1,
    0x11,
    0,
    255,
    218,
    0,
    8,
    1,
    1,
    0,
    0,
    63,
    0,
    12,
    255,
    0,
    4,
    255,
    208,
    0,
    255,
    217,
  ]);
}
export function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export const tick = () => new Promise((resolve) => setImmediate(resolve));
export const image = (width = 2, height = 3) => ({
  width,
  height,
  closed: 0,
  close() {
    this.closed++;
  },
});
export class Session {
  disposed = false;
  revision = "1";
  layoutRevision = "1";
  writes = [];
  pages = [];
  requests = [];
  constructor(count = 1) {
    this.reset(count);
  }
  get token() {
    return { revision: this.revision, layoutRevision: this.layoutRevision };
  }
  reset(count) {
    this.requests = Array.from({ length: count }, (_, i) => ({
      id: String(i + 1),
      generation: this.revision,
      kind: "image",
      url: `${i + 1}.png`,
      altText: "image",
      enclosingSourceByteOffset: 0,
      estimatedWidth: 100,
      estimatedHeight: 100,
    }));
  }
  edit(count = 1) {
    this.revision = String(BigInt(this.revision) + 1n);
    this.reflow();
    this.reset(count);
  }
  reflow() {
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  pendingAssets({ offset, limit, token }) {
    if (token.revision !== this.revision || token.layoutRevision !== this.layoutRevision)
      throw Object.assign(new Error(), { code: "STALE_LAYOUT" });
    this.pages.push(offset);
    // Deliberately small pages exercise gathering a shrinking inventory.
    const requests = this.requests.slice(offset, offset + Math.min(limit, 2));
    const end = offset + requests.length;
    return {
      ...this.token,
      schemaVersion: 1,
      offset,
      total: this.requests.length,
      nextOffset: end < this.requests.length ? end : null,
      requests,
    };
  }
  provideAsset(result) {
    if (result.generation !== this.revision)
      throw Object.assign(new Error(), { code: "STALE_REVISION" });
    const index = this.requests.findIndex((r) => r.id === result.requestId);
    if (index < 0) throw Object.assign(new Error(), { code: "UNKNOWN_ASSET_REQUEST" });
    this.requests.splice(index, 1);
    this.writes.push(result);
    this.reflow();
    return this.token;
  }
}
