//! Offline, reader-facing search shared by native and ZIP book exports.
//! A single generated page owns the index; chapter pages contain only a link.

use crate::book::Book;
use crate::{RenderError, Result, search_index_json};
use crate::search_index::build_full_search_index;

// Source-path encoding always escapes literal '~' bytes. This host filename
// cannot collide with a chapter produced by out_name, including search.md.
pub(crate) const PAGE_NAME: &str = "~fmd-search.html";
const MAX_INDEX_BYTES: usize = 32 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 250_000;
const SCRIPT: &str = include_str!("site_search.js");
const NAV_START: &str = "<nav class=\"fmd-book-nav\" aria-label=\"Book contents\">\n";

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book_search: {message}"))
}

/// Reuse the engine's actual emitted heading IDs, not JavaScript slug guesses.
/// Validate the complete index before publication; never silently truncate it.
pub(crate) fn index_json(book: &Book) -> Result<String> {
    let sources = super::checked_paths(book)?;
    let mut names = std::collections::BTreeSet::new();
    let mut count = 0usize;
    let mut json = String::from("{\"schema\":\"fmd-book-search-index-v1\",\"chapters\":[");
    for (i, (chapter, source)) in book.chapters.iter().zip(&sources).enumerate() {
        if chapter.out_name != crate::book::out_name(source)
            || chapter.out_name.len() > 255
            || !names.insert(chapter.out_name.to_ascii_lowercase())
        {
            return Err(invalid("invalid or colliding chapter output name"));
        }
        let index = build_full_search_index(&chapter.doc);
        count = count.checked_add(index.entries.len()).and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("entry count overflow"))?;
        if count > MAX_ENTRIES {
            return Err(invalid("more than 250000 searchable entries"));
        }
        if i > 0 { json.push(','); }
        let document = search_index_json(&index);
        let entry = format!(
            "{{\"source\":{},\"page\":{},\"title\":{},\"index\":{}}}",
            super::json_string(source), super::json_string(&chapter.out_name),
            super::json_string(&chapter.title), document,
        );
        if entry.len() > MAX_INDEX_BYTES.saturating_sub(json.len() + 2) {
            return Err(invalid("index exceeds the 32 MiB limit"));
        }
        json.push_str(&entry);
    }
    json.push_str("]}");
    Ok(json)
}

/// Only alter the generated navigation boundary, never arbitrary document HTML.
/// Browser hosts using inject_book_nav alone do not acquire a nonexistent link.
pub(crate) fn inject_link(rendered: &str) -> String {
    let Some(position) = rendered.find(NAV_START) else { return rendered.to_string(); };
    let position = position + NAV_START.len();
    let mut out = String::with_capacity(rendered.len() + 96);
    out.push_str(&rendered[..position]);
    out.push_str("<p><a class=\"fmd-book-search-link\" href=\"./");
    out.push_str(PAGE_NAME);
    out.push_str("\">Search this book</a></p>\n");
    out.push_str(&rendered[position..]);
    out
}

/// JSON inside a non-executable script element still obeys HTML's raw-text
/// termination rules. Escape every '<' before inserting any source data.
pub(crate) fn page(index: &str, title: &str, lang: Option<&str>) -> Result<String> {
    if index.len() > MAX_INDEX_BYTES || title.len() > MAX_INDEX_BYTES {
        return Err(invalid("index or title exceeds the 32 MiB limit"));
    }
    let title = crate::book::escape_text_pub(title);
    let lang = crate::book::escape_attr_pub(lang.unwrap_or("en"));
    if lang.len() > 1024 {
        return Err(invalid("language tag exceeds 1024 bytes"));
    }
    let mut html = format!(
        "<!DOCTYPE html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\">\
         <title>Search — {title}</title><style>\
         :root{{color-scheme:light dark}}\
         body{{font:1rem/1.6 system-ui,sans-serif;max-width:64rem;margin:2rem auto;padding:0 1rem}}\
         a{{color:LinkText}}input,button{{font:inherit;padding:.5rem}}\
         input{{width:min(75%,40rem)}}label{{display:block;font-weight:600}}\
         li{{padding:.5rem 0}}li p{{margin:.25rem 0;white-space:pre-wrap;overflow-wrap:anywhere}}\
         nav{{display:flex;gap:1rem}}button:disabled{{opacity:.5}}\
         </style></head><body><header><a href=\"./index.html\">Book contents</a>\
         <h1>Search — {title}</h1></header>\
         <form id=\"search-form\" role=\"search\"><label for=\"query\">Search this book</label>\
         <input id=\"query\" name=\"q\" type=\"search\" maxlength=\"512\" autocomplete=\"off\" \
         aria-describedby=\"search-help\" aria-controls=\"results\"><button type=\"submit\">Search</button></form>\
         <p id=\"search-help\">All words must match. Use quotation marks for an exact phrase. \
         Search runs locally; no network connection is needed.</p>\
         <p id=\"status\" role=\"status\" aria-live=\"polite\">Enter words or a quoted phrase to search this book.</p>\
         <ol id=\"results\"></ol><nav aria-label=\"Search result pages\">\
         <button id=\"previous\" type=\"button\" disabled>Previous results</button>\
         <button id=\"next\" type=\"button\" disabled>Next results</button></nav>\
         <noscript>Enable JavaScript to search, or use the Book contents link to browse the chapters.</noscript>\
         <script id=\"search-data\" type=\"application/json\">"
    );
    for ch in index.chars() {
        let escaped = match ch {
            '<' => Some("\\u003c"), '>' => Some("\\u003e"), '&' => Some("\\u0026"),
            '\u{2028}' => Some("\\u2028"), '\u{2029}' => Some("\\u2029"), _ => None,
        };
        let length = escaped.map_or(ch.len_utf8(), str::len);
        if length > MAX_PAGE_BYTES.saturating_sub(html.len()) {
            return Err(invalid("search page exceeds the 128 MiB limit"));
        }
        if let Some(escaped) = escaped { html.push_str(escaped); } else { html.push(ch); }
    }
    let tail = "</script><script>";
    let end = "</script></body></html>\n";
    if SCRIPT.len() + tail.len() + end.len() > MAX_PAGE_BYTES.saturating_sub(html.len()) {
        return Err(invalid("search page exceeds the 128 MiB limit"));
    }
    html.push_str(tail);
    html.push_str(SCRIPT);
    html.push_str(end);
    Ok(html)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::book::{BookInput, build_book};

    fn book() -> Book {
        build_book(&[
            BookInput { path: "guide/start.md".into(), source: "# Same\n\nOne.\n\n# Same\n\nTwo.".into() },
            BookInput { path: "search.md".into(), source: "# Other\n\nThree.".into() },
        ]).unwrap()
    }

    #[test]
    fn index_uses_real_chapter_paths_and_collision_safe_anchors() {
        let book = book();
        let json = index_json(&book).unwrap();
        assert!(json.contains("\"source\":\"guide/start.md\",\"page\":\"guide__start.html\""));
        assert!(json.contains("\"anchor\":\"same-2\""));
        assert!(json.contains("\"page\":\"search.html\""));
        assert_ne!(crate::book::out_name("~fmd-search.md"), PAGE_NAME);
        assert_eq!(index_json(&book).unwrap(), json);
    }

    #[test]
    fn raw_text_termination_and_title_markup_are_escaped() {
        let dangerous = "</script><img src=x onerror=alert(1)>&\u{2028}\u{2029}";
        let mut book = book();
        book.chapters[0].title = dangerous.into();
        let json = index_json(&book).unwrap();
        let html = page(&json, dangerous, Some("en\" onload=\"bad")).unwrap();
        assert!(!html.contains(dangerous));
        assert!(html.contains("\\u003c/script\\u003e"));
        assert!(html.contains("\\u0026\\u2028\\u2029"));
        assert!(html.contains("en&quot; onload=&quot;bad"));
        assert!(html.contains("default-src 'none'"));
        assert_eq!(html.matches("<script").count(), 2);
        assert_eq!(html.matches("</script>").count(), 2);
    }

    #[test]
    fn navigation_link_is_inserted_only_for_exported_sites() {
        let html = format!("<body>{NAV_START}<ul></ul></nav><main>Text</main></body>");
        let linked = inject_link(&html);
        assert!(linked.contains("href=\"./~fmd-search.html\""));
        assert!(linked.contains("<ul></ul></nav><main>Text</main>"));
        assert_eq!(inject_link("<main>Text</main>"), "<main>Text</main>");
    }

    #[test]
    fn invalid_manual_books_and_oversized_indexes_fail_before_publication() {
        let mut book = book();
        book.chapters[0].out_name = "javascript:bad.html".into();
        assert!(index_json(&book).is_err());
        assert!(index_json(&Book { chapters: vec![] }).is_err());
        assert!(page(&"x".repeat(MAX_INDEX_BYTES + 1), "Book", None).is_err());
    }

    #[test]
    fn exported_index_contains_code_tables_and_referenced_notes() {
        let book = build_book(&[BookInput {
            path: "guide.md".into(),
            source: "# Guide\n\n```rust\nlaunch_unique();\n```\n\n| option | value |\n| --- | --- |\n| timeout | 30 |\n\nRead[^n].\n\n[^n]: unique citation body\n".into(),
        }]).unwrap();
        let json = index_json(&book).unwrap();
        assert!(json.contains("launch_unique();"));
        assert!(json.contains("timeout 30"));
        assert!(json.contains("unique citation body"));
        assert!(json.contains("\"anchor\":\"fn-n\""));
    }
}
