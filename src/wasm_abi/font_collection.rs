//! Bounded collection inspection/extraction over the shared font core.

use wasm_bindgen::prelude::*;

use crate::text::collection::MAX_COLLECTION_FACES;
use crate::text::{CollectionError, CollectionErrorKind};

/// Count TTC/OTC faces after validating the complete collection header.
/// No font discovery, filesystem access or rendering is performed.
///
/// # Errors
/// Refuses standalone fonts, malformed/unsupported headers and input limits.
#[wasm_bindgen(js_name = fontCollectionFaceCount)]
pub fn font_collection_face_count(bytes: &[u8]) -> Result<u32, JsValue> {
    crate::wasm::font_collection_face_count(bytes).map_err(collection_error_to_js)
}

/// Extract an owned standalone SFNT face. Omitted `face_index` selects zero.
/// Receive a floating-point number so fractional, negative and wrapped indices
/// cannot silently select a different face through wasm-bindgen's `u32` cast.
///
/// # Errors
/// Refuses invalid indices, malformed tables, unsupported face formats and
/// input/output limits using the native extractor's stable error categories.
#[wasm_bindgen(js_name = extractFontCollectionFace)]
pub fn extract_font_collection_face(
    bytes: &[u8],
    face_index: Option<f64>,
) -> Result<Vec<u8>, JsValue> {
    let face = admit_face_index(face_index).map_err(JsValue::from_str)?;
    crate::wasm::extract_font_collection_face(bytes, face).map_err(collection_error_to_js)
}

fn admit_face_index(index: Option<f64>) -> Result<u32, &'static str> {
    let index = index.unwrap_or(0.0);
    if !index.is_finite()
        || index.fract() != 0.0
        || index < 0.0
        || index >= f64::from(MAX_COLLECTION_FACES)
    {
        return Err("FONT_COLLECTION_FACE_INDEX: faceIndex must be an integer from 0 through 1023");
    }
    Ok(index as u32)
}

fn collection_error_message(error: CollectionError) -> String {
    let code = match error.kind {
        CollectionErrorKind::NotCollection => "FONT_COLLECTION_NOT_COLLECTION",
        CollectionErrorKind::UnsupportedVersion => "FONT_COLLECTION_UNSUPPORTED_VERSION",
        CollectionErrorKind::UnsupportedFaceFormat => "FONT_COLLECTION_UNSUPPORTED_FACE_FORMAT",
        CollectionErrorKind::InvalidFaceIndex => "FONT_COLLECTION_FACE_INDEX",
        CollectionErrorKind::Malformed => "FONT_COLLECTION_MALFORMED",
        CollectionErrorKind::DuplicateTable => "FONT_COLLECTION_DUPLICATE_TABLE",
        CollectionErrorKind::MissingTable => "FONT_COLLECTION_MISSING_TABLE",
        CollectionErrorKind::LimitExceeded => "FONT_COLLECTION_LIMIT",
    };
    format!("{code}: {error}")
}

fn collection_error_to_js(error: CollectionError) -> JsValue {
    JsValue::from_str(&collection_error_message(error))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::{admit_face_index, collection_error_message};
    use crate::text::{CollectionError, CollectionErrorKind};

    #[test]
    fn javascript_indices_are_validated_before_integer_conversion() {
        assert_eq!(admit_face_index(None), Ok(0));
        assert_eq!(admit_face_index(Some(-0.0)), Ok(0));
        assert_eq!(admit_face_index(Some(1023.0)), Ok(1023));
        for index in [
            -1.0,
            0.5,
            1024.0,
            4_294_967_296.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(admit_face_index(Some(index)).is_err());
        }
    }

    #[test]
    fn collection_failures_keep_the_selected_face_table_and_offset() {
        let message = collection_error_message(CollectionError {
            kind: CollectionErrorKind::Malformed,
            face_index: Some(7),
            table: Some(*b"head"),
            offset: Some(192),
        });
        assert!(message.starts_with("FONT_COLLECTION_MALFORMED: "));
        assert!(message.contains("face Some(7)"));
        assert!(message.contains("table Some([104, 101, 97, 100])"));
        assert!(message.contains("offset Some(192)"));
    }
}
