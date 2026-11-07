#!/usr/bin/env python3
"""Original OFL fixture for independent gvar tuple inference and rounding.

Regeneration only: fonttools==4.61.1. Expected values are obtained by saving
and reopening FontTools static instances, never from the Rust implementation.
"""

from io import BytesIO
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.varLib.instancer import instantiateVariableFont


def rectangle(second=False):
    pen = TTGlyphPen(None)
    for offset in ([0, 300] if second else [0]):
        pen.moveTo((offset, 0))
        pen.lineTo((offset + 100, 0))
        pen.lineTo((offset + 100, 100))
        pen.lineTo((offset, 100))
        pen.closePath()
    return pen.glyph()


def sparse(point, delta, count=8, region=(0, 1, 1)):
    values = [None] * count
    values[point] = delta
    return TupleVariation({"wght": region}, values)


def build():
    order = [".notdef", "sparse", "reverse", "mixed", "contours", "rounding"]
    chars = dict(zip("SRMCN", order[1:]))
    glyphs = {name: rectangle(name == "contours") for name in order}
    glyphs[".notdef"] = TTGlyphPen(None).glyph()
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({ord(ch): name for ch, name in chars.items()})
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({name: (600, 0) for name in order})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({
        "familyName": "Fmd Sparse Tuple Variations Test", "styleName": "Regular",
        "uniqueFontIdentifier": "FmdSparseTupleVariationsVF-1",
        "fullName": "Fmd Sparse Tuple Variations Test Regular",
        "psName": "FmdSparseTupleVariationsTest-Regular", "version": "Version 1.000",
        "copyright": "Copyright (c) 2026, the franken_markdown authors",
        "licenseDescription": "SIL Open Font License, Version 1.1; no Reserved Font Names",
        "licenseInfoURL": "https://openfontlicense.org",
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200,
                     usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    builder.setupFvar([("wght", 100, 400, 900, "Weight")], [])
    font = builder.font
    gvar = newTable("gvar")
    first = lambda: sparse(0, (40, -20), region=(0, .5, 1))
    second = lambda: sparse(2, (-10, 60))
    gvar.variations = {
        ".notdef": [],
        "sparse": [first(), second()],
        "reverse": [second(), first()],
        "mixed": [first(), TupleVariation({"wght": (0, 1, 1)},
                    [(0, 0), (20, 0), (20, 20), (0, 20)] + [(0, 0)] * 4)],
        "contours": [sparse(0, (40, -20), 12, (0, .5, 1)), sparse(6, (-10, 60), 12)],
        "rounding": [TupleVariation({"wght": (0, 1, 1)},
                        [(-1, -1)] * 4 + [(0, 0)] * 4)],
    }
    font["gvar"] = gvar
    font["head"].created = font["head"].modified = 3_700_000_000
    font.recalcTimestamp = False
    target = Path(__file__).with_name("FmdSparseTupleVariationsVF.ttf")
    font.save(target)
    rows = ["weight\tchar\tpoints\tbbox\tadvance\tlsb"]
    for weight in [400, 525, 650, 775, 900]:
        instance = instantiateVariableFont(TTFont(target), {"wght": weight})
        encoded = BytesIO()
        instance.save(encoded)
        encoded.seek(0)
        instance = TTFont(encoded)
        for ch, name in chars.items():
            glyph = instance["glyf"][name]
            coords, _, _ = glyph.getCoordinates(instance["glyf"])
            points = ";".join(f"{x},{y}" for x, y in coords)
            bounds = ",".join(str(value) for value in [glyph.xMin, glyph.yMin, glyph.xMax, glyph.yMax])
            advance, lsb = instance["hmtx"][name]
            rows.append(f"{weight}\t{ch}\t{points}\t{bounds}\t{advance}\t{lsb}")
    target.with_suffix(".tsv").write_text("\n".join(rows) + "\n")
    print(f"Wrote {target.name}: {target.stat().st_size} bytes; {len(rows) - 1} reference rows")


if __name__ == "__main__":
    build()
