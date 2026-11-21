use franken_markdown::{
    DocumentSourceMap, SourceMapError, SourceSpan, TextSelectionRange, parse_markdown_spanned,
};

fn source_map(source: &str) -> Result<DocumentSourceMap, SourceMapError> {
    DocumentSourceMap::from_spanned_document(&parse_markdown_spanned(source), source)
}

#[test]
fn copy_operations_reject_partial_unicode_characters() -> Result<(), SourceMapError> {
    let source = "A café 😀 fin\n";
    let map = source_map(source)?;
    let rendered = map.rendered_text();

    for offset in 0..=rendered.len() {
        if rendered.is_char_boundary(offset) {
            continue;
        }
        for range in [
            TextSelectionRange::new(0, offset),
            TextSelectionRange::new(offset, rendered.len()),
            TextSelectionRange::new(offset, offset),
        ] {
            let expected = SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: rendered.len(),
            };
            assert_eq!(map.copy_rendered_text(range), Err(expected.clone()));
            assert_eq!(
                map.copy_enclosing_source(range, source),
                Err(expected.clone())
            );
            assert_eq!(
                map.copy_enclosing_block(range, source),
                Err(expected.clone())
            );
            assert_eq!(map.copy_exact_source_ranges(range), Err(expected.clone()));
            assert_eq!(
                map.copy_exact_source_slices(range, source),
                Err(expected.clone())
            );
            assert_eq!(
                map.reject_invented_contiguous(range, SourceSpan::new(0, source.len())),
                Err(expected)
            );
        }
    }
    Ok(())
}

#[test]
fn rendered_copy_accepts_every_complete_unicode_slice() -> Result<(), SourceMapError> {
    let source = "café 😀\n";
    let map = source_map(source)?;
    let rendered = map.rendered_text();
    let boundaries: Vec<_> = (0..=rendered.len())
        .filter(|&offset| rendered.is_char_boundary(offset))
        .collect();

    for &start in &boundaries {
        for &end in boundaries.iter().filter(|&&end| end >= start) {
            assert_eq!(
                map.copy_rendered_text(TextSelectionRange::new(start, end))?,
                rendered[start..end]
            );
        }
    }
    for range in [
        TextSelectionRange::new(1, 0),
        TextSelectionRange::new(0, rendered.len() + 1),
        TextSelectionRange::new(usize::MAX, usize::MAX),
    ] {
        assert_eq!(
            map.copy_rendered_text(range),
            Err(SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: rendered.len(),
            })
        );
    }
    Ok(())
}

#[test]
fn caret_copy_returns_the_enclosing_markdown_block() -> Result<(), SourceMapError> {
    let source = "Before\n\nA *styled* paragraph.\n\nAfter\n";
    let document = parse_markdown_spanned(source);
    let map = DocumentSourceMap::from_spanned_document(&document, source)?;
    let expected =
        document.blocks[1]
            .span
            .slice(source)
            .ok_or(SourceMapError::SourceOutOfBounds {
                offset: document.blocks[1].span.end,
                source_len: source.len(),
            })?;
    let offset =
        map.rendered_text()
            .find("styled")
            .ok_or(SourceMapError::InvalidSelectionRange {
                start: 0,
                end: 0,
                max: map.rendered_len(),
            })?;
    for position in offset..offset + "styled".len() {
        let caret = TextSelectionRange::new(position, position);
        assert_eq!(map.copy_enclosing_block(caret, source)?, expected);
        assert_eq!(map.copy_enclosing_source(caret, source)?, expected);
        assert_eq!(map.copy_exact_source_ranges(caret)?, Vec::new());
        assert_eq!(
            map.copy_exact_source_slices(caret, source)?,
            Vec::<&str>::new()
        );
        assert_eq!(map.copy_rendered_text(caret)?, "");
    }
    Ok(())
}
