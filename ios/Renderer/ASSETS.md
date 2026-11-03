# Authorized images for measured flow

`./flow-assets` connects the flow engine's pending image requests to the Canvas
backend, with the same API for synchronous and worker sessions. Markdown does
not grant network authority. You supply an authorization/loading callback; this
module never calls `fetch`, resolves a base URI, opens a URL, or sends credentials.
It does not install a second Markdown parser or alter the Rust layout engine.

```js
import { FlowImageAssets } from "@franken-suite/franken-markdown/flow-assets";

// session is a FlowSession or WorkerFlowSession. The host populated this map
// from explicitly selected File objects; it is NOT populated from Markdown URLs.
const selectedFiles = new Map();
const images = new FlowImageAssets(session, {
  async load(request, { signal, maxBytes }) {
    const file = selectedFiles.get(request.url); // exact, explicitly authorized name
    if (!file) return null;
    if (file.size > maxBytes) throw new Error("Selected image is too large");
    signal.throwIfAborted();
    const bytes = new Uint8Array(await file.arrayBuffer());
    signal.throwIfAborted();
    return bytes;
  }
});
const report = await images.loadPending();
const frame = await painter.render(session, {
  width: 640, height: 480, token: session.token,
  resolveImage: (image, token) => images.resolveImage(image, token)
});
// On teardown: clear/cancel paints before closing images borrowed by a paint.
painter.clear();
images.dispose();
// The host still owns painter and session; dispose each when it is no longer used.
```

## Loading, layout, and ownership

The manager reads every pending-request page under one source/layout token
**before** supplying results. Providing an image removes it from the pending
inventory and changes layout; reading later pages during delivery would skip
requests. Loading/decoding uses up to four concurrent lanes. When the session
advertises `supportsAssetBatches`, ready decoder results coalesce into serialized
`provideAssets` calls: one native parse/reflow per ready group, not per image.
The first ready image does not wait for a slow loader to fill a group. Results
that finish during a native write join the next group, still holding their lane
and reservations until acknowledged. Legacy sessions retain serialized
`provideAsset` calls; support is never inferred from a method name alone.

Images can finish out of order without being assigned to another request. A
loader refusal or invalid decode remains a per-image outcome. A rejected native
group fails all its members without publishing a bitmap prefix, replaying single
writes or preventing other groups from completing. `loadPending()` is not one
whole-inventory atomic transaction. See `ASSET_BATCHES.md` for the lower-level
API and its native rollback contract.

By default, actual oriented dimensions are supplied as **dimension-only**
results, without duplicating encoded bytes in the worker/WASM asset store.
`retainSourceBytes: true` also supplies the same immutable encoded snapshot used
for decoding, so document export can resolve the image. Encoded delivery groups
are capped at `maxAssetBytes` in aggregate (default 8 MiB), as well as being
bounded by the loader lane count. Lower or occupied worker queue budgets and
native cumulative payload limits still apply; rejection is not silently retried.
The manager owns decoded bitmaps; `resolveImage` only lends them to a matching
current frame. Disposal, revocation, rejected layout and late stale results close
owned bitmaps. No automatic cross-request deduplication or global URL cache can
bypass per-request authorization. `session.assetBytes` remains null for default
dimension-only deliveries; export requires explicit encoded retention. Revoking
bitmaps does not revoke native bytes: reload native assets or recreate the
session before exporting after any authorization change.

A source edit invalidates image ownership; an ordinary reflow does not.
`synchronize()` makes that revocation immediate, and is also invoked by loading
and image lookup. Use the default `reuseAssets: false` on edits. Explicit native
asset reuse does not transfer authorizations/bitmaps to a new manager generation;
call `reloadAssets` before reloading such a document. Request generations and
source revisions remain canonical decimal strings, never JavaScript numbers.

## Cancellation is not rollback

`loadPending({ signal })` can stop waiting promptly, and a whole-batch deadline
defaults to 30 seconds. The signal is passed to the host loader/decoder, **not**
to the worker RPC. An in-flight worker cancellation would destroy the session.
If a native image delivery has already been dispatched, it may still succeed;
its bitmap is retained only if the source and authorization have not changed.
A timed-out/rejected public wait must not be interpreted as a rolled-back write.

Physical work keeps its lane and byte/pixel reservations until its callbacks
actually settle. During that interval `busy` remains true and another batch is
refused with `ASSET_BUSY`. `whenIdle()` waits for physical completion, not merely
public cancellation. A host callback that ignores cancellation and never returns
can keep that manager busy indefinitely; the module cannot kill arbitrary host
JavaScript. Disposal still revokes immediately and closes late returned bitmaps.

No failed/declined request is retried automatically within a generation. For a
changed authorization set, clear the painter, revoke the manager, reload native
assets, and start a new batch once old work settles:

```js
painter.clear(); // also supersedes pending paints
images.clear();
await session.reloadAssets(session.revision);
await images.whenIdle();
await images.loadPending();
```

Changing a token alone does not erase already painted pixels: `painter.clear()`
is essential on authorization revocation. `onChange(stats)` fires once when a
batch with attempted work physically settles (or aborts), and on revocation,
not once per image. A host can coalesce refreshes rather than flood its worker.
Observer exceptions cannot undo already acknowledged deliveries and are ignored.

## Decoder profile and budgets

The default decoder is browser `createImageBitmap`, not a bundled image codec.
A host may provide an equivalent decoder that transfers an exclusively owned,
closeable Canvas image. Immutable Blob snapshots prevent later writes to loader
buffers from changing a pending decode. Shared and detached buffers are refused.

Before calling a decoder, a bounded container walk accepts static PNG and 8-bit
baseline/progressive grayscale/RGB JPEG. Dimensions, encoded bytes and aggregate
reserved pixels are checked before allocation; actual decoder dimensions must
match, allowing EXIF JPEG orientation to exchange width and height. SVG, GIF,
WebP, animation, CMYK/lossless/arithmetic JPEG, delayed JPEG dimensions, and PNG
compressed ancillary metadata (`iCCP`, `zTXt`, `iTXt`) are explicitly refused.
The conservative PNG metadata restriction avoids decompression independent of
image pixel bounds. Re-encode such input as plain PNG or supported JPEG before
loading; extensions and a caller-supplied MIME label never override admission.
PNG chunk rules follow the W3C PNG specification; this walker is admission, not
CRC/entropy validation. Final compressed-data validation is the browser codec's
responsibility. Codec vulnerabilities are not sandboxed by JavaScript limits.

Defaults (only lower values are accepted): 256 assets/attempts per generation,
4 concurrent loads, 8 MiB encoded bytes per image, 16 MiB in-flight immutable
encoded snapshots, 16,777,216 pixels per image, 33,554,432 aggregate reserved and
retained pixels, and 8,192 pixels per dimension. A rejected budget does not evict
images currently borrowed by a paint. The counters bound admitted structures,
not exact heap/GPU overhead, host-side loader allocations, or browser decoder
scratch buffers. The host must limit its own I/O *before* returning bytes.

## Verification

`node --test wasm/flow_assets.test.mjs wasm/flow_assets_batching.test.mjs wasm/flow_assets_reflow.test.mjs wasm/flow_raster.test.mjs` exercises the
production admission/ownership code with explicitly synthetic containers,
bitmaps, and session doubles, including an asynchronous session. It does not
claim real Rust/WASM or browser decoding.

`python wasm/tests/run_flow_assets.py --chromium /usr/bin/chromium` runs actual
PNG/JPEG decoding, oriented dimensions, drawing and pixel assertions in Chromium,
without network or a server. Its session is explicitly synthetic; Rust layout
and generated-WASM integration still require the package gates on a build host.

## Local-image live preview

The assembled `demo/flow-canvas.html` now includes an explicit local PNG/JPEG
picker, an **Insert image references at cursor** action, and **Revoke all images**.
Files are authorized by exact name (or their explicitly constructed percent-encoded
alias); no directory traversal, arbitrary URL resolution or network fallback is
allowed. Filename collisions and ambiguous encoded aliases refuse the entire
new selection without changing the previous grant. The picker holds at most
128 files, 8 MiB per file and 32 MiB total; pixel/codec limits still apply later.

Text and image placeholders paint first. Image work runs independently, then the
controller coalesces an updated measured frame after the batch settles. It does
not wait for image I/O before accepting source edits. Ordinary viewport changes
reuse authorized bitmaps; successful source changes revoke old-generation images
and load the new requests. A resize may advance the current layout after an image
acknowledgment: the accepted delivery remains valid when its source generation is
unchanged and its layout revision lies within the acknowledged monotonic history.

Changing the selected files or clicking **Revoke all images** clears pixels before
closing old bitmaps and explicitly recreates the session. Repeated restarts cannot
multiply outstanding image work: the controller keeps one physical batch slot
across sessions until old callbacks really settle. A loader that never honors
cancellation cannot block text rendering in a new session, but new image loading
waits for its old physical slot rather than escaping the configured limits.

The controller's `whenIdle()` means preview-loop idle, not completion of optional
image I/O. `state.images` separately reports loading, results or an image error.
Individual image failures leave text usable and unresolved images as placeholders.
The local picker never uploads bytes. It is a demonstration of explicit host grants,
not a generic network loader. The demo explicitly retains admitted bytes for
its document-export path and automatically benefits from ready-image coalescing.

Additional checks:

```sh
node --test wasm/tests/flow_preview_images.test.mjs wasm/tests/local_image_sources.test.mjs
node --test wasm/flow_assets_package.test.mjs
```

The controller tests use the production controller and asset manager with explicit
session/bitmap doubles. Package tests prove exported imports and shipping inventory,
not that a generated WASM package has been built. Serve the matching built package
to exercise the complete live preview; no renderer fallback hides a missing build.
