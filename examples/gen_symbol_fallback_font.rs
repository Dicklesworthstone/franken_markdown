//! Regenerate the bundled symbol-fallback face (`fonts/noto-sans-math/`).
//!
//! The PDF renderer's primary body/mono faces (IBM Plex Sans, Computer Modern)
//! do not cover every common technical glyph — arrows (`⇒`), math operators
//! (`≈`, `≠`, `∑`), the true minus sign (`−`), and so on. Instead of shipping
//! the full ~1 MiB Noto Sans Math face, the repo commits a curated subset
//! produced by this generator with the project's own clean-room subsetter
//! (`Font::subset`), keeping the binary and the WASM bundle small while the
//! per-document PDF embedder subsets it *again* down to the glyphs a document
//! actually uses.
//!
//! Usage:
//!
//! ```text
//! cargo run --example gen_symbol_fallback_font -- \
//!     /path/to/NotoSansMath-Regular.ttf fonts/noto-sans-math/NotoSansMathSymbols.ttf
//! ```
//!
//! Source font: Noto Sans Math (SIL OFL 1.1), e.g. from
//! <https://github.com/google/fonts/tree/main/ofl/notosansmath>. The curated
//! codepoint list below is the single source of truth for what the fallback
//! face covers; regenerating with the same source release is deterministic.

use franken_markdown::text::Font;
use std::process::ExitCode;

/// The curated fallback repertoire: common documentation/technical symbols
/// that Markdown sources use freely but text faces frequently omit.
///
/// Kept as inclusive ranges so review diffs stay readable. Codepoints the
/// source font does not cover are skipped (and reported) rather than failing,
/// but the critical set in `REQUIRED` must always survive.
const CURATED_RANGES: &[(u32, u32)] = &[
    // Latin-1 symbols occasionally missing from text cuts (×, ÷, ±, ¬, °, µ, ·).
    (0x00A2, 0x00A6),
    (0x00AC, 0x00AC),
    (0x00B0, 0x00B1),
    (0x00B2, 0x00B3),
    (0x00B5, 0x00B7),
    (0x00B9, 0x00B9),
    (0x00BC, 0x00BE),
    (0x00D7, 0x00D7),
    (0x00F7, 0x00F7),
    // Double vertical line, daggers, per mille, primes, fraction slash.
    (0x2016, 0x2016),
    (0x2020, 0x2021),
    (0x2030, 0x2030),
    (0x2032, 0x2034),
    (0x2044, 0x2044),
    // Superscripts and subscripts.
    (0x2070, 0x209C),
    // Letterlike symbols (ℂ, ℏ, ℓ, ℕ, ℝ, ℤ, Ω, ℵ, …).
    (0x2100, 0x214F),
    // Number forms / vulgar fractions.
    (0x2150, 0x215F),
    // Arrows — the whole block (→, ←, ↔, ⇐, ⇒, ⇔, ↦, ↯, …).
    (0x2190, 0x21FF),
    // Mathematical operators — the whole block (∀, ∂, ∑, −, ≈, ≠, ≤, ≥, …).
    (0x2200, 0x22FF),
    // Misc technical: ⌀, ceilings/floors, corner quotes, ⌐, return symbol.
    (0x2300, 0x230B),
    (0x2310, 0x2310),
    (0x231C, 0x2321),
    (0x23CE, 0x23CE),
    // Geometric shapes commonly used as markers.
    (0x25A0, 0x25CF),
    (0x25E6, 0x25E6),
    // Stars, ballot boxes, check/cross marks.
    (0x2605, 0x2606),
    (0x2610, 0x2612),
    (0x2713, 0x2718),
    // Misc math brackets and perpendicular.
    (0x27C2, 0x27C2),
    (0x27E6, 0x27EF),
    // Supplemental arrows A (long arrows ⟵, ⟶, ⟹, …).
    (0x27F0, 0x27FF),
    // Frequent n-ary/supplemental operators (⨀, ⨁, ⨂, ⨯, ⩽, ⩾).
    (0x2A00, 0x2A06),
    (0x2A2F, 0x2A2F),
    (0x2A7D, 0x2A7E),
];

/// Codepoints the generated face must cover; the generator fails if the source
/// font cannot provide them. These are exactly the glyphs real-world issues
/// reported as `.notdef` boxes plus the operators any technical doc leans on.
const REQUIRED: &[char] = &[
    '×', '÷', '±', '°', '·', '−', '→', '←', '↔', '⇐', '⇒', '⇔', '≈', '≠', '≡', '≤', '≥', '∑', '∏',
    '√', '∞', '∫', '∂', '∈', '∉', '∅', '∧', '∨', '⊂', '⊃', '⊕', '⊗',
];

fn curated_codepoints() -> Vec<char> {
    let mut out = Vec::new();
    for &(start, end) in CURATED_RANGES {
        for cp in start..=end {
            if let Some(c) = char::from_u32(cp) {
                out.push(c);
            }
        }
    }
    out
}

fn run(source_path: &str, output_path: &str) -> Result<(), String> {
    let bytes = std::fs::read(source_path).map_err(|e| format!("reading {source_path}: {e}"))?;
    let font = Font::parse(bytes).map_err(|e| format!("parsing {source_path}: {e}"))?;

    let curated = curated_codepoints();
    let mut keep = Vec::with_capacity(curated.len());
    let mut skipped = Vec::new();
    for c in curated {
        if font.glyph_index(c) != 0 {
            keep.push(c);
        } else {
            skipped.push(c);
        }
    }
    for &c in REQUIRED {
        if font.glyph_index(c) == 0 {
            return Err(format!(
                "source font {source_path} lacks required fallback glyph {c:?} (U+{:04X})",
                u32::from(c)
            ));
        }
    }

    let subset = font
        .subset(&keep)
        .ok_or_else(|| format!("subsetting {source_path} failed"))?;

    // Prove the artifact round-trips through the project's own reader and that
    // every kept character still resolves before committing bytes to disk.
    let reparsed = Font::parse(subset.clone()).map_err(|e| format!("re-parsing subset: {e}"))?;
    for &c in &keep {
        if reparsed.glyph_index(c) == 0 {
            return Err(format!(
                "subset lost coverage for {c:?} (U+{:04X})",
                u32::from(c)
            ));
        }
    }

    std::fs::write(output_path, &subset).map_err(|e| format!("writing {output_path}: {e}"))?;
    println!(
        "wrote {output_path}: {} bytes, {} chars kept, {} curated codepoints unavailable in source{}",
        subset.len(),
        keep.len(),
        skipped.len(),
        if skipped.is_empty() {
            String::new()
        } else {
            format!(" ({})", skipped.iter().collect::<String>())
        }
    );
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(source), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: gen_symbol_fallback_font <NotoSansMath-Regular.ttf> <output.ttf>");
        return ExitCode::from(64);
    };
    match run(&source, &output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gen_symbol_fallback_font: {message}");
            ExitCode::FAILURE
        }
    }
}
