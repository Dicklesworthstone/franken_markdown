#!/usr/bin/env python3
"""Build original mixed TrueType/CID-CFF collections with shared tables.

This generator writes sfnt/TTC bytes directly. FontTools independently reads
the finished files and verifies each face against its standalone source.
"""

from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent
CID = HERE.parent / "test-cid-cff" / "FmdCidFormat3.otf"


def checksum(data):
    data += bytes(-len(data) % 4)
    return sum(struct.unpack(">" + "I" * (len(data) // 4), data)) & 0xFFFFFFFF


def tables_from(font):
    result = {}
    for i in range(struct.unpack_from(">H", font, 4)[0]):
        tag, _, offset, size = struct.unpack_from(">4sIII", font, 12 + i * 16)
        data = font[offset:offset + size]
        if tag == b"head":
            data = data[:8] + bytes(4) + data[12:]
        result[tag] = data
    return result


def sfnt_header(version, count):
    power = 1 << (count.bit_length() - 1)
    return struct.pack(">IHHHH", version, count, power * 16, power.bit_length() - 1, (count - power) * 16)


def standalone(version, tables):
    output = bytearray(sfnt_header(version, len(tables)) + bytes(16 * len(tables)))
    head = None
    for i, (tag, data) in enumerate(sorted(tables.items())):
        offset = len(output)
        struct.pack_into(">4sIII", output, 12 + i * 16, tag, checksum(data), offset, len(data))
        output.extend(data)
        output.extend(bytes(-len(output) % 4))
        if tag == b"head":
            head = offset
    struct.pack_into(">I", output, head + 8, (0xB1B0AFBA - checksum(bytes(output))) & 0xFFFFFFFF)
    return bytes(output)


def polygon(points):
    xs, ys = zip(*points)
    output = bytearray(struct.pack(">hhhhhHH", 1, min(xs), min(ys), max(xs), max(ys), len(points) - 1, 0))
    output.extend(bytes([1] * len(points)))  # On-curve, signed 16-bit deltas.
    for coordinates in (xs, ys):
        previous = 0
        for value in coordinates:
            output.extend(struct.pack(">h", value - previous))
            previous = value
    output.extend(bytes(-len(output) % 2))
    return bytes(output)


def truetype():
    tables = tables_from(CID.read_bytes())
    del tables[b"CFF "]
    glyphs = [b"", polygon([(100, 0), (300, 0), (300, 450), (100, 450)]),
              polygon([(100, 0), (500, 0), (500, 250), (100, 250)]),
              polygon([(0, 0), (200, 0), (100, 300)])]
    offsets = [0]
    for glyph in glyphs:
        offsets.append(offsets[-1] + len(glyph))
    tables[b"glyf"] = b"".join(glyphs)
    tables[b"loca"] = b"".join(struct.pack(">H", offset // 2) for offset in offsets)
    tables[b"maxp"] = struct.pack(">I14H", 0x10000, 4, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0)
    name = "FmdCollectionTrueType".encode("utf-16be")
    tables[b"name"] = struct.pack(">HHHHHHHHH", 0, 1, 18, 3, 1, 0x409, 6, len(name), 0) + name
    return standalone(0x10000, tables)


def collection(fonts, version):
    tables = [tables_from(font) for font in fonts]
    header_size = 12 + len(fonts) * 4 + (12 if version == 2 else 0)
    output = bytearray(struct.pack(">4sII", b"ttcf", version << 16, len(fonts)) + bytes(header_size - 12))
    directories = []
    for i, (font, face_tables) in enumerate(zip(fonts, tables)):
        at = len(output)
        struct.pack_into(">I", output, 12 + i * 4, at)
        directories.append(at)
        output.extend(sfnt_header(struct.unpack_from(">I", font)[0], len(face_tables)))
        output.extend(bytes(16 * len(face_tables)))
    shared = {}
    for directory, face_tables in zip(directories, tables):
        for i, (tag, data) in enumerate(sorted(face_tables.items())):
            key = (tag, data)
            if key not in shared:
                shared[key] = len(output)
                output.extend(data)
                output.extend(bytes(-len(output) % 4))
            struct.pack_into(">4sIII", output, directory + 12 + i * 16,
                             tag, checksum(data), shared[key], len(data))
    if version == 2:
        signature = struct.pack(">IHH", 1, 0, 0)  # Empty DSIG structure.
        struct.pack_into(">4sII", output, 12 + len(fonts) * 4, b"DSIG", len(signature), len(output))
        output.extend(signature)
    return bytes(output)


def verify(path, sources):
    from fontTools.ttLib import TTCollection, TTFont
    from fontTools.pens.recordingPen import RecordingPen
    from io import BytesIO

    fonts = TTCollection(path).fonts
    assert len(fonts) == len(sources)
    for embedded, source in zip(fonts, sources):
        original = TTFont(BytesIO(source))
        assert embedded.sfntVersion == original.sfntVersion
        assert embedded.getBestCmap() == original.getBestCmap()
        for character, glyph in original.getBestCmap().items():
            a, b = RecordingPen(), RecordingPen()
            original.getGlyphSet()[glyph].draw(a)
            embedded.getGlyphSet()[embedded.getBestCmap()[character]].draw(b)
            assert a.value == b.value
            assert original["hmtx"][glyph] == embedded["hmtx"][glyph]


if __name__ == "__main__":
    ttf = truetype()
    (HERE / "FmdCollectionTrueType.ttf").write_bytes(ttf)
    sources = [ttf, CID.read_bytes()]
    for version, name in [(1, "FmdMixedFacesV1.ttc"), (2, "FmdMixedFacesV2.otc")]:
        path = HERE / name
        path.write_bytes(collection(sources, version))
        verify(path, sources)
        print(name, path.stat().st_size)
