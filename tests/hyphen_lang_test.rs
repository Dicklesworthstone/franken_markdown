//! 38re.1: Liang hyphenation for de/fr/nl/es against TeX pattern oracles.
//!
//! Every assertion prints one stderr checklist line (`check=… outcome=`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{HyphenLang, HyphenationOptions, Hyphenator};
use std::fs;
use std::path::Path;

fn log_points(id: &str, lang: HyphenLang, word: &str, got: &[usize], expected: &[usize]) {
    let outcome = if got == expected { "PASS" } else { "FAIL" };
    eprintln!(
        "check={id} lang={} subject={word:?} expected={expected:?} got={got:?} outcome={outcome}",
        lang.as_str()
    );
    assert_eq!(got, expected, "{id} {word}");
}

fn opts(lang: HyphenLang) -> HyphenationOptions {
    lang.default_options()
}

#[test]
fn pattern_file_sizes_and_license_headers() {
    let cases: &[(&str, usize, usize)] = &[
        ("hyph-de-1996", 36_709, 320_000),
        ("hyph-fr", 1_216, 20_000),
        ("hyph-nl", 12_724, 120_000),
        ("hyph-es", 4_694, 60_000),
    ];
    let root = Path::new("data");
    for (stem, tokens, ceiling) in cases {
        let pat = root.join(format!("{stem}.patterns"));
        let readme = root.join(format!("{stem}.README.md"));
        let bytes = fs::metadata(&pat)
            .unwrap_or_else(|e| panic!("missing {}: {e}", pat.display()))
            .len() as usize;
        let text = fs::read_to_string(&pat).unwrap();
        let count = text.split_ascii_whitespace().count();
        let readme_text = fs::read_to_string(&readme)
            .unwrap_or_else(|e| panic!("missing {}: {e}", readme.display()));
        let licensed = readme_text.contains("MIT");
        let outcome = if count == *tokens && bytes <= *ceiling && licensed {
            "PASS"
        } else {
            "FAIL"
        };
        eprintln!(
            "check=artifact subject={stem} tokens={count} expected={tokens} bytes={bytes} ceiling={ceiling} license={licensed} outcome={outcome}"
        );
        assert_eq!(count, *tokens, "{stem} token count");
        assert!(bytes <= *ceiling, "{stem} {bytes} > ceiling {ceiling}");
        assert!(licensed, "{stem} README must name the MIT licence");
    }
}

#[test]
fn hyphenator_token_counts_match_committed_files() {
    let cases = [
        (Hyphenator::german(), 36_709usize),
        (Hyphenator::french(), 1_216),
        (Hyphenator::dutch(), 12_724),
        (Hyphenator::spanish(), 4_694),
        (Hyphenator::english(), 4_938),
    ];
    for (h, expected) in cases {
        let got = h.encoded_pattern_count();
        let outcome = if got == expected { "PASS" } else { "FAIL" };
        eprintln!(
            "check=token-count lang={} expected={expected} got={got} outcome={outcome}",
            h.lang().as_str()
        );
        assert_eq!(got, expected, "{}", h.lang().as_str());
    }
}

#[test]
fn german_tex_hyphenation_points() {
    let h = Hyphenator::german();
    let o = opts(HyphenLang::German);
    let cases: &[(&str, &[usize])] = &[
        ("Dampfschiff", &[5]),
        ("Universität", &[3, 6, 8]),
        ("Computer", &[3, 5]),
        ("Ausführung", &[3, 6]),
        ("Bäckerei", &[2, 5]),
        ("Donaudampfschifffahrt", &[2, 5, 10, 16]),
        ("Straße", &[4]),
    ];
    for (word, expected) in cases {
        log_points(
            "de-points",
            HyphenLang::German,
            word,
            &h.hyphenation_points(word, o),
            expected,
        );
    }
}

#[test]
fn french_tex_hyphenation_points() {
    let h = Hyphenator::french();
    let o = opts(HyphenLang::French);
    let cases: &[(&str, &[usize])] = &[
        ("constitution", &[6, 8]),
        ("aujourd'hui", &[2, 6]),
        ("développement", &[2, 4, 7, 9]),
        ("informatique", &[2, 5, 7]),
        ("nécessaire", &[2, 5]),
    ];
    for (word, expected) in cases {
        log_points(
            "fr-points",
            HyphenLang::French,
            word,
            &h.hyphenation_points(word, o),
            expected,
        );
    }
}

#[test]
fn dutch_tex_hyphenation_points() {
    let h = Hyphenator::dutch();
    let o = opts(HyphenLang::Dutch);
    let cases: &[(&str, &[usize])] = &[
        ("aardappel", &[4, 6]),
        ("onafhankelijk", &[2, 4, 7, 9]),
        ("computer", &[3, 5]),
        ("verantwoordelijkheid", &[3, 6, 10, 12, 16]),
    ];
    for (word, expected) in cases {
        log_points(
            "nl-points",
            HyphenLang::Dutch,
            word,
            &h.hyphenation_points(word, o),
            expected,
        );
    }
}

#[test]
fn spanish_tex_hyphenation_points() {
    let h = Hyphenator::spanish();
    let o = opts(HyphenLang::Spanish);
    let cases: &[(&str, &[usize])] = &[
        ("constitución", &[4, 6, 8]),
        ("extraordinario", &[2, 5, 7, 9, 11]),
        ("computadora", &[5, 9]),
        ("español", &[2, 4]),
    ];
    for (word, expected) in cases {
        log_points(
            "es-points",
            HyphenLang::Spanish,
            word,
            &h.hyphenation_points(word, o),
            expected,
        );
    }
}

#[test]
fn english_default_is_unchanged() {
    let h = Hyphenator::english();
    let o = HyphenationOptions::default();
    let got = h.hyphenation_points("characterization", o);
    log_points(
        "en-regression",
        HyphenLang::English,
        "characterization",
        &got,
        &[4, 6, 9, 10, 12],
    );
    assert_eq!(h.encoded_pattern_count(), 4_938);
    assert_eq!(h.lang(), HyphenLang::English);
    assert_eq!(h.default_options().min_right, 3);
}

#[test]
fn for_tag_resolves_roster_and_rejects_unknown() {
    let cases = [
        ("en", Some(HyphenLang::English)),
        ("EN-US", Some(HyphenLang::English)),
        ("de", Some(HyphenLang::German)),
        ("ngerman", Some(HyphenLang::German)),
        ("fr", Some(HyphenLang::French)),
        ("nl", Some(HyphenLang::Dutch)),
        ("es", Some(HyphenLang::Spanish)),
        ("espanol", Some(HyphenLang::Spanish)),
        ("zh", None),
        ("", None),
    ];
    for (tag, expected) in cases {
        let got = Hyphenator::for_tag(tag).map(|h| h.lang());
        let outcome = if got == expected { "PASS" } else { "FAIL" };
        eprintln!(
            "check=for-tag subject={tag:?} expected={expected:?} got={got:?} outcome={outcome}"
        );
        assert_eq!(got, expected, "tag {tag:?}");
    }
}

#[test]
fn non_letters_do_not_hyphenate() {
    let h = Hyphenator::german();
    let o = opts(HyphenLang::German);
    for word in ["Dampf-schiff", "12345", "ok"] {
        let got = h.hyphenation_points(word, o);
        let outcome = if got.is_empty() { "PASS" } else { "FAIL" };
        eprintln!("check=reject-non-letter subject={word:?} got={got:?} outcome={outcome}");
        assert!(got.is_empty(), "{word}");
    }
}

#[test]
fn expanding_lowercase_does_not_emit_desynced_points() {
    // U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE lowercases to two
    // codepoints. Offsets into that expansion would not be character
    // indexes in the original word, so the hyphenator must refuse.
    let h = Hyphenator::german();
    let o = opts(HyphenLang::German);
    let word = "İstanbulxx";
    let got = h.hyphenation_points(word, o);
    let outcome = if got.is_empty() { "PASS" } else { "FAIL" };
    eprintln!("check=expanding-lower subject={word:?} got={got:?} outcome={outcome}");
    assert!(got.is_empty(), "{word} → {got:?}");
}
