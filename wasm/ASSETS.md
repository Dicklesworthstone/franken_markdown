# Authorized images for measured flow

`./flow-assets` connects the flow engine's pending image requests to the Canvas
backend, with the same API for synchronous and worker sessions. Markdown does
not grant network authority. You supply an authorization/loading callback; this
module never fetches a Markdown destination, resolves a base URI, or sends credentials.
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
`retainSourceBytes: true` also supplies the original immutable encoded snapshot, so document export can resolve the image. Encoded delivery groups
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

The default decoder uses browser `createImageBitmap`. For admitted static SVG,
browsers that cannot decode an SVG Blob directly use a detached `Image` with a
local object URL, followed by bitmap extraction. That URL contains only the
canonical admitted preview, never a Markdown destination. The element, URL and
abort listener are released on success, error or cancellation. A worker without
an SVG-capable `createImageBitmap` or an `Image` implementation needs a host
decoder. No bundled image codec is introduced.
A host may provide an equivalent decoder that transfers an exclusively owned,
closeable Canvas image. Immutable Blob snapshots prevent later writes to loader
buffers from changing a pending decode. Shared and detached buffers are refused.

Before calling a decoder, a bounded container walk accepts static PNG, GIF87a/GIF89a,
and 8-bit
baseline/progressive grayscale, RGB, and Adobe CMYK/YCCK JPEG. Four-component
JPEGs require an unambiguous Adobe APP14 transform (`0` for CMYK or `2` for YCCK)
before the first scan. Component identities, sampling, coding tables and every
scan are checked; missing, truncated or contradictory color metadata is refused.
Encoded bytes are retained unchanged when requested for publication.

GIF previews show the first frame on its logical canvas, including local/global
palettes, interlacing, frame offsets and transparency. The container walk checks
every frame and extension through the final trailer before decoding. The decoder
receives an immutable first-frame prefix with a GIF trailer, so later animation
frames never enter its input. The original complete animation remains the source
for HTML, SVG and EPUB export; PDF uses its existing first-frame rendering.
Plain-text graphics and unknown extensions are refused. Comment and application
metadata are bounded by the same encoded-byte limit. Palette-index and compressed
pixel decoding remain the browser codec's responsibility; no additional image
codec or animation timer is introduced.

Dimensions, encoded bytes and aggregate
reserved pixels are checked before allocation; actual decoder dimensions must
match, allowing EXIF JPEG orientation to exchange width and height. WebP, APNG, unmarked four-component/lossless/arithmetic JPEG, delayed JPEG dimensions, and PNG
compressed ancillary metadata (`iCCP`, `zTXt`, `iTXt`) are explicitly refused.
The conservative PNG metadata restriction avoids decompression independent of
image pixel bounds. Re-encode such input as plain PNG or supported JPEG before
loading; extensions and a caller-supplied MIME label never override admission.
PNG chunk rules follow the W3C PNG specification; this walker is admission, not
CRC/entropy validation. Final compressed-data validation is the browser codec's
responsibility. A browser decode failure reports `INVALID_IMAGE` and leaves the
image request unresolved; it does not discard source or consume a request.
Codec vulnerabilities are not sandboxed by JavaScript limits.

### Static SVG previews

Authorized SVG files can supply Flow preview pixels and original native export
bytes through the same image request. Admission requires strict UTF-8 and a
bounded static XML subset: shapes, paths, groups, text, local use references,
gradients, clipping, masks and bounded local arrow markers. A missing root SVG namespace is supplied in the
preview; an explicitly different namespace is refused. References must resolve
to unique simple IDs inside this image. CSS presentation attributes and inline
styles are supported only for the static presentation properties.

Root width/height accept positive unitless or px, pt, pc, mm, cm and in lengths.
An explicit positive viewBox supplies missing or percentage root dimensions,
preserving its aspect ratio when only one dimension is specified. Raster sizes
round upward to whole CSS pixels. Only the canonical preview receives these
normalized dimensions; retained publication bytes remain exactly as authorized.

Arrow markers use local `<marker>` definitions with their original `viewBox`,
`markerUnits`, dimensions, reference point and orientation. Set `marker-start`,
`marker-mid` or `marker-end` directly on a path, line, polyline or polygon,
either as a presentation attribute or an inline style. Each reference must
resolve to a marker in the same image. `none` is also accepted. Group/use
inheritance, the `marker` shorthand and indirect values remain unsupported.
Marker-bearing shapes may contain text-only title/desc elements but no other
element descendants. Referenced definitions inside a descriptive element could
otherwise inherit marker properties without a counted edge. These restrictions
keep every possible placement countable.

Every marker instance charges its complete referenced subtree against the same
expanded node and content limits. Conservative counts include implicit repeated
path commands, compact numbers, close commands and all subpaths; they also
charge malformed suffixes before a browser might truncate them. Reused geometry
and nested markers multiply those costs, and cycles remain invalid. Complex
but valid drawings can be refused when this conservative bound is exhausted.

This profile refuses external URLs, nested images, scripts, event handlers,
foreign content, stylesheets, font downloads, animation, filters, patterns,
namespace rebinding, xml:base, DTDs and custom entities. CSS escapes, comments
and indirect functions such as var/calc/image-set are refused. Static local
references cannot contain cycles or exceed the scene expansion limits.
Patterns remain excluded because their tile replication is not bounded here.

SVG source and canonical preview each have a 2 MiB ceiling. The profile also
admits at most 8192 nodes, 64 nesting/reference levels, 4096 local references,
65536 expanded element nodes and 8 MiB of expanded text/attribute content.
Repeated references charge their complete subtree content, including long text
and path geometry. These structural limits complement the ordinary image pixel
budgets; they are not a precise browser memory or rendering-time guarantee.

The manager reserves both original SVG bytes and canonical preview bytes
against maxInFlightBytes before invoking any decoder. A host decoder receives
only the canonical preview and public dimension/MIME/signal context. With
retainSourceBytes enabled, native delivery receives the unchanged original,
including its source formatting and XML metadata. An unsupported SVG remains
an unresolved image request; it never acquires external resource authority.

Defaults (only lower values are accepted): 256 assets/attempts per generation,
4 concurrent loads, 8 MiB encoded bytes per image, 16 MiB in-flight immutable
encoded snapshots, including separate SVG preview bytes, 16,777,216 pixels per
image, 33,554,432 aggregate reserved and retained pixels, and 8,192 pixels per dimension. A rejected budget does not evict
images currently borrowed by a paint. The counters bound admitted structures,
not exact heap/GPU overhead, host-side loader allocations, or browser decoder
scratch buffers. The host must limit its own I/O *before* returning bytes.

## Verification

`node --test wasm/flow_assets.test.mjs wasm/flow_assets_batching.test.mjs wasm/flow_assets_reflow.test.mjs wasm/flow_raster.test.mjs wasm/flow_raster_cmyk.test.mjs wasm/flow_raster_gif.test.mjs wasm/flow_svg.test.mjs wasm/flow_svg_markers.test.mjs wasm/flow_assets_svg.test.mjs` exercises the
production admission/ownership code with explicitly synthetic containers,
bitmaps, and session doubles, including an asynchronous session. The CMYK/YCCK
suite also uses the real baseline/progressive JPEG fixtures shared with PDF
tests and checks every truncated prefix and malformed color metadata. It does not
claim real Rust/WASM or browser decoding. Marker admission tests cover direct
shape properties, target types, reference cycles, implicit-segment and nested
fanout, unchanged resource ceilings, original-byte retention and revocation.

`python wasm/tests/run_flow_assets.py --chromium /usr/bin/chromium` runs actual
PNG/JPEG/GIF/static SVG decoding, oriented dimensions, drawing and pixel assertions in Chromium,
including all six color patches in the four CMYK/YCCK fixtures and real animated
and interlaced GIF fixtures, without network or a server. GIF checks cover every
interlaced pixel, transparency, local palettes and offsets, one-frame decoder
input, and exact animation retention. Static SVG checks cover local use/shape
pixels, viewBox dimensions, original-byte retention and rejection before decoding
for external or active content. Its session is explicitly synthetic; Rust layout
and generated-WASM integration still require the package gates on a build host.

## Local-image live preview

The assembled `demo/flow-canvas.html` now includes an explicit local PNG/JPEG/GIF/SVG
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

GIF framing follows the [GIF89a specification](https://www.w3.org/Graphics/GIF/spec-gif89a.txt).
For partially filled opaque frames, uncovered canvas pixels follow the browser
codec's background policy, which can differ from the native PDF renderer.
