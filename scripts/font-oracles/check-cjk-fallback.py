#!/usr/bin/env python3
"""Check the bundled CJK face against the pinned variable source with FontTools.

Development oracle only; the renderer and generator have no FontTools dependency.
The default checks all cmap coverage and 166 representative glyphs. --full checks
all retained character mappings, including their outlines and horizontal advances.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from fontTools import __version__ as fonttools_version
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[2]
SOURCE_SHA256 = "a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da"
RANGES = [
    (0x3000, 0x303F), (0x3040, 0x30FF), (0x31F0, 0x31FF),
    (0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xF900, 0xFAFF),
    (0xFE10, 0xFE1F), (0xFE30, 0xFE4F), (0xFF00, 0xFFEF),
]
SENTINELS = "中文日本語かなカナ龍齉龘鬱檸檬麒麟「」【】。、！？々〆〇ーあいうえおアイウエオ"


def difference(actual, expected):
    """Compare path topology as well as every coordinate (including None)."""
    if isinstance(actual, (tuple, list)) and isinstance(expected, (tuple, list)):
        if len(actual) != len(expected):
            raise ValueError("path element counts differ")
        return max((difference(a, e) for a, e in zip(actual, expected)), default=0.0)
    if isinstance(actual, (int, float)) and isinstance(expected, (int, float)):
        return abs(actual - expected)
    if actual != expected:
        raise ValueError(f"path operations differ: {actual!r} != {expected!r}")
    return 0.0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="pinned NotoSansSC-wght.ttf")
    parser.add_argument("--font", type=Path,
                        default=ROOT / "fmd-font/fonts/noto-sans-cjk/NotoSansSC-CJK.ttf")
    parser.add_argument("--full", action="store_true", help="check every retained character")
    parser.add_argument("--report", type=Path, help="also save the exact JSON result")
    args = parser.parse_args()
    source_bytes = args.source.read_bytes()
    if hashlib.sha256(source_bytes).hexdigest() != SOURCE_SHA256:
        parser.error("source SHA-256 differs from the pinned Google Fonts revision")
    source = TTFont(args.source, lazy=True)
    actual = TTFont(args.font, lazy=True)
    source_cmap = source.getBestCmap()
    actual_cmap = actual.getBestCmap()
    expected = {cp for lo, hi in RANGES for cp in range(lo, hi + 1) if cp in source_cmap}
    errors = []
    if set(actual_cmap) != expected:
        errors.append({"coverage_missing": len(expected - set(actual_cmap)),
                       "coverage_extra": len(set(actual_cmap) - expected)})
    if any(tag in actual for tag in ["fvar", "avar", "gvar"]):
        errors.append({"variation_tables": "fallback must be a frozen static instance"})
    samples = (sorted(expected) if args.full else
               sorted({ord(ch) for ch in SENTINELS} | set(sorted(expected)[::223])))
    source_glyphs = source.getGlyphSet(location={"wght": 400})
    actual_glyphs = actual.getGlyphSet()
    maximum = 0.0
    for cp in samples:
        if cp not in actual_cmap:
            continue
        reference = source_glyphs[source_cmap[cp]]
        retained = actual_glyphs[actual_cmap[cp]]
        source_pen = DecomposingRecordingPen(source_glyphs)
        actual_pen = DecomposingRecordingPen(actual_glyphs)
        reference.draw(source_pen)
        retained.draw(actual_pen)
        try:
            error = max(difference(actual_pen.value, source_pen.value),
                        abs(retained.width - reference.width))
        except ValueError as failure:
            errors.append({"character": f"U+{cp:04X}", "error": str(failure)})
            continue
        maximum = max(maximum, error)
        # Static glyf coordinates and advances are integers. One font unit
        # admits independent interpolation/rounding, not shape-scale errors.
        if error > 1.0:
            errors.append({"character": f"U+{cp:04X}", "error_units": error})
    result = {
        "ok": not errors,
        "fonttools_version": fonttools_version,
        "source_sha256": SOURCE_SHA256,
        "artifact_sha256": hashlib.sha256(args.font.read_bytes()).hexdigest(),
        "mapped_characters": len(actual_cmap),
        "glyphs": actual["maxp"].numGlyphs,
        "geometry_checks": len(samples),
        "max_coordinate_or_advance_error_units": maximum,
        "failure_count": len(errors),
        "failure_examples": errors[:8],
    }
    text = json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    print(text, end="")
    if args.report:
        args.report.write_text(text)
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
