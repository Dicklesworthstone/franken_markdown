#!/usr/bin/env python3
"""Regenerate the project-authored OFL packed-delta regression with FontTools.

Requires fonttools==4.61.1 only for regeneration; Rust tests use the committed
font. The shape and variation are original test data, with no upstream face.
"""
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.varLib.instancer import instantiateVariableFont


def build() -> None:
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder([".notdef", "A"])
    builder.setupCharacterMap({ord("A"): "A"})
    empty = TTGlyphPen(None).glyph()
    pen = TTGlyphPen(None)
    pen.moveTo((0, 0))
    pen.lineTo((100, 0))
    pen.lineTo((50, 100))
    pen.closePath()
    builder.setupGlyf({".notdef": empty, "A": pen.glyph()})
    builder.setupHorizontalMetrics({".notdef": (500, 0), "A": (500, 0)})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({
        "familyName": "Fmd Packed Deltas Test",
        "styleName": "Regular",
        "uniqueFontIdentifier": "FmdPackedDeltasVF-1",
        "fullName": "Fmd Packed Deltas Test Regular",
        "psName": "FmdPackedDeltasTest-Regular",
        "version": "Version 1.000",
        "copyright": "Copyright (c) 2026, the franken_markdown authors",
        "licenseDescription": "SIL Open Font License, Version 1.1; no Reserved Font Names",
        "licenseInfoURL": "https://openfontlicense.org",
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200,
                     usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    builder.setupFvar([("wght", 100, 400, 900, "Weight")], [])
    font = builder.font
    # Three outline points, followed by four phantom points. The right
    # phantom changes the advance by +80. All packed delta types occur.
    coordinates = [(20, 0), (-10, 40), (300, -30),
                   (0, 0), (80, 0), (0, 0), (0, 0)]
    gvar = newTable("gvar")
    gvar.variations = {
        ".notdef": [],
        "A": [TupleVariation({"wght": (0, 1, 1)}, coordinates)],
    }
    font["gvar"] = gvar
    font["head"].created = font["head"].modified = 3_700_000_000
    font.recalcTimestamp = False
    target = Path(__file__).with_name("FmdPackedDeltasVF.ttf")
    font.save(target)

    # Check the committed bytes with the independent instancer, including
    # every outline coordinate and the horizontal phantom-derived metrics.
    expected = {
        400: ([(0, 0), (100, 0), (50, 100)], (500, 0)),
        650: ([(10, 0), (95, 20), (200, 85)], (540, 10)),
        900: ([(20, 0), (90, 40), (350, 70)], (580, 20)),
    }
    for weight, (points, metrics) in expected.items():
        instance = instantiateVariableFont(TTFont(target), {"wght": weight})
        actual, _, _ = instance["glyf"]["A"].getCoordinates(instance["glyf"])
        assert list(actual) == points, (weight, actual, points)
        assert instance["hmtx"]["A"] == metrics
    print(f"Verified {target.name}: {target.stat().st_size} bytes, weights 400/650/900")


if __name__ == "__main__":
    build()
