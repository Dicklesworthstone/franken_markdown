//! Stateless SVG/EPUB publication from caller-owned source and resource bytes.
//! These routes never resolve includes, read a path, or write an output file.

use super::{
    ERROR_RENDER_FAILED, JsonValue, ToolError, authoring_profile, base64_encode, engine_failure,
    epub_options, file_options, markdown, resources, wrap_text_content,
};

pub(super) fn render_svg(args: &JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    let source = markdown(args, limit)?;
    let profile = authoring_profile(args)?;
    let options = file_options::svg_options(args)?;
    let assets = resources::parse(args)?;
    let document = crate::parse_markdown_with_profile(source, profile);
    let document = crate::apply_profile(&document, profile);
    let (bytes, _, warnings) =
        crate::svg::render_svg_with_resources(&document, &options, &assets.fonts, &assets.images)
            .map_err(|error| engine_failure(error, "Render failed", "render_failed"))?;
    let svg = std::str::from_utf8(&bytes).map_err(|_| {
        (
            ERROR_RENDER_FAILED,
            "Renderer returned invalid UTF-8 SVG".to_owned(),
            "render_failed",
        )
    })?;
    Ok(resources::with_svg_warnings(
        wrap_text_content(svg),
        warnings,
    ))
}

pub(super) fn render_epub(args: &JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    let source = markdown(args, limit)?;
    let mut options = epub_options(args)?;
    let assets = resources::parse(args)?;
    options.font_assets = assets.fonts;
    options.image_assets = assets.images;
    let document = crate::parse_markdown_with_profile(source, options.profile);
    let bytes = crate::render_epub(&document, &options)
        .map_err(|error| engine_failure(error, "Render failed", "render_failed"))?;
    Ok(wrap_text_content(&base64_encode(&bytes)))
}
