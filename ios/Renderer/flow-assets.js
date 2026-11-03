import { assetFailure, decodeRaster, FlowAssetError, rasterInfo } from "./flow_raster.mjs";

export { FlowAssetError } from "./flow_raster.mjs";

const fail = (code, message) => {
  throw assetFailure(code, message);
};
const DEFAULTS = Object.freeze({
  maxConcurrentLoads: 4,
  maxAssets: 256,
  maxAssetBytes: 8 * 1024 * 1024,
  maxInFlightBytes: 16 * 1024 * 1024,
  maxImagePixels: 16777216,
  maxRetainedPixels: 33554432,
  maxDimension: 8192,
});
function configured(input = {}) {
  if (!input || typeof input !== "object" || Array.isArray(input))
    fail("INVALID_OPTIONS", "asset limits must be an object");
  const result = { ...DEFAULTS };
  for (const key of Object.keys(input)) {
    if (
      !Object.hasOwn(DEFAULTS, key) ||
      !Number.isSafeInteger(input[key]) ||
      input[key] < 1 ||
      input[key] > DEFAULTS[key]
    ) {
      fail("INVALID_OPTIONS", "asset limits may only lower the positive default limits");
    }
    result[key] = input[key];
  }
  return Object.freeze(result);
}
function wireId(value) {
  if (
    typeof value !== "string" ||
    !/^(0|[1-9][0-9]{0,19})$/.test(value) ||
    BigInt(value) > 18446744073709551615n
  ) {
    fail("ASSET_PROTOCOL_ERROR", "expected a canonical u64 string identity");
  }
  return value;
}
function token(value) {
  return Object.freeze({
    revision: wireId(value?.revision),
    layoutRevision: wireId(value?.layoutRevision),
  });
}
const same = (a, b) => a.revision === b.revision && a.layoutRevision === b.layoutRevision;
const close = (image) => {
  try {
    image?.close?.();
  } catch {
    /* Revoked even when a host destructor fails. */
  }
};
function request(value, revision) {
  if (
    !value ||
    wireId(value.generation) !== revision ||
    typeof value.kind !== "string" ||
    typeof value.url !== "string" ||
    value.url.length > 65536 ||
    typeof value.altText !== "string" ||
    value.altText.length > 65536 ||
    !Number.isSafeInteger(value.enclosingSourceByteOffset) ||
    value.enclosingSourceByteOffset < 0
  )
    fail("ASSET_PROTOCOL_ERROR", "invalid asset request");
  return Object.freeze({
    id: wireId(value.id),
    generation: revision,
    kind: value.kind,
    url: value.url,
    altText: value.altText,
    enclosingSourceByteOffset: value.enclosingSourceByteOffset,
    estimatedWidth: value.estimatedWidth,
    estimatedHeight: value.estimatedHeight,
  });
}

/** Session-scoped ownership of authorized raster images. There is no fetch,
 * URL resolution, global cache, worker termination or automatic retry here. */
export class FlowImageAssets {
  #session;
  #load;
  #decode;
  #onChange;
  #limits;
  #timeout;
  #retainSourceBytes;
  #revision = null;
  #epoch = 0;
  #disposed = false;
  #run = null;
  #images = new Map();
  #attempts = new Set();
  #pixels = 0;
  #bytes = 0;
  constructor(session, options) {
    if (
      !session ||
      typeof session.pendingAssets !== "function" ||
      typeof session.provideAsset !== "function"
    ) {
      fail("INVALID_OPTIONS", "a synchronous or worker flow session is required");
    }
    if (!options || typeof options.load !== "function")
      fail("INVALID_OPTIONS", "an explicit authorized image loader is required");
    for (const key of Object.keys(options)) {
      if (!["load", "decode", "onChange", "limits", "timeoutMs", "retainSourceBytes"].includes(key))
        fail("INVALID_OPTIONS", "unknown image-loader option");
    }
    if (options.decode !== undefined && typeof options.decode !== "function")
      fail("INVALID_OPTIONS", "decode must be a function");
    if (options.onChange !== undefined && typeof options.onChange !== "function")
      fail("INVALID_OPTIONS", "onChange must be a function");
    this.#retainSourceBytes =
      options.retainSourceBytes === undefined ? false : options.retainSourceBytes;
    if (typeof this.#retainSourceBytes !== "boolean")
      fail("INVALID_OPTIONS", "retainSourceBytes must be boolean");
    this.#timeout = options.timeoutMs ?? 30000;
    if (!Number.isSafeInteger(this.#timeout) || this.#timeout < 0 || this.#timeout > 2147483647)
      fail("INVALID_OPTIONS", "invalid image-load timeout");
    this.#limits = configured(options.limits);
    this.#session = session;
    this.#load = options.load;
    this.#decode = options.decode ?? decodeRaster;
    this.#onChange = options.onChange ?? (() => {});
    this.synchronize();
  }
  get disposed() {
    return this.#disposed;
  }
  // Remains true until physical work settles, even after a timeout/abort. A
  // host ignoring AbortSignal cannot bypass concurrency limits by retrying.
  get busy() {
    return this.#run !== null;
  }
  /** Wait for actual callbacks/decoders, not just the cancellable public wait. */
  whenIdle() {
    return this.#run?.settled ?? Promise.resolve();
  }
  get stats() {
    return Object.freeze({
      revision: this.#revision,
      images: this.#images.size,
      retainedPixels: [...this.#images.values()].reduce((n, image) => n + image.pixels, 0),
      reservedPixels: this.#pixels,
      inFlightBytes: this.#bytes,
      attempted: this.#attempts.size,
      busy: this.busy,
    });
  }
  #notify() {
    try {
      this.#onChange(this.stats);
    } catch {
      /* An acknowledged mutation cannot be rolled back by an observer. */
    }
  }
  #alive() {
    if (this.#disposed || this.#session.disposed)
      fail("SESSION_DISPOSED", "image assets or their session are disposed");
  }
  #drop() {
    for (const entry of this.#images.values()) {
      close(entry.image);
      this.#pixels -= entry.pixels;
    }
    this.#images.clear();
    this.#attempts.clear();
  }
  /** Discard old-generation images after a source edit. Reflows keep ownership. */
  synchronize() {
    this.#alive();
    const current = token(this.#session.token);
    if (current.revision !== this.#revision) {
      this.#epoch++;
      this.#run?.controller.abort(
        assetFailure("STALE_REVISION", "image source generation changed"),
      );
      this.#drop();
      this.#revision = current.revision;
    }
    return current;
  }
  /** Immediate authorization revocation. Clear the painter too. Reload native
   * assets (or recreate the session) before attempting this generation again. */
  clear() {
    if (this.#disposed) return;
    this.#epoch++;
    this.#run?.controller.abort(assetFailure("ASSET_REVOKED", "image authorization was revoked"));
    this.#drop();
    this.#notify();
  }
  dispose() {
    if (this.#disposed) return;
    this.clear();
    this.#disposed = true;
  }
  /** Borrow a bitmap for one paint. Never use a bitmap from another revision,
   * request destination, or a stale layout. The manager retains ownership. */
  resolveImage(image, expected) {
    const current = this.synchronize();
    if (!same(token(expected), current))
      fail("STALE_LAYOUT", "image lookup belongs to a stale frame");
    const entry = this.#images.get(wireId(image?.requestId));
    return image.isResolved === true && entry?.url === image.destination ? entry.image : null;
  }
  async loadPending(options = {}) {
    this.#alive();
    if (!options || typeof options !== "object" || Object.keys(options).some((k) => k !== "signal"))
      fail("INVALID_OPTIONS", "only signal is accepted for an image batch");
    const signal = options.signal;
    if (
      signal !== undefined &&
      (!signal ||
        typeof signal.aborted !== "boolean" ||
        typeof signal.addEventListener !== "function" ||
        typeof signal.removeEventListener !== "function")
    )
      fail("INVALID_OPTIONS", "signal must be an AbortSignal");
    if (signal?.aborted) fail("ABORTED", "image loading was aborted");
    const expected = this.synchronize();
    if (this.#run) fail("ASSET_BUSY", "the previous image batch has not physically settled");
    const run = {
      controller: new AbortController(),
      epoch: this.#epoch,
      expected,
      attempted: 0,
      ready: [],
      publishing: false,
      batching: false,
    };
    this.#run = run;
    const abort = () => run.controller.abort(assetFailure("ABORTED", "image loading was aborted"));
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) abort();
    const timer = this.#timeout
      ? setTimeout(
          () => run.controller.abort(assetFailure("ASSET_TIMEOUT", "image batch deadline expired")),
          this.#timeout,
        )
      : null;
    let cancel;
    const cancelled = new Promise((_, reject) => {
      cancel = () => reject(run.controller.signal.reason);
      run.controller.signal.addEventListener("abort", cancel, { once: true });
      if (run.controller.signal.aborted) cancel();
    });
    const work = this.#batch(run).finally(() => {
      if (timer !== null) clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      run.controller.signal.removeEventListener("abort", cancel);
      if (this.#run === run) this.#run = null;
      // One completion notification, not one invalidation/repaint per image.
      if (run.attempted || run.controller.signal.aborted) this.#notify();
    });
    run.settled = work.then(
      () => {},
      () => {},
    );
    // Race only the public wait, not the physical accounting/ownership above.
    return Promise.race([work, cancelled]);
  }
  #check(run, allowCancelled = false) {
    this.#alive();
    if (this.#epoch !== run.epoch || this.synchronize().revision !== run.expected.revision)
      fail("STALE_REVISION", "image result belongs to a revoked source generation");
    if (!allowCancelled && run.controller.signal.aborted) throw run.controller.signal.reason;
  }
  async #batch(run) {
    // Negotiate once, before granting loader authority. An unadvertised method
    // is not evidence of atomic completion support in an older/custom session.
    run.batching = this.#session.supportsAssetBatches === true;
    if (run.batching && typeof this.#session.provideAssets !== "function") {
      fail("ASSET_PROTOCOL_ERROR", "session advertises image batches without a batch method");
    }
    const requests = [],
      ids = new Set();
    let offset = 0,
      total = null;
    // Read the COMPLETE pending inventory before any mutation changes its page
    // offsets/layout revision. Never page through a shrinking pending list.
    do {
      this.#check(run);
      const page = await this.#session.pendingAssets({ offset, limit: 256, token: run.expected });
      this.#check(run);
      if (
        page?.schemaVersion !== 1 ||
        !same(token(page), run.expected) ||
        page.offset !== offset ||
        !Number.isSafeInteger(page.total) ||
        page.total < 0 ||
        page.total > this.#limits.maxAssets ||
        (total !== null && total !== page.total) ||
        !Array.isArray(page.requests) ||
        page.requests.length > 256 ||
        offset + page.requests.length > page.total ||
        (offset < page.total && !page.requests.length) ||
        page.nextOffset !==
          (offset + page.requests.length < page.total ? offset + page.requests.length : null)
      ) {
        fail("ASSET_PROTOCOL_ERROR", "inconsistent or oversized pending-image inventory");
      }
      total = page.total;
      for (const raw of page.requests) {
        const item = request(raw, run.expected.revision);
        if (ids.has(item.id)) fail("ASSET_PROTOCOL_ERROR", "duplicate image request identity");
        ids.add(item.id);
        requests.push(item);
      }
      offset = page.nextOffset;
    } while (offset !== null);
    if (
      this.#attempts.size + requests.filter((item) => !this.#attempts.has(item.id)).length >
      this.#limits.maxAssets
    )
      fail("ASSET_COUNT_LIMIT", "too many image attempts in this generation");
    const report = {
      revision: run.expected.revision,
      loaded: 0,
      failed: 0,
      skipped: 0,
      errors: [],
    };
    let cursor = 0;
    const lane = async () => {
      while (cursor < requests.length) {
        this.#check(run);
        const item = requests[cursor++];
        if (this.#attempts.has(item.id) || item.kind !== "image") {
          report.skipped++;
          continue;
        }
        this.#attempts.add(item.id);
        run.attempted++;
        try {
          if (await this.#one(run, item)) report.loaded++;
          else report.skipped++;
        } catch (error) {
          this.#check(run);
          report.failed++;
          report.errors.push(
            Object.freeze({
              requestId: item.id,
              code: error instanceof FlowAssetError ? error.code : "ASSET_LOAD_FAILED",
            }),
          );
        }
      }
    };
    // allSettled is essential: one stale/error lane must not release the batch
    // slot while another still owns a decoder, buffer, or unacknowledged write.
    const results = await Promise.allSettled(
      Array.from({ length: Math.min(requests.length, this.#limits.maxConcurrentLoads) }, lane),
    );
    for (const result of results) if (result.status === "rejected") throw result.reason;
    this.#check(run);
    report.errors.sort(
      (a, b) => a.requestId.length - b.requestId.length || a.requestId.localeCompare(b.requestId),
    );
    return Object.freeze({ ...report, errors: Object.freeze(report.errors) });
  }
  async #one(run, item) {
    let image = null,
      pixels = 0,
      bytes = 0;
    try {
      let input = await this.#load(
        item,
        Object.freeze({ signal: run.controller.signal, maxBytes: this.#limits.maxAssetBytes }),
      );
      this.#check(run);
      if (input === null) return false; // Explicitly declined authorization.
      const info = rasterInfo(input, this.#limits);
      if (input.byteLength > this.#limits.maxInFlightBytes - this.#bytes)
        fail("ASSET_BYTE_LIMIT", "concurrent encoded-image budget exceeded");
      if (info.pixels > this.#limits.maxRetainedPixels - this.#pixels)
        fail("ASSET_PIXEL_LIMIT", "aggregate decoded-image budget exceeded");
      bytes = input.byteLength;
      pixels = info.pixels;
      this.#bytes += bytes;
      this.#pixels += pixels;
      // Synchronous immutable snapshot of precisely this non-shared view. A
      // callback cannot mutate queued decode input after it returns its bytes.
      const blob = new Blob([input], { type: info.mime });
      input = null;
      image = await this.#decode(blob, Object.freeze({ ...info, signal: run.controller.signal }));
      this.#check(run);
      if (
        !image ||
        typeof image.close !== "function" ||
        !Number.isInteger(image.width) ||
        !Number.isInteger(image.height) ||
        !(
          (image.width === info.width && image.height === info.height) ||
          (info.mime === "image/jpeg" && image.width === info.height && image.height === info.width)
        )
      ) {
        fail("INVALID_IMAGE", "decoder returned inconsistent dimensions or no owned bitmap");
      }
      await this.#enqueue(run, {
        result: {
          requestId: item.id,
          generation: item.generation,
          width: image.width,
          height: image.height,
        },
        blob,
        accept: () => {
          this.#images.set(item.id, { image, pixels, url: item.url });
          image = null;
          pixels = 0;
        },
      });
      return true;
    } finally {
      close(image);
      this.#pixels -= pixels;
      this.#bytes -= bytes;
    }
  }
  #enqueue(run, entry) {
    const delivery = new Promise((resolve, reject) => {
      run.ready.push({ ...entry, resolve, reject });
    });
    if (!run.publishing) {
      run.publishing = true;
      // Coalesce ready decoder continuations, not an entire load wave. A fast
      // image never waits for a slow/hung sibling just to fill a transaction.
      queueMicrotask(() => {
        this.#flush(run).catch((error) => {
          // Defensive terminal settlement: no queued lane may lose its promise
          // and leave physical accounting permanently busy after an exception.
          for (const pending of run.ready.splice(0)) pending.reject(error);
          run.publishing = false;
        });
      });
    }
    return delivery;
  }
  async #flush(run) {
    try {
      while (run.ready.length) {
        const group = [];
        let bytes = 0;
        do {
          const next = run.ready[0],
            size = this.#retainSourceBytes ? next.blob.size : 0;
          // The loader lanes bound result count and retained bitmaps. Bound
          // copied native payloads separately to maxAssetBytes (8 MiB), leaving
          // metadata headroom in the default 16 MiB worker ingress budget.
          if (group.length && (!run.batching || size > this.#limits.maxAssetBytes - bytes)) break;
          bytes += size;
          group.push(run.ready.shift());
        } while (run.ready.length);
        try {
          this.#check(run);
          const results = [];
          for (const entry of group) {
            // Copy only this admitted group, from the SAME immutable snapshots
            // used for decoding. Dimension-only delivery allocates no payload.
            const payload = this.#retainSourceBytes
              ? new Uint8Array(await entry.blob.arrayBuffer())
              : undefined;
            this.#check(run); // Authorization can change while copying a Blob.
            results.push({ ...entry.result, ...(payload ? { bytes: payload } : {}) });
          }
          const before = token(this.#session.token);
          // Never pass cancellation to a dispatched worker mutation: it would
          // destroy the document. Never replay a rejected batch as singles.
          const ack = run.batching
            ? await this.#session.provideAssets(results)
            : await this.#session.provideAsset(results[0]);
          // Cancellation is not rollback. Retain a committed group's bitmaps
          // only while its source and authorization are still valid.
          this.#check(run, true);
          const accepted = token(ack),
            current = token(this.#session.token);
          if (
            accepted.revision !== current.revision ||
            BigInt(accepted.layoutRevision) <= BigInt(before.layoutRevision) ||
            BigInt(accepted.layoutRevision) > BigInt(current.layoutRevision)
          ) {
            fail("ASSET_PROTOCOL_ERROR", "image acknowledgment does not match session state");
          }
          for (const entry of group) entry.accept();
          for (const entry of group) entry.resolve();
        } catch (error) {
          for (const entry of group) entry.reject(error);
        }
      }
    } finally {
      run.publishing = false;
    }
  }
}
