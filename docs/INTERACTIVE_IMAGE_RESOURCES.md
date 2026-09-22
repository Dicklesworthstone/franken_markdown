# Images in editable HTML workspaces

Interactive HTML now retains `HtmlOptions.image_assets` beyond the initial
native-rendered preview. The native HTML resource resolver encodes each unique,
trimmed destination into a JSON data block. Its image recognition, SVG
sanitization and duplicate-selection behavior are reused, not reimplemented.
The initial preview is unchanged. Unused supplied images remain available for
later edits, including after saving and reopening a workspace.

The browser controller reads these bindings once into a document-local `Map`.
Live Markdown image references resolve their decoded destination against it,
including images in reference definitions, lists, tables and footnotes.
Resolved bytes are emitted only as escaped `img` data-URI attributes, never as
injected SVG markup. Prototype-like destination names are ordinary map keys.
Damaged saved bindings do not fall back to a remote resource.

Canonical base64 `data:image/` source images (PNG, JPEG, GIF, WebP and SVG) also
survive live editing. This does not enable data URLs in links or autolinks.
The browser still determines whether the encoded bytes form a decodable image;
the URL admission check is not an image integrity validator.

## Saving and portability

**Save HTML** retains source, preview, editor runtime and image bindings through
repeated save/reopen cycles. Removing the final reference does not delete a
supplied image, so exact undo and later reinsertion remain possible. Print uses
the same resource-aware synchronous refresh as saving HTML.

**Save Markdown** remains a byte-preserving source download. It does not replace
image destinations or package separately supplied assets. Use the HTML workspace
to carry those explicit resources, or provide them again when rendering the
Markdown elsewhere. Images already written as data URIs remain in the source.

This is not a network sandbox. Unbound image destinations retain the existing
HTML URL behavior, and explicit caller-enabled raw HTML has its existing trust
contract. Supplied bindings themselves need no filesystem or network lookup.

## Regression checks

`node --test tests/interactive_images.test.mjs` exercises the shipped JavaScript
renderer with actual image bindings and data URLs. The normal Rust runtime test
entrypoint includes this suite alongside the existing renderer/controller tests.
`src/interactive/assets.rs` tests native resource-encoder parity, unused assets,
duplicate keys, unsupported bytes and script-data escaping.

For a native-generated interactive document containing at least one supplied
image, run `python tests/support/interactive_images_browser_check.py document.html`
on a verification host with Playwright and Chromium. It checks actual downloads,
image decoding, pending-edit print refresh, repeated reopening, source identity,
unused resources and application-selector collisions, and reports network/runtime
errors. Browser fixture execution is distinct from Rust/WASM compilation.
