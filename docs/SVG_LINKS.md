# Navigation in standalone SVG exports

All SVG entry points now retain Markdown hyperlinks. The painter records hit
regions from the exact final positions of prepared words, not by remeasuring
source strings. A source word carries its link identity through bold, italic,
code, shaping, optimal line breaking and emergency cluster wrapping. Inline
mathematics and resolved images carry the same identity and use their actual
layout height. Image alt-text fallbacks remain linked too.

Each contiguous fragment of a link on a physical line becomes a native SVG
`<a>` with a transparent hit rectangle. Adjacent styles and real spaces within
the link join; unlinked text, another destination, hard breaks and independent
table cells end the region. Regions are clipped to the poster and their edges
round outward to the existing 0.01-point grid. No new font measurement, glyph
scaling, underline geometry, JavaScript or `foreignObject` is introduced.

The anchor has `tabindex="0"` and an escaped accessible name made from its source
content. Optional Markdown titles become escaped `<title>` metadata. Neither
introduces visible `<text>` elements or requires view-time fonts. This is link
accessibility, not a claim of full document accessibility or conformance.

## Destinations and safety

Only explicit HTTP, HTTPS and mailto destinations, and fragments resolving to
exported headings, numbered notes or their citations, are activated. Scheme checks are case-insensitive. Plain
surrounding spaces are trimmed, but embedded whitespace, literal or encoded
controls, malformed escapes, backslashes, invalid Unicode and unsupported
schemes stay inert. Relative paths and protocol-relative URLs are intentionally
not active: a portable SVG has no trusted Markdown-source base URL. This is a
narrower policy than the HTML renderer, and no host URL resolver was added.

Fragment escapes are decoded once. They are looked up in the source destination registry;
raw fragments are never copied into an active `href`. In particular, a fragment
cannot target internal glyph/image definitions or inject an SVG view expression.
A malicious nested link supplied directly through the AST cannot inherit an
outer safe destination. Nested anchors are never emitted.

The renderer never fetches a link. Activating an external anchor is a viewer
navigation action in the current browsing context (`_self`), with
`rel="noopener noreferrer"`. Opening a poster merely renders its existing
resources; it does not follow these hyperlinks. Metadata is XML-escaped and
control/noncharacter text is replaced, not inserted as markup. Diagnostic
messages never echo rejected destinations or source content.

## Heading navigation

Forward and backward heading references resolve after complete document layout,
including headings in nested containers and prepared endnotes. Slugs match the
shared HTML spelling: lowercase Unicode letters and ASCII digits, collapsed separators for
spaces/hyphens/underscores, discarded punctuation and
`section` for an empty result. Inline styles, image alt text and math source are
included; footnote-reference markers are ignored. Suffixes start at `-2` and
skip all occupied names, including literal headings such as `Topic-2`.

Output identifiers are private `fmd-heading-N` names, separate from the `gN`
and `iN` definitions. Only referenced views are emitted. An empty `#` link
selects the `fmd-top` overview. Unknown headings leave their link content
visible but inactive; no fabricated or dangling active destination is emitted.

Navigation uses native SVG `<view>` elements. Their view boxes retain the
poster's original width and height but translate the origin to the heading's
laid-out top. This preserves scale; it is not HTML scrolling, pagination or a
newly rendered crop. A `#` link or browser history returns to the overview.
Interactive viewers can activate these anchors; rasterized exports and SVGs
embedded as inert images do not acquire an interactive browser surface.

## Footnote navigation

Footnotes remain complete numbered endnotes after the body. Their existing
superscript citations now link to the beginning of the corresponding note, and
each cited note's existing label links back to its first painted citation. No
extra return-arrow glyph or altered note typography is required. Ordinary
Markdown links such as `[Note](#fn-evidence)` and `[Citation](#fnref-1)` resolve
to those same actual positions. Forward references work because destination
resolution occurs after complete layout.

Numbering follows first reference in the body, then note-to-note references.
Each definition is emitted once, including cycles; unreferenced definitions
remain at the end and can be reached through `#fn-ID`. An unreferenced note has
no invented return destination. Duplicate definitions retain the existing
first-definition-wins policy, and undefined citations remain literal `[^id]`
text. Leading figures, code, tables, mathematics, headings and nested lists keep
their full block structure and dimensions.

The return position comes from actual painted citation geometry. Table
measurement, line-breaking probes and repeated citations cannot move it. A
citation inside an ordinary source link keeps that enclosing link's destination
and still provides the note's return position. Generated links use accessible
names such as `Footnote 1` and `Return to footnote 1 citation`, with the same
keyboard-focus and transparent hit-region behavior as ordinary links.

`PreparedEndnotes` is an additive core API for hosts needing this provenance.
Its immutable document is exactly the existing `Document::with_endnotes()`
transformation; a bounded sidecar identifies generated citation/label nodes and
note starts from their source structure. Superscript text, destination prefixes
and author-controlled IDs never establish generated identity. The wrapper
retains the source AST and borrows ordinary documents without note syntax.
Explicitly calling the legacy `with_endnotes()` before rendering retains its
old text-only note behavior because that API intentionally discards provenance.

Source aliases are literal: a note ID `%31` is addressed by `#fn-%2531`, whereas
`#fn-%31` addresses an ID `1`. Neither is decoded twice. If a heading and a note
or citation own the same source spelling, an ordinary link to that ambiguous
spelling stays inactive and reports `svg_link_unresolved`. Generated typed note
links continue to reach the correct object. Private output IDs
`fmd-note-N` and `fmd-note-ref-N` cannot be used as author-created aliases, just
as raw fragments cannot target glyph definitions.

The markup follows SVG 2's native links, named views and pointer-event regions:
https://www.w3.org/TR/SVG2/linking.html
https://www.w3.org/TR/SVG2/interact.html#PointerEventsProperty

## Work limits and diagnostics

A render retains at most 4,096 distinct link records, 4,096 named headings,
16,384 hit regions and 4 MiB of retained metadata string bytes (including both
copies of interned link keys). Destinations are limited to 4,096 input bytes.
Accessible names and titles retain at most 256 Unicode scalars each. Heading
names are admitted only when the full slug fits 1,024 bytes and the metadata
walk fits 64 KiB of source, 8,192 nodes and 128 levels. A heading exceeding a
limit stops subsequent name assignment to prevent duplicate renumbering.

Endnote preparation separately admits up to 4,096 numbered note identities,
16,384 generated inline identities and 4 MiB of literal alias strings; each
alias is at most 4,096 bytes. The final navigation registry includes its alias
copies in the shared 4 MiB metadata budget. Hitting a note-navigation bound
preserves every note and citation as visible content and reports
`svg_link_limit`. Source-fragment activation is then withheld because an
incomplete alias inventory cannot prove uniqueness; admitted generated typed
links and ordinary external links can still work.

Navigation markup has a separate 8 MiB emission ceiling. A complete node is
staged before admission; exhausting a budget never writes partial XML, clips
source text or suppresses the underlying glyph, image or formula. These are
navigation budgets, not a whole-render memory quota.

Warnings are additive and deduplicated once per reason per render:
`svg_link_unsafe`, `svg_link_unresolved`, `svg_link_limit`, `svg_anchor_limit`
and `svg_link_geometry`. Measurement can intern records but cannot emit hit
regions or warnings. In a document without painted links, metadata emission
is empty and the existing vector output stays unchanged.

## Verification status

The SVG link tests cover URL admission, XML escaping, metadata limits,
heading collisions/forward references, glyph-ID isolation, hit-region merging,
clipping, no measurement side effects, real shaped text/emergency wrapping,
math/image extents, hostile nested AST links, endnotes and public API parity.
The existing XML scanner now admits escaped text only inside descriptive
`title`/`desc` elements; negative tests retain rejection of unoutlined body text
and malformed metadata entities. The showcase already contains link titles.

`tests/svg_footnote_navigation.rs` checks serialized destinations and actual
painted citation coordinates, first-use order, repeated/nested citations,
source-heading collisions, literal percent signs, private-ID isolation,
reference cycles, leading rich blocks, unreferenced notes, immutable provenance,
bounded metadata, public entry-point parity and unchanged painted output.

Run `cargo test --test svg_footnote_navigation`, `cargo test svg_links` and the
existing SVG suites, followed by the standard format/check/Clippy/test and
native/WASM gates in the DSR environment. These structural and geometry checks
do not establish browser-interaction acceptance, full accessibility conformance
or a performance improvement.
