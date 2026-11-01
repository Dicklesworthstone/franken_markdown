//! Complete single-line tags for CommonMark's non-interrupting HTML blocks.
//!
//! This is source grammar, not HTML sanitization. Type 7 requires one complete
//! open or closing tag followed only by spaces/tabs; the block collector then
//! owns every line through the next blank line. Scan bytes once without joining
//! lines, allocating tag/attribute strings, or searching an accumulated prefix.

/// `line` starts immediately after the block's (at most three) leading spaces.
pub(super) fn complete_tag_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'<') {
        return false;
    }
    let closing = bytes.get(1) == Some(&b'/');
    let mut pos = if closing { 2 } else { 1 };
    let name_start = pos;
    if !bytes.get(pos).is_some_and(u8::is_ascii_alphabetic) {
        return false;
    }
    pos += 1;
    while bytes
        .get(pos)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        pos += 1;
    }

    // These opening tags belong only to type 1, whose start and end conditions
    // differ. Complete closing tags with the same names still qualify here.
    if !closing
        && [b"pre".as_slice(), b"script", b"style", b"textarea"]
            .iter()
            .any(|name| bytes[name_start..pos].eq_ignore_ascii_case(name))
    {
        return false;
    }
    if closing {
        skip_space_tab(bytes, &mut pos);
        return tag_end(bytes, pos);
    }

    loop {
        let before_space = pos;
        skip_space_tab(bytes, &mut pos);
        if bytes.get(pos) == Some(&b'>') {
            return tag_end(bytes, pos);
        }
        if bytes.get(pos) == Some(&b'/') {
            return tag_end(bytes, pos + 1);
        }
        // Each attribute needs separating whitespace and an ASCII name.
        if pos == before_space
            || !bytes
                .get(pos)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':'))
        {
            return false;
        }
        pos += 1;
        while bytes.get(pos).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-')
        }) {
            pos += 1;
        }

        let name_end = pos;
        skip_space_tab(bytes, &mut pos);
        if bytes.get(pos) != Some(&b'=') {
            // A boolean attribute leaves its trailing whitespace for the next
            // attribute. Each byte is visited at most twice, even on long tags.
            pos = name_end;
            continue;
        }
        pos += 1;
        skip_space_tab(bytes, &mut pos);
        match bytes.get(pos).copied() {
            Some(quote @ (b'\'' | b'"')) => {
                pos += 1;
                while bytes
                    .get(pos)
                    .is_some_and(|byte| *byte != quote && !matches!(byte, b'\r' | b'\n'))
                {
                    pos += 1;
                }
                if bytes.get(pos) != Some(&quote) {
                    return false;
                }
                pos += 1;
            }
            Some(_) => {
                let value_start = pos;
                while bytes.get(pos).is_some_and(|byte| {
                    !matches!(
                        byte,
                        b' ' | b'\t' | b'\r' | b'\n' | b'"' | b'\'' | b'=' | b'<' | b'>' | b'`'
                    )
                }) {
                    pos += 1;
                }
                if pos == value_start {
                    return false;
                }
            }
            None => return false,
        }
    }
}

fn skip_space_tab(bytes: &[u8], pos: &mut usize) {
    while matches!(bytes.get(*pos), Some(b' ' | b'\t')) {
        *pos += 1;
    }
}

fn tag_end(bytes: &[u8], pos: usize) -> bool {
    bytes.get(pos) == Some(&b'>')
        && bytes[pos + 1..]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
}

#[cfg(test)]
mod tests {
    use super::complete_tag_line;

    #[test]
    fn complete_open_and_closing_tags_follow_the_raw_html_grammar() {
        for line in [
            "<span>",
            "<WIDGET-2/>",
            "<widget disabled _x :ns.x-y=ok>",
            "<widget data-x = 'café > < β' title=\"quoted\"/>",
            "<widget a=b/c d=é/>",
            "<widget a=''>\t ",
            "<widget\ta\t=\tvalue\tb/>",
            "</custom-tag\t >  ",
            "</PRE>",
            "</Script >",
            "</style>",
            "</textarea>",
        ] {
            assert!(complete_tag_line(line), "{line:?}");
        }
        for line in [
            "<>",
            "<2tag>",
            "<tag.name>",
            "<tag❤>",
            "<tag a='unterminated>",
            "<tag a='x'b='y'>",
            "<tag a= >",
            "<tag a=`x`>",
            "<tag a=x=y>",
            "<tag a=<x>>",
            "<tag a='x' / >",
            "<tag a=x\nb=y>",
            "<tag a='x\ny'>",
            "<tag\u{a0}a=x>",
            "<tag\u{c}a=x>",
            "<tag a> text",
            "<tag><other>",
            "<tag>\u{a0}",
            "</tag a=x>",
            "</tag/>",
            "<!>",
            "<pre/>",
            "<SCRIPT>",
            "<style a=x>",
            "<textarea>",
        ] {
            assert!(!complete_tag_line(line), "{line:?}");
        }
    }
}
