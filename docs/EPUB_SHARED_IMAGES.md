# Shared images in EPUB books

Book export retains one image resource for equal decoded bytes with the same
media type, across all chapters. Different chapter-relative asset keys and
repeated figures can therefore share a single ZIP member and manifest item.
The first occurrence keeps its generated filename; distinct images retain their
chapter namespace. Fingerprints narrow comparisons, but exact byte equality is
required, including when equal-length payloads have colliding fingerprints.

The export-local pool owns each unique payload once. After preparing every
chapter it moves resources back to their first owning chapter, so the existing
ZIP writer and manifest builder each emit them once. Every referring chapter
keeps its own SVG/MathML properties. There is no process-global cache, source-AST
mutation, filesystem access, or network fetch.

For packaged images, only img/src attributes are rewritten. Prose, code,
links, alt text, other attributes and quoted tag-shaped text retain their bytes. Raw image bytes enter
the publication identifier before rewriting, so changing a shared illustration
still changes the content-derived edition identity.

The existing 4096-resource and 128-MiB book-image limits count unique payloads,
not the number of references. Per-chapter image extraction and per-image limits
remain in force. Single-document EPUB export is unchanged.

Verification targets:

```sh
cargo test --lib epub::book
cargo test --test epub_test
cargo clippy --all-targets -- -D warnings
```

Eight new regressions cover reuse, forced fingerprint collisions, MIME isolation,
full admission budgets, precise attribute rewriting, book identity, source
immutability, chapter metadata and the actual ZIP member inventory. Rust tests
were not executed in the authoring environment: no Rust toolchain is installed.

## Publication size accounting

All book exports enforce the same 256-MiB uncompressed-content ceiling, with or
without host fonts. The count includes the actual chapter XHTML after image
externalization and font linking, navigation, package metadata, stylesheets,
container/mimetype records, unique images and font subsets. ZIP headers and
compression overhead are not part of this uncompressed-content budget.

One temporary HTML chapter is independently limited to 256 MiB. Its embedded
base64 copies do not accumulate against the retained publication budget: a
shared illustration contributes its payload once, plus every chapter's small
reference markup. This corrects both false refusal of image-reusing books and
the previous font-free path's omission of final metadata/output admission.
Neither the 256-MiB ceiling nor the separate image/font limits were increased.

Five additional regressions exercise exact acceptance and one-byte refusal
through the real book preparation path, with and without fonts; metadata-only
expansion; repeated images whose transient HTML exceeds their packaged content;
and overflow-safe, nonmutating budget refusal. They are authored, not executed.

## Unavailable images and XML text

An image whose bytes were not supplied is represented by a visible
`[Image unavailable: alternative text]` placeholder in single-document and book
EPUB exports. It keeps an accessible label, the original title and
`data-image-source` metadata. An enclosing hyperlink remains intact. Native book
receipts continue to report their existing missing/unsupported image warnings.
Provide `HtmlOptions.image_assets`, `BookRenderer::set_image`, or the native
`--pdf-image DEST=PATH` override to embed the illustration instead. Supplied
remote URL keys are supported and are never fetched by the render core.

This replaces broken image references in the archive. EPUB 3.3
[requires images inside the container](https://www.w3.org/TR/epub-33/#sec-resource-locations);
adding a `remote-resources` property would not make an externally hosted image
conforming. Ordinary web hyperlinks and resolved image resource bytes are
unchanged. This behavior does not validate arbitrary custom CSS or SVG payloads.

If emitted chapter text, navigation or metadata contains a character excluded
by the [XML 1.0 character range](https://www.w3.org/TR/xml/#charsets), export
returns `InvalidInput` identifying the archive member, code point and byte
offset. The source is not silently changed and no archive is returned. Native
render exits with code 70 before replacing an existing output. TAB, LF, CR,
legal Unicode text and escaped literal entity examples retain their behavior.
