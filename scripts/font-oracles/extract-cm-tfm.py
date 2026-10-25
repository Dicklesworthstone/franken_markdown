#!/usr/bin/env python3
"""Offline extraction of TeX's Computer Modern and AMS symbol font metrics.

franken_manim ADR-0028 bundles TeX's own CM math outlines (the AMS Type 1
release). A Type 1 face carries advances and outlines only; TeX lays out from
the TFM file: each character's width, height, depth and italic correction, and
the lig/kern program. This script reads those TFM files and writes them as
`fmd-math/src/tfm.rs`, so fmd-math boxes characters exactly as TeX does.

Dev-only, standard library only; never run at build or render time. Run from
the repository root, pointing at TeX Live's `fonts/tfm/public` directory (cm,
and amsfonts for cmex7):

    python3 scripts/font-oracles/extract-cm-tfm.py /usr/share/texlive/texmf-dist/fonts/tfm/public

Each font's fontdimens are transcribed too. TeX reads Appendix G's parameters
from the symbol and extension fonts loaded at the current size. Under LaTeX's
article and amsmath those are cmsy10/cmsy7/cmsy5 and cmex10/cmex7/cmex7 at 5 pt,
per `\\fontname\\scriptfont3` and `\\scriptscriptfont3` (franken_manim fm-giir).

The Computer Modern TFM files are distributed with TeX under the Knuth licence
(free use; changed files must be renamed). The AMS symbol metrics accompany
the OFL AMS font release. The output is a numeric transcription, not a font
file. Values are TFM fix_words divided by
2^20, so every number is exact in an f64.

Ligatures are not transcribed: the Reference preamble's microtype
`\\DisableLigatures{encoding = *, family = *}` turns them off, and a pair whose
first matching instruction is a ligature gets no kern, as in TeX.
"""

import hashlib
import struct
import sys
from pathlib import Path

OUT = Path("fmd-math/src/tfm.rs")

# (TFM file, Rust static, skewchar, directory under fonts/tfm/public). The
# skewchar is not in the TFM: LaTeX's font definitions set it (omlcmm.fd:
# \skewchar\font'177; omscmsy.fd: \skewchar\font'60), and TeX reads an accent's
# skew as the kern between the accented character and that skewchar (tex.web §741).
FONTS = [
    ("cmmi10", "CMMI10", 0o177, "cm"),
    ("cmmi7", "CMMI7", 0o177, "cm"),
    ("cmmi5", "CMMI5", 0o177, "cm"),
    ("cmsy10", "CMSY10", 0o60, "cm"),
    ("cmsy7", "CMSY7", 0o60, "cm"),
    ("cmsy5", "CMSY5", 0o60, "cm"),
    ("cmex10", "CMEX10", None, "cm"),
    ("cmex7", "CMEX7", None, "amsfonts/cmextra"),
    ("cmr10", "CMR10", None, "cm"),
    ("cmr7", "CMR7", None, "cm"),
    ("cmr5", "CMR5", None, "cm"),
    ("msam10", "MSAM10", None, "amsfonts/symbols"),
    ("msam7", "MSAM7", None, "amsfonts/symbols"),
    ("msam5", "MSAM5", None, "amsfonts/symbols"),
    ("msbm10", "MSBM10", None, "amsfonts/symbols"),
    ("msbm7", "MSBM7", None, "amsfonts/symbols"),
    ("msbm5", "MSBM5", None, "amsfonts/symbols"),
    # The text italic's italic corrections, for the `\/` LaTeX's \textit and
    # \emph append (\check@icr); the Reference's T1 ecti1000 agrees within
    # 3e-5 em on every letter and digit.
    ("cmti10", "CMTI10", None, "cm"),
]

STOP_FLAG = 128
KERN_FLAG = 128
LIG_TAG = 1


def fix(word: bytes) -> float:
    return struct.unpack(">i", word)[0] / 2**20


def parse_tfm(data: bytes):
    lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np_ = struct.unpack(">12H", data[:24])
    if lf * 4 != len(data):
        raise SystemExit(f"TFM length {len(data)} does not match lf={lf}")
    words = [data[i : i + 4] for i in range(24, len(data), 4)]
    at = lh
    char_info = words[at : at + ec - bc + 1]
    at += ec - bc + 1
    width = [fix(w) for w in words[at : at + nw]]
    at += nw
    height = [fix(w) for w in words[at : at + nh]]
    at += nh
    depth = [fix(w) for w in words[at : at + nd]]
    at += nd
    italic = [fix(w) for w in words[at : at + ni]]
    at += ni
    lig_kern = [tuple(w) for w in words[at : at + nl]]
    at += nl
    kern = [fix(w) for w in words[at : at + nk]]
    at += nk + ne
    params = [fix(w) for w in words[at : at + np_]]
    chars = {}
    kerns = {}
    for offset, info in enumerate(char_info):
        code = bc + offset
        wi, hd, it, rem = info
        if wi == 0:
            continue  # no such character
        tag = it & 3
        chars[code] = (width[wi], height[hd >> 4], depth[hd & 15], italic[it >> 2])
        if tag != LIG_TAG:
            continue
        i = rem
        first = lig_kern[i]
        if first[0] > STOP_FLAG:
            i = 256 * first[2] + first[3]
        seen = set()
        while True:
            skip, nxt, op, r = lig_kern[i]
            if skip <= STOP_FLAG and nxt not in seen:
                seen.add(nxt)
                if op >= KERN_FLAG:
                    kerns[(code, nxt)] = kern[256 * (op - KERN_FLAG) + r]
            if skip >= STOP_FLAG:
                break
            i += skip + 1
    return params, chars, kerns


def lit(x: float) -> str:
    text = repr(x)
    if "e" in text or "E" in text:
        raise SystemExit(f"unexpected exponent literal {text}")
    if "." not in text:
        text += ".0"
    return text


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    root = Path(sys.argv[1])
    out = [
        "//! TeX's font metrics for the Computer Modern and AMS faces fmd-math sets",
        "//! mathematics in (generated; do not edit by hand).",
        "//!",
        "//! Written by `scripts/font-oracles/extract-cm-tfm.py` from TeX Live's",
        "//! `fonts/tfm/public` (Knuth's Computer Modern metrics: `cm`, and",
        "//! `amsfonts/cmextra` for cmex7, and `amsfonts/symbols` for AMS).",
        "//! AMS symbol metrics carry the original AMS OFL notice in `OFL-AMS.txt`.",
        "//! Values are the TFM fix_words over 2^20,",
        "//! in ems of each font's design size, so they are exact. Ligatures are",
        "//! omitted: the Reference preamble disables them.",
        "//!",
        "//! Sources (sha256):",
    ]
    bodies = []
    for name, static, skew, directory in FONTS:
        data = (root / directory / f"{name}.tfm").read_bytes()
        out.append(f"//! - `{name}.tfm` {hashlib.sha256(data).hexdigest()}")
        params, chars, kerns = parse_tfm(data)
        space = params[1] if len(params) > 1 else 0.0
        body = [
            "",
            f"/// `{name}.tfm`.",
            "#[rustfmt::skip]",
            f"pub static {static}: TfmFont = TfmFont {{",
            f'    name: "{name}",',
            f"    space: {lit(space)},",
            f"    skew_char: {'None' if skew is None else f'Some(0x{skew:02X})'},",
            f"    params: &[{', '.join(lit(p) for p in params)}],",
            "    chars: &[",
        ]
        for code in range(128):
            c = chars.get(code)
            if c is None:
                body.append(f"        None, // 0x{code:02X}")
            else:
                w, h, d, i = c
                body.append(
                    f"        Some(TfmChar {{ width: {lit(w)}, height: {lit(h)}, "
                    f"depth: {lit(d)}, italic: {lit(i)} }}), // 0x{code:02X}"
                )
        body.append("    ],")
        body.append("    kerns: &[")
        for (left, right), k in sorted(kerns.items()):
            body.append(f"        (0x{left:02X}, 0x{right:02X}, {lit(k)}),")
        body.append("    ],")
        body.append("};")
        bodies.extend(body)
    out += [
        "",
        "/// One character's TFM box, in ems of the font's design size.",
        "#[derive(Clone, Copy, Debug, PartialEq)]",
        "pub struct TfmChar {",
        "    /// Width.",
        "    pub width: f64,",
        "    /// Height above the baseline (negative for characters wholly below it).",
        "    pub height: f64,",
        "    /// Depth below the baseline (negative for characters wholly above it).",
        "    pub depth: f64,",
        "    /// Italic correction.",
        "    pub italic: f64,",
        "}",
        "",
        "/// One TFM file: its characters by code, its kerns and the two values TeX",
        "/// reads beside them.",
        "#[derive(Debug)]",
        "pub struct TfmFont {",
        "    /// The TFM file's name.",
        "    pub name: &'static str,",
        "    /// fontdimen 2, the interword space: zero for cmmi, cmsy and cmex,",
        "    /// which keeps rule 17's italic correction inside a run of letters.",
        "    pub space: f64,",
        "    /// The skewchar LaTeX assigns the font, if any.",
        "    pub skew_char: Option<u8>,",
        "    /// The fontdimens: `params[n - 1]` is fontdimen n, in ems of the",
        "    /// design size (fontdimen 1, the slant, is a plain ratio).",
        "    pub params: &'static [f64],",
        "    /// Codes 0-127.",
        "    pub chars: &'static [Option<TfmChar>; 128],",
        "    /// The lig/kern program's kerns, as `(left, right, kern)` sorted by pair.",
        "    pub kerns: &'static [(u8, u8, f64)],",
        "}",
        "",
        "impl TfmFont {",
        "    /// The character at `code`, if the font has it.",
        "    #[must_use]",
        "    pub fn char(&self, code: u8) -> Option<TfmChar> {",
        "        self.chars.get(usize::from(code)).copied().flatten()",
        "    }",
        "",
        "    /// Fontdimen `n` (1-based, as TeX numbers them), in ems of the",
        "    /// design size.",
        "    ///",
        "    /// # Panics",
        "    /// If the font has fewer than `n` fontdimens: a typo in a parameter",
        "    /// table, never input data.",
        "    #[must_use]",
        "    pub const fn param(&self, n: usize) -> f64 {",
        "        self.params[n - 1]",
        "    }",
        "",
        "    /// The kern the lig/kern program places between `left` and `right`.",
        "    #[must_use]",
        "    pub fn kern(&self, left: u8, right: u8) -> Option<f64> {",
        "        self.kerns",
        "            .binary_search_by(|&(l, r, _)| (l, r).cmp(&(left, right)))",
        "            .ok()",
        "            .map(|i| self.kerns[i].2)",
        "    }",
        "",
        "    /// An accent's skew over the character at `code`: its kern with the",
        "    /// skewchar (tex.web §741), zero without one.",
        "    #[must_use]",
        "    pub fn skew(&self, code: u8) -> f64 {",
        "        self.skew_char",
        "            .and_then(|skew| self.kern(code, skew))",
        "            .unwrap_or(0.0)",
        "    }",
        "}",
    ]
    out += bodies
    OUT.write_text("\n".join(out) + "\n")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
