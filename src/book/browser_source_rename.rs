//! Pure source refactoring exposed separately from retained render handles.

use super::{book_inputs, to_js};
use wasm_bindgen::prelude::*;

/// Prepare one complete source-only rename transaction. The host retains the
/// source checkpoint and applies this response atomically; no file or retained
/// render handle is changed by this operation.
///
/// # Errors
/// Invalid profiles/arrays, path collisions, source limits, include failures or
/// unproven destination ownership return an error without a partial capture.
#[wasm_bindgen(js_name = renameBookSource)]
#[allow(clippy::too_many_arguments)]
pub fn rename_book_source(
    paths: Vec<String>,
    sources: Vec<String>,
    include_paths: Vec<String>,
    include_sources: Vec<String>,
    old_path: String,
    new_path: String,
    profile: Option<String>,
    image_destinations: Vec<String>,
) -> Result<String, JsValue> {
    let profile =
        crate::wasm_abi::parse_authoring_profile(profile.as_deref()).map_err(JsValue::from_str)?;
    let chapters = book_inputs(paths, sources).map_err(JsValue::from_str)?;
    let resources = if include_paths.is_empty() && include_sources.is_empty() {
        Vec::new()
    } else {
        book_inputs(include_paths, include_sources).map_err(JsValue::from_str)?
    };
    super::super::rename_book_source_with_images(
        &chapters,
        &resources,
        &old_path,
        &new_path,
        profile,
        &image_destinations,
    )
    .and_then(|report| report.to_json())
    .map_err(to_js)
}
