//! Preserve configured document appearance without styling the editor itself.

use crate::{HtmlOptions, Theme};

pub(super) fn is_requested(opts: &HtmlOptions) -> bool {
    opts.theme != Theme::default()
        || opts.custom_css.is_some()
        || !opts.font_assets.is_empty()
        || opts.toc
        || opts.toc_depth.is_some()
        || opts.html_font_format != crate::HtmlFontFormat::default()
}

pub(super) fn push_frame(fragment: &str, opts: &HtmlOptions, css: &str, out: &mut String) {
    let mut document = String::new();
    document.push_str("<!DOCTYPE html><html lang=\"");
    super::escape_html_to(opts.lang.as_deref().unwrap_or("en"), &mut document);
    document.push_str("\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline' data:; base-uri 'none'; form-action 'none'\"><title>");
    super::escape_html_to(opts.title.as_deref().unwrap_or("Document"), &mut document);
    document.push_str("</title><style>");
    document.push_str(css);
    document.push_str("</style></head><body><main class=\"fmd\">");
    document.push_str(fragment);
    document.push_str("</main></body></html>");
    out.push_str("<iframe data-fmd-styled-preview title=\"Markdown preview\" sandbox=\"allow-same-origin allow-modals\" referrerpolicy=\"no-referrer\" style=\"display:block;width:100%;border:0;min-height:320px;height:100vh\" srcdoc=\"");
    super::escape_html_to(&document, out);
    out.push_str("\"></iframe>");
}

pub(super) fn push_manifest(opts: &HtmlOptions, css: &str, out: &mut String) {
    out.push_str(
        "<script type=\"application/json\" id=\"fmd-presentation\">{\"version\":1,\"css\":",
    );
    super::push_source_json(css, out);
    out.push_str(",\"title\":");
    super::push_source_json(opts.title.as_deref().unwrap_or("Document"), out);
    out.push_str(",\"lang\":");
    super::push_source_json(opts.lang.as_deref().unwrap_or("en"), out);
    out.push_str(",\"toc\":");
    out.push_str(if opts.toc { "true" } else { "false" });
    out.push_str(",\"tocDepth\":");
    out.push(char::from(b'0' + opts.toc_depth.unwrap_or(6).clamp(1, 6)));
    out.push_str("}</script>\n");
}
