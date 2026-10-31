#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

#[test]
fn every_unicode_scalar_matches_the_official_default_full_fold() {
    // Read the published input independently of the generator, so missed rows,
    // compression gaps, locale variants and accidental extra mappings all fail.
    let data = include_str!("../../tests/fixtures/unicode/CaseFolding-17.0.0.txt");
    let mut official = BTreeMap::new();
    for line in data.lines() {
        let fields: Vec<_> = line
            .split('#')
            .next()
            .unwrap()
            .split(';')
            .map(str::trim)
            .collect();
        if fields.len() < 3 || !matches!(fields[1], "C" | "F") {
            continue;
        }
        let cp = u32::from_str_radix(fields[0], 16).unwrap();
        let value: String = fields[2]
            .split_whitespace()
            .map(|value| char::from_u32(u32::from_str_radix(value, 16).unwrap()).unwrap())
            .collect();
        assert!(official.insert(cp, value).is_none());
    }
    assert_eq!(official.len(), 1585);
    let mut folded = String::new();
    for cp in 0..=0x10ffff {
        let Some(ch) = char::from_u32(cp) else {
            continue;
        };
        folded.clear();
        super::push(ch, &mut folded);
        let identity = ch.to_string();
        assert_eq!(
            &folded,
            official.get(&cp).unwrap_or(&identity),
            "U+{cp:04X}"
        );
    }
}
