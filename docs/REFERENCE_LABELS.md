# Unicode reference-label matching

Link and image references use the complete Unicode **17.0.0 default full case
fold**, following the case-insensitive matching rule in
[CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/#matches). This applies to
full (`[text][label]`), collapsed (`[label][]`) and shortcut (`[label]`)
references. For example:

```markdown
[guide][FFI] ![diagram][STRASSE]

[ﬃ]: guide.html
[Straße]: diagram.png
```

Both references resolve. The label's original display text and destination are
preserved; only its lookup key changes. Leading/trailing whitespace is removed,
internal whitespace is collapsed, and the first matching definition in document
source order wins, including definitions in supported containers.

The **999-character limit counts source Unicode scalars before trimming or
folding**. A legal label of 999 ligatures can expand to a 2,997-character lookup
key. A 1,000-character source label remains invalid, even if trimming or
whitespace collapsing would make its key shorter. Invalid definitions remain
ordinary visible source.

Default full folding includes expansions such as `ﬃ` → `ffi`, Armenian `և` →
`եւ`, and Greek letters with combining marks. It does not perform canonical or
compatibility normalization, transliteration, accent removal, locale-specific
Turkic casing, entity decoding, or backslash unescaping. For example, `I` and
dotless `ı` remain different keys, as do composed `é` and decomposed `é`.
Footnote identifiers and heading slugs retain their own existing rules.

## Shared behavior and data

The Rust parser and standalone offline JavaScript renderer use tables generated
from the same pinned
[Unicode data](https://www.unicode.org/Public/17.0.0/ucd/CaseFolding.txt). The Rust
parser also supplies WASM rendering, publication exports, source spans, and LSP
reference navigation and usage analysis. Those consumers require no separate
reference-label normalization. Existing WASM distributions must be rebuilt to
include parser changes.

The generated runtime tables encode all 1,585 mappings as 199 scalar ranges and
104 expansions, with identity for every unlisted scalar. They do not depend on
the Rust, JavaScript or Python runtime's Unicode casing version. No runtime
dependency or network access is added. The standalone renderer charges folded
output to its existing work budget and checks source-length limits before
building the key.

The source fixture, license, checksum and regeneration command are documented in
[`tests/fixtures/unicode/README.md`](../tests/fixtures/unicode/README.md).
Independent tests compare every Unicode scalar with that fixture; parser tests
also resolve real links and images in both directions for every full-fold
mapping. Other regression cases cover all reference forms, container precedence,
original UTF-8/UTF-16 positions, CRLF/BOM, limits, and non-equivalent labels.

```sh
cargo test --locked --test parser_reference_unicode
cargo test --locked --lib parse::unicode_casefold
cargo test --locked --features lsp --bin fmd-lsp unicode_
node --test wasm/tests/interactive_reference_unicode.test.mjs
python3 scripts/generate-reference-casefold.py --check
```
