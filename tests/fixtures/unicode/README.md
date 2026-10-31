# Pinned Unicode reference-label data

`CaseFolding-17.0.0.txt` is the unmodified Unicode Character Database file from
<https://www.unicode.org/Public/17.0.0/ucd/CaseFolding.txt>, dated July 30, 2025.
Its SHA-256 is
`ff8d8fefbf123574205085d6714c36149eb946d717a0c585c27f0f4ef58c4183`.

`LICENSE.txt` is Unicode License v3 from <https://www.unicode.org/license.txt>.
The license is also embedded in each generated runtime table so standalone
interactive HTML exports retain the Unicode copyright and permission notice.

The generator selects every default full-fold mapping (statuses **C + F**),
without locale-specific **T** mappings or simple-only **S** alternatives. The
official file defines every unlisted scalar as an identity mapping. Native and
JavaScript tests independently read this fixture and check every Unicode scalar,
plus actual link/image resolution in both directions for all 1,585 mappings.

Regenerate or verify from the repository root, using Python 3:

```sh
python3 scripts/generate-reference-casefold.py
python3 scripts/generate-reference-casefold.py --check
```

Generation uses the checked-in data, not Python's Unicode version or a network
download. Updating Unicode requires an explicit new input file, checksum,
version and mapping-count review; this fixture must not be rewritten to match
an implementation's output.
