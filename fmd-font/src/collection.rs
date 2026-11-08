//! Bounded TTC/OTC face extraction into ordinary standalone sfnt bytes.
//!
//! Collection table offsets are absolute from the start of the collection,
//! including tables shared by several faces. Extraction preserves table
//! contents, rebuilds the selected face's directory and checksums, and omits
//! signatures invalidated by that rewrite. Outline admission remains the
//! responsibility of [`crate::Font`] and the caller's renderer.
//!
//! OpenType collection format and checksum rules:
//! https://learn.microsoft.com/en-us/typography/opentype/spec/otff

use crate::{be_u16, be_u32, table_checksum, write_u32};
use std::ops::Range;

/// Maximum collection input and standalone extracted face size: 32 MiB.
/// This matches the renderer's per-slot host-font byte budget.
pub const MAX_COLLECTION_BYTES: usize = 32 * 1024 * 1024;
/// Maximum number of face-directory offsets admitted in a collection.
pub const MAX_COLLECTION_FACES: u32 = 1024;
/// Maximum tables per face. Larger directories cannot represent their
/// derived sfnt binary-search fields without overflowing `u16`.
pub const MAX_COLLECTION_TABLES: usize = 4095;

/// Stable reasons for refusing a collection or selected face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionErrorKind {
    /// Input does not have the `ttcf` collection signature.
    NotCollection,
    /// The collection version is neither 1.0 nor 2.0.
    UnsupportedVersion,
    /// The selected face is not TrueType or OpenType sfnt data.
    UnsupportedFaceFormat,
    /// The zero-based face index is outside the collection.
    InvalidFaceIndex,
    /// A header, directory, signature range, or table is malformed/truncated.
    Malformed,
    /// The selected face contains more than one record for the same table tag.
    DuplicateTable,
    /// A required extraction table is absent, such as `head`.
    MissingTable,
    /// Input size, extracted size, face count, or table count exceeds a limit.
    LimitExceeded,
}

/// Bounded error context, independent of host paths or font naming strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionError {
    /// Stable error category.
    pub kind: CollectionErrorKind,
    /// Selected face, if the failure concerns an individual face.
    pub face_index: Option<u32>,
    /// Selected table, if applicable.
    pub table: Option<[u8; 4]>,
    /// Byte offset within the source collection, when available.
    pub offset: Option<usize>,
}

impl CollectionError {
    const fn new(
        kind: CollectionErrorKind,
        face_index: Option<u32>,
        table: Option<[u8; 4]>,
        offset: Option<usize>,
    ) -> Self {
        Self {
            kind,
            face_index,
            table,
            offset,
        }
    }
}

impl core::fmt::Display for CollectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "font collection {:?}, face {:?}, table {:?}, offset {:?}",
            self.kind, self.face_index, self.table, self.offset
        )
    }
}

impl std::error::Error for CollectionError {}

fn header_error(kind: CollectionErrorKind, offset: usize) -> CollectionError {
    CollectionError::new(kind, None, None, Some(offset))
}

#[derive(Debug, Clone, Copy)]
struct Header {
    faces: u32,
    end: usize,
}

impl Header {
    fn parse(data: &[u8]) -> Result<Self, CollectionError> {
        use CollectionErrorKind as Kind;
        if data.len() > MAX_COLLECTION_BYTES {
            return Err(header_error(Kind::LimitExceeded, 0));
        }
        if data.get(..4) != Some(b"ttcf") {
            return Err(header_error(Kind::NotCollection, 0));
        }
        let version = be_u32(data, 4).ok_or(header_error(Kind::Malformed, 4))?;
        if !matches!(version, 0x0001_0000 | 0x0002_0000) {
            return Err(header_error(Kind::UnsupportedVersion, 4));
        }
        let faces = be_u32(data, 8).ok_or(header_error(Kind::Malformed, 8))?;
        if faces == 0 {
            return Err(header_error(Kind::Malformed, 8));
        }
        if faces > MAX_COLLECTION_FACES {
            return Err(header_error(Kind::LimitExceeded, 8));
        }
        let offsets_end = 12 + faces as usize * 4;
        let end = offsets_end + if version == 0x0002_0000 { 12 } else { 0 };
        if data.get(..end).is_none() {
            return Err(header_error(Kind::Malformed, 12));
        }
        if version == 0x0002_0000 {
            let tag =
                be_u32(data, offsets_end).ok_or(header_error(Kind::Malformed, offsets_end))?;
            let length = be_u32(data, offsets_end + 4)
                .ok_or(header_error(Kind::Malformed, offsets_end + 4))?
                as usize;
            let offset = be_u32(data, offsets_end + 8)
                .ok_or(header_error(Kind::Malformed, offsets_end + 8))?
                as usize;
            if tag == 0 {
                if length != 0 || offset != 0 {
                    return Err(header_error(Kind::Malformed, offsets_end));
                }
            } else if tag != u32::from_be_bytes(*b"DSIG")
                || length == 0
                || offset < end
                || offset
                    .checked_add(length)
                    .is_none_or(|limit| limit > data.len())
            {
                return Err(header_error(Kind::Malformed, offsets_end));
            }
        }
        let header = Self { faces, end };
        // Validate every face-directory pointer, without requiring unselected
        // faces to use outline or table formats understood by this consumer.
        for index in 0..faces {
            header.face_offset(data, index)?;
        }
        Ok(header)
    }

    fn face_offset(self, data: &[u8], index: u32) -> Result<usize, CollectionError> {
        use CollectionErrorKind as Kind;
        if index >= self.faces {
            return Err(CollectionError::new(
                Kind::InvalidFaceIndex,
                Some(index),
                None,
                None,
            ));
        }
        let at = 12 + index as usize * 4;
        let offset = be_u32(data, at).ok_or(CollectionError::new(
            Kind::Malformed,
            Some(index),
            None,
            Some(at),
        ))? as usize;
        if offset < self.end
            || !offset.is_multiple_of(4)
            || offset.checked_add(12).is_none_or(|end| end > data.len())
        {
            return Err(CollectionError::new(
                Kind::Malformed,
                Some(index),
                None,
                Some(at),
            ));
        }
        Ok(offset)
    }
}

/// Return a collection's face count after validating its header, signature
/// metadata, and face-directory pointers. The contents of individual face
/// directories are validated when that face is extracted.
///
/// Ordinary standalone sfnt input is deliberately not treated as a one-face
/// collection. No file extension, filesystem access, or host font API is used.
///
/// # Errors
/// Returns a typed error for a noncollection, unsupported version, malformed
/// header/signature/pointer, or a source size or face count above the limits.
pub fn collection_face_count(data: &[u8]) -> Result<u32, CollectionError> {
    Header::parse(data).map(|header| header.faces)
}

#[derive(Debug)]
struct Table {
    tag: [u8; 4],
    range: Range<usize>,
    record: usize,
}

fn valid_tag(tag: &[u8; 4]) -> bool {
    let mut padding = false;
    for (index, &byte) in tag.iter().enumerate() {
        if !(0x20..=0x7e).contains(&byte)
            || (index == 0 && byte == b' ')
            || (padding && byte != b' ')
        {
            return false;
        }
        padding |= byte == b' ';
    }
    true
}

/// Extract one zero-based TTC/OTC face into deterministic standalone sfnt
/// bytes. Source tables may be shared by multiple faces and may appear in any
/// directory order. The output orders tables by tag, uses four-byte padding,
/// and recomputes every checksum and `head.checksumAdjustment`.
///
/// Collection signatures and any selected `DSIG` table are omitted: changing
/// the container invalidates them. Other table bytes are retained exactly,
/// except for `head.checksumAdjustment`. A valid `head` table is required;
/// downstream [`crate::Font::parse`] and renderer admission validate the
/// selected font's metrics, character maps, and supported outline programs.
/// The caller's input bytes are never modified.
///
/// # Errors
/// Returns a typed error for an invalid index, unsupported face container,
/// duplicate tags, malformed table records, missing/truncated `head`, or an
/// input/output/count limit. The complete directory and output size are
/// validated before allocating the font output.
pub fn extract_collection_face(data: &[u8], face_index: u32) -> Result<Vec<u8>, CollectionError> {
    use CollectionErrorKind as Kind;
    let header = Header::parse(data)?;
    let face = header.face_offset(data, face_index)?;
    let failure =
        |kind, table, offset| CollectionError::new(kind, Some(face_index), table, Some(offset));
    let version = be_u32(data, face).ok_or(failure(Kind::Malformed, None, face))?;
    if !matches!(version, 0x0001_0000 | 0x7472_7565 | 0x4f54_544f) {
        return Err(failure(Kind::UnsupportedFaceFormat, None, face));
    }
    let count =
        usize::from(be_u16(data, face + 4).ok_or(failure(Kind::Malformed, None, face + 4))?);
    if count == 0 {
        return Err(failure(Kind::Malformed, None, face + 4));
    }
    if count > MAX_COLLECTION_TABLES {
        return Err(failure(Kind::LimitExceeded, None, face + 4));
    }
    let records_start = face + 12;
    let records_end = records_start.checked_add(count * 16).ok_or(failure(
        Kind::Malformed,
        None,
        records_start,
    ))?;
    let records = data.get(records_start..records_end).ok_or(failure(
        Kind::Malformed,
        None,
        records_start,
    ))?;
    let mut tables = Vec::with_capacity(count);
    for (index, record) in records.as_chunks::<16>().0.iter().enumerate() {
        let tag = [record[0], record[1], record[2], record[3]];
        let at = records_start + index * 16;
        if !valid_tag(&tag) {
            return Err(failure(Kind::Malformed, Some(tag), at));
        }
        let offset = u32::from_be_bytes([record[8], record[9], record[10], record[11]]) as usize;
        let length = u32::from_be_bytes([record[12], record[13], record[14], record[15]]) as usize;
        let end = offset
            .checked_add(length)
            .ok_or(failure(Kind::Malformed, Some(tag), at + 8))?;
        if !offset.is_multiple_of(4) || end > data.len() || (length != 0 && offset < header.end) {
            return Err(failure(Kind::Malformed, Some(tag), at + 8));
        }
        tables.push(Table {
            tag,
            range: offset..end,
            record: at,
        });
    }
    tables.sort_unstable_by_key(|table| table.tag);
    for pair in tables.windows(2) {
        if pair[0].tag == pair[1].tag {
            return Err(failure(
                Kind::DuplicateTable,
                Some(pair[1].tag),
                pair[1].record,
            ));
        }
    }
    let head = tables
        .iter()
        .find(|table| table.tag == *b"head")
        .ok_or(failure(Kind::MissingTable, Some(*b"head"), face))?;
    if head.range.len() < 54 {
        return Err(failure(Kind::Malformed, Some(*b"head"), head.record));
    }
    tables.retain(|table| table.tag != *b"DSIG");
    let output_count = tables.len();
    let mut output_size = 12 + output_count * 16;
    for table in &tables {
        let padded = table
            .range
            .len()
            .checked_add(3)
            .map(|length| length & !3)
            .ok_or(failure(Kind::LimitExceeded, Some(table.tag), table.record))?;
        output_size = output_size.checked_add(padded).ok_or(failure(
            Kind::LimitExceeded,
            Some(table.tag),
            table.record,
        ))?;
        if output_size > MAX_COLLECTION_BYTES {
            return Err(failure(Kind::LimitExceeded, Some(table.tag), table.record));
        }
    }
    // At least head remains, so output_count is nonzero and ilog2 is defined.
    let power = 1usize << output_count.ilog2();
    let mut output = Vec::with_capacity(output_size);
    output.extend_from_slice(&version.to_be_bytes());
    for value in [
        output_count,
        power * 16,
        output_count.ilog2() as usize,
        (output_count - power) * 16,
    ] {
        output.extend_from_slice(&(value as u16).to_be_bytes());
    }
    output.resize(12 + output_count * 16, 0);
    let mut head_start = 0;
    for (index, table) in tables.iter().enumerate() {
        let offset = output.len();
        let start = 12 + index * 16;
        let bytes = data.get(table.range.clone()).ok_or(failure(
            Kind::Malformed,
            Some(table.tag),
            table.record,
        ))?;
        output.extend_from_slice(bytes);
        if table.tag == *b"head" {
            head_start = offset;
            write_u32(&mut output, offset + 8, 0).ok_or(failure(
                Kind::Malformed,
                Some(table.tag),
                table.record,
            ))?;
        }
        let checksum = table_checksum(&output[offset..]);
        output.resize((output.len() + 3) & !3, 0);
        output[start..start + 4].copy_from_slice(&table.tag);
        for (at, value) in [
            (start + 4, checksum),
            (start + 8, offset as u32),
            (start + 12, bytes.len() as u32),
        ] {
            write_u32(&mut output, at, value).ok_or(failure(
                Kind::Malformed,
                Some(table.tag),
                table.record,
            ))?;
        }
    }
    let adjustment = 0xb1b0_afbau32.wrapping_sub(table_checksum(&output));
    write_u32(&mut output, head_start + 8, adjustment).ok_or(failure(
        Kind::Malformed,
        Some(*b"head"),
        face,
    ))?;
    Ok(output)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{Font, FontError};

    const V1: &[u8] = include_bytes!("../fonts/test-collection/FmdMixedFacesV1.ttc");
    const V2: &[u8] = include_bytes!("../fonts/test-collection/FmdMixedFacesV2.otc");
    const TTF: &[u8] = include_bytes!("../fonts/test-collection/FmdCollectionTrueType.ttf");
    const CFF: &[u8] = include_bytes!("../fonts/test-cid-cff/FmdCidFormat3.otf");

    fn directory(data: &[u8], face: u32) -> usize {
        be_u32(data, 12 + face as usize * 4).unwrap() as usize
    }

    fn record(data: &[u8], face: u32, tag: &[u8; 4]) -> usize {
        let offset = directory(data, face);
        (0..usize::from(be_u16(data, offset + 4).unwrap()))
            .map(|i| offset + 12 + i * 16)
            .find(|&at| &data[at..at + 4] == tag)
            .unwrap()
    }

    fn put32(data: &mut [u8], at: usize, value: u32) {
        data[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn error(data: &[u8], face: u32, kind: CollectionErrorKind) {
        assert_eq!(extract_collection_face(data, face).unwrap_err().kind, kind);
    }

    #[test]
    fn both_collection_versions_recover_exact_independent_standalone_faces() {
        for source in [V1, V2] {
            assert_eq!(collection_face_count(source).unwrap(), 2);
            assert_eq!(
                Font::parse(source.to_vec()).unwrap_err(),
                FontError::BadMagic
            );
            for (face, expected) in [TTF, CFF].into_iter().enumerate() {
                let bytes = extract_collection_face(source, face as u32).unwrap();
                assert_eq!(bytes, expected, "face {face}");
                assert_eq!(table_checksum(&bytes), 0xb1b0_afba);
                assert_eq!(bytes, extract_collection_face(source, face as u32).unwrap());
                let font = Font::parse(bytes).unwrap();
                assert_eq!(
                    [
                        font.glyph_index('A'),
                        font.glyph_index('B'),
                        font.glyph_index('日')
                    ],
                    [1, 2, 3]
                );
                assert_eq!(
                    font.glyph_bbox(1),
                    Some([100, 0, 300, if face == 0 { 450 } else { 300 }])
                );
            }
            // The fixture really shares data, rather than concatenating fonts.
            for tag in [b"head", b"hhea", b"hmtx", b"cmap", b"post"] {
                assert_eq!(
                    be_u32(source, record(source, 0, tag) + 8),
                    be_u32(source, record(source, 1, tag) + 8)
                );
            }
        }
    }

    #[test]
    fn directory_order_and_source_checksums_do_not_change_normalized_output() {
        let mut data = V1.to_vec();
        let face = directory(&data, 0);
        let count = usize::from(be_u16(&data, face + 4).unwrap());
        let rows: Vec<_> = data[face + 12..face + 12 + count * 16]
            .as_chunks::<16>()
            .0
            .iter()
            .map(|row| row.to_vec())
            .rev()
            .collect();
        for (index, row) in rows.iter().enumerate() {
            let at = face + 12 + index * 16;
            data[at..at + 16].copy_from_slice(row);
            put32(&mut data, at + 4, 0xdead_beef);
        }
        // Collections do not use this adjustment. Rewrite it from actual
        // output bytes even if the source carries a stale standalone value.
        let head = be_u32(&data, record(&data, 0, b"head") + 8).unwrap() as usize;
        put32(&mut data, head + 8, 123);
        assert_eq!(extract_collection_face(&data, 0).unwrap(), TTF);
        assert_eq!(extract_collection_face(&data, 1).unwrap(), CFF);
    }

    #[test]
    fn collection_and_face_signatures_are_omitted_without_ignoring_bad_ranges() {
        use CollectionErrorKind as Kind;
        let mut data = V2.to_vec();
        // Replace a dispensable source table with a selected-face signature.
        let post = record(&data, 0, b"post");
        data[post..post + 4].copy_from_slice(b"DSIG");
        let bytes = extract_collection_face(&data, 0).unwrap();
        let count = usize::from(be_u16(&bytes, 4).unwrap());
        assert_eq!(count, usize::from(be_u16(TTF, 4).unwrap()) - 1);
        assert!(
            bytes[12..12 + count * 16]
                .as_chunks::<16>()
                .0
                .iter()
                .all(|row| &row[..4] != b"DSIG")
        );
        assert_eq!(table_checksum(&bytes), 0xb1b0_afba);
        assert_eq!(
            Font::parse(bytes).unwrap().glyph_bbox(1),
            Some([100, 0, 300, 450])
        );
        put32(&mut data, post + 8, u32::MAX);
        error(&data, 0, Kind::Malformed);

        // Version 2 also permits an entirely null signature descriptor.
        let mut unsigned = V2.to_vec();
        unsigned[20..32].fill(0);
        assert_eq!(extract_collection_face(&unsigned, 1).unwrap(), CFF);
        for (at, value) in [
            (20, 0),
            (20, 1),
            (24, 0),
            (24, u32::MAX),
            (28, 0),
            (28, u32::MAX),
        ] {
            let mut broken = V2.to_vec();
            put32(&mut broken, at, value);
            assert_eq!(
                collection_face_count(&broken).unwrap_err().kind,
                Kind::Malformed
            );
        }
    }

    #[test]
    fn collection_headers_and_face_indices_have_typed_bounded_failures() {
        use CollectionErrorKind as Kind;
        for source in [&[][..], &TTF[..4], &CFF[..4]] {
            error(source, 0, Kind::NotCollection);
        }
        for source in [&V1[..4], &V1[..11], &V1[..19], &V2[..31]] {
            error(source, 0, Kind::Malformed);
        }
        for (at, value, kind) in [
            (4, 0x0003_0000, Kind::UnsupportedVersion),
            (8, 0, Kind::Malformed),
            (8, MAX_COLLECTION_FACES + 1, Kind::LimitExceeded),
            (12, 0, Kind::Malformed),
            (12, u32::MAX, Kind::Malformed),
            (12, 21, Kind::Malformed),
        ] {
            let mut data = V1.to_vec();
            put32(&mut data, at, value);
            error(&data, 0, kind);
        }
        for index in [2, u32::MAX] {
            let failure = extract_collection_face(V1, index).unwrap_err();
            assert_eq!(failure.kind, Kind::InvalidFaceIndex);
            assert_eq!(failure.face_index, Some(index));
        }
        let oversized = vec![0; MAX_COLLECTION_BYTES + 1];
        error(&oversized, 0, Kind::LimitExceeded);
    }

    #[test]
    fn unsupported_unselected_face_does_not_block_a_supported_selection() {
        let mut data = V1.to_vec();
        let face = directory(&data, 1);
        put32(&mut data, face, 0x1234_5678);
        assert_eq!(collection_face_count(&data).unwrap(), 2);
        assert_eq!(extract_collection_face(&data, 0).unwrap(), TTF);
        error(&data, 1, CollectionErrorKind::UnsupportedFaceFormat);
        let face = directory(&data, 0);
        put32(&mut data, face, u32::from_be_bytes(*b"true"));
        let bytes = extract_collection_face(&data, 0).unwrap();
        assert_eq!(&bytes[..4], b"true");
        assert!(Font::parse(bytes).is_ok());
    }

    #[test]
    fn selected_table_directories_refuse_duplicates_bad_tags_and_missing_head() {
        use CollectionErrorKind as Kind;
        for count in [0u16, MAX_COLLECTION_TABLES as u16, u16::MAX] {
            let mut data = V1.to_vec();
            let face = directory(&data, 0);
            data[face + 4..face + 6].copy_from_slice(&count.to_be_bytes());
            error(
                &data,
                0,
                if usize::from(count) > MAX_COLLECTION_TABLES {
                    Kind::LimitExceeded
                } else {
                    Kind::Malformed
                },
            );
        }
        let mut duplicate = V1.to_vec();
        let at = record(&duplicate, 0, b"post");
        duplicate[at..at + 4].copy_from_slice(b"head");
        let failure = extract_collection_face(&duplicate, 0).unwrap_err();
        assert_eq!(failure.kind, Kind::DuplicateTable);
        assert_eq!(failure.table, Some(*b"head"));

        for tag in [*b"    ", *b"ab c", [0, 1, 2, 3]] {
            let mut data = V1.to_vec();
            let at = record(&data, 0, b"post");
            data[at..at + 4].copy_from_slice(&tag);
            error(&data, 0, Kind::Malformed);
        }
        let mut data = V1.to_vec();
        let at = record(&data, 0, b"head");
        data[at..at + 4].copy_from_slice(b"test");
        let failure = extract_collection_face(&data, 0).unwrap_err();
        assert_eq!(failure.kind, Kind::MissingTable);
        assert_eq!(failure.table, Some(*b"head"));
    }

    #[test]
    fn selected_table_ranges_are_checked_before_any_output_is_built() {
        use CollectionErrorKind as Kind;
        for (field, value) in [(8, u32::MAX), (8, 1), (8, 0), (12, u32::MAX)] {
            let mut data = V1.to_vec();
            let at = record(&data, 0, b"glyf");
            put32(&mut data, at + field, value);
            let failure = extract_collection_face(&data, 0).unwrap_err();
            assert_eq!(failure.kind, Kind::Malformed);
            assert_eq!(failure.table, Some(*b"glyf"));
        }
        for length in [0, 12, 53] {
            let mut data = V1.to_vec();
            let at = record(&data, 0, b"head");
            put32(&mut data, at + 12, length);
            error(&data, 0, Kind::Malformed);
        }
    }

    #[test]
    fn aliased_source_tables_cannot_amplify_an_extracted_face_past_its_budget() {
        // Six table records fit in a tiny header, and five alias one payload.
        // The source is only 8 MiB; normalized copies would exceed 32 MiB.
        let face = 16usize;
        let count = 6usize;
        let head = face + 12 + count * 16;
        let payload = head + 56;
        let payload_len = 8 * 1024 * 1024;
        let mut data = vec![0; payload + payload_len];
        data[..4].copy_from_slice(b"ttcf");
        put32(&mut data, 4, 0x0001_0000);
        put32(&mut data, 8, 1);
        put32(&mut data, 12, face as u32);
        put32(&mut data, face, 0x0001_0000);
        data[face + 4..face + 6].copy_from_slice(&(count as u16).to_be_bytes());
        for (index, tag) in [*b"AAAA", *b"BBBB", *b"CCCC", *b"DDDD", *b"EEEE", *b"head"]
            .iter()
            .enumerate()
        {
            let at = face + 12 + index * 16;
            data[at..at + 4].copy_from_slice(tag);
            let is_head = tag == b"head";
            put32(
                &mut data,
                at + 8,
                if is_head { head } else { payload } as u32,
            );
            put32(
                &mut data,
                at + 12,
                if is_head { 54 } else { payload_len } as u32,
            );
        }
        error(&data, 0, CollectionErrorKind::LimitExceeded);
    }
}
