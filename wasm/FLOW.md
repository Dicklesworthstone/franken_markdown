# Persistent flow/editor sessions

The `./flow` entry point connects JavaScript to the Rust transactional editor,
measured display lists, font-run cache, hit testing and asset lifecycle. It uses
the same initialized WASM instance as the HTML/PDF APIs. Rebuild the generated
`pkg/` alongside these wrappers: an older binary is rejected with
`UNSUPPORTED_WASM_PACKAGE`, not silently substituted with estimated layout.

```js
import { createFlowSession } from "@franken-suite/franken-markdown/flow";

const editor = await createFlowSession(
  "# Guide\n\n**Bold** and [a link](#guide).",
  { font: "sans", viewportWidth: 360 }
);
try {
  const before = editor.token;
  // These offsets are UTF-16 source indices, just like textarea.selectionStart.
  editor.edit(0, 0, "Introduction\n\n", {
    expectedRevision: before.revision
  });
  editor.reflow({ viewportWidth: 720 }, editor.token);

  // A page contains real glyph/cluster data, not HTML or a screenshot.
  const page = editor.snapshot({ limit: 512, glyphs: true });
  for (const item of page.items) {
    if (item.kind === "text" && item.fontRun) {
      // Install precisely the face identified by each actual glyph's fontId.
      const fontBytes = editor.fontBytes(item.fontRun.fontId);
      // Host drawing code consumes item.bounds and item.fontRun.glyphs.
    }
  }
  const hit = editor.hitTest(20, 30, page);
  // hit.linkTarget is data for the host's authorization/navigation layer.
  console.log(hit);
} finally {
  editor.dispose();
}
```

## Coordinates and optimistic transactions

`revision`, `layoutRevision`, request IDs and font IDs are **decimal strings**;
input identities also accept `bigint`. JavaScript `Number` identities are
rejected so values above 2^53 cannot round to another document or resource.

`edit` uses UTF-16 source indices. `editBytes` uses UTF-8 source bytes. Both
require `expectedRevision`; invalid boundaries, unpaired JS surrogates and
mid-surrogate selections are rejected. Failed edits keep the previous source,
asset state, display and revisions. Identical source replacements are no-ops.

`hitTest` and `selectText` require the token of the **display actually shown**.
A resize or asset completion changes `layoutRevision`, so an old click cannot
be interpreted against new geometry. Their text offsets are local to one
rendered text fragment. `enclosingSourceSpan` is a top-level Markdown envelope,
not an invented exact inline mapping. `copySource(startByte, endByte, revision)`
and `selectText(...).text` are deliberately separate copy operations.

## Paged drawing and accessibility output

`snapshot`, `readingOrder` and `pendingAssets` return schema-version-one pages
with `offset`, `total`, `nextOffset`, `revision` and `layoutRevision`. Drawing
items include text/glyph runs, image references, vector decorations and heading
or link anchors. Reading nodes retain table row/cell roles and hierarchy.

Pages default to 512 entries, accept at most 2048, and serialize at most 16 MiB.
Use a smaller page or `glyphs: false` when the serializer reports
`SNAPSHOT_TOO_LARGE`. `editor.pages({ limit, glyphs })` iterates drawing pages
under one captured token; an intervening edit or resize makes it throw instead
of mixing snapshots. Do not collect the entire iterator for a virtualized view.

## Explicit image ownership

The renderer never fetches images or follows links. Hosts validate destinations,
permissions, content and decoding before submitting a pending request:

```js
const pending = editor.pendingAssets({ limit: 2048 });
for (const request of pending.requests) {
  const decoded = await loadAuthorizedImage(request.url); // application policy
  editor.provideAsset({
    requestId: request.id,
    generation: request.generation,
    width: decoded.width,
    height: decoded.height,
    bytes: decoded.bytes // optional Uint8Array; omit for dimension-only results
  });
}
```

A source edit during that `await` makes the old result stale. Resizing does not
invalidate other outstanding requests. Invalid results remain pending. Reload
external resources with `reloadAssets(editor.revision)` when bytes, base URI or
authorization changes. Source edits discard assets by default; `reuseAssets:
true` explicitly attests that the host has verified the resource context and
bytes are unchanged. Retrieval through `assetBytes(id, revision)` is generation
checked; `null` means a known dimension-only result. Bytes returned to JS are
owned copies. Stop presenting old snapshots immediately when authorization is
revoked, including when a refresh transaction fails.

## Limits and scope

The JS facade checks a 4 MiB UTF-8 source limit and an 8 MiB individual asset
limit before WASM ingress. Rust additionally bounds source lines, AST nodes,
projection strings, request counts, 32 MiB aggregate assets, and shaping work.
The JavaScript facade rejects shared asset buffers; copy them to owned bytes.

After asynchronous creation, all operations are synchronous. Use a Web Worker
for off-main-thread editing; this API does not silently create one. Edits still
reparse complete bounded snapshots. The bundled adapter is the existing simple
left-to-right profile, not full multilingual/bidirectional shaping. Unsupported
scripts or sequences produce a structured `FlowError` while preserving the
last successful document. Use Rust `FlowSession` with a host shaper for broader
coverage. This entry point exposes drawing data, not a built-in Canvas/GPU UI.

`dispose()` is idempotent and releases the native session and retained cache.
All later operations except checking `disposed` fail with `SESSION_DISPOSED`.
Snapshot mutation in JavaScript cannot modify the Rust session.

## Executable checks

`node --test wasm/flow_session.test.mjs` exercises the JS transport contract with
an explicit test double; it is not proof of the Rust renderer. The separate
`wasm/flow_smoke.mjs` loads the **generated** class and checks rich layout, edits,
stale selections, image reflow, font data and fresh-session equivalence. It is
invoked by both `scripts/check-wasm-package.sh` and the DSR quality route
`scripts/dsr-wasm-package.sh`.
Type declarations are checked with `tsc --noEmit --strict --target ES2022
--module NodeNext --moduleResolution NodeNext wasm/flow_types_test.mts`.
