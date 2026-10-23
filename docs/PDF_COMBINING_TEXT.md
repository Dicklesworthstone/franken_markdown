# Combining marks in PDF text

PDF text uses the selected embedded font's OpenType positioning for supported
Latin combining sequences that cannot be represented by a complete canonical
composite in that face. This includes mark-to-base attachment and stacked
mark-to-mark attachment. Paragraphs, headings, links, tables, code, running
headers and footers, and text inside embedded SVGs use the same retained
advances and offsets.

Ordinary ASCII and clusters with a complete canonical composite keep their
existing glyph, kerning, layout, and PDF operator paths. Source spelling is
never normalized in the document or verification text layer. A positioned PDF
run carries its original spelling in `/ActualText`; glyphs selected by GSUB
are included in the embedded subset and its `ToUnicode` mapping.

## Measurement and page geometry

Positioned runs are measured and painted with the same integer advances.
Drawing applies every glyph's horizontal and vertical offset, including
zero-advance marks. PDF font-width rounding therefore cannot move later bases
away from their measured positions. The existing microtype expansion and SVG
text transforms also apply to mark placement. SVG letter spacing, word
spacing, and `textLength` spacing operate between complete source clusters;
they do not detach marks from their bases or break a retained ligature.

The renderer measures actual glyph ink after positioning. Tall stacks grow
line leading before paragraph alternatives, footnote admission, and pagination
are finalized. Table row spacing and backgrounds, code panels and chips,
blockquote backgrounds, heading rule clearance, and text link annotations
include the positioned vertical ink. Running bands check their actual fitted,
page-expanded text before drawing and move existing rules clear of that ink.
A positioned band that cannot fit its page margin returns the existing
`pdf_running_header_does_not_fit` or `pdf_running_footer_does_not_fit` error.
Skipped bands and ordinary unpositioned bands retain their existing behavior.

Each render caches prepared runs by exact source text within its actual font
face. Cached data includes advances, offsets, ink, and fallback diagnostics;
it is never shared across unrelated fonts or altered by a PDF subset's glyph
renumbering. The bounded cache is an optimization: declining a cache entry
does not change the resulting glyph placement.

## Supported profile and fallback

This uses the existing strict `fmd-font` Latin shaper with its default language
and features. It supports the shaper's declared Latin and combining-mark
repertoire and its supported GSUB/GPOS lookups. It does not add bidirectional
paragraph layout, Arabic script selection, font discovery, or cross-face mark
attachment.

Whitespace-separated words are independent shaping units. A large paragraph
or merged PDF segment does not exhaust the budget of an otherwise supported
word. The profile limits one marked word to 64 KiB and 4096 Unicode scalars;
the strict shaper additionally limits consecutive marks and table work.
Emergency wrapping remeasures each actual fragment through this same path.
If joining line-break candidates changes strict-shaper admission and makes
the final line too wide, the renderer refits that line before pagination.
Tables likewise retain positioning-sensitive words across separator
opportunities, so column allocation and wrapping see the word that is drawn.
Diagnostics inspect source words before wrapping, so a long source word may
report a limit even when some of its smaller wrapped fragments can be
positioned. Measurement and drawing make the same decision for each fragment.

If the selected font cannot position a marked word, the complete word remains
on the existing glyph fallback path. Other supported words in the same
segment still receive their mark positions. A missing character continues to
produce the existing `missing_glyphs` diagnostic and visible missing-glyph
box. The render warning and verification report expose two additional stable
codes:

| Code | Meaning |
| --- | --- |
| `text_shaping_unsupported` | The font, script, source sequence, or required lookup is outside the supported strict Latin profile. |
| `text_shaping_limit` | The source or font processing exceeded the bounded shaping profile. |

Warnings retain a bounded source sample and the strict shaper's reason.
Repeated failures with the same sample and reason are deduplicated. Source
preflight also includes resolved running templates and text in running-only
SVG logos; page-number tokens use a representative folio without a second
pagination pass. These are non-fatal findings; rendering continues.

## Regression evidence

`tests/pdf_text_positioning_test.rs` inspects emitted PDF content streams and
font mappings against the independent HarfBuzz coordinates saved with
`fmd-font/fonts/test-shaping/FmdShaping.ttf`. It covers base and stacked marks,
ligatures, exact source extraction, repeated cached runs across pages,
chunked/monolithic determinism, page and annotation containment, wrapped code
and tables, long marked paragraphs, named fallback findings, and transformed
SVG text with repeated stroke/fill painting. Additional cases protect SVG
cluster spacing, emergency fragments and table separators with nonzero mark
advances, and running-band ink, margin errors, and diagnostic coverage.

`src/pdf/text_positioning_tests.rs` additionally checks retained design-unit
metrics, face-local cache identity, per-word admission of a long merged run,
strict refusal without partial glyph output, line ink across block kinds,
microtype scaling, and preservation of the ordinary ASCII/composition paths.
The existing `pdf_composition_test` target protects canonical composition and
its byte-identical glyph geometry across font families and styles.
