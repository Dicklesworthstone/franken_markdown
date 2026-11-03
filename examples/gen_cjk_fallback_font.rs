//! Regenerate the optional Noto Sans SC CJK fallback face.
//!
//! Instances the pinned Google Fonts TrueType source at Regular (wght=400),
//! then retains its BMP Han, kana, CJK punctuation, and fullwidth forms.
//! This is a Unicode-range subset, not a frequency-ranked character list.
//! Korean Hangul, supplementary-plane Han, and locale-specific Japanese glyph
//! substitutions are outside this Simplified-Chinese face's profile.
//!
//! Source revision, SHA-256, license, coverage, and reproduction instructions:
//! `fmd-font/fonts/noto-sans-cjk/README.md`.
//!
//! ```text
//! cargo run --no-default-features --example gen_cjk_fallback_font -- \
//!     /path/to/NotoSansSC-wght.ttf fmd-font/fonts/noto-sans-cjk/NotoSansSC-CJK.ttf
//! ```

use fmd_font::Font;
use std::process::ExitCode;

/// Retain every mapped character in these Unicode ranges. Coverage is bounded
/// by the pinned upstream face; unassigned/newer scalars remain unsupported.
const CJK_CURATED_RANGES: &[(u32, u32)] = &[
    (0x3000, 0x303F), // CJK punctuation, iteration marks, ideographic space.
    (0x3040, 0x30FF), // Hiragana and katakana, including combining voiced marks.
    (0x31F0, 0x31FF), // Katakana phonetic extensions.
    (0x3400, 0x4DBF), // CJK Unified Ideographs Extension A.
    (0x4E00, 0x9FFF), // CJK Unified Ideographs.
    (0xF900, 0xFAFF), // CJK compatibility ideographs mapped by the source.
    (0xFE10, 0xFE1F), // Vertical punctuation presentation forms.
    (0xFE30, 0xFE4F), // CJK compatibility punctuation forms.
    (0xFF00, 0xFFEF), // Fullwidth and halfwidth forms mapped by the source.
];

const REQUIRED_CJK: &[char] = &[
    '中', '文', '排', '版', '测', '试', '字', '符', '串', '换', '行', '处', '理', '日', '本', '語',
    '漢', '字', 'あ', 'い', 'う', 'え', 'お', 'か', 'き', 'く', 'け', 'こ', 'ア', 'イ', 'ウ', 'エ',
    'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', '。', '、', '「', '」', '【', '】', '！', '？', '：', '；',
    '々', 'ー', '　',
];

fn run(source_path: &str, output_path: &str) -> Result<(), String> {
    let source_bytes = std::fs::read(source_path)
        .map_err(|e| format!("reading source font {source_path}: {e}"))?;
    let font =
        Font::parse(source_bytes).map_err(|e| format!("parsing source font {source_path}: {e}"))?;
    let font = if font.instance_bounds(*b"wght").is_some() {
        font.instance(400.0)
            .ok_or_else(|| format!("instancing {source_path} at wght 400 failed"))?
    } else {
        font
    };

    let mut curated = Vec::new();
    for &(start, end) in CJK_CURATED_RANGES {
        for cp in start..=end {
            if let Some(c) = char::from_u32(cp) {
                curated.push(c);
            }
        }
    }
    curated.sort_unstable();
    curated.dedup();

    let mut keep = Vec::new();
    let mut skipped = Vec::new();
    for c in curated {
        if font.glyph_index(c) != 0 {
            keep.push(c);
        } else {
            skipped.push(c);
        }
    }

    for &c in REQUIRED_CJK {
        if font.glyph_index(c) == 0 {
            return Err(format!(
                "source font {source_path} lacks required CJK glyph {c:?} (U+{:04X})",
                u32::from(c)
            ));
        }
    }

    let subset = font
        .subset(&keep)
        .ok_or_else(|| format!("subsetting {source_path} failed"))?;

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
            format!(
                " (sample skipped: {:?})",
                skipped.iter().take(10).collect::<String>()
            )
        }
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, source, output] = args.as_slice() else {
        eprintln!("usage: gen_cjk_fallback_font <NotoSansSC-wght.ttf> <output.ttf>");
        return ExitCode::from(64);
    };
    match run(source, output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gen_cjk_fallback_font: {message}");
            ExitCode::FAILURE
        }
    }
}
