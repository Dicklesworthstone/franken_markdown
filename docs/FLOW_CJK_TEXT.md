# Chinese and Japanese text in Flow

Flow can use the existing optional `cjk-face` font for horizontal BMP Han,
precomposed hiragana and katakana, and supported CJK punctuation and fullwidth
forms. It uses the same bundled Noto Sans SC subset as the publication
renderers, without downloading fonts or relying on system font discovery.

Native builds with the default `cli` feature include `cjk-face`. Lean library
and browser builds opt in explicitly. Build the Rust Flow WASM artifact with
both features, then generate bindings using the normal WASM packaging workflow:

```sh
cargo build --locked --target wasm32-unknown-unknown \
  --no-default-features --features wasm-flow,cjk-face --lib
```

The ordinary `wasm-flow` profile does not gain the optional 9,867,212-byte font.
No JavaScript option, new dependency, font asset, or browser wire field is
needed. Rebuild the WASM artifact to change its compiled font coverage.

## Font and source identity

`BundledFlowFonts` keeps the selected body/code face first, followed by the
existing symbol face. Supported CJK characters missing from those faces use
the optional fallback. Every glyph and cluster identifies its actual face;
`font_bytes` returns that face's immutable bytes. The existing browser outline
API decodes the same glyphs for Canvas painting.

The optional face has regular Simplified Chinese glyph forms. It does not
synthesize bold/italic CJK styles or provide Japanese locale substitutions.
Surrounding Latin text retains its styles, ligatures, kerning, and supported
accent positioning. Mixed text such as `漢caféかな` preserves its original
UTF-8/UTF-16 ranges and the Latin base/mark's shared selection cluster.

## Wrapping and measured rows

Prose and table cells wrap between admitted CJK clusters without requiring
ASCII spaces. The shared CJK pair rules keep opening punctuation with the
following cluster and prevent closing punctuation and small kana from starting
a line. These rules also apply across styles and beside atomic math objects.
Code blocks retain their existing cluster-wrapping policy.

Long paragraphs and table cells use bounded measurement windows. A work-window
edge never becomes a visible line break: the renderer retains contextual
clusters and reshapes each final line. CJK boundaries do not permit splitting
a host-shaped cluster, Latin base/mark sequence, or admitted formula. Oversized
Latin words and windows that cannot settle a line still fail explicitly.
Table minimum widths include protected CJK punctuation groups. A group wider
than the available column is an explicit fit error.

Final fragments with complete vertical metrics share one measured baseline.
Rows expand to contain the greatest ascent and descent, including actual
fallback glyph bounds. Known list-marker metrics join the first body row so
markers align with CJK text and tall inline mathematics. Task boxes keep their
size and center in that row. Code highlighting retains the parent's baseline.

Custom shapers can still omit `OwnedTextRun::vertical_metrics`. Ordinary rows
with incomplete body metrics retain their established placement. Unknown
marker metrics are not guessed, and mixed mathematics retains its complete
source fallback when required body metrics are absent. See
[Flow mathematics](FLOW_MATHEMATICS.md) for the metric contract.

## Explicit limits

This is horizontal, left-to-right text using the existing greedy Flow breaker.
It does not provide complete Unicode line breaking, Japanese layout, vertical
writing, or paragraph-level bidi shaping. Missing glyphs and unsupported
sequences remain errors, including Hangul, supplementary Han, decomposed kana
voicing marks, CJK tone marks, vertical repeat marks, halfwidth kana,
CJK variation selectors, joined emoji sequences, and joining or bidi scripts.
Mapped standalone emoji and standardized presentation pairs are available
through the independent [`emoji-face` capability](FLOW_EMOJI_TEXT.md).
Failed edits preserve the last
successful source and display.

Per-call shaping, total shaping-work, display-item, line, font, and outline
budgets remain unchanged. Enabling the face does not bypass these bounds.

## Acceptance coverage

`tests/flow_cjk.rs` exercises the actual bundled font, source/caret domains,
outline containment, measured rows and markers, punctuation boundaries, edits,
and long unspaced prose/table/math content across different shaping windows.
It also covers explicit refusal with `cjk-face` disabled.

`wasm/tests/run_flow_cjk.py` consumes native-generated protocol fixtures and
optionally runs actual enabled and lean WASM packages in Chromium. Canvas,
Copy, revisions, and font-outline checks use the public Flow APIs. An optional
previous-runtime package provides ordinary-Latin pixel comparisons. The
browser does not install a system-font substitute.
