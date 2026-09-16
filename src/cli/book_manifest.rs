//! The supported `book.toml` schema: metadata strings and an ordered path list.
//! Parsing is quote-aware, so commas, brackets and `#` inside filenames are
//! data, not array delimiters or comments. Unsupported keys fail explicitly.

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Manifest {
    pub title: Option<String>,
    pub author: Option<String>,
    pub lang: Option<String>,
    pub order: Vec<String>,
}

pub(super) fn parse(source: &str) -> Result<Manifest, String> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut parser = Parser { source, at: 0 };
    let mut result = Manifest::default();
    let mut keys = std::collections::BTreeSet::new();
    parser.space();
    while parser.at < source.len() {
        let start = parser.at;
        while parser.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            parser.bump();
        }
        if start == parser.at {
            return Err(parser.error("expected a metadata or order key"));
        }
        let key = &source[start..parser.at];
        let canonical = if key == "chapters" { "order" } else { key };
        if !matches!(canonical, "title" | "author" | "lang" | "order") {
            return Err(parser.error(&format!("unsupported key {key:?}; use title, author, lang, order")));
        }
        if !keys.insert(canonical) {
            return Err(parser.error(&format!("duplicate {canonical} key")));
        }
        parser.horizontal();
        parser.expect('=')?;
        parser.horizontal();
        match canonical {
            "title" => result.title = Some(parser.string()?),
            "author" => result.author = Some(parser.string()?),
            "lang" => result.lang = Some(parser.string()?),
            _ => result.order = parser.array()?,
        }
        parser.horizontal();
        if parser.peek() == Some('#') {
            parser.comment();
        }
        if parser.at < source.len() && parser.peek() != Some('\n') {
            return Err(parser.error("unexpected content after value"));
        }
        parser.space();
    }
    Ok(result)
}

struct Parser<'a> {
    source: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        let line = self.source[..self.at].bytes().filter(|&b| b == b'\n').count() + 1;
        format!("book.toml:{line}: {message}")
    }

    fn peek(&self) -> Option<char> {
        self.source[self.at..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.at += ch.len_utf8();
        Some(ch)
    }

    fn horizontal(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
            self.bump();
        }
    }

    fn comment(&mut self) {
        while !matches!(self.peek(), None | Some('\n')) {
            self.bump();
        }
    }

    fn space(&mut self) {
        loop {
            while matches!(self.peek(), Some(' ' | '\t' | '\r' | '\n')) {
                self.bump();
            }
            if self.peek() != Some('#') {
                break;
            }
            self.comment();
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.bump() == Some(expected) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {expected:?}")))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let quote = self.bump().filter(|c| matches!(c, '\'' | '"'))
            .ok_or_else(|| self.error("expected a quoted string"))?;
        let mut value = String::new();
        loop {
            let ch = self.bump().ok_or_else(|| self.error("unterminated string"))?;
            if ch == quote {
                return Ok(value);
            }
            if matches!(ch, '\n' | '\r') {
                return Err(self.error("multiline strings are not supported"));
            }
            let ch = if ch == '\\' && quote == '"' {
                match self.bump().ok_or_else(|| self.error("unterminated escape"))? {
                    '"' => '"',
                    '\\' => '\\',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => self.unicode(4)?,
                    'U' => self.unicode(8)?,
                    _ => return Err(self.error("unsupported string escape")),
                }
            } else {
                ch
            };
            // Metadata is serialized to XML as well as HTML. Do not admit
            // XML-illegal controls through a TOML Unicode escape.
            if (ch.is_control() && !matches!(ch, '\t' | '\n' | '\r'))
                || matches!(ch, '\u{fffe}' | '\u{ffff}')
            {
                return Err(self.error("unsupported control character in string"));
            }
            value.push(ch);
        }
    }

    fn unicode(&mut self, count: usize) -> Result<char, String> {
        let mut value = 0u32;
        for _ in 0..count {
            let digit = self.bump().and_then(|c| c.to_digit(16))
                .ok_or_else(|| self.error("invalid Unicode escape"))?;
            value = (value << 4) | digit;
        }
        char::from_u32(value).ok_or_else(|| self.error("invalid Unicode scalar"))
    }

    fn array(&mut self) -> Result<Vec<String>, String> {
        self.expect('[')?;
        let mut values = Vec::new();
        self.space();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(values);
        }
        loop {
            if values.len() == 4096 {
                return Err(self.error("more than 4096 chapter entries"));
            }
            values.push(self.string()?);
            self.space();
            match self.bump() {
                Some(']') => return Ok(values),
                Some(',') => {
                    self.space();
                    if self.peek() == Some(']') {
                        self.bump();
                        return Ok(values);
                    }
                }
                _ => return Err(self.error("expected ',' or ']' in chapter array")),
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn accepts_multiline_order_comments_and_metadata() {
        let manifest = parse("title = \"A Book\" # comment\nauthor = 'Ada'\nlang = 'fr'\norder = [\n 'z.md', # first\n \"a.md\",\n]\n").unwrap();
        assert_eq!(manifest.title.as_deref(), Some("A Book"));
        assert_eq!(manifest.author.as_deref(), Some("Ada"));
        assert_eq!(manifest.lang.as_deref(), Some("fr"));
        assert_eq!(manifest.order, ["z.md", "a.md"]);
    }

    #[test]
    fn punctuation_inside_strings_is_not_syntax() {
        let manifest = parse("chapters = ['a,b.md', 'x]y.md', 'a#b.md', \"say\\\"hi.md\"]\n").unwrap();
        assert_eq!(manifest.order, ["a,b.md", "x]y.md", "a#b.md", "say\"hi.md"]);
    }

    #[test]
    fn unicode_escapes_and_literal_backslashes_are_supported() {
        assert_eq!(parse("title = \"\\u4e2d\\U0001f680\"\n").unwrap().title.as_deref(), Some("中🚀"));
        assert_eq!(parse("order = ['guide\\next.md']\n").unwrap().order, ["guide\\next.md"]);
    }

    #[test]
    fn refuses_ambiguous_or_silently_ignored_manifests() {
        for source in [
            "title = unquoted", "title = 'a' trailing", "order = ['a' 'b']",
            "order = ['a'", "order = [1]", "title = 'a'\ntitle = 'b'",
            "order = []\nchapters = []", "typo = 'x'", "[book]\ntitle = 'x'",
            "title = \"\\uD800\"", "title = \"\\u0000\"", "title = \"\\U00110000\"",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.starts_with("book.toml:"), "{source}: {error}");
        }
    }
}
