#!/usr/bin/env python3
"""Offline conversion of AMS Type 1 Computer Modern and AMS symbol faces.

franken_manim ADR-0028 (accepted 2026-10-19). Dev-only: fonttools 4.65; never
run at build or render time, and no production dependency. Run from the
repository root, pointing at the AMS release's `cm` directory (TeX Live ships it
as texmf-dist/fonts/type1/public/amsfonts/cm, with sibling symbols/):

    python3 scripts/font-oracles/convert-cm-math.py /path/to/amsfonts/cm

Writes `fmd-font/fonts/cm-math/*.ttf` and `PROVENANCE.json`. The AMS release
reserves its font names (CMMI10, CMEX10, ...), so every converted file carries a
new name ("FMD CM Math ...") with the original copyright line kept and the OFL
alongside.

Every glyph is reachable two ways:
- its TeX code position at U+F000 + code (the private-use page Microsoft's
  symbol convention uses), so fmd-math can address size variants and
  extensible pieces by their TFM positions;
- its Unicode meaning, where TeX's math mapping gives it one and the face is the
  glyph TeX would use for that character.

Outlines are converted cubic -> quadratic with cu2qu at a maximum error of one
design unit (1/1000 em), contour direction reversed to TrueType's convention.
Output bytes are deterministic: fixed timestamps, sorted tables.
"""

import hashlib
import json
import sys
from pathlib import Path

import fontTools
from fontTools.agl import toUnicode
from fontTools.fontBuilder import FontBuilder
from fontTools.misc.timeTools import timestampFromString
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.t1Lib import T1Font

MAX_ERR_UNITS = 1.0  # 1/1000 em at 1000 units per em
PUA_BASE = 0xF000
# The AMS release's date: the OFL relicensing (2009-06-30).
TIMESTAMP = timestampFromString("Tue Jun 30 00:00:00 2009")
OUT = Path("fmd-font/fonts/cm-math")

# cmmi: TeX family 1 (math italic). Letters, lowercase Greek and the variant
# Greek forms as fmd-math's commands table spells them (\epsilon = U+03F5,
# \varepsilon = U+03B5, \phi = U+03D5, \varphi = U+03C6); plus the
# characters plain TeX's \mathcode / \mathchardef take from family 1.
# Uppercase Greek stays PUA-only: TeX's \Gamma is upright (family 0).
CMMI = {
    "alpha": [0x03B1], "beta": [0x03B2], "gamma": [0x03B3], "delta": [0x03B4],
    "epsilon1": [0x03F5], "zeta": [0x03B6], "eta": [0x03B7], "theta": [0x03B8],
    "iota": [0x03B9], "kappa": [0x03BA], "lambda": [0x03BB], "mu": [0x03BC],
    "nu": [0x03BD], "xi": [0x03BE], "pi": [0x03C0], "rho": [0x03C1],
    "sigma": [0x03C3], "tau": [0x03C4], "upsilon": [0x03C5], "phi": [0x03D5],
    "chi": [0x03C7], "psi": [0x03C8], "omega": [0x03C9], "epsilon": [0x03B5],
    "theta1": [0x03D1], "pi1": [0x03D6], "rho1": [0x03F1], "sigma1": [0x03C2],
    "phi1": [0x03C6],
    "partialdiff": [0x2202], "lscript": [0x2113], "weierstrass": [0x2118],
    "dotlessi": [0x0131], "dotlessj": [0x0237], "star": [0x22C6],
    "period": [0x002E], "comma": [0x002C], "less": [0x003C], "greater": [0x003E],
    "slash": [0x002F], "flat": [0x266D], "natural": [0x266E], "sharp": [0x266F],
    "triangleright": [0x25B9], "triangleleft": [0x25C3],
    "arrowlefttophalf": [0x21BC], "arrowleftbothalf": [0x21BD],
    "arrowrighttophalf": [0x21C0], "arrowrightbothalf": [0x21C1],
    **{chr(c): [c] for c in range(ord("A"), ord("Z") + 1)},
    **{chr(c): [c] for c in range(ord("a"), ord("z") + 1)},
}

# \mathcal is cmsy's calligraphic capitals; fmd-math's calligraphic_char maps
# A-Z to the math-script codepoints, with the Letterlike holes.
LETTERLIKE_SCRIPT = {"B": 0x212C, "E": 0x2130, "F": 0x2131, "H": 0x210B,
                     "I": 0x2110, "L": 0x2112, "M": 0x2133, "R": 0x211B}
CALLIGRAPHIC = {
    chr(c): [LETTERLIKE_SCRIPT.get(chr(c), 0x1D49C + c - ord("A"))]
    for c in range(ord("A"), ord("Z") + 1)
}

# cmsy: TeX family 2 (math symbols). The calligraphic capitals map to the
# math-script codepoints (never to ASCII A-Z); pieces that are only parts of
# composite symbols (negationslash, mapsto's bar) and the small integral
# (TeX's \int is cmex's) stay PUA-only. The radical is the small variant of
# TeX's \surd and \sqrt.
CMSY = {
    **CALLIGRAPHIC,
    "radical": [0x221A],
    "minus": [0x2212], "periodcentered": [0x22C5], "multiply": [0x00D7],
    "asteriskmath": [0x2217], "divide": [0x00F7], "diamondmath": [0x22C4],
    "plusminus": [0x00B1], "minusplus": [0x2213], "circleplus": [0x2295],
    "circleminus": [0x2296], "circlemultiply": [0x2297], "circledivide": [0x2298],
    "circledot": [0x2299], "circlecopyrt": [0x25EF], "openbullet": [0x2218],
    "bullet": [0x2219], "equivasymptotic": [0x224D], "equivalence": [0x2261],
    "reflexsubset": [0x2286], "reflexsuperset": [0x2287], "lessequal": [0x2264],
    "greaterequal": [0x2265], "precedesequal": [0x2AAF], "followsequal": [0x2AB0],
    "similar": [0x223C], "approxequal": [0x2248], "propersubset": [0x2282],
    "propersuperset": [0x2283], "lessmuch": [0x226A], "greatermuch": [0x226B],
    "precedes": [0x227A], "follows": [0x227B], "arrowleft": [0x2190],
    "arrowright": [0x2192], "arrowup": [0x2191], "arrowdown": [0x2193],
    "arrowboth": [0x2194], "arrownortheast": [0x2197], "arrowsoutheast": [0x2198],
    "similarequal": [0x2243], "arrowdblleft": [0x21D0], "arrowdblright": [0x21D2],
    "arrowdblup": [0x21D1], "arrowdbldown": [0x21D3], "arrowdblboth": [0x21D4],
    "arrownorthwest": [0x2196], "arrowsouthwest": [0x2199], "proportional": [0x221D],
    "prime": [0x2032], "infinity": [0x221E], "element": [0x2208], "owner": [0x220B],
    "triangle": [0x25B3], "triangleinv": [0x25BD], "universal": [0x2200],
    "existential": [0x2203], "logicalnot": [0x00AC], "emptyset": [0x2205],
    "Rfractur": [0x211C], "Ifractur": [0x2111], "latticetop": [0x22A4],
    "perpendicular": [0x22A5], "aleph": [0x2135], "union": [0x222A],
    "intersection": [0x2229], "unionmulti": [0x228E], "logicaland": [0x2227],
    "logicalor": [0x2228], "turnstileleft": [0x22A2], "turnstileright": [0x22A3],
    "floorleft": [0x230A], "floorright": [0x230B], "ceilingleft": [0x2308],
    "ceilingright": [0x2309], "braceleft": [0x007B], "braceright": [0x007D],
    "angbracketleft": [0x27E8], "angbracketright": [0x27E9],
    "bar": [0x007C, 0x2223], "bardbl": [0x2016, 0x2225], "arrowbothv": [0x2195],
    "arrowdblbothv": [0x21D5], "backslash": [0x005C, 0x2216],
    "wreathproduct": [0x2240], "coproduct": [0x2A3F], "nabla": [0x2207],
    "unionsq": [0x2294], "intersectionsq": [0x2293], "subsetsqequal": [0x2291],
    "supersetsqequal": [0x2292], "section": [0x00A7], "dagger": [0x2020],
    "daggerdbl": [0x2021], "paragraph": [0x00B6], "club": [0x2663],
    "diamond": [0x2662], "heart": [0x2661], "spade": [0x2660],
}

# cmex: TeX family 3 (extension). Only the text-size big operators get their
# Unicode; display sizes, delimiter variants and extensible pieces are
# addressed by TeX code (PUA).
CMEX = {
    "summationtext": [0x2211], "producttext": [0x220F], "integraltext": [0x222B],
    "contintegraltext": [0x222E], "uniontext": [0x22C3], "intersectiontext": [0x22C2],
    "unionmultitext": [0x2A04], "logicalandtext": [0x22C0], "logicalortext": [0x22C1],
    "coproducttext": [0x2210], "circledottext": [0x2A00], "circleplustext": [0x2A01],
    "circlemultiplytext": [0x2A02], "unionsqtext": [0x2A06],
}

# The OT1 positions whose glyph names the Adobe Glyph List reads wrongly for
# TeX's text encoding (cmr's 0x22 and 0x5C are curly quotes, not " and \).
CMR_SKIP = {"quotedblright", "quotedblleft", "ff", "ffi", "ffl", "fi", "fl"}


def cmr_unicode(name):
    if name in CMR_SKIP:
        return []
    text = toUnicode(name)
    return [ord(text)] if len(text) == 1 else []


# (output stem, source stem, family, size, unicode table or "cmr")
# Unicode aliases are semantic identities, not guesses from Type 1 glyph names.
# In particular, AMS-A's shortminus remains PUA-only: ordinary subtraction
# belongs to cmsy, while the preamble's \\minus selects AMS-A code 0x39.
AMSA = {"clockwise": [0x21BB], "anticlockwise": [0x21BA], "check": [0x2713]}
BLACKBOARD_HOLES = {"C": 0x2102, "H": 0x210D, "N": 0x2115, "P": 0x2119,
                    "Q": 0x211A, "R": 0x211D, "Z": 0x2124}
AMSB = {
    **{chr(c): [BLACKBOARD_HOLES.get(chr(c), 0x1D538 + c - ord("A"))]
       for c in range(ord("A"), ord("Z") + 1)},
    "notbar": [0x2224], "planckover2pi1": [0x210F],
}
# eufm: \mathfrak's Euler Fraktur (amssymb loads eufrak). Letters map to the
# Fraktur codepoints, with the Letterlike holes; Unicode has no Fraktur digits,
# so the digits keep their ASCII codepoints inside this face only. Punctuation
# and the alternate letter forms stay PUA-only: TeX's math alphabet switches
# only variable-family characters.
FRAKTUR_HOLES = {"C": 0x212D, "H": 0x210C, "I": 0x2111, "R": 0x211C, "Z": 0x2128}
FRAKTUR = {
    **{chr(c): [FRAKTUR_HOLES.get(chr(c), 0x1D504 + c - ord("A"))]
       for c in range(ord("A"), ord("Z") + 1)},
    **{chr(c): [0x1D51E + c - ord("a")] for c in range(ord("a"), ord("z") + 1)},
    **{name: [ord("0") + d] for d, name in enumerate(
        ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"])},
}

FACES = [
    ("FmdCmMathItalic10", "cmmi10", "FMD CM Math Italic 10", CMMI),
    ("FmdCmMathItalic7", "cmmi7", "FMD CM Math Italic 7", CMMI),
    ("FmdCmMathItalic5", "cmmi5", "FMD CM Math Italic 5", CMMI),
    ("FmdCmMathSymbols10", "cmsy10", "FMD CM Math Symbols 10", CMSY),
    ("FmdCmMathSymbols7", "cmsy7", "FMD CM Math Symbols 7", CMSY),
    ("FmdCmMathSymbols5", "cmsy5", "FMD CM Math Symbols 5", CMSY),
    ("FmdCmMathExtension10", "cmex10", "FMD CM Math Extension 10", CMEX),
    ("FmdCmRoman10", "cmr10", "FMD CM Roman 10", "cmr"),
    ("FmdCmRoman7", "cmr7", "FMD CM Roman 7", "cmr"),
    ("FmdCmRoman5", "cmr5", "FMD CM Roman 5", "cmr"),
    ("FmdAmsASymbols10", "../symbols/msam10", "FMD AMS A Symbols 10", AMSA),
    ("FmdAmsASymbols7", "../symbols/msam7", "FMD AMS A Symbols 7", AMSA),
    ("FmdAmsASymbols5", "../symbols/msam5", "FMD AMS A Symbols 5", AMSA),
    ("FmdAmsBSymbols10", "../symbols/msbm10", "FMD AMS B Symbols 10", AMSB),
    ("FmdAmsBSymbols7", "../symbols/msbm7", "FMD AMS B Symbols 7", AMSB),
    ("FmdAmsBSymbols5", "../symbols/msbm5", "FMD AMS B Symbols 5", AMSB),
    ("FmdEulerFraktur10", "../euler/eufm10", "FMD Euler Fraktur 10", FRAKTUR),
    ("FmdEulerFraktur7", "../euler/eufm7", "FMD Euler Fraktur 7", FRAKTUR),
    ("FmdEulerFraktur5", "../euler/eufm5", "FMD Euler Fraktur 5", FRAKTUR),
]


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def convert(src_dir, stem, source, family, table):
    src = Path(src_dir) / f"{source}.pfb"
    t1 = T1Font(str(src))
    t1.parse()
    raw = t1.font
    glyph_set = t1.getGlyphSet()
    notice = raw["FontInfo"]["Notice"]
    encoding = raw["Encoding"]

    # Code positions 0..127 are the TFM positions; AMS fonts duplicate some at
    # 0xA1.. for old DOS drivers, which TeX never addresses.
    coded = [(code, name) for code, name in enumerate(encoding[:128]) if name != ".notdef"]
    order = [".notdef"] + sorted({name for _, name in coded}, key=lambda n: encoding.index(n))

    glyphs, metrics = {}, {}
    notdef = TTGlyphPen(None)
    glyphs[".notdef"] = notdef.glyph()
    metrics[".notdef"] = (500, 0)
    for name in order[1:]:
        ttpen = TTGlyphPen(None)
        pen = Cu2QuPen(ttpen, MAX_ERR_UNITS, reverse_direction=True)
        g = glyph_set[name]
        g.draw(pen)
        glyph = ttpen.glyph()
        glyph.recalcBounds(None)
        glyphs[name] = glyph
        lsb = glyph.xMin if glyph.numberOfContours else 0
        metrics[name] = (round(g.width), lsb)

    cmap = {}
    for code, name in coded:
        cmap[PUA_BASE + code] = name
    for _, name in coded:
        uni = cmr_unicode(name) if table == "cmr" else table.get(name, [])
        for cp in uni:
            cmap.setdefault(cp, name)

    fb = FontBuilder(1000, isTTF=True)
    fb.font.recalcTimestamp = False
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(metrics)
    bbox = raw["FontBBox"]
    fb.setupHorizontalHeader(ascent=bbox[3], descent=bbox[1])
    copyright = notice.replace("\\050", "(").replace("\\051", ")")
    ps_name = stem
    fb.setupNameTable({
        "copyright": copyright,
        "familyName": family,
        "styleName": "Regular",
        "uniqueFontIdentifier": f"{ps_name}; converted from AMS Type 1 {raw['FontName']}",
        "fullName": family,
        "version": "Version 1.000",
        "psName": ps_name,
        "description": (
            f"TrueType conversion of the AMS Type 1 {raw['FontName']} for franken_markdown's fmd-font "
            f"(franken_manim ADR-0028): cu2qu max error {MAX_ERR_UNITS:g}/1000 em, renamed to honour "
            "the Reserved Font Name."
        ),
        "licenseDescription": (
            "This Font Software is licensed under the SIL Open Font License, Version 1.1."
        ),
        "licenseInfoURL": "https://openfontlicense.org",
    })
    fb.setupOS2(
        sTypoAscender=bbox[3], sTypoDescender=bbox[1], sTypoLineGap=0,
        usWinAscent=max(bbox[3], 0), usWinDescent=max(-bbox[1], 0),
        achVendID="FMD ", fsType=0,
    )
    fb.setupPost(keepGlyphNames=True)
    fb.setupHead(unitsPerEm=1000, created=TIMESTAMP, modified=TIMESTAMP)
    fb.font["head"].fontRevision = 1.0
    out = OUT / f"{stem}.ttf"
    fb.font.save(str(out), reorderTables=True)
    return {
        "file": out.name,
        "family": family,
        "source": src.name,
        "source_font_name": raw["FontName"],
        "source_sha256": sha256(src),
        "sha256": sha256(out),
        "glyphs": len(order) - 1,
        "unicode_mapped": sum(1 for cp in cmap if not PUA_BASE <= cp < PUA_BASE + 0x100),
        "copyright": copyright,
    }


def main():
    src_dir = sys.argv[1]
    OUT.mkdir(parents=True, exist_ok=True)
    faces = [convert(src_dir, *face) for face in FACES]
    provenance = {
        "adr": "franken_manim ADR-0028",
        "tool": f"fonttools {fontTools.version} (fontTools.t1Lib, cu2qu, fontBuilder)",
        "script": "scripts/font-oracles/convert-cm-math.py",
        "cubic_to_quadratic_max_error_em": MAX_ERR_UNITS / 1000.0,
        "pua_base": f"U+{PUA_BASE:04X}",
        "license": "SIL Open Font License 1.1 (OFL.txt); Reserved Font Names honoured by renaming",
        "faces": faces,
    }
    (OUT / "PROVENANCE.json").write_text(json.dumps(provenance, indent=2) + "\n")
    for face in faces:
        print(f"{face['file']}: {face['glyphs']} glyphs, {face['unicode_mapped']} Unicode, {face['sha256'][:16]}")


if __name__ == "__main__":
    main()
