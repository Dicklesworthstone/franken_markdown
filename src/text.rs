//! Clean-room TrueType / OpenType font reader.
//!
//! Parses the sfnt table directory plus the metric, character-map, outline, and
//! layout tables we need to lay out and embed text: `head` (units per em),
//! `maxp` (glyph count), `hhea`/`hmtx` (vertical metrics + advance widths),
//! `cmap` (character → glyph, formats 4 and 12), `glyf`/`loca` (TrueType
//! outlines for subsetting), legacy `kern` format-0 pair kerning, focused GPOS
//! pair positioning, and GSUB standard ligatures. Latin-first, zero-dependency,
//! and free of `unsafe`/`unwrap`/`panic` — every read is bounds-checked.
//!
//! CFF/OpenType outline subsetting and broader script shaping are still future
//! increments. The current module is enough for bundled TrueType fonts, real
//! PDF metrics, deterministic subset embedding, kerning, ligatures, and
//! selectable `ToUnicode` output.

/// Hard ceiling on how many glyphs a single OpenType layout structure may
/// enumerate. A font cannot contain more than 65 536 glyphs, so a well-formed
/// Coverage / ligature / pair table never exceeds this. It bounds the work an
/// untrusted host font can drive: without it, a tiny malicious table (aliased
/// offsets or a 6-byte range claiming 65 536 ids) amplifies into billions of
/// iterations or gigabytes of retained state — a CPU-hang / OOM-kill DoS.
const MAX_LAYOUT_GLYPHS: usize = 65_536;

/// Alias of the layout ceiling used specifically for Coverage-table expansion.
const MAX_COVERAGE_GLYPHS: usize = MAX_LAYOUT_GLYPHS;
const MISSING_GLYPH_REMAP: u16 = u16::MAX;

#[derive(Debug, Clone)]
struct Cmap4Segment {
    start: u16,
    end: u16,
    id_delta: u16,
    id_range_offset: u16,
    id_range_offset_pos: usize,
}

#[derive(Debug, Clone)]
struct Cmap4Cache {
    segments: Vec<Cmap4Segment>,
    sorted_by_end: bool,
}

/// A parsed font, owning its backing bytes.
#[derive(Debug, Clone)]
pub struct Font {
    data: Vec<u8>,
    /// Font design units per em (the coordinate scale; advances are in these).
    pub units_per_em: u16,
    /// Number of glyphs in the font.
    pub num_glyphs: u16,
    /// Typographic ascender (design units).
    pub ascent: i16,
    /// Typographic descender (design units, usually negative).
    pub descent: i16,
    /// Recommended extra line gap (design units).
    pub line_gap: i16,
    num_h_metrics: u16,
    hmtx_off: usize,
    cmap_off: usize,
    cmap_format: u16,
    cmap4_cache: Option<Cmap4Cache>,
    /// `(offset, length)` of the `glyf` table, when the font has TrueType
    /// outlines. Absent for CFF/OpenType (`OTTO`) fonts.
    glyf: Option<(usize, usize)>,
    /// Offset of the `loca` table (glyph offsets into `glyf`).
    loca_off: Option<usize>,
    /// True when `loca` uses the 32-bit (long) offset format.
    loca_long: bool,
    /// `(pair_record_offset, pair_count)` for a legacy `kern` format-0 table.
    kern0: Option<(usize, u16)>,
}

/// Why a font failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// Not a recognized sfnt (`0x00010000`, `true`, or `OTTO`).
    BadMagic,
    /// A required table was absent.
    MissingTable(&'static str),
    /// The file ended before a required field could be read.
    Truncated,
    /// No usable Unicode `cmap` subtable (format 4 or 12) was found.
    NoUnicodeCmap,
}

impl core::fmt::Display for FontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a TrueType/OpenType font"),
            Self::MissingTable(t) => write!(f, "missing required font table: {t}"),
            Self::Truncated => write!(f, "font data is truncated"),
            Self::NoUnicodeCmap => write!(f, "no usable Unicode cmap (format 4/12)"),
        }
    }
}

impl std::error::Error for FontError {}

fn be_u16(d: &[u8], o: usize) -> Option<u16> {
    let bytes = d.get(o..o.checked_add(2)?)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}
fn be_i16(d: &[u8], o: usize) -> Option<i16> {
    be_u16(d, o).map(|v| v as i16)
}
fn be_u32(d: &[u8], o: usize) -> Option<u32> {
    let bytes = d.get(o..o.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn off(base: usize, delta: usize) -> Option<usize> {
    base.checked_add(delta)
}

fn off_mul(base: usize, index: usize, stride: usize) -> Option<usize> {
    base.checked_add(index.checked_mul(stride)?)
}

fn be_u16_at(d: &[u8], base: usize, delta: usize) -> Option<u16> {
    be_u16(d, off(base, delta)?)
}

fn be_u32_at(d: &[u8], base: usize, delta: usize) -> Option<u32> {
    be_u32(d, off(base, delta)?)
}

fn bytes_at(d: &[u8], base: usize, len: usize) -> Option<&[u8]> {
    d.get(base..off(base, len)?)
}

/// Write a big-endian `u16` at `off` into a mutable buffer, bounds-checked.
fn write_u16(d: &mut [u8], off: usize, v: u16) -> Option<()> {
    let b = v.to_be_bytes();
    let dst = d.get_mut(off..off.checked_add(2)?)?;
    dst.copy_from_slice(&b);
    Some(())
}

/// Write a big-endian `u32` at `off` into a mutable buffer, bounds-checked.
fn write_u32(d: &mut [u8], off: usize, v: u32) -> Option<()> {
    let b = v.to_be_bytes();
    let dst = d.get_mut(off..off.checked_add(4)?)?;
    dst.copy_from_slice(&b);
    Some(())
}

/// sfnt table checksum: the wrapping `u32` sum of the table's bytes read as
/// big-endian 32-bit words, with the final partial word zero-padded.
fn table_checksum(d: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut chunks = d.chunks_exact(4);
    for c in &mut chunks {
        sum = sum.wrapping_add(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut buf = [0u8; 4];
        buf[..rem.len()].copy_from_slice(rem);
        sum = sum.wrapping_add(u32::from_be_bytes(buf));
    }
    sum
}

fn find_table(d: &[u8], tag: &[u8; 4]) -> Option<usize> {
    find_table_full(d, tag).map(|(off, _)| off)
}

/// Locate a table by tag, returning `(offset, length)`.
fn find_table_full(d: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    let num_tables = be_u16(d, 4)? as usize;
    for i in 0..num_tables {
        let rec = off_mul(12, i, 16)?;
        if bytes_at(d, rec, 4)? == tag {
            return Some((
                be_u32_at(d, rec, 8)? as usize,
                be_u32_at(d, rec, 12)? as usize,
            ));
        }
    }
    None
}

/// Locate a legacy TrueType `kern` v0 format-0 horizontal pair table.
fn find_kern0(d: &[u8]) -> Option<(usize, u16)> {
    let (kern, kern_len) = find_table_full(d, b"kern")?;
    let table_end = kern.checked_add(kern_len)?;
    let version = be_u16(d, kern)?;
    let n_tables = be_u16_at(d, kern, 2)? as usize;
    if version != 0 {
        return None;
    }

    let mut sub = off(kern, 4)?;
    for _ in 0..n_tables {
        if sub.checked_add(6)? > table_end {
            return None;
        }
        let length = be_u16_at(d, sub, 2)? as usize;
        let coverage = be_u16_at(d, sub, 4)?;
        let format = coverage >> 8;
        let horizontal = coverage & 0x0001 != 0;
        let minimum = coverage & 0x0002 != 0;
        let pairs = off(sub, 14)?;
        if format == 0 && horizontal && !minimum && length >= 14 {
            let sub_end = sub.checked_add(length)?;
            if sub_end > table_end {
                return None;
            }
            let n_pairs = be_u16_at(d, sub, 6)?;
            let bytes_needed = (n_pairs as usize).checked_mul(6)?;
            if pairs.checked_add(bytes_needed)? <= sub_end {
                return Some((pairs, n_pairs));
            }
            return None;
        }
        if length == 0 {
            return None;
        }
        sub = sub.checked_add(length)?;
    }
    None
}

impl Font {
    /// Parse a font from its raw bytes (e.g. an `include_bytes!` blob).
    ///
    /// # Errors
    /// Returns a [`FontError`] for a non-sfnt file, a missing required table, a
    /// truncated file, or the absence of a usable Unicode `cmap`.
    pub fn parse(data: Vec<u8>) -> Result<Self, FontError> {
        let d = data.as_slice();
        let magic = be_u32(d, 0).ok_or(FontError::Truncated)?;
        // 0x00010000 = TrueType outlines; "true"; "OTTO" = CFF/OpenType.
        if magic != 0x0001_0000 && magic != 0x7472_7565 && magic != 0x4F54_544F {
            return Err(FontError::BadMagic);
        }

        let head = find_table(d, b"head").ok_or(FontError::MissingTable("head"))?;
        let maxp = find_table(d, b"maxp").ok_or(FontError::MissingTable("maxp"))?;
        let hhea = find_table(d, b"hhea").ok_or(FontError::MissingTable("hhea"))?;
        let hmtx = find_table(d, b"hmtx").ok_or(FontError::MissingTable("hmtx"))?;
        let cmap = find_table(d, b"cmap").ok_or(FontError::MissingTable("cmap"))?;

        let units_per_em =
            be_u16(d, off(head, 18).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
        let num_glyphs =
            be_u16(d, off(maxp, 4).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
        let ascent =
            be_i16(d, off(hhea, 4).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
        let descent =
            be_i16(d, off(hhea, 6).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
        let line_gap =
            be_i16(d, off(hhea, 8).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
        let num_h_metrics =
            be_u16(d, off(hhea, 34).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;

        let (cmap_off, cmap_format) = select_cmap(d, cmap).ok_or(FontError::NoUnicodeCmap)?;
        let cmap4_cache = if cmap_format == 4 {
            parse_cmap4_cache(d, cmap_off)
        } else {
            None
        };

        // Outline tables are optional: present for TrueType (glyf) fonts, absent
        // for CFF/OpenType. Their absence is not an error here.
        let loca_long = off(head, 50)
            .and_then(|offset| be_i16(d, offset))
            .unwrap_or(0)
            != 0;
        let loca_off = find_table(d, b"loca");
        let glyf = find_table_full(d, b"glyf");
        let kern0 = find_kern0(d);

        Ok(Self {
            data,
            units_per_em,
            num_glyphs,
            ascent,
            descent,
            line_gap,
            num_h_metrics,
            hmtx_off: hmtx,
            cmap_off,
            cmap_format,
            cmap4_cache,
            glyf,
            loca_off,
            loca_long,
            kern0,
        })
    }

    /// True when the font carries TrueType (`glyf`) outlines we can read/subset.
    #[must_use]
    pub fn has_glyf_outlines(&self) -> bool {
        self.glyf.is_some() && self.loca_off.is_some()
    }

    /// The `[start, end)` byte range of glyph `gid` within the `glyf` table.
    /// Returns `None` if the font has no `glyf`/`loca`, or `Some((s, s))` for an
    /// empty glyph (e.g. space).
    fn glyph_range(&self, gid: u16) -> Option<(usize, usize)> {
        let loca = self.loca_off?;
        let (glyf_off, glyf_len) = self.glyf?;
        let i = gid as usize;
        let (start, end) = if self.loca_long {
            (
                be_u32(&self.data, off_mul(loca, i, 4)?)? as usize,
                be_u32(&self.data, off_mul(loca, i.checked_add(1)?, 4)?)? as usize,
            )
        } else {
            // Short loca stores offsets / 2.
            (
                be_u16(&self.data, off_mul(loca, i, 2)?)? as usize * 2,
                be_u16(&self.data, off_mul(loca, i.checked_add(1)?, 2)?)? as usize * 2,
            )
        };
        if end < start || end > glyf_len {
            return None;
        }
        Some((off(glyf_off, start)?, off(glyf_off, end)?))
    }

    /// Raw `glyf` bytes for glyph `gid` (for subset embedding), or `None`.
    /// An empty (zero-length) glyph yields `Some(&[])`.
    #[must_use]
    pub fn glyph_data(&self, gid: u16) -> Option<&[u8]> {
        let (s, e) = self.glyph_range(gid)?;
        self.data.get(s..e)
    }

    /// Glyph bounding box `[xMin, yMin, xMax, yMax]` (design units), or `None`
    /// for an empty glyph / no outlines.
    #[must_use]
    pub fn glyph_bbox(&self, gid: u16) -> Option<[i16; 4]> {
        let (s, e) = self.glyph_range(gid)?;
        if e <= s {
            return None; // empty glyph (no contours)
        }
        Some([
            be_i16(&self.data, off(s, 2)?)?,
            be_i16(&self.data, off(s, 4)?)?,
            be_i16(&self.data, off(s, 6)?)?,
            be_i16(&self.data, off(s, 8)?)?,
        ])
    }

    /// True when glyph `gid` is a composite (built from component glyphs).
    #[must_use]
    pub fn is_composite(&self, gid: u16) -> bool {
        match self.glyph_range(gid) {
            Some((s, e)) if e > s => be_i16(&self.data, s).is_some_and(|n| n < 0),
            _ => false,
        }
    }

    /// Component glyph ids referenced by a composite glyph (for transitive
    /// subsetting). Empty for simple or empty glyphs.
    #[must_use]
    pub fn glyph_components(&self, gid: u16) -> Vec<u16> {
        const ARG_WORDS: u16 = 0x0001;
        const WE_HAVE_SCALE: u16 = 0x0008;
        const MORE: u16 = 0x0020;
        const X_Y_SCALE: u16 = 0x0040;
        const TWO_BY_TWO: u16 = 0x0080;

        let mut out = Vec::new();
        let Some((s, e)) = self.glyph_range(gid) else {
            return out;
        };
        if e <= s || !be_i16(&self.data, s).is_some_and(|n| n < 0) {
            return out;
        }
        let Some(mut p) = off(s, 10) else {
            return out;
        };
        while let Some(component_record_end) = off(p, 4) {
            if component_record_end > e {
                break;
            }
            let Some(flags) = be_u16(&self.data, p) else {
                break;
            };
            let Some(comp) = off(p, 2).and_then(|offset| be_u16(&self.data, offset)) else {
                break;
            };
            let mut step = 4usize + if flags & ARG_WORDS != 0 { 4 } else { 2 };
            step += if flags & WE_HAVE_SCALE != 0 {
                2
            } else if flags & X_Y_SCALE != 0 {
                4
            } else if flags & TWO_BY_TWO != 0 {
                8
            } else {
                0
            };
            let Some(next) = off(p, step) else {
                break;
            };
            if next > e {
                break;
            }
            out.push(comp);
            p = next;
            if flags & MORE == 0 || p >= e {
                break;
            }
        }
        out
    }

    /// The advance width of glyph `gid` in design units. Glyphs past the
    /// `hmtx` metric run share the last advance (monospaced trailing run).
    #[must_use]
    pub fn advance_width(&self, gid: u16) -> u16 {
        let last = self.num_h_metrics.saturating_sub(1);
        let idx = gid.min(last) as usize;
        off_mul(self.hmtx_off, idx, 4)
            .and_then(|offset| be_u16(&self.data, offset))
            .unwrap_or(0)
    }

    /// The left side bearing of glyph `gid` in design units. Glyphs past the
    /// long-metric run share the last advance but keep their own trailing LSB.
    #[must_use]
    pub fn left_side_bearing(&self, gid: u16) -> i16 {
        if self.num_h_metrics == 0 {
            return 0;
        }
        let gid = gid as usize;
        let num_h_metrics = self.num_h_metrics as usize;
        let offset = if gid < num_h_metrics {
            off_mul(self.hmtx_off, gid, 4).and_then(|base| off(base, 2))
        } else {
            off_mul(self.hmtx_off, num_h_metrics, 4)
                .and_then(|base| off_mul(base, gid - num_h_metrics, 2))
        };
        offset
            .and_then(|offset| be_i16(&self.data, offset))
            .unwrap_or(0)
    }

    /// The glyph id for a character, or `0` (`.notdef`) if unmapped.
    #[must_use]
    pub fn glyph_index(&self, ch: char) -> u16 {
        let cp = ch as u32;
        match self.cmap_format {
            4 => self.cmap4_lookup(cp).unwrap_or(0),
            12 => self.cmap12_lookup(cp).unwrap_or(0),
            _ => 0,
        }
    }

    /// Advance width of `ch` in 1/1000 em (PDF text-space units). An unmapped
    /// `ch` resolves to glyph 0 (`.notdef`) and reserves that glyph's advance
    /// (so a tofu box still occupies its natural width); only an unparsable face
    /// with `units_per_em == 0` yields `0`.
    #[must_use]
    pub fn advance_1000(&self, ch: char) -> u32 {
        if self.units_per_em == 0 {
            return 0;
        }
        let aw = self.advance_width(self.glyph_index(ch)) as u32;
        aw * 1000 / self.units_per_em as u32
    }

    /// Kerning adjustment between two glyph ids in design units.
    ///
    /// Unsupported or absent kerning tables return zero. This currently supports
    /// legacy TrueType/Microsoft `kern` table version 0, format 0, horizontal
    /// pairs. GPOS pair positioning is tracked separately.
    #[must_use]
    pub fn kerning_between_glyphs(&self, left: u16, right: u16) -> i16 {
        let Some((pairs, n_pairs)) = self.kern0 else {
            return 0;
        };
        let target = ((left as u32) << 16) | right as u32;
        let mut lo = 0usize;
        let mut hi = n_pairs as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let Some(rec) = off_mul(pairs, mid, 6) else {
                return 0;
            };
            let Some(l) = be_u16(&self.data, rec) else {
                return 0;
            };
            let Some(r) = off(rec, 2).and_then(|offset| be_u16(&self.data, offset)) else {
                return 0;
            };
            let key = ((l as u32) << 16) | r as u32;
            if key == target {
                return off(rec, 4)
                    .and_then(|offset| be_i16(&self.data, offset))
                    .unwrap_or(0);
            }
            if key < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        0
    }

    /// Kerning adjustment between two characters in design units.
    #[must_use]
    pub fn kerning(&self, left: char, right: char) -> i16 {
        self.kerning_between_glyphs(self.glyph_index(left), self.glyph_index(right))
    }

    /// Kerning adjustment between two characters in 1/1000 em units.
    #[must_use]
    pub fn kerning_1000(&self, left: char, right: char) -> i32 {
        if self.units_per_em == 0 {
            return 0;
        }
        self.kerning(left, right) as i32 * 1000 / self.units_per_em as i32
    }

    fn cmap4_lookup(&self, cp: u32) -> Option<u16> {
        if cp > 0xFFFF {
            return Some(0);
        }
        let c = cp as u16;
        if let Some(cache) = &self.cmap4_cache {
            return self.cmap4_cached_lookup(c, cache);
        }
        self.cmap4_uncached_lookup(c)
    }

    fn cmap4_cached_lookup(&self, c: u16, cache: &Cmap4Cache) -> Option<u16> {
        let segment = if cache.sorted_by_end {
            let idx = cache.segments.partition_point(|seg| seg.end < c);
            cache.segments.get(idx)
        } else {
            cache.segments.iter().find(|seg| c <= seg.end)
        }?;

        if c < segment.start {
            return Some(0);
        }
        if segment.id_range_offset == 0 {
            return Some(c.wrapping_add(segment.id_delta));
        }
        let gi_addr = off(
            off(
                segment.id_range_offset_pos,
                segment.id_range_offset as usize,
            )?,
            2usize.checked_mul((c - segment.start) as usize)?,
        )?;
        let g = be_u16(&self.data, gi_addr)?;
        Some(if g == 0 {
            0
        } else {
            g.wrapping_add(segment.id_delta)
        })
    }

    fn cmap4_uncached_lookup(&self, c: u16) -> Option<u16> {
        let d = &self.data;
        let base = self.cmap_off;
        let seg_x2 = be_u16(d, off(base, 6)?)? as usize;
        let seg_count = seg_x2 / 2;
        let end_codes = off(base, 14)?;
        let start_codes = off(off(end_codes, seg_x2)?, 2)?; // +2 for reservedPad
        let id_deltas = off(start_codes, seg_x2)?;
        let id_range_offsets = off(id_deltas, seg_x2)?;
        for i in 0..seg_count {
            let end = be_u16(d, off_mul(end_codes, i, 2)?)?;
            if c > end {
                continue;
            }
            let start = be_u16(d, off_mul(start_codes, i, 2)?)?;
            if c < start {
                return Some(0);
            }
            let id_delta = be_u16(d, off_mul(id_deltas, i, 2)?)?;
            let iro_pos = off_mul(id_range_offsets, i, 2)?;
            let id_range_offset = be_u16(d, iro_pos)?;
            if id_range_offset == 0 {
                return Some(c.wrapping_add(id_delta));
            }
            let gi_addr = off(
                off(iro_pos, id_range_offset as usize)?,
                2usize.checked_mul((c - start) as usize)?,
            )?;
            let g = be_u16(d, gi_addr)?;
            return Some(if g == 0 { 0 } else { g.wrapping_add(id_delta) });
        }
        Some(0)
    }

    /// Build a new, minimal, valid TrueType (`glyf`) font containing glyph 0
    /// (`.notdef`) plus exactly the glyphs needed to render `keep` (mapped
    /// through the original `cmap`), transitively closing over composite
    /// components. Returns a fresh sfnt (`0x00010000`) suitable for a PDF
    /// `FontFile2`, or `None` on any failure (missing `glyf`/`loca`/required
    /// table, or a malformed read).
    #[must_use]
    pub fn subset(&self, keep: &[char]) -> Option<Vec<u8>> {
        let seed: Vec<u16> = keep.iter().map(|&c| self.glyph_index(c)).collect();
        self.subset_core(&seed, keep).map(|(bytes, _)| bytes)
    }

    /// Subset to an explicit glyph set (the closure still pulls in composite
    /// components), building the `cmap` from `cmap_chars`. Returns the font bytes
    /// plus the old->new glyph id remap — for callers that pre-shaped a glyph
    /// sequence (e.g. GSUB ligatures) and must emit the renumbered ids.
    ///
    /// # Errors
    /// Returns `None` for a font without `glyf`/`loca` outlines or on a malformed
    /// read (same conditions as [`Font::subset`]).
    pub fn subset_glyphs(
        &self,
        glyphs: &[u16],
        cmap_chars: &[char],
    ) -> Option<(Vec<u8>, std::collections::BTreeMap<u16, u16>)> {
        self.subset_core(glyphs, cmap_chars)
    }

    fn subset_core(
        &self,
        seed_glyphs: &[u16],
        cmap_chars: &[char],
    ) -> Option<(Vec<u8>, std::collections::BTreeMap<u16, u16>)> {
        // --- 1. Glyph closure ------------------------------------------------
        // Require TrueType outlines; CFF/`OTTO` fonts cannot be subset here.
        if !self.has_glyf_outlines() {
            return None;
        }
        let mut set: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
        set.insert(0);
        for &gid in seed_glyphs {
            if gid != 0 && gid < self.num_glyphs {
                set.insert(gid);
            }
        }
        // Transitively pull in composite components until the set is stable.
        // A worklist expands each glyph's components exactly once, so a chain of
        // composites (glyph k referencing k-1 referencing ...) is O(n) instead of
        // the O(n^2) that re-scanning the whole growing set each round would cost.
        // `BTreeSet::insert` returns false for an already-present component, which
        // also terminates cyclic/self-referential composites. The final set — and
        // hence the ascending `old_gids` and the whole subset — is identical.
        let mut worklist: Vec<u16> = set.iter().copied().collect();
        while let Some(gid) = worklist.pop() {
            if self.is_composite(gid) {
                for c in self.glyph_components(gid) {
                    if c < self.num_glyphs && set.insert(c) {
                        worklist.push(c);
                    }
                }
            }
        }
        let old_gids: Vec<u16> = set.into_iter().collect(); // ascending, 0 first

        // --- 2. Renumber old -> new -----------------------------------------
        let mut new_of: std::collections::BTreeMap<u16, u16> = std::collections::BTreeMap::new();
        let mut new_of_lookup = vec![MISSING_GLYPH_REMAP; usize::from(self.num_glyphs).max(1)];
        for (i, &g) in old_gids.iter().enumerate() {
            let new_gid = u16::try_from(i).ok()?;
            new_of.insert(g, new_gid);
            *new_of_lookup.get_mut(usize::from(g))? = new_gid;
        }
        let n = old_gids.len();
        let n_u16 = u16::try_from(n).ok()?;

        // --- 3. Rebuild glyf + loca (long offsets) --------------------------
        let mut glyf_bytes: Vec<u8> = Vec::new();
        let mut loca: Vec<u32> = Vec::with_capacity(n.checked_add(1)?);
        for &old in &old_gids {
            loca.push(u32::try_from(glyf_bytes.len()).ok()?);
            let gb = self.subset_glyph_bytes(old, &new_of_lookup)?;
            glyf_bytes.extend_from_slice(&gb);
            // Pad each glyph to a 4-byte multiple so the next glyph (and every
            // long-loca offset) is word-aligned.
            while glyf_bytes.len() % 4 != 0 {
                glyf_bytes.push(0);
            }
        }
        loca.push(u32::try_from(glyf_bytes.len()).ok()?);
        let mut loca_bytes: Vec<u8> = Vec::with_capacity(loca.len().checked_mul(4)?);
        for o in &loca {
            loca_bytes.extend_from_slice(&o.to_be_bytes());
        }

        // --- 4. Metric/meta tables ------------------------------------------
        // maxp: original bytes with numGlyphs (u16 @ +4) set to n.
        let (maxp_off, maxp_len) = find_table_full(&self.data, b"maxp")?;
        let mut maxp = self.data.get(maxp_off..off(maxp_off, maxp_len)?)?.to_vec();
        write_u16(&mut maxp, 4, n_u16)?;

        // hhea: original bytes with numberOfHMetrics (u16 @ +34) set to n.
        let (hhea_off, hhea_len) = find_table_full(&self.data, b"hhea")?;
        let mut hhea = self.data.get(hhea_off..off(hhea_off, hhea_len)?)?.to_vec();
        write_u16(&mut hhea, 34, n_u16)?;

        // hmtx: n long metrics (advanceWidth + true lsb), no trailing run.
        let mut hmtx: Vec<u8> = Vec::with_capacity(n.checked_mul(4)?);
        for &old in &old_gids {
            hmtx.extend_from_slice(&self.advance_width(old).to_be_bytes());
            hmtx.extend_from_slice(&self.left_side_bearing(old).to_be_bytes());
        }

        // head: original bytes; zero checkSumAdjustment (@ +8), force long loca.
        let (head_off, head_len) = find_table_full(&self.data, b"head")?;
        let mut head = self.data.get(head_off..off(head_off, head_len)?)?.to_vec();
        write_u32(&mut head, 8, 0)?;
        write_u16(&mut head, 50, 1)?; // indexToLocFormat = 1 (long)

        // cmap: fresh single format-4 (3,1) subtable.
        let cmap = self.build_cmap4(cmap_chars, &new_of_lookup)?;

        // name: minimal valid table (format 0, count 0, stringOffset 6).
        let mut name: Vec<u8> = Vec::with_capacity(6);
        name.extend_from_slice(&0u16.to_be_bytes());
        name.extend_from_slice(&0u16.to_be_bytes());
        name.extend_from_slice(&6u16.to_be_bytes());

        // post: format 3.0, 32 bytes, all metric fields zero.
        let mut post: Vec<u8> = Vec::with_capacity(32);
        post.extend_from_slice(&0x0003_0000u32.to_be_bytes()); // version 3.0
        post.extend_from_slice(&0u32.to_be_bytes()); // italicAngle
        post.extend_from_slice(&0u16.to_be_bytes()); // underlinePosition
        post.extend_from_slice(&0u16.to_be_bytes()); // underlineThickness
        post.extend_from_slice(&0u32.to_be_bytes()); // isFixedPitch
        post.extend_from_slice(&0u32.to_be_bytes()); // minMemType42
        post.extend_from_slice(&0u32.to_be_bytes()); // maxMemType42
        post.extend_from_slice(&0u32.to_be_bytes()); // minMemType1
        post.extend_from_slice(&0u32.to_be_bytes()); // maxMemType1

        // --- 5. Assemble the sfnt -------------------------------------------
        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"head", head),
            (b"hhea", hhea),
            (b"maxp", maxp),
            (b"hmtx", hmtx),
            (b"loca", loca_bytes),
            (b"glyf", glyf_bytes),
            (b"cmap", cmap),
            (b"name", name),
            (b"post", post),
        ];
        tables.sort_by(|a, b| a.0.cmp(b.0)); // ascending by tag

        let num_tables = tables.len();
        // searchRange = (2^floor(log2(n)))*16, entrySelector = floor(log2(n)).
        let mut pw: usize = 1;
        let mut es: u16 = 0;
        while pw * 2 <= num_tables {
            pw *= 2;
            es += 1;
        }
        let search_range = (pw as u16).wrapping_mul(16);
        let entry_selector = es;
        let range_shift = (num_tables as u16)
            .wrapping_mul(16)
            .wrapping_sub(search_range);

        let dir_size = 12 + num_tables * 16;
        let mut body: Vec<u8> = Vec::new();
        // (tag, checksum, offset, length)
        let mut records: Vec<([u8; 4], u32, u32, u32)> = Vec::with_capacity(num_tables);
        let mut head_offset: usize = 0;
        for (tag, bytes) in &tables {
            // Align each table's file start to a 4-byte boundary.
            while (dir_size + body.len()) % 4 != 0 {
                body.push(0);
            }
            let table_offset = dir_size + body.len();
            if *tag == b"head" {
                head_offset = table_offset;
            }
            let checksum = table_checksum(bytes);
            records.push((
                **tag,
                checksum,
                u32::try_from(table_offset).ok()?,
                u32::try_from(bytes.len()).ok()?,
            ));
            body.extend_from_slice(bytes);
        }
        while body.len() % 4 != 0 {
            body.push(0);
        }

        let mut out: Vec<u8> = Vec::with_capacity(dir_size + body.len());
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sfntVersion
        out.extend_from_slice(&(num_tables as u16).to_be_bytes());
        out.extend_from_slice(&search_range.to_be_bytes());
        out.extend_from_slice(&entry_selector.to_be_bytes());
        out.extend_from_slice(&range_shift.to_be_bytes());
        for (tag, checksum, toff, tlen) in &records {
            out.extend_from_slice(tag);
            out.extend_from_slice(&checksum.to_be_bytes());
            out.extend_from_slice(&toff.to_be_bytes());
            out.extend_from_slice(&tlen.to_be_bytes());
        }
        out.extend_from_slice(&body);

        // checkSumAdjustment: 0xB1B0AFBA - checksum(whole file with field zeroed).
        let file_checksum = table_checksum(&out);
        let adj = 0xB1B0_AFBAu32.wrapping_sub(file_checksum);
        write_u32(&mut out, off(head_offset, 8)?, adj)?;

        Some((out, new_of))
    }

    /// Glyph bytes for the subset: simple glyphs are copied without hinting
    /// instructions; composite glyphs are copied with each component `glyphIndex`
    /// (u16) rewritten from its old gid to its new gid and any trailing
    /// instructions removed. Empty glyphs yield an empty `Vec`.
    fn subset_glyph_bytes(&self, old: u16, new_of: &[u16]) -> Option<Vec<u8>> {
        const ARG_WORDS: u16 = 0x0001;
        const WE_HAVE_SCALE: u16 = 0x0008;
        const MORE: u16 = 0x0020;
        const X_Y_SCALE: u16 = 0x0040;
        const TWO_BY_TWO: u16 = 0x0080;
        const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;

        let data = self.glyph_data(old).unwrap_or(&[]);
        if data.is_empty() {
            return Some(Vec::new());
        }
        let num_contours = be_i16(data, 0)?;
        if num_contours >= 0 {
            return strip_simple_glyph_instructions(data, num_contours as usize);
        }
        // Composite: walk component records, rewriting each glyphIndex.
        let mut out = data.to_vec();
        let mut p = 10usize; // skip numberOfContours + 4x i16 bbox
        let mut instruction_flags_positions = Vec::new();
        loop {
            let flags = be_u16(&out, p)?;
            if flags & WE_HAVE_INSTRUCTIONS != 0 {
                instruction_flags_positions.push(p);
            }
            let comp_old = be_u16_at(&out, p, 2)?;
            // A component that fell outside the subset (e.g. a component gid
            // >= numGlyphs in a malformed font — the closure never reaches it)
            // is substituted with `.notdef` (new gid 0, always present) rather
            // than failing the whole font. The composite still renders, minus
            // the one bad component.
            let comp_new = remapped_gid(new_of, comp_old).unwrap_or(0);
            let nb = comp_new.to_be_bytes();
            *out.get_mut(off(p, 2)?)? = nb[0];
            *out.get_mut(off(p, 3)?)? = nb[1];
            p = off(p, 4)?;
            p = off(p, if flags & ARG_WORDS != 0 { 4 } else { 2 })?;
            if flags & WE_HAVE_SCALE != 0 {
                p = off(p, 2)?;
            } else if flags & X_Y_SCALE != 0 {
                p = off(p, 4)?;
            } else if flags & TWO_BY_TWO != 0 {
                p = off(p, 8)?;
            }
            if flags & MORE == 0 {
                break;
            }
        }
        if !instruction_flags_positions.is_empty() {
            for flags_pos in instruction_flags_positions {
                let flags = be_u16(&out, flags_pos)?;
                write_u16(&mut out, flags_pos, flags & !WE_HAVE_INSTRUCTIONS)?;
            }
            let instruction_len = be_u16(&out, p)? as usize;
            let instruction_start = off(p, 2)?;
            let instruction_end = off(instruction_start, instruction_len)?;
            if instruction_end > out.len() {
                return None;
            }
            out.drain(p..instruction_end);
        }
        Some(out)
    }

    /// Build a complete `cmap` table holding a single format-4 `(3,1)` subtable
    /// mapping every BMP char in `keep` to its NEW gid (one 1-char segment each,
    /// plus the mandatory final `0xFFFF` segment).
    fn build_cmap4(&self, keep: &[char], new_of: &[u16]) -> Option<Vec<u8>> {
        // Unique, ascending code -> new gid (0xFFFF reserved for the final seg).
        let mut codes: std::collections::BTreeMap<u16, u16> = std::collections::BTreeMap::new();
        for &ch in keep {
            let cp = ch as u32;
            if cp >= 0xFFFF {
                continue;
            }
            let old = self.glyph_index(ch);
            // Skip a char whose glyph is not in the subset (a malformed source
            // cmap, or a glyph the closure could not reach) instead of failing the
            // whole font; it falls back to `.notdef` at render time.
            let Some(ng) = remapped_gid(new_of, old) else {
                continue;
            };
            codes.insert(cp as u16, ng);
        }
        let entries: Vec<(u16, u16)> = codes.into_iter().collect();
        let seg_count = entries.len().checked_add(1)?; // + final 0xFFFF segment
        let sub_len = 16usize.checked_add(seg_count.checked_mul(8)?)?;
        let sub_len_u16 = u16::try_from(sub_len).ok()?;
        let seg_count_x2 = u16::try_from(seg_count.checked_mul(2)?).ok()?;

        let mut pw: usize = 1;
        let mut es: u16 = 0;
        while pw * 2 <= seg_count {
            pw *= 2;
            es += 1;
        }
        let search_range = u16::try_from(pw.checked_mul(2)?).ok()?;
        let entry_selector = es;
        let range_shift = seg_count_x2.checked_sub(search_range)?;

        let mut sub: Vec<u8> = Vec::with_capacity(sub_len);
        sub.extend_from_slice(&4u16.to_be_bytes()); // format
        sub.extend_from_slice(&sub_len_u16.to_be_bytes()); // length
        sub.extend_from_slice(&0u16.to_be_bytes()); // language
        sub.extend_from_slice(&seg_count_x2.to_be_bytes()); // segCountX2
        sub.extend_from_slice(&search_range.to_be_bytes());
        sub.extend_from_slice(&entry_selector.to_be_bytes());
        sub.extend_from_slice(&range_shift.to_be_bytes());
        // endCode[]
        for &(code, _) in &entries {
            sub.extend_from_slice(&code.to_be_bytes());
        }
        sub.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // reservedPad
        sub.extend_from_slice(&0u16.to_be_bytes());
        // startCode[]
        for &(code, _) in &entries {
            sub.extend_from_slice(&code.to_be_bytes());
        }
        sub.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // idDelta[]: (code + idDelta) & 0xFFFF == new gid.
        for &(code, ng) in &entries {
            sub.extend_from_slice(&ng.wrapping_sub(code).to_be_bytes());
        }
        // Final segment idDelta = 1.
        sub.extend_from_slice(&1u16.to_be_bytes());
        // idRangeOffset[] (all zero, glyphIdArray empty).
        for _ in &entries {
            sub.extend_from_slice(&0u16.to_be_bytes());
        }
        sub.extend_from_slice(&0u16.to_be_bytes());

        let mut cmap: Vec<u8> = Vec::with_capacity(12 + sub.len());
        cmap.extend_from_slice(&0u16.to_be_bytes()); // version
        cmap.extend_from_slice(&1u16.to_be_bytes()); // numTables
        cmap.extend_from_slice(&3u16.to_be_bytes()); // platformID (Windows)
        cmap.extend_from_slice(&1u16.to_be_bytes()); // encodingID (Unicode BMP)
        cmap.extend_from_slice(&12u32.to_be_bytes()); // subtable offset
        cmap.extend_from_slice(&sub);
        Some(cmap)
    }

    fn cmap12_lookup(&self, cp: u32) -> Option<u16> {
        let d = &self.data;
        let base = self.cmap_off;
        let num_groups = be_u32(d, off(base, 12)?)? as usize;
        for i in 0..num_groups {
            let g = off_mul(off(base, 16)?, i, 12)?;
            let start = be_u32(d, g)?;
            let end = be_u32(d, off(g, 4)?)?;
            if cp >= start && cp <= end {
                let start_gid = be_u32(d, off(g, 8)?)?;
                let gid = start_gid.checked_add(cp - start)?;
                return Some((gid & 0xFFFF) as u16);
            }
        }
        Some(0)
    }
}

fn remapped_gid(new_of: &[u16], old: u16) -> Option<u16> {
    match new_of.get(usize::from(old)).copied()? {
        MISSING_GLYPH_REMAP => None,
        gid => Some(gid),
    }
}

fn strip_simple_glyph_instructions(data: &[u8], contour_count: usize) -> Option<Vec<u8>> {
    let instruction_len_offset = off(10, contour_count.checked_mul(2)?)?;
    let instruction_len = be_u16(data, instruction_len_offset)? as usize;
    let instruction_start = off(instruction_len_offset, 2)?;
    let instruction_end = off(instruction_start, instruction_len)?;
    if instruction_end > data.len() {
        return None;
    }

    let mut out = Vec::with_capacity(data.len().saturating_sub(instruction_len));
    out.extend_from_slice(data.get(..instruction_len_offset)?);
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(data.get(instruction_end..)?);
    Some(out)
}

fn parse_cmap4_cache(d: &[u8], base: usize) -> Option<Cmap4Cache> {
    let seg_x2 = be_u16(d, off(base, 6)?)? as usize;
    let seg_count = seg_x2 / 2;
    let end_codes = off(base, 14)?;
    let start_codes = off(off(end_codes, seg_x2)?, 2)?;
    let id_deltas = off(start_codes, seg_x2)?;
    let id_range_offsets = off(id_deltas, seg_x2)?;

    let mut segments = Vec::with_capacity(seg_count);
    let mut sorted_by_end = true;
    let mut prev_end: Option<u16> = None;
    for i in 0..seg_count {
        let end = be_u16(d, off_mul(end_codes, i, 2)?)?;
        let start = be_u16(d, off_mul(start_codes, i, 2)?)?;
        let id_delta = be_u16(d, off_mul(id_deltas, i, 2)?)?;
        let id_range_offset_pos = off_mul(id_range_offsets, i, 2)?;
        let id_range_offset = be_u16(d, id_range_offset_pos)?;
        if prev_end.is_some_and(|prev| end < prev) {
            sorted_by_end = false;
        }
        prev_end = Some(end);
        segments.push(Cmap4Segment {
            start,
            end,
            id_delta,
            id_range_offset,
            id_range_offset_pos,
        });
    }

    Some(Cmap4Cache {
        segments,
        sorted_by_end,
    })
}

/// Choose the best Unicode `cmap` subtable, returning its absolute offset and
/// format. Prefers a full-repertoire format-12 `(3,10)`/`(0,*)` table, then a
/// BMP format-4 `(3,1)`/`(0,*)` table.
fn select_cmap(d: &[u8], cmap: usize) -> Option<(usize, u16)> {
    let num = be_u16(d, off(cmap, 2)?)? as usize;
    let mut best: Option<(usize, u16, u8)> = None; // (offset, format, rank)
    for i in 0..num {
        let rec = off_mul(off(cmap, 4)?, i, 8)?;
        let platform = be_u16(d, rec)?;
        let encoding = be_u16(d, off(rec, 2)?)?;
        let sub = off(cmap, be_u32(d, off(rec, 4)?)? as usize)?;
        let format = be_u16(d, sub)?;
        let unicode = matches!((platform, encoding), (0, _) | (3, 1) | (3, 10));
        if !unicode {
            continue;
        }
        let rank = match format {
            12 => 3,
            4 => {
                if (platform, encoding) == (3, 1) || platform == 0 {
                    2
                } else {
                    1
                }
            }
            _ => continue,
        };
        if best.is_none_or(|(_, _, r)| rank > r) {
            best = Some((sub, format, rank));
        }
    }
    best.map(|(off, fmt, _)| (off, fmt))
}

// ===========================================================================
// OpenType GPOS pair-kerning parser (clean-room). Corrected version.
//
// Reuses existing module helpers be_u16/be_i16/be_u32/find_table_full.
// No unsafe, no unwrap/expect/panic; every read AND every allocation is
// bounds-checked against the font data.
// ===========================================================================

/// A class-definition table (`ClassDef`), used by Pair Adjustment format 2.
#[derive(Clone, Debug)]
enum ClassDef {
    /// `startGlyphID` + dense `classValueArray`.
    Format1 { start: u16, classes: Vec<u16> },
    /// Sorted `(startGlyphID, endGlyphID, class)` ranges.
    Format2 { ranges: Vec<(u16, u16, u16)> },
}

impl ClassDef {
    /// Class of `g`; glyphs not covered by any entry are class 0.
    fn class(&self, g: u16) -> u16 {
        match self {
            ClassDef::Format1 { start, classes } => {
                if g >= *start {
                    let i = (g - *start) as usize;
                    if i < classes.len() {
                        return classes[i];
                    }
                }
                0
            }
            ClassDef::Format2 { ranges } => {
                for &(s, e, c) in ranges {
                    if g >= s && g <= e {
                        return c;
                    }
                }
                0
            }
        }
    }
}

/// One parsed Pair Adjustment subtable (`lookupType` 2), reduced to the
/// `xAdvance` of `valueRecord1` (the only field we apply).
#[derive(Clone, Debug)]
enum KernSubtable {
    /// Specific-pair kerning: `(leftGlyph, rightGlyph) -> xAdvance`.
    Format1 {
        pairs: std::collections::BTreeMap<(u16, u16), i16>,
    },
    /// Class-based kerning.
    Format2 {
        /// First-glyph coverage, sorted ascending for `binary_search`.
        coverage: Vec<u16>,
        class1: ClassDef,
        class2: ClassDef,
        /// Declared matrix dimensions; needed to reject out-of-range class
        /// values that would otherwise index a wrong matrix cell.
        class1_count: u16,
        class2_count: u16,
        /// Row-major `xAdvance` matrix: `matrix[c1 * class2_count + c2]`.
        /// Empty iff both value formats are empty (all adjustments are 0).
        matrix: Vec<i16>,
    },
}

impl KernSubtable {
    /// Returns `Some(xAdvance)` if this subtable defines `(left, right)`.
    ///
    /// For format 2 a `Some(0)` is returned when `left` is covered but the
    /// resolved record is zero or the classes fall outside the declared
    /// dimensions — that still counts as a defined (first) match.
    fn lookup(&self, left: u16, right: u16) -> Option<i16> {
        match self {
            KernSubtable::Format1 { pairs } => pairs.get(&(left, right)).copied(),
            KernSubtable::Format2 {
                coverage,
                class1,
                class2,
                class1_count,
                class2_count,
                matrix,
            } => {
                // Format 2 only applies when `left` is in coverage.
                if coverage.binary_search(&left).is_err() {
                    return None;
                }
                let c1 = class1.class(left) as usize;
                let c2 = class2.class(right) as usize;
                let c1_count = *class1_count as usize;
                let c2_count = *class2_count as usize;
                // Out-of-range class values must NOT wrap into another row.
                if c1 >= c1_count || c2 >= c2_count {
                    return Some(0);
                }
                // Zero-length value records => every adjustment is 0.
                if matrix.is_empty() {
                    return Some(0);
                }
                let idx = c1.checked_mul(c2_count)?.checked_add(c2)?;
                // idx is guaranteed < matrix.len() given the bounds above, but
                // fall back to 0 defensively rather than ever returning None.
                Some(matrix.get(idx).copied().unwrap_or(0))
            }
        }
    }
}

/// Parsed GPOS `kern`-feature pair positioning for a font.
///
/// Built once via [`Font::gpos_kerning`]; [`Kerning::pair`] is a cheap,
/// allocation-free lookup. An empty `Kerning` (no GPOS / no kern feature /
/// malformed) makes every `pair()` return 0.
#[derive(Clone, Debug, Default)]
pub struct Kerning {
    subtables: Vec<KernSubtable>,
}

impl Kerning {
    /// x-advance adjustment (font design units) applied between `left` and
    /// `right` glyph ids; 0 if no kern pair applies. First matching subtable
    /// wins.
    #[must_use]
    pub fn pair(&self, left: u16, right: u16) -> i16 {
        for st in &self.subtables {
            if let Some(v) = st.lookup(left, right) {
                return v;
            }
        }
        0
    }
}

/// ValueRecord byte size = popcount(valueFormat) * 2.
fn value_record_size(value_format: u16) -> usize {
    value_format.count_ones() as usize * 2
}

/// Reads the `xAdvance` (0x0004) i16 of a ValueRecord starting at `off`.
///
/// Returns `Some(0)` when X_ADVANCE is not present, `None` only when the bytes
/// are missing. The field offset within the record is `2 * popcount(vf & 0x0003)`
/// (skip X/Y placement if set).
fn value_record_x_advance(d: &[u8], off: usize, value_format: u16) -> Option<i16> {
    const X_ADVANCE: u16 = 0x0004;
    if value_format & X_ADVANCE == 0 {
        return Some(0);
    }
    let skip = (value_format & 0x0003).count_ones() as usize * 2;
    be_i16(d, off.checked_add(skip)?)
}

/// Parses a Coverage table at `cov`, returning glyph ids ordered by coverage
/// index (index `i` -> returned vec position `i`).
fn parse_coverage_glyphs(d: &[u8], cov: usize) -> Option<Vec<u16>> {
    let format = be_u16(d, cov)?;
    match format {
        1 => {
            let count = be_u16_at(d, cov, 2)? as usize;
            let mut v = Vec::with_capacity(count.min(d.len() / 2 + 1));
            for i in 0..count {
                v.push(be_u16(d, off_mul(off(cov, 4)?, i, 2)?)?);
            }
            Some(v)
        }
        2 => {
            let range_count = be_u16_at(d, cov, 2)? as usize;
            // Key by coverage index so the result is correctly ordered even if
            // ranges are listed out of order.
            let mut by_index: std::collections::BTreeMap<u32, u16> =
                std::collections::BTreeMap::new();
            // A well-formed Coverage cannot enumerate more glyphs than exist in a
            // font (<= 65536). Each 6-byte RangeRecord can otherwise claim up to
            // 65536 ids, so without a cap a small malicious table drives billions
            // of iterations (a CPU-hang DoS on an untrusted host font). Cap the
            // total span and bail before expanding an over-claiming table.
            let mut total: usize = 0;
            for i in 0..range_count {
                let rec = off_mul(off(cov, 4)?, i, 6)?;
                let start = be_u16(d, rec)? as u32;
                let end = be_u16_at(d, rec, 2)? as u32;
                let start_idx = be_u16_at(d, rec, 4)? as u32;
                if end < start {
                    continue;
                }
                total = total.checked_add((end - start + 1) as usize)?;
                if total > MAX_COVERAGE_GLYPHS {
                    return None;
                }
                let mut g = start;
                let mut idx = start_idx;
                while g <= end {
                    by_index.insert(idx, g as u16);
                    g += 1;
                    idx += 1;
                }
            }
            Some(by_index.into_values().collect())
        }
        _ => None,
    }
}

/// Parses a ClassDef table at `cd`.
fn parse_class_def(d: &[u8], cd: usize) -> Option<ClassDef> {
    let format = be_u16(d, cd)?;
    match format {
        1 => {
            let start = be_u16_at(d, cd, 2)?;
            let count = be_u16_at(d, cd, 4)? as usize;
            let mut classes = Vec::with_capacity(count.min(d.len() / 2 + 1));
            for i in 0..count {
                classes.push(be_u16(d, off_mul(off(cd, 6)?, i, 2)?)?);
            }
            Some(ClassDef::Format1 { start, classes })
        }
        2 => {
            let range_count = be_u16_at(d, cd, 2)? as usize;
            let mut ranges = Vec::with_capacity(range_count.min(d.len() / 6 + 1));
            for i in 0..range_count {
                let rec = off_mul(off(cd, 4)?, i, 6)?;
                let s = be_u16(d, rec)?;
                let e = be_u16_at(d, rec, 2)?;
                let c = be_u16_at(d, rec, 4)?;
                ranges.push((s, e, c));
            }
            Some(ClassDef::Format2 { ranges })
        }
        _ => None,
    }
}

/// Parses a Pair Adjustment subtable (`lookupType` 2) whose start is `sub`.
fn parse_pair_subtable(d: &[u8], sub: usize) -> Option<KernSubtable> {
    let pos_format = be_u16(d, sub)?;
    match pos_format {
        1 => parse_pair_format1(d, sub),
        2 => parse_pair_format2(d, sub),
        _ => None,
    }
}

/// Pair Adjustment format 1 (specific pairs).
fn parse_pair_format1(d: &[u8], sub: usize) -> Option<KernSubtable> {
    let cov_off = be_u16_at(d, sub, 2)? as usize;
    let vf1 = be_u16_at(d, sub, 4)?;
    let vf2 = be_u16_at(d, sub, 6)?;
    let pair_set_count = be_u16_at(d, sub, 8)? as usize;

    let rec1_size = value_record_size(vf1);
    let rec2_size = value_record_size(vf2);
    // Each PairValueRecord: secondGlyph(2) + valueRecord1 + valueRecord2.
    let pair_rec_size = off(2, off(rec1_size, rec2_size)?)?;

    let coverage = parse_coverage_glyphs(d, off(sub, cov_off)?)?;

    let mut pairs: std::collections::BTreeMap<(u16, u16), i16> = std::collections::BTreeMap::new();

    // Bound total work: PairSet offsets may all alias one target, so a font of
    // O(pair_set_count + pair_value_count) bytes can otherwise drive their product
    // in iterations — a CPU-hang DoS on an untrusted host font.
    let mut work: usize = 0;
    for i in 0..pair_set_count {
        work += 1;
        if work > MAX_LAYOUT_GLYPHS {
            break;
        }
        // PairSet for coverage-index i is for coverage glyph at position i.
        let Some(left_glyph) = coverage.get(i).copied() else {
            continue;
        };
        let Some(ps_off) = off_mul(off(sub, 10)?, i, 2).and_then(|slot| be_u16(d, slot)) else {
            continue;
        };
        let Some(ps) = off(sub, ps_off as usize) else {
            continue;
        };
        let Some(pair_value_count) = be_u16(d, ps) else {
            continue;
        };
        let Some(mut p) = off(ps, 2) else {
            continue;
        };
        for _ in 0..pair_value_count {
            work += 1;
            if work > MAX_LAYOUT_GLYPHS {
                break;
            }
            let Some(second) = be_u16(d, p) else {
                break;
            };
            let x_adv = off(p, 2)
                .and_then(|value_off| value_record_x_advance(d, value_off, vf1))
                .unwrap_or(0);
            // First subtable / first record wins for a given pair.
            pairs.entry((left_glyph, second)).or_insert(x_adv);
            let Some(np) = p.checked_add(pair_rec_size) else {
                break;
            };
            p = np;
        }
    }

    Some(KernSubtable::Format1 { pairs })
}

/// Pair Adjustment format 2 (class-based).
fn parse_pair_format2(d: &[u8], sub: usize) -> Option<KernSubtable> {
    let cov_off = be_u16_at(d, sub, 2)? as usize;
    let vf1 = be_u16_at(d, sub, 4)?;
    let vf2 = be_u16_at(d, sub, 6)?;
    let class_def1_off = be_u16_at(d, sub, 8)? as usize;
    let class_def2_off = be_u16_at(d, sub, 10)? as usize;
    let class1_count = be_u16_at(d, sub, 12)? as usize;
    let class2_count = be_u16_at(d, sub, 14)? as usize;

    let rec1_size = value_record_size(vf1);
    let rec2_size = value_record_size(vf2);
    let class_rec_size = off(rec1_size, rec2_size)?;

    // Class1Record[]: each holds class2_count Class2Records (record[c1][c2]).
    let matrix_base = off(sub, 16)?;
    let cell_count = class1_count.checked_mul(class2_count)?;

    // Never allocate/iterate based on untrusted class counts unless the
    // declared matrix actually fits within the font data.
    let matrix: Vec<i16> = if class_rec_size == 0 {
        // Both value formats empty => every xAdvance is 0; store nothing.
        Vec::new()
    } else {
        let needed = cell_count.checked_mul(class_rec_size)?;
        let end = matrix_base.checked_add(needed)?;
        if end > d.len() {
            // Matrix cannot fit -> malformed; drop this subtable.
            return None;
        }
        let mut m = Vec::with_capacity(cell_count);
        for idx in 0..cell_count {
            let cell = off_mul(matrix_base, idx, class_rec_size)?;
            // In-bounds by the check above; reads only xAdvance of record1.
            let x_adv = value_record_x_advance(d, cell, vf1).unwrap_or(0);
            m.push(x_adv);
        }
        m
    };

    let mut coverage = parse_coverage_glyphs(d, off(sub, cov_off)?)?;
    coverage.sort_unstable();

    let class1 = parse_class_def(d, off(sub, class_def1_off)?)?;
    let class2 = parse_class_def(d, off(sub, class_def2_off)?)?;

    Some(KernSubtable::Format2 {
        coverage,
        class1,
        class2,
        class1_count: class1_count as u16,
        class2_count: class2_count as u16,
        matrix,
    })
}

/// Resolves an Extension Positioning subtable (`lookupType` 9), returning
/// `(extensionLookupType, realSubtableOffset)`.
fn resolve_extension(d: &[u8], sub: usize) -> Option<(u16, usize)> {
    let pos_format = be_u16(d, sub)?;
    if pos_format != 1 {
        return None;
    }
    let ext_type = be_u16_at(d, sub, 2)?;
    let ext_off = be_u32_at(d, sub, 4)? as usize;
    Some((ext_type, sub.checked_add(ext_off)?))
}

impl Font {
    /// Parses the GPOS `kern` feature once into a [`Kerning`] structure.
    ///
    /// Returns an empty `Kerning` (every `pair()` -> 0) when the font has no
    /// GPOS table, no `kern` feature, or the relevant offsets are malformed.
    #[must_use]
    pub fn gpos_kerning(&self) -> Kerning {
        self.parse_gpos_kerning().unwrap_or_default()
    }

    fn parse_gpos_kerning(&self) -> Option<Kerning> {
        let d = &self.data;
        let (gpos, _gpos_len) = find_table_full(d, b"GPOS")?;

        // GPOS header: major(0) minor(2) scriptList(4) featureList(6) lookupList(8).
        let feature_list_off = be_u16_at(d, gpos, 6)? as usize;
        let lookup_list_off = be_u16_at(d, gpos, 8)? as usize;
        let feature_list = off(gpos, feature_list_off)?;
        let lookup_list = off(gpos, lookup_list_off)?;

        // --- Collect every 'kern' feature's lookup indices (deduplicated). ---
        let feature_count = be_u16(d, feature_list)? as usize;
        let mut lookup_indices: Vec<u16> = Vec::new();
        for i in 0..feature_count {
            // FeatureRecord: tag[4] + featureOffset(2), from FeatureList.
            let rec = off_mul(off(feature_list, 2)?, i, 6)?;
            let Some(tag) = bytes_at(d, rec, 4) else {
                break;
            };
            if tag != b"kern" {
                continue;
            }
            let Some(feat_off) = be_u16_at(d, rec, 4) else {
                continue;
            };
            let Some(feat) = off(feature_list, feat_off as usize) else {
                continue;
            };
            // Feature: featureParams(0) lookupIndexCount(2) lookupIndices(4..).
            let Some(lookup_index_count) = be_u16_at(d, feat, 2) else {
                continue;
            };
            for j in 0..lookup_index_count as usize {
                if let Some(idx) = off_mul(off(feat, 4)?, j, 2).and_then(|slot| be_u16(d, slot)) {
                    if !lookup_indices.contains(&idx) {
                        lookup_indices.push(idx);
                    }
                }
            }
        }

        // --- Walk the gathered lookups, collecting pair subtables. ---
        let lookup_count = be_u16(d, lookup_list)? as usize;
        let mut subtables: Vec<KernSubtable> = Vec::new();

        for &li in &lookup_indices {
            let li = li as usize;
            if li >= lookup_count {
                continue;
            }
            let Some(lookup_off) =
                off_mul(off(lookup_list, 2)?, li, 2).and_then(|slot| be_u16(d, slot))
            else {
                continue;
            };
            let Some(lookup) = off(lookup_list, lookup_off as usize) else {
                continue;
            };
            // Lookup: lookupType(0) lookupFlag(2) subTableCount(4) offsets(6..).
            let Some(lookup_type) = be_u16(d, lookup) else {
                continue;
            };
            let Some(sub_count) = be_u16_at(d, lookup, 4) else {
                continue;
            };

            for s in 0..sub_count as usize {
                let Some(sub_off) = off_mul(off(lookup, 6)?, s, 2).and_then(|slot| be_u16(d, slot))
                else {
                    continue;
                };
                let Some(sub) = off(lookup, sub_off as usize) else {
                    continue;
                };

                match lookup_type {
                    2 => {
                        if let Some(st) = parse_pair_subtable(d, sub) {
                            subtables.push(st);
                        }
                    }
                    9 => {
                        // Extension: resolve, then handle a real type-2 subtable.
                        if let Some((ext_type, real_sub)) = resolve_extension(d, sub) {
                            if ext_type == 2 {
                                if let Some(st) = parse_pair_subtable(d, real_sub) {
                                    subtables.push(st);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Some(Kerning { subtables })
    }
}

// ===========================================================================
// GSUB ligature substitution (vxi.3). Reuses parse_coverage_glyphs +
// resolve_extension + be_u16 + find_table_full. No unsafe/unwrap/panic.
// ===========================================================================

/// One ligature rule: a first glyph (the map key) followed by `components`
/// (the remaining component glyph ids) substitutes to `ligature`.
#[derive(Clone, Debug)]
struct LigRule {
    components: Vec<u16>,
    ligature: u16,
}

/// Parsed GSUB `liga` standard ligatures for a font. Built once via
/// [`Font::gsub_ligatures`]; [`Ligatures::substitute`] applies them.
#[derive(Clone, Debug, Default)]
pub struct Ligatures {
    /// first glyph id -> rules, sorted longest-component-run first.
    rules: std::collections::BTreeMap<u16, Vec<LigRule>>,
}

impl Ligatures {
    /// True when the font defines no standard ligatures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Apply ligature substitution to a glyph-id sequence (greedy longest match),
    /// returning the shaped sequence (which may contain ligature glyph ids that
    /// no single character maps to).
    #[must_use]
    pub fn substitute(&self, gids: &[u16]) -> Vec<u16> {
        self.substitute_with_spans(gids)
            .into_iter()
            .map(|(g, _)| g)
            .collect()
    }

    /// Like [`Ligatures::substitute`] but pairs each output glyph with the number
    /// of input glyphs it consumed (1 for a pass-through, N for an N-component
    /// ligature) — so callers can map a ligature back to its source characters
    /// (e.g. to build a `ToUnicode` entry).
    #[must_use]
    pub fn substitute_with_spans(&self, gids: &[u16]) -> Vec<(u16, usize)> {
        let mut out = Vec::with_capacity(gids.len());
        let mut i = 0;
        while i < gids.len() {
            let mut applied = false;
            if let Some(rules) = self.rules.get(&gids[i]) {
                for r in rules {
                    let n = r.components.len();
                    if i + 1 + n <= gids.len() && gids[i + 1..i + 1 + n] == r.components[..] {
                        out.push((r.ligature, n + 1));
                        i += n + 1;
                        applied = true;
                        break;
                    }
                }
            }
            if !applied {
                out.push((gids[i], 1));
                i += 1;
            }
        }
        out
    }
}

impl Font {
    /// Parse the GSUB `liga` standard-ligature substitutions once.
    ///
    /// Returns empty [`Ligatures`] when the font has no GSUB / no `liga` feature
    /// or the relevant offsets are malformed.
    #[must_use]
    pub fn gsub_ligatures(&self) -> Ligatures {
        self.parse_gsub_ligatures().unwrap_or_default()
    }

    fn parse_gsub_ligatures(&self) -> Option<Ligatures> {
        let d = &self.data;
        let (gsub, _) = find_table_full(d, b"GSUB")?;
        let feature_list = off(gsub, be_u16_at(d, gsub, 6)? as usize)?;
        let lookup_list = off(gsub, be_u16_at(d, gsub, 8)? as usize)?;

        // Collect every 'liga' feature's lookup indices.
        let feature_count = be_u16(d, feature_list)? as usize;
        let mut lookup_indices: Vec<u16> = Vec::new();
        for i in 0..feature_count {
            let rec = off_mul(off(feature_list, 2)?, i, 6)?;
            let Some(tag) = bytes_at(d, rec, 4) else {
                break;
            };
            if tag != b"liga" {
                continue;
            }
            let Some(feat_off) = be_u16_at(d, rec, 4) else {
                continue;
            };
            let Some(feat) = off(feature_list, feat_off as usize) else {
                continue;
            };
            let Some(n) = be_u16_at(d, feat, 2) else {
                continue;
            };
            for j in 0..n as usize {
                if let Some(idx) = off_mul(off(feat, 4)?, j, 2).and_then(|slot| be_u16(d, slot)) {
                    if !lookup_indices.contains(&idx) {
                        lookup_indices.push(idx);
                    }
                }
            }
        }

        let lookup_count = be_u16(d, lookup_list)? as usize;
        let mut rules: std::collections::BTreeMap<u16, Vec<LigRule>> =
            std::collections::BTreeMap::new();
        for &li in &lookup_indices {
            let li = li as usize;
            if li >= lookup_count {
                continue;
            }
            let Some(lookup_off) =
                off_mul(off(lookup_list, 2)?, li, 2).and_then(|slot| be_u16(d, slot))
            else {
                continue;
            };
            let Some(lookup) = off(lookup_list, lookup_off as usize) else {
                continue;
            };
            let Some(lookup_type) = be_u16(d, lookup) else {
                continue;
            };
            let Some(sub_count) = be_u16_at(d, lookup, 4) else {
                continue;
            };
            for s in 0..sub_count as usize {
                let Some(sub_off) = off_mul(off(lookup, 6)?, s, 2).and_then(|slot| be_u16(d, slot))
                else {
                    continue;
                };
                let Some(sub) = off(lookup, sub_off as usize) else {
                    continue;
                };
                match lookup_type {
                    4 => parse_ligature_subst(d, sub, &mut rules),
                    // Extension Substitution -> a real type-4 subtable.
                    7 => {
                        if let Some((ext_type, real)) = resolve_extension(d, sub) {
                            if ext_type == 4 {
                                parse_ligature_subst(d, real, &mut rules);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // Greedy longest match: try the longest ligature first.
        for v in rules.values_mut() {
            v.sort_by_key(|r| std::cmp::Reverse(r.components.len()));
        }
        Some(Ligatures { rules })
    }
}

/// Parse one Ligature Substitution subtable (GSUB `lookupType` 4) at `sub`.
fn parse_ligature_subst(
    d: &[u8],
    sub: usize,
    rules: &mut std::collections::BTreeMap<u16, Vec<LigRule>>,
) {
    let Some(format) = be_u16(d, sub) else {
        return;
    };
    if format != 1 {
        return;
    }
    let Some(cov_off) = be_u16_at(d, sub, 2) else {
        return;
    };
    let Some(set_count) = be_u16_at(d, sub, 4) else {
        return;
    };
    let Some(coverage) = off(sub, cov_off as usize).and_then(|cov| parse_coverage_glyphs(d, cov))
    else {
        return;
    };
    let Some(set_offsets) = off(sub, 6) else {
        return;
    };
    // Bound total work: LigatureSet/Ligature offsets may all alias one target, so
    // a font of O(set_count + lig_count) bytes can otherwise drive set_count *
    // lig_count iterations (and retained `LigRule`s) — an OOM-kill DoS. A valid
    // font has far fewer ligature entries than the glyph ceiling.
    let mut work: usize = 0;
    for i in 0..set_count as usize {
        work += 1;
        if work > MAX_LAYOUT_GLYPHS {
            return;
        }
        // LigatureSet i is for coverage glyph i (the ligature's first component).
        let Some(first) = coverage.get(i).copied() else {
            continue;
        };
        let Some(set_off) = off_mul(set_offsets, i, 2).and_then(|slot| be_u16(d, slot)) else {
            continue;
        };
        let Some(lig_set) = off(sub, set_off as usize) else {
            continue;
        };
        let Some(lig_count) = be_u16(d, lig_set) else {
            continue;
        };
        let Some(lig_offsets) = off(lig_set, 2) else {
            continue;
        };
        for j in 0..lig_count as usize {
            work += 1;
            if work > MAX_LAYOUT_GLYPHS {
                return;
            }
            let Some(lig_off) = off_mul(lig_offsets, j, 2).and_then(|slot| be_u16(d, slot)) else {
                continue;
            };
            let Some(lig) = off(lig_set, lig_off as usize) else {
                continue;
            };
            let Some(lig_glyph) = be_u16(d, lig) else {
                continue;
            };
            let Some(comp_count) = be_u16_at(d, lig, 2) else {
                continue;
            };
            if comp_count == 0 {
                continue;
            }
            // componentGlyphIDs holds comp_count-1 entries (the first is `first`).
            let mut components = Vec::with_capacity(comp_count as usize - 1);
            let mut ok = true;
            let Some(component_base) = off(lig, 4) else {
                continue;
            };
            for k in 0..(comp_count as usize - 1) {
                match off_mul(component_base, k, 2).and_then(|slot| be_u16(d, slot)) {
                    Some(g) => components.push(g),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                rules.entry(first).or_default().push(LigRule {
                    components,
                    ligature: lig_glyph,
                });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod dos_tests {
    use super::{MAX_COVERAGE_GLYPHS, parse_coverage_glyphs};

    fn be(v: u16) -> [u8; 2] {
        v.to_be_bytes()
    }

    #[test]
    fn coverage_format2_valid_range_expands() {
        // format=2, rangeCount=1, range [10..=20] at coverage index 0.
        let mut d = Vec::new();
        d.extend_from_slice(&be(2));
        d.extend_from_slice(&be(1));
        d.extend_from_slice(&be(10)); // start
        d.extend_from_slice(&be(20)); // end
        d.extend_from_slice(&be(0)); // startCoverageIndex
        let got = parse_coverage_glyphs(&d, 0).unwrap();
        assert_eq!(got, (10u16..=20).collect::<Vec<_>>());
    }

    #[test]
    fn coverage_format2_overclaiming_table_is_rejected_not_expanded() {
        // Two ranges each spanning 0..=65535 => total 131072 > the glyph ceiling,
        // so the parser must bail (None) instead of grinding billions of inserts.
        let mut d = Vec::new();
        d.extend_from_slice(&be(2));
        d.extend_from_slice(&be(2)); // rangeCount = 2
        for _ in 0..2 {
            d.extend_from_slice(&be(0)); // start
            d.extend_from_slice(&be(0xFFFF)); // end
            d.extend_from_slice(&be(0)); // startCoverageIndex
        }
        assert!(parse_coverage_glyphs(&d, 0).is_none());
        // Sanity: the ceiling is the font-wide glyph limit.
        assert_eq!(MAX_COVERAGE_GLYPHS, 65_536);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod subset_degradation_tests {
    use super::{Font, MISSING_GLYPH_REMAP, be_i16, be_u16, strip_simple_glyph_instructions};

    fn cm_regular() -> Font {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fonts/computer-modern/cmunrm.ttf"
        ))
        .expect("read bundled font");
        Font::parse(bytes).expect("parse bundled font")
    }

    fn all_faces() -> Vec<Font> {
        let base = env!("CARGO_MANIFEST_DIR");
        [
            "/fonts/computer-modern/cmunrm.ttf",
            "/fonts/computer-modern/cmunbx.ttf",
            "/fonts/computer-modern/cmunti.ttf",
            "/fonts/computer-modern/cmunbi.ttf",
            "/fonts/computer-modern/cmuntt.ttf",
            "/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf",
            "/fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf",
            "/fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf",
            "/fonts/ibm-plex-sans/IBMPlexSans-BoldItalic.ttf",
        ]
        .iter()
        .filter_map(|p| Font::parse(std::fs::read(format!("{base}{p}")).ok()?).ok())
        .collect()
    }

    fn test_remap(font: &Font, pairs: &[(u16, u16)]) -> Vec<u16> {
        let mut new_of = vec![MISSING_GLYPH_REMAP; usize::from(font.num_glyphs).max(1)];
        for &(old, new) in pairs {
            if let Some(slot) = new_of.get_mut(usize::from(old)) {
                *slot = new;
            }
        }
        new_of
    }

    fn simple_instruction_len(data: &[u8]) -> Option<usize> {
        let contours = be_i16(data, 0)?;
        if contours < 0 {
            return None;
        }
        let instruction_len_offset = 10usize.checked_add((contours as usize).checked_mul(2)?)?;
        be_u16(data, instruction_len_offset).map(usize::from)
    }

    #[test]
    fn simple_glyph_instruction_stripper_zeroes_length_and_removes_bytes() {
        let mut glyph = Vec::new();
        glyph.extend_from_slice(&1i16.to_be_bytes()); // one contour
        glyph.extend_from_slice(&[0u8; 8]); // bbox
        glyph.extend_from_slice(&0u16.to_be_bytes()); // endPtsOfContours[0]
        glyph.extend_from_slice(&3u16.to_be_bytes()); // instructionLength
        glyph.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // instructions
        glyph.extend_from_slice(&[0x11, 0x22, 0x33]); // flag/coordinate payload

        let stripped = strip_simple_glyph_instructions(&glyph, 1).expect("valid simple glyph");
        assert_eq!(simple_instruction_len(&stripped), Some(0));
        assert_eq!(stripped.len(), glyph.len() - 3);
        assert_eq!(&stripped[stripped.len() - 3..], &[0x11, 0x22, 0x33]);
    }

    #[test]
    fn subset_glyph_bytes_strips_simple_instructions_when_present() {
        let Some((font, gid, original_len)) = all_faces().into_iter().find_map(|font| {
            (1..font.num_glyphs).find_map(|gid| {
                let data = font.glyph_data(gid)?;
                let len = simple_instruction_len(data)?;
                (len > 0).then_some((font.clone(), gid, len))
            })
        }) else {
            eprintln!("skipping: bundled fonts have no hinted simple glyphs");
            return;
        };

        let new_of = test_remap(&font, &[(0u16, 0u16), (gid, 1u16)]);

        let stripped = font
            .subset_glyph_bytes(gid, &new_of)
            .expect("hinted simple glyph should subset");
        assert_eq!(simple_instruction_len(&stripped), Some(0));
        assert_eq!(
            stripped.len(),
            font.glyph_data(gid).expect("original glyph").len() - original_len
        );
    }

    #[test]
    fn subset_hmtx_preserves_true_left_side_bearings() {
        let (font, ch, old_gid, old_lsb) = all_faces()
            .into_iter()
            .find_map(|font| {
                (33u8..=126).find_map(|byte| {
                    let ch = char::from(byte);
                    let gid = font.glyph_index(ch);
                    let lsb = font.left_side_bearing(gid);
                    (gid != 0 && lsb != 0).then_some((font.clone(), ch, gid, lsb))
                })
            })
            .expect("at least one bundled printable glyph has a nonzero lsb");

        let (bytes, remap) = font
            .subset_glyphs(&[old_gid], &[ch])
            .expect("subset with nonzero-lsb glyph");
        let subset = Font::parse(bytes).expect("subset re-parses");
        let new_gid = remap[&old_gid];
        assert_eq!(subset.left_side_bearing(new_gid), old_lsb);
    }

    #[test]
    fn subset_skips_cmap_char_whose_glyph_is_absent_from_the_set() {
        // `subset_glyphs` takes the glyph set explicitly but builds the cmap from
        // `cmap_chars`. A cmap char whose glyph is not in the set must be skipped,
        // not abort the whole subset (which would deny an otherwise-usable font).
        let font = cm_regular();
        let g_b = font.glyph_index('B');
        assert_ne!(g_b, 0, "test font must map 'B'");
        // Provide only B's glyph, but ask the cmap to also map 'A' (absent).
        let out = font.subset_glyphs(&[g_b], &['A', 'B']);
        let (bytes, _) = out.expect("un-subsettable cmap char must be skipped, not abort");
        // The produced subset must still be a parseable font.
        assert!(Font::parse(bytes).is_ok());
    }

    #[test]
    fn subset_glyph_bytes_substitutes_notdef_for_a_missing_component() {
        // A composite whose component is not in `new_of` (a malformed out-of-range
        // component gid) must be substituted with `.notdef`, not abort.
        let (font, comp) = all_faces()
            .into_iter()
            .find_map(|f| {
                (1..f.num_glyphs)
                    .find(|&g| f.is_composite(g))
                    .map(|g| (f, g))
            })
            .expect("at least one bundled face has a composite glyph");
        // Map .notdef and the composite itself, but NONE of its components, so the
        // component lookup misses and must fall back to gid 0.
        let new_of = test_remap(&font, &[(0u16, 0u16), (comp, 1u16)]);
        let bytes = font
            .subset_glyph_bytes(comp, &new_of)
            .expect("missing component must be substituted, not abort");
        assert!(!bytes.is_empty(), "a composite glyph is non-empty");
    }
}
