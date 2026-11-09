//! Small independent reader for this writer's PDF content operators.
//! Marked content can occur inside a continuing BT/ET; source newlines do not
//! delimit text state, glyph shows, or BDC/EMC scope.
#![allow(dead_code)]

pub struct Operation<'a> {
    pub operator: &'a str,
    pub operands: Vec<&'a str>,
}

pub fn tokens(content: &str) -> Vec<&str> {
    let bytes = content.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at] == b'%' {
            while at < bytes.len() && !matches!(bytes[at], b'\r' | b'\n') {
                at += 1;
            }
            continue;
        }
        let start = at;
        match bytes[at] {
            b'(' => {
                at += 1;
                let mut depth = 1;
                while at < bytes.len() && depth != 0 {
                    match bytes[at] {
                        b'\\' => {
                            at += 2;
                            continue;
                        }
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    at += 1;
                }
                assert_eq!(depth, 0, "complete PDF literal string");
                assert!(at <= bytes.len());
            }
            b'<' if bytes.get(at + 1) != Some(&b'<') => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'>' {
                    at += 1;
                }
                assert!(at < bytes.len(), "complete PDF hex string");
                at += 1;
            }
            b'<' | b'>' => {
                assert_eq!(bytes.get(at + 1), Some(&bytes[at]));
                at += 2;
            }
            b'[' | b']' => at += 1,
            _ => {
                at += 1;
                while at < bytes.len()
                    && !bytes[at].is_ascii_whitespace()
                    && !b"()<>[]/%".contains(&bytes[at])
                {
                    at += 1;
                }
            }
        }
        result.push(&content[start..at]);
    }
    result
}

pub fn operations(content: &str) -> Vec<Operation<'_>> {
    let mut result = Vec::new();
    let mut operands = Vec::new();
    let mut containers = Vec::new();
    for token in tokens(content) {
        match token {
            "<<" | "[" => containers.push(token),
            ">>" => assert_eq!(containers.pop(), Some("<<")),
            "]" => assert_eq!(containers.pop(), Some("[")),
            _ => {}
        }
        if containers.is_empty()
            && (token.as_bytes()[0].is_ascii_alphabetic() || matches!(token, "'" | "\""))
        {
            result.push(Operation {
                operator: token,
                operands: std::mem::take(&mut operands),
            });
        } else {
            operands.push(token);
        }
    }
    assert!(containers.is_empty(), "complete PDF operand containers");
    assert!(operands.is_empty(), "every operand belongs to an operator");
    result
}

/// Decode only actual text-show operands. ActualText and other marked-content
/// properties are not glyphs. Reject unsupported strings rather than silently
/// dropping text from a geometry or content oracle.
pub fn glyph_ids(operation: &Operation<'_>) -> Vec<u16> {
    let values = match operation.operator {
        "TJ" => {
            assert_eq!(operation.operands.first(), Some(&"["));
            assert_eq!(operation.operands.last(), Some(&"]"));
            &operation.operands[1..operation.operands.len() - 1]
        }
        "Tj" => {
            assert_eq!(operation.operands.len(), 1);
            operation.operands.as_slice()
        }
        _ => panic!("glyph decoding requires TJ/Tj"),
    };
    let mut result = Vec::new();
    for value in values {
        if let Some(hex) = value
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
        {
            let (cids, remainder) = hex.as_bytes().as_chunks::<4>();
            assert!(remainder.is_empty(), "every writer CID has four hex digits");
            for cid in cids {
                result.push(u16::from_str_radix(std::str::from_utf8(cid).unwrap(), 16).unwrap());
            }
        } else {
            assert_eq!(operation.operator, "TJ");
            assert!(
                value
                    .parse::<f64>()
                    .expect("numeric TJ adjustment")
                    .is_finite()
            );
        }
    }
    result
}

/// Exact paint sequence, including matrices, font selections, colors, paths
/// and nonzero pair adjustments. Only marked-content wrappers and the
/// partition of a continuing glyph show into TJ/Tj pieces are normalized.
pub fn normalized_paint(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut marks = 0usize;
    let mut in_text = false;
    for operation in operations(content) {
        match operation.operator {
            "BDC" | "BMC" => marks += 1,
            "EMC" => marks = marks.checked_sub(1).expect("balanced marked content"),
            "TJ" | "Tj" => {
                assert!(in_text);
                // Validate the complete text-show grammar even when this
                // particular operand contributes no painted CID.
                let _ = glyph_ids(&operation);
                for value in &operation.operands {
                    match *value {
                        "[" | "]" => {}
                        hex if hex.starts_with('<') => {
                            for cid in hex.as_bytes()[1..hex.len() - 1].as_chunks::<4>().0 {
                                result.push(format!("glyph:{}", std::str::from_utf8(cid).unwrap()));
                            }
                        }
                        adjustment => {
                            let adjustment = adjustment.parse::<f64>().unwrap();
                            if adjustment != 0.0 {
                                result.push(format!("kern:{adjustment}"));
                            }
                        }
                    }
                }
            }
            operator => {
                if operator == "BT" {
                    assert!(!in_text);
                    in_text = true;
                } else if operator == "ET" {
                    assert!(in_text);
                    in_text = false;
                }
                result.push(format!("{operator} {}", operation.operands.join(" ")));
            }
        }
    }
    assert_eq!(marks, 0);
    assert!(!in_text);
    result
}

struct Object {
    dictionary: String,
    stream: Option<Vec<u8>>,
}

fn number_after(source: &str, key: &str) -> usize {
    source
        .split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn objects(pdf: &[u8]) -> std::collections::BTreeMap<usize, Object> {
    let marker = b"startxref\n";
    let start = pdf
        .windows(marker.len())
        .rposition(|part| part == marker)
        .unwrap();
    let xref = std::str::from_utf8(&pdf[start + marker.len()..])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let section = lines.next().unwrap().split_whitespace().collect::<Vec<_>>();
    assert_eq!(section[0], "0");
    let offsets = lines
        .take(section[1].parse().unwrap())
        .map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields[2] == "n" {
                fields[0].parse::<usize>().unwrap()
            } else {
                0
            }
        })
        .collect::<Vec<_>>();
    offsets
        .iter()
        .enumerate()
        .filter(|(_, offset)| **offset != 0)
        .map(|(id, &offset)| {
            let end = offsets
                .iter()
                .copied()
                .filter(|&value| value > offset)
                .min()
                .unwrap_or(xref);
            let header = format!("{id} 0 obj\n");
            assert!(pdf[offset..end].starts_with(header.as_bytes()));
            let body = &pdf[offset + header.len()..end];
            let dictionary_end = body.iter().position(|&byte| byte == b'\n').unwrap();
            let dictionary = std::str::from_utf8(&body[..dictionary_end])
                .unwrap()
                .to_owned();
            let stream = body
                .windows(8)
                .position(|part| part == b"\nstream\n")
                .map(|start| {
                    let length = number_after(&dictionary, "/Length ");
                    let bytes = &body[start + 8..start + 8 + length];
                    if dictionary.contains("/FlateDecode") {
                        franken_markdown::zlib_decompress(bytes, 1 << 26).unwrap()
                    } else {
                        bytes.to_vec()
                    }
                });
            (id, Object { dictionary, stream })
        })
        .collect()
}

fn array_after(source: &str, key: &str) -> Vec<String> {
    let source = source.split_once(key).unwrap().1;
    let mut depth = 1;
    let mut result = Vec::new();
    for token in tokens(source) {
        if token == "[" {
            depth += 1;
        }
        if token == "]" {
            depth -= 1;
        }
        if depth == 0 {
            return result;
        }
        result.push(token.to_owned());
    }
    panic!("complete array {key}");
}

fn resources(source: &str, key: &str) -> Vec<(String, usize)> {
    let Some((_, source)) = source.split_once(key) else {
        return Vec::new();
    };
    let words = source
        .split_once(">>")
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>();
    let (entries, remainder) = words.as_chunks::<4>();
    assert!(remainder.is_empty());
    entries
        .iter()
        .map(|entry| {
            assert_eq!(&entry[2..], ["0", "R"]);
            (entry[0].to_owned(), entry[1].parse().unwrap())
        })
        .collect()
}

#[derive(Debug, PartialEq)]
struct PaintedPage {
    geometry: Vec<String>,
    operators: Vec<String>,
    assets: Vec<(String, Vec<u8>)>,
}

fn painted_pages(pdf: &[u8]) -> Vec<PaintedPage> {
    let objects = objects(pdf);
    let pages = objects
        .values()
        .find(|object| object.dictionary.contains("/Type /Pages "))
        .unwrap();
    let kids = array_after(&pages.dictionary, "/Kids [");
    let (kids, remainder) = kids.as_chunks::<3>();
    assert!(remainder.is_empty());
    kids.iter()
        .map(|kid| {
            let page = &objects[&kid[0].parse().unwrap()];
            let stream = objects[&number_after(&page.dictionary, "/Contents ")]
                .stream
                .as_ref()
                .unwrap();
            let mut assets = Vec::new();
            for (name, id) in resources(&page.dictionary, "/Font <<") {
                let font = &objects[&id].dictionary;
                let cmap = &objects[&number_after(font, "/ToUnicode ")];
                assets.push((format!("{name}:unicode"), cmap.stream.clone().unwrap()));
                let descendant = &objects[&number_after(font, "/DescendantFonts [")].dictionary;
                assets.push((
                    format!("{name}:widths"),
                    array_after(descendant, "/W [").join(" ").into_bytes(),
                ));
                let descriptor = &objects[&number_after(descendant, "/FontDescriptor ")].dictionary;
                let key = if descriptor.contains("/FontFile2 ") {
                    "/FontFile2 "
                } else {
                    "/FontFile3 "
                };
                let program = &objects[&number_after(descriptor, key)];
                assets.push((format!("{name}:program"), program.stream.clone().unwrap()));
            }
            for (name, id) in resources(&page.dictionary, "/XObject <<") {
                let object = &objects[&id];
                assets.push((
                    format!("{name}:object"),
                    object.dictionary.as_bytes().to_vec(),
                ));
                assets.push((format!("{name}:stream"), object.stream.clone().unwrap()));
            }
            PaintedPage {
                geometry: array_after(&page.dictionary, "/MediaBox ["),
                operators: normalized_paint(std::str::from_utf8(stream).unwrap()),
                assets,
            }
        })
        .collect()
}

pub fn assert_same_paint(actual: &[u8], expected: &[u8]) {
    assert_eq!(
        painted_pages(actual),
        painted_pages(expected),
        "PDF pages, glyph paint, paths, matrices, font programs and resource bytes must match"
    );
}

/// Semantic structure changes object numbers, but not the document's archival
/// declarations, metadata text, output-intent attributes, or ICC profile.
pub fn assert_same_archival_metadata(actual: &[u8], expected: &[u8], part: u8, conformance: &str) {
    fn archival(pdf: &[u8], part: u8, conformance: &str) -> Vec<Vec<u8>> {
        let objects = objects(pdf);
        let catalog = &objects
            .values()
            .find(|object| object.dictionary.contains("/Type /Catalog "))
            .expect("PDF/A catalog")
            .dictionary;
        let metadata = &objects[&number_after(catalog, "/Metadata ")];
        assert!(metadata.dictionary.contains("/Type /Metadata "));
        assert!(metadata.dictionary.contains("/Subtype /XML"));
        let xml = metadata.stream.as_ref().expect("XMP metadata stream");
        let text = std::str::from_utf8(xml).unwrap();
        assert!(text.contains(&format!("<pdfaid:part>{part}</pdfaid:part>")));
        assert!(text.contains(&format!(
            "<pdfaid:conformance>{conformance}</pdfaid:conformance>"
        )));
        let mut result = vec![metadata.dictionary.as_bytes().to_vec(), xml.clone()];
        let intents = array_after(catalog, "/OutputIntents [");
        let (intents, remainder) = intents.as_chunks::<3>();
        assert!(remainder.is_empty());
        assert!(!intents.is_empty(), "PDF/A requires its output intent");
        for intent in intents {
            assert_eq!(&intent[1..], ["0", "R"]);
            let dictionary = &objects[&intent[0].parse().unwrap()].dictionary;
            assert!(dictionary.contains("/Type /OutputIntent "));
            let profile_id = number_after(dictionary, "/DestOutputProfile ");
            let reference = format!("/DestOutputProfile {profile_id} 0 R");
            assert_eq!(dictionary.matches(&reference).count(), 1);
            result.push(
                dictionary
                    .replace(&reference, "/DestOutputProfile <ICC>")
                    .into_bytes(),
            );
            let profile = &objects[&profile_id];
            let bytes = profile
                .stream
                .as_ref()
                .expect("referenced ICC profile stream");
            assert!(bytes.len() >= 128, "complete ICC profile header");
            assert_eq!(&bytes[36..40], b"acsp");
            result.push(profile.dictionary.as_bytes().to_vec());
            result.push(bytes.clone());
        }
        let marker = b"trailer\n";
        let start = pdf
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap();
        let trailer = std::str::from_utf8(&pdf[start + marker.len()..]).unwrap();
        result.push(
            objects[&number_after(trailer, "/Info ")]
                .dictionary
                .as_bytes()
                .to_vec(),
        );
        result
    }
    assert_eq!(
        archival(actual, part, conformance),
        archival(expected, part, conformance),
        "XMP, PDF/A declarations, output intent, ICC profile and document info must remain exact"
    );
}
