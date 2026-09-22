//! Canonical offline book publication: chapter pages, navigation, search, and
//! the historical root-package receipt, all from one parsed/expanded book.

use super::{Book, BookRenderer, add_site_bytes, invalid, site_archive};
use crate::Result;

const RECEIPT_LIMIT: usize = 4 * 1024 * 1024;

impl BookRenderer {
    /// Publish the canonical HTML site with its chapter-to-output receipt.
    ///
    /// Unlike the old browser-only ZIP builder, this shares chapter-relative
    /// links and assets, frontmatter titles/languages, collision validation,
    /// offline search, and the aggregate output budget with `render_site`.
    /// No source document or retained option is mutated.
    ///
    /// # Errors
    /// Returns normal site-rendering errors, or rejects a receipt over 4 MiB.
    pub fn render_site_publication(&self) -> Result<Vec<u8>> {
        let receipt = receipt_json(self.book())?;
        let (mut archive, mut total) = site_archive(self.book(), &self.options().html_options())?;
        add_site_bytes(&mut total, receipt.len())?;
        archive.add_deflated("frankenmarkdown-receipt.json", receipt.as_bytes());
        Ok(archive.finish())
    }
}

fn receipt_json(book: &Book) -> Result<String> {
    let mut out = format!(
        "{{\"schema\":\"fmd-book-receipt-v1\",\"chapter_count\":{},\"chapters\":[",
        book.chapters.len()
    );
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str("{\"path\":");
        receipt_string(&mut out, &chapter.path)?;
        out.push_str(",\"title\":");
        receipt_string(&mut out, &chapter.title)?;
        out.push_str(",\"output\":");
        receipt_string(&mut out, &chapter.out_name)?;
        out.push('}');
    }
    out.push_str("]}");
    if out.len() > RECEIPT_LIMIT {
        return Err(invalid("site receipt exceeds the 4 MiB limit"));
    }
    Ok(out)
}

// Budget while escaping, not after allocating an unbounded expanded string.
fn receipt_string(out: &mut String, value: &str) -> Result<()> {
    use std::fmt::Write as _;
    out.push('"');
    for ch in value.chars() {
        if out.len() > RECEIPT_LIMIT.saturating_sub(6) {
            return Err(invalid("site receipt exceeds the 4 MiB limit"));
        }
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < ' ' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    Ok(())
}

#[cfg(feature = "wasm-bindgen")]
#[path = "site_publication_browser.rs"]
mod browser;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::book::BookInput;

    #[test]
    fn receipt_preserves_the_existing_schema_and_reading_order() {
        let renderer = BookRenderer::new(&[
            BookInput { path: "z.md".into(), source: "# Z".into() },
            BookInput { path: "guide/a.md".into(), source: "# A".into() },
        ]).unwrap();
        assert_eq!(receipt_json(renderer.book()).unwrap(),
            "{\"schema\":\"fmd-book-receipt-v1\",\"chapter_count\":2,\"chapters\":[{\"path\":\"z.md\",\"title\":\"Z\",\"output\":\"z.html\"},{\"path\":\"guide/a.md\",\"title\":\"A\",\"output\":\"guide__a.html\"}]}");
    }

    #[test]
    fn receipt_escapes_controls_without_losing_unicode_and_bounds_expansion() {
        let mut text = String::new();
        receipt_string(&mut text, "\"\\\n\r\t\0中").unwrap();
        assert_eq!(text, "\"\\\"\\\\\\n\\r\\t\\u0000中\"");
        assert!(receipt_string(&mut String::new(), &"\0".repeat(RECEIPT_LIMIT / 6 + 1)).is_err());
    }

    #[test]
    fn publication_reuses_the_exact_canonical_archive_entries() {
        let mut renderer = BookRenderer::new(&[
            BookInput { path: "guide/start.md".into(),
                source: "---\ntitle: Start here\nlang: de\n---\n# Same\n\n[Next](../end.md#same)\n\n![Chart](chart.svg)\n".into() },
            BookInput { path: "end.md".into(), source: "# Same\n\n[Back](guide/start.md#same)\n".into() },
        ]).unwrap();
        renderer.set_image("guide/chart.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#.to_vec()).unwrap();
        renderer.options_mut().title = Some("Manual".into());
        renderer.options_mut().custom_css = Some(".fmd { line-height: 1.7; }".into());
        let original = renderer.book().chapters[0].doc.clone();
        let plain = renderer.render_site().unwrap();
        let published = renderer.render_site_publication().unwrap();
        // Adding the receipt happens after all existing local file entries.
        // Find the first central-directory offset from the known writer's EOCD.
        let eocd = plain.len() - 22;
        assert_eq!(&plain[eocd..eocd + 4], b"PK\x05\x06");
        let start = u32::from_le_bytes(plain[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        assert_eq!(&published[..start], &plain[..start]);
        assert!(published.windows(b"frankenmarkdown-receipt.json".len())
            .any(|window| window == b"frankenmarkdown-receipt.json"));
        assert_eq!(published, renderer.render_site_publication().unwrap());
        assert_eq!(renderer.book().chapters[0].doc, original);
    }

    #[test]
    fn publication_keeps_site_collision_and_asset_validation() {
        assert!(BookRenderer::new(&[
            BookInput { path: "a/b.md".into(), source: "# One".into() },
            BookInput { path: "a__b.md".into(), source: "# Two".into() },
        ]).is_err());
        let mut renderer = BookRenderer::new(&[
            BookInput { path: "one.md".into(), source: "# One".into() },
        ]).unwrap();
        renderer.options_mut().pdf_image_assets.push(crate::PdfImageAsset {
            destination: "bad.svg".into(), bytes: vec![],
        });
        assert!(renderer.render_site_publication().is_err());
    }
}
