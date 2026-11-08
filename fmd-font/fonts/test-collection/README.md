# Mixed font-collection fixtures

These original geometric fixtures are distributed under the package
[LICENSE](../../LICENSE). They contain no third-party font artwork. They are
test data, not bundled design faces.

| File | Bytes | Contents |
| --- | ---: | --- |
| `FmdCollectionTrueType.ttf` | 564 | Standalone TrueType source for face 0 |
| `FmdMixedFacesV1.ttc` | 1,204 | TTC 1.0, two faces with shared tables |
| `FmdMixedFacesV2.otc` | 1,224 | TTC 2.0, the same faces and a DSIG descriptor |

Both collections contain these faces in the same order:

1. `FmdCollectionTrueType.ttf`: two rectangles and one triangle.
2. [`FmdCidFormat3.otf`](../test-cid-cff/README.md): two different rectangles
   and a cubic arch, using CID font dictionaries and local/global subroutines.

Both map `A`, `B` and U+65E5 to glyphs 1, 2 and 3. The U+65E5 glyph is an
original geometric test shape, not a drawing of the Japanese character.
The different outlines make wrong-face selection observable. The collection
directories share the actual `head`, `hhea`, `hmtx`, `cmap` and `post` table
payloads, so a decoder that treats table offsets as relative to a face
directory fails. Shared `head.checksumAdjustment` is zero in the collection;
extraction must regenerate the standalone value without altering shared bytes.

The version 2 fixture has an eight-byte empty DSIG structure and valid header
offset/length metadata. Extraction discards this signature metadata. It is a
format fixture and makes no authenticity claim.

`generate.py` constructs the TrueType outlines, sfnt containers and both
collections directly with Python's standard library. It then independently
opens the finished files with FontTools 4.61.1 and verifies every mapped glyph's
contour commands and metrics against its standalone source. FontTools is only
needed for regeneration; Rust tests consume the checked-in bytes.

```sh
python fmd-font/fonts/test-collection/generate.py
```

The collection unit tests require exact standalone byte recovery for both
faces and header versions. Additional cases cover table reordering, stale
source checksums, collection/face signature removal, explicit index errors,
malformed headers and ranges, duplicate tags, unsupported unselected faces,
count/byte limits, and shared-table output amplification.

Format reference: [OpenType Font File, font collections and checksums](https://learn.microsoft.com/en-us/typography/opentype/spec/otff).
