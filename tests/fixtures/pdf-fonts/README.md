These are new positive fixtures for the independent PDF-font checker, generated
with the preserved full-profile WASM engine at `81576af`. `PROVENANCE.json`
records the exact engine hash, Markdown hash, rendering options and PDF hashes.
`source.md` exercises five embedded text faces, ligatures, decomposed marks,
links and tables with both bundled font families. These are historical-engine
outputs and do not qualify the current Rust tree. No prior golden was replaced.

The checker reads font tables independently of the producer and uses qpdf,
MuPDF and Poppler. Mutated negative PDFs are private temporary copies rebuilt
by qpdf; tests verify that the positive fixture bytes remain unchanged.
PDF width quantization must remain strictly below one unit in 1000/em space;
reported deltas are not proof of exact glyph placement. Unicode continuation
nodes for ligatures are checked separately from actual glyph indices.
