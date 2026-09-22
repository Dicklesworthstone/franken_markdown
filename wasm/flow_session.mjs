// Pure JS boundary over the generated FmdFlowSession. No renderer or fake
// Markdown implementation lives here; the separate tests inject a transport
// double to verify validation, fencing and disposal without claiming WASM proof.
export const FLOW_SOURCE_LIMIT = 4 * 1024 * 1024;
export const FLOW_ASSET_LIMIT = 8 * 1024 * 1024;
export const FLOW_EDIT_LIMIT = 4096;
const U64_MAX = 18446744073709551615n;
const DEFAULT_LAYOUT = Object.freeze({
  viewportWidth: 800,
  bodySize: 14,
  codeSize: 13,
  lineHeight: 20,
});

export class FlowError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = "FlowError";
    this.code = code;
  }
}
const fail = (code, message) => {
  throw new FlowError(code, message);
};
export function normalizeFlowError(error) {
  if (error instanceof FlowError) return error;
  if (typeof error === "string" && error.length <= 4096) {
    try {
      const detail = JSON.parse(error);
      if (detail && typeof detail.code === "string" && typeof detail.message === "string") {
        return new FlowError(detail.code, detail.message, { cause: error });
      }
    } catch {
      /* A generated binding or a trap may throw a non-JSON diagnostic. */
    }
  }
  return new FlowError(
    "WASM_ERROR",
    error instanceof Error ? error.message : "WASM flow operation failed",
    { cause: error },
  );
}
function record(value, allowed, name) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    fail("INVALID_OPTIONS", `${name} must be an object`);
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) fail("INVALID_OPTIONS", `unknown ${name} field: ${key}`);
  }
  return value;
}
export function identity(value, name = "identity") {
  if (typeof value === "bigint") {
    if (value < 0n || value > U64_MAX) fail("INVALID_IDENTITY", `${name} is outside u64`);
    return value.toString();
  }
  if (
    typeof value !== "string" ||
    !/^(0|[1-9][0-9]{0,19})$/.test(value) ||
    BigInt(value) > U64_MAX
  ) {
    fail(
      "INVALID_IDENTITY",
      `${name} must be a canonical u64 decimal string or bigint, never Number`,
    );
  }
  return value;
}
function integer(value, name, min = 0, max = 0xffffffff) {
  if (!Number.isInteger(value) || value < min || value > max)
    fail("INVALID_ARGUMENT", `${name} must be an integer in ${min}..=${max}`);
  return value;
}
function finite(value, name) {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    !Number.isFinite(Math.fround(value))
  ) {
    fail("INVALID_ARGUMENT", `${name} must be a finite f32 number`);
  }
  return Math.fround(value);
}
function boolean(value, name) {
  if (typeof value !== "boolean") fail("INVALID_ARGUMENT", `${name} must be a boolean`);
  return value;
}
// Reject malformed UTF-16 before wasm-bindgen's UTF-8 encoder could silently
// replace unpaired surrogates. Count UTF-8 bytes without allocating another copy.
export function sourceText(value, name = "source") {
  if (typeof value !== "string") fail("INVALID_ARGUMENT", `${name} must be a string`);
  if (value.length > FLOW_SOURCE_LIMIT)
    fail("BUDGET_EXCEEDED", `${name} exceeds the 4 MiB source limit`);
  let bytes = 0;
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = value.charCodeAt(i + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        fail("INVALID_UNICODE", `${name} contains an unpaired high surrogate`);
      i++;
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      fail("INVALID_UNICODE", `${name} contains an unpaired low surrogate`);
    } else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
    if (bytes > FLOW_SOURCE_LIMIT)
      fail("BUDGET_EXCEEDED", `${name} exceeds the 4 MiB source limit`);
  }
  return value;
}
export function layoutOptions(value = {}, defaults = DEFAULT_LAYOUT) {
  record(value, Object.keys(DEFAULT_LAYOUT), "layout");
  const next = {};
  for (const key of Object.keys(DEFAULT_LAYOUT)) {
    next[key] = finite(value[key] === undefined ? defaults[key] : value[key], key);
    if (next[key] <= 0 || next[key] > 1000000)
      fail("INVALID_OPTIONS", `${key} must be positive and at most 1000000`);
  }
  if (next.lineHeight < Math.max(next.bodySize, next.codeSize))
    fail("INVALID_OPTIONS", "lineHeight must accommodate bodySize and codeSize");
  return Object.freeze(next);
}
export function validateCreation(source, options = {}) {
  record(options, ["font", ...Object.keys(DEFAULT_LAYOUT)], "creation options");
  const { font = "sans", ...layout } = options;
  if (font !== "sans" && font !== "serif") fail("INVALID_FONT", "font must be sans or serif");
  return { source: sourceText(source), font, layout: layoutOptions(layout) };
}
function textRange(start, end) {
  integer(start, "start");
  integer(end, "end");
  if (start > end) fail("INVALID_SELECTION", "selection start exceeds end");
}
// Preserve caller order. Native code validates overlap and source boundaries
// against the captured revision; never degrade an atomic batch into many edits.
function packEdits(value) {
  if (!Array.isArray(value)) fail("INVALID_ARGUMENT", "edits must be an array");
  const count = value.length;
  if (count > FLOW_EDIT_LIMIT)
    fail("BUDGET_EXCEEDED", `edit batch exceeds ${FLOW_EDIT_LIMIT} operations`);
  const ranges = new Uint32Array(count * 2);
  const lengths = new Uint32Array(count);
  const replacements = new Array(count);
  const encoder = new TextEncoder();
  let totalBytes = 0;
  for (let i = 0; i < count; i++) {
    const { start, end, replacement } = record(
      value[i], ["start", "end", "replacement"], `edit ${i}`,
    );
    textRange(start, end);
    sourceText(replacement, `edit ${i} replacement`);
    const length = encoder.encode(replacement).length;
    totalBytes += length;
    if (totalBytes > FLOW_SOURCE_LIMIT)
      fail("BUDGET_EXCEEDED", "combined edit replacements exceed the 4 MiB source limit");
    ranges[i * 2] = start;
    ranges[i * 2 + 1] = end;
    lengths[i] = length;
    replacements[i] = replacement;
  }
  return { ranges, lengths, replacements: replacements.join("") };
}
function token(value) {
  if (!value || typeof value !== "object")
    fail("INVALID_IDENTITY", "a source/layout revision token is required");
  return Object.freeze({
    revision: identity(value.revision, "revision"),
    layoutRevision: identity(value.layoutRevision, "layoutRevision"),
  });
}
function bytes(value, max = FLOW_ASSET_LIMIT) {
  if (
    !ArrayBuffer.isView(value) ||
    Object.prototype.toString.call(value) !== "[object Uint8Array]"
  ) {
    fail("INVALID_ARGUMENT", "asset bytes must be a Uint8Array");
  }
  if (value.byteLength > max) fail("BUDGET_EXCEEDED", "asset bytes exceed the 8 MiB payload limit");
  // Snapshot shared buffers before native ingress is intentionally unsupported.
  if (Object.prototype.toString.call(value.buffer) === "[object SharedArrayBuffer]") {
    fail("INVALID_ARGUMENT", "copy shared asset bytes into an owned Uint8Array first");
  }
  return value;
}
function response(value, expected) {
  if (typeof value !== "string" || value.length > 16 * 1024 * 1024)
    fail("INVALID_WASM_RESPONSE", "flow response exceeds the JSON limit");
  let parsed;
  try {
    parsed = JSON.parse(value);
  } catch {
    fail("INVALID_WASM_RESPONSE", "flow response is not JSON");
  }
  if (
    !parsed ||
    parsed.schemaVersion !== 1 ||
    typeof parsed.revision !== "string" ||
    typeof parsed.layoutRevision !== "string"
  ) {
    fail("INVALID_WASM_RESPONSE", "unsupported flow response schema");
  }
  identity(parsed.revision);
  identity(parsed.layoutRevision);
  if (parsed.revision !== expected.revision || parsed.layoutRevision !== expected.layoutRevision) {
    fail("STALE_LAYOUT", "flow response does not match the requested source/layout snapshot");
  }
  return parsed;
}
function checkedPage(value, key, offset, limit) {
  if (
    !Number.isInteger(value.total) ||
    value.total < offset ||
    value.total > 0xffffffff ||
    value.offset !== offset ||
    !Array.isArray(value[key]) ||
    value[key].length !== Math.min(limit, value.total - offset)
  ) {
    fail("INVALID_WASM_RESPONSE", "inconsistent flow page inventory");
  }
  const end = offset + value[key].length;
  if (value.nextOffset !== (end < value.total ? end : null))
    fail("INVALID_WASM_RESPONSE", "flow pagination did not advance consistently");
  return value;
}

// Shared ingress/response checks for direct, worker and Canvas viewport reads.
// Rectangles follow the existing native f32 coordinate contract. Preserve the
// original inventory cursor: visible pages are deliberately not dense pages.
function viewportRect(value) {
  record(value, ["x", "y", "width", "height"], "viewport");
  const out = {};
  for (const key of ["x", "y", "width", "height"]) {
    if (typeof value[key] !== "number" || Math.abs(value[key]) > 1e12)
      fail("INVALID_ARGUMENT", "viewport coordinate exceeds its bound");
    out[key] = finite(value[key], key);
  }
  if (value.width < 0 || value.height < 0)
    fail("INVALID_ARGUMENT", "viewport extents must be nonnegative");
  finite(out.x + out.width, "right");
  finite(out.y + out.height, "bottom");
  return Object.freeze(out);
}
export function viewportOptions(value) {
  record(value, ["viewport", "afterIndex", "limit", "glyphs", "token"], "viewport options");
  const out = {
    viewport: viewportRect(value.viewport),
    afterIndex: integer(value.afterIndex ?? 0, "afterIndex"),
    limit: integer(value.limit ?? 512, "limit", 1, 2048),
    glyphs: boolean(value.glyphs ?? false, "glyphs"),
  };
  if (value.token !== undefined) out.token = token(value.token);
  return out;
}
export function validateViewportPage(value, options, expected) {
  const query = viewportOptions(options);
  const wanted = token(expected);
  try {
    if (
      !value ||
      value.schemaVersion !== 1 ||
      value.queryKind !== "viewport-v1" ||
      value.shapingProfile !== "bundled-simple-ltr" ||
      value.revision !== wanted.revision ||
      value.layoutRevision !== wanted.layoutRevision ||
      value.afterIndex !== query.afterIndex ||
      !Number.isInteger(value.total) ||
      value.total < query.afterIndex ||
      value.total > 1000000 ||
      !Array.isArray(value.items) ||
      value.items.length > query.limit ||
      !Number.isInteger(value.visitedEntries) ||
      value.visitedEntries < value.items.length ||
      value.visitedEntries > value.total
    )
      throw new Error("invalid viewport inventory");
    const echoed = viewportRect(value.viewport);
    if (Object.keys(echoed).some((key) => echoed[key] !== query.viewport[key]))
      throw new Error("viewport changed");
    viewportRect(value.totalBounds);
    let previous = query.afterIndex - 1;
    for (const item of value.items) {
      if (
        !item ||
        !Number.isInteger(item.index) ||
        item.index <= previous ||
        item.index >= value.total ||
        !["text", "vector", "image", "anchor"].includes(item.kind)
      )
        throw new Error("invalid viewport item");
      viewportRect(item.bounds);
      viewportRect(item.effectiveClip);
      previous = item.index;
    }
    if (
      value.nextIndex !== null &&
      (value.items.length !== query.limit ||
        value.nextIndex !== previous + 1 ||
        value.nextIndex >= value.total)
    )
      throw new Error("invalid viewport cursor");
  } catch (error) {
    throw new FlowError("INVALID_WASM_RESPONSE", "inconsistent indexed viewport page", {
      cause: error,
    });
  }
  return value;
}

// Canvas queries cover the original double-precision viewport conservatively.
// Round outward once here, then viewportOptions performs idempotent f32 ingress.
// Ink is still clipped to the ORIGINAL camera rectangle by the presentation layer.
export function coveringViewport(value) {
  viewportRect(value);
  const bits = new DataView(new ArrayBuffer(4));
  const round = (number, up) => {
    const rounded = Math.fround(number);
    if (up ? rounded >= number : rounded <= number) return rounded;
    if (rounded === 0) return up ? 2 ** -149 : -(2 ** -149);
    bits.setFloat32(0, rounded);
    bits.setUint32(0, bits.getUint32(0) + (rounded > 0 === up ? 1 : -1));
    return bits.getFloat32(0);
  };
  const axis = (start, extent) => {
    const left = round(start, false);
    return [left, extent === 0 ? 0 : round(round(start + extent, true) - left, true)];
  };
  const [x, width] = axis(value.x, value.width);
  const [y, height] = axis(value.y, value.height);
  return viewportRect({ x, y, width, height });
}

export function createFlowAdapter(raw, initialLayout = DEFAULT_LAYOUT) {
  let backend = raw;
  let layout = layoutOptions(initialLayout);
  const alive = () => {
    if (backend === null) fail("SESSION_DISPOSED", "flow session has been disposed");
  };
  const call = (f) => {
    alive();
    try {
      return f(backend);
    } catch (error) {
      throw normalizeFlowError(error);
    }
  };
  const current = () =>
    call((b) => {
      if (typeof b.revision !== "string" || typeof b.layoutRevision !== "string")
        fail("INVALID_WASM_RESPONSE", "WASM returned non-string revision IDs");
      return token({ revision: b.revision, layoutRevision: b.layoutRevision });
    });
  const fence = (value) => {
    const expected = token(value);
    const actual = current();
    if (expected.revision !== actual.revision)
      fail("STALE_REVISION", "document revision changed since this operation was prepared");
    if (expected.layoutRevision !== actual.layoutRevision)
      fail("STALE_LAYOUT", "layout revision changed since this operation was prepared");
    return expected;
  };
  const editOptions = (value) => {
    record(value, ["expectedRevision", "reuseAssets"], "edit options");
    return {
      revision: identity(value.expectedRevision, "expectedRevision"),
      reuse: boolean(value.reuseAssets ?? false, "reuseAssets"),
    };
  };
  const page = (kind, value = {}) => {
    alive();
    record(
      value,
      ["offset", "limit", "token", ...(kind === "snapshot" ? ["glyphs"] : [])],
      "page options",
    );
    const offset = integer(value.offset ?? 0, "offset");
    const limit = integer(value.limit ?? 512, "limit", 1, 2048);
    const expected = value.token === undefined ? current() : fence(value.token);
    const data = call((b) =>
      kind === "snapshot"
        ? b.snapshotJson(
            expected.revision,
            expected.layoutRevision,
            offset,
            limit,
            boolean(value.glyphs ?? false, "glyphs"),
          )
        : kind === "reading"
          ? b.readingJson(expected.revision, expected.layoutRevision, offset, limit)
          : b.pendingAssetsJson(expected.revision, offset, limit),
    );
    return checkedPage(
      response(data, expected),
      kind === "snapshot" ? "items" : kind === "reading" ? "nodes" : "requests",
      offset,
      limit,
    );
  };
  const edit = (method, start, end, replacement, options) => {
    alive();
    textRange(start, end);
    const opts = editOptions(options);
    sourceText(replacement, "replacement");
    call((b) => b[method](opts.revision, start, end, replacement, opts.reuse));
    return current();
  };
  const api = {
    get disposed() {
      return backend === null;
    },
    get supportsViewport() {
      return call((b) => typeof b.viewportJson === "function");
    },
    get supportsEditBatches() {
      return call((b) => typeof b.editManyUtf16Packed === "function");
    },
    get revision() {
      return current().revision;
    },
    get layoutRevision() {
      return current().layoutRevision;
    },
    get token() {
      return current();
    },
    get source() {
      return call((b) => b.source);
    },
    get layoutOptions() {
      alive();
      return { ...layout };
    },
    edit(startUtf16, endUtf16, replacement, options) {
      return edit("editUtf16", startUtf16, endUtf16, replacement, options);
    },
    editBytes(startByte, endByte, replacement, options) {
      return edit("editBytes", startByte, endByte, replacement, options);
    },
    editMany(edits, options) {
      alive();
      const opts = editOptions(options);
      if (opts.revision !== current().revision)
        fail("STALE_REVISION", "document revision changed since this batch was prepared");
      const packed = packEdits(edits);
      call((b) => {
        if (typeof b.editManyUtf16Packed !== "function")
          fail("UNSUPPORTED_WASM_PACKAGE", "this native package does not expose atomic edit batches");
        b.editManyUtf16Packed(
          opts.revision, packed.ranges, packed.lengths, packed.replacements, opts.reuse,
        );
      });
      return current();
    },
    replaceSource(source, options) {
      alive();
      const opts = editOptions(options);
      sourceText(source);
      call((b) => b.replaceSource(opts.revision, source, opts.reuse));
      return current();
    },
    reflow(options, expectedToken) {
      const expected = fence(expectedToken);
      const next = layoutOptions(options, layout);
      call((b) =>
        b.reflow(
          expected.revision,
          expected.layoutRevision,
          next.viewportWidth,
          next.bodySize,
          next.codeSize,
          next.lineHeight,
        ),
      );
      // Do not publish new JS options until the Rust transaction succeeds.
      layout = next;
      return current();
    },
    provideAsset(result) {
      alive();
      record(result, ["requestId", "generation", "width", "height", "bytes"], "asset result");
      const requestId = identity(result.requestId, "requestId");
      const generation = identity(result.generation, "generation");
      const width = integer(result.width, "width", 1, 32768);
      const height = integer(result.height, "height", 1, 32768);
      const payload = result.bytes === undefined ? undefined : bytes(result.bytes);
      call((b) => b.provideAsset(requestId, generation, width, height, payload));
      return current();
    },
    reloadAssets(expectedRevision) {
      call((b) => b.reloadAssets(identity(expectedRevision, "expectedRevision")));
      return current();
    },
    snapshot(options) {
      return page("snapshot", options);
    },
    viewport(options) {
      alive();
      const query = viewportOptions(options);
      const expected = query.token === undefined ? current() : fence(query.token);
      const { x, y, width, height } = query.viewport;
      const data = call((b) => {
        if (typeof b.viewportJson !== "function")
          fail(
            "UNSUPPORTED_WASM_PACKAGE",
            "this native package does not expose indexed viewport queries",
          );
        return b.viewportJson(
          expected.revision,
          expected.layoutRevision,
          x,
          y,
          width,
          height,
          query.afterIndex,
          query.limit,
          query.glyphs,
        );
      });
      return validateViewportPage(response(data, expected), query, expected);
    },
    readingOrder(options) {
      return page("reading", options);
    },
    pendingAssets(options) {
      return page("assets", options);
    },
    pages(options = {}) {
      alive();
      record(options, ["limit", "glyphs", "token"], "iterator options");
      // Capture now, not lazily at the first next(); never combine generations.
      const expected = options.token === undefined ? current() : fence(options.token);
      const limit = integer(options.limit ?? 512, "limit", 1, 2048);
      const glyphs = boolean(options.glyphs ?? false, "glyphs");
      return (function* () {
        let offset = 0;
        do {
          const value = page("snapshot", { offset, limit, glyphs, token: expected });
          yield value;
          offset = value.nextOffset;
        } while (offset !== null);
      })();
    },
    hitTest(x, y, expectedToken) {
      const expected = fence(expectedToken);
      const px = finite(x, "x"),
        py = finite(y, "y");
      return response(
        call((b) => b.hitTestJson(expected.revision, expected.layoutRevision, px, py)),
        expected,
      );
    },
    selectText(itemIndex, startUtf16, endUtf16, expectedToken) {
      const expected = fence(expectedToken);
      integer(itemIndex, "itemIndex");
      textRange(startUtf16, endUtf16);
      return response(
        call((b) =>
          b.selectItemJson(
            expected.revision,
            expected.layoutRevision,
            itemIndex,
            startUtf16,
            endUtf16,
          ),
        ),
        expected,
      );
    },
    copySource(startByte, endByte, expectedRevision) {
      alive();
      textRange(startByte, endByte);
      return call((b) =>
        b.copySource(identity(expectedRevision, "expectedRevision"), startByte, endByte),
      );
    },
    fontBytes(fontId) {
      return call((b) => bytes(b.fontBytes(identity(fontId, "fontId")), 32 * 1024 * 1024).slice());
    },
    assetBytes(requestId, expectedRevision) {
      const data = call((b) =>
        b.assetBytes(
          identity(requestId, "requestId"),
          identity(expectedRevision, "expectedRevision"),
        ),
      );
      return data === undefined || data === null ? null : bytes(data).slice();
    },
    dispose() {
      if (backend === null) return;
      const owned = backend;
      backend = null; // Mark closed even if the binding's destructor throws.
      try {
        owned.free();
      } catch (error) {
        throw normalizeFlowError(error);
      }
    },
  };
  current(); // Validate the generated class before handing ownership to callers.
  return Object.freeze(api);
}
