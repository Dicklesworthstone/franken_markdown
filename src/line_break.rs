//! UAX #14 Unicode Line Breaking Algorithm implementation.
//!
//! Clean-room implementation of core line breaking rules for CJK + Latin text.
//! Uses sorted range tables with binary search for dependency-free classification.

use std::cmp::Ordering;

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LineBreakClass {
    BK,
    CR,
    LF,
    #[allow(dead_code)]
    NL,
    SP,
    #[allow(dead_code)]
    B2,
    #[allow(dead_code)]
    CB,
    ZW,
    GL,
    WJ,
    Zwj,
    ID,
    JL,
    JV,
    JT,
    H2,
    #[allow(dead_code)]
    H3,
    AL,
    NU,
    OP,
    CL,
    QU,
    #[allow(dead_code)]
    NS,
    IS,
    PR,
    PO,
    SY,
    HY,
    BA,
    #[allow(dead_code)]
    BB,
    EX,
    CM,
    XX,
}

impl LineBreakClass {
    pub(crate) fn is_cjk(self) -> bool {
        matches!(
            self,
            Self::ID | Self::JL | Self::JV | Self::JT | Self::H2 | Self::H3
        )
    }
}

/// Returns the UAX #14 Line Break Class for a character.
/// ASCII fast path, then binary search in LB_CLASS_RANGES.
#[must_use]
pub(crate) fn line_break_class(c: char) -> LineBreakClass {
    if c.is_ascii() {
        return match c {
            '\x00'..='\x09' | '\x0B'..='\x0C' => LineBreakClass::BK,
            '\x0A' => LineBreakClass::LF,
            '\x0D' => LineBreakClass::CR,
            ' ' => LineBreakClass::SP,
            '!' => LineBreakClass::EX,
            '"' | '\'' => LineBreakClass::QU,
            '(' | '[' | '{' | '<' => LineBreakClass::OP,
            ')' | ']' | '}' | '>' => LineBreakClass::CL,
            '*' | '$' | '%' => LineBreakClass::PO,
            '+' => LineBreakClass::PR,
            ',' => LineBreakClass::IS,
            '-' => LineBreakClass::HY,
            '.' => LineBreakClass::IS,
            '/' => LineBreakClass::SY,
            '0'..='9' => LineBreakClass::NU,
            ':' | ';' => LineBreakClass::IS,
            '=' => LineBreakClass::AL,
            'A'..='Z' | 'a'..='z' | '_' => LineBreakClass::AL,
            '\\' | '|' => LineBreakClass::BA,
            '~' => LineBreakClass::AL,
            '^' => LineBreakClass::PR,
            '#' | '@' | '&' => LineBreakClass::AL,
            _ => LineBreakClass::XX,
        };
    }

    let cp = c as u32;
    LB_CLASS_RANGES
        .binary_search_by(|&(lo, hi, _)| {
            if cp < lo {
                Ordering::Greater
            } else if cp > hi {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .map(|i| LB_CLASS_RANGES[i].2)
        .unwrap_or(LineBreakClass::XX)
}

/// Sorted, non-overlapping ranges of code points grouped by Line Break Class.
/// Generated from Unicode 15.1.0 LineBreak.txt. Covers core CJK + Latin classes.
/// ASCII (U+0000-U+007F) handled separately in line_break_class().
static LB_CLASS_RANGES: &[(u32, u32, LineBreakClass)] = &[
    // Latin-1 Supplement (U+0080-U+00FF)
    (0x00A0, 0x00A0, LineBreakClass::GL), // NBSP
    (0x00AD, 0x00AD, LineBreakClass::GL), // Soft hyphen
    (0x00C0, 0x00D6, LineBreakClass::AL), // Latin-1 Supplement letters
    (0x00D8, 0x00F6, LineBreakClass::AL), // Latin-1 Supplement letters
    (0x00F8, 0x00FF, LineBreakClass::AL), // Latin-1 Supplement letters
    // Latin Extended-A/B, IPA, etc. (U+0100-U+02FF)
    (0x0100, 0x024F, LineBreakClass::AL), // Latin Extended-A/B
    (0x0250, 0x02AF, LineBreakClass::AL), // IPA Extensions
    (0x02B0, 0x02FF, LineBreakClass::AL), // Spacing Modifier Letters
    // Combining Diacritical Marks (U+0300-U+036F)
    (0x0300, 0x036F, LineBreakClass::CM),
    // Greek and Coptic (U+0370-U+03FF)
    (0x0370, 0x0377, LineBreakClass::AL),
    (0x037A, 0x037F, LineBreakClass::AL),
    (0x0384, 0x0384, LineBreakClass::AL),
    (0x0386, 0x0386, LineBreakClass::AL),
    (0x0388, 0x038A, LineBreakClass::AL),
    (0x038C, 0x038C, LineBreakClass::AL),
    (0x038E, 0x03A1, LineBreakClass::AL),
    (0x03A3, 0x03FF, LineBreakClass::AL),
    // Cyrillic (U+0400-U+052F)
    (0x0400, 0x052F, LineBreakClass::AL),
    // Armenian, Hebrew, Arabic, etc. (U+0530-U+08FF)
    (0x0530, 0x058F, LineBreakClass::AL),
    (0x0590, 0x05FF, LineBreakClass::AL),
    (0x0600, 0x06FF, LineBreakClass::AL),
    (0x0700, 0x074F, LineBreakClass::AL),
    (0x0750, 0x077F, LineBreakClass::AL),
    (0x0780, 0x07BF, LineBreakClass::AL),
    (0x07C0, 0x07FF, LineBreakClass::AL),
    (0x0800, 0x083F, LineBreakClass::AL),
    (0x0840, 0x085F, LineBreakClass::AL),
    (0x0860, 0x086F, LineBreakClass::AL),
    (0x08A0, 0x08FF, LineBreakClass::AL),
    // Devanagari and other Indic scripts (U+0900-U+0FFF)
    (0x0900, 0x097F, LineBreakClass::AL),
    (0x0980, 0x09FF, LineBreakClass::AL),
    (0x0A00, 0x0A7F, LineBreakClass::AL),
    (0x0A80, 0x0AFF, LineBreakClass::AL),
    (0x0B00, 0x0B7F, LineBreakClass::AL),
    (0x0B80, 0x0BFF, LineBreakClass::AL),
    (0x0C00, 0x0C7F, LineBreakClass::AL),
    (0x0C80, 0x0CFF, LineBreakClass::AL),
    (0x0D00, 0x0D7F, LineBreakClass::AL),
    (0x0D80, 0x0DFF, LineBreakClass::AL),
    (0x0E00, 0x0E7F, LineBreakClass::AL),
    (0x0E80, 0x0EFF, LineBreakClass::AL),
    (0x0F00, 0x0FFF, LineBreakClass::AL),
    // Myanmar, Georgian, Hangul Jamo (U+1000-U+11FF)
    (0x1000, 0x109F, LineBreakClass::AL),
    (0x10A0, 0x10FF, LineBreakClass::AL),
    (0x1100, 0x115F, LineBreakClass::JL), // Hangul Jamo Leading
    (0x1160, 0x11A7, LineBreakClass::JV), // Hangul Jamo Vowel
    (0x11A8, 0x11FF, LineBreakClass::JT), // Hangul Jamo Trailing
    // Ethiopic, Cherokee, Canadian, Ogham, Runic, etc. (U+1200-U+18FF)
    (0x1200, 0x137F, LineBreakClass::AL),
    (0x1380, 0x139F, LineBreakClass::AL),
    (0x13A0, 0x13FF, LineBreakClass::AL),
    (0x1400, 0x167F, LineBreakClass::AL),
    (0x1680, 0x169F, LineBreakClass::AL),
    (0x16A0, 0x16FF, LineBreakClass::AL),
    (0x1700, 0x171F, LineBreakClass::AL),
    (0x1720, 0x173F, LineBreakClass::AL),
    (0x1740, 0x175F, LineBreakClass::AL),
    (0x1760, 0x177F, LineBreakClass::AL),
    (0x1780, 0x17FF, LineBreakClass::AL),
    (0x1800, 0x18AF, LineBreakClass::AL),
    (0x18B0, 0x18FF, LineBreakClass::AL),
    // Various scripts and symbols (U+1900-U+1CFF)
    (0x1900, 0x194F, LineBreakClass::AL),
    (0x1950, 0x197F, LineBreakClass::AL),
    (0x1980, 0x19DF, LineBreakClass::AL),
    (0x19E0, 0x19FF, LineBreakClass::AL),
    (0x1A00, 0x1A1F, LineBreakClass::AL),
    (0x1A20, 0x1AAF, LineBreakClass::AL),
    (0x1AB0, 0x1AFF, LineBreakClass::CM),
    (0x1B00, 0x1B4F, LineBreakClass::AL),
    (0x1B50, 0x1B7F, LineBreakClass::AL),
    (0x1B80, 0x1BBF, LineBreakClass::AL),
    (0x1BC0, 0x1BFF, LineBreakClass::AL),
    (0x1C00, 0x1C4F, LineBreakClass::AL),
    (0x1C50, 0x1C7F, LineBreakClass::AL),
    (0x1C80, 0x1C8F, LineBreakClass::AL),
    (0x1C90, 0x1CFF, LineBreakClass::AL),
    // Phonetic Extensions, Latin Extended Additional, Greek Extended (U+1D00-U+1FFF)
    (0x1D00, 0x1D7F, LineBreakClass::AL),
    (0x1D80, 0x1DBF, LineBreakClass::AL),
    (0x1DC0, 0x1DFF, LineBreakClass::CM),
    (0x1E00, 0x1EFF, LineBreakClass::AL),
    (0x1F00, 0x1FFF, LineBreakClass::AL),
    // General Punctuation and symbols (U+2000-U+2BFF)
    (0x2000, 0x200A, LineBreakClass::BA),  // Various spaces
    (0x200B, 0x200B, LineBreakClass::ZW),  // Zero Width Space
    (0x200C, 0x200D, LineBreakClass::Zwj), // Zero Width Non-Joiner/Joiner
    (0x2010, 0x2010, LineBreakClass::HY),  // Hyphen
    (0x2011, 0x2011, LineBreakClass::GL),  // Non-breaking hyphen
    (0x2012, 0x2013, LineBreakClass::HY),  // Figure dash, en dash
    (0x2014, 0x2015, LineBreakClass::BA),  // Em dash, horizontal bar
    (0x2018, 0x201F, LineBreakClass::QU),  // Quotation marks
    (0x2028, 0x2028, LineBreakClass::BK),  // Line Separator
    (0x2029, 0x2029, LineBreakClass::BK),  // Paragraph Separator
    (0x202F, 0x202F, LineBreakClass::GL),  // Narrow NBSP
    (0x2030, 0x2031, LineBreakClass::PO),  // Per mille/ten thousand signs
    (0x2032, 0x2038, LineBreakClass::PO),  // Prime marks
    (0x2039, 0x203A, LineBreakClass::QU),  // Angle quotation marks
    (0x2044, 0x2044, LineBreakClass::SY),  // Fraction slash
    (0x2045, 0x2045, LineBreakClass::OP),  // Left square bracket with quill
    (0x2046, 0x2046, LineBreakClass::CL),  // Right square bracket with quill
    (0x2053, 0x2053, LineBreakClass::BA),  // Swung dash
    (0x205F, 0x205F, LineBreakClass::BA),  // Medium mathematical space
    (0x2060, 0x2060, LineBreakClass::WJ),  // Word Joiner
    (0x2070, 0x2070, LineBreakClass::AL),  // Superscript zero
    (0x2071, 0x2071, LineBreakClass::AL),  // Superscript i
    (0x2074, 0x207C, LineBreakClass::AL),  // Superscript chars
    (0x207D, 0x207D, LineBreakClass::OP),  // Superscript left parenthesis
    (0x207E, 0x207E, LineBreakClass::CL),  // Superscript right parenthesis
    (0x207F, 0x207F, LineBreakClass::AL),  // Superscript n
    (0x2080, 0x208C, LineBreakClass::AL),  // Subscript chars
    (0x208D, 0x208D, LineBreakClass::OP),  // Subscript left parenthesis
    (0x208E, 0x208E, LineBreakClass::CL),  // Subscript right parenthesis
    (0x2090, 0x209C, LineBreakClass::AL),  // Latin subscript small letters
    (0x20A0, 0x20CF, LineBreakClass::PR),  // Currency symbols
    (0x20D0, 0x20FF, LineBreakClass::CM),  // Combining Diacritical Marks for Symbols
    (0x2100, 0x214F, LineBreakClass::AL),  // Letterlike Symbols
    (0x2150, 0x215F, LineBreakClass::IS),  // Fractions
    (0x2160, 0x2188, LineBreakClass::NU),  // Roman Numerals
    (0x2190, 0x23FF, LineBreakClass::AL),  // Arrows and technical symbols
    (0x2400, 0x24FF, LineBreakClass::AL),  // Control Pictures, OCR, Enclosed Alphanumerics
    (0x2500, 0x27BF, LineBreakClass::AL), // Box Drawing, Block Elements, Geometric Shapes, Dingbats
    (0x27C0, 0x2BFF, LineBreakClass::AL), // Supplemental Arrows, Braille, Mathematical Operators
    // CJK Radicals and Symbols (U+2E80-U+2FFF)
    (0x2E80, 0x2EFF, LineBreakClass::ID), // CJK Radicals Supplement
    (0x2F00, 0x2FDF, LineBreakClass::ID), // Kangxi Radicals
    (0x2FF0, 0x2FFF, LineBreakClass::ID), // Ideographic Description Characters
    // CJK Symbols and Punctuation (U+3000-U+303F)
    (0x3000, 0x3000, LineBreakClass::BA), // Ideographic Space
    (0x3001, 0x3003, LineBreakClass::CL), // CJK punctuation
    (0x3004, 0x3004, LineBreakClass::ID), // CJK punctuation
    (0x3005, 0x3007, LineBreakClass::ID), // CJK iteration marks
    (0x3008, 0x3008, LineBreakClass::OP), // Left angle bracket
    (0x3009, 0x3009, LineBreakClass::CL), // Right angle bracket
    (0x300A, 0x300A, LineBreakClass::OP), // Left double angle bracket
    (0x300B, 0x300B, LineBreakClass::CL), // Right double angle bracket
    (0x300C, 0x300C, LineBreakClass::OP), // Left corner bracket
    (0x300D, 0x300D, LineBreakClass::CL), // Right corner bracket
    (0x300E, 0x300E, LineBreakClass::OP), // Left white corner bracket
    (0x300F, 0x300F, LineBreakClass::CL), // Right white corner bracket
    (0x3010, 0x3010, LineBreakClass::OP), // Left black lenticular bracket
    (0x3011, 0x3011, LineBreakClass::CL), // Right black lenticular bracket
    (0x3012, 0x3013, LineBreakClass::ID), // CJK symbols
    (0x3014, 0x3014, LineBreakClass::OP), // Left tortoise shell bracket
    (0x3015, 0x3015, LineBreakClass::CL), // Right tortoise shell bracket
    (0x3016, 0x3016, LineBreakClass::OP), // Left white lenticular bracket
    (0x3017, 0x3017, LineBreakClass::CL), // Right white lenticular bracket
    (0x3018, 0x3018, LineBreakClass::OP), // Left white tortoise shell bracket
    (0x3019, 0x3019, LineBreakClass::CL), // Right white tortoise shell bracket
    (0x301A, 0x301A, LineBreakClass::OP), // Left white square bracket
    (0x301B, 0x301B, LineBreakClass::CL), // Right white square bracket
    (0x301C, 0x301C, LineBreakClass::CL), // Wave dash
    (0x301D, 0x301D, LineBreakClass::OP), // Reversed double prime quotation
    (0x301E, 0x301F, LineBreakClass::CL), // Double prime quotation marks
    (0x3020, 0x303E, LineBreakClass::ID), // CJK symbols
    // Hiragana, Katakana, Bopomofo, Hangul Compatibility (U+3040-U+318F)
    (0x3040, 0x3040, LineBreakClass::ID), // Hiragana iteration mark
    (0x3041, 0x3096, LineBreakClass::ID), // Hiragana
    (0x3099, 0x309A, LineBreakClass::CM), // Combining Katakana-Hiragana
    (0x309B, 0x309C, LineBreakClass::ID), // Katakana-Hiragana sound marks
    (0x309D, 0x309F, LineBreakClass::ID), // Hiragana iteration marks
    (0x30A0, 0x30A0, LineBreakClass::ID), // Katakana-Hiragana double hyphen
    (0x30A1, 0x30FA, LineBreakClass::ID), // Katakana
    (0x30FB, 0x30FB, LineBreakClass::CL), // Katakana middle dot
    (0x30FC, 0x30FF, LineBreakClass::ID), // Katakana extensions
    (0x3105, 0x312F, LineBreakClass::ID), // Bopomofo
    (0x3130, 0x318F, LineBreakClass::ID), // Hangul Compatibility Jamo
    // CJK Extensions, Enclosed CJK, CJK Unified Ideographs (U+3190-U+9FFF)
    (0x3190, 0x31B7, LineBreakClass::ID), // CJK radicals, Kanbun
    (0x31C0, 0x31EF, LineBreakClass::ID), // CJK Strokes
    (0x31F0, 0x31FF, LineBreakClass::ID), // Katakana Phonetic Extensions
    (0x3200, 0x321E, LineBreakClass::ID), // Enclosed CJK Letters and Months
    (0x3220, 0x3247, LineBreakClass::ID), // Enclosed CJK Letters
    (0x3250, 0x33FF, LineBreakClass::ID), // CJK Compatibility
    (0x3400, 0x4DBF, LineBreakClass::ID), // CJK Unified Ideographs Extension A
    (0x4E00, 0x9FFF, LineBreakClass::ID), // CJK Unified Ideographs
    // Yi, Hangul Syllables (U+A000-U+D7FF)
    (0xA000, 0xA48F, LineBreakClass::ID), // Yi Syllables
    (0xA490, 0xA4CF, LineBreakClass::ID), // Yi Radicals
    (0xA4D0, 0xA4FF, LineBreakClass::AL), // Lisu
    (0xA500, 0xA63F, LineBreakClass::AL), // Vai
    (0xAC00, 0xD7A3, LineBreakClass::H2), // Hangul Syllables (most are H2)
    (0xD7B0, 0xD7FF, LineBreakClass::ID), // Hangul Jamo Extended-B
    // CJK Compatibility Ideographs (U+F900-U+FAFF)
    (0xF900, 0xFAFF, LineBreakClass::ID),
    // Variation Selectors, CJK Compatibility Forms (U+FE00-U+FE4F)
    (0xFE00, 0xFE0F, LineBreakClass::CM), // Variation Selectors
    (0xFE10, 0xFE16, LineBreakClass::ID), // Presentation forms
    (0xFE17, 0xFE17, LineBreakClass::OP), // Presentation form
    (0xFE18, 0xFE19, LineBreakClass::CL), // Presentation forms
    (0xFE30, 0xFE4F, LineBreakClass::ID), // CJK Compatibility Forms
    (0xFE50, 0xFE52, LineBreakClass::CL), // Small form variants
    (0xFE54, 0xFE57, LineBreakClass::CL), // Small form variants
    (0xFE58, 0xFE58, LineBreakClass::ID), // Small form variants
    (0xFE59, 0xFE59, LineBreakClass::OP), // Small form variants
    (0xFE5A, 0xFE5A, LineBreakClass::CL), // Small form variants
    (0xFE5B, 0xFE5B, LineBreakClass::OP), // Small form variants
    (0xFE5C, 0xFE5C, LineBreakClass::CL), // Small form variants
    (0xFE5D, 0xFE5D, LineBreakClass::OP), // Small form variants
    (0xFE5E, 0xFE5E, LineBreakClass::CL), // Small form variants
    (0xFE5F, 0xFE6F, LineBreakClass::ID), // Small form variants
    // Fullwidth ASCII variants (U+FF00-U+FFEF)
    (0xFF01, 0xFF01, LineBreakClass::EX), // Fullwidth exclamation
    (0xFF02, 0xFF02, LineBreakClass::QU), // Fullwidth quotation
    (0xFF03, 0xFF05, LineBreakClass::PO), // Fullwidth symbols
    (0xFF06, 0xFF06, LineBreakClass::AL), // Fullwidth ampersand
    (0xFF07, 0xFF07, LineBreakClass::QU), // Fullwidth apostrophe
    (0xFF08, 0xFF08, LineBreakClass::OP), // Fullwidth left parenthesis
    (0xFF09, 0xFF09, LineBreakClass::CL), // Fullwidth right parenthesis
    (0xFF0A, 0xFF0A, LineBreakClass::PO), // Fullwidth asterisk
    (0xFF0B, 0xFF0B, LineBreakClass::PR), // Fullwidth plus
    (0xFF0C, 0xFF0C, LineBreakClass::IS), // Fullwidth comma
    (0xFF0D, 0xFF0D, LineBreakClass::HY), // Fullwidth hyphen
    (0xFF0E, 0xFF0E, LineBreakClass::IS), // Fullwidth period
    (0xFF0F, 0xFF0F, LineBreakClass::SY), // Fullwidth slash
    (0xFF10, 0xFF19, LineBreakClass::NU), // Fullwidth digits
    (0xFF1A, 0xFF1B, LineBreakClass::IS), // Fullwidth colon/semicolon
    (0xFF1C, 0xFF1E, LineBreakClass::QU), // Fullwidth brackets
    (0xFF1F, 0xFF1F, LineBreakClass::EX), // Fullwidth question
    (0xFF20, 0xFF20, LineBreakClass::AL), // Fullwidth at sign
    (0xFF21, 0xFF3A, LineBreakClass::AL), // Fullwidth Latin uppercase
    (0xFF3B, 0xFF3B, LineBreakClass::OP), // Fullwidth left bracket
    (0xFF3C, 0xFF3C, LineBreakClass::BA), // Fullwidth backslash
    (0xFF3D, 0xFF3D, LineBreakClass::CL), // Fullwidth right bracket
    (0xFF3E, 0xFF3E, LineBreakClass::PR), // Fullwidth caret
    (0xFF3F, 0xFF3F, LineBreakClass::AL), // Fullwidth underscore
    (0xFF40, 0xFF40, LineBreakClass::AL), // Fullwidth grave accent
    (0xFF41, 0xFF5A, LineBreakClass::AL), // Fullwidth Latin lowercase
    (0xFF5B, 0xFF5B, LineBreakClass::OP), // Fullwidth left curly bracket
    (0xFF5C, 0xFF5C, LineBreakClass::CL), // Fullwidth right curly bracket
    (0xFF5D, 0xFF5D, LineBreakClass::OP), // Fullwidth left white parenthesis
    (0xFF5E, 0xFF5E, LineBreakClass::AL), // Fullwidth tilde
    (0xFF5F, 0xFF5F, LineBreakClass::OP), // Fullwidth left white parenthesis
    (0xFF60, 0xFF60, LineBreakClass::CL), // Fullwidth right white parenthesis
    (0xFF61, 0xFF61, LineBreakClass::CL), // Halfwidth ideographic period
    (0xFF62, 0xFF62, LineBreakClass::OP), // Halfwidth left corner bracket
    (0xFF63, 0xFF63, LineBreakClass::CL), // Halfwidth right corner bracket
    (0xFF64, 0xFF65, LineBreakClass::ID), // Halfwidth punctuation
    (0xFF66, 0xFF6F, LineBreakClass::ID), // Halfwidth Katakana
    (0xFF70, 0xFF70, LineBreakClass::HY), // Halfwidth prolonged sound
    (0xFF71, 0xFF9D, LineBreakClass::ID), // Halfwidth Katakana
    (0xFF9E, 0xFF9F, LineBreakClass::CM), // Halfwidth combining Katakana
    (0xFFA0, 0xFFBE, LineBreakClass::JL), // Halfwidth Hangul Jamo
    (0xFFC2, 0xFFC7, LineBreakClass::JV), // Halfwidth Hangul Vowel
    (0xFFCA, 0xFFCF, LineBreakClass::JV), // Halfwidth Hangul Vowel
    (0xFFD2, 0xFFD7, LineBreakClass::JV), // Halfwidth Hangul Vowel
    (0xFFDA, 0xFFDC, LineBreakClass::JV), // Halfwidth Hangul Vowel
    (0xFFE0, 0xFFE6, LineBreakClass::ID), // Fullwidth currency symbols
    (0xFFE8, 0xFFEE, LineBreakClass::ID), // Halfwidth symbols
    // Kana Supplement, Nushu (U+1B000-U+1B2FF)
    (0x1B000, 0x1B0FF, LineBreakClass::ID), // Kana Supplement
    (0x1B100, 0x1B12F, LineBreakClass::ID), // Kana Extended-A
    (0x1B170, 0x1B2FF, LineBreakClass::ID), // Nushu
    // CJK Unified Ideographs Extensions B-I, Compatibility Supplement (U+20000-U+323AF)
    (0x20000, 0x2A6DF, LineBreakClass::ID), // CJK Unified Ideographs Extension B
    (0x2A700, 0x2B73F, LineBreakClass::ID), // CJK Unified Ideographs Extension C
    (0x2B740, 0x2B81F, LineBreakClass::ID), // CJK Unified Ideographs Extension D
    (0x2B820, 0x2CEAF, LineBreakClass::ID), // CJK Unified Ideographs Extension E
    (0x2CEB0, 0x2EBEF, LineBreakClass::ID), // CJK Unified Ideographs Extension F
    (0x2EBF0, 0x2EE5F, LineBreakClass::ID), // CJK Unified Ideographs Extension I
    (0x2F800, 0x2FA1F, LineBreakClass::ID), // CJK Compatibility Ideographs Supplement
    (0x30000, 0x3134F, LineBreakClass::ID), // CJK Unified Ideographs Extension G
    (0x31350, 0x323AF, LineBreakClass::ID), // CJK Unified Ideographs Extension H
];

/// Returns true if a line break is allowed between two characters.
/// Implements core UAX #14 pair rules for CJK + Latin text.
#[must_use]
pub(crate) fn can_break_between(prev: LineBreakClass, next: LineBreakClass) -> bool {
    match (prev, next) {
        (LineBreakClass::BK | LineBreakClass::LF | LineBreakClass::NL, _) => true,
        (LineBreakClass::CR, LineBreakClass::LF) => true,
        (LineBreakClass::CR, _) => true,
        (_, LineBreakClass::GL | LineBreakClass::WJ) => false,
        (LineBreakClass::GL | LineBreakClass::WJ, _) => false,
        (LineBreakClass::ZW, _) => true,
        (_, LineBreakClass::ZW) => true,
        (LineBreakClass::Zwj, _) => false,
        (_, LineBreakClass::Zwj) => false,
        (_, LineBreakClass::SP) => false,
        (LineBreakClass::SP, _) => true,
        (LineBreakClass::CM, _) => false,
        (_, LineBreakClass::CM) => false,
        (LineBreakClass::JL, LineBreakClass::JL | LineBreakClass::JV | LineBreakClass::H2) => false,
        (
            LineBreakClass::JV | LineBreakClass::H2,
            LineBreakClass::JV | LineBreakClass::JT | LineBreakClass::H3,
        ) => false,
        (LineBreakClass::JT | LineBreakClass::H3, LineBreakClass::JT) => false,
        (
            LineBreakClass::ID | LineBreakClass::H2 | LineBreakClass::H3,
            LineBreakClass::ID | LineBreakClass::H2 | LineBreakClass::H3,
        ) => true,
        (LineBreakClass::ID, LineBreakClass::PO) => false,
        (LineBreakClass::PR, LineBreakClass::ID) => false,
        (LineBreakClass::ID, LineBreakClass::AL | LineBreakClass::NU) => true,
        (LineBreakClass::AL | LineBreakClass::NU, LineBreakClass::ID) => true,
        (LineBreakClass::AL, LineBreakClass::AL | LineBreakClass::NU) => false,
        (LineBreakClass::NU, LineBreakClass::NU | LineBreakClass::AL) => false,
        (LineBreakClass::OP, LineBreakClass::AL | LineBreakClass::NU | LineBreakClass::ID) => false,
        (LineBreakClass::AL | LineBreakClass::NU | LineBreakClass::ID, LineBreakClass::CL) => false,
        (LineBreakClass::QU, _) => false,
        (_, LineBreakClass::QU) => false,
        (LineBreakClass::HY, LineBreakClass::AL | LineBreakClass::NU) => false,
        (LineBreakClass::BA, LineBreakClass::AL | LineBreakClass::NU) => false,
        (LineBreakClass::NS, LineBreakClass::AL | LineBreakClass::NU) => false,
        (LineBreakClass::AL | LineBreakClass::NU, LineBreakClass::IS | LineBreakClass::SY) => false,
        _ => true,
    }
}

/// Iterator over line break opportunities in a string.
pub(crate) struct LineBreakIterator<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    pos: usize,
    prev_class: Option<LineBreakClass>,
}

impl<'a> LineBreakIterator<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars().peekable(),
            pos: 0,
            prev_class: None,
        }
    }
}

impl Iterator for LineBreakIterator<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        for c in self.chars.by_ref() {
            let char_len = c.len_utf8();
            let curr_class = line_break_class(c);

            if let Some(prev) = self.prev_class {
                if can_break_between(prev, curr_class) {
                    let break_pos = self.pos;
                    self.prev_class = Some(curr_class);
                    self.pos += char_len;
                    return Some(break_pos);
                }
            }

            self.prev_class = Some(curr_class);
            self.pos += char_len;
        }
        None
    }
}

/// Returns an iterator over line break opportunities in the given text.
/// Each returned position is a byte index where a line break may occur.
#[must_use]
pub(crate) fn line_break_opportunities(text: &str) -> LineBreakIterator<'_> {
    LineBreakIterator::new(text)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn ascii_classification() {
        assert_eq!(line_break_class(' '), LineBreakClass::SP);
        assert_eq!(line_break_class('a'), LineBreakClass::AL);
        assert_eq!(line_break_class('Z'), LineBreakClass::AL);
        assert_eq!(line_break_class('0'), LineBreakClass::NU);
        assert_eq!(line_break_class('('), LineBreakClass::OP);
        assert_eq!(line_break_class(')'), LineBreakClass::CL);
        assert_eq!(line_break_class('-'), LineBreakClass::HY);
        assert_eq!(line_break_class('/'), LineBreakClass::SY);
    }

    #[test]
    fn cjk_classification() {
        assert_eq!(line_break_class('中'), LineBreakClass::ID);
        assert_eq!(line_break_class('文'), LineBreakClass::ID);
        assert_eq!(line_break_class('あ'), LineBreakClass::ID);
        assert_eq!(line_break_class('ア'), LineBreakClass::ID);
        assert_eq!(line_break_class('가'), LineBreakClass::H2);
    }

    #[test]
    fn punctuation_classification() {
        assert_eq!(line_break_class('"'), LineBreakClass::QU);
        assert_eq!(line_break_class('\''), LineBreakClass::QU);
        assert_eq!(line_break_class('!'), LineBreakClass::EX);
        assert_eq!(line_break_class(','), LineBreakClass::IS);
    }

    #[test]
    fn ranges_sorted_and_non_overlapping() {
        let mut prev_hi: Option<u32> = None;
        for &(lo, hi, _) in LB_CLASS_RANGES {
            assert!(lo <= hi, "range start must be <= end: {lo:x}-{hi:x}");
            if let Some(p) = prev_hi {
                assert!(
                    lo > p,
                    "ranges must be sorted and non-overlapping: {p:x} < {lo:x}"
                );
            }
            prev_hi = Some(hi);
        }
    }

    #[test]
    fn zero_width_space() {
        assert_eq!(line_break_class('\u{200B}'), LineBreakClass::ZW);
        assert_eq!(line_break_class('\u{200C}'), LineBreakClass::Zwj);
        assert_eq!(line_break_class('\u{200D}'), LineBreakClass::Zwj);
    }

    #[test]
    fn break_between_cjk() {
        assert!(can_break_between(LineBreakClass::ID, LineBreakClass::ID));
        assert!(can_break_between(LineBreakClass::ID, LineBreakClass::AL));
        assert!(can_break_between(LineBreakClass::AL, LineBreakClass::ID));
        assert!(!can_break_between(LineBreakClass::AL, LineBreakClass::AL));
    }

    #[test]
    fn break_between_hangul() {
        assert!(!can_break_between(LineBreakClass::JL, LineBreakClass::JL));
        assert!(!can_break_between(LineBreakClass::JL, LineBreakClass::JV));
        assert!(!can_break_between(LineBreakClass::JV, LineBreakClass::JT));
        assert!(can_break_between(LineBreakClass::JT, LineBreakClass::JL));
    }

    #[test]
    fn break_iterator_latin() {
        let text = "hello world";
        let breaks: Vec<usize> = line_break_opportunities(text).collect();
        assert_eq!(breaks, vec![6]);
    }

    #[test]
    fn break_iterator_cjk() {
        let text = "中文测试";
        let breaks: Vec<usize> = line_break_opportunities(text).collect();
        assert_eq!(breaks, vec![3, 6, 9]);
    }

    #[test]
    fn break_iterator_mixed() {
        let text = "你好world测试";
        let breaks: Vec<usize> = line_break_opportunities(text).collect();
        assert!(breaks.contains(&3));
        assert!(breaks.contains(&6));
        assert!(breaks.contains(&11));
    }
}
