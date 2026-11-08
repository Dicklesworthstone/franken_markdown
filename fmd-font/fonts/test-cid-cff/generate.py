#!/usr/bin/env python3
"""Generate original geometric CID-CFF1 fixtures and a FontTools oracle.

The font bytes are authored from the CFF/sfnt specifications, using only the
Python standard library. FontTools is used independently to decode the result
and record expected outlines. This is test data, not a display font.
"""

from pathlib import Path
import math
import struct

HERE = Path(__file__).resolve().parent


def u16(value):
    return struct.pack(">H", value)


def u32(value):
    return struct.pack(">I", value)


def integer(value):
    return b"\x1d" + struct.pack(">i", value)


def index(objects):
    if not objects:
        return b"\0\0"
    offsets = [1]
    for obj in objects:
        offsets.append(offsets[-1] + len(obj))
    return u16(len(objects)) + b"\x04" + b"".join(map(u32, offsets)) + b"".join(objects)


def cs(*values):
    return b"".join(b"\x1c" + struct.pack(">h", value) for value in values)


def make_cff(selector_format):
    names = index([b"FmdCidFixture"])
    strings = index([b"Fmd", b"Test", b"FmdCidFixture-FD0", b"FmdCidFixture-FD1"])
    # A global subroutine calls local subroutine zero. Its meaning depends on
    # the glyph's active FD, including when invoked from a global subroutine.
    global_subrs = index([cs(-107) + b"\x0a\x0b"])
    chars = index([
        b"\x0e",
        cs(100, 0) + b"\x15" + cs(-107) + b"\x1d\x0e",
        cs(600, 100, 0) + b"\x15" + cs(-107) + b"\x1d\x0e",
        cs(0, 0) + b"\x15" + cs(-106) + b"\x0a\x0e",
    ])
    local0 = index([
        cs(200, 0, 0, 300, -200, 0) + b"\x05\x0b",
        cs(0, 200, 200, 0, 0, -200) + b"\x08\x0b",
    ])
    local1 = index([cs(400, 0, 0, 500, -400, 0) + b"\x05\x0b"])
    # Non-identity and deliberately unsorted CIDs: selectors index GIDs.
    charset = b"\0" + u16(25) + u16(1000) + u16(7)
    selectors = (b"\0\0\0\1\0" if selector_format == 0 else
                 b"\3" + u16(3) + u16(0) + b"\0" +
                 u16(2) + b"\1" + u16(3) + b"\0" + u16(4))

    def private(default, nominal):
        return integer(default) + b"\x14" + integer(nominal) + b"\x15" + integer(18) + b"\x13"

    private0, private1 = private(600, 100), private(800, 200)

    def fd_array(p0, p1):
        return index([
            integer(393) + b"\x0c\x26" + integer(len(private0)) + integer(p0) + b"\x12",
            integer(394) + b"\x0c\x26" + integer(len(private1)) + integer(p1) + b"\x12",
        ])

    def top(charset_at, selectors_at, chars_at, fds_at):
        return index([
            integer(391) + integer(392) + integer(0) + b"\x0c\x1e" +
            integer(1001) + b"\x0c\x22" +
            integer(charset_at) + b"\x0f" +
            integer(selectors_at) + b"\x0c\x25" +
            integer(fds_at) + b"\x0c\x24" +
            integer(chars_at) + b"\x11"
        ])

    charset_at = 4 + len(names) + len(top(0, 0, 0, 0)) + len(strings) + len(global_subrs)
    selectors_at = charset_at + len(charset)
    chars_at = selectors_at + len(selectors)
    fds_at = chars_at + len(chars)
    p0 = fds_at + len(fd_array(0, 0))
    p1 = p0 + len(private0) + len(local0)
    return (b"\1\0\4\4" + names + top(charset_at, selectors_at, chars_at, fds_at) +
            strings + global_subrs + charset + selectors + chars + fd_array(p0, p1) +
            private0 + local0 + private1 + local1)


def checksum(data):
    data += b"\0" * (-len(data) % 4)
    return sum(struct.unpack(">" + "I" * (len(data) // 4), data)) & 0xFFFFFFFF


def make_font(cff):
    head = bytearray(54)
    struct.pack_into(">IIIIHH", head, 0, 0x10000, 0x10000, 0, 0x5F0F3CF5, 0, 1000)
    struct.pack_into(">QQhhhhHHhhh", head, 20, 2082844800, 2082844800,
                     0, 0, 500, 500, 0, 8, 2, 0, 0)
    hhea = bytearray(36)
    struct.pack_into(">IhhhHhhh", hhea, 0, 0x10000, 800, -200, 0, 800, 0, 0, 500)
    struct.pack_into(">h", hhea, 18, 1)
    struct.pack_into(">H", hhea, 34, 4)
    groups = [(0x41, 0x41, 1), (0x42, 0x42, 2), (0x65E5, 0x65E5, 3)]
    cmap12 = struct.pack(">HHIII", 12, 0, 16 + len(groups) * 12, 0, len(groups))
    cmap12 += b"".join(struct.pack(">III", *group) for group in groups)
    font_name = "FmdCidFixture".encode("utf-16be")
    name = struct.pack(">HHHHHHHHH", 0, 1, 18, 3, 1, 0x409, 6, len(font_name), 0) + font_name
    tables = {
        b"CFF ": cff,
        b"cmap": struct.pack(">HHHHI", 0, 1, 3, 10, 12) + cmap12,
        b"head": bytes(head),
        b"hhea": bytes(hhea),
        b"hmtx": b"".join(struct.pack(">Hh", *metrics) for metrics in [(600, 0), (600, 100), (800, 100), (600, 0)]),
        b"maxp": struct.pack(">IH", 0x5000, 4),
        b"name": name,
        b"post": u32(0x30000) + bytes(28),
    }
    count = len(tables)
    power = 1 << (count.bit_length() - 1)
    output = bytearray(b"OTTO" + struct.pack(">HHHH", count, power * 16, power.bit_length() - 1, (count - power) * 16))
    output.extend(bytes(count * 16))
    head_at = None
    for i, (tag, data) in enumerate(sorted(tables.items())):
        output.extend(bytes(-len(output) % 4))
        offset = len(output)
        struct.pack_into(">4sIII", output, 12 + i * 16, tag, checksum(data), offset, len(data))
        output.extend(data)
        if tag == b"head":
            head_at = offset
    output.extend(bytes(-len(output) % 4))
    struct.pack_into(">I", output, head_at + 8, (0xB1B0AFBA - checksum(bytes(output))) & 0xFFFFFFFF)
    return bytes(output)


def oracle(path):
    from fontTools.ttLib import TTFont
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.pens.recordingPen import RecordingPen

    font = TTFont(path)
    glyphs = font.getGlyphSet()
    lines = []
    for gid, name in enumerate(font.getGlyphOrder()):
        pen = RecordingPen()
        glyphs[name].draw(pen)
        fingerprint = 0xCBF29CE484222325
        for op, points in pen.value:
            data = {"moveTo": b"M", "lineTo": b"L", "curveTo": b"C", "closePath": b"Z"}[op]
            for point in points:
                for value in point:
                    data += struct.pack("<d", 0.0 if value == 0 else float(value))
            for byte in data:
                fingerprint = ((fingerprint ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
        bounds = BoundsPen(glyphs, ignoreSinglePoints=True)
        glyphs[name].draw(bounds)
        bbox = "none" if bounds.bounds is None else " ".join(map(str, [math.floor(bounds.bounds[0]), math.floor(bounds.bounds[1]), math.ceil(bounds.bounds[2]), math.ceil(bounds.bounds[3])]))
        advance, lsb = font["hmtx"][name]
        assert glyphs[name].width == advance
        lines.append(f"{gid} {advance} {lsb} {fingerprint:016x} {bbox}")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    references = []
    for selector_format in (0, 3):
        path = HERE / f"FmdCidFormat{selector_format}.otf"
        path.write_bytes(make_font(make_cff(selector_format)))
        references.append(oracle(path))
        print(path.name, path.stat().st_size)
    assert references[0] == references[1]
    (HERE / "fonttools-reference.txt").write_text(references[0])
