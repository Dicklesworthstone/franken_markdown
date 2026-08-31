//! Document frontmatter (bead qqst): a leading `---` fenced block of
//! `key=value` lines carrying per-document metadata.
//!
//! Deliberately a `key=value` subset (the project's existing config grammar),
//! NOT YAML/TOML: the zero-dependency doctrine rules out real YAML parsers,
//! and a documented minimal subset surprises nobody. Recognized keys:
//! `title`, `author`, `lang`, `toc`, `toc_depth`. Unknown keys are collected
//! (never fatal) so the CLI can warn and editors can lint.
//!
//! A frontmatter block is recognized ONLY at byte 0 (after an optional BOM):
//! the first line must be exactly `---`, a closing line that is exactly `---`
//! must follow, and the body must contain at least one `key=value` line. A
//! leading `---` that fails any of those is parsed as ordinary content (a
//! thematic break), matching reader expectations for non-frontmatter docs.

/// Parsed frontmatter values. `None`/absent keys leave the render defaults
/// (first-heading title, no author, language autodetect).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub lang: Option<String>,
    pub toc: Option<bool>,
    pub toc_depth: Option<u8>,
    /// Unrecognized keys (in source order) for warning/lint surfaces.
    pub unknown_keys: Vec<String>,
}

/// Split a leading frontmatter block from the source. Returns the parsed
/// frontmatter and the remaining source (starting on the line after the
/// closing fence). When no valid frontmatter block exists, returns
/// `(None, src)` with the input untouched.
#[must_use]
pub fn split_frontmatter(src: &str) -> (Option<Frontmatter>, &str) {
    let body = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut lines = body.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (None, src);
    };
    if first.trim_end_matches(['\n', '\r']) != "---" {
        return (None, src);
    }
    let mut offset = first.len();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut closed_at = None;
    for line in lines.by_ref() {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            closed_at = Some(offset + line.len());
            break;
        }
        // Frontmatter bodies are line-oriented key=value or key: value; a blank line,
        // comment (#), or conforming key-value line is accepted.
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.contains('=')
            || trimmed.contains(':')
        {
            body_lines.push(trimmed);
        } else {
            return (None, src);
        }
        offset += line.len();
    }
    let Some(end) = closed_at else {
        return (None, src);
    };
    let fm = parse_frontmatter_lines(&body_lines);
    if fm.is_none() {
        return (None, src);
    }
    (fm, &body[end..])
}

/// Parse the collected body lines. Returns None when no key=value or key: value line is
/// present (an empty or comment-only block is not frontmatter).
fn parse_frontmatter_lines(lines: &[&str]) -> Option<Frontmatter> {
    let mut fm = Frontmatter::default();
    let mut saw_any = false;
    for line in lines {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let pair = line.split_once('=').or_else(|| line.split_once(':'));
        let Some((key, value)) = pair else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let value_unquoted = if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
        {
            value[1..value.len() - 1].trim()
        } else {
            value
        };
        saw_any = true;
        if key.eq_ignore_ascii_case("title") {
            fm.title = Some(value_unquoted.to_string());
        } else if key.eq_ignore_ascii_case("author") {
            fm.author = Some(value_unquoted.to_string());
        } else if key.eq_ignore_ascii_case("lang") {
            fm.lang = Some(value_unquoted.to_string());
        } else if key.eq_ignore_ascii_case("toc") {
            fm.toc = if value_unquoted.eq_ignore_ascii_case("true")
                || value_unquoted.eq_ignore_ascii_case("yes")
                || value_unquoted.eq_ignore_ascii_case("on")
                || value_unquoted == "1"
            {
                Some(true)
            } else if value_unquoted.eq_ignore_ascii_case("false")
                || value_unquoted.eq_ignore_ascii_case("no")
                || value_unquoted.eq_ignore_ascii_case("off")
                || value_unquoted == "0"
            {
                Some(false)
            } else {
                None
            };
        } else if key.eq_ignore_ascii_case("toc_depth") {
            fm.toc_depth = value_unquoted.parse::<u8>().ok().filter(|d| (1..=6).contains(d));
        } else {
            fm.unknown_keys.push(key.to_string());
        }
    }
    saw_any.then_some(fm)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_only_at_document_start() {
        let src = "# Title\n\n---\ntitle=not metadata\n---\n";
        let (fm, rest) = split_frontmatter(src);
        assert!(fm.is_none(), "mid-document fences are not frontmatter");
        assert_eq!(rest, src);
    }

    #[test]
    fn parses_recognized_and_collects_unknown() {
        let src = "---\ntitle=My Book\nauthor=Jane\nlang=de\ntoc=true\ntoc_depth=2\nflavor=x\n---\n# Hi\n";
        let (fm, rest) = split_frontmatter(src);
        let fm = fm.expect("frontmatter recognized");
        assert_eq!(fm.title.as_deref(), Some("My Book"));
        assert_eq!(fm.author.as_deref(), Some("Jane"));
        assert_eq!(fm.lang.as_deref(), Some("de"));
        assert_eq!(fm.toc, Some(true));
        assert_eq!(fm.toc_depth, Some(2));
        assert_eq!(fm.unknown_keys, ["flavor"]);
        assert_eq!(rest, "# Hi\n");
    }

    #[test]
    fn rejects_unclosed_and_empty_blocks() {
        assert!(split_frontmatter("---\ntitle=x\n# Hi\n").0.is_none());
        assert!(split_frontmatter("---\n---\n# Hi\n").0.is_none());
        assert!(split_frontmatter("---\njust words\n---\n").0.is_none());
    }

    #[test]
    fn bom_prefixed_frontmatter() {
        let src = "\u{feff}---\ntitle=BOM Doc\n---\n# Hi\n";
        let (fm, rest) = split_frontmatter(src);
        assert_eq!(
            fm.expect("bom frontmatter").title.as_deref(),
            Some("BOM Doc")
        );
        assert_eq!(rest, "# Hi\n");
    }

    #[test]
    fn parses_yaml_style_colon_and_quoted_values() {
        let src = "---\n# Comment line\ntitle: \"YAML Title\"\nauthor: 'Alice'\nlang: fr\ntoc: yes\ntoc_depth: 3\n---\n# Content\n";
        let (fm, rest) = split_frontmatter(src);
        let fm = fm.expect("yaml frontmatter recognized");
        assert_eq!(fm.title.as_deref(), Some("YAML Title"));
        assert_eq!(fm.author.as_deref(), Some("Alice"));
        assert_eq!(fm.lang.as_deref(), Some("fr"));
        assert_eq!(fm.toc, Some(true));
        assert_eq!(fm.toc_depth, Some(3));
        assert_eq!(rest, "# Content\n");
    }
}
