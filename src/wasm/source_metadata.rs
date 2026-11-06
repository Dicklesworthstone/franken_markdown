//! Source-owned render defaults, using the same frontmatter parser as the CLI.

/// Optional render defaults from the document's leading frontmatter.
///
/// Hosts merge these values below explicit options, then pass the resolved
/// settings to the existing render APIs. Keeping `toc` optional distinguishes
/// an omitted option from an explicit `false`; [`super::WasmRenderOptions`]
/// continues to describe already-resolved rendering settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WasmSourceMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub lang: Option<String>,
    pub toc: Option<bool>,
    pub toc_depth: Option<u8>,
}

/// Read source defaults without rewriting the source or its byte offsets.
///
/// Uses the native parser's `key=value`, YAML-style scalar and block-scalar
/// grammar, including BOM and CR/CRLF handling. Absent or malformed frontmatter
/// returns empty defaults. Unknown keys do not become render settings.
/// Literal NUL in metadata becomes U+FFFD, as in the parsed document body.
#[must_use]
pub fn source_metadata(markdown: &str) -> WasmSourceMetadata {
    let Some(metadata) = crate::parse::split_frontmatter(markdown).0 else {
        return WasmSourceMetadata::default();
    };
    WasmSourceMetadata {
        title: metadata.title,
        author: metadata.author,
        lang: metadata.lang,
        toc: metadata.toc,
        toc_depth: metadata.toc_depth,
    }
}

impl WasmSourceMetadata {
    /// Browser option spellings with absent values omitted. Text is preserved
    /// exactly, including quoted whitespace, Unicode and control characters.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::from("{");
        for (key, value) in [
            ("title", self.title.as_deref()),
            ("author", self.author.as_deref()),
            ("lang", self.lang.as_deref()),
        ] {
            if let Some(value) = value {
                member(&mut out, key);
                out.push('"');
                json_text(&mut out, value);
                out.push('"');
            }
        }
        if let Some(value) = self.toc {
            member(&mut out, "toc");
            out.push_str(if value { "true" } else { "false" });
        }
        if let Some(value) = self.toc_depth {
            member(&mut out, "tocDepth");
            out.push_str(&value.to_string());
        }
        out.push('}');
        out
    }
}

fn member(out: &mut String, key: &str) {
    if out.len() > 1 {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
}

fn json_text(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{1f}' => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
}
