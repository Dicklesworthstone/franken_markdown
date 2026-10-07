# On-demand book chapter HTML

`BookRenderer::render_chapter_html(index)` renders one zero-based chapter using
its existing parsed AST. `BookWorkspace` exposes the same method through Deref.
The result retains the site's chapter-relative links/images, title, frontmatter
language, stylesheet, fonts and shared navigation. It does not build a site ZIP,
render other chapters, or generate a whole-book search index.

`render_chapter_preview(index)` returns bounded UTF-8 JSON:

```json
{"schema":"fmd-book-chapter-preview-v1","selected":0,"pages":[{"path":"intro.html","source":"intro.md","title":"Introduction"}],"html":"<!doctype html>..."}
```

The map includes every chapter in reading order; only the selected chapter has
HTML. Source/title metadata is limited to 4096 UTF-8 bytes per field, filenames
to the same portable 255-byte site contract, final HTML to 8 MiB, and serialized
JSON to 64 MiB. The book's existing 4096-chapter, source and asset budgets still
apply. Metadata is checked before rendering and JSON escape expansion is checked
before allocating each escaped field. The output ceiling is not a bound on the
HTML emitter's transient allocations. The existing options conversion may still
copy supplied assets; this does not claim asset-zero-copy native rendering.

The raw WASM `FmdBook.renderChapterPreview(index)` binding preserves JavaScript
numbers until validating integer/range semantics. It is additive: rebuild with
`wasm-book` to expose it. Existing PDF, EPUB and site APIs and output paths are
unchanged. The HTML remains untrusted host content, not a sanitized document;
preview hosts must retain their isolated reader/CSP/resource policy.

Native tests in `src/book/chapter_preview_tests.rs` compare selected HTML with
the actual compressed site ZIP member and cover chapter links/images/languages,
129-chapter maps, deterministic output, read-only behavior, exact message/page
boundaries, escaped JSON budgets, bad indexes/assets/metadata and include edits.
The browser module adds a numeric-admission regression. These tests were added
but not executed in the implementation environment: neither Cargo/Rust nor the
repository-required rch remote build service was available. Run the repository
Rust and WASM gates through rch before making native runtime or parity claims.
