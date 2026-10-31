#!/usr/bin/env python3
"""Prove a lossless TrueType glyph re-encoding against a fully verified baseline.

Uses read-only mmap buffers and FontTools' low-level Glyph decoder, retaining
only two decoded glyphs at a time. All non-outline tables must be identical;
head may differ only in checkSumAdjustment. Composite records must remain
byte-identical, while their referenced simple glyphs are checked independently.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import mmap
from pathlib import Path
import struct
import sys

try:
    import resource
except ImportError:  # Windows: comparison remains bounded without a process cap.
    resource = None


def require(condition, message):
    if not condition:
        raise ValueError(message)


class FontBytes:
    def __init__(self, path):
        self.file = path.open("rb")
        self.data = mmap.mmap(self.file.fileno(), 0, access=mmap.ACCESS_READ)
        self.tables = {}
        count = struct.unpack_from(">H", self.data, 4)[0]
        for i in range(count):
            tag, _, start, length = struct.unpack_from(">4sIII", self.data, 12 + i * 16)
            require(start + length <= len(self.data), f"table {tag!r} exceeds the file")
            self.tables[tag] = (start, length)
        self.num_glyphs = struct.unpack_from(">H", self.data, self.tables[b"maxp"][0] + 4)[0]
        self.long_loca = struct.unpack_from(">h", self.data, self.tables[b"head"][0] + 50)[0]
        require(self.long_loca in (0, 1), "invalid loca format")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.data.close()
        self.file.close()

    def table(self, tag):
        start, size = self.tables[tag]
        return memoryview(self.data)[start:start + size]

    def sha256(self):
        digest = hashlib.sha256()
        for start in range(0, len(self.data), 65536):
            digest.update(memoryview(self.data)[start:start + 65536])
        return digest.hexdigest()

    def glyph(self, gid):
        loca, size = self.tables[b"loca"]
        stride = 4 if self.long_loca else 2
        require((gid + 2) * stride <= size, "loca does not cover the glyph count")
        fmt = ">II" if self.long_loca else ">HH"
        start, end = struct.unpack_from(fmt, self.data, loca + gid * stride)
        if not self.long_loca:
            start *= 2
            end *= 2
        glyf, length = self.tables[b"glyf"]
        require(0 <= start <= end <= length, f"glyph {gid} exceeds glyf")
        return self.data[glyf + start:glyf + end]


def apply_memory_limit(mib):
    if mib is None:
        return None
    require(mib > 0, "--max-memory-mib must be positive")
    require(resource is not None and hasattr(resource, "RLIMIT_AS"),
            "--max-memory-mib is unavailable on this platform")
    limit = mib * 1024 * 1024
    _, hard = resource.getrlimit(resource.RLIMIT_AS)
    if hard != resource.RLIM_INFINITY:
        limit = min(limit, hard)
    resource.setrlimit(resource.RLIMIT_AS, (limit, limit))
    return limit / (1024 * 1024)


def maximum_rss_kib():
    if resource is None:
        return None
    maximum = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return maximum // 1024 if sys.platform == "darwin" else maximum


def compare(args, memory_limit):
    # Delayed imports keep --help and argument validation lightweight and put
    # optional process limits in effect before loading the independent decoder.
    from fontTools import __version__ as fonttools_version
    from fontTools.ttLib.tables._g_l_y_f import Glyph

    with FontBytes(args.baseline) as base, FontBytes(args.candidate) as compact:
        base_sha, compact_sha = base.sha256(), compact.sha256()
        baseline = json.loads(args.baseline_report.read_text())
        require(baseline["ok"] and baseline["artifact_sha256"] == base_sha,
                "baseline report must be successful and match the baseline SHA-256")
        require(baseline["geometry_checks"] == baseline["mapped_characters"],
                "baseline source oracle must cover every mapped character")
        require(base.tables.keys() == compact.tables.keys(), "table sets differ")
        require(base.num_glyphs == compact.num_glyphs, "glyph counts differ")
        exact_tables = []
        for tag in base.tables:
            if tag in (b"glyf", b"loca", b"head"):
                continue
            require(base.table(tag) == compact.table(tag), f"changed table {tag!r}")
            exact_tables.append(tag.decode("ascii"))
        head_a, head_b = bytearray(base.table(b"head")), bytearray(compact.table(b"head"))
        head_a[8:12] = head_b[8:12] = b"\0" * 4
        require(head_a == head_b, "head differs beyond checkSumAdjustment")

        problems = []
        failure_count = point_count = contour_count = composite_count = 0
        for gid in range(base.num_glyphs):
            raw_a, raw_b = base.glyph(gid), compact.glyph(gid)
            count_a = struct.unpack_from(">h", raw_a)[0] if raw_a else 0
            count_b = struct.unpack_from(">h", raw_b)[0] if raw_b else 0
            checks = [("contours", count_a == count_b)]
            if count_a < 0 or count_b < 0:
                composite_count += 1
                checks.append(("composite records", raw_a == raw_b))
            elif count_a == count_b == 0:
                checks.append(("empty glyph data", raw_a == raw_b))
            else:
                a, b = Glyph(raw_a), Glyph(raw_b)
                a.expand(None)
                b.expand(None)
                if count_a > 0 and count_b > 0:
                    point_count += len(a.coordinates)
                    contour_count += a.numberOfContours
                    checks.extend([
                        ("bbox", (a.xMin, a.yMin, a.xMax, a.yMax)
                         == (b.xMin, b.yMin, b.xMax, b.yMax)),
                        ("contour endpoints", a.endPtsOfContours == b.endPtsOfContours),
                        ("coordinates", a.coordinates == b.coordinates),
                        ("point flags", a.flags == b.flags),
                        ("instructions", a.program.getBytecode() == b.program.getBytecode()),
                    ])
            failed = [name for name, ok in checks if not ok]
            if failed:
                failure_count += 1
                if len(problems) < 8:
                    problems.append({"glyph_id": gid, "differences": failed})
            # No TTFont/glyf cache: the next iteration replaces these glyphs.
        return {
            "ok": failure_count == 0,
            "fonttools_version": fonttools_version,
            "baseline_sha256": base_sha,
            "artifact_sha256": compact_sha,
            "baseline_full_source_oracle": str(args.baseline_report),
            "mapped_characters": baseline["mapped_characters"],
            "glyphs_compared": base.num_glyphs,
            "points_compared": point_count,
            "contours_compared": contour_count,
            "composites_compared": composite_count,
            "coordinate_difference_units": 0 if not failure_count else None,
            "unchanged_tables": sorted(exact_tables),
            "head_difference": "checkSumAdjustment only",
            "baseline_size_bytes": len(base.data),
            "compact_size_bytes": len(compact.data),
            "max_rss_kib": maximum_rss_kib(),
            "address_space_limit_mib": memory_limit,
            "failure_count": failure_count,
            "failure_examples": problems,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="fully source-oracle-verified static font")
    parser.add_argument("candidate", type=Path, help="font with compacted outline encoding")
    parser.add_argument("--baseline-report", type=Path, required=True,
                        help="successful full-source-oracle JSON for the baseline")
    parser.add_argument("--report", type=Path, help="also save the exact JSON result")
    parser.add_argument("--max-memory-mib", type=int,
                        help="optional address-space limit, where resource.RLIMIT_AS is available")
    args = parser.parse_args()
    try:
        result = compare(args, apply_memory_limit(args.max_memory_mib))
    except (KeyError, ValueError, OSError, struct.error) as error:
        parser.error(str(error))
    text = json.dumps(result, indent=2) + "\n"
    print(text, end="")
    if args.report:
        args.report.write_text(text)
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
