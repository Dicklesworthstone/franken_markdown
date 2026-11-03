# Monochrome emoji in Flow

The optional `emoji-face` build capability lets the native Flow engine and its
browser sessions draw the existing curated Noto Emoji outlines. Common README
content such as `🚀`, `✅`, `⚠️`, `📦`, and `🎉` participates in real font
measurement, wrapping, text selection, search, and copy. The browser paints the
native glyph paths; it does not ask a browser text renderer or system emoji font
to choose the glyphs.

The feature uses the existing 234,392-byte bundled font. It adds no font asset,
dependency, network request, or runtime option. Native CLI builds already enable
`emoji-face`; a dedicated browser Flow package can opt in with
`--no-default-features --features wasm-flow,emoji-face`. Add `cjk-face` separately
when both optional repertoires are required. Rebuild the matching wasm-bindgen
package and consumers together. The ordinary lean profile keeps its smaller
font registry and explicitly refuses uncovered emoji.

## Actual faces and source clusters

Ordinary text keeps its current body, emphasis, code, symbol, and optional CJK
font precedence. A bare symbol such as `©`, `™`, or `↩` therefore keeps its
existing typography. A mapped standalone emoji without an earlier text glyph
uses the monochrome emoji face, including in headings, lists, quotes, code,
table cells, safe links, and text beside mathematics. Each glyph records that
actual font identity. `fonts::emoji_bytes()` exposes the compiled font bytes;
the session's existing `fontBytes` and `glyphOutlines` methods expose the exact
same face used in layout.

The engine uses actual font advances and vertical extents. Emoji can increase a
line's measured height; neighboring text and list markers retain the same
native baseline. This regular monochrome face does not synthesize bold, italic,
or colored emoji artwork.

One scalar is one source cluster. An admitted presentation sequence is one
cluster covering its base and selector in both UTF-8 and UTF-16 coordinates.
Wrapping and hit testing cannot split that cluster. Source edits and copying
retain the actual characters, including selectors, supplementary-plane emoji,
decomposed Latin accents, BOMs, and original CRLF line endings.

## Presentation selectors

The curated font contains scalar outlines and has no `cmap` format 14 or GSUB
sequence substitutions. Flow therefore applies a deliberately bounded
presentation policy to the 134 standardized Unicode 17 variation bases that
also exist in the curated font:

- U+FE0F (VS16, emoji presentation) selects the actual curated monochrome emoji
  face. For example, `©️` uses that face even though bare `©` uses body text.
- U+FE0E (VS15, text presentation) requires a mapped glyph in the selected body
  or code face, or the existing symbol face. For example, `©︎`, `™︎`, and `↩︎`
  use real text glyphs. If no such text glyph exists, the operation fails rather
  than treating the emoji glyph as a different presentation.

The selector consumes no independent glyph or width; it remains part of the
cluster's logical source. A selector is never removed from arbitrary input.
Isolated, repeated, or nonstandard selectors are errors. For example, the bare
rocket `🚀` is supported, while rocket plus VS16 is not a standardized variation
sequence and remains outside this profile.

The admitted variation bases come from the [Unicode 17 emoji variation
sequence data](https://www.unicode.org/Public/17.0.0/ucd/emoji/emoji-variation-sequences.txt),
pinned in the source with its SHA-256. Font coverage is checked again when
shaping; the list cannot invent a missing glyph.

## Limits

This is bounded standalone emoji support, not full Unicode emoji shaping.
ZWJ sequences, skin-tone modifiers, regional-indicator flags, keycaps, emoji
tag sequences, and uncovered codepoints still require a host shaper. The
bundled profile refuses them as a complete failed layout transaction. It never
substitutes disconnected component pictures for a joined emoji or silently
drops a modifier. General variation selectors, bidi and joining scripts, and
unsupported combining sequences retain their existing refusal policy.

The existing source, scalar, shaping-window, aggregate-work, and outline-command
budgets remain in force. Presentation selectors count toward the scalar budget.
Long content uses the existing safe whitespace/CJK work boundaries; this feature
does not invent general Unicode line breaking. A failed edit keeps the last
successful source, display, assets, and revision identities.

## Executable coverage

`tests/flow_emoji.rs` exercises the real bundled cmap and contours, font
precedence, metrics, presentation selection, source/UTF-16 clusters, narrow
wrapping, bounded continuation, rich Markdown blocks, mixed Latin accents and
CJK, actual browser-wire outlines, source edits, copy, and rollback. A separate
feature-disabled test ensures the lean profile gains neither font bytes nor
emoji presentation capability. Set `FMD_FLOW_EMOJI_FIXTURE` when running the
native test to emit current snapshot, viewport, reading, glyph-outline, hit,
and selection records for comparison with a generated browser package.
