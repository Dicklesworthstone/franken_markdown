//! Document frontmatter (bead qqst): a leading `---` fenced block carrying
//! per-document metadata.
//!
//! Supports the project's `key=value` grammar and a small YAML-style subset:
//! `key: value`, quoted scalar values, and indented literal (`|`) or folded
//! (`>`) block scalars with optional strip (`-`) or keep (`+`) chomping. This
//! is NOT a general YAML/TOML parser: collections, tags, anchors, and explicit
//! block indentation indicators are not interpreted. Recognized keys:
//! `title`, `author`, `lang`, `toc`, `toc_depth`. Unknown keys are collected
//! (never fatal) so the CLI can warn and editors can lint.
//!
//! A frontmatter block is recognized ONLY at byte 0 (after an optional BOM):
//! the first line must be exactly `---`, a closing line that is exactly `---`
//! must follow, and the body must contain at least one key-value line. Invalid
//! or unsupported block structure leaves the ENTIRE source untouched, so a
//! failed metadata parse never silently discards document content.

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
    let mut block_indent = None;
    for line in lines.by_ref() {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            closed_at = Some(offset + line.len());
            break;
        }
        // Reject ordinary prose early rather than buffering the rest of a
        // document. Indented scalar content is validated by the parser below.
        let indent = leading_spaces(trimmed);
        if is_blank_line(trimmed)
            || trimmed.trim_start_matches([' ', '\t']).starts_with('#')
            || block_indent.is_some_and(|parent| indent > parent)
        {
            body_lines.push(trimmed);
        } else if let Some((_, value)) = split_assignment(trimmed) {
            block_indent = block_scalar_style(value).map(|_| indent);
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

/// The FIRST separator belongs to the key; later `=` or `:` characters are
/// part of the value (notably equations, URLs, and quoted titles).
fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let separator = line.find(['=', ':'])?;
    let key = line[..separator].trim();
    if key.is_empty() {
        return None;
    }
    Some((key, line[separator + 1..].trim()))
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|&byte| byte == b' ').count()
}

fn is_blank_line(line: &str) -> bool {
    line.bytes().all(|byte| byte == b' ' || byte == b'\t')
}

#[derive(Clone, Copy)]
enum Chomping {
    Strip,
    Clip,
    Keep,
}

#[derive(Clone, Copy)]
struct BlockScalarStyle {
    folded: bool,
    chomping: Chomping,
}

fn block_scalar_style(value: &str) -> Option<BlockScalarStyle> {
    // A header comment is not scalar content. A hash without preceding
    // whitespace stays literal, just as it does in an ordinary value.
    let header = value
        .char_indices()
        .find(|&(index, ch)| {
            ch == '#' && (index == 0 || value[..index].ends_with(char::is_whitespace))
        })
        .map_or(value, |(index, _)| &value[..index])
        .trim();
    let (folded, chomping) = match header {
        "|" => (false, Chomping::Clip),
        "|-" => (false, Chomping::Strip),
        "|+" => (false, Chomping::Keep),
        ">" => (true, Chomping::Clip),
        ">-" => (true, Chomping::Strip),
        ">+" => (true, Chomping::Keep),
        _ => return None,
    };
    Some(BlockScalarStyle { folded, chomping })
}

/// Parse a scalar starting after its header, returning the first unconsumed
/// metadata line. Folding works on runs of line breaks: one break between
/// ordinary lines becomes a space, a paragraph loses one break, and breaks
/// adjacent to more-indented text are preserved.
fn parse_block_scalar(
    lines: &[&str],
    start: usize,
    parent_indent: usize,
    style: BlockScalarStyle,
) -> Option<(String, usize)> {
    let mut end = start;
    while end < lines.len()
        && (is_blank_line(lines[end]) || leading_spaces(lines[end]) > parent_indent)
    {
        end += 1;
    }
    let block = &lines[start..end];
    let indent = block
        .iter()
        .find(|line| !is_blank_line(line))
        .map(|line| leading_spaces(line));
    let mut content = Vec::with_capacity(block.len());
    let mut seen_text = false;
    for &line in block {
        let Some(indent) = indent else {
            // An all-blank scalar has line breaks but no textual indentation.
            content.push("");
            continue;
        };
        let blank = is_blank_line(line);
        if !seen_text && blank {
            if leading_spaces(line) > indent {
                return None;
            }
            content.push("");
        } else if leading_spaces(line) >= indent {
            content.push(&line[indent..]);
        } else if blank {
            content.push("");
        } else {
            // A partially dedented body is ambiguous/invalid, not a new key.
            return None;
        }
        seen_text |= !blank;
    }

    let more_indented = |text: &str| text.starts_with(' ') || text.starts_with('\t');
    let mut out = String::new();
    let mut previous: Option<usize> = None;
    for (index, &text) in content.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        if let Some(before) = previous {
            let breaks = index - before;
            if !style.folded || more_indented(content[before]) || more_indented(text) {
                out.extend(std::iter::repeat_n('\n', breaks));
            } else if breaks == 1 {
                out.push(' ');
            } else {
                out.extend(std::iter::repeat_n('\n', breaks - 1));
            }
        } else {
            out.extend(std::iter::repeat_n('\n', index));
        }
        out.push_str(text);
        previous = Some(index);
    }
    let trailing_breaks = previous.map_or(content.len(), |last| content.len() - last);
    out.extend(std::iter::repeat_n('\n', trailing_breaks));
    match style.chomping {
        Chomping::Strip => out.truncate(out.trim_end_matches('\n').len()),
        Chomping::Clip => {
            out.truncate(out.trim_end_matches('\n').len());
            if previous.is_some() {
                out.push('\n');
            }
        }
        Chomping::Keep => {}
    }
    Some((out, end))
}

/// Parse the collected body lines. Empty/comment-only blocks and invalid
/// structure are not frontmatter; callers retain their original source.
fn parse_frontmatter_lines(lines: &[&str]) -> Option<Frontmatter> {
    let mut fm = Frontmatter::default();
    let mut saw_any = false;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        index += 1;
        if is_blank_line(line) || line.trim_start_matches([' ', '\t']).starts_with('#') {
            continue;
        }
        let (key, value) = split_assignment(line)?;
        let value = if let Some(style) = block_scalar_style(value) {
            let (parsed, next) = parse_block_scalar(lines, index, leading_spaces(line), style)?;
            index = next;
            parsed
        } else if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
        {
            // Explicitly quoted whitespace belongs to the metadata value.
            value[1..value.len() - 1].to_string()
        } else {
            value.to_string()
        };
        let value_unquoted = value.as_str();
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
            fm.toc_depth = value_unquoted
                .parse::<u8>()
                .ok()
                .filter(|d| (1..=6).contains(d));
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

    #[test]
    fn first_separator_preserves_equations_and_urls() {
        let src = "---\ntitle: \"E = mc²: a guide\"\nauthor: https://example.test/?a=b\nlang=en:custom\n---\nBody";
        let (fm, rest) = split_frontmatter(src);
        let fm = fm.unwrap();
        assert_eq!(fm.title.as_deref(), Some("E = mc²: a guide"));
        assert_eq!(fm.author.as_deref(), Some("https://example.test/?a=b"));
        assert_eq!(fm.lang.as_deref(), Some("en:custom"));
        assert!(fm.unknown_keys.is_empty());
        assert_eq!(rest, "Body");
    }

    #[test]
    fn multiline_metadata_keeps_following_keys_and_document() {
        let src = "---\ntitle: >- # folded title\n  Durable documentation\n  across output formats.\nauthor: |-\n  Jane Doe\n  Team Rust\ntoc=true\n---\n# Content\n";
        let (fm, rest) = split_frontmatter(src);
        let fm = fm.unwrap();
        assert_eq!(
            fm.title.as_deref(),
            Some("Durable documentation across output formats.")
        );
        assert_eq!(fm.author.as_deref(), Some("Jane Doe\nTeam Rust"));
        assert_eq!(fm.toc, Some(true));
        assert_eq!(rest, "# Content\n");
    }

    #[test]
    fn literal_and_folded_chomping_modes() {
        for (header, expected) in [
            ("|", "one\ntwo\n"),
            ("|-", "one\ntwo"),
            ("|+", "one\ntwo\n\n\n"),
            (">", "one two\n"),
            (">-", "one two"),
            (">+", "one two\n\n\n"),
        ] {
            let src = format!("---\ntitle: {header}\n  one\n  two\n\n\n---\nBody");
            let (fm, rest) = split_frontmatter(&src);
            assert_eq!(fm.unwrap().title.as_deref(), Some(expected), "{header}");
            assert_eq!(rest, "Body");
        }
    }

    #[test]
    fn folding_preserves_paragraphs_and_more_indented_text() {
        for (body, expected) in [
            ("  one\n\n  two\n  three\n", "one\ntwo three"),
            ("  one\n    code\n\n  two\n", "one\n  code\n\ntwo"),
            ("  one\n\n    code\n  two\n", "one\n\n  code\ntwo"),
        ] {
            let src = format!("---\ntitle: >-\n{body}---\n");
            assert_eq!(
                split_frontmatter(&src).0.unwrap().title.as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn scalar_content_is_not_mistaken_for_metadata_or_fences() {
        let src = "---\ntitle: |-\n  title=inside\n  author: inside\n  # literal comment\n  ---\nlang: en\n---\nBody";
        let (fm, rest) = split_frontmatter(src);
        let fm = fm.unwrap();
        assert_eq!(
            fm.title.as_deref(),
            Some("title=inside\nauthor: inside\n# literal comment\n---")
        );
        assert_eq!(fm.author, None);
        assert_eq!(fm.lang.as_deref(), Some("en"));
        assert_eq!(rest, "Body");
    }

    #[test]
    fn malformed_scalar_or_empty_key_preserves_entire_source() {
        for src in [
            "---\ntitle: |\n    one\n  partial dedent\n---\nBody",
            "---\ntitle: |\n  one\nunindented prose\n---\nBody",
            "---\n=not a key\n---\nBody",
            "---\n: not a key\n---\nBody",
            "---\ntitle: |\n    \n  text\n---\nBody",
            "\u{feff}---\ntitle: |\n  unclosed\n",
        ] {
            let (fm, rest) = split_frontmatter(src);
            assert!(fm.is_none(), "{src:?}");
            assert_eq!(rest, src);
        }
    }

    #[test]
    fn crlf_and_bom_keep_exact_body_slice() {
        let src = "\u{feff}---\r\ntitle: >-\r\n  first\r\n  second\r\n---\r\n# Body\r\n";
        let (fm, rest) = split_frontmatter(src);
        assert_eq!(fm.unwrap().title.as_deref(), Some("first second"));
        assert_eq!(rest, "# Body\r\n");
    }

    #[test]
    fn empty_scalars_and_leading_blank_lines() {
        for (header, expected) in [("|", ""), ("|-", ""), ("|+", "\n")] {
            let src = format!("---\ntitle: {header}\n\nlang: en\n---\n");
            let fm = split_frontmatter(&src).0.unwrap();
            assert_eq!(fm.title.as_deref(), Some(expected));
            assert_eq!(fm.lang.as_deref(), Some("en"));
        }
        let src = "---\ntitle: |+\n\n  first\n---\n";
        assert_eq!(
            split_frontmatter(src).0.unwrap().title.as_deref(),
            Some("\nfirst\n")
        );
    }

    #[test]
    fn blank_indentation_is_not_text_but_unicode_whitespace_is() {
        let src = "---\ntitle: |+\n    \n  \n---\n";
        assert_eq!(
            split_frontmatter(src).0.unwrap().title.as_deref(),
            Some("\n\n")
        );
        let src = "---\ntitle: |-\n  \u{a0}\n---\n";
        assert_eq!(
            split_frontmatter(src).0.unwrap().title.as_deref(),
            Some("\u{a0}")
        );
    }

    #[test]
    fn unknown_block_scalars_and_quoted_whitespace_are_preserved() {
        let src = "---\n  # metadata comment\nnotes: |-\n  unknown metadata\n  is still consumed\ntitle: '  spaced  '\nauthor=\"  Jane  \"\n---\nBody";
        let (fm, rest) = split_frontmatter(src);
        let fm = fm.unwrap();
        assert_eq!(fm.title.as_deref(), Some("  spaced  "));
        assert_eq!(fm.author.as_deref(), Some("  Jane  "));
        assert_eq!(fm.unknown_keys, ["notes"]);
        assert_eq!(rest, "Body");
    }
}
