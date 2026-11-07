#!/usr/bin/env python3
"""Original OFL composite-variable-font fixture; FontTools is the oracle.

Requires fonttools==4.61.1 for regeneration only. Rust regression tests consume
the committed font. Components deliberately precede their referenced glyphs in
glyph order, and the nested composite has no variation of its own.
"""

import json
from io import BytesIO
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.varLib.instancer import instantiateVariableFont


def triangle(points):
    pen = TTGlyphPen(None)
    pen.moveTo(points[0])
    for point in points[1:]:
        pen.lineTo(point)
    pen.closePath()
    return pen.glyph()


def build():
    order = [".notdef", "Aacute", "nested", "attached", "A", "acute"]
    glyphs = {
        ".notdef": TTGlyphPen(None).glyph(),
        "A": triangle([(0, 0), (100, 0), (50, 100)]),
        "acute": triangle([(0, 0), (40, 0), (20, 40)]),
    }
    pen = TTGlyphPen(glyphs)
    pen.addComponent("A", (1, 0, 0, 1, 10, 20))
    pen.addComponent("acute", (1, 0, 0, 1, 100, 100))
    glyphs["Aacute"] = pen.glyph()
    pen = TTGlyphPen(glyphs)
    pen.addComponent("Aacute", (1.5, 0, 0, 1.5, -20, 30))
    glyphs["nested"] = pen.glyph()
    pen = TTGlyphPen(glyphs)
    pen.addComponent("A", (1, 0, 0, 1, 0, 0))
    pen.addComponent("acute", (1, 0, 0, 1, 50, 100))
    attached = pen.glyph()
    accent = attached.components[1]
    del accent.x, accent.y
    accent.firstPt, accent.secondPt = 2, 0
    glyphs["attached"] = attached

    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({ord("A"): "A", ord("Á"): "Aacute", ord("Ǻ"): "nested", ord("Å"): "attached"})
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({
        ".notdef": (500, 0), "A": (500, 0), "acute": (100, 0),
        "Aacute": (500, 10), "nested": (700, -5), "attached": (500, 0),
    })
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({
        "familyName": "Fmd Composite Variations Test", "styleName": "Regular",
        "uniqueFontIdentifier": "FmdCompositeVariationsVF-1",
        "fullName": "Fmd Composite Variations Test Regular",
        "psName": "FmdCompositeVariationsTest-Regular", "version": "Version 1.000",
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
    # Composite pseudo-points are the two components FIRST, followed by the
    # four phantom points. The accent's offsets outgrow byte arguments.
    gvar.variations = {
        ".notdef": [], "nested": [],
        "Aacute": [TupleVariation({"wght": (0, 1, 1)}, [
            (30, 10), (60, 80), (0, 0), (120, 0), (0, 0), (0, 0),
        ])],
        "attached": [TupleVariation({"wght": (0, 1, 1)}, [
            (10, 20), (0, 0), (0, 0), (40, 0), (0, 0), (0, 0),
        ])],
        "A": [TupleVariation({"wght": (0, 1, 1)}, [
            (20, 0), (30, 0), (25, 30),
            (0, 0), (80, 0), (0, 0), (0, 0),
        ])],
        "acute": [TupleVariation({"wght": (0, 1, 1)}, [
            (0, 0), (20, 0), (10, 20),
            (0, 0), (20, 0), (0, 0), (0, 0),
        ])],
    }
    font["gvar"] = gvar
    font["head"].created = font["head"].modified = 3_700_000_000
    font.recalcTimestamp = False
    target = Path(__file__).with_name("FmdCompositeVariationsVF.ttf")
    font.save(target)

    reference = {}
    for weight in [400, 650, 900]:
        instance = instantiateVariableFont(TTFont(target), {"wght": weight})
        # Compare the actual static sfnt, including TrueType's final integer
        # coordinates, rather than fractional in-memory instancer scratch.
        encoded = BytesIO()
        instance.save(encoded)
        encoded.seek(0)
        instance = TTFont(encoded)
        reference[weight] = {}
        for name in ["Aacute", "nested", "attached", "A", "acute"]:
            glyph = instance["glyf"][name]
            coords, _, _ = glyph.getCoordinates(instance["glyf"])
            reference[weight][name] = {
                "points": list(coords),
                "bbox": [glyph.xMin, glyph.yMin, glyph.xMax, glyph.yMax],
                "hmtx": instance["hmtx"][name],
            }
    target.with_suffix(".json").write_text(json.dumps(reference, indent=2) + "\n")
    print(json.dumps(reference, separators=(",", ":")))
    print(f"Wrote {target.name}: {target.stat().st_size} bytes")


if __name__ == "__main__":
    build()
