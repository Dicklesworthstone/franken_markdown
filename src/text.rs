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
        if e <= s || be_i16(&self.data, s).is_none_or(|n| n >= 0) {
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
        // Web-embedding path: include `OS/2` (see `subset_core`) so browser
        // OpenType sanitizers (Chromium's OTS) accept the font instead of
        // silently falling back to system fonts.
        self.subset_core(&seed, keep, true).map(|(bytes, _)| bytes)
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
        // PDF font programs do not require `OS/2`; leaving it out keeps the
        // embedded font streams (and existing golden PDF bytes) unchanged.
        self.subset_core(glyphs, cmap_chars, false)
    }

    fn subset_core(
        &self,
        seed_glyphs: &[u16],
        cmap_chars: &[char],
        include_os2: bool,
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

        // OS/2: copied verbatim from the source face when requested and
        // present. Browsers' OpenType sanitizer (Chromium's OTS) rejects web
        // fonts without an `OS/2` table ("OS/2: missing required table"), so
        // the HTML embedding path opts in. The aggregate fields (average
        // width, Unicode ranges, win metrics) remain those of the full face,
        // which is valid if conservative for a subset. A source face without
        // `OS/2` subsets as before and is rejected by OTS either way.
        let os2: Option<Vec<u8>> = if include_os2 {
            find_table_full(&self.data, b"OS/2")
                .and_then(|(o, l)| Some(self.data.get(o..off(o, l)?)?.to_vec()))
        } else {
            None
        };

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
        if let Some(os2) = os2 {
            tables.push((b"OS/2", os2));
        }
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
    use super::{
        Font, MISSING_GLYPH_REMAP, be_i16, be_u16, find_table_full, strip_simple_glyph_instructions,
    };

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
    fn html_subset_carries_verbatim_os2_while_pdf_subset_stays_lean() {
        // Chromium's OpenType sanitizer (OTS) rejects web fonts without an
        // `OS/2` table ("OS/2: missing required table"), silently downgrading
        // HTML previews to system fonts. The HTML path (`subset`) must carry
        // the source table verbatim; the PDF path (`subset_glyphs`) must keep
        // omitting it so embedded font streams and golden PDFs stay identical.
        for font in all_faces() {
            let (src_off, src_len) = find_table_full(&font.data, b"OS/2")
                .expect("every bundled face carries an OS/2 table");
            let src_os2 = font.data[src_off..src_off + src_len].to_vec();

            let html_bytes = font.subset(&['A', 'b']).expect("html subset");
            let html_font = Font::parse(html_bytes).expect("html subset re-parses");
            let (o, l) = find_table_full(&html_font.data, b"OS/2")
                .expect("html subset must keep OS/2 for browser sanitizers");
            assert_eq!(
                &html_font.data[o..o + l],
                &src_os2[..],
                "OS/2 must be copied verbatim"
            );

            let gid = font.glyph_index('A');
            assert_ne!(gid, 0, "bundled faces must map 'A'");
            let (pdf_bytes, _) = font.subset_glyphs(&[gid], &['A']).expect("pdf subset");
            assert!(
                find_table_full(&pdf_bytes, b"OS/2").is_none(),
                "pdf subset must not grow an OS/2 table (golden bytes)"
            );
            assert!(Font::parse(pdf_bytes).is_ok());
        }
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
    fn simple_instruction_len_rejects_composite_data() {
        // The helper reads numberOfContours first; a negative count (composite)
        // has no simple-glyph instruction stream to measure.
        assert_eq!(simple_instruction_len(&(-1i16).to_be_bytes()), None);
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod synthetic_font_tests {
    use super::*;

    // --- byte-level builders ------------------------------------------------

    fn push16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn push_i16(out: &mut Vec<u8>, v: i16) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn push32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    /// Assemble an sfnt file. The directory records each table's real length;
    /// truncation tests chop bytes off the end of the returned file afterwards
    /// (the directory keeps claiming the full length, exactly like a damaged
    /// or malicious font would).
    fn sfnt(magic: u32, tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        push32(&mut out, magic);
        push16(&mut out, u16::try_from(tables.len()).unwrap());
        out.extend_from_slice(&[0u8; 6]); // search fields: unread by the parser
        let mut offset = 12 + tables.len() * 16;
        let mut body = Vec::new();
        for (tag, bytes) in tables {
            out.extend_from_slice(&tag[..]);
            push32(&mut out, 0); // checksum: unread by the parser
            push32(&mut out, u32::try_from(offset).unwrap());
            push32(&mut out, u32::try_from(bytes.len()).unwrap());
            offset += bytes.len();
            body.extend_from_slice(bytes);
        }
        out.extend_from_slice(&body);
        out
    }

    fn head_table(upem: u16, loca_long: bool) -> Vec<u8> {
        let mut t = vec![0u8; 54];
        t[18..20].copy_from_slice(&upem.to_be_bytes());
        t[50..52].copy_from_slice(&u16::from(loca_long).to_be_bytes());
        t
    }

    fn maxp_table(num_glyphs: u16) -> Vec<u8> {
        let mut t = vec![0u8; 6];
        t[4..6].copy_from_slice(&num_glyphs.to_be_bytes());
        t
    }

    fn hhea_table(num_h_metrics: u16) -> Vec<u8> {
        let mut t = vec![0u8; 36];
        t[4..6].copy_from_slice(&700i16.to_be_bytes());
        t[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        t[8..10].copy_from_slice(&50i16.to_be_bytes());
        t[34..36].copy_from_slice(&num_h_metrics.to_be_bytes());
        t
    }

    fn hmtx_long(metrics: &[(u16, i16)]) -> Vec<u8> {
        let mut t = Vec::new();
        for &(aw, lsb) in metrics {
            push16(&mut t, aw);
            push_i16(&mut t, lsb);
        }
        t
    }

    /// A complete `cmap` table holding one format-4 `(3,1)` subtable built from
    /// raw `(endCode, startCode, idDelta, idRangeOffset)` segments, followed by
    /// `glyph_id_array` bytes.
    fn cmap4_table(segs: &[(u16, u16, u16, u16)], glyph_id_array: &[u8]) -> Vec<u8> {
        let seg_count = segs.len();
        let mut t = Vec::new();
        push16(&mut t, 0); // version
        push16(&mut t, 1); // numTables
        push16(&mut t, 3); // platformID (Windows)
        push16(&mut t, 1); // encodingID (Unicode BMP)
        push32(&mut t, 12); // subtable offset
        push16(&mut t, 4); // format
        push16(
            &mut t,
            u16::try_from(16 + seg_count * 8 + glyph_id_array.len()).unwrap(),
        );
        push16(&mut t, 0); // language
        push16(&mut t, u16::try_from(seg_count * 2).unwrap()); // segCountX2
        push16(&mut t, 0); // searchRange (unread)
        push16(&mut t, 0); // entrySelector (unread)
        push16(&mut t, 0); // rangeShift (unread)
        for &(end, _, _, _) in segs {
            push16(&mut t, end);
        }
        push16(&mut t, 0); // reservedPad
        for &(_, start, _, _) in segs {
            push16(&mut t, start);
        }
        for &(_, _, delta, _) in segs {
            push16(&mut t, delta);
        }
        for &(_, _, _, iro) in segs {
            push16(&mut t, iro);
        }
        t.extend_from_slice(glyph_id_array);
        t
    }

    /// A format-4 cmap mapping each ascending `(code, gid)` pair via its own
    /// delta-only segment, plus the mandatory final 0xFFFF segment.
    fn cmap4_simple(map: &[(u16, u16)]) -> Vec<u8> {
        let mut segs: Vec<(u16, u16, u16, u16)> = map
            .iter()
            .map(|&(code, gid)| (code, code, gid.wrapping_sub(code), 0))
            .collect();
        segs.push((0xFFFF, 0xFFFF, 1, 0));
        cmap4_table(&segs, &[])
    }

    /// A complete `cmap` table holding one format-12 `(3,10)` subtable.
    fn cmap12_table(groups: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut t = Vec::new();
        push16(&mut t, 0); // version
        push16(&mut t, 1); // numTables
        push16(&mut t, 3); // platformID (Windows)
        push16(&mut t, 10); // encodingID (Unicode full repertoire)
        push32(&mut t, 12); // subtable offset
        push16(&mut t, 12); // format
        push16(&mut t, 0); // reserved
        push32(&mut t, u32::try_from(16 + groups.len() * 12).unwrap());
        push32(&mut t, 0); // language
        push32(&mut t, u32::try_from(groups.len()).unwrap());
        for &(start, end, gid) in groups {
            push32(&mut t, start);
            push32(&mut t, end);
            push32(&mut t, gid);
        }
        t
    }

    fn base_tables(
        num_glyphs: u16,
        num_h_metrics: u16,
        upem: u16,
        hmtx: Vec<u8>,
        cmap: Vec<u8>,
    ) -> Vec<(&'static [u8; 4], Vec<u8>)> {
        vec![
            (b"head", head_table(upem, false)),
            (b"maxp", maxp_table(num_glyphs)),
            (b"hhea", hhea_table(num_h_metrics)),
            (b"hmtx", hmtx),
            (b"cmap", cmap),
        ]
    }

    fn parse(tables: &[(&[u8; 4], Vec<u8>)]) -> Font {
        Font::parse(sfnt(0x0001_0000, tables)).expect("synthetic font parses")
    }

    // --- glyph builders -------------------------------------------------

    const ARGW: u16 = 0x0001; // ARG_1_AND_2_ARE_WORDS
    const WHS: u16 = 0x0008; // WE_HAVE_A_SCALE
    const MORE: u16 = 0x0020; // MORE_COMPONENTS
    const XYS: u16 = 0x0040; // WE_HAVE_AN_X_AND_Y_SCALE
    const TWO: u16 = 0x0080; // WE_HAVE_A_TWO_BY_TWO
    const INSTR: u16 = 0x0100; // WE_HAVE_INSTRUCTIONS

    /// A composite glyph: each record is `(flags, component gid, arg/scale
    /// payload)`; `trailer` bytes follow the last record.
    fn composite_glyph(bbox: [i16; 4], records: &[(u16, u16, &[u8])], trailer: &[u8]) -> Vec<u8> {
        let mut g = Vec::new();
        push_i16(&mut g, -1);
        for v in bbox {
            push_i16(&mut g, v);
        }
        for &(flags, gid, payload) in records {
            push16(&mut g, flags);
            push16(&mut g, gid);
            g.extend_from_slice(payload);
        }
        g.extend_from_slice(trailer);
        g
    }

    /// A minimal valid hint-free simple glyph (16 bytes).
    fn simple_glyph16() -> Vec<u8> {
        let mut g = Vec::new();
        push_i16(&mut g, 1); // numberOfContours
        g.extend_from_slice(&[0u8; 8]); // bbox
        push16(&mut g, 0); // endPtsOfContours[0]
        push16(&mut g, 0); // instructionLength
        g.extend_from_slice(&[0x01, 0x00]); // flag + coordinate payload
        g
    }

    /// Glyph zoo: 0 empty, 1 bare simple stub, 2/3/4 composites using the three
    /// transform payload sizes, 5 valid simple, 6 word-args + MORE chain,
    /// 7 MORE record ending exactly at the glyph end, 8 overlong instruction
    /// claim, 9 component gid past numGlyphs, 10 trailing junk after the last
    /// record, 11 transform payload overrunning the glyph, 12 valid composite
    /// instructions.
    fn zoo_font() -> Font {
        let glyphs: Vec<Vec<u8>> = vec![
            Vec::new(),
            1i16.to_be_bytes().to_vec(),
            composite_glyph([1, 2, 3, 4], &[(WHS, 5, &[0, 0, 0x40, 0])], &[]),
            composite_glyph([0; 4], &[(XYS, 5, &[0, 0, 0x40, 0, 0x40, 0])], &[]),
            composite_glyph(
                [0; 4],
                &[(TWO, 5, &[0, 0, 0x40, 0, 0, 0, 0, 0, 0x40, 0])],
                &[],
            ),
            simple_glyph16(),
            composite_glyph(
                [0; 4],
                &[(ARGW | MORE, 2, &[0, 0, 0, 0]), (0, 3, &[0, 0])],
                &[],
            ),
            composite_glyph([0; 4], &[(MORE, 5, &[0, 0])], &[]),
            composite_glyph([0; 4], &[(INSTR, 5, &[0, 0])], &[0xFF, 0xFF]),
            composite_glyph([0; 4], &[(0, 900, &[0, 0])], &[]),
            composite_glyph([0; 4], &[(MORE, 5, &[0, 0])], &[0, 0]),
            composite_glyph([0; 4], &[(TWO, 5, &[0, 0, 0x40, 0])], &[]),
            composite_glyph([0; 4], &[(INSTR, 5, &[0, 0])], &[0x00, 0x02, 0xAA, 0xBB]),
        ];
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        push16(&mut loca, 0);
        for g in &glyphs {
            glyf.extend_from_slice(g);
            push16(&mut loca, u16::try_from(glyf.len() / 2).unwrap());
        }
        let metrics: Vec<(u16, i16)> = (0..13u16).map(|g| (500 + g, g as i16)).collect();
        let mut tables = base_tables(13, 13, 1000, hmtx_long(&metrics), cmap4_simple(&[]));
        tables.push((b"loca", loca));
        tables.push((b"glyf", glyf));
        parse(&tables)
    }

    /// One composite glyph whose loca/glyf directory claims 16 bytes while the
    /// file physically ends after `keep` of them.
    fn truncated_composite_font(keep: usize) -> Font {
        let glyph = composite_glyph([0; 4], &[(0, 5, &[0, 0])], &[]);
        assert_eq!(glyph.len(), 16);
        let mut loca = Vec::new();
        push16(&mut loca, 0);
        push16(&mut loca, 8);
        let mut tables = base_tables(1, 1, 1000, hmtx_long(&[(500, 0)]), cmap4_simple(&[]));
        tables.push((b"loca", loca));
        tables.push((b"glyf", glyph));
        let mut bytes = sfnt(0x0001_0000, &tables);
        bytes.truncate(bytes.len() - (16 - keep));
        Font::parse(bytes).expect("glyf payload is lazily read")
    }

    /// A `kern` table with one version-0 format-0 horizontal subtable.
    fn kern0_table(pairs: &[(u16, u16, i16)]) -> Vec<u8> {
        let mut t = Vec::new();
        push16(&mut t, 0); // version
        push16(&mut t, 1); // nTables
        push16(&mut t, 0); // subtable version
        push16(&mut t, u16::try_from(14 + pairs.len() * 6).unwrap()); // length
        push16(&mut t, 0x0001); // coverage: horizontal, format 0
        push16(&mut t, u16::try_from(pairs.len()).unwrap()); // nPairs
        t.extend_from_slice(&[0u8; 6]); // search fields (unread)
        for &(l, r, v) in pairs {
            push16(&mut t, l);
            push16(&mut t, r);
            push_i16(&mut t, v);
        }
        t
    }

    /// Raw GPOS table: a `kern` feature routing through an Extension (type 9)
    /// lookup to a Pair Adjustment format-1 subtable holding (5, 6) -> -40.
    fn gpos_table(ext_format: u16, ext_type: u16, lookup_index: u16, pos_format: u16) -> Vec<u8> {
        let mut g = Vec::new();
        push32(&mut g, 0x0001_0000); // version
        push16(&mut g, 0); // scriptList (unread)
        push16(&mut g, 10); // featureList
        push16(&mut g, 24); // lookupList
        // FeatureList @10
        push16(&mut g, 1); // featureCount
        g.extend_from_slice(b"kern");
        push16(&mut g, 8); // feature @ featureList+8
        // Feature @18
        push16(&mut g, 0); // featureParams
        push16(&mut g, 1); // lookupIndexCount
        push16(&mut g, lookup_index);
        // LookupList @24
        push16(&mut g, 1); // lookupCount
        push16(&mut g, 4); // lookup @ lookupList+4
        // Lookup @28: Extension Positioning
        push16(&mut g, 9); // lookupType
        push16(&mut g, 0); // lookupFlag
        push16(&mut g, 1); // subTableCount
        push16(&mut g, 8); // subtable @ lookup+8
        // Extension @36
        push16(&mut g, ext_format);
        push16(&mut g, ext_type);
        push32(&mut g, 8); // wrapped subtable @ 36+8
        // PairPos @44
        push16(&mut g, pos_format);
        push16(&mut g, 18); // coverage @ 44+18
        push16(&mut g, 0x0004); // valueFormat1: X_ADVANCE
        push16(&mut g, 0); // valueFormat2
        push16(&mut g, 1); // pairSetCount
        push16(&mut g, 12); // pair set @ 44+12
        // PairSet @56
        push16(&mut g, 1); // pairValueCount
        push16(&mut g, 6); // secondGlyph
        push_i16(&mut g, -40); // xAdvance
        // Coverage @62
        push16(&mut g, 1);
        push16(&mut g, 1);
        push16(&mut g, 5);
        assert_eq!(g.len(), 68);
        g
    }

    /// Attach raw GPOS bytes (as the last table, so shortened tables truncate
    /// the file) and parse its kerning.
    fn gpos_kerning_of(table: Vec<u8>) -> Kerning {
        let mut tables = base_tables(
            2,
            2,
            1000,
            hmtx_long(&[(600, 0), (600, 0)]),
            cmap4_simple(&[]),
        );
        tables.push((b"GPOS", table));
        parse(&tables).gpos_kerning()
    }

    fn gpos_font(ext_format: u16, ext_type: u16, lookup_index: u16, pos_format: u16) -> Kerning {
        gpos_kerning_of(gpos_table(ext_format, ext_type, lookup_index, pos_format))
    }

    /// Raw GSUB table: a `liga` feature routing through an Extension (type 7)
    /// lookup to a LigatureSubst with (10,11,12)->99 and (10,11)->77.
    fn gsub_table(ext_format: u16, ext_type: u16, lookup_index: u16) -> Vec<u8> {
        let mut g = Vec::new();
        push32(&mut g, 0x0001_0000);
        push16(&mut g, 0); // scriptList (unread)
        push16(&mut g, 10); // featureList
        push16(&mut g, 24); // lookupList
        // FeatureList @10
        push16(&mut g, 1);
        g.extend_from_slice(b"liga");
        push16(&mut g, 8); // feature @18
        // Feature @18
        push16(&mut g, 0);
        push16(&mut g, 1);
        push16(&mut g, lookup_index);
        // LookupList @24
        push16(&mut g, 1);
        push16(&mut g, 4); // lookup @28
        // Lookup @28: Extension Substitution
        push16(&mut g, 7);
        push16(&mut g, 0);
        push16(&mut g, 1);
        push16(&mut g, 8); // subtable @36
        // Extension @36
        push16(&mut g, ext_format);
        push16(&mut g, ext_type);
        push32(&mut g, 8); // wrapped subtable @44
        // LigatureSubst @44
        push16(&mut g, 1); // substFormat
        push16(&mut g, 28); // coverage @ 44+28
        push16(&mut g, 1); // ligSetCount
        push16(&mut g, 8); // ligature set @ 44+8
        // LigatureSet @52
        push16(&mut g, 2); // ligatureCount
        push16(&mut g, 6); // ligature @ 52+6
        push16(&mut g, 14); // ligature @ 52+14
        // Ligature @58: components (10, 11, 12) -> 99
        push16(&mut g, 99);
        push16(&mut g, 3);
        push16(&mut g, 11);
        push16(&mut g, 12);
        // Ligature @66: components (10, 11) -> 77
        push16(&mut g, 77);
        push16(&mut g, 2);
        push16(&mut g, 11);
        // Coverage @72
        push16(&mut g, 1);
        push16(&mut g, 1);
        push16(&mut g, 10);
        assert_eq!(g.len(), 78);
        g
    }

    /// Attach raw GSUB bytes (as the last table) and parse its ligatures.
    fn gsub_ligatures_of(table: Vec<u8>) -> Ligatures {
        let mut tables = base_tables(
            2,
            2,
            1000,
            hmtx_long(&[(600, 0), (600, 0)]),
            cmap4_simple(&[]),
        );
        tables.push((b"GSUB", table));
        parse(&tables).gsub_ligatures()
    }

    fn gsub_font(ext_format: u16, ext_type: u16, lookup_index: u16) -> Ligatures {
        gsub_ligatures_of(gsub_table(ext_format, ext_type, lookup_index))
    }

    // --- parse errors and magics ---------------------------------------

    #[test]
    fn parse_error_variants_and_display_messages() {
        assert_eq!(Font::parse(Vec::new()).err(), Some(FontError::Truncated));
        assert_eq!(
            Font::parse(vec![0x00, 0x02, 0x00, 0x00]).err(),
            Some(FontError::BadMagic)
        );

        // Required tables are demanded in a fixed order.
        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        let steps: [(&'static [u8; 4], &'static str, Vec<u8>); 4] = [
            (b"head", "head", head_table(1000, false)),
            (b"maxp", "maxp", maxp_table(1)),
            (b"hhea", "hhea", hhea_table(1)),
            (b"hmtx", "hmtx", hmtx_long(&[(500, 0)])),
        ];
        for (tag, name, table) in steps {
            assert_eq!(
                Font::parse(sfnt(0x0001_0000, &tables)).err(),
                Some(FontError::MissingTable(name))
            );
            tables.push((tag, table));
        }
        assert_eq!(
            Font::parse(sfnt(0x0001_0000, &tables)).err(),
            Some(FontError::MissingTable("cmap"))
        );

        // A cmap with only a Mac record and an unsupported-format Windows
        // record has no usable Unicode subtable.
        let mut bad_cmap = Vec::new();
        push16(&mut bad_cmap, 0);
        push16(&mut bad_cmap, 2);
        push16(&mut bad_cmap, 1); // platform 1 (Macintosh): not Unicode
        push16(&mut bad_cmap, 0);
        push32(&mut bad_cmap, 20);
        push16(&mut bad_cmap, 3); // (3,1) but pointing at a format-6 subtable
        push16(&mut bad_cmap, 1);
        push32(&mut bad_cmap, 20);
        push16(&mut bad_cmap, 6); // subtable @20: format 6 (unsupported)
        tables.push((b"cmap", bad_cmap));
        assert_eq!(
            Font::parse(sfnt(0x0001_0000, &tables)).err(),
            Some(FontError::NoUnicodeCmap)
        );

        // All tables found, but the file ends before head's unitsPerEm.
        let short_head: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"maxp", maxp_table(1)),
            (b"hhea", hhea_table(1)),
            (b"hmtx", hmtx_long(&[(500, 0)])),
            (b"cmap", cmap4_simple(&[])),
            (b"head", vec![0u8; 10]),
        ];
        assert_eq!(
            Font::parse(sfnt(0x0001_0000, &short_head)).err(),
            Some(FontError::Truncated)
        );

        assert_eq!(
            FontError::BadMagic.to_string(),
            "not a TrueType/OpenType font"
        );
        assert_eq!(
            FontError::MissingTable("hhea").to_string(),
            "missing required font table: hhea"
        );
        assert_eq!(FontError::Truncated.to_string(), "font data is truncated");
        assert_eq!(
            FontError::NoUnicodeCmap.to_string(),
            "no usable Unicode cmap (format 4/12)"
        );
    }

    #[test]
    fn parse_accepts_true_and_otto_magics() {
        let tables = base_tables(
            3,
            3,
            2048,
            hmtx_long(&[(500, 1), (510, 2), (520, 3)]),
            cmap4_simple(&[(0x41, 1)]),
        );
        let t = Font::parse(sfnt(0x7472_7565, &tables)).expect("'true' magic parses");
        assert_eq!(t.units_per_em, 2048);
        assert_eq!(t.num_glyphs, 3);
        assert_eq!(t.ascent, 700);
        assert_eq!(t.descent, -200);
        assert_eq!(t.line_gap, 50);
        assert_eq!(t.glyph_index('A'), 1);

        // CFF-flavored fonts parse for metrics but expose no glyf outlines.
        let o = Font::parse(sfnt(0x4F54_544F, &tables)).expect("'OTTO' magic parses");
        assert!(!o.has_glyf_outlines());
        assert_eq!(o.subset(&['A']), None);
        assert_eq!(o.glyph_bbox(1), None);
        assert_eq!(o.glyph_data(1), None);
        assert!(!o.is_composite(1));
        assert!(o.glyph_components(1).is_empty());
    }

    // --- hmtx edges -------------------------------------------------------

    #[test]
    fn left_side_bearing_reads_trailing_run_and_zero_metrics() {
        // 3 glyphs, 1 long metric: gid 0 keeps (advance, lsb); gids 1..2 share
        // the last advance but read their own trailing i16 lsb.
        let mut hmtx = hmtx_long(&[(500, 50)]);
        push_i16(&mut hmtx, -7);
        push_i16(&mut hmtx, 33);
        let font = parse(&base_tables(3, 1, 1000, hmtx, cmap4_simple(&[])));
        assert_eq!(font.left_side_bearing(0), 50);
        assert_eq!(font.left_side_bearing(1), -7);
        assert_eq!(font.left_side_bearing(2), 33);
        assert_eq!(font.advance_width(0), 500);
        assert_eq!(font.advance_width(2), 500);

        // A face declaring zero hMetrics reports zero bearings.
        let font0 = parse(&base_tables(1, 0, 1000, Vec::new(), cmap4_simple(&[])));
        assert_eq!(font0.left_side_bearing(0), 0);
    }

    // --- legacy kern --------------------------------------------------------

    #[test]
    fn legacy_kern_pair_and_char_kerning() {
        let pairs = [(1u16, 2u16, -30i16), (1, 3, 15), (4, 1, 7)];
        let metrics: Vec<(u16, i16)> = (0..8u16).map(|g| (600 + g, 0)).collect();
        let mut tables = base_tables(
            8,
            8,
            1000,
            hmtx_long(&metrics),
            cmap4_simple(&[(0x41, 1), (0x56, 2)]),
        );
        tables.push((b"kern", kern0_table(&pairs)));
        let font = parse(&tables);
        assert_eq!(font.kerning_between_glyphs(1, 2), -30);
        assert_eq!(font.kerning_between_glyphs(1, 3), 15);
        assert_eq!(font.kerning_between_glyphs(4, 1), 7);
        assert_eq!(font.kerning_between_glyphs(2, 1), 0);
        assert_eq!(font.kerning_between_glyphs(1, 4), 0);
        assert_eq!(font.kerning('A', 'V'), -30);
        assert_eq!(font.kerning_1000('A', 'V'), -30); // upem == 1000
        assert_eq!(font.advance_1000('A'), 601);

        // unitsPerEm == 0 short-circuits both per-mille scalers.
        let mut zero = base_tables(
            8,
            8,
            0,
            hmtx_long(&metrics),
            cmap4_simple(&[(0x41, 1), (0x56, 2)]),
        );
        zero.push((b"kern", kern0_table(&pairs)));
        let z = parse(&zero);
        assert_eq!(z.units_per_em, 0);
        assert_eq!(z.advance_1000('A'), 0);
        assert_eq!(z.kerning_1000('A', 'V'), 0);
    }

    #[test]
    fn legacy_kern_skips_short_vertical_minimum_and_format2_subtables() {
        let mut k = Vec::new();
        push16(&mut k, 0); // version
        push16(&mut k, 5); // nTables
        // horizontal format 0 but length < 14: skipped
        push16(&mut k, 0);
        push16(&mut k, 10);
        push16(&mut k, 0x0001);
        k.extend_from_slice(&[0u8; 4]);
        // vertical (horizontal bit clear)
        push16(&mut k, 0);
        push16(&mut k, 14);
        push16(&mut k, 0x0000);
        k.extend_from_slice(&[0u8; 8]);
        // minimum-values bit set
        push16(&mut k, 0);
        push16(&mut k, 14);
        push16(&mut k, 0x0003);
        k.extend_from_slice(&[0u8; 8]);
        // format 2
        push16(&mut k, 0);
        push16(&mut k, 14);
        push16(&mut k, 0x0201);
        k.extend_from_slice(&[0u8; 8]);
        // the real horizontal format-0 subtable
        push16(&mut k, 0);
        push16(&mut k, 20);
        push16(&mut k, 0x0001);
        push16(&mut k, 1); // nPairs
        k.extend_from_slice(&[0u8; 6]);
        push16(&mut k, 3);
        push16(&mut k, 4);
        push_i16(&mut k, -11);

        let mut tables = base_tables(8, 1, 1000, hmtx_long(&[(600, 0)]), cmap4_simple(&[]));
        tables.push((b"kern", k));
        let font = parse(&tables);
        assert_eq!(font.kerning_between_glyphs(3, 4), -11);
        assert_eq!(font.kerning_between_glyphs(3, 5), 0);
    }

    #[test]
    fn legacy_kern_rejects_malformed_table_headers() {
        fn kern_font(kern: Vec<u8>) -> Font {
            let mut tables = base_tables(8, 1, 1000, hmtx_long(&[(600, 0)]), cmap4_simple(&[]));
            tables.push((b"kern", kern));
            parse(&tables)
        }
        // Table version != 0 (e.g. AAT kern 1.0): ignored entirely.
        let mut v1 = kern0_table(&[(1, 2, -30)]);
        v1[0..2].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(kern_font(v1).kerning_between_glyphs(1, 2), 0);

        // A zero-length subtable would never advance: bail.
        let mut zero_len = Vec::new();
        push16(&mut zero_len, 0);
        push16(&mut zero_len, 2);
        push16(&mut zero_len, 0); // subtable version
        push16(&mut zero_len, 0); // length 0
        push16(&mut zero_len, 0x0000); // vertical, so the format match misses
        zero_len.extend_from_slice(&[0u8; 8]);
        assert_eq!(kern_font(zero_len).kerning_between_glyphs(1, 2), 0);

        // nTables claims a second subtable beyond the table end.
        let mut walk_off = Vec::new();
        push16(&mut walk_off, 0);
        push16(&mut walk_off, 2);
        push16(&mut walk_off, 0);
        push16(&mut walk_off, 14);
        push16(&mut walk_off, 0x0000); // vertical: skipped
        walk_off.extend_from_slice(&[0u8; 8]);
        assert_eq!(kern_font(walk_off).kerning_between_glyphs(1, 2), 0);

        // Subtable length overrunning the kern table itself.
        let mut overlong = Vec::new();
        push16(&mut overlong, 0);
        push16(&mut overlong, 1);
        push16(&mut overlong, 0);
        push16(&mut overlong, 200); // sub_end > table_end
        push16(&mut overlong, 0x0001);
        overlong.extend_from_slice(&[0u8; 8]);
        assert_eq!(kern_font(overlong).kerning_between_glyphs(1, 2), 0);

        // nPairs needing more bytes than the subtable declares.
        let mut hungry = Vec::new();
        push16(&mut hungry, 0);
        push16(&mut hungry, 1);
        push16(&mut hungry, 0);
        push16(&mut hungry, 20); // room for exactly one pair
        push16(&mut hungry, 0x0001);
        push16(&mut hungry, 3); // nPairs 3: needs 18 pair bytes, has 6
        hungry.extend_from_slice(&[0u8; 12]);
        assert_eq!(kern_font(hungry).kerning_between_glyphs(1, 2), 0);

        // A single skipped subtable: the walk ends without a match.
        let mut vertical_only = Vec::new();
        push16(&mut vertical_only, 0);
        push16(&mut vertical_only, 1);
        push16(&mut vertical_only, 0);
        push16(&mut vertical_only, 14);
        push16(&mut vertical_only, 0x0000);
        vertical_only.extend_from_slice(&[0u8; 8]);
        assert_eq!(kern_font(vertical_only).kerning_between_glyphs(1, 2), 0);
    }

    #[test]
    fn legacy_kern_truncated_pair_records_kern_to_zero() {
        // kern is the last table; its directory claims 4 pair records but the
        // file ends inside them, so binary-search probes hit EOF and yield 0.
        // chop 10 leaves record 2's left glyph readable (right glyph missing);
        // chop 16 removes even the left glyph of the probed record.
        let pairs = [(1u16, 2u16, -30i16), (1, 3, 15), (4, 1, 7), (5, 5, 9)];
        for chop in [10usize, 16] {
            let mut tables = base_tables(8, 1, 1000, hmtx_long(&[(600, 0)]), cmap4_simple(&[]));
            tables.push((b"kern", kern0_table(&pairs)));
            let mut bytes = sfnt(0x0001_0000, &tables);
            bytes.truncate(bytes.len() - chop);
            let font = Font::parse(bytes).expect("kern pair payload is lazily read");
            assert_eq!(font.kerning_between_glyphs(1, 2), 0, "chop={chop}");
        }
    }

    // --- glyf / loca edges ----------------------------------------------

    #[test]
    fn glyph_range_rejects_inverted_and_overlong_loca_entries() {
        // loca (short) = [4, 2, 6]: glyph 0 is inverted (end < start); glyph 1
        // claims [2, 6) but the glyf table is only 4 bytes long.
        let mut loca = Vec::new();
        push16(&mut loca, 2);
        push16(&mut loca, 1);
        push16(&mut loca, 3);
        let mut tables = base_tables(
            2,
            2,
            1000,
            hmtx_long(&[(500, 0), (500, 0)]),
            cmap4_simple(&[]),
        );
        tables.push((b"loca", loca));
        tables.push((b"glyf", vec![0u8; 4]));
        let font = parse(&tables);
        assert!(font.has_glyf_outlines());
        assert_eq!(font.glyph_data(0), None);
        assert_eq!(font.glyph_data(1), None);
        assert_eq!(font.glyph_bbox(0), None);
        assert!(!font.is_composite(0));
    }

    #[test]
    fn glyph_components_walk_all_transform_variants() {
        let font = zoo_font();
        assert!(font.glyph_components(0).is_empty()); // empty glyph
        assert!(font.glyph_components(1).is_empty()); // simple glyph
        assert_eq!(font.glyph_components(2), vec![5]); // WE_HAVE_A_SCALE
        assert_eq!(font.glyph_components(3), vec![5]); // X_AND_Y_SCALE
        assert_eq!(font.glyph_components(4), vec![5]); // TWO_BY_TWO
        assert_eq!(font.glyph_components(6), vec![2, 3]); // word args + MORE
        assert_eq!(font.glyph_components(7), vec![5]); // MORE, record ends at glyph end
        assert_eq!(font.glyph_components(10), vec![5]); // junk after the last record
        assert!(font.glyph_components(11).is_empty()); // 2x2 payload overruns glyph
        assert!(font.is_composite(2));
        assert!(!font.is_composite(1));
        assert!(!font.is_composite(0));
        assert_eq!(font.glyph_bbox(2), Some([1, 2, 3, 4]));
        assert_eq!(font.glyph_bbox(0), None);
        assert_eq!(font.glyph_data(0), Some(&[][..]));
    }

    #[test]
    fn glyph_components_stop_at_truncated_component_records() {
        // keep = physically present bytes of the 16-byte composite: 1 cuts the
        // contour count, 10 cuts the first record's flags, 12 its glyph index.
        for keep in [1usize, 10, 12] {
            let font = truncated_composite_font(keep);
            assert!(font.glyph_components(0).is_empty(), "keep={keep}");
            assert_eq!(font.glyph_data(0), None, "keep={keep}");
        }
    }

    // --- subsetting edges -------------------------------------------------

    #[test]
    fn subset_rewrites_component_ids_across_transform_variants() {
        let font = zoo_font();
        let (bytes, remap) = font.subset_glyphs(&[2, 3, 4], &[]).expect("subset");
        let remap: Vec<(u16, u16)> = remap.into_iter().collect();
        assert_eq!(remap, vec![(0, 0), (2, 1), (3, 2), (4, 3), (5, 4)]);
        let sub = Font::parse(bytes).expect("subset re-parses");
        assert_eq!(sub.num_glyphs, 5);
        assert_eq!(sub.glyph_components(1), vec![4]);
        assert_eq!(sub.glyph_components(2), vec![4]);
        assert_eq!(sub.glyph_components(3), vec![4]);
        assert_eq!(sub.glyph_bbox(1), Some([1, 2, 3, 4]));
        assert_eq!(sub.advance_width(1), 502);
        assert_eq!(sub.left_side_bearing(1), 2);
        assert_eq!(sub.advance_width(4), 505);
        assert_eq!(sub.left_side_bearing(4), 5);
    }

    #[test]
    fn subset_closure_skips_component_ids_past_num_glyphs() {
        let font = zoo_font();
        let (bytes, remap) = font.subset_glyphs(&[9], &[]).expect("subset");
        assert_eq!(remap.get(&9).copied(), Some(1));
        assert_eq!(remap.len(), 2); // .notdef + composite; gid 900 never joins
        let sub = Font::parse(bytes).expect("subset re-parses");
        assert_eq!(sub.num_glyphs, 2);
        // The out-of-range component was substituted with .notdef.
        assert_eq!(sub.glyph_components(1), vec![0]);
    }

    #[test]
    fn subset_shares_a_component_between_two_composites() {
        let font = zoo_font();
        let (bytes, remap) = font.subset_glyphs(&[2, 4], &[]).expect("subset");
        let remap: Vec<(u16, u16)> = remap.into_iter().collect();
        assert_eq!(remap, vec![(0, 0), (2, 1), (4, 2), (5, 3)]);
        let sub = Font::parse(bytes).expect("subset re-parses");
        assert_eq!(sub.glyph_components(1), vec![3]);
        assert_eq!(sub.glyph_components(2), vec![3]);
    }

    #[test]
    fn subset_aborts_on_composite_whose_last_record_dangles_more() {
        // `glyph_components` tolerates a final record with MORE_COMPONENTS set
        // and nothing after it (gid 7), but `subset_glyph_bytes` keeps walking,
        // fails the next bounds-checked read, and refuses the whole subset
        // rather than emit a corrupt composite.
        let font = zoo_font();
        assert_eq!(font.glyph_components(7), vec![5]);
        let mut new_of = vec![MISSING_GLYPH_REMAP; usize::from(font.num_glyphs)];
        new_of[0] = 0;
        new_of[5] = 1;
        new_of[7] = 2;
        assert_eq!(font.subset_glyph_bytes(7, &new_of), None);
        assert!(font.subset_glyphs(&[7], &[]).is_none());
    }

    #[test]
    fn subset_strips_valid_composite_instructions_and_clears_the_flag() {
        let font = zoo_font();
        let mut new_of = vec![MISSING_GLYPH_REMAP; usize::from(font.num_glyphs)];
        new_of[0] = 0;
        new_of[5] = 1;
        new_of[12] = 2;
        let out = font
            .subset_glyph_bytes(12, &new_of)
            .expect("valid instructions strip");
        assert_eq!(out.len(), 16); // 20 minus the length field and 2 bytes
        assert_eq!(be_u16(&out, 10), Some(0)); // WE_HAVE_INSTRUCTIONS cleared
        assert_eq!(be_u16(&out, 12), Some(1)); // component 5 renumbered
        let (bytes, remap) = font.subset_glyphs(&[12], &[]).expect("subset");
        assert_eq!(remap.get(&12).copied(), Some(2));
        let sub = Font::parse(bytes).expect("subset re-parses");
        assert_eq!(sub.glyph_components(2), vec![1]);
    }

    #[test]
    fn subset_cmap_skips_supplementary_plane_chars() {
        let font = zoo_font();
        let (bytes, _) = font.subset_glyphs(&[2], &['😀']).expect("subset");
        let sub = Font::parse(bytes).expect("subset re-parses");
        assert_eq!(sub.glyph_index('😀'), 0); // never entered the format-4 cmap
    }

    #[test]
    fn subset_rejects_composite_with_overlong_instruction_claim() {
        let font = zoo_font();
        let mut new_of = vec![MISSING_GLYPH_REMAP; usize::from(font.num_glyphs)];
        new_of[0] = 0;
        new_of[5] = 1;
        new_of[8] = 2;
        assert_eq!(font.subset_glyph_bytes(8, &new_of), None);
        assert!(font.subset_glyphs(&[8], &[]).is_none());
    }

    #[test]
    fn strip_simple_glyph_instructions_rejects_overlong_length() {
        let mut glyph = Vec::new();
        push_i16(&mut glyph, 1);
        glyph.extend_from_slice(&[0u8; 8]); // bbox
        push16(&mut glyph, 0); // endPtsOfContours[0]
        push16(&mut glyph, 255); // instructionLength reaching past the data
        assert_eq!(strip_simple_glyph_instructions(&glyph, 1), None);
    }

    // --- cmap lookup paths --------------------------------------------------

    #[test]
    fn cmap4_truncated_segment_arrays_fall_back_to_uncached_lookup() {
        // Six declared segments; idRangeOffset[5] is cut off by the file end,
        // so the parse-time cache fails and lookups walk the raw arrays.
        // idRangeOffsets of segments 1/2 alias later idRangeOffset entries as
        // their glyphIdArray storage.
        let segs = [
            (0x5Au16, 0x41u16, 1u16.wrapping_sub(0x41), 0u16), // 'A'..'Z' -> 1..26
            (0x61, 0x61, 1, 6),                                // 'a' -> array at iro[4]
            (0x62, 0x62, 0, 2),                                // 'b' -> array at iro[3] (0)
            (0x63, 0x63, 0, 0),                                // 'c' -> delta path
            (0x64, 0x64, 0, 7),                                // 'd' -> array past EOF
            (0x00FF, 0x00F0, 0, 0),                            // its iro entry is cut off
        ];
        let tables = base_tables(30, 1, 1000, hmtx_long(&[(500, 0)]), cmap4_table(&segs, &[]));
        let mut bytes = sfnt(0x0001_0000, &tables);
        bytes.truncate(bytes.len() - 2); // drop idRangeOffset[5]
        let font = Font::parse(bytes).expect("cmap payload is lazily read");
        assert!(font.cmap4_cache.is_none());
        assert_eq!(font.glyph_index('A'), 1);
        assert_eq!(font.glyph_index('Z'), 26);
        assert_eq!(font.glyph_index('@'), 0); // below the first segment start
        assert_eq!(font.glyph_index('a'), 8); // glyphIdArray 7 + idDelta 1
        assert_eq!(font.glyph_index('b'), 0); // glyphIdArray slot holds 0
        assert_eq!(font.glyph_index('c'), 99); // idDelta 0 -> the code itself
        assert_eq!(font.glyph_index('d'), 0); // glyphIdArray slot beyond EOF
        assert_eq!(font.glyph_index('õ'), 0); // idRangeOffset entry beyond EOF
        assert_eq!(font.glyph_index('Ā'), 0); // above every segment
        assert_eq!(font.glyph_index('😀'), 0); // beyond the BMP
    }

    #[test]
    fn cmap4_cached_lookup_reads_glyph_id_array() {
        let segs = [(0x42u16, 0x41u16, 3u16, 4u16), (0xFFFF, 0xFFFF, 1, 0)];
        let mut array = Vec::new();
        push16(&mut array, 7);
        push16(&mut array, 0);
        let font = parse(&base_tables(
            20,
            1,
            1000,
            hmtx_long(&[(500, 0)]),
            cmap4_table(&segs, &array),
        ));
        let cache = font.cmap4_cache.as_ref().expect("valid table caches");
        assert!(cache.sorted_by_end);
        assert_eq!(font.glyph_index('A'), 10); // glyphIdArray 7 + idDelta 3
        assert_eq!(font.glyph_index('B'), 0); // glyphIdArray slot holds 0
        assert_eq!(font.glyph_index('C'), 0); // below the final segment's start
    }

    #[test]
    fn cmap4_unsorted_segments_use_first_match_linear_scan() {
        let segs = [
            (0x61u16, 0x61u16, 2u16.wrapping_sub(0x61), 0u16),
            (0x5A, 0x41, 1u16.wrapping_sub(0x41), 0),
            (0xFFFF, 0xFFFF, 1, 0),
        ];
        let font = parse(&base_tables(
            30,
            1,
            1000,
            hmtx_long(&[(500, 0)]),
            cmap4_table(&segs, &[]),
        ));
        let cache = font
            .cmap4_cache
            .as_ref()
            .expect("caches even when unsorted");
        assert!(!cache.sorted_by_end);
        assert_eq!(font.glyph_index('a'), 2);
        // The linear scan takes the FIRST segment whose end covers the code,
        // so the out-of-order table shadows 'A' behind the 'a' segment.
        assert_eq!(font.glyph_index('A'), 0);
        assert_eq!(font.glyph_index('p'), 0);
    }

    #[test]
    fn cmap4_unsorted_lookup_misses_when_no_segment_covers_the_code() {
        // Malformed table: no final 0xFFFF segment AND out-of-order ends, so
        // the cached linear scan can run off the end of the segment list.
        let segs = [
            (0x61u16, 0x61u16, 2u16.wrapping_sub(0x61), 0u16),
            (0x5A, 0x41, 1u16.wrapping_sub(0x41), 0),
        ];
        let font = parse(&base_tables(
            30,
            1,
            1000,
            hmtx_long(&[(500, 0)]),
            cmap4_table(&segs, &[]),
        ));
        assert!(!font.cmap4_cache.as_ref().expect("caches").sorted_by_end);
        assert_eq!(font.glyph_index('a'), 2);
        assert_eq!(font.glyph_index('p'), 0); // beyond every segment end
    }

    #[test]
    fn select_cmap_accepts_format4_under_a_non_bmp_encoding_record() {
        // A (3,10) record pointing at a format-4 subtable ranks lowest but is
        // still selected when nothing better exists.
        let mut cmap = cmap4_simple(&[(0x41, 1)]);
        cmap[6..8].copy_from_slice(&10u16.to_be_bytes()); // encodingID 1 -> 10
        let font = parse(&base_tables(5, 1, 1000, hmtx_long(&[(500, 0)]), cmap));
        assert_eq!(font.cmap_format, 4);
        assert_eq!(font.glyph_index('A'), 1);
    }

    #[test]
    fn cmap12_groups_map_across_planes_and_truncate_gids() {
        let cmap = cmap12_table(&[
            (0x41, 0x5A, 100),
            (0x2000, 0x2000, 0x0001_2345),
            (0x1F600, 0x1F601, 7),
        ]);
        let font = parse(&base_tables(200, 1, 1000, hmtx_long(&[(500, 0)]), cmap));
        assert_eq!(font.cmap_format, 12);
        assert!(font.cmap4_cache.is_none());
        assert_eq!(font.glyph_index('A'), 100);
        assert_eq!(font.glyph_index('Z'), 125);
        assert_eq!(font.glyph_index('\u{2000}'), 0x2345); // gid wraps to u16
        assert_eq!(font.glyph_index('😀'), 7);
        assert_eq!(font.glyph_index('😁'), 8);
        assert_eq!(font.glyph_index('0'), 0); // in no group
    }

    // --- GPOS -----------------------------------------------------------

    #[test]
    fn gpos_extension_lookup_resolves_wrapped_pair_kerning() {
        let kern = gpos_font(1, 2, 0, 1);
        assert_eq!(kern.pair(5, 6), -40);
        assert_eq!(kern.pair(5, 7), 0);
        assert_eq!(kern.pair(6, 6), 0);
    }

    #[test]
    fn gpos_skips_foreign_extensions_bad_formats_and_lookup_indices() {
        // Extension wrapping a non-pair lookup type is ignored.
        assert_eq!(gpos_font(1, 5, 0, 1).pair(5, 6), 0);
        // Extension subtable with an unknown format fails to resolve.
        assert_eq!(gpos_font(2, 2, 0, 1).pair(5, 6), 0);
        // Wrapped pair subtable with an unknown posFormat parses to nothing.
        assert_eq!(gpos_font(1, 2, 0, 3).pair(5, 6), 0);
        // A kern feature pointing past the lookup list is skipped.
        assert_eq!(gpos_font(1, 2, 9, 1).pair(5, 6), 0);
    }

    #[test]
    fn gpos_truncated_structures_yield_empty_kerning() {
        // Each end point cuts the table just before a field the walker needs:
        // 16 the feature offset, 20 the lookup-index count, 22 the index slot,
        // 26 the lookup offset slot, 28 the lookup type, 32 the subtable
        // count, 34 the subtable offset slot.
        for end in [16usize, 20, 22, 26, 28, 32, 34] {
            let mut g = gpos_table(1, 2, 0, 1);
            g.truncate(end);
            assert_eq!(gpos_kerning_of(g).pair(5, 6), 0, "end={end}");
        }
        // featureCount over-claim: trailing phantom records read garbage tags
        // until the walk falls off the table; the real record still applies.
        let mut over = gpos_table(1, 2, 0, 1);
        over[10..12].copy_from_slice(&12u16.to_be_bytes());
        assert_eq!(gpos_kerning_of(over).pair(5, 6), -40);
        // A direct non-pair, non-extension lookup type is ignored.
        let mut direct = gpos_table(1, 2, 0, 1);
        direct[28..30].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(gpos_kerning_of(direct).pair(5, 6), 0);
    }

    #[test]
    fn resolve_extension_requires_format_1() {
        let mut d = Vec::new();
        push16(&mut d, 2);
        push16(&mut d, 2);
        push32(&mut d, 8);
        assert_eq!(resolve_extension(&d, 0), None);
        d[0..2].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(resolve_extension(&d, 0), Some((2, 8)));
        // Unknown pair-subtable formats are rejected outright.
        assert!(parse_pair_subtable(&[0, 3], 0).is_none());
    }

    #[test]
    fn value_record_x_advance_field_extraction() {
        // No X_ADVANCE bit: defined as zero without touching the data.
        assert_eq!(value_record_x_advance(&[], 0, 0), Some(0));
        // X/Y placement precede xAdvance: skip 4 bytes.
        let rec = [0, 0, 0, 0, 0x12, 0x34];
        assert_eq!(value_record_x_advance(&rec, 0, 0x0007), Some(0x1234));
        // Truncated record with the bit set: undecodable.
        assert_eq!(value_record_x_advance(&[0], 0, 0x0004), None);
    }

    #[test]
    fn coverage_and_class_def_malformed_and_boundary_variants() {
        // Coverage format-2 range with end < start contributes nothing.
        let mut cov = Vec::new();
        push16(&mut cov, 2);
        push16(&mut cov, 1);
        push16(&mut cov, 20); // start
        push16(&mut cov, 10); // end < start
        push16(&mut cov, 0);
        assert_eq!(parse_coverage_glyphs(&cov, 0), Some(Vec::new()));
        // Coverage format 3 does not exist.
        assert_eq!(parse_coverage_glyphs(&[0, 3, 0, 0], 0), None);
        // ClassDef format 3 does not exist.
        assert!(parse_class_def(&[0, 3, 0, 0], 0).is_none());

        // Format-1 class array: in-range indices map, everything else class 0.
        let mut cd = Vec::new();
        push16(&mut cd, 1);
        push16(&mut cd, 5); // startGlyphID
        push16(&mut cd, 2); // glyphCount
        push16(&mut cd, 7);
        push16(&mut cd, 9);
        let cd1 = parse_class_def(&cd, 0).expect("format 1 parses");
        assert_eq!(cd1.class(5), 7);
        assert_eq!(cd1.class(6), 9);
        assert_eq!(cd1.class(7), 0); // past the array
        assert_eq!(cd1.class(4), 0); // before startGlyphID

        // Format-2 ranges: covered ranges map, gaps are class 0.
        let mut cd2b = Vec::new();
        push16(&mut cd2b, 2);
        push16(&mut cd2b, 1);
        push16(&mut cd2b, 10);
        push16(&mut cd2b, 20);
        push16(&mut cd2b, 3);
        let cd2 = parse_class_def(&cd2b, 0).expect("format 2 parses");
        assert_eq!(cd2.class(15), 3);
        assert_eq!(cd2.class(9), 0);
        assert_eq!(cd2.class(21), 0);
    }

    #[test]
    fn kern_subtable_format2_guards_class_ranges_and_empty_matrix() {
        let st = KernSubtable::Format2 {
            coverage: vec![5, 9],
            class1: ClassDef::Format1 {
                start: 5,
                classes: vec![1, 0, 0, 0, 9],
            },
            class2: ClassDef::Format1 {
                start: 6,
                classes: vec![1, 7],
            },
            class1_count: 2,
            class2_count: 2,
            matrix: vec![0, 0, 0, -55],
        };
        assert_eq!(st.lookup(4, 6), None); // left glyph not covered
        assert_eq!(st.lookup(5, 6), Some(-55)); // classes (1, 1) -> cell 3
        assert_eq!(st.lookup(9, 6), Some(0)); // class1 out of declared range
        assert_eq!(st.lookup(5, 7), Some(0)); // class2 out of declared range

        let empty = KernSubtable::Format2 {
            coverage: vec![5],
            class1: ClassDef::Format2 { ranges: Vec::new() },
            class2: ClassDef::Format2 { ranges: Vec::new() },
            class1_count: 1,
            class2_count: 1,
            matrix: Vec::new(),
        };
        assert_eq!(empty.lookup(5, 6), Some(0));

        // A covered-but-zero first subtable still wins over later subtables.
        let kerning = Kerning {
            subtables: vec![empty, st],
        };
        assert_eq!(kerning.pair(5, 6), 0);
        assert_eq!(kerning.pair(4, 6), 0);
    }

    #[test]
    fn pair_format1_skips_malformed_sets_and_truncated_records() {
        // pairSetCount 2 but the coverage names one glyph; the second set is
        // skipped while the first still yields (5, 6) -> -40.
        let mut d = Vec::new();
        push16(&mut d, 1); // posFormat
        push16(&mut d, 20); // coverage @20
        push16(&mut d, 0x0004); // valueFormat1
        push16(&mut d, 0); // valueFormat2
        push16(&mut d, 2); // pairSetCount
        push16(&mut d, 14); // pairSet[0] @14
        push16(&mut d, 14); // pairSet[1] (no coverage glyph -> skipped)
        push16(&mut d, 1); // pairValueCount
        push16(&mut d, 6); // secondGlyph
        push_i16(&mut d, -40);
        push16(&mut d, 1); // coverage format
        push16(&mut d, 1);
        push16(&mut d, 5);
        let st = parse_pair_subtable(&d, 0).expect("format 1 parses");
        assert_eq!(st.lookup(5, 6), Some(-40));
        assert_eq!(st.lookup(5, 7), None);

        // A pair-set offset pointing past the data contributes nothing.
        let mut d2 = Vec::new();
        push16(&mut d2, 1);
        push16(&mut d2, 12); // coverage @12
        push16(&mut d2, 0x0004);
        push16(&mut d2, 0);
        push16(&mut d2, 1);
        push16(&mut d2, 0x4000); // pairSet[0]: far past the end
        push16(&mut d2, 1);
        push16(&mut d2, 1);
        push16(&mut d2, 5);
        let st2 = parse_pair_subtable(&d2, 0).expect("parses to an empty set");
        assert_eq!(st2.lookup(5, 6), None);

        // pairValueCount claims 2 records but the data ends after the first.
        let mut d3 = Vec::new();
        push16(&mut d3, 1);
        push16(&mut d3, 12); // coverage @12
        push16(&mut d3, 0x0004);
        push16(&mut d3, 0);
        push16(&mut d3, 1);
        push16(&mut d3, 18); // pairSet @18
        push16(&mut d3, 1); // coverage format
        push16(&mut d3, 1);
        push16(&mut d3, 5);
        push16(&mut d3, 2); // pairValueCount (overlong)
        push16(&mut d3, 6);
        push_i16(&mut d3, -40);
        let st3 = parse_pair_subtable(&d3, 0).expect("parses the readable record");
        assert_eq!(st3.lookup(5, 6), Some(-40));
        assert_eq!(st3.lookup(5, 0), None);
    }

    #[test]
    fn pair_format1_work_cap_stops_aliased_pair_set_expansion() {
        // Two pair sets alias one huge set. With 65 535 records the ceiling
        // trips between the sets; with 65 534 it trips inside the second one.
        for count in [65_535u16, 65_534] {
            let mut d = Vec::new();
            push16(&mut d, 1); // posFormat
            push16(&mut d, 14); // coverage @14
            push16(&mut d, 0); // valueFormat1: empty records
            push16(&mut d, 0); // valueFormat2
            push16(&mut d, 2); // pairSetCount
            push16(&mut d, 22); // pairSet[0] @22
            push16(&mut d, 22); // pairSet[1]: aliases the same set
            push16(&mut d, 1); // coverage format
            push16(&mut d, 2);
            push16(&mut d, 5);
            push16(&mut d, 6);
            push16(&mut d, count); // pairValueCount
            d.resize(d.len() + usize::from(count) * 2, 0); // secondGlyph = 0 each
            let st = parse_pair_subtable(&d, 0).expect("parses under the work cap");
            // The first set registers (5, 0); the ceiling stops the aliased
            // second set before it can register (6, 0).
            assert_eq!(st.lookup(5, 0), Some(0), "count={count}");
            assert_eq!(st.lookup(6, 0), None, "count={count}");
        }
    }

    #[test]
    fn pair_format2_empty_value_formats_and_oversized_matrix() {
        // Both value formats empty: the matrix is elided and every covered
        // pair resolves to zero.
        let mut d = Vec::new();
        push16(&mut d, 2); // posFormat
        push16(&mut d, 16); // coverage @16
        push16(&mut d, 0); // valueFormat1
        push16(&mut d, 0); // valueFormat2
        push16(&mut d, 22); // classDef1 @22
        push16(&mut d, 22); // classDef2 @22 (shared)
        push16(&mut d, 1); // class1Count
        push16(&mut d, 1); // class2Count
        push16(&mut d, 1); // coverage format
        push16(&mut d, 1);
        push16(&mut d, 3);
        push16(&mut d, 1); // classdef format 1, empty array
        push16(&mut d, 0);
        push16(&mut d, 0);
        let st = parse_pair_subtable(&d, 0).expect("empty-value format 2 parses");
        assert!(matches!(
            &st,
            KernSubtable::Format2 { matrix, .. } if matrix.is_empty()
        ));
        assert_eq!(st.lookup(3, 42), Some(0));
        assert_eq!(st.lookup(4, 42), None);

        // A declared matrix larger than the whole table is rejected.
        let mut big = Vec::new();
        push16(&mut big, 2);
        push16(&mut big, 16);
        push16(&mut big, 0x0004);
        push16(&mut big, 0);
        push16(&mut big, 22);
        push16(&mut big, 22);
        push16(&mut big, 0xFFFF);
        push16(&mut big, 0xFFFF);
        assert!(parse_pair_subtable(&big, 0).is_none());
    }

    // --- GSUB -----------------------------------------------------------

    #[test]
    fn gsub_extension_lookup_parses_greedy_ligatures() {
        let ligs = gsub_font(1, 4, 0);
        assert!(!ligs.is_empty());
        assert!(Ligatures::default().is_empty());
        assert_eq!(ligs.substitute(&[10, 11, 12]), vec![99]);
        assert_eq!(ligs.substitute(&[10, 11, 7]), vec![77, 7]);
        assert_eq!(ligs.substitute(&[10, 7]), vec![10, 7]);
        assert_eq!(
            ligs.substitute_with_spans(&[10, 11, 12, 10, 11]),
            vec![(99, 3), (77, 2)]
        );
    }

    #[test]
    fn gsub_skips_foreign_extensions_and_bad_lookup_indices() {
        // Extension wrapping a non-ligature lookup type is ignored.
        assert!(gsub_font(1, 2, 0).is_empty());
        // Extension subtable with an unknown format fails to resolve.
        assert!(gsub_font(2, 4, 0).is_empty());
        // A liga feature pointing past the lookup list is skipped.
        assert!(gsub_font(1, 4, 9).is_empty());
    }

    #[test]
    fn gsub_truncated_structures_yield_no_ligatures() {
        // Same cut points as the GPOS walker: the two table layouts share
        // their header/feature/lookup shape.
        for end in [16usize, 20, 22, 26, 28, 32, 34] {
            let mut g = gsub_table(1, 4, 0);
            g.truncate(end);
            assert!(gsub_ligatures_of(g).is_empty(), "end={end}");
        }
        // featureCount over-claim: phantom records break the walk after the
        // real record already registered its ligatures.
        let mut over = gsub_table(1, 4, 0);
        over[10..12].copy_from_slice(&12u16.to_be_bytes());
        assert_eq!(gsub_ligatures_of(over).substitute(&[10, 11]), vec![77]);
        // A direct non-ligature, non-extension lookup type is ignored.
        let mut direct = gsub_table(1, 4, 0);
        direct[28..30].copy_from_slice(&1u16.to_be_bytes());
        assert!(gsub_ligatures_of(direct).is_empty());
    }

    #[test]
    fn ligature_subst_skips_malformed_entries() {
        let mut rules: std::collections::BTreeMap<u16, Vec<LigRule>> =
            std::collections::BTreeMap::new();

        // Unknown subtable format: ignored outright.
        parse_ligature_subst(&[0, 2, 0, 0], 0, &mut rules);
        assert!(rules.is_empty());

        // Header reads running off the end return without any rules.
        parse_ligature_subst(&[], 0, &mut rules); // no format
        parse_ligature_subst(&[0, 1], 0, &mut rules); // no coverage offset
        parse_ligature_subst(&[0, 1, 0, 8], 0, &mut rules); // no ligSetCount
        parse_ligature_subst(&[0, 1, 0, 6, 0, 1, 0, 3], 0, &mut rules); // coverage fmt 3
        assert!(rules.is_empty());

        // LigatureSet offset far past the data: no rules.
        let mut d = Vec::new();
        push16(&mut d, 1); // substFormat
        push16(&mut d, 8); // coverage @8
        push16(&mut d, 1); // ligSetCount
        push16(&mut d, 0x4000); // ligatureSet: far past the end
        push16(&mut d, 1); // coverage format
        push16(&mut d, 1);
        push16(&mut d, 10);
        parse_ligature_subst(&d, 0, &mut rules);
        assert!(rules.is_empty());

        // ligSetCount 2 with a single-glyph coverage: the second set has no
        // coverage glyph, the first still parses (10, 11) -> 77.
        let mut d2 = Vec::new();
        push16(&mut d2, 1); // substFormat
        push16(&mut d2, 20); // coverage @20
        push16(&mut d2, 2); // ligSetCount
        push16(&mut d2, 10); // set[0] @10
        push16(&mut d2, 10); // set[1] (never reached)
        push16(&mut d2, 1); // ligatureCount
        push16(&mut d2, 4); // ligature @14
        push16(&mut d2, 77); // ligatureGlyph
        push16(&mut d2, 2); // componentCount
        push16(&mut d2, 11); // component[1]
        push16(&mut d2, 1); // coverage format
        push16(&mut d2, 1);
        push16(&mut d2, 10);
        parse_ligature_subst(&d2, 0, &mut rules);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[&10].len(), 1);
        assert_eq!(rules[&10][0].components, vec![11]);
        assert_eq!(rules[&10][0].ligature, 77);

        // Zero component count, comp-count/glyph reads past the end, and a
        // truncated component array: each entry drops without a rule.
        rules.clear();
        let mut d3 = Vec::new();
        push16(&mut d3, 1); // substFormat
        push16(&mut d3, 8); // coverage @8
        push16(&mut d3, 1); // ligSetCount
        push16(&mut d3, 14); // ligatureSet @14
        push16(&mut d3, 1); // coverage format
        push16(&mut d3, 1);
        push16(&mut d3, 10);
        push16(&mut d3, 4); // ligatureCount
        push16(&mut d3, 10); // @24: zero componentCount
        push16(&mut d3, 20); // @34: componentCount past the end
        push16(&mut d3, 0x4000); // unreadable ligature glyph
        push16(&mut d3, 14); // @28: component array past the end
        push16(&mut d3, 33); // ligature @24
        push16(&mut d3, 0); // componentCount 0
        push16(&mut d3, 88); // ligature @28
        push16(&mut d3, 5); // componentCount 5, components cut off
        push16(&mut d3, 11);
        push16(&mut d3, 12);
        assert_eq!(d3.len(), 36);
        parse_ligature_subst(&d3, 0, &mut rules);
        assert!(rules.is_empty());
    }

    #[test]
    fn ligature_subst_work_cap_stops_aliased_sets() {
        // Two ligature sets alias one set whose declared count is huge and
        // whose offset array is entirely missing. With 65 535 the ceiling
        // trips between the sets; with 65 534 inside the second one.
        for lig_count in [65_535u16, 65_534] {
            let mut d = Vec::new();
            push16(&mut d, 1); // substFormat
            push16(&mut d, 10); // coverage @10
            push16(&mut d, 2); // ligSetCount
            push16(&mut d, 18); // set[0] @18
            push16(&mut d, 18); // set[1]: aliases set[0]
            push16(&mut d, 1); // coverage format
            push16(&mut d, 2);
            push16(&mut d, 10);
            push16(&mut d, 11);
            push16(&mut d, lig_count); // every ligature offset is unreadable
            let mut rules: std::collections::BTreeMap<u16, Vec<LigRule>> =
                std::collections::BTreeMap::new();
            parse_ligature_subst(&d, 0, &mut rules);
            assert!(rules.is_empty(), "lig_count={lig_count}");
        }
    }
}
