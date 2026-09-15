#![forbid(unsafe_code)]

//! HTML incremental lexer (FCB-022 · fcb-9vx.21).
//!
//! Classifies HTML/XML/SVG/XHTML source into byte-exact [`Span`]s that tile the input.
//! Implements:
//! - HTML comments (`<!-- ... -->`) and conditional comments.
//! - DOCTYPE declarations (`<!DOCTYPE html ...>`) and XML declarations (`<?xml ... ?>`).
//! - CDATA sections (`<![CDATA[ ... ]]>`).
//! - Tags: open (`<div>`), close (`</div>`), self-closing (`<br/>`, `<img ... />`).
//! - Attribute names and quoted/unquoted attribute values (`"..."`, `'...'`).
//! - Named and numeric character entities (`&amp;`, `&lt;`, `&gt;`, `&#38;`, `&#x26;`).
//! - Embedded `<script>` and `<style>` content boundaries (inert source-only treatment).
//! - Exact source byte tiling across whole inputs and arbitrary splits.

use crate::highlight::{Span, Tok};

/// Declared capability for the HTML lexical route (FCB-022.21).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HtmlCapabilityV1 {
    pub tags: bool,
    pub attributes: bool,
    pub comments: bool,
    pub entities: bool,
    pub doctype: bool,
    pub cdata: bool,
    pub embedded_blocks: bool,
}

impl HtmlCapabilityV1 {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            tags: true,
            attributes: true,
            comments: true,
            entities: true,
            doctype: true,
            cdata: true,
            embedded_blocks: true,
        }
    }
}

/// Lex HTML source into exact tiling spans.
pub fn lex_html_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0;
    let bytes = code.as_bytes();

    while pos < len {
        let rest = &code[pos..];

        // 1. HTML Comment: `<!-- ... -->`
        if rest.starts_with("<!--") {
            let start = pos;
            pos += 4;
            if let Some(offset) = code[pos..].find("-->") {
                pos += offset + 3;
            } else {
                pos = len;
            }
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end: pos,
            });
            continue;
        }

        // 2. CDATA Section: `<![CDATA[ ... ]]>`
        if rest.starts_with("<![CDATA[") {
            let start = pos;
            pos += 9;
            if let Some(offset) = code[pos..].find("]]>") {
                pos += offset + 3;
            } else {
                pos = len;
            }
            spans.push(Span {
                kind: Tok::Str,
                start,
                end: pos,
            });
            continue;
        }

        // 3. DOCTYPE or declaration: `<!DOCTYPE ...>` or `<!...>`
        if rest.starts_with("<!") {
            let start = pos;
            pos += 2;
            let mut in_quotes: Option<u8> = None;
            while pos < len {
                let b = bytes[pos];
                if let Some(q) = in_quotes {
                    if b == q {
                        in_quotes = None;
                    }
                } else if b == b'"' || b == b'\'' {
                    in_quotes = Some(b);
                } else if b == b'>' {
                    pos += 1;
                    break;
                }
                pos += 1;
            }
            spans.push(Span {
                kind: Tok::Keyword,
                start,
                end: pos,
            });
            continue;
        }

        // 4. XML processing instruction: `<?xml ... ?>` or `<?...?>`
        if rest.starts_with("<?") {
            let start = pos;
            pos += 2;
            if let Some(offset) = code[pos..].find("?>") {
                pos += offset + 2;
            } else {
                pos = len;
            }
            spans.push(Span {
                kind: Tok::Keyword,
                start,
                end: pos,
            });
            continue;
        }

        // 5. HTML Tag: `<...>`
        if bytes[pos] == b'<' {
            let start = pos;
            pos += 1;

            // Check for closing tag slash: `</`
            let is_closing = pos < len && bytes[pos] == b'/';
            if is_closing {
                pos += 1;
            }

            // Tag name
            let tag_name_start = pos;
            while pos < len && (bytes[pos] == b'_' || bytes[pos] == b'-' || bytes[pos] == b':' || bytes[pos].is_ascii_alphanumeric()) {
                pos += 1;
            }
            let has_tag_name = pos > tag_name_start;

            if !has_tag_name && !is_closing {
                // Lone `<` not followed by tag name or `/` is plain/operator
                spans.push(Span {
                    kind: Tok::Operator,
                    start,
                    end: pos,
                });
                continue;
            }

            // Push `<` or `</`
            spans.push(Span {
                kind: Tok::Operator,
                start,
                end: tag_name_start,
            });

            if has_tag_name {
                let tag_name = &code[tag_name_start..pos];
                let is_script_or_style = tag_name.eq_ignore_ascii_case("script") || tag_name.eq_ignore_ascii_case("style");
                let is_open_script_or_style = is_script_or_style && !is_closing;

                spans.push(Span {
                    kind: Tok::Keyword,
                    start: tag_name_start,
                    end: pos,
                });

                let mut is_self_closing = false;

                // Attributes loop inside `<tag ...>`
                while pos < len {
                    // Whitespace
                    if bytes[pos].is_ascii_whitespace() {
                        let ws_start = pos;
                        while pos < len && bytes[pos].is_ascii_whitespace() {
                            pos += 1;
                        }
                        spans.push(Span {
                            kind: Tok::Plain,
                            start: ws_start,
                            end: pos,
                        });
                        continue;
                    }

                    // Self-closing `/>` or closing `>`
                    if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'>' {
                        spans.push(Span {
                            kind: Tok::Operator,
                            start: pos,
                            end: pos + 2,
                        });
                        pos += 2;
                        is_self_closing = true;
                        break;
                    }
                    if bytes[pos] == b'>' {
                        spans.push(Span {
                            kind: Tok::Operator,
                            start: pos,
                            end: pos + 1,
                        });
                        pos += 1;
                        break;
                    }

                    // Attribute name
                    let attr_name_start = pos;
                    while pos < len && (bytes[pos] == b'_' || bytes[pos] == b'-' || bytes[pos] == b':' || bytes[pos] == b'.' || bytes[pos].is_ascii_alphanumeric()) {
                        pos += 1;
                    }
                    if pos > attr_name_start {
                        spans.push(Span {
                            kind: Tok::Type,
                            start: attr_name_start,
                            end: pos,
                        });

                        // Optional whitespace before `=`
                        let ws_eq_start = pos;
                        while pos < len && bytes[pos].is_ascii_whitespace() {
                            pos += 1;
                        }
                        if pos > ws_eq_start {
                            spans.push(Span {
                                kind: Tok::Plain,
                                start: ws_eq_start,
                                end: pos,
                            });
                        }

                        // Optional `=`
                        if pos < len && bytes[pos] == b'=' {
                            spans.push(Span {
                                kind: Tok::Operator,
                                start: pos,
                                end: pos + 1,
                            });
                            pos += 1;

                            // Optional whitespace after `=`
                            let ws_val_start = pos;
                            while pos < len && bytes[pos].is_ascii_whitespace() {
                                pos += 1;
                            }
                            if pos > ws_val_start {
                                spans.push(Span {
                                    kind: Tok::Plain,
                                    start: ws_val_start,
                                    end: pos,
                                });
                            }

                            // Attribute value: double-quoted, single-quoted, or unquoted
                            if pos < len && (bytes[pos] == b'"' || bytes[pos] == b'\'') {
                                let quote = bytes[pos];
                                let val_start = pos;
                                pos += 1;
                                while pos < len && bytes[pos] != quote {
                                    pos += 1;
                                }
                                if pos < len && bytes[pos] == quote {
                                    pos += 1;
                                }
                                spans.push(Span {
                                    kind: Tok::Str,
                                    start: val_start,
                                    end: pos,
                                });
                            } else if pos < len && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'>' && bytes[pos] != b'/' {
                                // Unquoted attribute value
                                let val_start = pos;
                                while pos < len && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'>' && bytes[pos] != b'/' {
                                    pos += 1;
                                }
                                spans.push(Span {
                                    kind: Tok::Str,
                                    start: val_start,
                                    end: pos,
                                });
                            }
                        }
                        continue;
                    }

                    // Any stray character inside tag
                    spans.push(Span {
                        kind: Tok::Plain,
                        start: pos,
                        end: pos + 1,
                    });
                    pos += 1;
                }

                // If this was an opening <script> or <style>, scan embedded block until closing </script> or </style>
                if is_open_script_or_style && !is_self_closing && pos < len {
                    let close_tag = if tag_name.eq_ignore_ascii_case("script") {
                        "</script"
                    } else {
                        "</style"
                    };
                    let content_start = pos;
                    let mut found_close = false;
                    while pos < len {
                        if pos + close_tag.len() <= len && code[pos..pos + close_tag.len()].eq_ignore_ascii_case(close_tag) {
                            found_close = true;
                            break;
                        }
                        pos += 1;
                    }
                    if pos > content_start {
                        spans.push(Span {
                            kind: Tok::Plain,
                            start: content_start,
                            end: pos,
                        });
                    }
                    if found_close {
                        // Let main loop handle </script> or </style> tag
                        continue;
                    }
                }
            }
            continue;
        }

        // 6. Character entities: `&name;` or `&#1234;` or `&#x1f;`
        if bytes[pos] == b'&' {
            let start = pos;
            pos += 1;
            if pos < len && bytes[pos] == b'#' {
                pos += 1;
                if pos < len && (bytes[pos] == b'x' || bytes[pos] == b'X') {
                    pos += 1;
                    while pos < len && bytes[pos].is_ascii_hexdigit() {
                        pos += 1;
                    }
                } else {
                    while pos < len && bytes[pos].is_ascii_digit() {
                        pos += 1;
                    }
                }
            } else {
                while pos < len && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                    pos += 1;
                }
            }
            if pos < len && bytes[pos] == b';' {
                pos += 1;
                spans.push(Span {
                    kind: Tok::Keyword,
                    start,
                    end: pos,
                });
                continue;
            } else {
                // Unterminated or plain `&`
                spans.push(Span {
                    kind: Tok::Plain,
                    start,
                    end: pos,
                });
                continue;
            }
        }

        // 7. Text run: all characters up to next `<`, `&`, or EOF
        let text_start = pos;
        while pos < len && bytes[pos] != b'<' && bytes[pos] != b'&' {
            pos += 1;
        }
        if pos > text_start {
            spans.push(Span {
                kind: Tok::Plain,
                start: text_start,
                end: pos,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_tiling(spans: &[Span], total_len: usize) {
        let mut next = 0usize;
        for span in spans {
            assert_eq!(span.start, next, "gap/overlap at {}", span.start);
            assert!(span.end > span.start, "empty span at {}", span.start);
            next = span.end;
        }
        assert_eq!(next, total_len, "spans must tile entire input");
    }

    #[test]
    fn basic_tag_and_attributes() {
        let source = "<div class=\"btn\" id='main' data-ok=true>Click</div>";
        let mut spans = Vec::new();
        lex_html_into(source, &mut spans);
        assert_tiling(&spans, source.len());

        let tag_name = spans.iter().find(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "div");
        assert!(tag_name.is_some(), "tag name must be Keyword");

        let attr_name = spans.iter().find(|s| s.kind == Tok::Type && &source[s.start..s.end] == "class");
        assert!(attr_name.is_some(), "attr name must be Type");

        let attr_val = spans.iter().find(|s| s.kind == Tok::Str && &source[s.start..s.end] == "\"btn\"");
        assert!(attr_val.is_some(), "quoted attr value must be Str");

        let text_span = spans.iter().find(|s| s.kind == Tok::Plain && &source[s.start..s.end] == "Click");
        assert!(text_span.is_some(), "content must be Plain");
    }

    #[test]
    fn comments_cdata_and_doctype() {
        let source = "<!DOCTYPE html><!-- my comment --><![CDATA[ raw text ]]>";
        let mut spans = Vec::new();
        lex_html_into(source, &mut spans);
        assert_tiling(&spans, source.len());

        assert!(spans.iter().any(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "<!DOCTYPE html>"));
        assert!(spans.iter().any(|s| s.kind == Tok::Comment && &source[s.start..s.end] == "<!-- my comment -->"));
        assert!(spans.iter().any(|s| s.kind == Tok::Str && &source[s.start..s.end] == "<![CDATA[ raw text ]]>"));
    }

    #[test]
    fn entities_and_xml_declaration() {
        let source = "<?xml version=\"1.0\"?>&amp;&#38;&#x26;";
        let mut spans = Vec::new();
        lex_html_into(source, &mut spans);
        assert_tiling(&spans, source.len());

        assert!(spans.iter().any(|s| s.kind == Tok::Keyword && &source[s.start..s.end] == "<?xml version=\"1.0\"?>"));
        let entity_count = spans.iter().filter(|s| s.kind == Tok::Keyword && source[s.start..s.end].starts_with('&')).count();
        assert_eq!(entity_count, 3);
    }
}
