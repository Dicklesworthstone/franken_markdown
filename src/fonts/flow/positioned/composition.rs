//! Canonical glyph composition for the bundled Latin reader profile.
//!
//! This is NOT a general Unicode normalizer. It accepts only complete chains
//! of canonical Latin base+mark compositions, with an actual glyph in the
//! requested face. Other sequences continue to the strict positioned shaper.
//! The transient glyph text never replaces logical_text or source offsets.
//!
//! Data: Unicode 15.1.0 canonical two-scalar decompositions for U+00C0..U+024F
//! and U+1E00..U+1EFF, second scalar U+0300..U+036F, excluding pairs whose NFC
//! is not the target character (UAX #15 composition exclusions). No compatibility
//! mappings are used. Rows are (mark, sorted bases, corresponding composites).
//! The executable Node data oracle checks every pair against ICU NFC/NFD.

use super::{BundledFlowFonts, FlowInlineStyle, FlowTextRole, OwnedTextRun, is_mark};
use std::ops::Range;

impl BundledFlowFonts {
    /// Look up one supported canonical Latin base/mark composition.
    ///
    /// Shares the reader's fixed Unicode 15.1 table with other render backends.
    /// This is not a Unicode normalizer: callers must handle complete mark
    /// sequences, retain source ranges and verify the chosen font has the
    /// resulting glyph. There is no font lookup or allocation in this method.
    #[must_use]
    pub fn canonical_latin_composite(base: char, mark: char) -> Option<char> {
        compose_pair(base, mark)
    }
}

struct SourceUnit {
    rendered: Range<usize>,
    source: Range<usize>,
    utf16: Range<usize>,
}

/// Return None when this word requires real mark positioning. Do not partially
/// compose: an unconsumed mark may block later compositions, and must remain
/// with its original base for the strict shaper to handle as one cluster.
pub(super) fn shape(
    fonts: &BundledFlowFonts,
    primary: usize,
    source: &str,
    size: f32,
    role: FlowTextRole,
    style: FlowInlineStyle,
) -> Result<Option<OwnedTextRun>, String> {
    if source.chars().take(4097).count() > 4096 {
        return Ok(None); // The strict shaper reports its own admission budget.
    }
    let Some((text, units)) = compose_word(source) else { return Ok(None); };
    let font = fonts.faces[primary].font;
    if !text.chars().all(|ch| {
        let glyph = font.glyph_index(if ch == '\t' { ' ' } else { ch });
        glyph != 0 && glyph < font.num_glyphs
    }) {
        return Ok(None); // Do not silently swap faces to get a composite.
    }
    // No remaining marks: this calls the ordinary ligature/kerning path.
    let mut run = fonts.shape(&text, size, role, style)?;
    let invalid = || "invalid canonical flow source mapping".to_owned();
    for cluster in &mut run.clusters {
        let first = units.binary_search_by_key(&cluster.byte_range.start, |u| u.rendered.start)
            .map_err(|_| invalid())?;
        let last = units.binary_search_by_key(&cluster.byte_range.end, |u| u.rendered.end)
            .map_err(|_| invalid())?;
        if last < first { return Err(invalid()); }
        cluster.byte_range = units[first].source.start..units[last].source.end;
        cluster.utf16_range = units[first].utf16.start..units[last].utf16.end;
    }
    run.logical_text = source.to_owned();
    Ok(Some(run))
}

fn compose_word(source: &str) -> Option<(String, Vec<SourceUnit>)> {
    let mut text = String::with_capacity(source.len());
    let mut units = Vec::new();
    let mut chars = source.char_indices().peekable();
    let mut utf16 = 0;
    while let Some((start, mut base)) = chars.next() {
        if is_mark(base) { return None; }
        let mut end = start + base.len_utf8();
        let utf16_start = utf16;
        utf16 += base.len_utf16();
        let mut marks = 0;
        while let Some(&(offset, mark)) = chars.peek() {
            if !is_mark(mark) { break; }
            marks += 1;
            if marks > 64 { return None; }
            base = compose_pair(base, mark)?;
            chars.next();
            end = offset + mark.len_utf8();
            utf16 += mark.len_utf16();
        }
        let rendered_start = text.len();
        text.push(base);
        units.push(SourceUnit {
            rendered: rendered_start..text.len(), source: start..end,
            utf16: utf16_start..utf16,
        });
    }
    Some((text, units))
}

fn compose_pair(base: char, mark: char) -> Option<char> {
    let row = COMPOSITIONS.binary_search_by_key(&mark, |row| row.0).ok()?;
    let (_, bases, composites) = COMPOSITIONS[row];
    let column = bases.chars().position(|ch| ch == base)?;
    composites.chars().nth(column)
}

const COMPOSITIONS: &[(char, &str, &str)] = &[
    ('\u{0300}', "AEINOUWYaeinouwyÂÊÔÜâêôüĂăĒēŌōƠơƯư", "ÀÈÌǸÒÙẀỲàèìǹòùẁỳẦỀỒǛầềồǜẰằḔḕṐṑỜờỪừ"),
    ('\u{0301}', "ACEGIKLMNOPRSUWYZacegiklmnoprsuwyzÂÅÆÇÊÏÔÕØÜâåæçêïôõøüĂăĒēŌōŨũƠơƯư", "ÁĆÉǴÍḰĹḾŃÓṔŔŚÚẂÝŹáćéǵíḱĺḿńóṕŕśúẃýźẤǺǼḈẾḮỐṌǾǗấǻǽḉếḯốṍǿǘẮắḖḗṒṓṸṹỚớỨứ"),
    ('\u{0302}', "ACEGHIJOSUWYZaceghijosuwyzẠạẸẹỌọ", "ÂĈÊĜĤÎĴÔŜÛŴŶẐâĉêĝĥîĵôŝûŵŷẑẬậỆệỘộ"),
    ('\u{0303}', "AEINOUVYaeinouvyÂÊÔâêôĂăƠơƯư", "ÃẼĨÑÕŨṼỸãẽĩñõũṽỹẪỄỖẫễỗẴẵỠỡỮữ"),
    ('\u{0304}', "AEGIOUYaegiouyÄÆÕÖÜäæõöüǪǫȦȧȮȯḶḷṚṛ", "ĀĒḠĪŌŪȲāēḡīōūȳǞǢȬȪǕǟǣȭȫǖǬǭǠǡȰȱḸḹṜṝ"),
    ('\u{0306}', "AEGIOUaegiouȨȩẠạ", "ĂĔĞĬŎŬăĕğĭŏŭḜḝẶặ"),
    ('\u{0307}', "ABCDEFGHIMNOPRSTWXYZabcdefghmnoprstwxyzŚśŠšſṢṣ", "ȦḂĊḊĖḞĠḢİṀṄȮṖṘṠṪẆẊẎŻȧḃċḋėḟġḣṁṅȯṗṙṡṫẇẋẏżṤṥṦṧẛṨṩ"),
    ('\u{0308}', "AEHIOUWXYaehiotuwxyÕõŪū", "ÄËḦÏÖÜẄẌŸäëḧïöẗüẅẍÿṎṏṺṻ"),
    ('\u{0309}', "AEIOUYaeiouyÂÊÔâêôĂăƠơƯư", "ẢẺỈỎỦỶảẻỉỏủỷẨỂỔẩểổẲẳỞởỬử"),
    ('\u{030a}', "AUauwy", "ÅŮåůẘẙ"),
    ('\u{030b}', "OUou", "ŐŰőű"),
    ('\u{030c}', "ACDEGHIKLNORSTUZacdeghijklnorstuzÜüƷʒ", "ǍČĎĚǦȞǏǨĽŇǑŘŠŤǓŽǎčďěǧȟǐǰǩľňǒřšťǔžǙǚǮǯ"),
    ('\u{030f}', "AEIORUaeioru", "ȀȄȈȌȐȔȁȅȉȍȑȕ"),
    ('\u{0311}', "AEIORUaeioru", "ȂȆȊȎȒȖȃȇȋȏȓȗ"),
    ('\u{031b}', "OUou", "ƠƯơư"),
    ('\u{0323}', "ABDEHIKLMNORSTUVWYZabdehiklmnorstuvwyzƠơƯư", "ẠḄḌẸḤỊḲḶṂṆỌṚṢṬỤṾẈỴẒạḅḍẹḥịḳḷṃṇọṛṣṭụṿẉỵẓỢợỰự"),
    ('\u{0324}', "Uu", "Ṳṳ"),
    ('\u{0325}', "Aa", "Ḁḁ"),
    ('\u{0326}', "STst", "ȘȚșț"),
    ('\u{0327}', "CDEGHKLNRSTcdeghklnrst", "ÇḐȨĢḨĶĻŅŖŞŢçḑȩģḩķļņŗşţ"),
    ('\u{0328}', "AEIOUaeiou", "ĄĘĮǪŲąęįǫų"),
    ('\u{032d}', "DELNTUdelntu", "ḒḘḼṊṰṶḓḙḽṋṱṷ"),
    ('\u{032e}', "Hh", "Ḫḫ"),
    ('\u{0330}', "EIUeiu", "ḚḬṴḛḭṵ"),
    ('\u{0331}', "BDKLNRTZbdhklnrtz", "ḆḎḴḺṈṞṮẔḇḏẖḵḻṉṟṯẕ"),
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::display::DisplayItem;
    use crate::flow_display::{FlowLayoutOptions, ResumableFlowDisplay};
    use crate::theme::FontFamily;

    #[test]
    fn canonical_units_preserve_original_source_domains() {
        let source = "Cafe\u{0301}\tA\u{030a}";
        let (text, units) = compose_word(source).unwrap();
        assert_eq!(text, "Café\tÅ");
        assert_eq!(units.len(), 6);
        assert_eq!(units[3].rendered, 3..5);
        assert_eq!(units[3].source, 3..6);
        assert_eq!(units[3].utf16, 3..5);
        assert_eq!(units[5].source, 7..10);
        assert_eq!(units[5].utf16, 6..8);
        assert_eq!(compose_word("u\u{0308}\u{0301}").unwrap().0, "ǘ");
        assert_eq!(compose_word("U\u{0308}\u{0304}").unwrap().0, "Ǖ");
    }

    #[test]
    fn unconsumed_marks_and_compatibility_characters_are_not_silently_normalized() {
        for source in ["\u{0301}a", "a\u{0301}\u{0307}", "a\u{034f}", "e\u{0301}\u{0301}"] {
            assert!(compose_word(source).is_none());
        }
        assert_eq!(compose_word("ﬃ ① ²").unwrap().0, "ﬃ ① ²");
        let mut count = 0;
        for &(mark, bases, composites) in COMPOSITIONS {
            assert!(is_mark(mark));
            assert_eq!(bases.chars().count(), composites.chars().count());
            count += bases.chars().count();
        }
        assert_eq!(count, 497);
    }

    #[test]
    fn bundled_families_styles_and_code_use_precomposed_glyphs_with_original_text() {
        let source = "office cafe\u{0301}\tA\u{030a}ngstro\u{0308}m";
        let canonical = "office café\tÅngström";
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            for (bold, italic, code) in [(false,false,false), (true,false,false),
                (false,true,false), (true,true,false), (false,false,true)]
            {
                let style = FlowInlineStyle { bold, italic, code, ..FlowInlineStyle::default() };
                let actual = fonts.shape(source, 14.0, FlowTextRole::Body, style).unwrap();
                let expected = fonts.shape(canonical, 14.0, FlowTextRole::Body, style).unwrap();
                assert_eq!(actual.logical_text, source);
                assert_eq!(actual.glyphs, expected.glyphs);
                assert_eq!(actual.total_advance, expected.total_advance);
                let mut byte = 0;
                let mut utf16 = 0;
                for cluster in &actual.clusters {
                    assert_eq!(cluster.byte_range.start, byte);
                    assert_eq!(cluster.utf16_range.start, utf16);
                    let slice = &source[cluster.byte_range.clone()];
                    assert!(!slice.chars().next().is_some_and(is_mark));
                    byte = cluster.byte_range.end;
                    utf16 += slice.encode_utf16().count();
                    assert_eq!(cluster.utf16_range.end, utf16);
                }
                assert_eq!(byte, source.len());
                assert_eq!(utf16, source.encode_utf16().count());
            }
        }
    }

    #[test]
    fn real_bundled_marks_flow_through_headings_tables_and_narrow_reflow() {
        let source = "# Cafe\u{0301}\n\nA\u{030a}ngstro\u{0308}m **re\u{0301}sume\u{0301}**.\n\n| Name |\n| --- |\n| nai\u{0308}ve |\n";
        let mut engine = ResumableFlowDisplay::new(source, 1);
        engine.process_all().unwrap();
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let fonts = BundledFlowFonts::new(family).unwrap();
            for width in [100.0, 220.0, 720.0] {
                let display = fonts.render(&engine, FlowLayoutOptions {
                    viewport_width: width, ..FlowLayoutOptions::default()
                }).unwrap();
                let mut marked = 0;
                for item in display.items() {
                    if let DisplayItem::Text(item) = item {
                        let run = item.font_run.as_ref().unwrap();
                        assert_eq!(run.logical_text, item.text);
                        assert!(run.glyphs.iter().all(|g| fonts.font_bytes(g.font_id).is_some()));
                        assert!(item.bounds.right() <= width + 0.01);
                        if run.logical_text.chars().any(is_mark) { marked += 1; }
                    }
                }
                assert!(marked >= 3);
            }
        }
    }
}
