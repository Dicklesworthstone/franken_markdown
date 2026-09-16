//! Bind book links to destinations from our own PDF writer, without touching
//! page streams, font subsets, annotation ownership, or cross-reference offsets.
//!
//! This is deliberately not a general PDF editor. Only the writer's complete
//! classic-xref output is accepted. Dictionaries are tokenized, not searched
//! inside arbitrary text or compressed streams. Replacements occupy the space
//! reserved by temporary URI actions; unused bytes become PDF whitespace.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::{RenderError, Result};

fn invalid() -> RenderError {
    RenderError::InvalidInput("book_pdf_navigation: unexpected PDF writer structure".into())
}

/// `targets` maps temporary URI strings to emitted-heading ordinals. `None`
/// removes the action of an unresolved chapter-local link instead of allowing
/// an identically named heading in another chapter to capture it.
pub(super) fn bind(
    mut bytes: Vec<u8>,
    targets: &BTreeMap<String, Option<usize>>,
    heading_count: usize,
    toc: bool,
) -> Result<Vec<u8>> {
    if targets.is_empty() {
        return Ok(bytes);
    }
    let pdf = Pdf::parse(&bytes).ok_or_else(invalid)?;
    let outlines = pdf.outlines().ok_or_else(invalid)?;
    let offset = if outlines.len() == heading_count {
        0
    } else if toc && outlines.len() == heading_count.saturating_add(1)
        && outlines.first().is_some_and(|item| item.contents_heading)
    {
        1
    } else {
        return Err(RenderError::InvalidInput(format!(
            "book_pdf_navigation: expected {heading_count} heading destinations, found {}",
            outlines.len(),
        )));
    };
    let mut patches = Vec::new();
    for id in 1..pdf.objects.len() {
        let dict = pdf.dictionary(id).ok_or_else(invalid)?;
        if !pdf.named(&dict, b"/Type", b"/Annot")
            || !pdf.named(&dict, b"/Subtype", b"/Link")
        {
            continue;
        }
        let Some(action) = pdf.field(&dict, b"/A") else { continue; };
        if !pdf.named(&action.value, b"/S", b"/URI") {
            continue;
        }
        let Some(uri) = pdf.field(&action.value, b"/URI") else { continue; };
        let value = pdf.bytes.get(uri.value.clone()).ok_or_else(invalid)?;
        let Some(value) = value.strip_prefix(b"(").and_then(|s| s.strip_suffix(b")")) else {
            continue;
        };
        // Generated URIs are ASCII with no PDF literal-string escapes.
        let Some(target) = std::str::from_utf8(value).ok().and_then(|s| targets.get(s)) else {
            continue;
        };
        if pdf.field(&dict, b"/Dest").is_some() {
            return Err(invalid());
        }
        let mut replacement = Vec::new();
        if let Some(ordinal) = target {
            let item = outlines.get(ordinal.checked_add(offset).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            replacement.extend_from_slice(b"/Dest ");
            replacement.extend_from_slice(pdf.bytes.get(item.destination.clone()).ok_or_else(invalid)?);
        }
        if replacement.len() > action.range.len() {
            return Err(invalid());
        }
        patches.push((action.range, replacement));
    }
    if patches.is_empty() {
        return Ok(bytes);
    }
    let id_ranges = pdf.identifier_ranges().ok_or_else(invalid)?;
    let trailer_start = pdf.trailer.start;
    for (range, replacement) in patches {
        let output = bytes.get_mut(range).ok_or_else(invalid)?;
        output.fill(b' ');
        output[..replacement.len()].copy_from_slice(&replacement);
    }
    // The writer's ID is a deterministic, non-cryptographic change detector.
    // Refresh it after binding while preserving the two 16-byte hex tokens.
    let first = digest(0xcbf2_9ce4_8422_2325, &bytes[..trailer_start]);
    let second = digest(first ^ 0x9e37_79b9_7f4a_7c15, b"fmd/book/pdf-navigation/v1");
    let identifier = format!("{first:016X}{second:016X}");
    for range in id_ranges {
        bytes.get_mut(range).ok_or_else(invalid)?.copy_from_slice(identifier.as_bytes());
    }
    Ok(bytes)
}

/// Count the actual emitted pages, not a source-layout estimate or a profiling
/// stage whose work count might mean lines rather than pages.
#[cfg(feature = "cli")]
pub(super) fn page_count(bytes: &[u8]) -> Option<u64> {
    let pdf = Pdf::parse(bytes)?;
    let catalog = pdf.dictionary(pdf.reference(pdf.field(&pdf.trailer, b"/Root")?.value)?)?;
    let pages = pdf.dictionary(pdf.reference(pdf.field(&catalog, b"/Pages")?.value)?)?;
    if !pdf.named(&pages, b"/Type", b"/Pages") { return None; }
    let count = pdf.field(&pages, b"/Count")?;
    let mut reader = Reader::new(bytes.get(count.value)?);
    let count = reader.integer()?;
    reader.skip();
    (count > 0 && reader.pos == reader.bytes.len()).then_some(count as u64)
}

fn digest(mut state: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        state ^= u64::from(byte);
        state = state.wrapping_mul(0x0000_0100_0000_01b3);
    }
    state
}

struct Field {
    range: Range<usize>,
    value: Range<usize>,
}

struct Outline {
    destination: Range<usize>,
    contents_heading: bool,
}

struct Pdf<'a> {
    bytes: &'a [u8],
    objects: Vec<Range<usize>>,
    trailer: Range<usize>,
}

impl<'a> Pdf<'a> {
    fn parse(bytes: &'a [u8]) -> Option<Self> {
        if !bytes.starts_with(b"%PDF-") { return None; }
        let marker = b"startxref";
        let start = bytes.windows(marker.len()).rposition(|s| s == marker)?;
        let mut tail = Reader::new(bytes.get(start + marker.len()..)?);
        let xref = tail.integer()?;
        let mut reader = Reader::new(bytes.get(xref..)?);
        if reader.atom()? != b"xref" || reader.integer()? != 0 { return None; }
        let count = reader.integer()?;
        if count == 0 || count > bytes.len() / 20 + 1 { return None; }
        let mut positions = Vec::with_capacity(count.saturating_sub(1));
        for id in 0..count {
            let position = reader.integer()?;
            let generation = reader.integer()?;
            let flag = reader.atom()?;
            if id == 0 {
                if position != 0 || generation != 65535 || flag != b"f" { return None; }
            } else {
                if generation != 0 || flag != b"n" || position < 8 || position >= xref {
                    return None;
                }
                positions.push((position, id));
            }
        }
        if reader.atom()? != b"trailer" { return None; }
        let range = reader.value(0)?;
        let trailer = xref.checked_add(range.start)?..xref.checked_add(range.end)?;
        if trailer.end > start { return None; }
        positions.sort_unstable();
        let mut objects = vec![0..0; count];
        for (index, &(position, id)) in positions.iter().enumerate() {
            let end = positions.get(index + 1).map_or(xref, |&(next, _)| next);
            if position >= end { return None; }
            objects[id] = position..end;
        }
        let pdf = Self { bytes, objects, trailer };
        let size = pdf.field(&pdf.trailer, b"/Size")?;
        if Reader::new(bytes.get(size.value)?).integer()? != count { return None; }
        Some(pdf)
    }

    fn dictionary(&self, id: usize) -> Option<Range<usize>> {
        let range = self.objects.get(id)?;
        let mut reader = Reader::new(self.bytes.get(range.clone())?);
        if reader.integer()? != id || reader.integer()? != 0 || reader.atom()? != b"obj" {
            return None;
        }
        let value = reader.value(0)?;
        if !reader.bytes.get(value.clone())?.starts_with(b"<<") { return None; }
        Some(range.start + value.start..range.start + value.end)
    }

    fn field(&self, dict: &Range<usize>, name: &[u8]) -> Option<Field> {
        let bytes = self.bytes.get(dict.clone())?;
        if !bytes.starts_with(b"<<") || !bytes.ends_with(b">>") { return None; }
        let mut reader = Reader { bytes, pos: 2 };
        loop {
            reader.skip();
            if reader.bytes.get(reader.pos..)?.starts_with(b">>") { return None; }
            let start = reader.pos;
            let key = reader.atom()?;
            if !key.starts_with(b"/") { return None; }
            let value = reader.value(0)?;
            if key == name {
                return Some(Field {
                    range: dict.start + start..dict.start + value.end,
                    value: dict.start + value.start..dict.start + value.end,
                });
            }
        }
    }

    fn named(&self, dict: &Range<usize>, key: &[u8], expected: &[u8]) -> bool {
        self.field(dict, key).and_then(|field| self.bytes.get(field.value)) == Some(expected)
    }

    fn reference(&self, range: Range<usize>) -> Option<usize> {
        let mut reader = Reader::new(self.bytes.get(range)?);
        let id = reader.integer()?;
        if reader.integer()? != 0 || reader.atom()? != b"R" { return None; }
        reader.skip();
        (reader.pos == reader.bytes.len() && id > 0 && id < self.objects.len()).then_some(id)
    }

    fn outlines(&self) -> Option<Vec<Outline>> {
        let catalog = self.dictionary(self.reference(self.field(&self.trailer, b"/Root")?.value)?)?;
        let Some(root) = self.field(&catalog, b"/Outlines") else { return Some(Vec::new()); };
        let root = self.dictionary(self.reference(root.value)?)?;
        if !self.named(&root, b"/Type", b"/Outlines") { return None; }
        let Some(first) = self.field(&root, b"/First") else { return Some(Vec::new()); };
        let mut stack = vec![self.reference(first.value)?];
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) { return None; }
            let dict = self.dictionary(id)?;
            let destination = self.field(&dict, b"/Dest")?.value;
            let array = self.bytes.get(destination.clone())?;
            if !array.starts_with(b"[") || !array.ends_with(b"]") { return None; }
            let mut reader = Reader::new(&array[1..array.len() - 1]);
            let page = reader.integer()?;
            if reader.integer()? != 0 || reader.atom()? != b"R"
                || !self.named(&self.dictionary(page)?, b"/Type", b"/Page")
            { return None; }
            let title = self.field(&dict, b"/Title")?;
            let title = self.bytes.get(title.value)?;
            out.push(Outline {
                destination,
                contents_heading: matches!(title, b"(Contents)" | b"(Table of Contents)"),
            });
            if let Some(next) = self.field(&dict, b"/Next") {
                stack.push(self.reference(next.value)?);
            }
            if let Some(first) = self.field(&dict, b"/First") {
                stack.push(self.reference(first.value)?);
            }
        }
        Some(out)
    }

    fn identifier_ranges(&self) -> Option<Vec<Range<usize>>> {
        let Some(field) = self.field(&self.trailer, b"/ID") else { return Some(Vec::new()); };
        let array = self.bytes.get(field.value.clone())?;
        if !array.starts_with(b"[") || !array.ends_with(b"]") { return None; }
        let mut reader = Reader::new(&array[1..array.len() - 1]);
        let mut ranges = Vec::with_capacity(2);
        for _ in 0..2 {
            let token = reader.value(0)?;
            let value = reader.bytes.get(token.clone())?;
            if value.len() != 34 || value[0] != b'<' || value[33] != b'>'
                || !value[1..33].iter().all(u8::is_ascii_hexdigit)
            { return None; }
            let start = field.value.start + 1 + token.start + 1;
            ranges.push(start..start + 32);
        }
        reader.skip();
        (reader.pos == reader.bytes.len()).then_some(ranges)
    }
}

/// Bounded PDF value scanner. Literal-string parentheses/escapes, comments,
/// hex strings, arrays, dictionaries, and indirect references stay atomic.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

fn whitespace(byte: u8) -> bool { matches!(byte, 0 | 9 | 10 | 12 | 13 | 32) }
fn delimiter(byte: u8) -> bool { whitespace(byte) || b"()<>[]{}/%".contains(&byte) }

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, pos: 0 } }

    fn skip(&mut self) {
        loop {
            while self.bytes.get(self.pos).is_some_and(|b| whitespace(*b)) { self.pos += 1; }
            if self.bytes.get(self.pos) != Some(&b'%') { break; }
            while self.bytes.get(self.pos).is_some_and(|b| !matches!(b, b'\r' | b'\n')) {
                self.pos += 1;
            }
        }
    }

    fn atom(&mut self) -> Option<&'a [u8]> {
        self.skip();
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'/') { self.pos += 1; }
        while self.bytes.get(self.pos).is_some_and(|b| !delimiter(*b)) { self.pos += 1; }
        (self.pos > start).then(|| &self.bytes[start..self.pos])
    }

    fn integer(&mut self) -> Option<usize> {
        let token = self.atom()?;
        if token.is_empty() || !token.iter().all(u8::is_ascii_digit) { return None; }
        std::str::from_utf8(token).ok()?.parse().ok()
    }

    fn value(&mut self, depth: usize) -> Option<Range<usize>> {
        if depth > 64 { return None; }
        self.skip();
        let start = self.pos;
        match *self.bytes.get(self.pos)? {
            b'(' => {
                self.pos += 1;
                let mut nesting = 1usize;
                while nesting != 0 {
                    match *self.bytes.get(self.pos)? {
                        b'\\' => { self.pos += 1; self.bytes.get(self.pos)?; }
                        b'(' => nesting = nesting.checked_add(1)?,
                        b')' => nesting -= 1,
                        _ => {}
                    }
                    self.pos += 1;
                }
            }
            b'[' => {
                self.pos += 1;
                loop {
                    self.skip();
                    if self.bytes.get(self.pos) == Some(&b']') { self.pos += 1; break; }
                    self.value(depth + 1)?;
                }
            }
            b'<' if self.bytes.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                loop {
                    self.skip();
                    if self.bytes.get(self.pos..)?.starts_with(b">>") { self.pos += 2; break; }
                    self.value(depth + 1)?;
                }
            }
            b'<' => {
                self.pos += 1;
                while *self.bytes.get(self.pos)? != b'>' {
                    let byte = self.bytes[self.pos];
                    if !byte.is_ascii_hexdigit() && !whitespace(byte) { return None; }
                    self.pos += 1;
                }
                self.pos += 1;
            }
            _ => {
                let atom = self.atom()?;
                let end = self.pos;
                if atom.iter().all(u8::is_ascii_digit)
                    && self.integer() == Some(0) && self.atom() == Some(b"R")
                {
                    // An indirect reference is one dictionary value.
                } else {
                    self.pos = end;
                }
            }
        }
        Some(start..self.pos)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn fixture(uri: &str) -> Vec<u8> {
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".to_string(),
            format!("<< /Type /Annot /Subtype /Link /Contents (ignore /A << /S /URI >>) /A << /S /URI /URI ({uri}) >> /StructParent 9 >>"),
            "<< /Type /Outlines /First 6 0 R /Last 6 0 R /Count 1 >>".to_string(),
            "<< /Title (Actual \\(heading\\)) /Parent 5 0 R /Dest [3 0 R /XYZ null 712.5 null] >>".to_string(),
            "<< /Length 30 >>\nstream\n/A << /S /URI /URI (decoy) >>\nendstream".to_string(),
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = vec![0];
        for (index, body) in bodies.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
        for offset in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /ID [<00000000000000000000000000000000> <00000000000000000000000000000000>] >>\nstartxref\n{xref}\n%%EOF\n", offsets.len()).as_bytes());
        out
    }

    #[test]
    fn binding_preserves_offsets_streams_and_structure_ownership() {
        let uri = format!("fmd-book-link/{}", "0".repeat(128));
        let before = fixture(&uri);
        let targets = BTreeMap::from([(uri.clone(), Some(0))]);
        let after = bind(before.clone(), &targets, 1, false).unwrap();
        let original = Pdf::parse(&before).unwrap();
        let linked = Pdf::parse(&after).unwrap();
        assert_eq!(before.len(), after.len());
        assert_eq!(original.objects, linked.objects);
        assert_eq!(&before[original.objects[7].clone()], &after[linked.objects[7].clone()]);
        let annot = linked.dictionary(4).unwrap();
        assert!(linked.field(&annot, b"/A").is_none());
        let dest = linked.field(&annot, b"/Dest").unwrap();
        assert_eq!(&after[dest.value], b"[3 0 R /XYZ null 712.5 null]");
        let owner = linked.field(&annot, b"/StructParent").unwrap();
        assert_eq!(&after[owner.value], b"9");
        assert_eq!(after, bind(before, &targets, 1, false).unwrap());
        assert_eq!(after, bind(after.clone(), &targets, 1, false).unwrap());
    }

    #[test]
    fn unresolved_links_are_inert_not_cross_chapter_fallbacks() {
        let uri = format!("fmd-book-link/{}", "x".repeat(128));
        let bytes = bind(fixture(&uri), &BTreeMap::from([(uri, None)]), 1, false).unwrap();
        let pdf = Pdf::parse(&bytes).unwrap();
        let annot = pdf.dictionary(4).unwrap();
        assert!(pdf.field(&annot, b"/A").is_none());
        assert!(pdf.field(&annot, b"/Dest").is_none());
    }

    #[test]
    fn external_links_and_decoy_keys_are_not_rewritten() {
        let before = fixture("https://example.org/manual");
        assert_eq!(before, bind(before.clone(), &BTreeMap::from([("decoy".into(), None)]), 1, false).unwrap());
    }

    #[test]
    fn malformed_xrefs_and_destination_count_drift_fail_closed() {
        let uri = "placeholder".repeat(16);
        let mut bytes = fixture(&uri);
        let targets = BTreeMap::from([(uri, Some(0))]);
        assert!(bind(bytes.clone(), &targets, 2, false).is_err());
        let start = bytes.windows(4).position(|s| s == b"xref").unwrap();
        bytes[start] = b'!';
        assert!(bind(bytes, &targets, 1, false).is_err());
    }

    #[cfg(feature = "cli")]
    #[test]
    fn page_count_comes_from_the_catalogs_page_tree() {
        assert_eq!(page_count(&fixture("https://example.org")), Some(1));
        assert_eq!(page_count(b"not a PDF /Type /Pages /Count 999"), None);
    }

    #[test]
    fn value_scanner_bounds_nesting_and_preserves_compound_values() {
        let source = b"<< /A (nested (literal) \\) /fake) /Ref 12 0 R /Arr [<FEFF> null] >>";
        assert_eq!(Reader::new(source).value(0), Some(0..source.len()));
        let too_deep = format!("{}0{}", "[".repeat(66), "]".repeat(66));
        assert!(Reader::new(too_deep.as_bytes()).value(0).is_none());
        for bad in [b"(unterminated".as_slice(), b"<zz>", b"[0", b"<< /A"] {
            assert!(Reader::new(bad).value(0).is_none());
        }
    }
}
