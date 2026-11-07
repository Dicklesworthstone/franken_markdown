# PDF link destinations

External PDF links serialize their destinations as URI bytes. Unicode path,
query and fragment text is encoded as UTF-8 percent escapes, so a link to
`https://example.com/café?q=中文` becomes
`https://example.com/caf%C3%A9?q=%E4%B8%AD%E6%96%87` in the annotation action.
Spaces, control bytes and characters outside the URI alphabet are encoded too.
Existing percent escapes retain their spelling and are never double-encoded;
a stray percent sign becomes `%25`.

URI delimiters and reserved characters keep their meaning, including `?`, `#`,
`&`, IPv6 brackets and parentheses. Parentheses are escaped only for the PDF
literal-string container; readers receive the original URI punctuation.
No HTML entity escaping is applied to URI bytes. This does not perform IDNA
conversion, DNS lookup, resource fetching or external-link validation.

The writer emits PDF 1.7, whose
[URI action dictionary requires a 7-bit ASCII string (ISO 32000-1, Table 206)](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).
The separate PDF text-string encoder remains responsible for Unicode titles,
authors, outline labels and accessible text. Those values retain their Unicode
encoding rather than becoming percent-escaped URLs.

Same-document `#fragment` links retain the existing destination resolution and
are emitted as internal `/Dest` references. The safe-scheme policy is unchanged:
HTTP, HTTPS, mailto, tel and relative destinations are admitted; script, file
and data actions remain blocked. Unresolved internal targets do not acquire
external URI actions.

The shared annotation writer covers ordinary and PDF/A output, both page
emission modes, table links, images and SVG links. The native CLI and the
browser/WASM facade use this writer as well; existing generated browser
packages need rebuilding to include the change.

`tests/pdf_navigation_test.rs` inspects annotations through their actual xref
entries, decodes the emitted URI strings, and checks exact Unicode/escaped
targets, metadata preservation, reserved punctuation, malformed escapes,
internal destinations, safe-scheme exclusions and emitter parity. The native
WASM facade test is not generated wasm32 or browser execution.
