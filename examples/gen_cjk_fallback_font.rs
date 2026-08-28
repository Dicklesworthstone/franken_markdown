//! Regenerate the curated CJK fallback face (`fonts/noto-sans-cjk/`).
//!
//! Produces a curated subset of Noto Sans CJK SC for Han, Kana, and Hangul
//! frequency-ranked characters using `Font::subset`.
//!
//! Usage:
//! ```text
//! cargo run --example gen_cjk_fallback_font -- \
//!     /path/to/NotoSansCJKsc-Regular.otf fonts/noto-sans-cjk/NotoSansCJK-Subset.ttf
//! ```

use franken_markdown::text::Font;
use std::process::ExitCode;

/// Curated CJK ranges: Hiragana, Katakana, common CJK Unified Ideographs,
/// Hangul Syllables core, and Fullwidth punctuation.
const CJK_CURATED_RANGES: &[(u32, u32)] = &[
    // Fullwidth ASCII and punctuation
    (0xFF01, 0xFF60),
    // CJK Symbols and Punctuation (、。〈〉《》「」『』【】〔〕)
    (0x3001, 0x301F),
    // Hiragana (あ-ん)
    (0x3041, 0x3096),
    // Katakana (ア-ン)
    (0x30A1, 0x30FA),
    // Common Kanji / Hanzi core (most frequent ~2500 Joyo / Level 1 Simplified)
    (0x4E00, 0x5500),
    (0x5501, 0x6000),
    (0x6001, 0x7000),
    (0x7001, 0x8000),
    (0x8001, 0x9000),
    (0x9001, 0x9FA5),
    // Hangul Syllables core
    (0xAC00, 0xD7A3),
];

/// Non-negotiable required sample codepoints across Han, Kana, Hangul, Fullwidth.
const REQUIRED_CJK: &[char] = &[
    '中', '文', '排', '版', '测', '试', '字', '符', '串', '换', '行', '处', '理',
    'あ', 'い', 'う', 'え', 'お', 'か', 'き', 'く', 'け', 'こ',
    'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ',
    '한', '글', '가', '나', '다', '라', '마', '바', '사', '아', '자', '차', '카', '타', '파', '하',
    '。', '、', '「', '」', '【', '】', '！', '？', '：', '；',
];

fn run(source_path: &str, output_path: &str) -> Result<(), String> {
    let source_bytes = std::fs::read(source_path)
        .map_err(|e| format!("reading source font {source_path}: {e}"))?;
    let font = Font::parse(source_bytes)
        .map_err(|e| format!("parsing source font {source_path}: {e}"))?;

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
            return Err(format!("subset lost coverage for {c:?} (U+{:04X})", u32::from(c)));
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
            format!(" (sample skipped: {:?})", skipped.iter().take(10).collect::<String>())
        }
    );
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(source), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: gen_cjk_fallback_font <NotoSansCJK-Regular.ttf> <output.ttf>");
        return ExitCode::from(64);
    };
    match run(&source, &output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gen_cjk_fallback_font: {message}");
            ExitCode::FAILURE
        }
    }
}
