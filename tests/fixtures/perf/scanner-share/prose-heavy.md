# Prose heavy scanner corpus

This file is mostly ordinary technical prose. Headings exist so the parser has
real block structure, but the body avoids tables, fences, autolinks, and HTML
so the HTML escape scanners stay on the clean-copy path.

Typography quality in a Markdown renderer is dominated by paragraph breaking,
font metrics, and page layout rather than by hunting for a handful of special
bytes. Ordinary documentation spends almost all of its characters on letters,
spaces, and punctuation that never need HTML escaping. That is the point of
this fixture: a large, realistic clean run.

The same observation applies to README files, design notes, and most of a
performance plan. Special bytes still occur at line starts and in the occasional
emphasis marker, but they are sparse compared with the surrounding words.

Repeated paragraphs follow so the generated 1 MiB document has enough bulk to
make isolated scanner timing distinguishable from clock noise.

## Section one

Deterministic rendering means that the same Markdown, theme, fonts, and options
produce the same HTML and PDF bytes on every host. That property is more
valuable for agents and CI than a slightly faster special-byte hunt that never
shows up in end-to-end p95. The gate for SIMD work is therefore a share of
render time, not a microbenchmark that only measures the scanner in isolation.

A clean paragraph like this one still has to be walked by the HTML emitter.
If the walk is a tiny fraction of parse plus render, accelerating it cannot
move end-to-end p95 by a meaningful amount. The measurement must say so
plainly instead of proceeding on intuition.

## Section two

Hyphenation, kerning, ligatures, table allocation, and font subsetting are the
expensive stages on the PDF path. HTML emission is mostly appending and
escaping. Escaping is cheap when the input is clean, because the scanner can
skip eight-byte chunks that contain none of the four special bytes.

This paragraph is intentionally long so that a single text node is large enough
for the word-sized scanner to stay in its fast path. Words such as
deterministic, typography, optimization, representation, hyphenation,
pagination, markdown, rendering, ligature, kerning, paragraph, and document
repeat because they are ordinary documentation vocabulary.

## Section three

Agents guess `fmd README.md` first. That command should remain the happy path.
Nothing in this fixture requires a browser, a JavaScript runtime, or a
third-party Markdown crate. The scanner under test is a few dozen lines of
scalar Rust. If it is not hot, it should stay scalar.

More prose keeps the generated large document from collapsing into a single
short seed. Each extra paragraph is another clean run for the HTML emitter and
another line for the Markdown special-byte classifier.

The winter report described ordinary shipping delays, warehouse counts, and
seasonal demand without any markup beyond these headings. Readers care about
the numbers, not about how many times the renderer looked for an ampersand.
