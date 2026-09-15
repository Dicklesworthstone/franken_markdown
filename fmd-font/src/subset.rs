//! Diagnosable, strict subset APIs. Legacy Option APIs retain their tolerance.
use crate::{Font, be_u16, find_table_full, outline::OutlineError};

/// Font program encoding; never infer a PDF dictionary from the filename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingFormat {
    /// sfnt with glyf outlines: PDF FontFile2 / CIDFontType2.
    TrueType,
    /// OTTO sfnt with CFF outlines: PDF FontFile3 / Subtype OpenType;
    /// use CIDFontType0 (not CIDFontType2 or CIDToGIDMap).
    OpenTypeCff,
}
/// A compact font and its original-glyph to subset-glyph mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subset {
    pub bytes: Vec<u8>,
    /// Absent glyphs use [`crate::MISSING_GLYPH_REMAP`].
    pub glyph_map: Vec<u16>,
    pub format: EmbeddingFormat,
}
/// Stable categories independent of host paths and diagnostic prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsetErrorKind {
    UnsupportedFormat,
    UnsupportedOperator,
    MissingTable,
    InvalidGlyph,
    Malformed,
    BudgetExceeded,
    Capacity,
}
/// Bounded failure context. Offsets are relative to the named table unless
/// the table is `sfnt`, in which case they are file offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubsetError {
    pub kind: SubsetErrorKind,
    pub table: [u8; 4],
    pub glyph: Option<u16>,
    pub offset: Option<usize>,
}
impl SubsetError {
    pub(crate) const fn new(kind: SubsetErrorKind, table: [u8; 4], glyph: Option<u16>, offset: Option<usize>) -> Self {
        Self { kind, table, glyph, offset }
    }
}
impl core::fmt::Display for SubsetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "font subset {:?} in {:?}, glyph {:?}, offset {:?}", self.kind, self.table, self.glyph, self.offset)
    }
}
impl std::error::Error for SubsetError {}

impl Font {
    /// Strict subset for browser embedding. Unlike the legacy Option API,
    /// invalid glyphs and malformed composites are refused, never repaired.
    pub fn try_subset(&self, keep: &[char]) -> Result<Subset, SubsetError> {
        let glyphs: Vec<_> = keep.iter().map(|&ch| self.glyph_index(ch)).collect();
        self.try_subset_impl(&glyphs, keep, true)
    }
    /// Strict subset of a pre-shaped glyph set, with explicit embedding format.
    pub fn try_subset_glyphs(&self, glyphs: &[u16], cmap_chars: &[char]) -> Result<Subset, SubsetError> {
        self.try_subset_impl(glyphs, cmap_chars, false)
    }
    /// Dense-map spelling for callers migrating from the legacy lookup API.
    pub fn try_subset_glyphs_with_lookup(&self, glyphs: &[u16], cmap_chars: &[char]) -> Result<Subset, SubsetError> {
        self.try_subset_glyphs(glyphs, cmap_chars)
    }
    fn try_subset_impl(&self, glyphs: &[u16], chars: &[char], web: bool) -> Result<Subset, SubsetError> {
        if !self.has_glyf_outlines() {
            return Err(SubsetError::new(SubsetErrorKind::UnsupportedFormat, *b"sfnt", None, None));
        }
        for (tag, minimum) in [(b"head", 54), (b"hhea", 36), (b"maxp", 6), (b"loca", (usize::from(self.num_glyphs) + 1) * if self.loca_long { 4 } else { 2 }), (b"hmtx", usize::from(self.num_h_metrics) * 4 + usize::from(self.num_glyphs.saturating_sub(self.num_h_metrics)) * 2)] {
            let (offset, len) = find_table_full(&self.data, tag).ok_or(SubsetError::new(SubsetErrorKind::MissingTable, *tag, None, None))?;
            if len < minimum || offset.checked_add(len).is_none_or(|end| end > self.data.len()) {
                return Err(SubsetError::new(SubsetErrorKind::Malformed, *tag, None, Some(len)));
            }
        }
        if self.num_h_metrics == 0 || self.num_h_metrics > self.num_glyphs {
            return Err(SubsetError::new(SubsetErrorKind::Malformed, *b"hhea", None, Some(34)));
        }
        let (bytes, glyph_map) = self.subset_core(glyphs, chars, web, true)?;
        Ok(Subset { bytes, glyph_map, format: EmbeddingFormat::TrueType })
    }
    pub(crate) fn validate_subset_glyph(&self, gid: u16) -> Result<(), SubsetError> {
        let error = |kind, offset| SubsetError::new(kind, *b"glyf", Some(gid), offset);
        if gid >= self.num_glyphs { return Err(error(SubsetErrorKind::InvalidGlyph, None)); }
        self.glyph_outline(gid).map_err(|e| error(match e {
            OutlineError::BudgetExceeded => SubsetErrorKind::BudgetExceeded,
            _ => SubsetErrorKind::Malformed,
        }, None))?;
        let d = self.glyph_data(gid).ok_or(error(SubsetErrorKind::Malformed, None))?;
        if !self.is_composite(gid) { return Ok(()); }
        let mut p = 10;
        loop {
            let bad = error(SubsetErrorKind::Malformed, Some(p));
            let flags = be_u16(d, p).ok_or(bad)?;
            let child = be_u16(d, p + 2).ok_or(bad)?;
            if child >= self.num_glyphs { return Err(error(SubsetErrorKind::InvalidGlyph, Some(p + 2))); }
            p += 4 + if flags & 1 != 0 { 4 } else { 2 };
            p += if flags & 8 != 0 { 2 } else if flags & 64 != 0 { 4 } else if flags & 128 != 0 { 8 } else { 0 };
            if p > d.len() { return Err(bad); }
            if flags & 32 == 0 {
                if flags & 256 != 0 {
                    let n = usize::from(be_u16(d, p).ok_or(bad)?);
                    if p + 2 + n > d.len() { return Err(bad); }
                }
                return Ok(());
            }
        }
    }
}
