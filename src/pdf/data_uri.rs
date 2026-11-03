//! Image bytes carried directly in Markdown or lowered safe-HTML sources.
//!
//! Use the SVG backend's bounded streaming decoder so percent-escaped base64,
//! strict padding, header limits and allocation budgets agree across outputs.
//! PDF format admission remains here: a declared MIME selects exactly one
//! decoder and cannot authorize a differently typed payload.

use super::{
    PdfImageData, parse_gif_image_asset, parse_jpeg_image_asset, parse_png_image_asset,
    parse_svg_image_asset, svg_inline_image_key,
};

pub(super) fn is_data_uri(source: &str) -> bool {
    source
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
}

pub(super) fn resolve(source: &str) -> Option<PdfImageData> {
    if !is_data_uri(source) {
        return None;
    }
    let bytes = crate::svg::image_source::decode_data_uri(source).ok()?;
    // Successful decoding proves the ASCII prefix and a supported MIME inside
    // a header of at most 256 bytes. No payload-sized header copy is needed.
    let mime = source[5..].split([';', ',']).next()?;
    // Image resource keys need identity, not a second copy of a potentially
    // large base64 source. Equal payloads can share the same PDF resource.
    let key = svg_inline_image_key("pdf-data-uri", &bytes);
    if mime.eq_ignore_ascii_case("image/png") {
        parse_png_image_asset(&key, &bytes)
    } else if mime.eq_ignore_ascii_case("image/jpeg") {
        parse_jpeg_image_asset(&key, &bytes)
    } else if mime.eq_ignore_ascii_case("image/gif") {
        parse_gif_image_asset(&key, &bytes)
    } else if mime.eq_ignore_ascii_case("image/svg+xml") {
        parse_svg_image_asset(&key, &bytes)
    } else {
        None
    }
}

/// Diagnostics are also consumed by verify/JSON and browser hosts, some of
/// which format the warning's destination directly. Keep data payloads out of
/// the warning itself, while retaining ordinary path/URL bytes unchanged.
pub(super) fn diagnostic_destination(destination: &str) -> String {
    let source = destination.trim();
    if !is_data_uri(source) {
        return destination.to_owned();
    }
    // Even a malformed or deliberately huge header must not be copied into
    // stderr. A known MIME and the source size are sufficient identification.
    let mime = source
        .as_bytes()
        .iter()
        .take(256)
        .position(|&byte| byte == b',' || byte == b';')
        .and_then(|end| source.get(5..end))
        .and_then(|declared| {
            ["image/png", "image/jpeg", "image/gif", "image/svg+xml"]
                .into_iter()
                .find(|mime| declared.eq_ignore_ascii_case(mime))
        });
    match mime {
        Some(mime) => format!("data:{mime} ({} source bytes)", source.len()),
        None => format!("embedded image data URI ({} source bytes)", source.len()),
    }
}
