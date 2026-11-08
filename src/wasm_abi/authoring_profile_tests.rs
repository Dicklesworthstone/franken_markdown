#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::parse_authoring_profile;
use crate::Profile;

#[test]
fn authoring_profile_admission_preserves_omission_and_rejects_unknown_values() {
    assert_eq!(parse_authoring_profile(None), Ok(None));
    assert_eq!(
        parse_authoring_profile(Some("commonmark-gfm")),
        Ok(Some(Profile::CommonMarkGfm))
    );
    assert_eq!(
        parse_authoring_profile(Some("gfm-plus")),
        Ok(Some(Profile::GfmPlus))
    );
    // No unknown spelling, accidental PDF/A selector or blank explicit value
    // may silently select the extension-enabled default.
    for value in ["", " ", "GFM-PLUS", "gfm", "default", "2b", "gfm-plus "] {
        assert!(parse_authoring_profile(Some(value)).is_err(), "{value:?}");
    }
}
