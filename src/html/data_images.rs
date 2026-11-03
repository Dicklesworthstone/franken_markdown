//! Admit source-carried image data without widening the general URL policy.
//!
//! SVG is emitted only as a base64 image subresource, never inline markup or
//! a navigable URL. SVG image processing disables scripts, interaction and
//! external references; the existing HTML SVG style-import cleanup also
//! applies. Container and size validation are shared with standalone SVG.

use crate::HtmlOptions;
use crate::svg::image_source::{decode_data_uri, inspect};

pub(super) fn is_data_uri(source: &str) -> bool {
    source
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
}

/// Append only after successful admission so callers can preserve visible
/// alt text on every rejected source. Neither the original source nor a
/// host's explicit image bytes are changed.
pub(super) fn push(source: &str, opts: &HtmlOptions, out: &mut String) -> bool {
    let Some(declared) = declared_mime(source) else {
        return false;
    };
    // An exact host binding takes precedence even when the source payload is
    // malformed. The first binding also owns failure: a bad explicit override
    // must not silently reveal the original image or a later duplicate.
    if let Some(asset) = opts
        .image_assets
        .iter()
        .find(|asset| asset.destination.trim() == source)
    {
        let Ok((mime, _, _)) = inspect(&asset.bytes) else {
            return false;
        };
        super::push_html_image_data_uri(mime, &asset.bytes, out);
        return true;
    }
    let Ok(bytes) = decode_data_uri(source) else {
        return false;
    };
    let Ok((mime, _, _)) = inspect(&bytes) else {
        return false;
    };
    if mime != declared {
        return false;
    }
    super::push_html_image_data_uri(mime, &bytes, out);
    true
}

fn declared_mime(source: &str) -> Option<&'static str> {
    // A host override cannot turn a non-image data URL into an allowed source.
    // Inspect only the bounded MIME field here: the shared decoder validates
    // the complete header and payload if no explicit override is supplied.
    let end = source
        .as_bytes()
        .iter()
        .take(256)
        .position(|&byte| matches!(byte, b';' | b','))?;
    let declared = source.get(5..end)?;
    ["image/png", "image/jpeg", "image/gif", "image/svg+xml"]
        .into_iter()
        .find(|mime| declared.eq_ignore_ascii_case(mime))
}
