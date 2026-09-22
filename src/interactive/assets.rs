//! Persist native image bindings for the offline editor, not just its first frame.

use std::collections::BTreeSet;
use crate::HtmlOptions;

/// Encode with the actual HTML resource resolver (including SVG sanitization),
/// preserving its destination trimming and duplicate-selection rules. Keep even
/// unused assets: a later edit can add a reference without ambient file access.
pub(super) fn push_manifest(opts: &HtmlOptions, out: &mut String) {
    if opts.image_assets.is_empty() {
        return;
    }
    out.push_str("<script type=\"application/json\" id=\"fmd-image-assets\">[");
    let mut seen = BTreeSet::new();
    let mut first = true;
    for asset in &opts.image_assets {
        let key = asset.destination.trim();
        if !seen.insert(key) {
            continue;
        }
        let mut uri = String::new();
        if !crate::html::push_html_image_asset_data_uri(key, opts, &mut uri) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push('[');
        super::push_source_json(key, out);
        out.push(',');
        super::push_source_json(&uri, out);
        out.push(']');
    }
    out.push_str("]</script>\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PdfImageAsset;

    fn svg() -> Vec<u8> {
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="2" height="1" fill="red"/></svg>"#.to_vec()
    }

    #[test]
    fn manifest_uses_native_encoder_and_retains_unused_images() {
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new(" plot.svg ", svg())],
            ..Default::default()
        };
        let mut native = String::new();
        assert!(crate::html::push_html_image_asset_data_uri("plot.svg", &opts, &mut native));
        let html = super::super::render_interactive_html(&crate::parse_markdown("Text"), "Text", &opts);
        assert!(html.contains(&format!("[\"plot.svg\",\"{native}\"]")));
        assert!(!html.contains("<rect width=\"2\""));
    }

    #[test]
    fn repeated_keys_use_the_same_resolver_as_the_initial_render() {
        let opts = HtmlOptions {
            image_assets: vec![
                PdfImageAsset::new(" image.svg ", vec![0]),
                PdfImageAsset::new("image.svg", svg()),
                PdfImageAsset::new("image.svg", svg()),
            ],
            ..Default::default()
        };
        let mut native = String::new();
        let resolved = crate::html::push_html_image_asset_data_uri("image.svg", &opts, &mut native);
        let mut manifest = String::new();
        push_manifest(&opts, &mut manifest);
        assert_eq!(manifest.matches("[\"image.svg\",").count(), if resolved { 1 } else { 0 });
        if resolved { assert!(manifest.contains(&native)); }
    }

    #[test]
    fn opaque_keys_cannot_close_the_data_block_or_change_json() {
        let key = "</ScRiPt><!--\"\\\n\u{2028}\u{2029}";
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new(key, svg())],
            ..Default::default()
        };
        let mut manifest = String::new();
        push_manifest(&opts, &mut manifest);
        assert!(!manifest.contains("</ScRiPt>"));
        assert!(!manifest.contains("<!--"));
        assert!(manifest.contains("\\u003c/ScRiPt>"));
        assert_eq!(manifest.matches("</script>").count(), 1);
    }

    #[test]
    fn missing_and_unrecognized_assets_do_not_invent_image_bytes() {
        let opts = HtmlOptions {
            image_assets: vec![PdfImageAsset::new("bad.bin", vec![0, 1, 2])],
            ..Default::default()
        };
        let mut manifest = String::new();
        push_manifest(&opts, &mut manifest);
        assert!(manifest.ends_with("[]</script>\n"));
    }
}
