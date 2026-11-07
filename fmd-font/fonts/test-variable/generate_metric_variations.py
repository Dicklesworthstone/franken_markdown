"""Independent FontTools oracle for HVAR/MVAR static metric instancing.

FontTools 4.61.1 does not instantiate the MVAR gsp0..gsp9 tags. The fixtures
include gsp0/gsp1, but this oracle omits their values: the Rust integration
test checks those separately using explicit expectations from OpenType MVAR.

Run from any directory with FontTools 4.61.1 or newer. All outlines and deltas
are authored here; no third-party font is copied. The fixtures share this
directory's OFL-1.1 license. FontTools is a fixture/oracle tool only and is not
a build or runtime dependency of fmd-font.
"""

import io
from pathlib import Path

import fontTools
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables import otTables
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.varLib import builder, instancer


ROOT = Path(__file__).resolve().parent
ORDER = [".notdef", "space", "A", "B"]
REGIONS = [
    {"wght": (-1.0, -1.0, 0.0)},
    {"wght": (0.0, 1.0, 1.0)},
    {"wght": (0.25, 0.5, 0.75)},
]
AXES = ["wght", "wdth"]
ADVANCES = [[0, 0, 0], [-30, 90, 12], [-40, 120, 16], [-40, 120, 16]]
BEARINGS = [[0, 0, 0], [0, 0, 0], [-8, 24, 4], [-8, 24, 4]]

# Tag, source table, field, offset, signedness, three region deltas.
METRICS = [
    ("hasc", "OS/2", "sTypoAscender", 68, "i16", [-80, 100, 20]),
    ("hdsc", "OS/2", "sTypoDescender", 70, "i16", [20, -30, -4]),
    ("hlgp", "OS/2", "sTypoLineGap", 72, "i16", [-10, 20, 4]),
    ("hcla", "OS/2", "usWinAscent", 74, "u16", [-80, 100, 20]),
    ("hcld", "OS/2", "usWinDescent", 76, "u16", [-20, 30, 4]),
    ("xhgt", "OS/2", "sxHeight", 86, "i16", [-30, 40, 6]),
    ("cpht", "OS/2", "sCapHeight", 88, "i16", [-40, 50, 8]),
    ("sbxs", "OS/2", "ySubscriptXSize", 10, "i16", [-10, 20, 2]),
    ("sbys", "OS/2", "ySubscriptYSize", 12, "i16", [-12, 22, 4]),
    ("sbxo", "OS/2", "ySubscriptXOffset", 14, "i16", [-2, 6, 2]),
    ("sbyo", "OS/2", "ySubscriptYOffset", 16, "i16", [-4, 8, 2]),
    ("spxs", "OS/2", "ySuperscriptXSize", 18, "i16", [-10, 20, 2]),
    ("spys", "OS/2", "ySuperscriptYSize", 20, "i16", [-12, 22, 4]),
    ("spxo", "OS/2", "ySuperscriptXOffset", 22, "i16", [-2, 6, 2]),
    ("spyo", "OS/2", "ySuperscriptYOffset", 24, "i16", [-6, 12, 2]),
    ("strs", "OS/2", "yStrikeoutSize", 26, "i16", [-5, 15, 2]),
    ("stro", "OS/2", "yStrikeoutPosition", 28, "i16", [-20, 30, 4]),
    ("hcrs", "hhea", "caretSlopeRise", 18, "i16", [-20, 50, 4]),
    ("hcrn", "hhea", "caretSlopeRun", 20, "i16", [-10, 20, 2]),
    ("hcof", "hhea", "caretOffset", 22, "i16", [-4, 10, 2]),
    ("vasc", "vhea", "ascent", 4, "i16", [-30, 40, 6]),
    ("vdsc", "vhea", "descent", 6, "i16", [20, -30, -4]),
    ("vlgp", "vhea", "lineGap", 8, "i16", [0, 10, 2]),
    ("vcrs", "vhea", "caretSlopeRise", 18, "i16", [-6, 12, 2]),
    ("vcrn", "vhea", "caretSlopeRun", 20, "i16", [-10, 20, 2]),
    ("vcof", "vhea", "caretOffset", 22, "i16", [-4, 10, 2]),
    ("undo", "post", "underlinePosition", 8, "i16", [10, -20, -2]),
    ("unds", "post", "underlineThickness", 10, "i16", [-4, 12, 2]),
    ("gsp0", "gasp", None, 4, "u16", [-2, 2, 0]),
    ("gsp1", "gasp", None, 8, "u16", [-4, 4, 0]),
]


def store(rows):
    regions = builder.buildVarRegionList(REGIONS, AXES)
    return builder.buildVarStore(regions, [builder.buildVarData([0, 1, 2], rows)])


def make_font(explicit):
    fb = FontBuilder(1000, isTTF=True)
    fb.setupGlyphOrder(ORDER)
    fb.setupCharacterMap({32: "space", 65: "A", 66: "B"})
    glyphs = {}
    for name, left, right in [(".notdef", 0, 400), ("space", 0, 0), ("A", 50, 500), ("B", 40, 480)]:
        pen = TTGlyphPen(None)
        if name != "space":
            pen.moveTo((left, 0))
            pen.lineTo((right, 0))
            pen.lineTo((right, 700))
            pen.lineTo((left, 700))
            pen.closePath()
        glyphs[name] = pen.glyph()
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics({".notdef": (500, 0), "space": (300, 0), "A": (600, 50), "B": (620, 40)})
    fb.setupHorizontalHeader(ascent=800, descent=-200, lineGap=100, caretSlopeRise=1000)
    fb.setupVerticalMetrics({name: (1000, 100) for name in ORDER})
    fb.setupVerticalHeader(ascent=500, descent=-500, lineGap=50)
    fb.setupOS2(version=4, sTypoAscender=800, sTypoDescender=-200, sTypoLineGap=100,
                usWinAscent=800, usWinDescent=200, sxHeight=500, sCapHeight=700,
                ySubscriptXSize=600, ySubscriptYSize=650, ySubscriptYOffset=100,
                ySuperscriptXSize=600, ySuperscriptYSize=650, ySuperscriptYOffset=350,
                yStrikeoutSize=50, yStrikeoutPosition=300)
    fb.setupNameTable({"familyName": "Fmd Metrics Fixture", "styleName": "Regular",
                       "uniqueFontIdentifier": "FmdMetricsFixture1", "fullName": "Fmd Metrics Fixture",
                       "psName": "FmdMetricsFixture", "version": "Version 1.0",
                       "copyright": "Copyright 2026 FrankenMarkdown contributors. SIL Open Font License 1.1."})
    fb.setupPost(underlinePosition=-100, underlineThickness=50)
    fb.setupMaxp()
    fb.setupFvar([("wght", 100, 400, 900, "Weight"), ("wdth", 75, 100, 125, "Width")], [])
    font = fb.font
    font["head"].flags |= 2
    font["head"].created = font["head"].modified = 2082844800
    font.recalcTimestamp = False
    avar = newTable("avar")
    avar.segments = {"wght": {-1: -1, 0: 0, 0.5: 0.75, 1: 1}, "wdth": {-1: -1, 0: 0, 1: 1}}
    font["avar"] = avar
    font["gasp"] = newTable("gasp")
    font["gasp"].gaspRange = {8: 2, 16: 1, 65535: 3}
    font["gvar"] = newTable("gvar")
    font["gvar"].variations = {}
    for gid, name in enumerate(ORDER):
        points = 0 if name == "space" else 4
        variations = []
        for region, support in enumerate(REGIONS):
            outline = [(BEARINGS[gid][region], 0)] * points
            phantoms = [(0, 0), (ADVANCES[gid][region], 0), (0, 0), (0, 0)]
            variations.append(TupleVariation(support, outline + phantoms))
        font["gvar"].variations[name] = variations
    hvar = newTable("HVAR")
    hvar.table = otTables.HVAR()
    hvar.table.Version = 0x00010000
    if explicit:
        # Two outer subtables and shared final entries exercise packed outer/
        # inner mappings; the advance map may be shorter than the glyph list.
        regions = builder.buildVarRegionList(REGIONS, AXES)
        bearing_rows = [[0, 0, 0], BEARINGS[2], [a - b for a, b in zip(ADVANCES[2], BEARINGS[2])]]
        hvar.table.VarStore = builder.buildVarStore(regions, [
            builder.buildVarData([0, 1, 2], bearing_rows),
            builder.buildVarData([0, 1, 2], ADVANCES[:3]),
        ])
        hvar.table.AdvWidthMap = builder.buildVarIdxMap([0x10000, 0x10001, 0x10002, 0x10002], ORDER)
        hvar.table.LsbMap = builder.buildVarIdxMap([0, 0, 1, 1], ORDER)
        hvar.table.RsbMap = builder.buildVarIdxMap([0, 0x10001, 2, 2], ORDER)
    else:
        hvar.table.VarStore = store(ADVANCES)
        hvar.table.AdvWidthMap = hvar.table.LsbMap = hvar.table.RsbMap = None
    font["HVAR"] = hvar
    mvar = newTable("MVAR")
    mvar.table = otTables.MVAR()
    mvar.table.Version = 0x00010000
    mvar.table.Reserved = 0
    mvar.table.ValueRecordSize = 8
    mvar.table.VarStore = store([metric[5] for metric in METRICS])
    mvar.table.ValueRecord = []
    for index, metric in sorted(enumerate(METRICS), key=lambda item: item[1][0]):
        record = otTables.MetricsValueRecord()
        record.ValueTag = metric[0]
        record.VarIdx = index
        mvar.table.ValueRecord.append(record)
    mvar.table.ValueRecordCount = len(mvar.table.ValueRecord)
    font["MVAR"] = mvar
    return font


def static_instance(original, weight):
    static = instancer.instantiateVariableFont(original, {"wght": weight, "wdth": 100}, inplace=False)
    # Serializing makes FontTools independently recompute global bounds and
    # hhea summary metrics from the complete instanced glyph set.
    stream = io.BytesIO()
    static.save(stream)
    return TTFont(io.BytesIO(stream.getvalue()), recalcTimestamp=False)


def verify_wide_empty_summary(path):
    # Separate explicit integration cases: an empty glyph participates in
    # advanceWidthMax, but not the signed hhea extrema or global ink bounds.
    for bearing in (-1000, 2000):
        original = TTFont(path, recalcTimestamp=False)
        original["hmtx"].metrics["space"] = (1000, bearing)
        static = static_instance(original, 900)
        assert static["hmtx"].metrics["space"] == (1090, bearing)
        assert [getattr(static["hhea"], field) for field in
                ("advanceWidthMax", "minLeftSideBearing", "minRightSideBearing", "xMaxExtent")] == [1090, 0, 100, 524]
        assert [getattr(static["head"], field) for field in ("xMin", "yMin", "xMax", "yMax")] == [0, 0, 524, 700]


def main():
    lines = [f"# FontTools {fontTools.__version__}; generated by generate_metric_variations.py",
             "# gasp MVAR values are intentionally omitted: FontTools 4.61.1 does not instantiate gsp0..gsp9.",
             "# fixture\tweight\ttable\tglyph-or-offset\tadvance-or-type\tlsb-or-value"]
    for explicit in (False, True):
        stem = "FmdMappedMetricsVF" if explicit else "FmdMetricsVF"
        path = ROOT / f"{stem}.ttf"
        make_font(explicit).save(path)
        for weight in (100, 250, 400, 525, 650, 900):
            original = TTFont(path, recalcTimestamp=False)
            static = static_instance(original, weight)
            for gid, name in enumerate(ORDER):
                advance, bearing = static["hmtx"].metrics[name]
                lines.append(f"{stem}\t{weight}\thmtx\t{gid}\t{advance}\t{bearing}")
            for _, tag, attr, offset, kind, _ in METRICS:
                if tag == "gasp":
                    continue
                value = getattr(static[tag], attr)
                lines.append(f"{stem}\t{weight}\t{tag}\t{offset}\t{kind}\t{value}")
            for tag, fields in [
                ("hhea", [(4, "ascender", "i16"), (6, "descender", "i16"), (8, "lineGap", "i16"),
                           (10, "advanceWidthMax", "u16"), (12, "minLeftSideBearing", "i16"),
                           (14, "minRightSideBearing", "i16"), (16, "xMaxExtent", "i16")]),
                ("head", [(36, "xMin", "i16"), (38, "yMin", "i16"), (40, "xMax", "i16"), (42, "yMax", "i16")]),
            ]:
                for offset, attr, kind in fields:
                    lines.append(f"{stem}\t{weight}\t{tag}\t{offset}\t{kind}\t{getattr(static[tag], attr)}")
        if explicit:
            verify_wide_empty_summary(path)
    (ROOT / "metric-variations-reference.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
