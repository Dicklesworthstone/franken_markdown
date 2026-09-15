# CFF reference fixture

`Bravura.otf` is Steinberg's Bravura 1.392, licensed under the adjacent SIL OFL
1.1. This unmodified fixture is shared with MTDT's pinned source artifact.
It is test data, not bundled into renderer binaries. Music layout remains a
consumer responsibility.

`fonttools-reference.txt` records advance, left side bearing, and an FNV-1a
fingerprint of every cubic outline, independently decoded by FontTools 4.65.0.
The fingerprint includes command opcodes and little-endian f64 coordinates
(negative zero normalized). Tests cover all 3,693 glyphs, not just selected
music symbols. Regenerate with `scripts/font-oracles/generate-references.py`.
