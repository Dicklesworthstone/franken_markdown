# Checksummed book-edition delivery

`createBookEditionBundle` in `wasm/demo/book_edition.mjs` packages already
rendered publication Blobs without rerendering, rewriting or recompressing them.
The native Rust publisher remains the only renderer. The helper is in the
existing packaged workbench module; it adds no dependency or WASM ABI change.

```js
import { createBookEditionBundle } from './demo/book_edition.mjs';
const delivery = await createBookEditionBundle([
  { format: 'pdf', blob: pdfBlob },
  { format: 'epub', blob: epubBlob },
  { format: 'site', blob: siteZipBlob },
], { signal, sourceLength });
// delivery.blob is the complete ZIP. Creating it does not save any file.
```

One, two or three unique formats are allowed. ZIP entries have fixed, flat
names, in PDF / EPUB / site order, followed by `manifest.json`. A subset omits
unrequested formats. ZIP uses STORE, a fixed 1980-01-01 DOS date and UTF-8 names.
Both the PDF and the two nested ZIP-based formats retain their exact input
bytes. Caller-supplied filenames and arbitrary archive paths are not accepted.

The `fmd-book-edition-v1` manifest records the captured publication's
`sourceLength` metric and each output's format, filename, media type, byte
length and lowercase SHA-256. It excludes raw Markdown, resource grants,
standalone font/image files, local paths, timestamps and object URLs. Published
files can of course contain the assets and metadata embedded by the renderer.
This is a distribution bundle, **not** an editable project or portable backup.
Checksums detect byte changes; they are not signatures or proof of authorship.

All metadata and the complete 128 MiB aggregate publication limit are checked
before any payload is read or hashed. The final archive has an additional
8 KiB allowance for the manifest and ZIP headers. One output is read at a time;
archive assembly uses immutable Blob parts instead of a concatenated output
buffer. CRC processing yields every 256 KiB. Abort settles outstanding waits
without waiting for an uncooperative digest; underlying Web Crypto work may
finish later and its result is ignored. SHA-256 requires Web Crypto in a secure
context. A missing crypto capability is an explicit `CRYPTO_UNAVAILABLE` error.

## Validation

Run `node --test wasm/tests/book_edition_bundle.test.mjs`. The test inputs are
opaque binary fixtures, not substitutes for the native renderer. The tests use
an independent ZIP central-directory reader, a separate bitwise CRC and Node
SHA-256 to verify archive structure, exact payloads, receipts, deterministic
ordering, subsets, input ownership, limits and cancellation. Native rendering
and real-browser acceptance are separate checks.
