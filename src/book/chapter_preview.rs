//! On-demand book HTML, without a full-site render or a ZIP round trip.
//!
//! Only the selected AST is copied and rendered. Navigation still names the
//! complete captured book; links and image keys use the site's own resolvers.

use super::{
    BTreeMap, BTreeSet, BookRenderer, MAX_CHAPTERS, Result, asset_keys, checked_paths,
    inject_book_nav, invalid, json_string, out_name, render_html_document, resolve_images,
    resolve_site_links, site_search, validate_assets,
};

const PAGE_BYTES: usize = 8 * 1024 * 1024;
const MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const METADATA_BYTES: usize = 4096;

impl BookRenderer {
    /// Render exactly one zero-based chapter as self-contained HTML. The page
    /// carries the same chapter title, language, navigation, styles, fonts and
    /// chapter-relative images as the corresponding `render_site()` ZIP member.
    /// No other chapter is cloned or rendered and no search index or ZIP is built.
    ///
    /// The returned HTML is not a security boundary: browser hosts must use the
    /// same isolated reader and resource policy as for ordinary site previews.
    ///
    /// # Errors
    /// Rejects an invalid index, paths/assets, or a rendered page over 8 MiB.
    /// The HTML emitter's transient allocation is not bounded by that output
    /// ceiling. As with site exports, rendering is synchronous and read-only.
    pub fn render_chapter_html(&self, selected: usize) -> Result<String> {
        let sources = preview_paths(self, selected)?;
        self.chapter_html_with_paths(selected, &sources)
    }

    /// Return `fmd-book-chapter-preview-v1` UTF-8 JSON: a complete ordered
    /// chapter map, the selected index, and only that chapter's generated HTML.
    /// Unlike the legacy all-pages preview, a 129..=4096-chapter book need not
    /// render every chapter or fit all chapter HTML into one message.
    ///
    /// # Errors
    /// Uses `render_chapter_html` validation, additionally limiting source/title
    /// metadata to 4096 UTF-8 bytes each and the encoded message to 64 MiB.
    /// Metadata is admitted before rendering. An error never mutates the book.
    pub fn render_chapter_preview(&self, selected: usize) -> Result<Vec<u8>> {
        self.chapter_preview_with_limits(selected, PAGE_BYTES, MESSAGE_BYTES)
    }

    fn chapter_preview_with_limits(
        &self,
        selected: usize,
        page_limit: usize,
        message_limit: usize,
    ) -> Result<Vec<u8>> {
        let sources = preview_paths(self, selected)?;
        let mut json = String::new();
        append(
            &mut json,
            &format!(
                "{{\"schema\":\"fmd-book-chapter-preview-v1\",\"selected\":{selected},\"pages\":["
            ),
            message_limit,
        )?;
        for (index, (chapter, source)) in self.book.chapters.iter().zip(&sources).enumerate() {
            if source.len() > METADATA_BYTES || chapter.title.len() > METADATA_BYTES {
                return Err(invalid("chapter preview metadata exceeds 4096 bytes"));
            }
            if index != 0 {
                append(&mut json, ",", message_limit)?;
            }
            for (key, value) in [
                ("{\"path\":", chapter.out_name.as_str()),
                (",\"source\":", source.as_str()),
                (",\"title\":", chapter.title.as_str()),
            ] {
                append(&mut json, key, message_limit)?;
                append_json(&mut json, value, message_limit)?;
            }
            append(&mut json, "}", message_limit)?;
        }
        append(&mut json, "],\"html\":", message_limit)?;
        // The lookup map reads chapter metadata, not any unselected AST. Even
        // an enormous unrelated chapter does not enter rendering or compression.
        let html = self.chapter_html_with_paths(selected, &sources)?;
        if html.len() > page_limit {
            return Err(invalid("chapter preview HTML exceeds the page byte limit"));
        }
        append_json(&mut json, &html, message_limit)?;
        append(&mut json, "}", message_limit)?;
        Ok(json.into_bytes())
    }

    fn chapter_html_with_paths(&self, selected: usize, sources: &[String]) -> Result<String> {
        validate_assets(&self.options.pdf_image_assets)?;
        let chapter = self
            .book
            .chapters
            .get(selected)
            .ok_or_else(|| invalid("chapter preview index is outside this book"))?;
        let source = sources
            .get(selected)
            .ok_or_else(|| invalid("chapter preview source map is incomplete"))?;
        let known: BTreeMap<_, _> = sources
            .iter()
            .zip(&self.book.chapters)
            .map(|(source, chapter)| (source.as_str(), chapter.out_name.as_str()))
            .collect();
        let keys = asset_keys(&self.options.pdf_image_assets);
        let mut doc = chapter.doc.clone();
        resolve_site_links(&mut doc.blocks, source, &known);
        resolve_images(&mut doc.blocks, source, &keys);
        let mut options = self.options.html_options();
        options.title = Some(chapter.title.clone());
        options.lang = chapter
            .frontmatter
            .as_ref()
            .and_then(|frontmatter| frontmatter.lang.clone())
            .or_else(|| options.lang.clone());
        let html = render_html_document(&doc, &options)?;
        if html.len() > PAGE_BYTES {
            return Err(invalid("chapter preview HTML exceeds the 8 MiB limit"));
        }
        let html = inject_book_nav(&html, &self.book, &chapter.out_name);
        let html = site_search::inject_link(&html);
        if html.len() > PAGE_BYTES {
            return Err(invalid("chapter preview HTML exceeds the 8 MiB limit"));
        }
        Ok(html)
    }
}

fn preview_paths(renderer: &BookRenderer, selected: usize) -> Result<Vec<String>> {
    if selected >= renderer.book.chapters.len() || selected >= MAX_CHAPTERS {
        return Err(invalid("chapter preview index is outside this book"));
    }
    let sources = checked_paths(&renderer.book)?;
    let mut names = BTreeSet::new();
    for (chapter, source) in renderer.book.chapters.iter().zip(&sources) {
        if chapter.out_name != out_name(source)
            || chapter.out_name.len() > 255
            || chapter.out_name.eq_ignore_ascii_case("index.html")
            || !names.insert(chapter.out_name.to_ascii_lowercase())
        {
            return Err(invalid("invalid, nonportable, or colliding HTML output filename"));
        }
    }
    Ok(sources)
}

fn admit(current: usize, extra: usize, limit: usize) -> Result<()> {
    if current.checked_add(extra).is_none_or(|total| total > limit) {
        return Err(invalid("chapter preview JSON exceeds the message byte limit"));
    }
    Ok(())
}

fn append(output: &mut String, value: &str, limit: usize) -> Result<()> {
    admit(output.len(), value.len(), limit)?;
    output.push_str(value);
    Ok(())
}

// Preflight the exact escaped byte length before allocating the JSON string.
// Non-ASCII UTF-8 passes through unchanged; C0 controls can expand sixfold.
fn append_json(output: &mut String, value: &str, limit: usize) -> Result<()> {
    let mut length = 2usize;
    for byte in value.bytes() {
        let extra = match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' => 2,
            0..=31 => 6,
            _ => 1,
        };
        length = length
            .checked_add(extra)
            .ok_or_else(|| invalid("chapter preview JSON size overflow"))?;
        admit(output.len(), length, limit)?;
    }
    admit(output.len(), length, limit)?;
    output.push_str(&json_string(value));
    Ok(())
}

#[cfg(test)]
#[path = "chapter_preview_tests.rs"]
mod tests;
