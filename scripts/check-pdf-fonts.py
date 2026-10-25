#!/usr/bin/env python3
"""Independently inspect FMD Identity-H/embedded-TrueType text PDFs.

Uses qpdf for PDF objects/syntax, MuPDF for actual glyph uses and text, and
Poppler for a second logical-text decoder. Does not import renderer code.
Width differences must be strictly below one unit in the PDF's 1000/em space;
that quantization is reported, not described as exact layout/typography proof.
"""
import argparse
import base64
from fractions import Fraction
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET

SCOPE = "Identity-H embedded TrueType glyph/width checks; not layout, PDF/A, PDF/UA or current-source qualification"


class InvalidPDF(Exception):
    pass


def require(condition, message):
    if not condition:
        raise InvalidPDF(message)


def execute(arguments):
    result = subprocess.run(arguments, capture_output=True, text=True, timeout=30)
    require(result.returncode == 0,
            f"{arguments[0]} exited {result.returncode}: {result.stderr or result.stdout}")
    require("warning:" not in result.stderr.lower() and "error:" not in result.stderr.lower(),
            f"{arguments[0]} diagnostic: {result.stderr}")
    return result


def number(value):
    require(type(value) in (int, float) and math.isfinite(value), "Non-finite or nonnumeric PDF width")
    require(value >= 0, "Negative PDF glyph width")
    return Fraction(str(value))


def ttf_metrics(data):
    require(len(data) >= 12 and data[:4] in (b"\x00\x01\x00\x00", b"true"),
            "Unsupported or invalid embedded TrueType sfnt header")
    count = struct.unpack_from(">H", data, 4)[0]
    require(12 + count * 16 <= len(data), "Truncated TrueType table directory")
    tables = {}
    for index in range(count):
        tag, _, offset, length = struct.unpack_from(">4sIII", data, 12 + index * 16)
        require(tag not in tables and offset + length <= len(data), "Invalid TrueType table range or duplicate tag")
        tables[tag] = (offset, length)
    for tag, minimum in [(b"head", 54), (b"maxp", 6), (b"hhea", 36), (b"hmtx", 4)]:
        require(tag in tables and tables[tag][1] >= minimum, f"Missing/truncated TrueType {tag!r}")
    units = struct.unpack_from(">H", data, tables[b"head"][0] + 18)[0]
    glyphs = struct.unpack_from(">H", data, tables[b"maxp"][0] + 4)[0]
    metrics = struct.unpack_from(">H", data, tables[b"hhea"][0] + 34)[0]
    require(16 <= units <= 16384 and 1 <= metrics <= glyphs, "Invalid TrueType units/glyph/metric counts")
    offset, length = tables[b"hmtx"]
    require(length >= metrics * 4 + (glyphs - metrics) * 2, "Truncated TrueType hmtx metrics")
    advances = [struct.unpack_from(">H", data, offset + index * 4)[0] for index in range(metrics)]
    advances.extend([advances[-1]] * (glyphs - metrics))
    return units, glyphs, advances


def pdf_widths(font):
    values = font.get("/W", [])
    require(isinstance(values, list), "Invalid PDF W array")
    result, index = {}, 0
    while index < len(values):
        start = values[index]
        require(type(start) is int and 0 <= start <= 65535 and index + 1 < len(values), "Invalid PDF width start CID")
        item = values[index + 1]
        if isinstance(item, list):
            require(start + len(item) <= 65536, "PDF widths exceed CID domain")
            for offset, width in enumerate(item):
                require(start + offset not in result, "Duplicate PDF width CID")
                result[start + offset] = number(width)
            index += 2
        else:
            require(type(item) is int and start <= item <= 65535 and index + 2 < len(values), "Invalid PDF width range")
            width = number(values[index + 2])
            for cid in range(start, item + 1):
                require(cid not in result, "Duplicate PDF width CID")
                result[cid] = width
            index += 3
    return result, number(font.get("/DW", 1000))


def verify(path, expected):
    original = path.read_bytes()
    syntax = execute(["qpdf", "--check", str(path)])
    document = json.loads(execute(["qpdf", "--json=2", "--json-stream-data=inline", str(path)]).stdout)
    objects = document["qpdf"][1]

    def resolve(reference):
        require(isinstance(reference, str) and re.fullmatch(r"\d+ \d+ R", reference) is not None,
                "Expected an indirect PDF object reference")
        require("obj:" + reference in objects, f"Missing PDF object {reference}")
        return objects["obj:" + reference]

    def value(reference):
        entry = resolve(reference)
        require(isinstance(entry.get("value"), dict), "Expected PDF dictionary")
        return entry["value"]

    def stream(reference):
        entry = resolve(reference)
        require(isinstance(entry.get("stream"), dict) and "data" in entry["stream"], "Missing decoded PDF stream")
        return base64.b64decode(entry["stream"]["data"], validate=True)

    fonts = {}
    for reference, entry in objects.items():
        dictionary = entry.get("value", {})
        if not isinstance(dictionary, dict) or dictionary.get("/Subtype") != "/Type0":
            continue
        require(dictionary.get("/Encoding") == "/Identity-H", "Unsupported Type0 encoding; require Identity-H")
        descendants = dictionary.get("/DescendantFonts")
        require(isinstance(descendants, list) and len(descendants) == 1, "Invalid descendant-font list")
        descendant = value(descendants[0])
        require(descendant.get("/Subtype") == "/CIDFontType2" and descendant.get("/CIDToGIDMap") == "/Identity",
                "Unsupported CID font or nonidentity glyph map")
        descriptor = value(descendant.get("/FontDescriptor"))
        require("/FontFile2" in descriptor, "Missing embedded TrueType FontFile2")
        require("/ToUnicode" in dictionary, "Missing ToUnicode mapping")
        cmap = stream(dictionary["/ToUnicode"])
        require(b"begincmap" in cmap and b"endcmap" in cmap, "Invalid ToUnicode stream")
        data = stream(descriptor["/FontFile2"])
        units, count, advances = ttf_metrics(data)
        widths, default = pdf_widths(descendant)
        raw_name = dictionary.get("/BaseFont")
        require(isinstance(raw_name, str) and raw_name.startswith("/"), "Invalid embedded font name")
        name = raw_name.removeprefix("/")
        require(name and name not in fonts, "Missing or ambiguous embedded font name")
        fonts[name] = {"object": reference, "unitsPerEm": units, "glyphCount": count,
                       "fontSha256": hashlib.sha256(data).hexdigest(), "advances": advances,
                       "widths": widths, "defaultWidth": default, "used": set(),
                       "uses": 0, "maxDelta": Fraction(0)}
    require(fonts, "No supported embedded text fonts")
    traced = execute(["mutool", "trace", str(path)])
    root = ET.fromstring(traced.stdout)
    uses, continuations = 0, 0
    for span in root.iter("span"):
        name = span.get("font")
        require(name in fonts, f"Unsupported or unembedded traced font {name}")
        font, previous = fonts[name], None
        for glyph in span.findall("g"):
            raw = glyph.get("glyph")
            # MuPDF emits Unicode-only nodes for ligature continuation codes.
            # They are not glyph IDs; admit only a zero-advance same-origin
            # continuation immediately following a real glyph in this span.
            if raw is None or raw == "-1":
                require(previous is not None and glyph.get("unicode") and
                        glyph.get("x") == previous.get("x") and glyph.get("y") == previous.get("y") and
                        float(glyph.get("adv", "nan")) == 0, "Orphan Unicode-only glyph continuation")
                continuations += 1
                continue
            gid = int(raw)
            require(0 < gid < font["glyphCount"], f"Invalid/.notdef used glyph {gid} in {name}")
            require(glyph.get("unicode") is not None, "Used glyph lacks decoded Unicode")
            declared = font["widths"].get(gid, font["defaultWidth"])
            measured = Fraction(font["advances"][gid] * 1000, font["unitsPerEm"])
            delta = abs(declared - measured)
            require(delta < 1, f"Glyph width disagrees with embedded hmtx: {name} gid={gid} delta={float(delta)}")
            font["maxDelta"] = max(font["maxDelta"], delta)
            font["used"].add(gid)
            font["uses"] += 1
            uses += 1
            previous = glyph
    require(uses > 0, "No actual text glyphs decoded; refusing a vacuous pass")
    decoders = {}
    for name, command in [("Poppler", ["pdftotext", str(path), "-"]),
                          ("MuPDF", ["mutool", "draw", "-F", "txt", "-o", "-", str(path)])]:
        decoded = execute(command)
        for token in expected:
            require(token in decoded.stdout, f"{name} logical text lacks exact expected token {token!r}")
        decoders[name] = {"textSha256": hashlib.sha256(decoded.stdout.encode()).hexdigest(),
                          "expectedTokens": len(expected), "diagnostics": decoded.stderr}
    require(path.read_bytes() == original, "Input PDF changed during validation")
    return {"ok": True, "scope": SCOPE, "pdfSha256": hashlib.sha256(original).hexdigest(),
            "inputUnchanged": True, "glyphUses": uses, "unicodeContinuations": continuations,
            "widthQuantizationBound": "strictly less than 1 in 1000/em units; not exact placement proof",
            "fonts": [{k: f[k] for k in ("object", "unitsPerEm", "glyphCount", "fontSha256")} |
                      {"name": name, "usedGlyphs": len(f["used"]), "uses": f["uses"],
                       "maxWidthDelta1000Em": float(f["maxDelta"])} for name, f in fonts.items()],
            "decoders": decoders, "syntaxDiagnostics": syntax.stdout + syntax.stderr}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pdf", type=Path)
    parser.add_argument("--expect-text", action="append", default=[], help="Exact Unicode token required in both logical-text decoders (no normalization)")
    args = parser.parse_args()
    if any(token == "" for token in args.expect_text):
        parser.error("--expect-text must be nonempty; an empty token cannot establish text coverage")
    args.pdf = args.pdf.resolve()
    missing = [name for name in ("qpdf", "mutool", "pdftotext") if shutil.which(name) is None]
    if missing:
        print(json.dumps({"ok": False, "scope": SCOPE, "error": "Missing required tools: " + ", ".join(missing)}))
        return 66
    try:
        report = verify(args.pdf, args.expect_text)
        report["tools"] = {}
        for name, command in [("qpdf", ["qpdf", "--version"]), ("MuPDF", ["mutool", "-v"]),
                              ("Poppler", ["pdftotext", "-v"])]:
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            require(result.returncode == 0, f"Cannot determine {name} version")
            report["tools"][name] = (result.stdout + result.stderr).splitlines()[0]
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (InvalidPDF, OSError, ValueError, KeyError, TypeError, struct.error, ET.ParseError, subprocess.TimeoutExpired) as error:
        print(json.dumps({"ok": False, "scope": SCOPE, "error": str(error)}, ensure_ascii=False))
        return 70


if __name__ == "__main__":
    sys.exit(main())
